//! E5(e) — the PM↔SCHED scheduling chain, end to end (hosted, over the wire
//! contract).
//!
//! The acceptance surface recorded for this chain: sched is the only service in
//! the repository where "every behavior depends on another party calling in"
//! (PM's START/INHERIT/SET_NICE, the kernel's NO_QUANTUM, its own 5-second
//! balance alarm); the three chains previously had only per-crate unit tests
//! and zero cross-service wire coverage. This file is the host half, shaped
//! after the pm_vm_fork.rs precedent: real code from all three sides runs
//! against itself, and only the end that cannot be driven on the host is
//! replaced by the wire contract.
//!
//! # The chain surface
//!
//! 1. **PM**: `minix_pm::sched::{sched_init, sched_start_user}` is the real
//!    state machine running every step, and outgoing traffic uses the real
//!    `MinixSchedCtl` (not a trait stand-in) — packing the SCHEDULING_*
//!    messages, the `sendrec` round trip and the rv decision all sit on the
//!    production code path. The PM-side transport is the local `PmSide`
//!    (implementing PM's `IpcTransport`): `sendrec` posts the request into the
//!    shared bus, then pumps one round of the SCHED main loop on the spot and
//!    takes the receipt, matching the timing of C's blocking `ipc_sendrec`
//!    (minix3/minix/lib/libsys/taskcall.c:16).
//! 2. **SCHED**: `minix_sched::server::SchedServer::run_once` is the real loop
//!    consuming requests from the bus — the five-letter dispatch, the birth /
//!    occupancy / seal gates and the fanout ring all sit on the production
//!    path. Its IPC side is the local `SchedSide` (implementing SCHED's
//!    `IpcTransport`), and its kernel side is the local `CapturedKernel`
//!    (implementing SCHED's `KernelApi`).
//! 3. **Kernel**: `minix-kernel` is not a dependency (the workspace feature
//!    unification it would bring has three errors that stand as the
//!    pre-existing baseline, and the decode halves of
//!    SYS_SCHEDCTL/SYS_SCHEDULE are already pinned by kernel-crate unit tests).
//!    The artifact reconciled against is the minix-types wire form produced by
//!    SCHED's production rendering functions: `SchedctlCall::wire()` (matching
//!    the read order of the kernel's `dispatch_schedctl`,
//!    os/kernel/src/syscall_process.rs:630) and `Fanout::wire_*` (matching the
//!    SchedParams read order of `do_schedule`,
//!    os/kernel/src/sched.rs:272-277); the NO_QUANTUM arrival side is built to
//!    the wire shape of the kernel's `notify_scheduler`
//!    (os/kernel/src/proc_table.rs:969-999: m_source = the spender, the
//!    FROM_KERNEL seal, and the m_krn_lsys_schedule accounting payload).
//!
//! # Coverage (three chains plus one broken-ring guard)
//!
//! - **START chain**: PM `sched_init` → SCHEDULING_START → `sys_schedctl`
//!   takeover registration + a full-field `sys_schedule` down-send → receipt OK
//!   naming SCHED.
//! - **INHERIT chain**: forked child `sched_start_user` → SCHEDULING_INHERIT →
//!   the child slot inherits the parent's current values (priority/quantum, C
//!   schedule.c:199-211).
//! - **NO_QUANTUM ring**: a kernel notification (sealed) → drop one level →
//!   `sys_schedule` written back in place (LOCAL: CPU KEEP) with no reply ever
//!   (C main.c:73-77); a forged seal → EPERM reply (C main.c:78-83).
//! - **Rejection pass-through**: a second START hits SCHED's occupancy gate
//!   (EDEADEPT) → PM's rv = the reply's m_type (C taskcall.c:17-20), so a
//!   rejection is no longer read as success.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use minix_pm::ipc::IpcTransport as PmIpc;
use minix_pm::ipc::IpcTransportError;
use minix_pm::mproc::{Credentials, Guardianship, Lifecycle, Privilege, ProcTable};
use minix_pm::sched::{
    MinixSchedCtl, SCHED_PROC_NR, USER_Q, USER_QUANTUM, sched_init, sched_start_user,
};
use minix_sched::cpu::MachineTopology;
use minix_sched::kernel_api::schedctl::{SchedctlCall, SchedulerAssignment};
use minix_sched::kernel_api::schedule::Fanout;
use minix_sched::kernel_api::transport::{IpcTransport as SchedIpc, KernelApi};
use minix_sched::sef::MachineInfo;
use minix_sched::server::{SchedServer, Step};
use minix_sys::ipc::{CALL_RECEIVE, IpcStatus, STATUS_FLAG_FROM_KERNEL};
use minix_types::ipc::{MessKrnLsysSchedule, MessLsysKrnSchedule};
use minix_types::{EDEADEPT, EPERM, Endpoint, Message, SCHEDULING_NO_QUANTUM, UserSlot};

// ---------------------------------------------------------------------------
// The shared bus: PM's sendrec and SCHED's receive/send meet here
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BusInner {
    /// The PM → SCHED request queue; each message carries its arrival status
    /// word (an ordinary call has no flag, a kernel notification carries the
    /// FROM_KERNEL seal — SCHED's noquantum_trust reads it).
    to_sched: RefCell<VecDeque<(Message, IpcStatus)>>,
    /// The SCHED → PM receipt queue (the return leg of `sendrec`).
    to_pm: RefCell<VecDeque<Message>>,
    /// A record of every SCHED `send` landing (used by receipt-side
    /// assertions).
    sched_sent: RefCell<Vec<(Endpoint, Message)>>,
}

#[derive(Clone, Default)]
struct Bus(Rc<BusInner>);

impl Bus {
    /// Delivery from the kernel side: bypass PM and put an arrival straight
    /// into SCHED's inbox (used by the NO_QUANTUM chain — the kernel's
    /// `mini_send` does not go through PM).
    fn deliver_from_kernel(&self, message: Message) {
        self.0
            .to_sched
            .borrow_mut()
            .push_back((message, IpcStatus::with_flags(STATUS_FLAG_FROM_KERNEL)));
    }
}

/// The SCHED-side transport: dequeue arrivals, record sends and add them to
/// the receipt queue.
struct SchedSide {
    bus: Bus,
}

impl SchedIpc for SchedSide {
    fn receive(&self) -> Result<(Message, IpcStatus), i32> {
        self.bus.0.to_sched.borrow_mut().pop_front().ok_or(0)
    }

    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        self.bus.0.sched_sent.borrow_mut().push((to, *message));
        self.bus.0.to_pm.borrow_mut().push_back(*message);
        Ok(())
    }

    // After SchedIpc gained the non-blocking sendnb, this mock was not
    // updated in step and `cargo build --workspace --all-targets` failed to
    // compile (E0046). This chain's SCHED never calls sendnb; the body mirrors
    // send's delivery accounting (non-blocking differs only in semantics).
    fn sendnb(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        self.bus.0.sched_sent.borrow_mut().push((to, *message));
        self.bus.0.to_pm.borrow_mut().push_back(*message);
        Ok(())
    }
}

/// The SCHED-side kernel seam: accounts through the same rendering function
/// family as the production `SysKernelApi` (the wire order at
/// transport.rs:356-364); the answers are scriptable (OK by default).
#[derive(Default)]
struct CapturedKernel {
    schedctl_calls: RefCell<Vec<SchedctlCall>>,
    schedctl_rvs: RefCell<VecDeque<i32>>,
    schedule_calls: RefCell<Vec<MessLsysKrnSchedule>>,
    schedule_rvs: RefCell<VecDeque<i32>>,
    setalarm_calls: RefCell<Vec<u32>>,
    hz: u32,
}

impl CapturedKernel {
    fn with_hz(hz: u32) -> Self {
        Self {
            hz,
            ..Default::default()
        }
    }
}

impl KernelApi for CapturedKernel {
    fn get_machine(&mut self) -> Result<MachineInfo, i32> {
        Ok(MachineInfo {
            processors_count: 2,
            bsp_id: 0,
        })
    }

    fn get_hz(&mut self) -> Result<u32, i32> {
        Ok(self.hz)
    }

    fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32> {
        self.schedctl_calls.borrow_mut().push(*call);
        match self.schedctl_rvs.borrow_mut().pop_front() {
            Some(0) | None => Ok(()),
            Some(errno) => Err(errno),
        }
    }

    fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32> {
        self.schedule_calls.borrow_mut().push(MessLsysKrnSchedule {
            endpoint: fanout.endpoint.0,
            quantum: fanout.wire_quantum_ms(),
            priority: fanout.wire_priority(),
            cpu: fanout.wire_cpu(),
            niced: fanout.wire_niced(),
            _padding: [0; 36],
        });
        match self.schedule_rvs.borrow_mut().pop_front() {
            Some(0) | None => Ok(()),
            Some(errno) => Err(errno),
        }
    }

    fn setalarm(&mut self, ticks: u32) -> Result<(), i32> {
        self.setalarm_calls.borrow_mut().push(ticks);
        Ok(())
    }
}

/// The PM-side transport: after posting a request, `sendrec` pumps one round of
/// the SCHED main loop on the spot and then takes the receipt — matching the
/// timing of C's blocking sendrec (taskcall.c:16 `ipc_sendrec`).
struct PmSide<'a> {
    bus: Bus,
    pump: RefCell<&'a mut dyn FnMut()>,
}

impl PmIpc for PmSide<'_> {
    fn receive(&mut self) -> Result<(Message, minix_pm::ipc::IpcStatus), IpcTransportError> {
        Err(IpcTransportError::WouldBlock) // on this chain PM receives nothing
    }

    fn send(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), IpcTransportError> {
        panic!("E5(e) 链上 PM 只对 SCHED 做 sendrec，无异步发送")
    }

    // After PmIpc gained send_blocking this mock was not updated in step, so
    // --workspace did not compile (E0046). This chain's PM only sendrecs to
    // SCHED and never blocks in send (same semantics as send()): being called
    // would mean something that cannot happen on the chain, so it fails
    // loudly.
    fn send_blocking(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), IpcTransportError> {
        panic!("E5(e) 链上 PM 只对 SCHED 做 sendrec，无 send_blocking")
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError> {
        assert_eq!(dest, SCHED_PROC_NR, "E5(e) 链上 PM 只对 SCHED sendrec");
        // The real kernel stamps m_source with the sender's identity on
        // delivery (os/kernel/src/ipc.rs:1067
        // `p_delivermsg.m_source = caller_endpoint`); on the host bus this
        // transport half applies that stamp, and SCHED's origin gate
        // (valid.rs accept_message) recognizes PM by it.
        msg.m_source = Endpoint::PM;
        self.bus
            .0
            .to_sched
            .borrow_mut()
            .push_back((*msg, IpcStatus::from_call(CALL_RECEIVE)));
        (self.pump.borrow_mut())();
        *msg = self
            .bus
            .0
            .to_pm
            .borrow_mut()
            .pop_front()
            .expect("SCHED 必须对 taskcall 回执");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const INIT_SLOT: usize = 11;
const CHILD_SLOT: usize = 12;

fn init_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, INIT_SLOT as i32)
}

fn child_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, CHILD_SLOT as i32)
}

/// The INIT fixture: slot 11 in use, self-parent, scheduler still KERNEL (C's
/// initial state at main.c:199; START at schedule.c:37-43 hands it to SCHED).
fn table_with_init() -> ProcTable {
    let mut table = ProcTable::new();
    let p = &mut table.procs[INIT_SLOT];
    p.identity.endpoint = init_endpoint();
    p.identity.id.pid = 1;
    p.state.lifecycle = Lifecycle::Running;
    p.state.guardianship = Guardianship::Normal {
        parent: UserSlot::new(INIT_SLOT),
    };
    p.resources.nice = 0;
    p.resources.scheduler = Endpoint::KERNEL;
    p.resources.privilege = Privilege::User(Credentials::new(0, 0));
    table
}

/// The forked child's mproc state (the shape visible at the main.c:373 call
/// site: scheduler and nice are both copied from the parent slot).
fn add_forked_child(table: &mut ProcTable) {
    let c = &mut table.procs[CHILD_SLOT];
    c.identity.endpoint = child_endpoint();
    c.identity.id.pid = 2;
    c.state.lifecycle = Lifecycle::Running;
    c.state.guardianship = Guardianship::Normal {
        parent: UserSlot::new(INIT_SLOT),
    };
    c.resources.nice = 0;
    c.resources.scheduler = Endpoint::SCHED;
    c.resources.privilege = Privilege::User(Credentials::new(0, 0));
}

/// A two-CPU SCHED server plus an armed kernel seam (5s × 60Hz = 300 ticks).
fn server_with_kernel() -> (SchedServer, CapturedKernel) {
    let mut kernel = CapturedKernel::with_hz(60);
    let mut server = SchedServer::new(MachineTopology {
        processors_count: 2,
        bsp_id: 0,
    });
    server
        .init_scheduling(&mut kernel)
        .expect("balance bell armed");
    (server, kernel)
}

/// The wire shape of a kernel NO_QUANTUM notification (proc_table.rs:969-999:
/// m_source = the spender, the m_krn_lsys_schedule accounting payload; the
/// FROM_KERNEL seal rides in the status word of deliver_from_kernel).
fn no_quantum_notify(spender: Endpoint) -> Message {
    let mut message = Message {
        m_type: SCHEDULING_NO_QUANTUM,
        m_source: spender,
        ..Message::default()
    };
    message.m_u.m_krn_lsys_schedule = MessKrnLsysSchedule {
        acnt_queue: 10,
        acnt_deqs: 2,
        acnt_ipc_sync: 1,
        acnt_ipc_async: 0,
        acnt_preempt: 0,
        acnt_cpu: 0,
        acnt_cpu_load: 42,
        _padding: [0; 24],
    };
    message
}

// ---------------------------------------------------------------------------
// E5(e).1 START chain: PM sched_init → SCHED takeover → kernel down-send →
// receipt naming SCHED
// ---------------------------------------------------------------------------

#[test]
fn start_chain_pm_sched_takeover_and_kernel_fanout() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        let results = sched_init(&mut table, &mut ctl);
        assert_eq!(
            results.len(),
            1,
            "用户进程只有 INIT 一个（schedule.c:26-31）"
        );
        assert_eq!(results[0].0, UserSlot::new(INIT_SLOT));
        assert_eq!(
            results[0].1, 0,
            "START 成功：rv = 回复 m_type（taskcall.c:19-20）"
        );
    }

    // PM side: INIT's scheduler has been handed from KERNEL to SCHED (the
    // success half of schedule.c:40-48; the refusal half is pinned by the
    // refuse test).
    assert_eq!(table.procs[INIT_SLOT].resources.scheduler, Endpoint::SCHED);

    // Kernel wire one: sys_schedctl takeover registration (a plain
    // registration with flags=0, do_schedctl.c:40-43; the decode half is at
    // os/kernel/src/syscall_process.rs:630).
    let schedctls = kernel.schedctl_calls.borrow();
    assert_eq!(schedctls.len(), 1);
    assert_eq!(schedctls[0].target, init_endpoint());
    assert_eq!(schedctls[0].assignment, SchedulerAssignment::Server);
    assert_eq!(schedctls[0].wire().flags, 0, "注册分支不携带 KERNEL 旗标");

    // Kernel wire two: a full-field sys_schedule down-send (the factory
    // USER_Q/USER_QUANTUM values; the decode half is SchedParams at
    // os/kernel/src/sched.rs:272-277).
    let schedules = kernel.schedule_calls.borrow();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].endpoint, init_endpoint().0);
    assert_eq!(schedules[0].priority, USER_Q, "maxprio = nice 0 → 队列 7");
    assert_eq!(schedules[0].quantum, USER_QUANTUM);
    assert_eq!(schedules[0].cpu, 1, "2 CPU 拓扑：非 BSP 座位承接用户进程");
    assert_eq!(schedules[0].niced, 0, "天花板=USER_Q 不算 niced");

    // Receipt side: OK, and the receipt payload names SCHED — C
    // sched_start.c:73-75 reads this field back into *newscheduler_e, the
    // authoritative source for PM writing back Endpoint::SCHED.
    let sent = bus.0.sched_sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, Endpoint::PM);
    assert_eq!(sent[0].1.m_type, 0);
    // SAFETY(test): the receipt's active union arm is
    // m_sched_lsys_scheduling_start (server.rs do_start's payload_mut writes the
    // same arm).
    unsafe {
        assert_eq!(
            sent[0].1.m_u.m_sched_lsys_scheduling_start.scheduler,
            Endpoint::SCHED.0
        );
    }

    // The balance bell is already armed (5s × 60Hz), ahead of every business
    // arrival.
    assert_eq!(kernel.setalarm_calls.borrow().as_slice(), [300]);
}

// ---------------------------------------------------------------------------
// E5(e).2 INHERIT chain: the forked child inherits the parent slot's current
// values through sched_start_user
// ---------------------------------------------------------------------------

#[test]
fn inherit_chain_fork_child_copies_parent_slot() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    // Run the START chain first: SCHED's slot 11 now holds INIT's row
    // (priority 7 / slice 200) — exactly what INHERIT's parent-slot probe
    // reads.
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        assert_eq!(sched_init(&mut table, &mut ctl)[0].1, 0);
    }
    let schedctls = kernel.schedctl_calls.borrow();
    assert_eq!(schedctls.len(), 1, "前链恰好一次注册");
    drop(schedctls);

    // fork: the child lands in slot 12 with scheduler/nice copied from the
    // parent slot (the call-site state at main.c:371-373); sched_start_user
    // then sends INHERIT.
    add_forked_child(&mut table);
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        sched_start_user(
            &mut table,
            child_endpoint(),
            UserSlot::new(CHILD_SLOT),
            &mut ctl,
        )
        .expect("INHERIT 应成功");
    }

    // PM side: the child's scheduler stays SCHED (the read-back semantics of C
    // sched_start.c:32-36 are equivalent on this line: SCHED does not forward,
    // and the receipt's scheduler is the receiver itself).
    assert_eq!(table.procs[CHILD_SLOT].resources.scheduler, Endpoint::SCHED);

    // Kernel wire: the second down-send carries the child slot's inherited
    // values — priority/quantum come from the parent's current values (C
    // schedule.c:199-211), and the ceiling from the message's nice→queue.
    let schedules = kernel.schedule_calls.borrow();
    assert_eq!(schedules.len(), 2, "START + INHERIT 两次下发");
    assert_eq!(schedules[1].endpoint, child_endpoint().0);
    assert_eq!(schedules[1].priority, 7, "子继承父槽当前优先级");
    assert_eq!(schedules[1].quantum, USER_QUANTUM, "子继承父槽时间片");
    assert_eq!(schedules[1].niced, 0);
    drop(schedules);

    // Registration side: the takeover target is the child's endpoint.
    let schedctls = kernel.schedctl_calls.borrow();
    assert_eq!(schedctls[1].target, child_endpoint());
}

// ---------------------------------------------------------------------------
// E5(e).3 NO_QUANTUM ring: kernel notification → level drop → in-place
// write-back; forged seal → EPERM
// ---------------------------------------------------------------------------

#[test]
fn no_quantum_loop_kernel_notify_demotes_and_writes_back() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    // Prior chain: INIT already sits in SCHED's queue 7 / 200ms.
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        assert_eq!(sched_init(&mut table, &mut ctl)[0].1, 0);
    }

    // A kernel quantum-exhaustion notification arrives (the real send half is
    // notify_scheduler, proc_table.rs:937-1013; the host constructs the arrival
    // in the same wire shape).
    bus.deliver_from_kernel(no_quantum_notify(init_endpoint()));
    assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);

    // Write-back side: a LOCAL fanout — drop one level 7→8, quantum carried
    // along, CPU KEEP (C schedule.c:99-105 plus schedule_process_local :32-33)
    // — and the kernel is never replied to (C main.c:73-77).
    let schedules = kernel.schedule_calls.borrow();
    assert_eq!(schedules.len(), 2, "START + 降级回写");
    assert_eq!(schedules[1].endpoint, init_endpoint().0);
    assert_eq!(schedules[1].priority, 8, "降一级");
    assert_eq!(schedules[1].quantum, USER_QUANTUM, "LOCAL 携带时间片");
    assert_eq!(schedules[1].cpu, -1, "KEEP：原地扇出不迁移");
    drop(schedules);
    assert!(
        bus.0.to_pm.borrow().is_empty(),
        "内核通知永不回复（C main.c:73-77 continue）"
    );

    // Broken-ring boundary guard: the same message without the seal = forged →
    // an EPERM reply (C main.c:78-83), and the slot is not demoted again (no new
    // down-send).
    let forged = no_quantum_notify(init_endpoint());
    bus.0
        .to_sched
        .borrow_mut()
        .push_back((forged, IpcStatus::from_call(CALL_RECEIVE)));
    assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
    let sent = bus.0.sched_sent.borrow();
    assert_eq!(
        sent.len(),
        2,
        "前链 START 回执 + 伪造回复各一条（走普通回复面）"
    );
    // Wire-order signs: C's SCHED is compiled with `_SYSTEM`
    // (minix3/minix/servers/sched/sched.h:4), so the error codes written back at
    // main.c:89-96 already carry a negative sign; this tree's decision surface
    // uses positive constants and folds the sign at the reply boundary
    // (minix_types::reply_status), so what is pinned here is the negative
    // value.
    assert_eq!(sent[1].1.m_type, -EPERM);
    drop(sent);
    assert_eq!(
        kernel.schedule_calls.borrow().len(),
        2,
        "伪造通知不触发任何下发"
    );
}

// ---------------------------------------------------------------------------
// E5(e).4 Rejection pass-through: SCHED's occupancy-gate rejection (EDEADEPT)
// is visible to PM through rv
// ---------------------------------------------------------------------------

#[test]
fn sched_refusal_surfaces_to_pm_as_nonzero_rv() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    // Prior chain: START succeeded, and SCHED's slot 11 is occupied by INIT.
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        assert_eq!(sched_init(&mut table, &mut ctl)[0].1, 0);
    }

    // A second sched_init: the same INIT sends START again → hits the birth
    // gate (C sched_isemtyendpt → EDEADEPT, utility.c:46-56). C
    // taskcall.c:17-20 hands the rejection code to PM as the rv — if taskcall
    // swallowed the reply's m_type, this would read 0 (success), and the
    // rejection-warning branch at schedule.c:44-47 would be dead forever.
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        let results = sched_init(&mut table, &mut ctl);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].1, -EDEADEPT,
            "拒绝码必须经 rv 透传（taskcall.c:19-20）；线序上是负值（C `_SYSTEM` SCHED）"
        );
    }
    assert_eq!(
        table.procs[INIT_SLOT].resources.scheduler,
        Endpoint::SCHED,
        "拒绝分支只告警，不改写已成立的调度器（schedule.c:44-47）"
    );

    // The rejection chain produces no new kernel call (the birth gate precedes
    // schedctl, C :154-157).
    assert_eq!(kernel.schedctl_calls.borrow().len(), 1);
    assert_eq!(kernel.schedule_calls.borrow().len(), 1);
}
