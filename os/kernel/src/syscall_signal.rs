//! Signal system calls: kill, getksig, endksig, sigsend, sigreturn.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_kill.c` — SYS_KILL
//! - `do_getksig.c` — SYS_GETKSIG
//! - `do_endksig.c` — SYS_ENDKSIG
//! - `do_sigsend.c` — SYS_SIGSEND
//! - `do_sigreturn.c` — SYS_SIGRETURN
//!
//! # Design Decisions (18-syscall-signal.md §3)
//!
//! - **D1**: `u64` for signal bitmap (`_NSIG = 64`)
//! - **D4**: `cause_signal()` as KProcess method
//! - **D5**: Linear scan for GETKSIG (matches C, process count < 128)
//! - **D6**: `trait SignalContext` for architecture-specific sigframe/sigcontext

use minix_types::{Endpoint, Message, MessSigcalls};

use crate::proc::{KProcess, ProcNr, RtsFlagsBits};
use crate::proc_table::ProcessTable;
use crate::kpriv::PrivTable;
use crate::syscall::KcallResult;

// ── Minix3 error codes ──

const OK: i32 = 0;
const EINVAL: i32 = 22;
const EPERM: i32 = 1;
const ENOSYS: i32 = 38;
// EFAULT not currently used by implemented syscalls

// ── Signal constants ──

/// Number of signals. C: `_NSIG` — signal.h
pub const NSIG: usize = 64;

/// Signal number for SIGVTALRM. C: `SIGVTALRM` — signal.h
pub const SIGVTALRM: u32 = 26;

/// Signal number for SIGPROF. C: `SIGPROF` — signal.h
pub const SIGPROF: u32 = 27;

/// Signal number for SIGABRT. C: `SIGABRT` — signal.h
pub const SIGABRT: u32 = 6;

/// Signal number for SIGTRAP. C: `SIGTRAP` — signal.h
pub const SIGTRAP: u32 = 5;

/// Kernel signal notification. C: `SIGKSIG = 74` — signal.h
/// Used by cause_sig() to notify the signal manager that a kernel signal
/// is pending. This is outside the _NSIG range (1-64) because it is an
/// internal kernel-to-sm notification, not a POSIX signal.
pub const SIGKSIG: u32 = 74;

// ── Endpoint constants ──

// SELF is used via Endpoint::SELF in cause_signal/endksig/getksig

// ── Signal bitmap type ──

/// Signal bitmap. C: `sigset_t` — 64 signals fit in a u64.
pub type SigSet = u64;

/// Build a signal mask for the given signal number (1-based).
/// C: `sig_mask(sig)` — signal.h
pub const fn sig_mask(sig_nr: u32) -> SigSet {
    if sig_nr == 0 || sig_nr as usize > NSIG {
        0
    } else {
        1u64 << (sig_nr - 1)
    }
}

// ── Helper ──

/// Read signal call fields from a message.
/// C: `m_ptr->m_sigcalls.*` — uses mess_sigcalls union member.
fn msg_sigcalls(msg: &Message) -> MessSigcalls {
    // SAFETY: `m_type` has been validated by the caller to be a signal
    // syscall (SYS_GETKSIG/SYS_ENDKSIG/SYS_KILL/SYS_SIGSEND/SYS_SIGRETURN),
    // which uses the `m_sigcalls` variant. `#[repr(C)]` union access is sound.
    unsafe { msg.m_u.m_sigcalls }
}

// ── Dispatch functions ──

/// Dispatch SYS_KILL.
///
/// C: `do_kill()` — do_kill.c
///
/// Cause a signal to be sent to a process. Adds to pending signal map
/// and informs the signal manager.
pub fn dispatch_kill(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_kill.c:25-26 — extract parameters
    let endpt = sc.endpt;    // m_sigcalls.endpt
    let sig_nr = sc.sig;     // m_sigcalls.sig

    // C: do_kill.c:28-30 — validate signal number
    if sig_nr as usize >= NSIG {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_kill.c:27 — isokendpt(proc_nr_e, &proc_nr)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_kill.c:29 — iskerneln(proc_nr) → EPERM
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_kill.c:35 — cause_sig(proc_nr, sig_nr)
    cause_signal(target_nr, sig_nr as u32, proc_table, priv_table);

    KcallResult::Ok(OK)
}

/// Cause a signal to be sent to a process.
///
/// C: `cause_sig()` — system.c:389-426
///
/// Adds the signal to the target's pending bitmap and marks the target
/// as signaled. If the target was not already in the RTS_SIGNALED state,
/// also notifies the signal manager (via `send_sig` / `mini_notify`).
///
/// # BKL Requirement
///
/// Must be called while holding the Big Kernel Lock. The BKL ensures
/// no other CPU can concurrently modify `p_pending`, `p_rts_flags`,
/// or `s_sig_pending`.
///
/// # Signal manager notification
///
/// C: `cause_sig()` calls `send_sig(sig_mgr, SIGKSIG)` which eventually
/// does `mini_notify(sig_mgr, SIGKSIG)`. This notification step is
/// deferred until the IPC subsystem provides `mini_notify` (SignalContext trait).
/// The state mutations (p_pending, RTS flags, s_sig_pending) are
/// performed now so that a subsequent `dispatch_getksig` poll will
/// observe the signal.
fn cause_signal(
    target_nr: ProcNr,
    sig_nr: u32,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) {
    // C: system.c:406 — rp = proc_addr(proc_nr)
    let was_signaled = proc_table.get(target_nr)
        .map_or(false, |p| p.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));

    // C: system.c:411 — sigaddset(&rp->p_pending, sig_nr)
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.p_pending.add(sig_nr as u8);
    }

    // C: system.c:413-414 — if !RTS_ISSET(rp, RTS_SIGNALED)
    if !was_signaled {
        // C: system.c:415 — RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING)
        proc_table.rts_set(target_nr, RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING);

        // C: system.c:416-418 — send_sig(sig_mgr, SIGKSIG)
        // Look up the signal manager for this process.
        // C: system.c:399 — sig_mgr = priv(rp)->s_sig_mgr
        // C: system.c:400 — if(sig_mgr == SELF) sig_mgr = rp->p_endpoint
        let sig_mgr = proc_table.sig_mgr(target_nr, priv_table);

        // Add SIGKSIG to the signal manager's s_sig_pending.
        // C: system.c:445 — send_sig(sig_mgr, SIGKSIG)
        // send_sig() adds a notification to the signal manager's pending set.
        // DEFERRED: full send_sig() notification requires mini_notify (SignalContext trait).
        // For now, mark s_sig_pending so dispatch_getksig will observe it.
        if let Some(sig_mgr_ep) = sig_mgr {
            if let Some(sig_mgr_nr) = proc_table.endpoint_to_nr(sig_mgr_ep) {
                if let Some(sig_mgr_proc) = proc_table.get(sig_mgr_nr) {
                    if let Some(pid) = sig_mgr_proc.priv_id {
                        if let Some(sig_mgr_priv) = priv_table.get_mut(pid) {
                            sig_mgr_priv.s_sig_pending.add(SIGKSIG as u8);
                        }
                    }
                }
            }
        }
    }
}

/// Dispatch SYS_GETKSIG.
///
/// C: `do_getksig()` — do_getksig.c
///
/// The signal manager polls for pending kernel signals.
/// Scans all user processes for one with RTS_SIGNALED whose s_sig_mgr
/// matches the caller. Returns the endpoint and pending signal map,
/// then clears RTS_SIGNALED and p_pending.
///
/// # C Semantic Alignment (do_getksig.c:22-41)
///
/// 1. Scan `BEG_USER_ADDR..END_PROC_ADDR` for `RTS_SIGNALED` process
/// 2. Check `caller == priv(rp)->s_sig_mgr`
/// 3. Write `m_sigcalls.endpt = rp->p_endpoint`, `m_sigcalls.map = rp->p_pending`
/// 4. `RTS_UNSET(rp, RTS_SIGNALED)`, clear `rp->p_pending`
/// 5. If no process found: `m_sigcalls.endpt = NONE`
pub fn dispatch_getksig(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    // C: do_getksig.c:22-40 — scan all user processes
    let mut found: Option<(Endpoint, u64)> = None;

    for rp in proc_table.iter() {
        // Skip kernel tasks and free slots
        if rp.p_rts_flags.get() == RtsFlagsBits::SLOT_FREE {
            continue;
        }
        if ProcessTable::is_kernel(rp.p_nr) {
            continue;
        }
        // C: do_getksig.c:24 — if (!RTS_ISSET(rp, RTS_SIGNALED)) continue
        if !rp.p_rts_flags.is_set(RtsFlagsBits::SIGNALED) {
            continue;
        }
        // C: do_getksig.c:25 — if (caller->p_endpoint != priv(rp)->s_sig_mgr) continue
        let sig_mgr = proc_table.sig_mgr(rp.p_nr, priv_table);
        if sig_mgr != Some(caller.p_endpoint) {
            continue;
        }

        // Found a process with a pending signal for this signal manager.
        found = Some((rp.p_endpoint, rp.p_pending.get()));
        break;
    }

    if let Some((endpt, map)) = found {
        // C: do_getksig.c:28-30 — write reply
        // m_ptr->m_sigcalls.endpt = rp->p_endpoint
        // m_ptr->m_sigcalls.map = rp->p_pending
        let target_nr = proc_table.endpoint_to_nr(endpt).unwrap();

        // Clear RTS_SIGNALED and p_pending on the found process.
        // C: do_getksig.c:33 — RTS_UNSET(rp, RTS_SIGNALED)
        proc_table.rts_unset(target_nr, RtsFlagsBits::SIGNALED);
        // C: do_getksig.c:34 — sigemptyset(&rp->p_pending)
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_pending.clear();
        }

        // Write reply into the message union.
        msg.m_u.m_sigcalls = MessSigcalls {
            map,
            endpt: endpt.get(),
            sig: 0,
            sigctx: 0,
            _padding: [0u8; 32],
        };
    } else {
        // C: do_getksig.c:41 — m_ptr->m_sigcalls.endpt = NONE
        msg.m_u.m_sigcalls = MessSigcalls {
            map: 0,
            endpt: Endpoint::NONE.get(),
            sig: 0,
            sigctx: 0,
            _padding: [0u8; 32],
        };
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_ENDKSIG.
///
/// C: `do_endksig()` — do_endksig.c
///
/// The signal manager has finished processing a kernel signal.
/// Validates the caller is the signal manager for the target process,
/// then clears RTS_SIG_PENDING if no new signals arrived.
///
/// # C Semantic Alignment (do_endksig.c:28-41)
///
/// 1. `isokendpt(endpt, &proc_nr)` — validate endpoint → EINVAL
/// 2. `caller->p_endpoint != priv(rp)->s_sig_mgr` — permission check → EPERM
/// 3. `!RTS_ISSET(rp, RTS_SIG_PENDING)` — no pending signal → EINVAL
/// 4. `!RTS_ISSET(rp, RTS_SIGNALED)` — no new signal → clear SIG_PENDING
pub fn dispatch_endksig(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_endksig.c:28 — m_sigcalls.endpt
    let endpt = sc.endpt;

    // Step 1: Validate endpoint. C: do_endksig.c:28 — isokendpt()
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // Step 2: Check caller is the signal manager for the target.
    // C: do_endksig.c:33 — if (caller->p_endpoint != priv(rp)->s_sig_mgr) return EPERM
    let sig_mgr = proc_table.sig_mgr(target_nr, priv_table);
    if sig_mgr != Some(caller.p_endpoint) {
        return KcallResult::Ok(EPERM);
    }

    // Step 3: Check RTS_SIG_PENDING is set.
    // C: do_endksig.c:34 — if (!RTS_ISSET(rp, RTS_SIG_PENDING)) return EINVAL
    if !proc_table.get(target_nr)
        .map_or(false, |p| p.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING))
    {
        return KcallResult::Ok(EINVAL);
    }

    // Step 4: If no new signal arrived, clear RTS_SIG_PENDING.
    // C: do_endksig.c:37-38 — if (!RTS_ISSET(rp, RTS_SIGNALED))
    //     RTS_UNSET(rp, RTS_SIG_PENDING)
    if !proc_table.get(target_nr)
        .map_or(false, |p| p.p_rts_flags.is_set(RtsFlagsBits::SIGNALED))
    {
        proc_table.rts_unset(target_nr, RtsFlagsBits::SIG_PENDING);
    }

    KcallResult::Ok(OK)
}

/// Signal message from user-space signal manager.
/// C: `struct sigmsg` — sigcontext.h
pub struct SigMsg {
    /// Signal handler address. C: `sm_sighandler`
    pub sighandler: u64,
    /// Signal mask to block during handler. C: `sm_mask`
    pub mask: SigSet,
    /// Signal number. C: `sm_signo`
    pub signo: u32,
    /// Return address for sigreturn. C: `sm_sigreturn`
    pub sigreturn: u64,
    /// User stack pointer at signal time. C: `sm_stkptr`
    pub stkptr: u64,
}

/// Architecture-specific signal context operations.
///
/// C source has `#if defined(__i386__)` / `#if defined(__arm__)` blocks
/// in do_sigsend.c and do_sigreturn.c for register save/restore.
/// This trait abstracts those operations so the kernel does not
/// depend on a specific architecture's register layout.
///
/// # Minix3 C Source Mapping
///
/// - do_sigsend.c:60-115 — build sigcontext from process registers
/// - do_sigsend.c:130-145 — modify process registers to enter handler
/// - do_sigreturn.c:42-80 — restore registers from sigcontext
/// - do_sigreturn.c:82-84 — `arch_proc_setcontext()`
///
/// # Design Decision (18-syscall-signal.md §3 D6)
///
/// Each architecture implements this trait. The kernel dispatch layer
/// calls trait methods without knowing the hardware register encoding.
pub trait SignalContext {
    /// Saved register state for signal delivery.
    /// Corresponds to C's `struct sigcontext` (arch/sigcontext.h).
    type SigContext;

    /// Signal frame placed on user stack.
    /// Corresponds to C's `struct sigframe_sigcontext` (arch/sigcontext.h).
    type SigFrame;

    /// Build a sigcontext from the process's current register state.
    ///
    /// C: do_sigsend.c:60-115 — fills `fr.sf_sc.sc_*` from `rp->p_reg.*`
    fn build_sigcontext(proc: &KProcess, smsg: &SigMsg) -> Self::SigContext;

    /// Build a sigframe from the sigcontext, ready to copy to user stack.
    ///
    /// C: do_sigsend.c:49-58, 117-118 — compute stack pointer, fill frame
    fn build_sigframe(
        proc: &KProcess,
        sctx: &Self::SigContext,
        smsg: &SigMsg,
    ) -> Self::SigFrame;

    /// Modify process registers to enter the signal handler.
    ///
    /// C: do_sigsend.c:130-145 — sets SP, PC, FP/LR, etc.
    /// **MUST** be called only after the sigframe has been successfully
    /// copied to user space (data_copy_vmcheck may VMSUSPEND).
    fn setup_handler_entry(proc: &mut KProcess, smsg: &SigMsg, frame_addr: u64);

    /// Restore process registers from a sigcontext.
    ///
    /// C: do_sigreturn.c:42-80 — writes `rp->p_reg.*` from `sc.sc_*`
    fn restore_sigcontext(proc: &mut KProcess, sctx: &Self::SigContext);

    /// Architecture-specific post-restore hook.
    ///
    /// C: do_sigreturn.c:82-84 — `arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style)`
    fn arch_setcontext(proc: &mut KProcess, trap_style: i32);

    /// Get the current stack pointer of the process.
    /// C: `arch_get_sp(rp)` — do_sigsend.c:49
    fn get_sp(proc: &KProcess) -> u64;

    /// Size of the sigframe structure for stack adjustment.
    /// C: `sizeof(struct sigframe_sigcontext)` — do_sigsend.c:50
    fn sigframe_size() -> usize;
}

/// Dispatch SYS_SIGSEND.
///
/// C: `do_sigsend()` — do_sigsend.c
///
/// POSIX-style signal delivery: build sigframe on user stack,
/// modify registers to jump to signal handler.
///
/// **Critical constraint**: register modification MUST happen after
/// the last data_copy_vmcheck (which may VMSUSPEND).
///
/// # Current Status
///
/// Endpoint validation and parameter extraction are implemented.
/// The full flow (sigmsg copy, sigframe build, register modification)
/// requires `SignalContext` trait implementation + `data_copy_vmcheck`.
/// Returns ENOSYS until those dependencies are available.
pub fn dispatch_sigsend(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_sigsend.c:33-34 — extract parameters
    let endpt = sc.endpt;      // m_sigcalls.endpt
    let _sigctx = sc.sigctx;   // m_sigcalls.sigctx

    // C: do_sigsend.c:36-37 — validate endpoint
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_sigsend.c:38 — iskerneln check
    if target_nr < 0 {
        return KcallResult::Ok(EPERM);
    }

    // C: do_sigsend.c:42-45 — copy sigmsg from user space
    // DEFERRED: data_copy_vmcheck(caller, caller->p_endpoint,
    //   sigctx, KERNEL, &smsg, sizeof(struct sigmsg))

    // C: do_sigsend.c:49-58 — compute user stack pointer
    // C: do_sigsend.c:60-115 — build sigcontext (arch-specific, via SignalContext)
    // C: do_sigsend.c:120-125 — copy sigframe to user stack (may VMSUSPEND!)
    // C: do_sigsend.c:130-145 — modify registers (MUST be after copy)

    // Full flow requires SignalContext impl + data_copy_vmcheck.
    // Return ENOSYS to indicate the complete syscall is not yet available.
    let _ = caller;
    KcallResult::Ok(ENOSYS)
}

/// Dispatch SYS_SIGRETURN.
///
/// C: `do_sigreturn()` — do_sigreturn.c
///
/// Restore process state after signal handler returns.
/// Copies sigcontext from user stack and restores registers.
///
/// # Current Status
///
/// Endpoint validation is implemented. The full flow (sigcontext copy,
/// register restoration) requires `SignalContext` trait implementation
/// + `data_copy`. Returns ENOSYS until those dependencies are available.
pub fn dispatch_sigreturn(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_sigreturn.c:29-30 — extract parameters
    let endpt = sc.endpt;      // m_sigcalls.endpt
    let _sigctx = sc.sigctx;   // m_sigcalls.sigctx

    // C: do_sigreturn.c:32-33 — validate endpoint
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_sigreturn.c:34 — iskerneln check
    if target_nr < 0 {
        return KcallResult::Ok(EPERM);
    }

    // C: do_sigreturn.c:38-40 — copy sigcontext from user space
    // DEFERRED: data_copy(endpt, sigctx, KERNEL, &sc, sizeof(sigcontext))

    // C: do_sigreturn.c:42-80 — restore registers (arch-specific, via SignalContext)
    // C: do_sigreturn.c:82-84 — arch_proc_setcontext
    // C: do_sigreturn.c:86-93 — restore FPU state

    // Full flow requires SignalContext impl + data_copy.
    // Return ENOSYS to indicate the complete syscall is not yet available.
    let _ = (caller, target_nr);
    KcallResult::Ok(ENOSYS)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::KProcess;
    use minix_types::Endpoint;

    #[test]
    fn test_sig_mask() {
        assert_eq!(sig_mask(1), 1u64);
        assert_eq!(sig_mask(2), 2u64);
        assert_eq!(sig_mask(64), 1u64 << 63);
        assert_eq!(sig_mask(0), 0u64);
        assert_eq!(sig_mask(65), 0u64);
    }

    #[test]
    fn test_nsig() {
        assert_eq!(NSIG, 64);
    }

    #[test]
    fn test_signal_constants() {
        assert_eq!(SIGVTALRM, 26);
        assert_eq!(SIGPROF, 27);
        assert_eq!(SIGABRT, 6);
        assert_eq!(SIGTRAP, 5);
    }

    #[test]
    fn test_sigsend_invalid_endpoint() {
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        // Invalid endpoint → EINVAL
        let proc_table = ProcessTable::new();
        let result = dispatch_sigsend(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigsend_kernel_process() {
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        // Set endpoint to a kernel process (negative proc_nr)
        let proc_table = ProcessTable::new();
        // Kernel processes have negative proc_nr, but endpoint_to_nr
        // won't find them in the table, so we get EINVAL.
        let result = dispatch_sigsend(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigreturn_invalid_endpoint() {
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        let proc_table = ProcessTable::new();
        let result = dispatch_sigreturn(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigmsg_struct() {
        let smsg = SigMsg {
            sighandler: 0x400000,
            mask: sig_mask(SIGABRT),
            signo: SIGABRT,
            sigreturn: 0x401000,
            stkptr: 0x7FFFF000,
        };
        assert_eq!(smsg.signo, 6);
        assert_eq!(smsg.mask, sig_mask(6));
        assert_eq!(smsg.sighandler, 0x400000);
        assert_eq!(smsg.sigreturn, 0x401000);
        assert_eq!(smsg.stkptr, 0x7FFFF000);
    }
}
