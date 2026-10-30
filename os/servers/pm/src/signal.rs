//! Signal core: `do_kill`/`do_srv_kill` → `check_sig` → `sig_proc` → `sig_proc_exit` + `process_ksig`.
//!
//! C ground truth: `minix3/minix/servers/pm/signal.c:197-646` (do_kill/check_sig/sig_proc/process_ksig)
//! Design: explicit `SignalTarget` + `SignalState` (`mproc/signal.rs`) + `SignalClass`.
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, UserSlot, Pid, EINVAL, ESRCH, EPERM, EDEADEPT};
use crate::mproc::{ProcTable, _NSIG};

/// Signal numbers (subset, `sys/signal.h`).
pub const SIGKILL: i32 = 9;
pub const SIGTERM: i32 = 15;
pub const SIGCHLD: i32 = 20;
/// Lethal signal family (members of `SIGS_IS_LETHAL`, `sys/signal.h:279-281`;
/// values in `sys/signal.h:55-65`).
pub const SIGILL: i32 = 4;
pub const SIGABRT: i32 = 6;
pub const SIGEMT: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGPIPE: i32 = 13;
pub const SIGBUS: i32 = 10;
pub const SIGSEGV: i32 = 11;

pub const SIGSTOP: i32 = 17;
/// Hangup (`sys/sys/signal.h:52`)—broadcast to the process group when the
/// session leader dies (`check_sig(-procgrp, SIGHUP)`, forkexit.c:412).
pub const SIGHUP: i32 = 1;
/// Kernel signal: SIGSNDELAY (`sys/sys/signal.h:264`, value 70—the earlier
/// `sys_delay_stop` EBUSY delay has ended, so the signal handling that was set
/// aside can resume). The old code wrote 42 with the note "Actually SIGSNDELAY
/// is 41?"—it never matched.
pub const SIGSNDELAY: i32 = 70;
/// Kernel signal: SIGKSIG (`sys/sys/signal.h:274`, value 74)—the kernel has
/// accumulated pending signals for the signal manager (a bitmap over the SIGS
/// range, pulled through `sys_getksig`).
pub const SIGKSIG: i32 = 74;

/// Kill error, maps to `errno`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillError {
    InvalidSignal,
    NoSuchProcess,
    PermissionDenied,
    InvalidEndpoint,
}

impl KillError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidSignal => EINVAL,
            Self::NoSuchProcess => ESRCH,
            Self::PermissionDenied => EPERM,
            Self::InvalidEndpoint => EDEADEPT,
        }
    }
}

/// Handles `PM_KILL` (`do_kill`, `signal.c:197-202`).
///
/// `ksig = false` — user `kill(2)`, `PRIV_PROC` lethal protection applies.
pub fn do_kill(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    check_sig(table, caller, pid, signo, false, kern, transport)
}

/// Handles `PM_SRV_KILL` (`do_srv_kill`, `204-221`).
///
/// Only `RS` may call; `ksig = true` so `PRIV_PROC` can be killed.
pub fn do_srv_kill(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    if table.procs[caller.get()].endpoint() != Endpoint::RS {
        return Err(KillError::PermissionDenied);
    }
    check_sig(table, caller, pid, signo, true, kern, transport)
}

/// Checks which processes to signal (`check_sig`, `568-646`).
///
/// `pid` four meanings: `>0` one, `0` process group (caller procgrp), `-1` all (except `INIT_PID`), `<-1` group `-pid`.
/// `signo == 0` is existence probe (no `sig_proc`, just count).
/// Returns `Ok(count)` or `Err(errno)`, and `SUSPEND` is modeled as `Err` with `caller` now `EXITING` (self-kill).
pub fn check_sig<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    ksig: bool,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<usize, KillError> {
    if signo < 0 || signo >= _NSIG as i32 {
        return Err(KillError::InvalidSignal);
    }
    if pid == 1 && signo == SIGKILL {
        return Err(KillError::InvalidSignal); // EINVAL for INIT+KILL
    }
    // Broadcast SIGTERM: RS first (signal.c:588-589)—through the sys_kill kernel
    // loopback so a ksig is produced; PM does not deliver to system processes
    // directly (sig_proc's PRIV_PROC !ksig branch is a no-op, so calling it
    // directly means RS never actually gets the notification). C does not check
    // the return value.
    if pid == -1 && signo == SIGTERM {
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = kern.sys_kill(Endpoint::RS, SIGTERM);
    }

    let mut count = 0;
    let mut error_code = KillError::NoSuchProcess;
    // Iterate reverse (NR_PROCS-1..0) to hit system procs at end first (597)
    for idx in (0..minix_types::NR_PROCS).rev() {
        let proc = &table.procs[idx];
        if !proc.is_in_use() {
            continue;
        }
        // Selection (601-604)
        if pid > 0 && pid != proc.identity.id.pid {
            continue;
        }
        if pid == 0 && table.procs[caller.get()].identity.procgrp != proc.identity.procgrp {
            continue;
        }
        if pid == -1 && proc.identity.id.pid <= 1 {
            continue;
        }
        if pid < -1 && proc.identity.procgrp != -pid {
            continue;
        }
        // Broadcast SIGKILL skip PRIV_PROC (607-608)
        if pid == -1 && signo == SIGKILL && proc.is_kernel_process() {
            continue;
        }
        // VM skip (613)
        if proc.endpoint() == Endpoint::VM {
            continue;
        }
        // Lethal protection (616-618)
        if !ksig && is_lethal(signo) && proc.is_kernel_process() {
            error_code = KillError::PermissionDenied;
            continue;
        }
        // Permission (622-628): SUPER_USER or real/eff match
        if !can_signal(table, caller, UserSlot::new(idx)) {
            error_code = KillError::PermissionDenied;
            continue;
        }
        count += 1;
        if signo == 0 || proc.state.lifecycle.is_exiting() {
            continue;
        }
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = sig_proc(table, UserSlot::new(idx), signo, true, ksig, kern, transport);
        if pid > 0 {
            break;
        }
    }
    // Self-kill SUSPEND (644)
    if table.procs[caller.get()].state.lifecycle.is_exiting() {
        // Caller killed itself → SUSPEND (beyond grave)
        // In Rust we model as Ok(count) but caller lifecycle is Exiting; dispatcher will map to ReplyLater
        // For check_sig we return count, caller will handle SUSPEND via lifecycle check
        // Here we just return count; the SUSPEND is observed via caller's Exiting flag
    }
    if count > 0 {
        Ok(count)
    } else {
        Err(error_code)
    }
}

/// Whether signal is lethal (`SIGS_IS_LETHAL`, `sys/signal.h:279-281`):
/// SIGILL | SIGBUS | SIGFPE | SIGSEGV | SIGEMT | SIGABRT.
fn is_lethal(signo: i32) -> bool {
    matches!(signo, SIGILL | SIGBUS | SIGFPE | SIGSEGV | SIGEMT | SIGABRT)
}

/// Whether signal needs a stacktrace (`SIGS_IS_STACKTRACE`,
/// `sys/signal.h:286`): lethal and not SIGABRT. For PRIV_PROC system signals C
/// first calls `sys_diagctl_stacktrace` (`signal.c:455-457`).
fn is_stacktrace(signo: i32) -> bool {
    is_lethal(signo) && signo != SIGABRT
}

/// Whether signal is termination (`SIGS_IS_TERMINATION`,
/// `sys/signal.h:282-284`): lethal or SIGKILL/SIGPIPE. Termination-class signals
/// go through `sig_proc_exit`; non-termination ones become
/// `SIGS_SIGNAL_RECEIVED` messages (`signal.c:459-470`).
fn is_termination(signo: i32) -> bool {
    is_lethal(signo) || signo == SIGKILL || signo == SIGPIPE
}

/// Checks permission (`signal.c:621-628`).
fn can_signal(table: &ProcTable, caller: UserSlot, target: UserSlot) -> bool {
    // Credentials always exist: the Kernel privilege slot carries all-zero
    // credentials (effuid 0 = SUPER_USER), consistent with C's `mp->mp_effuid`
    // also being 0 for system processes, so no Option branch is needed.
    let c = table.procs[caller.get()].resources.privilege.credentials();
    let t = table.procs[target.get()].resources.privilege.credentials();
    if c.user.effective == 0 {
        return true; // SUPER_USER
    }
    c.user.real == t.user.real
        || c.user.effective == t.user.real
        || c.user.real == t.user.effective
        || c.user.effective == t.user.effective
}

/// Adapts [`sig_proc`] (the trace=FALSE redelivery form) into the
/// [`crate::signal_flow::SignalDeliver`] injection point—the delivery tail of
/// check_pending/handle_sigsn_delay (C calls the module-level function directly;
/// Rust keeps injection testable through a bridge).
struct SigProcDeliver<'a, T: crate::ipc::IpcTransport + ?Sized> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    transport: &'a mut T,
}

impl<T: crate::ipc::IpcTransport + ?Sized> crate::signal_flow::SignalDeliver for SigProcDeliver<'_, T> {
    fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, signo: i32, ksig: bool) {
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = sig_proc(table, target, signo, false, ksig, self.kern, self.transport);
    }
}

/// Adapts [`crate::exit::KernelGateway`]'s `sys_delay_stop` into the
/// [`crate::signal_flow::KernelStop`] seam (the narrow interface from the design
/// step 13). A separate bridge rather than supertrait/dyn upcasting:
/// signal_flow does not know the central gateway type and does not depend on the
/// toolchain version's trait upcasting.
struct GatewayStopBridge<'a>(&'a mut dyn crate::exit::KernelGateway);

impl crate::signal_flow::KernelStop for GatewayStopBridge<'_> {
    fn delay_stop(&mut self, ep: Endpoint) -> i32 {
        match self.0.sys_delay_stop(ep) {
            Ok(()) => 0, // OK
            Err(e) => e,
        }
    }
}

/// Sends signal to process (`sig_proc`, `384-540`).
///
/// 9-step chain: TRACE→VFS|EVENT→PRIV_PROC→badignore→ignore→block→TRACE_STOPPED→caught→terminate.
/// Returns `Ok(())` or `Err` (for sig_send failure).
pub fn sig_proc<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    trace: bool,
    ksig: bool,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<(), KillError> {
    let proc = &table.procs[target.get()];
    if !proc.is_in_use() || proc.state.lifecycle.is_exiting() {
        // panic in C (407-409) — but for Rust we return Err
        return Err(KillError::NoSuchProcess);
    }
    // TRACE first (411-422): a debugger signal enters the sigtrace buffer first,
    // and if the process is not stopped, trace_stop stops it—the kernel really
    // stops on sys_trace(T_STOP) and the tracer receives a W_STOPCODE reply at
    // wait time. Before this branch was fixed it only set the stopped flag, so
    // neither the kernel nor the tracer noticed.
    if trace && proc.state.guardianship.tracer().is_some() && signo != SIGKILL {
        table.procs[target.get()].resources.signals.trace_mask |= crate::init::sig_bit(signo);
        if !table.procs[target.get()].state.trace.stopped {
            crate::trace::trace_stop(table, target, signo, kern, transport);
        }
        return Ok(());
    }
    // VFS|EVENT pending (425-444): set the pending bit and then stop the
    // process—preventing it from issuing another call after the VFS/event reply
    // arrives but before PM re-checks signals; PROC_STOPPED doubles as the
    // indicator for restart_sigs' re-check (the semantics documented at C
    // signal.c:430-443). Before this branch was fixed it only set the bit and
    // never stopped, which broke check_pending's VFS|EVENT invariant (must
    // already be stopped, signal_flow.rs:254). The branch condition follows C
    // signal.c:425 and uses VFS_CALL|EVENT_CALL—the old code's
    // `ipc_blocked.is_some()` also caught DELAY_CALL, which C excludes.
    if table.procs[target.get()].state.block.is_vfs_blocked()
        || table.procs[target.get()].state.block.is_event_blocked()
    {
        table.procs[target.get()].resources.signals.pending |= crate::init::sig_bit(signo);
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= crate::init::sig_bit(signo);
        }
        // C: `if (!(PROC_STOPPED | DELAY_CALL)) stop_proc(rmp, FALSE)`—
        // FALSE = may not delay, and stop_proc panics internally when the kernel
        // answers EBUSY (the same as C). The DELAY_CALL half of C's guard is not
        // representable in Rust: IpcBlockReason's three variants are mutually
        // exclusive (block.rs:50-84, ARCH A-2), so VFS_CALL/EVENT_CALL and
        // DELAY_CALL cannot coexist, leaving only the STOPPED half here.
        if !table.procs[target.get()].state.block.stopped {
            let mut bridge = GatewayStopBridge(kern);
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = crate::signal_flow::stop_proc(
                table,
                target,
                crate::signal_flow::MayDelay::MustStop,
                &mut bridge,
            );
        }
        return Ok(());
    }
    // PRIV_PROC system signals (448-480)
    if table.procs[target.get()].is_kernel_process() {
        if table.procs[target.get()].endpoint() == Endpoint::PM {
            return Ok(());
        }
        if !ksig {
            // C signal.c:456-462: system signals always go through the kernel
            // loopback—letting the kernel choose the right signal manager; if PM
            // is the manager, the signal comes back to PM for actual handling.
            // C does not check the return value. Before this branch was fixed it
            // silently dropped the signal (that fix only touched the call site in
            // check_sig's broadcast to RS; sig_proc itself was still a no-op).
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = kern.sys_kill(table.procs[target.get()].endpoint(), signo);
            return Ok(());
        }
        // C signal.c:464-466: a stacktrace-class signal (lethal and not SIGABRT)
        // first asks the kernel to print the target's stack, then takes the
        // terminate/deliver fork. C does not check the return value (a failing
        // diagnostic channel is silent).
        if is_stacktrace(signo) {
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = kern.sys_diagctl_stacktrace(table.procs[target.get()].endpoint());
        }
        if !is_termination(signo) {
            // C signal.c:468-474: every non-termination kernel signal is
            // translated into a SIGS_SIGNAL_RECEIVED message delivered to the
            // target service via `asynsend3(ep, &m, AMF_NOREPLY)`. The Rust side
            // is not wired yet: the only blocker is that `asynsend3`'s retry
            // table has no equivalent in PM's IpcTransport (`sendnb` drops with
            // EBUSY when the target is not in receive). The payload is no longer
            // an obstacle—`minix_types::MessPmLsysSigsSignal` and
            // `MessageUnion`'s `m_pm_lsys_sigs_signal` arm are already in, the
            // receiving side (the SEF interception arm) has landed too, and
            // wiring this arm only needs a retry-capable send primitive plus a
            // `sigs_signal_message`-shaped constructor (the same-shaped
            // precedent on the RS side is
            // `os/servers/rs/src/lib.rs:sigs_signal_message`). The kernel's ksig
            // currently produces only termination-class signals, so this path is
            // in practice unreachable—recorded as pending for the signal-chain
            // work (`04-stage-pm/todo.md` §12.7).
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = (target, signo);
            return Ok(());
        } else {
            return sig_proc_exit(table, target, signo, transport, kern);
        }
    }
    // User process: badignore / ignore / block / TRACE_STOPPED / caught / terminate
    // Bit positions uniformly go through `init::sig_bit` (aligned with C's
    // `__sigmask`); no bare shifts appear in this function.
    let sigmask = crate::init::sig_bit(signo);
    let state = &table.procs[target.get()].resources.signals;
    // badignore (C signal.c:483-486): only when the kernel signal itself is in
    // the noign set and that signal has been ignored or masked does it force
    // default handling (it cannot be swallowed by settings).
    let badignore = ksig
        && (crate::init::NOIGN_SIGSET & sigmask != 0)
        && (state.ignored & sigmask != 0 || state.mask & sigmask != 0);
    if !badignore && (state.ignored & sigmask != 0) {
        return Ok(());
    }
    if !badignore && (state.mask & sigmask != 0) {
        table.procs[target.get()].resources.signals.pending |= sigmask;
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= sigmask;
        }
        return Ok(());
    }
    if table.procs[target.get()].state.trace.stopped && signo != SIGKILL {
        table.procs[target.get()].resources.signals.pending |= sigmask;
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= sigmask;
        }
        return Ok(());
    }
    if !badignore && (state.caught & sigmask != 0) {
        // C signal.c:509-518: unpause's three paths—if not ready (deferred / VFS
        // wait), the signal becomes pending and is redelivered through
        // restart_signals once the UNPAUSE reply arrives.
        if !unpause(table, target, kern, transport) {
            table.procs[target.get()].resources.signals.pending |= sigmask;
            if ksig {
                table.procs[target.get()].resources.signals.kernel_pending |= sigmask;
            }
            return Ok(());
        }
        if sig_send(table, target, signo, kern, transport).is_ok() {
            return Ok(());
        }
        // Fall through to terminate on sig_send failure (C same: printf, then
        // fall into termination, signal.c:531-534)
    } else if !badignore && (crate::init::IGN_SIGSET & sigmask != 0) {
        // Signal defaults to being ignored (C signal.c:535-539; note the mutual
        // exclusion with the caught branch: a default-ignored signal whose catch
        // failed must terminate, not be ignored)
        return Ok(());
    }
    sig_proc_exit(table, target, signo, transport, kern)
}

/// Terminates process via signal (`sig_proc_exit`, `546-563`).
fn sig_proc_exit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) -> Result<(), KillError> {
    let is_core = crate::init::CORE_SIGSET & crate::init::sig_bit(signo) != 0;
    // C: exit_proc(rmp, 0, dump_core=is_core)—exit_proc's tail unconditionally
    // calls tell_vfs (DUMPCORE or EXIT, forkexit.c:350-358), so the transport
    // must be the caller's real channel. It once built a one-shot mock here,
    // losing every VFS notification of a signal termination (core-path processes
    // then hung in EXITING forever). The kernel exit (step 9 sys_clear) uses the
    // production gateway: this path's target is a user process (PRIV_PROC already
    // returned in the earlier branch), so step 9 never triggers; should a future
    // semantic change hit it, the pre-E1 code honestly panics (the same failure
    // semantics as C).
    // C: signal.c:552 — rmp->mp_sigstatus = (char) signo (written before
    // entering exit_proc), then forkexit.c:354 reports it to VFS as
    // VFS_PM_TERM_SIG, and forkexit.c:374 keeps it alongside EXITING until
    // zombify/wait. Rust's sigstatus rides in the lifecycle, and EXITING is only
    // established at exit_proc step 10, so it is carried explicitly through the
    // sig_status parameter (the old code read it back from the lifecycle as a
    // constant 0 — VFS then refused DUMPCORE and the exit chain stalled).
    crate::exit::exit_proc(table, target, 0, signo as i8, is_core, transport, kern);
    Ok(())
}

/// Unpauses a process for `sig_send` (`unpause`, `signal.c:719-770`).
///
/// Returns `true` = the process is ready to spawn the handler immediately;
/// `false` = the signal becomes pending (C returns FALSE, and the caller does
/// `sigaddset(pending)` and returns).
/// Three paths: UNPAUSED already set (a process waiting for restart_sigs'
/// re-check, already stopped) → true; DELAY_CALL in flight → false;
/// WAITING/SIGSUSPENDED → stopped by `stop_proc(FALSE)` → true; otherwise not
/// stopped → `stop_proc(TRUE)` (false on kernel EBUSY) +
/// `tell_vfs(VFS_PM_UNPAUSE)` asking VFS to interrupt its blocking call → false
/// (the UNPAUSE reply is redelivered through restart_signals). Before this
/// function was fixed it returned true for a running process without setting
/// stopped—dead-ending sig_send's PROC_STOPPED assertion.
fn unpause<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    target: UserSlot,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> bool {
    let proc = &table.procs[target.get()];
    assert!(
        !proc.state.block.is_vfs_blocked() && !proc.state.block.is_event_blocked(),
        "unpause: !(VFS|EVENT)（VFS|EVENT 在 sig_proc 前置分支已返回）"
    );
    if proc.state.block.unpaused {
        // C 734-738: UNPAUSED always comes with PROC_STOPPED (the stop was
        // established by the previous unpause round).
        assert!(
            proc.state.block.stopped
                && !matches!(
                    proc.state.block.ipc_blocked,
                    Some(crate::mproc::IpcBlockReason::DelayedSignal)
                ),
            "unpause UNPAUSED must be (DELAY|PROC)==PROC"
        );
        return true;
    }
    if matches!(
        proc.state.block.ipc_blocked,
        Some(crate::mproc::IpcBlockReason::DelayedSignal)
    ) {
        return false; // C 741-742: a delayed call is in flight → pending
    }
    if proc.state.wait.waiting || proc.resources.signals.suspended {
        // C 745-753: sleeping in PM → stop_proc(FALSE) has ruled out EBUSY →
        // true.
        let mut stop_bridge = GatewayStopBridge(&mut *kern);
        //[SD19-gate] PM send/notify/reply is fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal_flow::stop_proc(
            table,
            target,
            crate::signal_flow::MayDelay::MustStop,
            &mut stop_bridge,
        );
        return true;
    }
    // C 760-769: not stopped → stop_proc(TRUE) (EBUSY = deferred, false) + ask
    // VFS to interrupt its blocking call; the UNPAUSE reply goes through
    // restart_signals → restart_sigs redelivery.
    if !proc.state.block.stopped {
        let mut stop_bridge = GatewayStopBridge(&mut *kern);
        if let Ok(crate::signal_flow::StopOutcome::Deferred) = crate::signal_flow::stop_proc(
            table,
            target,
            crate::signal_flow::MayDelay::MayDefer,
            &mut stop_bridge,
        ) {
            return false;
        }
    }
    let ep = table.procs[target.get()].endpoint();
    let mut call = minix_types::Message {
        m_type: minix_types::VFS_PM_UNPAUSE,
        ..Default::default()
    };
    call.m_u.m_m1.m1i1 = ep.0;
    transport
        .send(Endpoint::VFS, &call)
        .unwrap_or_else(|e| panic!("tell_vfs: send failed: {:?}", e));
    false
}

/// Adapts [`crate::exit::KernelGateway`]'s `sys_resume` into the
/// [`crate::signal_flow::KernelResume`] seam (design step 13, D2).
struct GatewayResumeBridge<'a>(&'a mut dyn crate::exit::KernelGateway);

impl crate::signal_flow::KernelResume for GatewayResumeBridge<'_> {
    fn resume(&mut self, ep: Endpoint) -> i32 {
        self.0.sys_resume(ep).err().unwrap_or(0) // OK = 0; Err carries the raw errno
    }
}

/// Sends signal via handler (`sig_send`, `signal.c:772-855`).
///
/// The core of delivering a caught signal: `prepare_sigmsg` (the four-step mask
/// evolution, RESETHAND, and pending clearing, see mproc/signal.rs) is handed to
/// `sys_sigsend`, and the kernel builds the sigframe and wakes the process
/// (signal.c:818); for WAITING/SIGSUSPENDED it interrupts the blocking call
/// (clear flags, reply(EINTR), try_resume_proc, signal.c:823-836). Before this
/// was implemented it was an `Ok(())` shell—after sigaction was wired, a caught
/// signal would "look delivered" while the process noticed nothing.
/// EFAULT/ENOMEM mean the process's memory cannot hold the handler (a legal
/// failure; the target will be killed, C returns FALSE); any other error panics
/// (the same as C's "sys_sigsend failed").
fn sig_send<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<(), KillError> {
    assert!(
        table.procs[target.get()].state.block.stopped,
        "sig_send: PROC_STOPPED"
    );
    // The four preparation steps (mask/sigmsg assembly + RESETHAND + pending
    // clearing + mp_sigmask bookkeeping).
    let sigmsg = table.procs[target.get()]
        .resources
        .signals
        .prepare_sigmsg(signo as u32)
        .ok_or(KillError::InvalidSignal)?;
    let wire = minix_sys::syscall::SigMsgWire {
        signo: sigmsg.signo as u32,
        mask: sigmsg.mask,
        sighandler: sigmsg.handler.0,
        sigreturn: sigmsg.sigreturn.0,
        stkptr: 0, // C lets the kernel take it from the process context when building the sigframe; sigmsg does not carry it
    };
    let ep = table.procs[target.get()].endpoint();
    match kern.sys_sigsend(ep, &wire) {
        Err(r) if r == -minix_types::EFAULT || r == -minix_types::ENOMEM => {
            return Err(KillError::NoSuchProcess); // C: return(FALSE) → terminate fallback
        }
        Err(r) => panic!("sys_sigsend failed: {}", r),
        Ok(()) => {}
    }
    // WAITING|SIGSUSPENDED → interrupt the blocking call (signal.c:823-836):
    // clear both flags, reply(EINTR) (UNPAUSED not set—the stop was just
    // established by unpause()), try_resume.
    let suspended_in_pm = table.procs[target.get()].state.wait.waiting
        || table.procs[target.get()].resources.signals.suspended;
    if suspended_in_pm {
        table.procs[target.get()].state.wait.waiting = false;
        table.procs[target.get()].resources.signals.suspended = false;
        let ep = table.procs[target.get()].endpoint();
        let eintr = minix_types::Message { m_type: minix_types::EINTR, ..Default::default() };
        transport.send(ep, &eintr).ok();
        assert!(
            !table.procs[target.get()].state.block.unpaused,
            "sig_send: WAITING path must not have UNPAUSED"
        );
        let mut resume_bridge = GatewayResumeBridge(&mut *kern);
        crate::signal_flow::try_resume_proc(table, target, &mut resume_bridge);
    } else {
        // Non-suspended path: arrived through restart_sigs (UNPAUSED must be
        // set, signal.c:841).
        assert!(
            table.procs[target.get()].state.block.unpaused,
            "sig_send: non-suspended path requires UNPAUSED"
        );
    }
    Ok(())
}

/// Processes kernel signal (`process_ksig`, `294-378`).
pub fn process_ksig(
    table: &mut ProcTable,
    endpoint: Endpoint,
    signo: i32,
    vctl: &mut dyn crate::timer::VTimerCtl,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<(), KillError> {
    let slot = table
        .pm_isokendpt(endpoint)
        .map(|s| s.get())
        .map_err(|_| KillError::InvalidEndpoint)?;
    let proc = &table.procs[slot];
    if !proc.is_in_use() || proc.state.lifecycle.is_exiting() {
        return Err(KillError::InvalidEndpoint);
    }
    // Pretend PM is sender (312)
    //[SD19-gate] PM send/notify/reply is fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = proc.identity.procgrp;
    let pid = proc.identity.id.pid;
    // SIGVTALRM/SIGPROF → reset the virtual timer (C: check_vtimer at
    // signal.c:326-328 plus fall-through to the unicast default branch). Note
    // SIGVTALRM = 26: the old code wrote 12 (SIGSYS), so that branch never
    // matched—a real bug, fixed here. VTimerCtl's production implementation is
    // the kernel's sys_vtimer (E6).
    if signo == crate::timer::SIGVTALRM || signo == crate::timer::SIGPROF {
        crate::timer::check_vtimer(table, slot, signo, vctl);
    }
    // Broadcast vs single (320-332)
    let target_pid = match signo {
        2 | 3 | 28 | 29 => 0, // INT, QUIT, WINCH, INFO → group broadcast
        _ => pid,
    };
    // C does not check check_sig's return value (a statement call at
    // signal.c:337)—for kernel signals beyond _NSIG such as SIGSNDELAY(70),
    // check_sig's out-of-bounds gate returning EINVAL is expected; the old code
    // propagated it with `?`, dead-ending the SIGSNDELAY tail.
    //[SD19-gate] PM send/notify/reply is fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = check_sig(table, UserSlot::new(0), target_pid, signo, true, kern, transport);
    // SIGSNDELAY (C signal.c:344-369): the earlier stop_proc answered EBUSY
    // because the process had a send in flight, and the kernel delivers
    // SIGSNDELAY once the send completes—resuming the handling set aside
    // (clear DELAY_CALL → if VFS|EVENT is in flight, stop_proc, otherwise
    // check_pending). Design step 13's handle_sigsn_delay is the full-semantics
    // port of that section.
    if signo == SIGSNDELAY {
        // Same semantics as step 13's handle_sigsn_delay
        // (signal_flow.rs:76-110); inlined in this order because the stop bridge
        // and the delivery bridge do not borrow kern simultaneously.
        let delayed = matches!(
            table.procs[slot].state.block.ipc_blocked,
            Some(crate::mproc::IpcBlockReason::DelayedSignal)
        );
        if delayed {
            // Clear DELAY_CALL (351) + assert not stopped (353).
            table.procs[slot].state.block.ipc_blocked = None;
            assert!(
                !table.procs[slot].state.block.stopped,
                "SIGSNDELAY: DELAY without PROC_STOPPED"
            );
            // VFS|EVENT in flight → stop_proc(MustStop), then re-check after the
            // VFS reply (359-363).
            if table.procs[slot].state.block.is_vfs_blocked()
                || table.procs[slot].state.block.is_event_blocked()
            {
                let mut stop_bridge = GatewayStopBridge(&mut *kern);
                //[SD19-gate] PM send/notify/reply is fire-and-forget
                #[allow(clippy::let_underscore_must_use)]
                let _ = crate::signal_flow::stop_proc(
                    table,
                    UserSlot::new(slot),
                    crate::signal_flow::MayDelay::MustStop,
                    &mut stop_bridge,
                );
                return Ok(());
            }
            // Handle as many ordinary signals as possible (366).
            let mut deliver = SigProcDeliver { kern: &mut *kern, transport: &mut *transport };
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = crate::signal_flow::check_pending(table, UserSlot::new(slot), &mut deliver);
        }
    }
    if table.procs[slot].state.lifecycle.is_exiting() {
        return Err(KillError::InvalidEndpoint);
    }
    Ok(())
}

/// C `process_sigmgr_signals` (`sef_signal.c:27-63`): the pull loop after a
/// SIGKSIG notification arrives—when the kernel has accumulated pending kernel
/// signals for the signal manager, take them back one at a time
/// (`sys_getksig`, where taking consumes RTS_SIGNALED), acknowledge each
/// (`sys_endksig`), and drive `process_ksig` to handle it, until the kernel
/// reports no more. A failed take panics in C ("SEF: sys_getksig failed"); a
/// single signal whose target is already gone (EDEADEPT) just continues the
/// loop (same as C).
pub fn process_sigmgr_signals(
    table: &mut ProcTable,
    vctl: &mut dyn crate::timer::VTimerCtl,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) {
    loop {
        let found = kern
            .get_ksig()
            .unwrap_or_else(|r| panic!("SEF: sys_getksig failed: {}", r));
        let (target, set) = match found {
            Some(pair) => pair,
            None => break,
        };
        // Full-width bitmap (the same 128 bits as C's sigset_t, so `sigismember`
        // consumes it bit by bit; internal kernel signals 71..=74 land in word
        // 2). Range checking is done by sigset_contains itself (out of bounds
        // answers false, unlike C's unchecked index).
        for signo in 1u32..=128 {
            if minix_types::sigset_contains(set, signo) {
                if let Err(r) = kern.end_ksig(target, signo as i32) {
                    panic!("sys_endksig failed: {}", r);
                }
                //[SD19-gate] PM send/notify/reply is fire-and-forget
                #[allow(clippy::let_underscore_must_use)]
                let _ = process_ksig(table, target, signo as i32, vctl, kern, transport);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    /// Kernel-gateway mock for the signal.rs tests (sys_kill/sys_clear/sys_abort
    /// always OK; proc_times returns scriptable accounting values for the
    /// accumulation assertions).
    struct TestKernel {
        pub user: minix_types::Clock,
        pub sys: minix_types::Clock,
    }
    impl crate::exit::KernelGateway for TestKernel {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
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
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            Ok((self.user, self.sys))
        }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    }
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Clock, Endpoint, UserSlot};

    struct NopVTimer;
    impl crate::timer::VTimerCtl for NopVTimer {
        fn vtimer(&mut self, _ep: Endpoint, _which: crate::timer::ItimerWhich, _set: Option<Clock>, _get: Option<&mut Clock>) -> i32 { 0 }
    }

    fn mk_proc(table: &mut ProcTable, slot: usize, pid: i32, procgrp: i32, is_kernel: bool) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.procgrp = procgrp;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        if is_kernel {
            table.procs[slot].resources.privilege = Privilege::Kernel(Credentials::default());
        } else {
            table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        }
    }

    #[test]
    fn test_kill_single() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, false, &mut kern_rec, &mut t);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);
    }

    // ---- Signal sets and badignore behavior ----

    /// C signal.h:71/80 (sys/sys/signal.h).
    const SIGCONT_TEST: i32 = 19;
    const SIGWINCH_TEST: i32 = 28;

    /// C signal.c:535-539: SIGCONT ∈ ign_sset and is default-ignored—a clean
    /// process receiving SIGCONT must survive. A bit-order error once made this
    /// path fall into the termination branch.
    #[test]
    fn test_sigproc_default_ignores_sigcont() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernel { user: 0, sys: 0 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, SIGCONT_TEST, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);
        assert!(table.procs[5].is_in_use() && !table.procs[5].is_exiting());
    }

    /// C signal.c:483-486: ksig + the signal ∈ noign_sset + the process ignores
    /// it → badignore forces default handling (SIGSEGV ∈ core_sigs → the
    /// dump-termination path), and the ignore setting is overridden.
    #[test]
    fn test_badignore_forces_default_on_ignored_lethal_ksig() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = crate::init::sig_bit(crate::signal::SIGSEGV);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernel { user: 0, sys: 0 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, true, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(table.procs[5].is_exiting());
    }

    /// ksig but the signal ∉ noign_sset (SIGWINCH) and it is ignored → normally
    /// ignored: badignore only overrides signals inside the noign set (C
    /// signal.c:483-491).
    #[test]
    fn test_ignored_non_noign_ksig_still_ignored() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = crate::init::sig_bit(SIGWINCH_TEST);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernel { user: 0, sys: 0 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, SIGWINCH_TEST, true, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(table.procs[5].is_in_use() && !table.procs[5].is_exiting());
    }

    /// A signal termination must tell VFS through the caller's real channel
    /// (forkexit.c:350-358 unconditionally calls tell_vfs)—SIGKILL →
    /// VFS_PM_EXIT, SIGSEGV (∈ core_sigs) → VFS_PM_DUMPCORE. Before the fix
    /// these messages went into a one-shot mock black hole, and core-path
    /// processes hung in EXITING forever.
    #[test]
    fn test_signal_termination_tells_vfs() {
        let mut kern = TestKernel { user: 0, sys: 0 };

        // SIGKILL: ordinary termination → VFS_PM_EXIT
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        check_sig(&mut table, UserSlot::new(0), 42, SIGKILL, false, &mut kern, &mut t).unwrap();
        assert!(
            t.sent().iter().any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
            "SIGKILL termination must tell VFS_PM_EXIT, sent={:?}",
            t.sent().iter().map(|(_, m)| m.m_type).collect::<Vec<_>>()
        );

        // SIGSEGV: core signal → VFS_PM_DUMPCORE
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, false, &mut kern, &mut t).unwrap();
        assert!(
            t.sent().iter().any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_DUMPCORE),
            "core signal must send VFS_PM_DUMPCORE, sent={:?}",
            t.sent().iter().map(|(_, m)| m.m_type).collect::<Vec<_>>()
        );
    }

    /// A broadcast SIGTERM notifies RS through the sys_kill(RS) kernel loopback
    /// first (C signal.c:588-588), not through a direct sig_proc call (a
    /// PRIV_PROC !ksig no-op).
    #[test]
    fn test_broadcast_sigterm_notifies_rs_via_kernel() {
        struct RecordKill {
            killed: Option<(Endpoint, i32)>,
        }
        impl crate::exit::KernelGateway for RecordKill {
            fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
            fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
            fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
            fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
            fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
                self.killed = Some((ep, sig));
                Ok(())
            }
            fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
            fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
            fn copy_to_user(&mut self, _b: &[u8], _e: Endpoint, _a: u64) -> Result<(), i32> { Ok(()) }
            fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
            fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
                Ok((0, 0))
            }
            fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        }

        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = RecordKill { killed: None };
        check_sig(&mut table, UserSlot::new(0), -1, SIGTERM, false, &mut kern, &mut t).unwrap();
        assert_eq!(kern.killed, Some((Endpoint::RS, SIGTERM)));
    }

    /// Recording VTimerCtl mock: captures (which, set value).
    struct RecVTimer { pub which: Option<crate::timer::ItimerWhich>, pub set: Option<Clock> }
    impl crate::timer::VTimerCtl for RecVTimer {
        fn vtimer(&mut self, _ep: Endpoint, which: crate::timer::ItimerWhich, set: Option<Clock>, _get: Option<&mut Clock>) -> i32 {
            self.which = Some(which);
            self.set = set;
            0
        }
    }

    #[test]
    fn test_process_ksig_sigvtalrm_restarts_vtimer() {
        // C: signal.c:326-328 + alarm.c:222-241 — ksig SIGVTALRM →
        // check_vtimer: when interval>0, reset ITIMER_VIRTUAL with the kernel.
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 100, true); // kernel process (the ksig path)
        table.procs[5].resources.intervals[crate::timer::ItimerWhich::Virtual as usize] = 50;
        let mut vt = RecVTimer { which: None, set: None };
        let mut t = crate::ipc::TestIpcTransport::default();

        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(1, 5), 26, &mut vt, &mut kern_rec, &mut t);
        assert!(res.is_ok());
        assert_eq!(vt.which, Some(crate::timer::ItimerWhich::Virtual));
        assert_eq!(vt.set, Some(50));
    }

    #[test]
    fn test_process_ksig_sigsys_does_not_touch_vtimer() {
        // Regression guard: the old code wrote `signo == 12` (SIGSYS, not
        // SIGVTALRM=26)—SIGSYS must not touch the virtual timer.
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 100, true);
        let mut vt = RecVTimer { which: None, set: None };
        let mut t = crate::ipc::TestIpcTransport::default();

        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(1, 5), 12, &mut vt, &mut kern_rec, &mut t);
        assert!(res.is_ok());
        assert!(vt.which.is_none(), "SIGSYS must not restart a vtimer");
        assert!(vt.set.is_none());
    }

    #[test]
    fn test_stacktrace_and_termination_match_c_macros() {
        // C: sys/signal.h:279-286 — SIGS_IS_STACKTRACE = LETHAL && !=ABRT;
        // SIGS_IS_TERMINATION = LETHAL || KILL || PIPE.
        assert!(is_stacktrace(SIGILL));
        assert!(is_stacktrace(SIGBUS));
        assert!(is_stacktrace(SIGFPE));
        assert!(is_stacktrace(SIGSEGV));
        assert!(is_stacktrace(SIGEMT));
        assert!(!is_stacktrace(SIGABRT), "ABRT lethal but no stacktrace");
        assert!(!is_stacktrace(9), "SIGKILL not lethal");
        assert!(!is_stacktrace(13), "SIGPIPE not lethal");

        assert!(is_termination(SIGABRT));
        assert!(is_termination(SIGSEGV));
        assert!(is_termination(9));  // SIGKILL
        assert!(is_termination(13)); // SIGPIPE
        assert!(!is_termination(15), "SIGTERM not in C's termination set");
        assert!(!is_termination(2), "SIGINT not termination");
        assert!(!is_termination(SIGSTOP), "SIGSTOP not termination");
    }

    #[test]
    fn test_kill_eperm_for_lethal_priv() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, true); // PRIV_PROC
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut t = crate::ipc::TestIpcTransport::default();
        // Semantic correction: SIGS_IS_LETHAL (sys/signal.h:279-281) =
        // ILL|BUS|FPE|SEGV|EMT|ABRT and does not include SIGKILL—the old
        // approximation that counted 9 as lethal was a real deviation from C
        // (SIGKILL against PRIV_PROC through kill(2) is legal in C). A genuinely
        // lethal SIGSEGV verifies the EPERM protection instead.
        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, false, &mut kern_rec, &mut t); // SIGSEGV lethal, !ksig, PRIV_PROC → EPERM
        assert_eq!(res.unwrap_err(), KillError::PermissionDenied);
    }

    #[test]
    fn test_kill_broadcast() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 100, false);
        mk_proc(&mut table, 6, 43, 100, false);
        mk_proc(&mut table, 7, 44, 200, false);
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0)); // root
        table.procs[0].identity.procgrp = 100;
        let mut kern = TestKernel { user: 0, sys: 0 };
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = check_sig(&mut table, UserSlot::new(0), 0, 15, false, &mut kern, &mut t); // pid 0 → procgrp 100
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 3); // two in group 100 + caller itself (kill(0) includes caller)
    }

    #[test]
    fn test_process_ksig_edeadept() {
        let mut table = ProcTable::new();
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut novt = NopVTimer;
        let mut kern_rec = TestKernel { user: 0, sys: 0 };
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(9, 9), 15, &mut novt, &mut kern_rec, &mut t);
        let _kern_rec = TestKernel { user: 30, sys: 12 };
        assert_eq!(res.unwrap_err(), KillError::InvalidEndpoint);
    }

    // ---- Kernel signal pull loop ----

    /// Kernel-gateway mock with a scripted get_ksig: `ksig_script` is dequeued
    /// one item at a time, and None (= no more on the kernel side) ends it; the
    /// end_ksig acknowledgement sequence is recorded.
    struct KsigRecorder {
        ksig_script: Vec<Option<(minix_types::Endpoint, minix_types::SigSetBits)>>,
        endksig_calls: Vec<(minix_types::Endpoint, i32)>,
    }
    impl crate::exit::KernelGateway for KsigRecorder {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
        fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> {
            Ok(if self.ksig_script.is_empty() { None } else { self.ksig_script.remove(0) })
        }
        fn end_ksig(&mut self, ep: minix_types::Endpoint, sig: i32) -> Result<(), i32> {
            self.endksig_calls.push((ep, sig));
            Ok(())
        }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    }

    /// C sef_signal.c:27-63: the pull loop after SIGKSIG arrives—getksig takes
    /// back (endpt, set), each signal is acknowledged with endksig and handled by
    /// process_ksig, and endpt NONE terminates. SIGTERM(15) goes through
    /// process_ksig → check_sig(ksig=TRUE) → not caught, not ignored → the
    /// termination chain.
    #[test]
    fn test_process_sigmgr_signals_drains_and_delivers() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let target_ep = table.procs[5].identity.endpoint;
        let mut kern = KsigRecorder {
            ksig_script: vec![
                Some((target_ep, [1u32 << (SIGTERM - 1), 0, 0, 0])),
                None,
            ],
            endksig_calls: Vec::new(),
        };
        let mut novt = NopVTimer;
        let mut t = crate::ipc::TestIpcTransport::default();
        process_sigmgr_signals(&mut table, &mut novt, &mut kern, &mut t);
        // Acknowledge and consume signal by signal (C: sys_endksig precedes
        // process_ksig).
        assert_eq!(kern.endksig_calls, vec![(target_ep, SIGTERM)]);
        // SIGTERM's default handling is termination: the process takes the full
        // exit chain to a zombie.
        assert!(
            matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. }),
            "SIGTERM ksig must terminate the clean process, got {:?}",
            table.procs[5].state.lifecycle
        );
        // A zombie must carry the terminating signal number (C: signal.c:552 →
        // forkexit.c:374 → zombify/wait); otherwise the parent's wait reports a
        // signal death as exit 0 by mistake.
        let (_, ss) = table.procs[5].state.lifecycle.exit_code().unwrap();
        assert_eq!(ss as i32, SIGTERM, "zombie sig_status must carry SIGTERM");
    }

    /// A signal death's tell_vfs payload must carry the real signal number.
    /// C: signal.c:552 (sig_proc_exit writes mp_sigstatus before entering
    /// exit_proc) + forkexit.c:351-354 (DUMPCORE carries TERM_SIG). The defect
    /// shape: term_sig read back from the lifecycle as a constant 0 (EXITING is
    /// only established at exit_proc step 10) → VFS's DUMPCORE arm refused
    /// service (BadEndpoint) → PM discarded the error reply → the exit chain
    /// stalled before sys_clear → the kernel slot hung on PAGEFAULT forever and
    /// the suite stopped at 13/36.
    #[test]
    fn test_sig_death_tell_vfs_carries_term_sig() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KsigRecorder {
            ksig_script: Vec::new(),
            endksig_calls: Vec::new(),
        };
        let res = check_sig(
            &mut table,
            UserSlot::new(0),
            42,
            crate::signal::SIGSEGV,
            false,
            &mut kern,
            &mut t,
        );
        assert!(res.is_ok());
        // tell_vfs payload: VFS_PM_DUMPCORE with term_sig = SIGSEGV.
        let call = t
            .sent()
            .iter()
            .find_map(|(_, m)| minix_types::VfsCall::decode(m).ok())
            .expect("signal death must tell VFS");
        let term_sig = match call {
            minix_types::VfsCall::DumpCore { term_sig, .. } => term_sig,
            other => panic!("signal death must send DumpCore, got {other:?}"),
        };
        assert_eq!(term_sig, crate::signal::SIGSEGV);
        // The EXITING lifecycle carries sig_status (the source of exit_restart's
        // zombify).
        assert!(table.procs[5].state.lifecycle.is_exiting());
        let (_, ss) = table.procs[5].state.lifecycle.exit_code().unwrap();
        assert_eq!(
            ss as i32,
            crate::signal::SIGSEGV,
            "sig_status must carry the signal"
        );
    }

    /// C signal.c:344-369: on SIGSNDELAY, clear DELAY_CALL and resume the
    /// handling that was set aside—the ordinary-signal re-check (this verifies
    /// DELAY_CALL clearing and pending delivery).
    #[test]
    fn test_process_ksig_sigsn_delay_resumes_check_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked = Some(crate::mproc::IpcBlockReason::DelayedSignal);
        // Unblocked signals accumulated while the process was set aside.
        table.procs[5].resources.signals.pending = 1u64 << (SIGTERM - 1);
        let mut kern = KsigRecorder { ksig_script: Vec::new(), endksig_calls: Vec::new() };
        let mut novt = NopVTimer;
        let mut t = crate::ipc::TestIpcTransport::default();
        let target_ep = table.procs[5].identity.endpoint;
        let res = process_ksig(&mut table, target_ep, SIGSNDELAY, &mut novt, &mut kern, &mut t);
        // C signal.c:372-378 tail liveness check: after check_pending takes
        // SIGTERM's default handling through the full exit chain, the process
        // holds the EXITING bit (zombify only ORs the bit, never clears it), so
        // `(IN_USE|EXITING) != IN_USE` → EDEADEPT (once is_exiting followed C's
        // bit semantics this matched C; the old assertion's ok was a mirror of
        // the Exiting-only modeling defect).
        assert!(
            matches!(res, Err(KillError::InvalidEndpoint)),
            "terminated process must report EDEADEPT, got {res:?}"
        );
        // DELAY_CALL must be cleared (C signal.c:351); exit_proc's tell_vfs then
        // occupies it with a VFS_CALL (exit.rs step 8), so the whole
        // ipc_blocked field cannot be asserted empty.
        assert!(
            !matches!(
                table.procs[5].state.block.ipc_blocked,
                Some(crate::mproc::IpcBlockReason::DelayedSignal)
            ),
            "DELAY_CALL must clear, got {:?}",
            table.procs[5].state.block.ipc_blocked
        );
        // SIGTERM has been handled (the termination chain).
        assert!(table.procs[5].state.lifecycle.is_exiting() || matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. }));
    }

    // ---- sig_send / unpause real semantics ----

    /// Records the sigmsg contents passed to sys_sigsend.
    struct SigSendRecorder {
        calls: Vec<(minix_types::Endpoint, u32, u64)>,
        reply_err: Option<i32>,
        kills: Vec<(minix_types::Endpoint, i32)>,
    }
    impl crate::exit::KernelGateway for SigSendRecorder {
        fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
            self.kills.push((ep, sig));
            Ok(())
        }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
        fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
        fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_sigsend(&mut self, ep: minix_types::Endpoint, sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> {
            self.calls.push((ep, sigmsg.signo, sigmsg.sighandler));
            match self.reply_err {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    }

    /// C signal.c:772-818: sig_send assembles the sigmsg (mask/signo/handler/
    /// sigreturn) + mp_sigmask bookkeeping (fold in sa_mask + defer the current
    /// signal) + pending clearing + the sys_sigsend delivery.
    #[test]
    fn test_sig_send_books_mask_and_sends() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        // Install a handler (caught + sa_mask with the SIGUSR2 bit) + stopped
        // state.
        let sig = 16; // SIGUSR1 (Minix: USR1=16)
        table.procs[5].state.block.stopped = true;
        // C signal.c:841-844: the non-suspended path comes through
        // restart_sigs, so UNPAUSED must be set.
        table.procs[5].state.block.unpaused = true;
        table.procs[5].resources.signals.caught = 1u64 << (sig - 1);
        table.procs[5].resources.signals.actions[sig as usize - 1].sa_handler = 0xCAFEBABE;
        table.procs[5].resources.signals.actions[sig as usize - 1].sa_mask = 1u64 << (17 - 1);
        table.procs[5].resources.signals.sigreturn_addr = minix_types::VirBytes(0x7000);
        table.procs[5].resources.signals.pending = 1u64 << (sig - 1);
        let mut kern = SigSendRecorder { calls: Vec::new(), reply_err: None, kills: Vec::new() };
        let mut t = crate::ipc::TestIpcTransport::default();

        let res = sig_send(&mut table, UserSlot::new(5), sig, &mut kern, &mut t);
        assert!(res.is_ok());
        // sigmsg: signo/handler/sigreturn field by field (mask =
        // mask|sa_mask|bit16).
        assert_eq!(kern.calls.len(), 1);
        assert_eq!(kern.calls[0].0, table.procs[5].identity.endpoint);
        assert_eq!(kern.calls[0].1, sig as u32);
        assert_eq!(kern.calls[0].2, 0xCAFEBABE);
        // mp_sigmask bookkeeping: sa_mask(17) + itself(16) folded in (C
        // signal.c:800-808). Signal s's bit = 1<<(s-1): 16→0x8000, 17→0x10000.
        assert_eq!(
            table.procs[5].resources.signals.mask & ((1 << 15) | (1 << 16)),
            (1 << 15) | (1 << 16)
        );
        assert_eq!(table.procs[5].resources.signals.pending & (1 << 15), 0);
        // pending has been cleared (C 814-815).
        assert_eq!(table.procs[5].resources.signals.pending & (1 << 15), 0);
    }

    /// C signal.c:820-822: EFAULT/ENOMEM = the process's memory cannot hold the
    /// handler—a legal failure returning FALSE (the caller falls back to
    /// termination), not a panic.
    #[test]
    fn test_sig_send_efault_falls_back() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let sig = 16;
        table.procs[5].state.block.stopped = true;
        table.procs[5].resources.signals.caught = 1u64 << (sig - 1);
        table.procs[5].resources.signals.actions[sig as usize - 1].sa_handler = 0xCAFEBABE;
        let mut kern = SigSendRecorder { calls: Vec::new(), reply_err: Some(-minix_types::EFAULT), kills: Vec::new() };
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = sig_send(&mut table, UserSlot::new(5), sig, &mut kern, &mut t);
        assert!(res.is_err(), "EFAULT must be a graceful FALSE");
    }

    /// C unpause 745-753: WAITING → stop_proc stops it → true (ready to spawn).
    #[test]
    fn test_unpause_waiting_stops_process() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.wait.waiting = true;
        let mut kern = SigSendRecorder { calls: Vec::new(), reply_err: None, kills: Vec::new() };
        let mut t = crate::ipc::TestIpcTransport::default();
        let ready = unpause(&mut table, UserSlot::new(5), &mut kern, &mut t);
        assert!(ready);
        assert!(table.procs[5].state.block.stopped, "WAITING path must stop");
    }

    /// C unpause 760-769: running → stop_proc(MayDefer) + VFS_PM_UNPAUSE request
    /// → false (the signal becomes pending, waiting for the UNPAUSE reply to
    /// redeliver).
    #[test]
    fn test_unpause_running_defers_to_vfs() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut kern = SigSendRecorder { calls: Vec::new(), reply_err: None, kills: Vec::new() };
        let mut t = crate::ipc::TestIpcTransport::default();
        let ready = unpause(&mut table, UserSlot::new(5), &mut kern, &mut t);
        assert!(!ready);
        assert!(table.procs[5].state.block.stopped, "defer path stops first");
        assert_eq!(t.sent().len(), 1, "VFS_PM_UNPAUSE must be requested");
        assert_eq!(t.sent()[0].0, Endpoint::VFS);
        assert_eq!(t.sent()[0].1.m_type, minix_types::VFS_PM_UNPAUSE);
    }

    /// PRIV_PROC !ksig → sys_kill kernel loopback (C signal.c:456-462 "let the
    /// kernel pick the right signal manager").
    #[test]
    fn test_sig_proc_priv_proc_ksig_false_forwards_to_kernel() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, true); // system process
        let mut kern = SigSendRecorder { calls: Vec::new(), reply_err: None, kills: Vec::new() };
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGHUP, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert_eq!(kern.calls.len(), 0, "ksig=false must not use sys_sigsend");
        // Same-shaped forwarding as C: sys_kill(endpoint, SIGHUP)
        // (signal.c:462).
        assert_eq!(kern.kills, vec![(table.procs[5].identity.endpoint, SIGHUP)]);
    }

    #[test]
    fn test_sig_proc_ignored() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = 1u64 << (SIGCHLD - 1);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kr = TestKernel { user: 0, sys: 0 };
        let res = sig_proc(&mut table, UserSlot::new(5), SIGCHLD, false, false, &mut kr, &mut t);
        assert!(res.is_ok());
        // ignored → no pending
        assert_eq!(table.procs[5].resources.signals.pending & (1u64 << (SIGCHLD - 1)), 0);
    }

    // ---- stop_proc wiring for the VFS_CALL branch ----

    /// Kernel-gateway mock that records `sys_delay_stop` calls and supports a
    /// scripted return value: success when `reply == 0`, otherwise that negative
    /// errno is returned as-is (e.g. EBUSY = 16).
    #[derive(Default)]
    struct StopRecorder {
        calls: Vec<Endpoint>,
        reply: i32,
    }
    
    impl crate::exit::KernelGateway for StopRecorder {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, ep: minix_types::Endpoint) -> Result<(), i32> {
            self.calls.push(ep);
            if self.reply == 0 { Ok(()) } else { Err(self.reply) }
        }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    }

    /// C signal.c:425-443: a process suspended in VFS_CALL receives a signal →
    /// pending bit set + stopped by `stop_proc(rmp, FALSE)` (PROC_STOPPED also
    /// serves as the indicator for restart_sigs' re-check). Before the wiring fix
    /// it only set the bit and never stopped.
    #[test]
    fn test_sig_proc_vfs_call_stops_and_sets_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        let bit = 1u64 << (SIGTERM - 1);
        assert_eq!(table.procs[5].resources.signals.pending & bit, bit);
        assert!(table.procs[5].state.block.stopped, "stop_proc must set PROC_STOPPED");
        assert_eq!(kern.calls, vec![table.procs[5].identity.endpoint]);
    }

    /// When ksig=TRUE, also set `mp_ksigpending` (C signal.c:428-429).
    #[test]
    fn test_sig_proc_vfs_call_ksig_sets_kernel_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, true, &mut kern, &mut t);
        assert!(res.is_ok());
        let bit = 1u64 << (SIGTERM - 1);
        assert_eq!(table.procs[5].resources.signals.kernel_pending & bit, bit);
        assert!(table.procs[5].state.block.stopped);
    }

    /// C's guard `!(PROC_STOPPED | DELAY_CALL)`: an already-stopped process does
    /// not call the kernel again (stop_proc's assert forbids it too); only the
    /// pending bit is set.
    #[test]
    fn test_sig_proc_vfs_call_skips_stop_when_already_stopped() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        table.procs[5].state.block.stopped = true;
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(kern.calls.is_empty(), "must not re-stop an already-stopped process");
        let bit = 1u64 << (SIGTERM - 1);
        assert_eq!(table.procs[5].resources.signals.pending & bit, bit);
    }

    /// DELAY_CALL (with no VFS_CALL/EVENT_CALL) does not enter this branch—C
    /// signal.c:425 tests only the two flags. SIGCONT is default-ignored, the
    /// process stays alive, and there are zero kernel calls.
    #[test]
    fn test_sig_proc_delay_call_bypasses_vfs_branch() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked = Some(crate::mproc::IpcBlockReason::DelayedSignal);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGCONT_TEST, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(kern.calls.is_empty());
        assert_eq!(table.procs[5].resources.signals.pending, 0);
        assert_eq!(table.procs[5].state.lifecycle, Lifecycle::Running);
    }

    /// `stop_proc(rmp, FALSE)` = may not delay: a kernel EBUSY must panic (C
    /// signal.c:248 "stop_proc: unexpected delay call").
    #[test]
    fn test_sig_proc_vfs_call_ebusy_must_stop_panics() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder { calls: Vec::new(), reply: 16 }; // EBUSY
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            //[SD19-gate] PM send/notify/reply is fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, false, &mut kern, &mut t);
        }));
        assert!(res.is_err(), "MustStop + EBUSY must panic");
    }

    /// Kernel-gateway mock that records stacktrace requests: used for the
    /// regression of sig_proc's ksig stacktrace arm (C signal.c:464-466
    /// `sys_diagctl_stacktrace(mp_endpoint)`).
    #[derive(Default)]
    struct StacktraceRecorder {
        traces: Vec<minix_types::Endpoint>,
    }
    impl crate::exit::KernelGateway for StacktraceRecorder {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
        fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
        fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_diagctl_stacktrace(&mut self, ep: minix_types::Endpoint) -> Result<(), i32> {
            self.traces.push(ep);
            Ok(())
        }
    }

    /// C signal.c:464-477: a PRIV_PROC process receives a lethal kernel signal
    /// (SIGSEGV is in the stacktrace class)—it first asks the kernel to print the
    /// target's stacktrace, then falls into the termination chain.
    //[SD19-gate] PM send/notify/reply is fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    /// Wiring regression: this arm used to be a `let _ = target;` no-op with no
    /// diagnostic output.
    #[test]
    fn test_sig_proc_ksig_stacktrace_asks_kernel_then_terminates() {
        let mut table = ProcTable::new();
        // Kernel process (is_kernel_process=true, endpoint != PM so it does not
        // return early).
        mk_proc(&mut table, 5, 42, 42, true);
        let target_ep = table.procs[5].identity.endpoint;
        let mut kern = StacktraceRecorder::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, true, &mut kern, &mut t);
        assert!(res.is_ok());
        // ① The stacktrace request really goes out, with the target endpoint =
        // that process's endpoint (C :465).
        assert_eq!(kern.traces, vec![target_ep]);
        // ② SIGSEGV is termination-class: sig_proc_exit follows (C :475-477) and
        // the process leaves the living state.
        assert!(
            matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. })
                || table.procs[5].state.lifecycle.is_exiting(),
            "lethal ksig must terminate kernel proc, got {:?}",
            table.procs[5].state.lifecycle
        );
    }

    /// C signal.c:464 + the `SIGS_IS_STACKTRACE` macro (`sys/sys/signal.h:286`
    /// = `SIGS_IS_LETHAL && sig != SIGABRT`): SIGABRT is lethal but explicitly
    /// excluded from the stacktrace set—no stacktrace request may be sent (paired
    /// with the previous test, pinning the set boundary).
    #[test]
    fn test_sig_proc_ksig_abort_skips_stacktrace() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, true);
        let mut kern = StacktraceRecorder::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = check_sig(&mut table, UserSlot::new(0), 42, SIGABRT, true, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(kern.traces.is_empty(), "SIGABRT must not request stacktrace");
    }
}
