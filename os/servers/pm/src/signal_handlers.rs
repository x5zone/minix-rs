//! Signal handler installation and mask semantics (`do_sigaction` / `do_sigpending`
//! / `do_sigprocmask` / `do_sigsuspend` / `do_sigreturn` + `sig_send`).
//!
//! C ground truth: `minix3/minix/servers/pm/signal.c:40-192` (do_sigaction et al)
//! + `minix3/minix/servers/pm/signal.c:776-855` (`sig_send`)
//! Design: `.design/12-design.v1.md` D1–D8 (explicit `SigHandler`/`SigMaskOp`/
//! `without_unkillable`/`KernelSig`).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, VirBytes, UserSlot, EINVAL, EFAULT, ENOMEM};
use crate::mproc::{
    ProcTable, SigSet, SigHandler, SigMaskOp, MaskOpEffect, SigMsg,
    SigAction, SIGKILL, _NSIG, UNKILLABLE_MASK,
    SigSetExt, SIG_BLOCK, SIG_UNBLOCK,
};
use crate::ipc::ReplyIntent;

/// Errors for `handle_sigaction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigActionError {
    InvalidSignal,
    Fault,
}

impl SigActionError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidSignal => EINVAL,
            Self::Fault => EFAULT,
        }
    }
}

/// Errors for `handle_sigprocmask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigMaskError {
    InvalidHow,
}

impl SigMaskError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidHow => EINVAL,
        }
    }
}

/// Errors for `sig_send` kernel delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigSendError {
    FaultOrNoMem,
    Unexpected(i32),
}

/// Errors for `handle_sigreturn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigReturnError {
    Fault(i32),
}

/// Post-action after successful `sig_send` (`signal.c:832-851` D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostAction {
    /// `WAITING|SIGSUSPENDED` — interrupted, caller should `reply(EINTR)` + `try_resume`.
    InterruptedWait,
    /// Otherwise `UNPAUSED` — VFS already confirmed, will `restart_sigs` later.
    AwaitVfsUnpause,
}

/// Kernel signal delivery abstraction (ARCH A-3: hardware behind trait).
pub trait KernelSig {
    /// `sys_sigsend(endpoint, &sigmsg)` — `signal.c:818`.
    fn sigsend(&mut self, endpoint: Endpoint, msg: &SigMsg) -> Result<(), i32>;
    /// `sys_sigreturn(endpoint, ctx)` — `signal.c:189`.
    fn sigreturn(&mut self, endpoint: Endpoint, ctx: VirBytes) -> Result<(), i32>;
}

/// Request for `handle_sigaction` (D1: `Option<SigAction>` replaces `vir_bytes act==0`).
#[derive(Debug, Clone, Copy)]
pub struct SigActionReq {
    /// Signal number (1.._NSIG-1, SIGKILL 9 special).
    pub signo: i32,
    /// New action (`None` = `act==0` read-only, `Some` = `sys_datacopy` new svec).
    pub act: Option<SigAction>,
    /// Whether old action should be returned (`oact != 0`).
    pub need_oact: bool,
    /// `__sigreturn` trampoline (`m_lc_pm_sig.ret`).
    pub sigreturn: VirBytes,
}

/// Handles `PM_SIGACTION` (`do_sigaction`, `signal.c:40-86`, D1/D2/D3).
///
/// Returns `Ok(Some(old))` if `need_oact` and old was copied, `Ok(None)` otherwise.
/// `SIGKILL` early returns `Ok(None)` (C `49` `return OK` before `oact` copy).
pub fn handle_sigaction(
    table: &mut ProcTable,
    caller: UserSlot,
    req: SigActionReq,
) -> Result<Option<SigAction>, SigActionError> {
    let signo = req.signo;
    // D1: SIGKILL early return (49) before oact handling — per C order.
    if signo == SIGKILL {
        return Ok(None);
    }
    if signo < 1 || signo >= _NSIG as i32 {
        return Err(SigActionError::InvalidSignal);
    }

    // oact handling: clone old action if requested (53-57).
    let old = if req.need_oact {
        let idx = (signo - 1) as usize;
        Some(table.procs[caller.get()].resources.signals.actions[idx])
    } else {
        None
    };

    // act==0 read-only (59-60).
    let Some(new_act) = req.act else {
        return Ok(old);
    };

    // Three-way handler mapping (67-78 D2) — handler 0=DFL 1=IGN else Catch.
    let handler = SigHandler::from_raw(new_act.sa_handler);
    // Install atomically (D2: four-bitmap transaction, D3: mask stripping inside install).
    let ok = table.procs[caller.get()].resources.signals.install(
        signo as u32,
        handler,
        new_act.sa_mask,
        new_act.sa_flags,
        req.sigreturn,
    );
    if !ok {
        return Err(SigActionError::InvalidSignal);
    }
    // Ensure sigreturn trampoline is stored (install already did).
    Ok(old)
}

/// Handles `PM_SIGPENDING` (`do_sigpending`, `signal.c:88-97`).
///
/// Returns snapshot of `pending` only (not `ksigpending`/`mask`).
pub fn handle_sigpending(table: &ProcTable, caller: UserSlot) -> SigSet {
    // In C: assert(!(mp->mp_flags & (PROC_STOPPED|VFS_CALL|UNPAUSED|EVENT_CALL))) (93)
    // For 12 we document the precondition but do not panic — single-threaded caller
    // must not be in those states when invoking sigpending (13 will enforce).
    table.procs[caller.get()].resources.signals.pending_snapshot()
}

/// Handles `PM_SIGPROCMASK` (`do_sigprocmask`, `signal.c:99-155`, D4).
///
/// Returns `(old_mask, effect)` — old mask for reply (`120`), effect for caller
/// to decide `check_pending` (`137/144`).
pub fn handle_sigprocmask(
    table: &mut ProcTable,
    caller: UserSlot,
    how: i32,
    set: SigSet,
) -> Result<(SigSet, MaskOpEffect), SigMaskError> {
    let old = table.procs[caller.get()].resources.signals.mask;
    let op = SigMaskOp::try_from_how(how).ok_or(SigMaskError::InvalidHow)?;
    let effect = table.procs[caller.get()].resources.signals.apply_mask_op(op, set);
    Ok((old, effect))
}

/// Handles `PM_SIGSUSPEND` (`do_sigsuspend`, `signal.c:157-171`, D5).
///
/// Saves `mask→mask_saved`, installs new mask, sets `suspended`, then
/// effect `needs_check` is for caller to `check_pending`. Always `ReplyLater`.
pub fn handle_sigsuspend(
    table: &mut ProcTable,
    caller: UserSlot,
    set: SigSet,
) -> ReplyIntent {
    table.procs[caller.get()].resources.signals.prepare_suspend(set);
    // Caller should `check_pending` after this; if pending became runnable,
    // sig_send will run synchronously. Otherwise caller stays SUSPEND.
    ReplyIntent::ReplyLater
}

/// Handles `PM_SIGRETURN` (`do_sigreturn`, `signal.c:173-192`, D6).
pub fn handle_sigreturn(
    table: &mut ProcTable,
    caller: UserSlot,
    set: SigSet,
    ctx: VirBytes,
    kernel: &mut dyn KernelSig,
) -> Result<(), SigReturnError> {
    table.procs[caller.get()].resources.signals.restore_for_sigreturn(set);
    let ep = table.procs[caller.get()].endpoint();
    let r = kernel.sigreturn(ep, ctx);
    // check_pending unconditionally after sigreturn (190), even on fault.
    // In Rust the caller triggers check_pending via MaskOpEffect; we model
    // sigreturn as always needing check (consistent with 190).
    match r {
        Ok(()) => Ok(()),
        Err(code) => Err(SigReturnError::Fault(code)),
    }
}

/// Sends signal via handler (`sig_send`, `signal.c:776-855`, D7/D8).
///
/// Requires `PROC_STOPPED` (`787` assert). Returns `PostAction` for caller
/// to `reply(EINTR)` + `try_resume` vs `assert(unpaused)`.
pub fn sig_send(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    kernel: &mut dyn KernelSig,
) -> Result<PostAction, SigSendError> {
    // Precondition: PROC_STOPPED (787). In Rust `BlockState.stopped` must be true.
    assert!(
        table.procs[target.get()].state.block.stopped,
        "sig_send requires PROC_STOPPED"
    );
    if signo < 1 || signo >= _NSIG as i32 {
        return Err(SigSendError::Unexpected(EINVAL));
    }
    // Prepare sigmsg (D7: four-step mask evolution + RESETHAND + pending clear).
    let msg = {
        let sig_state = &mut table.procs[target.get()].resources.signals;
        sig_state
            .prepare_sigmsg(signo as u32)
            .ok_or(SigSendError::Unexpected(EINVAL))?
    };

    let ep = table.procs[target.get()].endpoint();
    match kernel.sigsend(ep, &msg) {
        Ok(()) => {}
        Err(code) if code == EFAULT || code == ENOMEM => {
            return Err(SigSendError::FaultOrNoMem);
        }
        Err(code) => {
            panic!("sys_sigsend failed: {}", code);
        }
    }

    // Post-branch (832-851 D8): WAITING|SIGSUSPENDED vs UNPAUSED.
    let proc = &mut table.procs[target.get()];
    let is_waiting = proc.state.wait.waiting;
    let is_suspended = proc.resources.signals.suspended;
    if is_waiting || is_suspended {
        // In C: mp_flags &= ~(WAITING|SIGSUSPENDED) + reply(slot, EINTR) + try_resume_proc
        // Here we clear and return InterruptedWait so caller can reply(EINTR) + try_resume.
        proc.state.wait.waiting = false;
        proc.resources.signals.suspended = false;
        // BlockState.stopped remains true until try_resume clears it (13 will do).
        // For 12's unit test we model that the handler delivery will resume;
        // caller should `try_resume` (clear stopped if not VFS/EVENT).
        Ok(PostAction::InterruptedWait)
    } else {
        // Must be UNPAUSED (VFS confirmed unpause, 845-851). In C: assert(UNPAUSED).
        assert!(
            proc.state.block.unpaused,
            "sig_send non-waiting target must be UNPAUSED"
        );
        Ok(PostAction::AwaitVfsUnpause)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, VirBytes, UserSlot, EINVAL};

    fn mk_proc(table: &mut ProcTable, slot: usize, pid: i32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        table.procs[slot].state.block.stopped = false;
        table.procs[slot].state.block.unpaused = false;
        table.procs[slot].state.wait.waiting = false;
        table.procs[slot].resources.signals.suspended = false;
    }

    struct OkKernel;
    impl KernelSig for OkKernel {
        fn sigsend(&mut self, _ep: Endpoint, _msg: &SigMsg) -> Result<(), i32> { Ok(()) }
        fn sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> { Ok(()) }
    }
    struct FaultKernel;
    impl KernelSig for FaultKernel {
        fn sigsend(&mut self, _ep: Endpoint, _msg: &SigMsg) -> Result<(), i32> { Err(EFAULT) }
        fn sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> { Err(EFAULT) }
    }
    struct UnexpectedKernel;
    impl KernelSig for UnexpectedKernel {
        fn sigsend(&mut self, _ep: Endpoint, _msg: &SigMsg) -> Result<(), i32> { Err(99) }
        fn sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> { Err(99) }
    }

    #[test]
    fn test_sigaction_kill_returns_ok() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        let req = SigActionReq { signo: SIGKILL, act: Some(SigAction { sa_handler: 1, sa_mask: 0, sa_flags: 0 }), need_oact: true, sigreturn: VirBytes(0x100) };
        let res = handle_sigaction(&mut table, UserSlot::new(0), req);
        assert!(res.is_ok());
        assert!(res.unwrap().is_none()); // early return before oact copy per C 49
    }

    #[test]
    fn test_sigaction_invalid_signal() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        let req = SigActionReq { signo: 0, act: None, need_oact: false, sigreturn: VirBytes(0) };
        assert_eq!(handle_sigaction(&mut table, UserSlot::new(0), req).unwrap_err(), SigActionError::InvalidSignal);
        let req2 = SigActionReq { signo: 64, act: None, need_oact: false, sigreturn: VirBytes(0) };
        assert_eq!(handle_sigaction(&mut table, UserSlot::new(0), req2).unwrap_err(), SigActionError::InvalidSignal);
    }

    #[test]
    fn test_sigaction_oact_only_reads() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        table.procs[0].resources.signals.actions[2].sa_handler = 0x2000;
        let req = SigActionReq { signo: 3, act: None, need_oact: true, sigreturn: VirBytes(0) };
        let old = handle_sigaction(&mut table, UserSlot::new(0), req).unwrap().unwrap();
        assert_eq!(old.sa_handler, 0x2000);
    }

    #[test]
    fn test_sigaction_mask_strips_unkillable() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        let mask = UNKILLABLE_MASK | (1u64 << 5);
        let req = SigActionReq { signo: 3, act: Some(SigAction { sa_handler: 0x3000, sa_mask: mask, sa_flags: 0 }), need_oact: false, sigreturn: VirBytes(0x500) };
        handle_sigaction(&mut table, UserSlot::new(0), req).unwrap();
        assert!(table.procs[0].resources.signals.is_caught(3));
        assert_eq!(table.procs[0].resources.signals.actions[2].sa_mask & UNKILLABLE_MASK, 0);
        assert_ne!(table.procs[0].resources.signals.actions[2].sa_mask & (1u64 << 5), 0);
        assert_eq!(table.procs[0].resources.signals.sigreturn_addr, VirBytes(0x500));
    }

    #[test]
    fn test_sigpending_snapshot() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.pending = 1u64 << 5;
        table.procs[5].resources.signals.mask = 1u64 << 5;
        let snap = handle_sigpending(&table, UserSlot::new(5));
        assert_eq!(snap, 1u64 << 5);
    }

    #[test]
    fn test_sigprocmask_block_strips() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        let set = UNKILLABLE_MASK | (1u64 << 5);
        let (old, eff) = handle_sigprocmask(&mut table, UserSlot::new(0), SIG_BLOCK, set).unwrap();
        assert_eq!(old, 0);
        assert_eq!(eff, MaskOpEffect::Changed { needs_check: false });
        assert_eq!(table.procs[0].resources.signals.mask & UNKILLABLE_MASK, 0);
        assert_ne!(table.procs[0].resources.signals.mask & (1u64 << 5), 0);
    }

    #[test]
    fn test_sigprocmask_unblock_does_not_strip() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        table.procs[0].resources.signals.mask = UNKILLABLE_MASK | (1u64 << 5);
        // UNBLOCK should clear bit 5 even though set contains kill/stop bits (it doesn't strip)
        let set = (1u64 << 5) | UNKILLABLE_MASK;
        let (_old, eff) = handle_sigprocmask(&mut table, UserSlot::new(0), SIG_UNBLOCK, set).unwrap();
        assert_eq!(eff, MaskOpEffect::Changed { needs_check: true });
        // UNBLOCK clears bit 5
        assert_eq!(table.procs[0].resources.signals.mask & (1u64 << 5), 0);
    }

    #[test]
    fn test_sigprocmask_invalid_how() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        assert_eq!(handle_sigprocmask(&mut table, UserSlot::new(0), 99, 0).unwrap_err(), SigMaskError::InvalidHow);
    }

    #[test]
    fn test_sigsuspend_returns_reply_later() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        table.procs[0].resources.signals.mask = 1u64 << 3;
        let intent = handle_sigsuspend(&mut table, UserSlot::new(0), 1u64 << 6);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        assert!(table.procs[0].resources.signals.suspended);
        assert_eq!(table.procs[0].resources.signals.mask_saved, 1u64 << 3);
        assert_eq!(table.procs[0].resources.signals.mask & UNKILLABLE_MASK, 0);
    }

    #[test]
    fn test_sigsuspend_preserves_kill_stop() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        let intent = handle_sigsuspend(&mut table, UserSlot::new(0), UNKILLABLE_MASK);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        assert_eq!(table.procs[0].resources.signals.mask & UNKILLABLE_MASK, 0);
    }

    #[test]
    fn test_sigreturn_restores_and_calls_kernel() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1);
        table.procs[0].resources.signals.mask = 1u64 << 5;
        let mut k = OkKernel;
        let res = handle_sigreturn(&mut table, UserSlot::new(0), 1u64 << 6, VirBytes(0x1000), &mut k);
        assert!(res.is_ok());
        assert_eq!(table.procs[0].resources.signals.mask, 1u64 << 6);
    }

    #[test]
    fn test_sig_send_requires_stopped() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        table.procs[5].state.block.stopped = false; // not stopped → panic
        let mut k = OkKernel;
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = sig_send(&mut table, UserSlot::new(5), 3, &mut k);
        }));
        assert!(res.is_err());
    }

    #[test]
    fn test_sig_send_fault_returns_false() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.unpaused = true;
        let mut k = FaultKernel;
        let res = sig_send(&mut table, UserSlot::new(5), 3, &mut k);
        assert_eq!(res.unwrap_err(), SigSendError::FaultOrNoMem);
    }

    #[test]
    #[should_panic]
    fn test_sig_send_unexpected_panics() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.unpaused = true;
        let mut k = UnexpectedKernel;
        let _ = sig_send(&mut table, UserSlot::new(5), 3, &mut k);
    }

    #[test]
    fn test_sig_send_waiting_returns_eintr() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.wait.waiting = true;
        let mut k = OkKernel;
        let res = sig_send(&mut table, UserSlot::new(5), 3, &mut k).unwrap();
        assert_eq!(res, PostAction::InterruptedWait);
        assert!(!table.procs[5].state.wait.waiting);
        assert!(!table.procs[5].resources.signals.suspended);
    }

    #[test]
    fn test_sig_send_unpaused() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.unpaused = true;
        table.procs[5].state.wait.waiting = false;
        table.procs[5].resources.signals.suspended = false;
        let mut k = OkKernel;
        let res = sig_send(&mut table, UserSlot::new(5), 3, &mut k).unwrap();
        assert_eq!(res, PostAction::AwaitVfsUnpause);
    }

    #[test]
    fn test_sig_send_suspended_uses_mask2() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        table.procs[5].resources.signals.actions[2].sa_mask = 0;
        table.procs[5].resources.signals.actions[2].sa_flags = 0;
        table.procs[5].resources.signals.mask = 1u64 << 5;
        table.procs[5].resources.signals.mask_saved = 1u64 << 6;
        table.procs[5].resources.signals.suspended = true;
        table.procs[5].state.block.stopped = true;
        table.procs[5].state.block.unpaused = true;
        // Capture SigMsg
        struct Capture(Option<SigMsg>);
        impl KernelSig for Capture {
            fn sigsend(&mut self, _ep: Endpoint, msg: &SigMsg) -> Result<(), i32> { self.0 = Some(*msg); Ok(()) }
            fn sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> { Ok(()) }
        }
        let mut cap = Capture(None);
        sig_send(&mut table, UserSlot::new(5), 3, &mut cap).unwrap();
        let msg = cap.0.unwrap();
        assert!(msg.mask & (1u64 << 6) != 0); // mask_saved
        assert!(msg.mask & (1u64 << 2) != 0); // current signal blocked
    }
}
