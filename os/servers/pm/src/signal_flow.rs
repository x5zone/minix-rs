//! Signal flow: delay, stop and resume (`stop_proc` / `try_resume_proc` /
//! `unpause` / `check_pending` / `restart_sigs` + `SIGSNDELAY`).
//!
//! C ground truth: `minix3/minix/servers/pm/signal.c:226-289` (stop/try_resume)
//! + `651-776` (check_pending/restart_sigs/unpause) + `344-369` (SIGSNDELAY)
//! Design: `.design/13-design.v1.md` D1–D8 (explicit `MayDelay`/`KernelStop`/`UnpauseOutcome`/etc).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, UserSlot};
use crate::mproc::{ProcTable, Lifecycle, BlockState, IpcBlockReason};

/// `may_delay` as explicit enum (D1: `bool` → `MayDelay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MayDelay {
    MustStop,
    MayDefer,
}

/// Result of `stop_proc` (D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    Stopped,
    Deferred,
}

/// Kernel stop abstraction (`sys_delay_stop`, `signal.c:239`, D1/D3).
pub trait KernelStop {
    /// `sys_delay_stop(endpoint)` → `OK` (stopped) / `EBUSY` (deferred) / other (panic).
    fn delay_stop(&mut self, ep: Endpoint) -> i32;
}

/// Kernel resume abstraction (`sys_resume`, `signal.c:282`, D2).
pub trait KernelResume {
    fn resume(&mut self, ep: Endpoint) -> i32;
}

/// VFS unpause abstraction (`tell_vfs(VFS_PM_UNPAUSE)`, `signal.c:767`, D3).
pub trait VfsCtl {
    fn tell_unpause(&mut self, ep: Endpoint);
}

/// `unpause` outcome (D3: `bool` → `UnpauseOutcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnpauseOutcome {
    Ready,
    Busy,
    VfsWait,
}

/// `check_pending` outcome (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckPendingOutcome {
    Completed,
    BrokenOnVfs,
}

/// `restart_sigs` action (D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartAction {
    Noop,
    Exit(i8),
    CheckAndResume,
}

/// Deliver signal for `check_pending` (injected for testability).
pub trait SignalDeliver {
    fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, signo: i32, ksig: bool);
}

/// Exit handler for `restart_sigs` (injected).
pub trait ExitHandler {
    fn exit_proc(&mut self, table: &mut ProcTable, target: UserSlot, status: i8);
}

/// Handles `SIGSNDELAY` delay token (`signal.c:344-369`, D6).
pub fn handle_sigsn_delay(
    table: &mut ProcTable,
    slot: usize,
    kstop: &mut dyn KernelStop,
    deliver: &mut dyn SignalDeliver,
) -> bool {
    const SIGSNDELAY: i32 = 70;
    let _ = SIGSNDELAY;
    // In C this is called only when signo==SIGSNDELAY && DELAY_CALL.
    // Here we expose as explicit check: if slot has DELAY_CALL, clear and handle.
    let is_delayed = matches!(
        table.procs[slot].state.block.ipc_blocked,
        Some(IpcBlockReason::DelayedSignal)
    );
    if !is_delayed {
        return false;
    }
    // Clear DELAY_CALL (351) + assert !PROC_STOPPED (353)
    table.procs[slot].state.block.ipc_blocked = None;
    assert!(!table.procs[slot].state.block.stopped, "SIGSNDELAY: DELAY without PROC_STOPPED assert");
    // If VFS|EVENT → stop_proc(FALSE) → return (359-363)
    let is_vfs_or_event = table.procs[slot].state.block.is_vfs_blocked()
        || table.procs[slot].state.block.is_event_blocked();
    if is_vfs_or_event {
        let _ = stop_proc(table, UserSlot::new(slot), MayDelay::MustStop, kstop);
        return true;
    }
    // Else check_pending (366)
    let _ = check_pending(table, UserSlot::new(slot), deliver);
    assert!(!matches!(
        table.procs[slot].state.block.ipc_blocked,
        Some(IpcBlockReason::DelayedSignal)
    ));
    true
}

/// Tries to stop the process (`stop_proc`, `signal.c:226-261`, D1).
///
/// `assert(!(PROC_STOPPED|DELAY|UNPAUSED))` (237) before `sys_delay_stop`.
pub fn stop_proc(
    table: &mut ProcTable,
    target: UserSlot,
    may_delay: MayDelay,
    kstop: &mut dyn KernelStop,
) -> Result<StopOutcome, &'static str> {
    let proc = &table.procs[target.get()];
    assert!(
        !proc.state.block.stopped
            && !matches!(proc.state.block.ipc_blocked, Some(IpcBlockReason::DelayedSignal))
            && !proc.state.block.unpaused,
        "stop_proc: !(PROC_STOPPED|DELAY|UNPAUSED)"
    );
    let ep = proc.endpoint();
    let r = kstop.delay_stop(ep);
    const OK: i32 = 0;
    const EBUSY: i32 = 16; // minix errno.h EBUSY
    match r {
        x if x == OK => {
            table.procs[target.get()].state.block.stopped = true;
            Ok(StopOutcome::Stopped)
        }
        x if x == EBUSY => {
            if may_delay == MayDelay::MustStop {
                panic!("stop_proc: unexpected delay call");
            }
            table.procs[target.get()].state.block.ipc_blocked = Some(IpcBlockReason::DelayedSignal);
            Ok(StopOutcome::Deferred)
        }
        _ => panic!("sys_delay_stop failed: {}", r),
    }
}

/// Tries to resume (`try_resume_proc`, `signal.c:266-289`, D2).
///
/// Returns `true` if resumed, `false` if guarded.
pub fn try_resume_proc(
    table: &mut ProcTable,
    target: UserSlot,
    kres: &mut dyn KernelResume,
) -> bool {
    let proc = &table.procs[target.get()];
    assert!(proc.state.block.stopped, "try_resume_proc: PROC_STOPPED");
    if proc.state.block.is_vfs_blocked()
        || proc.state.block.is_event_blocked()
        || proc.state.lifecycle.is_exiting()
    {
        return false;
    }
    let ep = proc.endpoint();
    let r = kres.resume(ep);
    const OK: i32 = 0;
    if r != OK {
        panic!("sys_resume failed: {}", r);
    }
    table.procs[target.get()].state.block.stopped = false;
    table.procs[target.get()].state.block.unpaused = false;
    true
}

/// Unpauses for `sig_proc` caught path (`unpause`, `signal.c:719-770`, D3).
///
/// `assert(!(VFS|EVENT))` (731) + three branches.
pub fn unpause(
    table: &mut ProcTable,
    target: UserSlot,
    kstop: &mut dyn KernelStop,
    vfs: &mut dyn VfsCtl,
) -> UnpauseOutcome {
    let proc = &table.procs[target.get()];
    assert!(
        !proc.state.block.is_vfs_blocked() && !proc.state.block.is_event_blocked(),
        "unpause: !(VFS|EVENT)"
    );
    if proc.state.block.unpaused {
        assert!(
            proc.state.block.stopped
                && !matches!(
                    proc.state.block.ipc_blocked,
                    Some(IpcBlockReason::DelayedSignal)
                ),
            "unpause UNPAUSED must be (DELAY|PROC)==PROC"
        );
        return UnpauseOutcome::Ready;
    }
    if matches!(
        proc.state.block.ipc_blocked,
        Some(IpcBlockReason::DelayedSignal)
    ) {
        return UnpauseOutcome::Busy;
    }
    if proc.state.wait.waiting || proc.resources.signals.suspended {
        // `WAITING|SIGSUSPENDED` → stop(MustStop) → Ready (745-753)
        let _ = stop_proc(table, target, MayDelay::MustStop, kstop).expect("MustStop should not defer");
        return UnpauseOutcome::Ready;
    }
    // VFS blocked path (760-769)
    if !proc.state.block.stopped {
        match stop_proc(table, target, MayDelay::MayDefer, kstop) {
            Ok(StopOutcome::Deferred) => return UnpauseOutcome::Busy,
            Ok(StopOutcome::Stopped) => {}
            Err(_) => return UnpauseOutcome::Busy,
        }
    }
    let ep = table.procs[target.get()].endpoint();
    vfs.tell_unpause(ep);
    UnpauseOutcome::VfsWait
}

/// Checks pending unblocked signals (`check_pending`, `signal.c:651-682`, D4).
pub fn check_pending(
    table: &mut ProcTable,
    target: UserSlot,
    deliver: &mut dyn SignalDeliver,
) -> CheckPendingOutcome {
    loop {
        // Find smallest pending & !mask
        let (signo, ksig) = {
            let state = &table.procs[target.get()].resources.signals;
            let unblocked = state.pending & !state.mask;
            if unblocked == 0 {
                break;
            }
            let signo = unblocked.trailing_zeros() + 1;
            let ksig = (state.kernel_pending >> (signo - 1) & 1) != 0;
            (signo as i32, ksig)
        };
        // Take pending (668-669)
        {
            let state = &mut table.procs[target.get()].resources.signals;
            let b = 1u64 << (signo as u64 - 1);
            state.pending &= !b;
            state.kernel_pending &= !b;
        }
        // Deliver with trace==FALSE (670)
        deliver.sig_proc(table, target, signo, ksig);
        // If VFS|EVENT → break (672-679)
        let proc = &table.procs[target.get()];
        if proc.state.block.is_vfs_blocked() || proc.state.block.is_event_blocked() {
            assert!(proc.state.block.stopped, "check_pending VFS|EVENT must be PROC_STOPPED");
            return CheckPendingOutcome::BrokenOnVfs;
        }
    }
    CheckPendingOutcome::Completed
}

/// `restart_sigs` 三个注入点的合并 trait（D5，2026-09-06 收敛）。
///
/// 原设计按 C 的三个模块级调用（`sys_resume`/`exit_proc`/`sig_proc`）拆成
/// [`KernelResume`]/[`ExitHandler`]/[`SignalDeliver`] 三个独立 trait；生产
/// 装配（`event.rs` 的事件终止分派）中三者共享同一 `transport`——三个独立
/// 的 `&mut` 借用无法共存。合并为一个 supertrait，[`restart_sigs`] 函数体
/// 内经 trait 上转（upcasting）按需取用；三个成员 trait 保留不动，测试仍
/// 可单独注入 `check_pending`/`stop_proc` 等其他消费者。
pub trait RestartServices: KernelResume + ExitHandler + SignalDeliver {}

/// Restarts signal work after VFS reply (`restart_sigs`, `signal.c:687-714`, D5).
pub fn restart_sigs(
    table: &mut ProcTable,
    target: UserSlot,
    svc: &mut dyn RestartServices,
) -> RestartAction {
    let proc = &table.procs[target.get()];
    if proc.state.block.is_vfs_blocked()
        || proc.state.block.is_event_blocked()
        || proc.state.lifecycle.is_exiting()
    {
        return RestartAction::Noop;
    }
    // Check TRACE_EXIT first (695-698) — tracer 强制退出：以缓存的退出码
    // 走 exit_proc（dump_core=FALSE，C `signal.c:697`）。
    if table.procs[target.get()].state.trace.exit_pending {
        let status = table.procs[target.get()].state.lifecycle.exit_code().map(|(c,_)| c).unwrap_or(0);
        svc.exit_proc(table, target, status);
        return RestartAction::Exit(status);
    }
    if proc.state.block.stopped {
        assert!(
            !matches!(
                proc.state.block.ipc_blocked,
                Some(IpcBlockReason::DelayedSignal)
            ),
            "restart_sigs: !DELAY_CALL"
        );
        let _ = check_pending(table, target, svc);
        let _ = try_resume_proc(table, target, svc);
        return RestartAction::CheckAndResume;
    }
    RestartAction::Noop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials, SignalState};
    use minix_types::{Endpoint, UserSlot};

    fn mk_running(table: &mut ProcTable, slot: usize) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        table.procs[slot].state.block = BlockState::default();
        table.procs[slot].state.wait.waiting = false;
        table.procs[slot].resources.signals.suspended = false;
        table.procs[slot].state.trace.exit_pending = false;
    }

    struct OkStop;
    impl KernelStop for OkStop {
        fn delay_stop(&mut self, _ep: Endpoint) -> i32 { 0 }
    }
    struct BusyStop;
    impl KernelStop for BusyStop {
        fn delay_stop(&mut self, _ep: Endpoint) -> i32 { 16 }
    }
    struct FailStop;
    impl KernelStop for FailStop {
        fn delay_stop(&mut self, _ep: Endpoint) -> i32 { 99 }
    }
    struct OkRes;
    impl KernelResume for OkRes {
        fn resume(&mut self, _ep: Endpoint) -> i32 { 0 }
    }
    struct NoopVfs;
    impl VfsCtl for NoopVfs {
        fn tell_unpause(&mut self, _ep: Endpoint) {}
    }
    struct CountingVfs { cnt: usize }
    impl VfsCtl for CountingVfs {
        fn tell_unpause(&mut self, _ep: Endpoint) { self.cnt += 1; }
    }
    struct NoopDeliver;
    impl SignalDeliver for NoopDeliver {
        fn sig_proc(&mut self, _t: &mut ProcTable, _tr: UserSlot, _s: i32, _k: bool) {}
    }
    struct VfsDeliver;
    impl SignalDeliver for VfsDeliver {
        fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, _s: i32, _k: bool) {
            // Simulate sig_proc causing VFS block + PROC_STOPPED
            table.procs[target.get()].state.block.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
            table.procs[target.get()].state.block.stopped = true;
        }
    }
    struct NoopExit;
    impl ExitHandler for NoopExit {
        fn exit_proc(&mut self, _t: &mut ProcTable, _tr: UserSlot, _s: i8) {}
    }
    struct RecExit { called: bool, status: i8 }
    impl ExitHandler for RecExit {
        fn exit_proc(&mut self, _t: &mut ProcTable, _tr: UserSlot, s: i8) { self.called = true; self.status = s; }
    }

    #[test]
    fn test_stop_proc_ok_stops() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        let mut k = OkStop;
        let r = stop_proc(&mut table, UserSlot::new(5), MayDelay::MustStop, &mut k).unwrap();
        assert_eq!(r, StopOutcome::Stopped);
        assert!(table.procs[5].state.block.stopped);
    }

    #[test]
    #[should_panic(expected = "unexpected delay")]
    fn test_stop_proc_ebusy_must_panic() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        let mut k = BusyStop;
        let _ = stop_proc(&mut table, UserSlot::new(5), MayDelay::MustStop, &mut k);
    }

    #[test]
    fn test_stop_proc_ebusy_may_defer() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        let mut k = BusyStop;
        let r = stop_proc(&mut table, UserSlot::new(5), MayDelay::MayDefer, &mut k).unwrap();
        assert_eq!(r, StopOutcome::Deferred);
        assert!(matches!(table.procs[5].state.block.ipc_blocked, Some(IpcBlockReason::DelayedSignal)));
    }

    #[test]
    #[should_panic(expected = "sys_delay_stop failed")]
    fn test_stop_proc_unexpected_panics() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        let mut k = FailStop;
        let _ = stop_proc(&mut table, UserSlot::new(5), MayDelay::MayDefer, &mut k);
    }

    #[test]
    fn test_try_resume_noop_when_vfs() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut k = OkRes;
        assert!(!try_resume_proc(&mut table, UserSlot::new(5), &mut k));
        assert!(table.procs[5].state.block.stopped);
    }

    #[test]
    fn test_try_resume_clears_stopped_and_unpaused() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.unpaused = true;
        let mut k = OkRes;
        assert!(try_resume_proc(&mut table, UserSlot::new(5), &mut k));
        assert!(!table.procs[5].state.block.stopped);
        assert!(!table.procs[5].state.block.unpaused);
    }

    #[test]
    fn test_unpause_ready_when_unpaused() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.unpaused = true;
        let mut k = OkStop;
        let mut v = NoopVfs;
        assert_eq!(unpause(&mut table, UserSlot::new(5), &mut k, &mut v), UnpauseOutcome::Ready);
    }

    #[test]
    fn test_unpause_busy_when_delay() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.ipc_blocked = Some(IpcBlockReason::DelayedSignal);
        let mut k = OkStop;
        let mut v = NoopVfs;
        assert_eq!(unpause(&mut table, UserSlot::new(5), &mut k, &mut v), UnpauseOutcome::Busy);
    }

    #[test]
    fn test_unpause_ready_when_waiting() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.wait.waiting = true;
        let mut k = OkStop;
        let mut v = NoopVfs;
        assert_eq!(unpause(&mut table, UserSlot::new(5), &mut k, &mut v), UnpauseOutcome::Ready);
        assert!(table.procs[5].state.block.stopped);
    }

    #[test]
    fn test_unpause_vfs_wait_when_not_stopped() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.stopped = false;
        let mut k = OkStop;
        let mut v = CountingVfs { cnt: 0 };
        let r = unpause(&mut table, UserSlot::new(5), &mut k, &mut v);
        assert_eq!(r, UnpauseOutcome::VfsWait);
        assert_eq!(v.cnt, 1);
        assert!(table.procs[5].state.block.stopped);
    }

    #[test]
    fn test_check_pending_single_delivers() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].resources.signals.pending = 1u64 << 2;
        table.procs[5].resources.signals.mask = 0;
        let mut d = NoopDeliver;
        let r = check_pending(&mut table, UserSlot::new(5), &mut d);
        assert_eq!(r, CheckPendingOutcome::Completed);
        assert_eq!(table.procs[5].resources.signals.pending, 0);
    }

    #[test]
    fn test_check_pending_breaks_on_vfs() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].resources.signals.pending = (1u64 << 2) | (1u64 << 3);
        table.procs[5].resources.signals.mask = 0;
        let mut d = VfsDeliver;
        let r = check_pending(&mut table, UserSlot::new(5), &mut d);
        assert_eq!(r, CheckPendingOutcome::BrokenOnVfs);
        // first pending cleared, second remains because break after first VFS
        assert_eq!(table.procs[5].resources.signals.pending, 1u64 << 3);
        assert!(table.procs[5].state.block.stopped);
    }

    #[test]
    fn test_check_pending_ksig_restored() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].resources.signals.pending = 1u64 << 4;
        table.procs[5].resources.signals.kernel_pending = 1u64 << 4;
        table.procs[5].resources.signals.mask = 0;
        struct KsigRec { ksig: Option<bool> }
        impl SignalDeliver for KsigRec {
            fn sig_proc(&mut self, _t: &mut ProcTable, _tr: UserSlot, _s: i32, k: bool) { self.ksig = Some(k); }
        }
        let mut r = KsigRec { ksig: None };
        check_pending(&mut table, UserSlot::new(5), &mut r);
        assert_eq!(r.ksig, Some(true));
        assert_eq!(table.procs[5].resources.signals.kernel_pending, 0);
    }

    /// `restart_sigs` 的合并注入 mock（D5 收敛后单一 service 对象）：
    /// 记录 exit 调用、转发 deliver 计数、kernel resume 恒 OK。
    struct RestartMock {
        exit_called: bool,
        exit_status: i8,
        delivered: usize,
        resume_ok: bool,
    }
    impl RestartMock {
        fn new() -> Self {
            Self { exit_called: false, exit_status: 0, delivered: 0, resume_ok: true }
        }
    }
    impl KernelResume for RestartMock {
        fn resume(&mut self, _ep: Endpoint) -> i32 {
            if self.resume_ok { 0 } else { -5 }
        }
    }
    impl ExitHandler for RestartMock {
        fn exit_proc(&mut self, _t: &mut ProcTable, _tr: UserSlot, status: i8) {
            self.exit_called = true;
            self.exit_status = status;
        }
    }
    impl SignalDeliver for RestartMock {
        fn sig_proc(&mut self, _t: &mut ProcTable, _tr: UserSlot, _s: i32, _k: bool) {
            self.delivered += 1;
        }
    }
    impl RestartServices for RestartMock {}

    #[test]
    fn test_restart_sigs_noop_when_vfs() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut svc = RestartMock::new();
        assert_eq!(restart_sigs(&mut table, UserSlot::new(5), &mut svc), RestartAction::Noop);
    }

    #[test]
    fn test_restart_sigs_trace_exit_first() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.trace.exit_pending = true;
        // TRACE_EXIT is independent of EXITING; C checks TRACE_EXIT after guard
        // but before PROC_STOPPED branch. Keep lifecycle Running to avoid EXITING guard.
        let mut svc = RestartMock::new();
        let r = restart_sigs(&mut table, UserSlot::new(5), &mut svc);
        assert_eq!(r, RestartAction::Exit(0));
        assert!(svc.exit_called);
    }

    #[test]
    fn test_restart_sigs_check_and_resume() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.stopped = true;
        table.procs[5].resources.signals.pending = 1u64 << 2;
        table.procs[5].resources.signals.mask = 0;
        let mut svc = RestartMock::new();
        let r = restart_sigs(&mut table, UserSlot::new(5), &mut svc);
        assert_eq!(r, RestartAction::CheckAndResume);
        assert!(!table.procs[5].state.block.stopped);
        assert_eq!(svc.delivered, 1, "pending signal must be redelivered");
    }

    #[test]
    fn test_sigsn_delay_clears_and_checks() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 5);
        table.procs[5].state.block.ipc_blocked = Some(IpcBlockReason::DelayedSignal);
        table.procs[5].resources.signals.pending = 1u64 << 2;
        table.procs[5].resources.signals.mask = 0;
        let mut k = OkStop;
        let mut d = NoopDeliver;
        let handled = handle_sigsn_delay(&mut table, 5, &mut k, &mut d);
        assert!(handled);
        assert!(!matches!(table.procs[5].state.block.ipc_blocked, Some(IpcBlockReason::DelayedSignal)));
        assert_eq!(table.procs[5].resources.signals.pending, 0);
    }

    #[test]
    fn test_next_unblocked_smallest() {
        let mut s = SignalState::default();
        s.pending = (1u64 << 5) | (1u64 << 2);
        s.mask = 0;
        let nxt = s.next_unblocked().unwrap();
        assert_eq!(nxt.0, 3); // 1<<2 → signo 3 smallest
    }

    #[test]
    fn test_take_pending_clears_both() {
        let mut s = SignalState::default();
        s.pending = 1u64 << 4;
        s.kernel_pending = 1u64 << 4;
        s.take_pending(5);
        assert_eq!(s.pending, 0);
        assert_eq!(s.kernel_pending, 0);
    }
}
