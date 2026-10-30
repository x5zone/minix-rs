//! PM process event publish/subscribe facility.
//!
//! Corresponds to the Minix3 C sources:
//! - `minix3/minix/servers/pm/event.c:1-353` — the whole facility
//! - `minix3/minix/servers/pm/mproc.h:27` — `mp_eventsub`
//! - `minix3/minix/servers/pm/const.h:13` — `NO_EVENTSUB`
//! - `minix3/minix/include/minix/com.h:597-619` — the `PROC_EVENT` family
//! - `minix3/minix/include/minix/syslib.h:289-293` — `PROC_EVENT_EXIT/SIGNAL`
//!
//! The design contract of this facility is recorded in the PM stage's design
//! snapshot.
//!
//! # Single-threaded model
//!
//! PM is a user-space single-threaded event loop; `EventRegistry` is owned
//! exclusively by `PmServer`, so `&mut EventRegistry` / `&mut ProcTable` /
//! `&mut dyn IpcTransport` are safe under one thread. `nested` is a `usize`
//! counter, not an `Atomic`.

use minix_types::{Endpoint, Message, ProcEvent, ProcEventMask, UserSlot};

use crate::ipc::IpcTransport;
use crate::ipc::ReplyIntent;
use crate::mproc::{EventCursor, ProcTable};

/// The production service assembly for event termination dispatch (the
/// `restart_sigs` entry at `event.c:122-123`).
///
/// In C `restart_sigs` calls module-level functions directly; on the Rust side
/// they are injected through [`crate::signal_flow::RestartServices`], with
/// `transport` as the only external dependency. Implementations of the three
/// member traits:
/// - `SignalDeliver` → `crate::signal::sig_proc` (`trace=false`, C's
///   `check_pending` redelivery path `signal.c:670`);
/// - `ExitHandler` → `crate::exit::exit_proc` (`dump_core=false`, C's
///   TRACE_EXIT branch `signal.c:697`);
/// - `KernelResume` → `sys_resume` (`signal.c:282`). Until minix-sys's kernel
///   call face lands, no real stopped state exists on the kernel side —
///   `block.stopped` is set by PM's own `unpause`/`stop_proc`, needs no kernel
///   undo, and returning OK lets `try_resume_proc` clear the PM-side state;
///   once that face lands this is replaced by a real `sys_resume`.
pub(crate) struct PmEventServices<'a, T: IpcTransport + ?Sized> {
    transport: &'a mut T,
    /// The kernel call exit (sys_times accounting on sig_proc's termination
    /// path).
    kern: &'a mut dyn crate::exit::KernelGateway,
}

impl<'a, T: IpcTransport + ?Sized> PmEventServices<'a, T> {
    /// The production adapter shared by the event and signal domains (the
    /// full RestartServices implementation for restart_sigs; the VFS reply
    /// tail's `restart_signals` reuses it too).
    pub(crate) fn new(
        transport: &'a mut T,
        kern: &'a mut dyn crate::exit::KernelGateway,
    ) -> Self {
        Self { transport, kern }
    }
}

impl<T: IpcTransport + ?Sized> crate::signal_flow::SignalDeliver for PmEventServices<'_, T> {
    fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, signo: i32, ksig: bool) {
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::sig_proc(table, target, signo, false, ksig, self.kern, self.transport);
    }
}

impl<T: IpcTransport + ?Sized> crate::signal_flow::ExitHandler for PmEventServices<'_, T> {
    fn exit_proc(&mut self, table: &mut ProcTable, target: UserSlot, status: i8) {
        // The kernel exit (step 9 sys_clear) uses the production gateway:
        // TRACE_EXIT targets are user processes, so step 9 does not fire; if it
        // does, panic honestly (C fails the same way).
        let mut kern =
            crate::exit::TrapKernelGateway::new(minix_sys::syscall::DirectKernelCallTransport);
        crate::exit::exit_proc(table, target, status, 0, false, self.transport, &mut kern);
    }
}

impl<T: IpcTransport + ?Sized> crate::signal_flow::KernelResume for PmEventServices<'_, T> {
    fn resume(&mut self, ep: Endpoint) -> i32 {
        // C: signal.c:282 — sys_resume(ep) resumes the stopped process. A
        // non-OK result makes try_resume_proc panic (signal.c:285). The Trap
        // gateway answers -EIO, which panics (fail closed, no faked success);
        // the test-injected mock always answers OK.
        self.kern.sys_resume(ep).err().unwrap_or(0) // OK = 0; Err carries the raw errno
    }
}

impl<T: IpcTransport + ?Sized> crate::signal_flow::RestartServices for PmEventServices<'_, T> {}

/// Subscription table cap (`event.c:58` `NR_SUBS 4`).
pub const NR_SUBS: usize = 4;

/// `NO_EVENTSUB` is expressed as `None` in Rust (when `EventCall` is absent).
/// For cross-service debug printing it can be formatted as `-1` (`const.h:13`).
pub const NO_EVENTSUB_RAW: i8 = -1;

/// A single subscriber (`event.c:60-64` `subs[i]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscriber {
    /// The subscriber's endpoint (`subs[i].endpt`).
    pub endpoint: Endpoint,
    /// Bitmask of the events it cares about (`subs[i].mask`).
    pub mask: ProcEventMask,
    /// How many processes are blocked waiting for its reply
    /// (`subs[i].waiting`).
    pub waiting: usize,
}

/// The process event registry (the `subs` / `nsubs` / `nested` aggregate of
/// `event.c:60-67`).
///
/// `subs[0..nsubs)` is the compact prefix (`Option::Some`) and `nsubs` is the
/// used slot count; `nested` is the reentry guard counter (`event.c:67`),
/// bumped `+=1`/`-=1` only around `remove_sub`'s `resume_event` call.
#[derive(Debug)]
pub struct EventRegistry {
    subs: [Option<Subscriber>; NR_SUBS],
    nsubs: usize,
    nested: usize,
}

impl Default for EventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            subs: [None; NR_SUBS],
            nsubs: 0,
            nested: 0,
        }
    }

    /// Used slot count.
    pub fn len(&self) -> usize {
        self.nsubs
    }

    /// Whether it is empty.
    pub fn is_empty(&self) -> bool {
        self.nsubs == 0
    }

    /// Whether it is full (a new subscription with `nsubs == NR_SUBS` →
    /// `ENOMEM`).
    pub fn is_full(&self) -> bool {
        self.nsubs >= NR_SUBS
    }

    /// Reentry counter (readable by tests).
    #[cfg(test)]
    pub fn nested(&self) -> usize {
        self.nested
    }

    /// The waiting count of one subscriber (readable by tests).
    #[cfg(test)]
    pub fn waiting(&self, idx: usize) -> Option<usize> {
        self.subs.get(idx)?.as_ref().map(|s| s.waiting)
    }

    /// The mask of one subscriber (readable by tests).
    #[cfg(test)]
    pub fn mask(&self, idx: usize) -> Option<ProcEventMask> {
        self.subs.get(idx)?.as_ref().map(|s| s.mask)
    }

    /// The endpoint of one subscriber (readable by tests).
    #[cfg(test)]
    pub fn endpoint(&self, idx: usize) -> Option<Endpoint> {
        self.subs.get(idx)?.as_ref().map(|s| s.endpoint)
    }

    // ── Core: publish / resume / remove ──

    /// Publish an event (`event.c:316-353` `publish_event`).
    ///
    /// Three stages: (1) assert `nested==0` + target `IN_USE && !EVENT_CALL` +
    /// cursor None; (2) if `PRIV_PROC|EXITING` are both set (a system service
    /// in the middle of exiting), scan `subs` for
    /// `endpoint == target.endpoint` and `remove_sub`; (3) set
    /// `EventCall { cursor: 0 }` and `resume_event`.
    pub fn publish_event<T: IpcTransport + ?Sized>(
        &mut self,
        target: UserSlot,
        table: &mut ProcTable,
        transport: &mut T,
        kern: &mut dyn crate::exit::KernelGateway,
    ) {
        assert_eq!(self.nested, 0, "publish_event: nested must be 0");
        let proc = &table.procs[target.get()];
        assert!(proc.is_in_use(), "publish_event: target must be IN_USE");
        assert!(
            !proc.state.block.is_event_blocked(),
            "publish_event: target must not already be EVENT_CALL"
        );
        assert!(
            proc.state.block.event_cursor().is_none(),
            "publish_event: cursor must be None"
        );

        // (2) Dead-service cleanup: an exiting privileged service that is
        // itself a subscriber is removed first (event.c:330-343; the leftover
        // reply after unsubscribing is ignored by do_proc_event_reply).
        if proc.is_kernel_process() && proc.is_exiting() {
            let ep = proc.endpoint();
            let mut to_remove: Option<usize> = None;
            for i in 0..self.nsubs {
                if let Some(sub) = self.subs[i]
                    && sub.endpoint == ep {
                        to_remove = Some(i);
                        break;
                    }
            }
            if let Some(slot) = to_remove {
                self.remove_sub(slot, table, transport, kern);
            }
        }

        // (3) Set EVENT_CALL + cursor 0 → resume
        table.procs[target.get()]
            .state
            .block
            .set_event_blocked(EventCursor(0));
        self.resume_event(target, table, transport, kern);
    }

    /// Advance the event serially (`event.c:74-123` `resume_event`).
    ///
    /// (1) Infer the event (`EXITING → Exit` / `UNPAUSED → Signal`, else
    /// panic); (2) `for i = cursor .. nsubs`, and the first
    /// `mask.contains(event)` gets `asynsend3` + `waiting++` and then `return`s
    /// (suspended); (3) no more matching subscribers → clear `EventCall` →
    /// termination dispatch: Exit takes `exit_restart`, Signal takes
    /// `restart_sigs` (now for real).
    pub fn resume_event<T: IpcTransport + ?Sized>(
        &mut self,
        target: UserSlot,
        table: &mut ProcTable,
        transport: &mut T,
        kern: &mut dyn crate::exit::KernelGateway,
    ) {
        // (1) Assertions and event inference
        let (event, mut cursor) = {
            let proc = &table.procs[target.get()];
            assert!(proc.is_in_use(), "resume_event: target must be IN_USE");
            let cur = proc
                .state
                .block
                .event_cursor()
                .expect("resume_event: must be EVENT_CALL");
            let ev = if proc.is_exiting() {
                ProcEvent::Exit
            } else if proc.state.block.unpaused {
                ProcEvent::Signal
            } else {
                panic!(
                    "resume_event: unknown event for flags (slot {}, lifecycle {:?}, unpaused {})",
                    target.get(),
                    proc.state.lifecycle,
                    proc.state.block.unpaused
                );
            };
            (ev, cur)
        };

        // (2) Serial scan
        while cursor.0 < self.nsubs {
            let sub = self.subs[cursor.0].expect("resume_event: subs prefix must be Some");
            if sub.mask.contains_event(event) {
                // Build the PROC_EVENT message (event.c:99-103)
                let msg = minix_types::proc_event_msg(table.procs[target.get()].endpoint(), event);
                // C: a failing asynsend3(..., AMF_NOREPLY) panics
                // (event.c:104-106)
                transport
                    .send(sub.endpoint, &msg)
                    .expect("resume_event: asynsend to subscriber failed");
                // waiting++ (event.c:108-109). C's assert runs unconditionally
                // **before** the increment (`assert(waiting < NR_PROCS)`) and
                // holds in a release build too; the bound references the
                // NR_PROCS constant instead of a hardcoded 256.
                assert!(
                    self.subs[cursor.0].expect("resume_event: cursor in prefix").waiting
                        < minix_types::NR_PROCS,
                    "resume_event: waiting must stay below NR_PROCS"
                );
                self.subs[cursor.0].as_mut().unwrap().waiting += 1;
                // Update the cursor (it now points at the current subscriber,
                // whose reply we await)
                table.procs[target.get()].state.block.set_event_blocked(cursor);
                return;
            }
            cursor.0 += 1;
        }

        // (3) No more matching subscribers → clear EVENT_CALL + cursor →
        // termination dispatch
        table.procs[target.get()].state.block.clear_event_blocked();
        // C: the termination dispatch of event.c:115-122 (the 09/13 entry)
        match event {
            ProcEvent::Exit => {
                // 09-pm-exit.md: VFS has already replied EXIT and the event
                // has gone to every subscriber → second-phase teardown
                crate::exit::exit_restart(table, target, transport, kern);
            }
            ProcEvent::Signal => {
                // C: event.c:122-123 — restart_sigs(rmp) (13-signal-flow.md):
                // after clearing EVENT_CALL, redeliver pending signals and
                // resume the process as needed.
                let mut svc = PmEventServices { transport, kern };
                crate::signal_flow::restart_sigs(table, target, &mut svc);
            }
        }
    }

    /// Remove a subscriber in order (`event.c:130-161` `remove_sub`).
    ///
    /// (1) Shift the compact prefix of `subs` forward, `nsubs--`; (2) walk the
    /// whole `mproc[NR_PROCS]` table and, for every entry with
    /// `IN_USE|EVENT_CALL` and a valid cursor:
    /// `cursor == slot → nested++ → resume_event → nested--`;
    /// `cursor > slot → cursor--`.
    pub fn remove_sub<T: IpcTransport + ?Sized>(
        &mut self,
        slot: usize,
        table: &mut ProcTable,
        transport: &mut T,
        kern: &mut dyn crate::exit::KernelGateway,
    ) {
        assert!(slot < self.nsubs, "remove_sub: slot out of range");

        // (1) Shift the array forward
        self.subs.copy_within(slot + 1..self.nsubs, slot);
        self.subs[self.nsubs - 1] = None;
        self.nsubs -= 1;

        // (2) Fix the cursors of the affected processes
        for idx in 0..minix_types::NR_PROCS {
            let proc = &table.procs[idx];
            if !proc.is_in_use() || !proc.state.block.is_event_blocked() {
                continue;
            }
            let cur = proc.state.block.event_cursor().expect("must be EventCall");
            if cur.0 == slot {
                // This process waits on the removed subscriber → advance it
                // immediately
                self.nested += 1;
                // SAFETY: idx was already checked in_use and EVENT_CALL, so
                // resume_event's precondition holds
                self.resume_event(UserSlot::new(idx), table, transport, kern);
                self.nested -= 1;
            } else if cur.0 > slot {
                // The cursor points past the removed position → step back by 1
                let new_cur = EventCursor(cur.0 - 1);
                table.procs[idx].state.block.set_event_blocked(new_cur);
            }
        }
    }

    // ── do_proceventmask / do_proc_event_reply ──


    /// The full mutable-table version of `do_proceventmask` (for when
    /// `remove_sub` needs to adjust cursors).
    ///
    /// When a subscription with `mask==0 && waiting==0` needs `remove_sub` and
    /// affected processes exist, the caller should use this method so the
    /// cursors are rolled back correctly.
    pub fn do_proceventmask_mut(
        &mut self,
        caller: UserSlot,
        mask: ProcEventMask,
        table: &mut ProcTable,
        transport: &mut dyn IpcTransport,
        kern: &mut dyn crate::exit::KernelGateway,
    ) -> ReplyIntent {
        if !table.procs[caller.get()].is_kernel_process() {
            return ReplyIntent::Reply(minix_types::EPERM);
        }

        for i in 0..self.nsubs {
            if let Some(sub) = self.subs[i]
                && sub.endpoint == table.procs[caller.get()].endpoint() {
                    if mask.is_empty() && sub.waiting == 0 {
                        self.remove_sub(i, table, transport, kern);
                    } else {
                        self.subs[i].as_mut().unwrap().mask = mask;
                    }
                    return ReplyIntent::Reply(minix_types::OK);
                }
        }

        if mask.is_empty() {
            return ReplyIntent::Reply(minix_types::OK);
        }

        if self.is_full() {
            return ReplyIntent::Reply(minix_types::ENOMEM);
        }

        let ep = table.procs[caller.get()].endpoint();
        self.subs[self.nsubs] = Some(Subscriber {
            endpoint: ep,
            mask,
            waiting: 0,
        });
        self.nsubs += 1;
        ReplyIntent::Reply(minix_types::OK)
    }

    /// `do_proc_event_reply` (`event.c:218-309`).
    ///
    ///   Seven checks (any failure → `ReplyLater` / `printf+SUSPEND`; only
    ///   `!PRIV_PROC → Reply(ENOSYS)`):
    /// 1. `!PRIV_PROC → ENOSYS`; 2. `pm_isokendpt(endpt)`; 3. `EVENT_CALL`;
    /// 4. cursor `< nsubs`; 5. `subs[i].endpoint == who_e`; 6. the flags infer
    /// an event; 7. `msg.event == inferred`.
    ///    Success path: `waiting--`, then `mask empty && waiting==0 → remove_sub`,
    ///    otherwise `cursor++ → resume_event`; every path returns `ReplyLater`
    ///    (this reply message is not answered, `main.c:88-89`).
    pub fn do_proc_event_reply(
        &mut self,
        msg: &Message,
        caller: UserSlot,
        table: &mut ProcTable,
        transport: &mut dyn IpcTransport,
        kern: &mut dyn crate::exit::KernelGateway,
    ) -> ReplyIntent {
        assert_eq!(self.nested, 0, "do_proc_event_reply: nested must be 0");

        // 1. Only system services may reply (event.c:232-233 → ENOSYS)
        // The same sign flip as C's `_syscall` contract (m_type<0 = errno).
        if !table.procs[caller.get()].is_kernel_process() {
            return ReplyIntent::Reply(-minix_types::ENOSYS);
        }

        // Decode m_pm_lsys_proc_event (event.c:240)
        let (endpt, reply_event) = {
            let pl = unsafe { msg.m_u.m_pm_lsys_proc_event };
            let ep = Endpoint(pl.endpt);
            let ev = match ProcEvent::from_bits(pl.event) {
                Some(v) => v,
                None => {
                    // Illegal event value (C fails the comparison against
                    // m_in.event after event inference → SUSPEND). Here it is
                    // treated as a SUSPEND precondition, not distinguished.
                    return ReplyIntent::ReplyLater;
                }
            };
            (ep, ev)
        };

        // 2. The endpoint must resolve (event.c:241-245 → SUSPEND)
        let slot = match table.pm_isokendpt(endpt) {
            Ok(s) => s,
            Err(_) => return ReplyIntent::ReplyLater,
        };

        // 3. EVENT_CALL is really set (event.c:247-251 → SUSPEND)
        if !table.procs[slot.get()].state.block.is_event_blocked() {
            return ReplyIntent::ReplyLater;
        }

        // 4. The cursor is valid (event.c:252-257 → SUSPEND)
        let cursor = match table.procs[slot.get()].state.block.event_cursor() {
            Some(c) => c,
            None => return ReplyIntent::ReplyLater,
        };
        if cursor.0 >= self.nsubs {
            return ReplyIntent::ReplyLater;
        }

        // 5. The replier is indeed the subscriber the cursor points at
        // (event.c:259-263 → SUSPEND)
        let sub_ep = self.subs[cursor.0].expect("subs prefix must be Some").endpoint;
        if sub_ep != table.procs[caller.get()].endpoint() {
            return ReplyIntent::ReplyLater;
        }

        // 6. Infer the event (event.c:265-273 → SUSPEND)
        let inferred = {
            let proc = &table.procs[slot.get()];
            if proc.is_exiting() {
                ProcEvent::Exit
            } else if proc.state.block.unpaused {
                ProcEvent::Signal
            } else {
                return ReplyIntent::ReplyLater;
            }
        };

        // 7. Event consistency (event.c:274-278 → SUSPEND)
        if reply_event != inferred {
            return ReplyIntent::ReplyLater;
        }

        // The mask's consistency with the event is not checked
        // (event.c:280-287)

        // waiting-- (event.c:289-290)
        {
            let sub = self.subs[cursor.0].as_mut().expect("must be Some");
            assert!(sub.waiting > 0, "waiting must be >0");
            sub.waiting -= 1;
        }

        // Final step: mask empty && waiting==0 → remove_sub, otherwise
        // cursor++ → resume (event.c:299-305)
        let should_remove = {
            let sub = self.subs[cursor.0].expect("must be Some");
            sub.mask.is_empty() && sub.waiting == 0
        };

        if should_remove {
            self.remove_sub(cursor.0, table, transport, kern);
        } else {
            // cursor++ → resume
            let next = EventCursor(cursor.0 + 1);
            table.procs[slot.get()].state.block.set_event_blocked(next);
            self.resume_event(slot, table, transport, kern);
        }

        ReplyIntent::ReplyLater
    }
}

#[cfg(test)]
mod tests {

    /// Mock kernel gateway for the event path (sys_clear/sys_kill always OK).
    struct MockKernelGateway;
    impl crate::exit::KernelGateway for MockKernelGateway {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            Ok(())
        }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    }
    use super::*;
    use crate::mproc::{Lifecycle, ProcTable};
    use minix_types::{Endpoint, Message, UserSlot, OK, EPERM, ENOMEM, ENOSYS, PROC_EVENT, PROC_EVENT_REPLY};

    fn mk_table_with_subscriber(mask: ProcEventMask, waiting: usize) -> (ProcTable, EventRegistry, UserSlot, UserSlot) {
        let mut table = ProcTable::new();
        // caller subscriber (slot 2) — PRIV_PROC
        let sub_slot = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        // target process (slot 5) — may be EXITING or UNPAUSED per test
        let tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].identity.id.pid = 42;

        let mut reg = EventRegistry::new();
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask, waiting });
        reg.nsubs = 1;

        (table, reg, sub_slot, tgt)
    }

    fn running_target(table: &mut ProcTable, slot: usize, exiting: bool, unpaused: bool) {
        let p = &mut table.procs[slot];
        if exiting {
            p.state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        } else {
            p.state.lifecycle = Lifecycle::Running;
        }
        p.state.block.unpaused = unpaused;
        p.identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        p.state.lifecycle = if exiting {
            Lifecycle::Exiting { exit_code: 0, sig_status: 0 }
        } else {
            Lifecycle::Running
        };
    }

    // ── do_proceventmask ──

    #[test]
    fn test_proceventmask_new_subscription() {
        let mut table = ProcTable::new();
        let caller = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());

        let mut reg = EventRegistry::new();
        let mask = ProcEventMask::EXIT | ProcEventMask::SIGNAL;
        let intent = reg.do_proceventmask_mut(caller, mask, &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(OK));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.mask(0), Some(mask));
    }

    #[test]
    fn test_proceventmask_eperm_for_user() {
        let mut table = ProcTable::new();
        let caller = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        // default Privilege::User (non-priv)
        let mut reg = EventRegistry::new();
        let intent = reg.do_proceventmask_mut(caller, ProcEventMask::EXIT, &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(EPERM));
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_proceventmask_update_existing() {
        let (mut table, mut reg, caller, _) = mk_table_with_subscriber(ProcEventMask::EXIT, 0);
        // update mask
        let intent = reg.do_proceventmask_mut(caller, ProcEventMask::SIGNAL, &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(OK));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.mask(0), Some(ProcEventMask::SIGNAL));
    }

    #[test]
    fn test_proceventmask_remove_when_idle() {
        let (mut table, mut reg, caller, _) = mk_table_with_subscriber(ProcEventMask::EXIT, 0);
        let intent = reg.do_proceventmask_mut(caller, ProcEventMask::empty(), &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(OK));
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_proceventmask_defer_remove_when_waiting() {
        let (mut table, mut reg, caller, _) = mk_table_with_subscriber(ProcEventMask::EXIT, 1);
        let intent = reg.do_proceventmask_mut(caller, ProcEventMask::empty(), &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(OK));
        assert_eq!(reg.len(), 1); // not removed yet
        assert_eq!(reg.mask(0), Some(ProcEventMask::empty()));
        assert_eq!(reg.waiting(0), Some(1));
    }

    #[test]
    fn test_proceventmask_empty_noop_for_unknown() {
        let mut table = ProcTable::new();
        let caller = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        let mut reg = EventRegistry::new();
        let intent = reg.do_proceventmask_mut(caller, ProcEventMask::empty(), &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(OK));
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_proceventmask_enomem_when_full() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        for i in 0..NR_SUBS {
            let _slot = UserSlot::new(i);
            table.procs[i].state.lifecycle = Lifecycle::Running;
            table.procs[i].identity.endpoint = Endpoint::from_generation_slot(1, i as i32);
            table.procs[i].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
            reg.subs[i] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, i as i32), mask: ProcEventMask::EXIT, waiting: 0 });
        }
        reg.nsubs = NR_SUBS;
        let caller = UserSlot::new(10);
        table.procs[10].state.lifecycle = Lifecycle::Running;
        table.procs[10].identity.endpoint = Endpoint::from_generation_slot(1, 10);
        table.procs[10].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        let intent = reg.do_proceventmask_mut(caller, ProcEventMask::EXIT, &mut table, &mut crate::TestIpcTransport::default(), &mut MockKernelGateway);
        assert_eq!(intent, ReplyIntent::Reply(ENOMEM));
        assert_eq!(reg.len(), NR_SUBS);
    }

    // ── publish / resume ──

    #[test]
    fn test_publish_no_subscriber_immediately_resumes() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        let tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        // make it EXITING to infer Exit event
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };

        reg.publish_event(tgt, &mut table, &mut transport, &mut kern);
        // No subscriber → immediately cleared + termination dispatch
        // (exit_restart)
        assert!(!table.procs[5].state.block.is_event_blocked());
        // exit_restart now really sends VM_EXIT (there is no subscriber
        // notification, so this is the only one)
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.sent()[0].0, Endpoint::VM);
        assert_eq!(transport.sent()[0].1.m_type, minix_types::VM_EXIT as i32);
    }

    #[test]
    fn test_publish_single_subscriber_sends() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        // subscriber
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.nsubs = 1;
        // target exiting
        let tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);

        reg.publish_event(tgt, &mut table, &mut transport, &mut kern);
        assert!(table.procs[5].state.block.is_event_blocked());
        assert_eq!(table.procs[5].state.block.event_cursor(), Some(EventCursor(0)));
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.sent()[0].0, Endpoint::from_generation_slot(1, 2));
        assert_eq!(transport.sent()[0].1.m_type, PROC_EVENT);
        assert_eq!(reg.waiting(0), Some(1));
    }

    #[test]
    fn test_publish_skips_non_matching() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        // subscriber only cares about SIGNAL, target is EXIT
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::SIGNAL, waiting: 0 });
        reg.nsubs = 1;
        let tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);

        reg.publish_event(tgt, &mut table, &mut transport, &mut kern);
        // No matching subscriber → immediately cleared + termination dispatch
        // (exit_restart)
        assert!(!table.procs[5].state.block.is_event_blocked());
        // The only wire send is exit_restart's VM_EXIT
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.sent()[0].0, Endpoint::VM);
    }

    #[test]
    fn test_resume_serializes_two_subscribers() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        // two subscribers both care about EXIT
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.subs[1] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 3), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.nsubs = 2;

        let tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        // also need privilege subscriber slots for reply handling
        for i in [2, 3] {
            table.procs[i].state.lifecycle = Lifecycle::Running;
            table.procs[i].identity.endpoint = Endpoint::from_generation_slot(1, i as i32);
            table.procs[i].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        }

        reg.publish_event(tgt, &mut table, &mut transport, &mut kern);
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.sent()[0].0, Endpoint::from_generation_slot(1, 2));
        assert_eq!(reg.waiting(0), Some(1));

        // first subscriber replies
        let mut reply = Message { m_type: PROC_EVENT_REPLY, ..Message::default() };
        reply.m_source = Endpoint::from_generation_slot(1, 2);
        reply.m_u.m_pm_lsys_proc_event.endpt = Endpoint::from_generation_slot(1, 5).get();
        reply.m_u.m_pm_lsys_proc_event.event = ProcEvent::Exit as u32;
        let caller = UserSlot::new(2);
        let intent = reg.do_proc_event_reply(&reply, caller, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // now should have sent to second subscriber
        assert_eq!(transport.sent().len(), 2);
        assert_eq!(transport.sent()[1].0, Endpoint::from_generation_slot(1, 3));
        assert_eq!(reg.waiting(0), Some(0));
        assert_eq!(reg.waiting(1), Some(1));
        assert_eq!(table.procs[5].state.block.event_cursor(), Some(EventCursor(1)));

        // second subscriber replies → done
        let mut reply2 = Message { m_type: PROC_EVENT_REPLY, ..Message::default() };
        reply2.m_source = Endpoint::from_generation_slot(1, 3);
        reply2.m_u.m_pm_lsys_proc_event.endpt = Endpoint::from_generation_slot(1, 5).get();
        reply2.m_u.m_pm_lsys_proc_event.event = ProcEvent::Exit as u32;
        let caller2 = UserSlot::new(3);
        let intent2 = reg.do_proc_event_reply(&reply2, caller2, &mut table, &mut transport, &mut kern);
        assert_eq!(intent2, ReplyIntent::ReplyLater);
        assert!(!table.procs[5].state.block.is_event_blocked());
        // Two PROC_EVENT notifications + the termination dispatch's VM_EXIT
        assert_eq!(transport.sent().len(), 3);
    }

    #[test]
    fn test_publish_cleans_dead_subscriber_on_exit() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        // subscriber is slot 2, endpoint 1:2
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.nsubs = 1;
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());

        // target is the subscriber itself, exiting (PRIV_PROC|EXITING)
        let tgt = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };

        reg.publish_event(tgt, &mut table, &mut transport, &mut kern);
        // The subscriber's own entry should have been removed before publish
        assert_eq!(reg.len(), 0);
        // No send (no subscriber left) → immediately cleared
        assert!(!table.procs[2].state.block.is_event_blocked());
    }

    // ── remove_sub cursor adjustment ──

    #[test]
    fn test_remove_sub_adjusts_future_cursor() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint(10), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.subs[1] = Some(Subscriber { endpoint: Endpoint(11), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.nsubs = 2;
        // process waiting on second subscriber (cursor 1)
        let _tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.block.set_event_blocked(EventCursor(1));

        reg.remove_sub(0, &mut table, &mut transport, &mut kern);
        assert_eq!(reg.len(), 1);
        assert_eq!(table.procs[5].state.block.event_cursor(), Some(EventCursor(0)));
    }

    #[test]
    fn test_remove_sub_resumes_waiting_on_removed() {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        // two subscribers, target waiting on first (cursor 0)
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::EXIT, waiting: 1 });
        reg.subs[1] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 3), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.nsubs = 2;
        let _tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.block.set_event_blocked(EventCursor(0));
        // subscriber slots for resume to send to next
        for i in [2, 3] {
            table.procs[i].state.lifecycle = Lifecycle::Running;
            table.procs[i].identity.endpoint = Endpoint::from_generation_slot(1, i as i32);
        }

        reg.remove_sub(0, &mut table, &mut transport, &mut kern);
        // Should have resumed and sent to next subscriber (original index 1 now 0)
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.sent()[0].0, Endpoint::from_generation_slot(1, 3));
        assert_eq!(reg.nested(), 0);
    }

    // ── do_proc_event_reply validation ──

    fn setup_reply_test() -> (ProcTable, EventRegistry, crate::TestIpcTransport, UserSlot, UserSlot) {
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let transport = crate::TestIpcTransport::default();
        // subscriber at subs[0]
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::EXIT, waiting: 1 });
        reg.nsubs = 1;
        // target exiting, blocked on cursor 0
        let tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.block.set_event_blocked(EventCursor(0));
        // subscriber process
        let sub = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        // another slot for bad endpoint test
        table.procs[6].state.lifecycle = Lifecycle::Running;
        table.procs[6].identity.endpoint = Endpoint::from_generation_slot(1, 6);

        (table, reg, transport, sub, tgt)
    }

    fn reply_msg(endpt: Endpoint, event: ProcEvent) -> Message {
        let mut m = Message { m_type: PROC_EVENT_REPLY, ..Message::default() };
        m.m_u.m_pm_lsys_proc_event.endpt = endpt.get();
        m.m_u.m_pm_lsys_proc_event.event = event as u32;
        m
    }

    #[test]
    fn test_reply_rejects_non_privileged_caller() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, _sub, tgt) = setup_reply_test();
        // make caller non-priv
        let caller = UserSlot::new(10);
        table.procs[10].state.lifecycle = Lifecycle::Running;
        table.procs[10].identity.endpoint = Endpoint::from_generation_slot(1, 10);
        // default privilege is User (non-kernel)
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, caller, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::Reply(-ENOSYS));
        // still blocked
        assert!(table.procs[tgt.get()].state.block.is_event_blocked());
    }

    #[test]
    fn test_reply_rejects_bad_endpoint() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, _) = setup_reply_test();
        let msg = reply_msg(Endpoint::from_generation_slot(9, 9), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_reply_rejects_not_event_blocked() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, tgt) = setup_reply_test();
        // clear block
        table.procs[tgt.get()].state.block.clear_event_blocked();
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // re-block for other tests not needed
    }

    #[test]
    fn test_reply_rejects_bad_cursor() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, tgt) = setup_reply_test();
        table.procs[tgt.get()].state.block.set_event_blocked(EventCursor(5)); // out of range
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_reply_rejects_wrong_subscriber() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, _sub, _) = setup_reply_test();
        // caller is not the subscriber at cursor
        let caller = UserSlot::new(3);
        table.procs[3].state.lifecycle = Lifecycle::Running;
        table.procs[3].identity.endpoint = Endpoint::from_generation_slot(1, 3);
        table.procs[3].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, caller, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_reply_rejects_bad_flags() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, tgt) = setup_reply_test();
        // make target neither EXITING nor UNPAUSED
        table.procs[tgt.get()].state.lifecycle = Lifecycle::Running;
        table.procs[tgt.get()].state.block.unpaused = false;
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_reply_rejects_event_mismatch() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, _) = setup_reply_test();
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Signal); // inferred is Exit
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_reply_ignores_mask_mismatch() {
        // mask empty but still should accept reply (event.c:280-287)
        let mut table = ProcTable::new();
        let mut reg = EventRegistry::new();
        let mut transport = crate::TestIpcTransport::default();
        let mut kern = MockKernelGateway;
        // subscriber with empty mask but waiting
        reg.subs[0] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 2), mask: ProcEventMask::empty(), waiting: 1 });
        reg.nsubs = 1;
        let _tgt = UserSlot::new(5);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.block.set_event_blocked(EventCursor(0));
        let sub = UserSlot::new(2);
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        table.procs[2].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());

        // Even though mask empty doesn't contain Exit, reply should still be accepted
        // Actually publish would have skipped empty mask, but if waiting, reply still accepted
        // Simulate that publish already sent (waiting=1) even though mask empty — in real
        // scenario this would not happen because publish skips non-matching. But the
        // "mask not checked" path is about leftover notifications after unsubscribe.
        // We test that do_proc_event_reply does not reject based on mask.
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // After reply, since mask empty && waiting==0 → removed
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_reply_advances_to_next_subscriber() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, _tgt) = setup_reply_test();
        // add second subscriber
        reg.subs[1] = Some(Subscriber { endpoint: Endpoint::from_generation_slot(1, 3), mask: ProcEventMask::EXIT, waiting: 0 });
        reg.nsubs = 2;
        table.procs[3].state.lifecycle = Lifecycle::Running;
        table.procs[3].identity.endpoint = Endpoint::from_generation_slot(1, 3);
        table.procs[3].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());

        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        assert_eq!(reg.waiting(0), Some(0));
        assert_eq!(reg.waiting(1), Some(1));
        assert_eq!(transport.sent().len(), 1);
    }

    #[test]
    fn test_reply_removes_when_mask_empty_and_no_waiting() {
        let mut kern = MockKernelGateway;
        let (mut table, mut reg, mut transport, sub, _) = setup_reply_test();
        // make mask empty
        reg.subs[0].as_mut().unwrap().mask = ProcEventMask::empty();
        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Exit);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_reply_signal_event_terminates_via_restart_sigs() {
        let mut kern = MockKernelGateway;
        // End to end: the last subscriber replies to a Signal event →
        // termination dispatch (event.c:122-123) → restart_sigs →
        // check_pending redelivers the pending SIGKILL → sig_proc's termination
        // path (sig_proc_exit → exit_proc).
        // The target process must be Running (restart_sigs's EXITING guard
        // would turn it into a no-op).
        let (mut table, mut reg, mut transport, sub, _tgt) = setup_reply_test();
        reg.subs[0].as_mut().unwrap().mask = ProcEventMask::SIGNAL;
        // The target becomes Running (so the Exiting precondition for Exit
        // dispatch does not apply) and stays blocked by EVENT_CALL on cursor 0;
        // the PM side stops it and suspends an unblocked SIGKILL. Event
        // inference (event.c:265-273) only infers a Signal event for a
        // non-Exiting target carrying the UNPAUSED flag.
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].state.block.unpaused = true;
        table.procs[5].state.block.stopped = true;
        table.procs[5].resources.signals.pending = 1u64 << (crate::signal::SIGKILL - 1);
        table.procs[5].resources.signals.mask = 0;

        let msg = reply_msg(Endpoint::from_generation_slot(1, 5), ProcEvent::Signal);
        let intent = reg.do_proc_event_reply(&msg, sub, &mut table, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // A subscriber with a non-empty mask keeps its subscription
        // (remove_sub only fires when the mask is empty and waiting=0,
        // event.c:130-161); the termination dispatch is triggered by the cursor
        // running past the end, not by subscriber removal.
        assert_eq!(reg.len(), 1);
        assert!(
            !table.procs[5].state.block.is_event_blocked(),
            "EVENT_CALL must be cleared before the termination dispatch"
        );
        assert_eq!(
            table.procs[5].resources.signals.pending, 0,
            "pending SIGKILL must have been redelivered (consumed)"
        );
        assert!(
            matches!(
                table.procs[5].state.lifecycle,
                Lifecycle::Exiting { .. } | Lifecycle::Zombie { .. }
            ),
            "SIGKILL redelivery must terminate the target, got {:?}",
            table.procs[5].state.lifecycle
        );
    }
}
