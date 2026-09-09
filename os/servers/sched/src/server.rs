//! The composition layer: one table, one ledger, one loop.
//!
//! Mirrors `main()` + the caller-side halves of the four arms
//! (`minix3/minix/servers/sched/main.c:22-96` and the write-back steps of
//! `schedule.c`). 01 owns the startup order, 02 owns the message rules, the
//! arms (06~08) own the verdicts; this module owns what is left — the state
//! those rules talk about, and the order the verbs run in. [ARCH S-11]
//! (plan.md ARCH table, doc 02 D8): C keeps four file-scope globals
//! (`schedproc[NR_PROCS]`, `cpu_proc[]`, `machine`, `balance_timeout` —
//! `schedproc.h:36`, `schedule.c:16,46`, `main.c:17`); the rewrite folds
//! them into one owner so the single-threaded event loop never splits a
//! borrow to touch two of them together (a birth writes the table and the
//! ledger in one breath, `schedule.c:223-231`).
//!
//! Single-threaded event loop: one `&mut SchedServer` walks each turn, no
//! locks, no interior mutability.
//!
//! The eight execution-side semantics this layer must hold (todo.md §1.2,
//! each pinned by a test named for it): reply failure only logs; kernel
//! NO_QUANTUM is never answered; a forged one answers EPERM; notifications
//! never answer; NO_QUANTUM does not roll back while NICE does; a START
//! whose fan-out failed leaves the slot occupied; the balance walk ignores
//! wire answers; a successful START names SCHED in the reply.

use crate::balancer::{rebalance_one, Balancer};
use crate::cpu::{add_load, mark_dead, pick, release_load, CpuId, CpuLoad, MachineTopology};
use crate::dispatch::{no_sys_verdict, noquantum_trust, settle, DispatchVerdict, SchedMsg};
use crate::kernel_api::schedule::{aggregate, ChangeMask, SlotValues};
use crate::kernel_api::schedctl::SchedctlCall;
use crate::kernel_api::transport::{is_notify, IpcTransport, KernelApi};
use crate::priority::is_system_proc;
use crate::schedproc::{Priority, SchedProc, SlotState};
use crate::scheduling::{nice, noquantum, start, stop};
use crate::table::{OccupiedSlot, SlotVerdict, check_occupied, check_vacant};
use crate::valid::{accept, sender_from};
use minix_types::{
    Endpoint, Message, NR_PROCS, EPERM, SCHEDULING_INHERIT, SCHEDULING_SET_NICE, SCHEDULING_START,
    SCHEDULING_STOP,
};

/// Ledger capacity. C: `CONFIG_MAX_CPUS` (`schedproc.h:15` falls back to 1
/// without CONFIG_SMP; the kernel's own bound is 32,
/// `os/kernel/src/smp.rs:61`). Slots past the topology's count are never
/// touched — [`crate::cpu::pick`] only reads what exists.
pub const MAX_CPUS: usize = 32;

/// Consecutive receive failures before the loop declares the transport
/// broken. C panics on the first failure (`main.c:39-40`); the bound is the
/// one deliberate deviation (VM's V10-P0-2 precedent,
/// `os/servers/vm/src/vm_server.rs`): a blocked receive has no transient
/// "no message" failure, so one `Err` is already a broken transport — but
/// counting to a bound instead of panicking immediately keeps pre-E1 runs
/// diagnosable (every turn fails while the trap layer is a stub) and keeps
/// the failure path unit-drivable.
const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 64;

/// The errno convention the arms speak (`i32`, `0` = OK), read off a
/// kernel-call answer. The seam returns `Result`, the C-shaped arms return
/// the code — the translation lives once, here.
fn code(result: Result<(), i32>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(errno) => errno,
    }
}

/// What one loop turn did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A message was received and processed (answered, silenced, or
    /// dispatched).
    Handled,
    /// The receive failed; the message was lost. [`SchedServer::run`]
    /// counts these.
    ReceiveFailed,
}

/// SCHED's whole world: the table, the ledger, the machine, the bell.
pub struct SchedServer {
    /// One record per process (C: `schedproc[NR_PROCS]`,
    /// `schedproc.h:36`; `NR_PROCS` is 256, `minix-types` com.rs:38).
    procs: [SchedProc; NR_PROCS],
    /// Per-CPU load ledger (C: `cpu_proc[]`, `schedule.c:46`). Booked at a
    /// birth, released at a stop, branded dead by the retry ring — one
    /// selection pairs with one release (10 D3), not C's book-on-every-pick.
    loads: [CpuLoad; MAX_CPUS],
    /// The machine (C: `machine`, `main.c:17`) — two numbers every pick
    /// reads.
    topo: MachineTopology,
    /// The balance wait (C: `balance_timeout`, `schedule.c:16`); `None`
    /// until [`SchedServer::init_scheduling`] arms it.
    balancer: Option<Balancer>,
}

impl SchedServer {
    /// Build the server for a known machine.
    ///
    /// The machine walks in first (`main.c:130` reads it before anything
    /// else runs); the bell arms separately ([`SchedServer::init_scheduling`]),
    /// exactly C's order — `sef_cb_init_fresh` reads the machine at 130 and
    /// calls `init_scheduling` at 133.
    pub fn new(topo: MachineTopology) -> Self {
        Self {
            procs: core::array::from_fn(|_| SchedProc {
                endpoint: Endpoint(0),
                parent: Endpoint(0),
                state: SlotState::Free,
                max_priority: Priority::new(0).expect("0 < 16"),
                priority: Priority::new(0).expect("0 < 16"),
                time_slice_ms: 0,
                cpu: CpuId(0),
            }),
            // Zero-initialized like C's static array (schedule.c:46): every
            // CPU alive with no load. `None` is the mark of a branded-dead
            // seat (10 D2), not of an unprobed one.
            loads: [Some(0); MAX_CPUS],
            topo,
            balancer: None,
        }
    }

    /// Arm the balance bell (C: `init_scheduling`, `schedule.c:334-342`).
    ///
    /// Five seconds times the clock rate, then one `sys_setalarm`. The
    /// `Err` path lands in the binary's panic — C panics inline (340-341),
    /// the rewrite keeps failures at the binary layer where every other
    /// startup failure already dies (01 D2's convention).
    pub fn init_scheduling(&mut self, kernel: &mut impl KernelApi) -> Result<(), i32> {
        let hz = kernel.get_hz()?;
        let balancer = Balancer::init(hz);
        kernel.setalarm(balancer.timeout_ticks())?;
        self.balancer = Some(balancer);
        Ok(())
    }

    /// The forever loop (C: `main.c:35-96`), one turn at a time inside
    /// [`SchedServer::run_once`].
    pub fn run(&mut self, ipc: &impl IpcTransport, kernel: &mut impl KernelApi) -> ! {
        let mut consecutive_failures: u32 = 0;
        loop {
            match self.run_once(ipc, kernel) {
                Step::Handled => consecutive_failures = 0,
                Step::ReceiveFailed => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        panic!(
                            "SCHED: IPC transport broken after {consecutive_failures} \
                             consecutive receive failures"
                        );
                    }
                }
            }
        }
    }

    /// One turn: receive, classify, dispatch, settle (C: `main.c:35-96`).
    ///
    /// The turn is the unit the eight execution-side semantics are tested
    /// against; [`SchedServer::run`] only adds the forever and the failure
    /// bound.
    pub fn run_once(&mut self, ipc: &impl IpcTransport, kernel: &mut impl KernelApi) -> Step {
        // ── receive (C 38-42) ──
        let (mut message, status) = match ipc.receive() {
            Ok(arrival) => arrival,
            // A failed receive loses one turn; `run` counts. (C panics at
            // 39-40; the bound is the documented deviation above.)
            Err(_) => return Step::ReceiveFailed,
        };
        let sender = message.m_source; // C 41

        // ── notifications first (C 44-55) ──
        if is_notify(status) {
            if sender == Endpoint::CLOCK {
                // C 47-49: the balance round. A failed re-arm is fatal —
                // C panics right there (367-368); the loop has no future
                // without its bell.
                self.balance_queues(kernel)
                    .expect("sys_setalarm failed (schedule.c:367-368)");
            } // C 50-52: any other notification passes in silence.
            return Step::Handled; // C 54: notifications are never answered.
        }

        // ── dispatch (C 57-87) ──
        let message_type = message.m_type;
        let verdict = match SchedMsg::from_raw(message_type) {
            Some(SchedMsg::NoQuantum) => {
                if noquantum_trust(status.is_from_kernel()) {
                    // C 70-77: the seal holds — run the demotion and never
                    // answer, success or failure alike. (C 73-74 prints a
                    // warning on failure; the crate has no logging
                    // facility, and the no-reply behavior is the
                    // observable part.)
                    let _ = self.do_noquantum(&message, kernel);
                    return Step::Handled;
                }
                // C 78-83: forged — EPERM rides the normal reply rule.
                settle(EPERM)
            }
            Some(SchedMsg::Start) => settle(self.do_start(sender, false, &mut message, kernel)),
            Some(SchedMsg::Inherit) => settle(self.do_start(sender, true, &mut message, kernel)),
            Some(SchedMsg::Stop) => settle(self.do_stop(sender, &message)),
            Some(SchedMsg::SetNice) => settle(self.do_nice(sender, &message, kernel)),
            None => settle(no_sys_verdict()), // C 85-86 → utility.c:18-23
        };

        // ── settle (C 89-96) ──
        if let DispatchVerdict::Reply(code) = verdict {
            let mut reply = message;
            reply.m_type = code;
            // C 101-106: a failed reply is logged and dropped — the loop
            // moves on. The print has no Rust home (no logging facility);
            // continue-on-failure is the observable half.
            let _ = ipc.send(sender, &reply);
        }
        Step::Handled
    }

    /// The CLOCK arm: one step up for everyone below their ceiling
    /// (C: `balance_queues`, `schedule.c:353-369`).
    ///
    /// The walk is fire-and-forget (C 358-364: the answers are not checked
    /// — a failed promote retries in five seconds); the re-arm is not
    /// (`Err` back to the loop, which panics).
    fn balance_queues(&mut self, kernel: &mut impl KernelApi) -> Result<(), i32> {
        for index in 0..NR_PROCS {
            if !self.procs[index].is_used() {
                continue; // C 359: only live slots
            }
            if let Some(up) = rebalance_one(self.procs[index].max_priority, self.procs[index].priority)
            {
                self.procs[index].priority = up; // C 361
                let _ = self.fanout_local(index, kernel); // C 362: unchecked
            }
        }
        // C 367-368: ring the bell again, or the balance dies with this round.
        let balancer = self
            .balancer
            .expect("balance round before init_scheduling — startup order violated");
        kernel.setalarm(balancer.timeout_ticks())
    }

    /// An occupied-slot probe (`sched_isokendpt`, `utility.c:29-41`):
    /// the door, and — on passage — where the slot sits and what it
    /// holds. The `Err` half carries the verdict so an arm can answer
    /// the C errno; the `Ok` half makes the facts unforgeable (a
    /// refused target has no row to read, not even dummies).
    fn probe_occupied(&self, endpoint: Endpoint) -> Result<OccupiedSlot, SlotVerdict> {
        let slot = endpoint.slot();
        let in_range = slot >= 0 && (slot as usize) < NR_PROCS;
        let (name_match, in_use) = if in_range {
            let row = &self.procs[slot as usize];
            (row.endpoint == endpoint, row.is_used())
        } else {
            (false, false)
        };
        match check_occupied(slot, NR_PROCS, name_match, in_use) {
            SlotVerdict::Occupied => Ok(OccupiedSlot {
                index: slot as usize,
                row: self.procs[slot as usize],
            }),
            verdict => Err(verdict),
        }
    }

    /// A vacant-slot probe (`sched_isemtyendpt`, `utility.c:46-56`):
    /// the door, and — on passage — the slot index a birth will write.
    fn probe_vacant(&self, endpoint: Endpoint) -> Result<usize, SlotVerdict> {
        let slot = endpoint.slot();
        let in_range = slot >= 0 && (slot as usize) < NR_PROCS;
        let in_use = if in_range {
            self.procs[slot as usize].is_used()
        } else {
            false
        };
        match check_vacant(slot, NR_PROCS, in_use) {
            SlotVerdict::Occupied => Ok(slot as usize),
            verdict => Err(verdict),
        }
    }

    /// The takeover arm's caller half (C: `do_start_scheduling`,
    /// `schedule.c:140-249`).
    ///
    /// The arm (06) plans the birth; this writes the slot, registers the
    /// takeover, picks a CPU, and runs the fan-out ring — in C's order, so
    /// a refusal lands before the slot is touched and a fan-out failure
    /// lands after it (the residue is C's, `schedule.c:223` before
    /// `227-237`).
    fn do_start(
        &mut self,
        sender: Endpoint,
        inherit: bool,
        message: &mut Message,
        kernel: &mut impl KernelApi,
    ) -> i32 {
        let expected = if inherit {
            SCHEDULING_INHERIT
        } else {
            SCHEDULING_START
        };
        // C 154 (`ipc.h:1431-1434`): four fields ride the letter.
        let (child, parent, maxprio, quantum) =
            message.payload_ref::<minix_types::MessLsysSchedSchedulingStart, _, _>(expected, |m| unsafe {
                let f = &m.m_u.m_lsys_sched_scheduling_start;
                (Endpoint(f.endpoint), Endpoint(f.parent), f.maxprio, f.quantum)
            });
        let request = start::Request {
            kind: match start::Kind::from_msg(if inherit {
                SchedMsg::Inherit
            } else {
                SchedMsg::Start
            }) {
                Some(kind) => kind,
                // Unreachable through the loop (02 already classified);
                // 06 D1's second gate keeps the arm total regardless.
                None => return minix_types::EINVAL,
            },
            child,
            parent,
            maxprio,
            quantum,
        };
        let sender_ok = accept(sender_from(sender)); // C 150-152
        // The vacant door first (`154-157`): the birth slot rides out of
        // the probe only on passage.
        let child_probe = self.probe_vacant(child);
        let child_verdict = SlotVerdict::from_probe(&child_probe);
        let seed = if inherit {
            // C 199-211: the parent must itself be scheduled; the parent
            // probe hands over the inherited fields only when its door
            // passed — a self-parented INHERIT reads `Err(Dead)` here
            // because the child's slot is still unflagged (C fills
            // 160-163 but flags 223 come after the switch — the ordering
            // is the proof, 06 D4).
            let parent = self
                .probe_occupied(parent)
                .map(|slot| start::ParentState {
                    priority: slot.row.priority,
                    time_slice_ms: slot.row.time_slice_ms,
                });
            start::plan_inherit(sender_ok, child_verdict, &parent, &request)
        } else {
            start::plan_start(sender_ok, child_verdict, &request)
        };
        let seed = match seed {
            Ok(seed) => seed,
            Err(code) => return code,
        };

        // C 218-222: register the takeover. A refusal leaves the slot
        // untouched — the birth never happened.
        if let Err(rv) = kernel.schedctl(&SchedctlCall::register(child)) {
            return rv;
        }

        // C 223 (+160-163): the slot becomes SCHED's, occupied from here on.
        // The write address is the vacant door's own passage — plan and
        // schedctl above refused everything else, so the probe cannot be
        // `Err` here; if it ever were, the door's verdict is the answer.
        let index = match child_probe {
            Ok(index) => index,
            Err(verdict) => return verdict.errno(),
        };
        self.procs[index] = SchedProc {
            endpoint: seed.endpoint,
            parent: seed.parent,
            state: SlotState::InUse,
            max_priority: seed.max_priority,
            priority: seed.priority,
            time_slice_ms: seed.time_slice_ms,
            cpu: CpuId(0),
        };

        // C 226-231: pick, fan out, and let dead CPUs circle the ring. The
        // ledger books each seat this ring tries (one booking per
        // selection, 10 D3); a branded seat drops its booking with it.
        let is_system = is_system_proc(seed.parent);
        let mut chosen = pick(is_system, &self.topo, &self.loads);
        let rv = loop {
            if self.topo.processors_count > 1 {
                add_load(&mut self.loads, chosen);
            }
            self.procs[index].cpu = chosen;
            let fanout = aggregate(ChangeMask::ALL, &self.row_values(index));
            match start::classify_fanout(code(kernel.schedule(&fanout))) {
                start::Fanout::CpuDead => {
                    // C 229-230: brand it, never try it again.
                    mark_dead(&mut self.loads, chosen);
                    chosen = pick(is_system, &self.topo, &self.loads);
                    // (A one-CPU world short-circuits back to the same seat
                    // — exactly C's non-SMP ring, where EBADCPU cannot
                    // arise by contract.)
                }
                start::Fanout::Done(rv) => break rv,
            }
        };
        if rv != 0 {
            // C 233-237: the kernel said no. The slot stays occupied —
            // C's residue (`223` ran, `236` returns without cleanup), and
            // a STOP will clear it (semantic 6).
            return rv;
        }

        // C 246: the reply names the new scheduler (PM stores it as the
        // process's scheduler, `ipc/message.rs:1187`). The union write is
        // safe — a Copy field, overwrite-only, no read.
        message.payload_mut::<minix_types::MessSchedLsysSchedulingStart, _, ()>(expected, |m| {
            m.m_u.m_sched_lsys_scheduling_start.scheduler = Endpoint::SCHED.0;
        });
        0
    }

    /// The release arm's caller half (C: `do_stop_scheduling`,
    /// `schedule.c:112-135`).
    fn do_stop(&mut self, sender: Endpoint, message: &Message) -> i32 {
        let (child,) = message
            .payload_ref::<minix_types::MessLsysSchedSchedulingStop, _, _>(SCHEDULING_STOP, |m| unsafe {
                (Endpoint(m.m_u.m_lsys_sched_scheduling_stop.endpoint),)
            });
        let request = stop::Request { child };
        let sender_ok = accept(sender_from(sender)); // C 118-119
        let target = self.probe_occupied(child);
        let release = match stop::plan_stop(sender_ok, &target, &request) {
            Ok(release) => release,
            Err(code) => return code,
        };
        // The write address rides out of the probe — the door passed
        // above (plan_stop refused every refusal), so the verdict left
        // would be the door's own answer.
        let slot = match target {
            Ok(slot) => slot,
            Err(verdict) => return verdict.errno(),
        };
        // C 129-131: the ledger sheds one unit — under CONFIG_SMP only, so
        // the gate reads the topology (07's contract: the Release still
        // names the CPU, the gate has one reader).
        if self.topo.processors_count > 1 {
            release_load(&mut self.loads, release.cpu);
        }
        self.procs[slot.index].state = SlotState::Free; // C 132
        0
    }

    /// The regrade arm's caller half (C: `do_nice`, `schedule.c:254-292`).
    fn do_nice(
        &mut self,
        sender: Endpoint,
        message: &Message,
        kernel: &mut impl KernelApi,
    ) -> i32 {
        let (child, maxprio) =
            message.payload_ref::<minix_types::MessPmSchedSchedulingSetNice, _, _>(SCHEDULING_SET_NICE, |m| unsafe {
                (
                    Endpoint(m.m_u.m_pm_sched_scheduling_set_nice.endpoint),
                    m.m_u.m_pm_sched_scheduling_set_nice.maxprio,
                )
            });
        let request = nice::Request {
            child,
            // The wire carries u32 (ipc.h:1824); the arm reads i32 and
            // refuses the out-of-band itself (nice.rs:22-25) — a huge
            // unsigned wraps negative into the same EINVAL.
            maxprio: maxprio as i32,
        };
        let sender_ok = accept(sender_from(sender)); // C 262-263
        let target = self.probe_occupied(child);
        // C 278-279: the snapshot is the rollback — `Current` is `Copy`,
        // keeping is snapshotting (08's contract). C takes the old values
        // after the ceiling check too; the probed slot rides out of
        // `admit` together with the ceiling, so the snapshot reads only
        // what the doors blessed.
        let (ceiling, slot) = match nice::admit(sender_ok, &target, request.maxprio) {
            Ok(pair) => pair,
            Err(code) => return code,
        };
        let current = nice::Current {
            priority: slot.row.priority,
            max_priority: slot.row.max_priority,
        };
        let after = nice::regrade(ceiling); // C 282: both numbers, one move
        self.procs[slot.index].priority = after.priority;
        self.procs[slot.index].max_priority = after.max_priority;
        let rv = self.fanout_local(slot.index, kernel); // C 284
        if rv != 0 {
            // C 285-288: the kernel refused — write the snapshot back.
            // (NO_QUANTUM does NOT roll back; the asymmetry is C's real
            // semantics, todo.md §1.2 #5.)
            self.procs[slot.index].priority = current.priority;
            self.procs[slot.index].max_priority = current.max_priority;
        }
        rv
    }

    /// The demotion arm's caller half (C: `do_noquantum`,
    /// `schedule.c:87-109`).
    fn do_noquantum(&mut self, message: &Message, kernel: &mut impl KernelApi) -> i32 {
        // C 92: the kernel names the spender by source — the body carries
        // no endpoint at all, and the sender whitelist does not apply (the
        // seal was checked one level up, main.c:70-71).
        let source = message.m_source;
        let target = self.probe_occupied(source);
        let slot = match noquantum::admit(&target) {
            Ok(slot) => slot,
            Err(code) => return code, // C 92-96
        };
        // C 99-101: one step down unless already at the floor. `demote`'s
        // `None` is the constants-drifted defensive arm — falling back to
        // "no change" keeps the fan-out below well-defined.
        let new_priority = noquantum::demote(slot.row.priority).unwrap_or(slot.row.priority);
        self.procs[slot.index].priority = new_priority;
        // C 103-105: fan out and hand the answer straight back — a failure
        // does NOT roll the demotion back (the spent slot stays sunk; NICE
        // is the one that rolls back).
        self.fanout_local(slot.index, kernel)
    }

    /// The in-place fan-out (C: `schedule_process_local`,
    /// `schedule.c:32-33` — priority and slice ride, the CPU stays home).
    ///
    /// C's `schedule_process` re-picks the CPU on every call (302); the
    /// rewrite does not: under a LOCAL mask the CPU never rides the wire,
    /// so re-picking changes nothing the kernel sees — it only mutates a
    /// private field and double-books the ledger (an accident of C's, not
    /// its contract; 10 D3 books one selection with one release).
    fn fanout_local(&self, index: usize, kernel: &mut impl KernelApi) -> i32 {
        let fanout = aggregate(ChangeMask::LOCAL, &self.row_values(index));
        code(kernel.schedule(&fanout))
    }

    /// A row as the fan-out sees it (C: `schedule_process`'s reads,
    /// `schedule.c:305-319`).
    fn row_values(&self, index: usize) -> SlotValues {
        let row = &self.procs[index];
        SlotValues {
            endpoint: row.endpoint,
            priority: row.priority,
            time_slice_ms: row.time_slice_ms,
            cpu: row.cpu,
            max_priority: row.max_priority,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_api::transport::mock::{MockIpc, MockKernel};
    use minix_sys::ipc::{CALL_NOTIFY, CALL_RECEIVE, IpcStatus, STATUS_FLAG_FROM_KERNEL};
    use minix_types::{
        MessLsysSchedSchedulingStart, MessLsysSchedSchedulingStop, MessPmSchedSchedulingSetNice,
        MessageUnion, SCHEDULING_NO_QUANTUM, SCHEDULING_SET_NICE, SCHEDULING_START,
        SCHEDULING_STOP,
    };

    const PM: Endpoint = Endpoint::PM;
    const CHILD: i32 = 20;

    fn status_call() -> IpcStatus {
        IpcStatus::from_call(CALL_RECEIVE)
    }

    fn status_kernel() -> IpcStatus {
        IpcStatus::with_flags(STATUS_FLAG_FROM_KERNEL)
    }

    fn status_notify() -> IpcStatus {
        IpcStatus::from_call(CALL_NOTIFY)
    }

    fn blank(type_: i32) -> Message {
        Message {
            m_source: PM,
            m_type: type_,
            m_u: MessageUnion::zeroed(),
        }
    }

    fn start_message(child: i32, parent: i32, maxprio: i32, quantum: i32) -> Message {
        let mut m = blank(SCHEDULING_START);
        m.m_u.m_lsys_sched_scheduling_start = MessLsysSchedSchedulingStart {
            endpoint: child,
            parent,
            maxprio,
            quantum,
            _padding: [0; 40],
        };
        m
    }

    fn stop_message(child: i32) -> Message {
        let mut m = blank(SCHEDULING_STOP);
        m.m_u.m_lsys_sched_scheduling_stop = MessLsysSchedSchedulingStop {
            endpoint: child,
            _padding: [0; 52],
        };
        m
    }

    fn nice_message(child: i32, maxprio: i32) -> Message {
        let mut m = blank(SCHEDULING_SET_NICE);
        m.m_u.m_pm_sched_scheduling_set_nice = MessPmSchedSchedulingSetNice {
            endpoint: child,
            maxprio: maxprio as u32,
            _padding: [0; 48],
        };
        m
    }

    fn noquantum_message(source: i32) -> Message {
        let mut m = blank(SCHEDULING_NO_QUANTUM);
        m.m_source = Endpoint(source);
        m
    }

    fn server(count: u32) -> SchedServer {
        SchedServer::new(MachineTopology {
            processors_count: count,
            bsp_id: 0,
        })
    }

    fn slot_state(server: &SchedServer, index: usize) -> (SlotState, u8, u32) {
        let row = &server.procs[index];
        (row.state, row.priority.get(), row.time_slice_ms)
    }

    #[test]
    fn test_init_scheduling_arms_bell() {
        // C 334-342: five seconds times the clock rate, armed once.
        let mut s = server(1);
        let mut kernel = MockKernel {
            hz: 100,
            ..MockKernel::new(1, 0)
        };
        s.init_scheduling(&mut kernel)
            .expect("init with a live bell");
        assert_eq!(kernel.setalarm_calls.borrow().as_slice(), [500]);
        assert!(s.balancer.is_some());
        // A refused arm propagates: the binary panics, the library reports.
        kernel.setalarm_rvs.borrow_mut().push(minix_types::EIO);
        assert!(s2_init_errors(kernel).is_err());
    }

    fn s2_init_errors(kernel: MockKernel) -> Result<(), i32> {
        let mut s = server(1);
        let mut kernel = kernel;
        s.init_scheduling(&mut kernel)
    }

    #[test]
    fn test_start_from_pm_happy_path() {
        // C 140-249 on the sunny path: doors pass, takeover registers, the
        // slot is born with START's explicit values, the fan-out carries
        // everything, and the reply names SCHED.
        let mut s = server(4);
        let mut kernel = MockKernel::new(4, 0);
        let ipc = MockIpc::default();
        ipc.deliver(start_message(CHILD, 50, 5, 100), status_call());
        let step = s.run_once(&ipc, &mut kernel);
        assert_eq!(step, Step::Handled);
        // The slot: occupied, priority 5, slice 100.
        assert_eq!(slot_state(&s, CHILD as usize), (SlotState::InUse, 5, 100));
        // The takeover registered for the child.
        assert_eq!(kernel.schedctl_calls.borrow().as_slice(), [Endpoint(CHILD)]);
        // The fan-out: ALL fields, niced false, CPU picked (least-loaded
        // non-BSP seat of four → seat 1; the ledger booked it).
        let calls = kernel.schedule_calls.borrow();
        assert_eq!(calls.len(), 1);
        let wire = calls[0];
        assert_eq!(wire.endpoint, CHILD);
        assert_eq!((wire.priority, wire.quantum), (5, 100));
        assert_eq!(wire.cpu, 1);
        assert_eq!(wire.niced, 0);
        assert_eq!(s.loads[1], Some(1));
        // The reply: OK, addressed to PM, naming SCHED as the scheduler.
        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        let (to, reply) = &sent[0];
        assert_eq!(*to, PM);
        assert_eq!(reply.m_type, 0);
        assert_eq!(
            unsafe { reply.m_u.m_sched_lsys_scheduling_start.scheduler },
            Endpoint::SCHED.0
        );
    }

    #[test]
    fn test_inherit_copies_parent_state() {
        // C 199-211: the child opens where the parent runs, with the
        // parent's share; the ceiling is the message's.
        let mut s = server(1);
        let parent = 12usize;
        s.procs[parent] = SchedProc {
            endpoint: Endpoint(12),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(7).expect("7 < 16"),
            priority: Priority::new(9).expect("9 < 16"),
            time_slice_ms: 150,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        let mut m = start_message(21, 12, 7, 0);
        m.m_type = SCHEDULING_INHERIT;
        ipc.deliver(m, status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(slot_state(&s, 21), (SlotState::InUse, 9, 150));
        assert_eq!(s.procs[21].max_priority.get(), 7);
        let sent = ipc.sent.borrow();
        assert_eq!(sent[0].1.m_type, 0);
    }

    #[test]
    fn test_start_refusals_touch_nothing() {
        // C 150-166, in order: a stranger's EPERM, a taken slot's
        // EDEADEPT, an illegal ceiling's EINVAL — and no takeover, no
        // birth, no reply payload beyond the error code.
        for (who, mtype, maxprio, expected) in [
            (Endpoint(100), SCHEDULING_START, 5, EPERM),           // stranger
            (PM, SCHEDULING_START, 16, minix_types::EINVAL),       // ceiling
        ] {
            let mut s = server(1);
            let mut kernel = MockKernel::default();
            let ipc = MockIpc::default();
            let mut m = start_message(CHILD, Endpoint::RS.0, maxprio, 200);
            m.m_source = who;
            m.m_type = mtype;
            ipc.deliver(m, status_call());
            s.run_once(&ipc, &mut kernel);
            assert_eq!(kernel.schedctl_calls.borrow().len(), 0, "no takeover");
            assert_eq!(s.procs[CHILD as usize].state, SlotState::Free);
            let sent = ipc.sent.borrow();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].1.m_type, expected);
        }
        // A taken slot refuses too (the START door is the vacant door).
        let mut s = server(1);
        s.procs[CHILD as usize].state = SlotState::InUse;
        s.procs[CHILD as usize].endpoint = Endpoint(CHILD);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(start_message(CHILD, 50, 5, 200), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, minix_types::EDEADEPT);
    }

    #[test]
    fn test_inherit_self_parent_refused() {
        // C 171 + 203: init is self-parented, but its slot is still
        // unflagged when the parent verdict runs — EDEADEPT, no birth.
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        let mut m = start_message(1, 1, 7, 0);
        m.m_type = SCHEDULING_INHERIT;
        ipc.deliver(m, status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, minix_types::EDEADEPT);
        assert_eq!(s.procs[1].state, SlotState::Free);
    }

    #[test]
    fn test_start_fanout_failure_leaves_slot_occupied() {
        // C 223 then 233-237: the takeover registered, the slot is IN_USE,
        // the fan-out failed — the residue is C's real behavior (semantic
        // 6), and the error code still rides the reply.
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        kernel.fail_next_schedule(minix_types::EINVAL);
        let ipc = MockIpc::default();
        ipc.deliver(start_message(CHILD, 50, 5, 100), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].state, SlotState::InUse);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, minix_types::EINVAL);
    }

    #[test]
    fn test_start_retries_after_dead_cpu() {
        // C 227-231: EBADCPU brands the seat and circles the ring; the
        // second attempt lands elsewhere and the ledger follows.
        let mut s = server(4);
        let mut kernel = MockKernel::new(4, 0);
        kernel.fail_next_schedule(minix_types::EBADCPU);
        let ipc = MockIpc::default();
        ipc.deliver(start_message(CHILD, 50, 5, 100), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(kernel.schedule_calls.borrow().len(), 2);
        // Seat 1 was tried first (all-fresh ledger ties at zero, lowest
        // non-BSP index wins), branded dead, and seat 2 carried the retry.
        assert_eq!(s.loads[1], None, "seat 1 was branded dead");
        assert_eq!(s.loads[2], Some(1), "seat 2 carries the booking");
        assert_eq!(s.procs[CHILD as usize].cpu, CpuId(2));
        assert_eq!(ipc.sent.borrow()[0].1.m_type, 0);
    }

    #[test]
    fn test_stop_clears_slot_and_ledger() {
        // C 112-135: two releases — the ledger sheds a unit, the slot
        // empties; the vacant door agrees afterwards.
        let mut s = server(4);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(5).expect("5 < 16"),
            priority: Priority::new(5).expect("5 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(2),
        };
        s.loads[2] = Some(3);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(stop_message(CHILD), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].state, SlotState::Free);
        assert_eq!(s.loads[2], Some(2));
        assert_eq!(ipc.sent.borrow()[0].1.m_type, 0);
    }

    #[test]
    fn test_stop_refuses_strangers_and_dead_slots() {
        // C 118-125: EPERM first, then the slot verdict.
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        let mut m = stop_message(CHILD);
        m.m_source = Endpoint(100); // neither PM nor RS
        ipc.deliver(m, status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, EPERM);
        // A vacant slot, even from PM: EDEADEPT.
        let ipc = MockIpc::default();
        ipc.deliver(stop_message(33), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, minix_types::EDEADEPT);
    }

    #[test]
    fn test_nice_regrades_and_rolls_back_on_failure() {
        // C 254-292: both numbers become the ceiling; a refused fan-out
        // restores the snapshot (semantic 5's rollback half).
        let mut s = server(1);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(7).expect("7 < 16"),
            priority: Priority::new(9).expect("9 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(nice_message(CHILD, 3), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 3);
        assert_eq!(s.procs[CHILD as usize].max_priority.get(), 3);
        // Rollback: the same regrade with the fan-out refused.
        let mut kernel = MockKernel::default();
        kernel.fail_next_schedule(minix_types::EINVAL);
        let ipc = MockIpc::default();
        ipc.deliver(nice_message(CHILD, 2), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 3, "restored");
        assert_eq!(s.procs[CHILD as usize].max_priority.get(), 3, "restored");
        assert_eq!(ipc.sent.borrow()[0].1.m_type, minix_types::EINVAL);
    }

    #[test]
    fn test_nice_refuses_strangers_and_bad_ceilings() {
        // C 262-276: EPERM first, then the slot, then the ceiling.
        let mut s = server(1);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(7).expect("7 < 16"),
            priority: Priority::new(9).expect("9 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        let mut m = nice_message(CHILD, 3);
        m.m_source = Endpoint(100);
        ipc.deliver(m, status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, EPERM);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 9, "untouched");
        // A legal sender with a past-the-end ceiling: EINVAL.
        let ipc = MockIpc::default();
        ipc.deliver(nice_message(CHILD, 16), status_call());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(ipc.sent.borrow()[0].1.m_type, minix_types::EINVAL);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 9, "untouched");
    }

    #[test]
    fn test_kernel_noquantum_demotes_and_never_replies() {
        // C 68-77 with 87-107: the seal holds, one step down, and no reply
        // — success or failure alike (semantic 2). The demotion stands even
        // when the fan-out fails (semantic 5's no-rollback half).
        let mut s = server(1);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(5).expect("5 < 16"),
            priority: Priority::new(5).expect("5 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(noquantum_message(CHILD), status_kernel());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 6);
        assert!(ipc.sent.borrow().is_empty(), "kernel is never answered");
        // The failure half: the demotion stands, the error is swallowed.
        let mut kernel = MockKernel::default();
        kernel.fail_next_schedule(minix_types::EINVAL);
        let ipc = MockIpc::default();
        ipc.deliver(noquantum_message(CHILD), status_kernel());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 7, "still sunk");
        assert!(ipc.sent.borrow().is_empty());
    }

    #[test]
    fn test_noquantum_floor_still_fans_out() {
        // C 99-101 at the floor: no change, but C 103 runs anyway — the
        // in-place fan-out (and its fresh quantum) is unconditional.
        let mut s = server(1);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(5).expect("5 < 16"),
            priority: Priority::new(15).expect("15 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(noquantum_message(CHILD), status_kernel());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 15);
        assert_eq!(kernel.schedule_calls.borrow().len(), 1);
    }

    #[test]
    fn test_forged_noquantum_answers_eperm() {
        // C 78-83: no kernel seal, no trust — EPERM rides the reply rule
        // (this one IS answered), and the slot is untouched (semantic 3).
        let mut s = server(1);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(5).expect("5 < 16"),
            priority: Priority::new(5).expect("5 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(noquantum_message(CHILD), status_call()); // no seal
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 5, "untouched");
        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1.m_type, EPERM);
        assert_eq!(kernel.schedule_calls.borrow().len(), 0);
    }

    #[test]
    fn test_clock_notification_rebalances_and_rearms() {
        // C 44-55 + 353-369: the CLOCK bell promotes one rung, fans out
        // without checking, and re-arms — with no reply (semantic 4 + 7).
        let mut s = server(1);
        s.procs[CHILD as usize] = SchedProc {
            endpoint: Endpoint(CHILD),
            parent: Endpoint::RS,
            state: SlotState::InUse,
            max_priority: Priority::new(8).expect("8 < 16"),
            priority: Priority::new(10).expect("10 < 16"),
            time_slice_ms: 100,
            cpu: CpuId(0),
        };
        let mut kernel = MockKernel::new(1, 0);
        let ipc = MockIpc::default();
        s.init_scheduling(&mut kernel).expect("bell armed");
        let mut clock = blank(0);
        clock.m_source = Endpoint::CLOCK;
        ipc.deliver(clock, status_notify());
        s.run_once(&ipc, &mut kernel);
        assert_eq!(s.procs[CHILD as usize].priority.get(), 9, "one rung up");
        // The fan-out carried LOCAL (priority rides, CPU keeps).
        let calls = kernel.schedule_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].priority, 9);
        assert_eq!(calls[0].cpu, -1, "LOCAL keeps the CPU (KEEP = -1)");
        // The bell rang again: 5s × 60Hz from the mock kernel.
        assert_eq!(kernel.setalarm_calls.borrow().len(), 2);
        assert!(ipc.sent.borrow().is_empty(), "notifications never answer");
    }

    #[test]
    fn test_other_notification_passes_in_silence() {
        // C 50-52: a non-CLOCK notification changes nothing.
        let mut s = server(1);
        let mut kernel = MockKernel::new(1, 0);
        let ipc = MockIpc::default();
        let mut m = blank(0);
        m.m_source = Endpoint::RS;
        ipc.deliver(m, status_notify());
        s.run_once(&ipc, &mut kernel);
        assert!(ipc.sent.borrow().is_empty());
        assert_eq!(kernel.schedule_calls.borrow().len(), 0);
        assert_eq!(kernel.setalarm_calls.borrow().len(), 0);
    }

    #[test]
    fn test_unknown_call_answers_enosys() {
        // C 85-86 + utility.c:18-23: a wild number earns ENOSYS as its
        // reply — refusal is an answer.
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default();
        ipc.deliver(blank(0x1234), status_call());
        s.run_once(&ipc, &mut kernel);
        let sent = ipc.sent.borrow();
        assert_eq!(sent[0].1.m_type, minix_types::ENOSYS);
    }

    #[test]
    fn test_reply_failure_does_not_kill_the_loop() {
        // C 101-106: a failed reply is dropped; the next turn runs (and
        // answers) as if nothing happened (semantic 1).
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc {
            send_result: Err(minix_types::EIO),
            ..MockIpc::default()
        };
        ipc.deliver(blank(0x1234), status_call());
        s.run_once(&ipc, &mut kernel); // the reply fails, silently
        assert_eq!(ipc.sent.borrow().len(), 1, "the attempt was made");
        // The next turn still works — and answers.
        let ipc2 = MockIpc::default();
        ipc2.deliver(blank(0x1234), status_call());
        s.run_once(&ipc2, &mut kernel);
        assert_eq!(ipc2.sent.borrow()[0].1.m_type, minix_types::ENOSYS);
    }

    #[test]
    fn test_receive_failure_is_reported_for_the_bound() {
        // The deviation from C 39-40: one failed receive loses one turn
        // and feeds `run`'s counter, instead of dying on the spot.
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default(); // empty script → EIO
        assert_eq!(s.run_once(&ipc, &mut kernel), Step::ReceiveFailed);
    }

    #[test]
    #[should_panic(expected = "IPC transport broken")]
    fn test_run_panics_after_sustained_receive_failures() {
        // The bound: 64 consecutive failures kill the loop loudly instead
        // of busy-spinning forever (VM's V10-P0-2 precedent).
        let mut s = server(1);
        let mut kernel = MockKernel::default();
        let ipc = MockIpc::default(); // every turn fails
        s.run(&ipc, &mut kernel);
    }
}
