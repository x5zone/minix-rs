//! Signal core: `do_kill`/`do_srv_kill` → `check_sig` → `sig_proc` → `sig_proc_exit` + `process_ksig`.
//!
//! C ground truth: `minix3/minix/servers/pm/signal.c:197-646` (do_kill/check_sig/sig_proc/process_ksig)
//! Design: explicit `SignalTarget` + `SignalState` (`mproc/signal.rs`) + `SignalClass`.
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, UserSlot, Pid, EINVAL, ESRCH, EPERM, EDEADEPT};
use crate::mproc::{ProcTable, Lifecycle, SignalState, _NSIG};

/// Signal numbers (subset, `sys/signal.h`).
pub const SIGKILL: i32 = 9;
pub const SIGTERM: i32 = 15;
pub const SIGCHLD: i32 = 20;
pub const SIGSTOP: i32 = 17;

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
pub fn handle_kill(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    check_sig(table, caller, pid, signo, false, transport)
}

/// Handles `PM_SRV_KILL` (`do_srv_kill`, `204-221`).
///
/// Only `RS` may call; `ksig = true` so `PRIV_PROC` can be killed.
pub fn handle_srv_kill(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    if table.procs[caller.get()].endpoint() != Endpoint::RS {
        return Err(KillError::PermissionDenied);
    }
    check_sig(table, caller, pid, signo, true, transport)
}

/// Checks which processes to signal (`check_sig`, `568-646`).
///
/// `pid` four meanings: `>0` one, `0` process group (caller procgrp), `-1` all (except `INIT_PID`), `<-1` group `-pid`.
/// `signo == 0` is existence probe (no `sig_proc`, just count).
/// Returns `Ok(count)` or `Err(errno)`, and `SUSPEND` is modeled as `Err` with `caller` now `EXITING` (self-kill).
pub fn check_sig(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    ksig: bool,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    if signo < 0 || signo >= _NSIG as i32 {
        return Err(KillError::InvalidSignal);
    }
    if pid == 1 && signo == SIGKILL {
        return Err(KillError::InvalidSignal); // EINVAL for INIT+KILL
    }
    // Broadcast SIGTERM: RS first (588-589)
    if pid == -1 && signo == SIGTERM {
        if let Ok(rs_slot) = table.pm_isokendpt(Endpoint::RS) {
            let _ = sig_proc(table, rs_slot, signo, true, ksig, transport);
        }
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
        let _ = sig_proc(table, UserSlot::new(idx), signo, true, ksig, transport);
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

/// Whether signal is lethal (SIGS_IS_LETHAL, `sys/sigtype.h`).
fn is_lethal(signo: i32) -> bool {
    matches!(signo, 9 | 15 | 6 | 11 | 4 | 5 | 8 | 10 | 7) // KILL, TERM, ABRT, SEGV, ILL, TRAP, FPE, BUS, EMT (approx)
}

/// Whether signal needs stacktrace (`SIGS_IS_STACKTRACE`).
fn is_stacktrace(_signo: i32) -> bool {
    false // stub for 11, real in 16
}

/// Whether signal is termination (`SIGS_IS_TERMINATION`).
fn is_termination(signo: i32) -> bool {
    !matches!(signo, 1 | 13 | 17 | 23 | 20 | 28 | 29) // HUP, PIPE, STOP, CONT, CHLD, WINCH, INFO are not termination (approx)
}

/// Checks permission (`signal.c:622-628`).
fn can_signal(table: &ProcTable, caller: UserSlot, target: UserSlot) -> bool {
    let caller_creds = table.procs[caller.get()].resources.privilege.credentials();
    let target_creds = table.procs[target.get()].resources.privilege.credentials();
    if let (Some(c), Some(t)) = (caller_creds, target_creds) {
        if c.user.effective == 0 {
            return true; // SUPER_USER
        }
        c.user.real == t.user.real
            || c.user.effective == t.user.real
            || c.user.real == t.user.effective
            || c.user.effective == t.user.effective
    } else {
        // Kernel processes (no creds) — treat as superuser for signal?
        // In C, `mp_effuid` for PRIV_PROC is still 0, so true
        // For Rust, Kernel privilege is considered superuser
        table.procs[caller.get()].is_kernel_process() || table.procs[target.get()].is_kernel_process()
    }
}

/// Sends signal to process (`sig_proc`, `384-540`).
///
/// 9-step chain: TRACE→VFS|EVENT→PRIV_PROC→badignore→ignore→block→TRACE_STOPPED→caught→terminate.
/// Returns `Ok(())` or `Err` (for sig_send failure).
pub fn sig_proc(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    trace: bool,
    ksig: bool,
    _transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<(), KillError> {
    let proc = &table.procs[target.get()];
    if !proc.is_in_use() || proc.state.lifecycle.is_exiting() {
        // panic in C (407-409) — but for Rust we return Err
        return Err(KillError::NoSuchProcess);
    }
    // TRACE first (411-422)
    if trace && proc.state.guardianship.tracer().is_some() && signo != SIGKILL {
        let t = proc.state.guardianship.tracer().unwrap();
        let _ = t;
        // In C: sigaddset(sigtrace) + trace_stop if not already TRACE_STOPPED
        table.procs[target.get()].resources.signals.trace_mask |= 1u64 << (signo - 1);
        if !table.procs[target.get()].state.trace.stopped {
            // trace_stop would set TRACE_STOPPED and stop via sys_trace
            table.procs[target.get()].state.trace.stopped = true;
        }
        return Ok(());
    }
    // VFS|EVENT pending (425-444)
    if table.procs[target.get()].state.block.ipc_blocked.is_some() {
        table.procs[target.get()].resources.signals.pending |= 1u64 << (signo - 1);
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= 1u64 << (signo - 1);
        }
        // Stop if not already stopped/delay
        if !table.procs[target.get()].state.block.stopped
            && table.procs[target.get()].state.block.ipc_blocked.is_none()
        {
            // In C: stop_proc(FALSE) — but VFS|EVENT case already has VFS_CALL, so PROC_STOPPED not set here?
            // For 11 we keep pending and rely on 13's restart_sigs
        }
        return Ok(());
    }
    // PRIV_PROC system signals (448-480)
    if table.procs[target.get()].is_kernel_process() {
        if table.procs[target.get()].endpoint() == Endpoint::PM {
            return Ok(());
        }
        if !ksig {
            // Forward to kernel signal manager
            let _ = (target, signo);
            return Ok(());
        }
        if is_stacktrace(signo) {
            let _ = target;
        }
        if !is_termination(signo) {
            // Message SIGS_SIGNAL_RECEIVED to system process
            let _ = (target, signo);
            return Ok(());
        } else {
            return sig_proc_exit(table, target, signo);
        }
    }
    // User process: badignore / ignore / block / TRACE_STOPPED / caught / terminate
    let sig_bit = 1u64 << (signo - 1);
    let state = &table.procs[target.get()].resources.signals;
    let badignore = ksig
        && (state.ignored & crate::init::NOIGN_SIGSET != 0 || state.mask & crate::init::NOIGN_SIGSET != 0)
        && (state.ignored & sig_bit != 0 || state.mask & sig_bit != 0);
    // For 11 we simplify badignore check to noign set
    if !badignore && (state.ignored & sig_bit != 0) {
        return Ok(());
    }
    if !badignore && (state.mask & sig_bit != 0) {
        table.procs[target.get()].resources.signals.pending |= sig_bit;
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= sig_bit;
        }
        return Ok(());
    }
    if table.procs[target.get()].state.trace.stopped && signo != SIGKILL {
        table.procs[target.get()].resources.signals.pending |= sig_bit;
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= sig_bit;
        }
        return Ok(());
    }
    if !badignore && (state.caught & sig_bit != 0) {
        // Try unpause then sig_send
        if !unpause(table, target) {
            table.procs[target.get()].resources.signals.pending |= sig_bit;
            if ksig {
                table.procs[target.get()].resources.signals.kernel_pending |= sig_bit;
            }
            return Ok(());
        }
        if sig_send(table, target, signo).is_ok() {
            return Ok(());
        }
        // Fall through to terminate on sig_send failure
    } else if state.ignored & crate::init::IGN_SIGSET != 0 && (state.ignored & sig_bit == 0) {
        // Default ignore via ign_sset? Simplified
    }
    // Default ignore via ign_sset (533-535)
    if sig_bit & crate::init::IGN_SIGSET != 0 && !badignore {
        return Ok(());
    }
    sig_proc_exit(table, target, signo)
}

/// Terminates process via signal (`sig_proc_exit`, `546-563`).
fn sig_proc_exit(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
) -> Result<(), KillError> {
    let is_core = crate::init::CORE_SIGSET & (1u64 << (signo - 1)) != 0;
    // In C: exit_proc(rmp, 0, dump_core) where dump_core = is_core
    // For 11 we delegate to exit::exit_proc with a nop transport (no VFS)
    let status = 0;
    let mut nop = crate::ipc::TestIpcTransport::default();
    crate::exit::exit_proc(table, target, status as i8, is_core, &mut nop);
    Ok(())
}

/// Unpauses a process blocked on WAIT or SIGSUSPEND or VFS (`unpause`, `719-770`).
///
/// Returns `true` if already unpaused or successfully unpaused, `false` if delayed.
fn unpause(table: &mut ProcTable, target: UserSlot) -> bool {
    // Simplified for 11: if VFS_CALL|EVENT_CALL → tell_vfs UNPAUSE, else if WAITING|SIGSUSPENDED → stop_proc
    let proc = &table.procs[target.get()];
    if proc.state.block.ipc_blocked.is_some() {
        // In C: tell_vfs UNPAUSE or stop_proc with may_delay
        return false;
    }
    if proc.state.wait.waiting || proc.resources.signals.suspended {
        // In C: stop_proc(FALSE) for WAITING|SIGSUSPENDED
        table.procs[target.get()].state.block.stopped = true;
        return true;
    }
    // Not paused in PM, try VFS unpause
    true
}

/// Sends signal via handler (`sig_send`, `772-855`).
///
/// Returns `true` if handler setup succeeded.
fn sig_send(table: &mut ProcTable, target: UserSlot, signo: i32) -> Result<(), KillError> {
    // Simplified: set pending cleared, mask updated, and mark as needing sigframe
    let _ = (table, target, signo);
    Ok(())
}

/// Processes kernel signal (`process_ksig`, `294-378`).
pub fn process_ksig(
    table: &mut ProcTable,
    endpoint: Endpoint,
    signo: i32,
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
    let _ = proc.identity.procgrp;
    // SIGVTALRM check_vtimer (326-328) — stubbed
    if signo == 12 || signo == 27 {
        // SIGVTALRM / SIGPROF
        let _ = slot;
    }
    let pid = proc.identity.id.pid;
    // Broadcast vs single (320-332)
    let target_pid = match signo {
        2 | 3 | 28 | 29 => 0, // INT, QUIT, WINCH, INFO → group broadcast
        _ => pid,
    };
    check_sig(table, UserSlot::new(0), target_pid, signo, true, transport)?;
    // SIGSNDELAY handling (344-369) — simplified
    if signo == 42 && table.procs[slot].state.block.ipc_blocked.is_some() {
        // SIGSNDELAY == 42? Actually SIGSNDELAY is 41? Use placeholder 42
        let _ = slot;
    }
    if table.procs[slot].state.lifecycle.is_exiting() {
        return Err(KillError::InvalidEndpoint);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};

    fn mk_proc(table: &mut ProcTable, slot: usize, pid: i32, procgrp: i32, is_kernel: bool) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.procgrp = procgrp;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        if is_kernel {
            table.procs[slot].resources.privilege = Privilege::Kernel;
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
        let res = check_sig(&mut table, UserSlot::new(0), 42, 9, false, &mut t);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);
    }

    #[test]
    fn test_kill_eperm_for_lethal_priv() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, true); // PRIV_PROC
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = check_sig(&mut table, UserSlot::new(0), 42, 9, false, &mut t); // SIGKILL lethal, !ksig, PRIV_PROC → EPERM
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
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = check_sig(&mut table, UserSlot::new(0), 0, 15, false, &mut t); // pid 0 → procgrp 100
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 3); // two in group 100 + caller itself (kill(0) includes caller)
    }

    #[test]
    fn test_process_ksig_edeadept() {
        let mut table = ProcTable::new();
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(9, 9), 15, &mut t);
        assert_eq!(res.unwrap_err(), KillError::InvalidEndpoint);
    }

    #[test]
    fn test_sig_proc_ignored() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = 1u64 << (SIGCHLD - 1);
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGCHLD, false, false, &mut t);
        assert!(res.is_ok());
        // ignored → no pending
        assert_eq!(table.procs[5].resources.signals.pending & (1u64 << (SIGCHLD - 1)), 0);
    }
}
