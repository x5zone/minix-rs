//! Signal handling state definition.
//!
//! Provides process signal mask, pending signals, and other management.

use alloc::boxed::Box;
use minix_types::VirBytes;

/// Signal numbers (subset, `sys/signal.h`).
pub const SIGKILL: i32 = 9;
pub const SIGSTOP: i32 = 17;

/// SA flags (`sys/signal.h:152-153`).
pub const SA_RESETHAND: i32 = 0x04;
pub const SA_NODEFER: i32 = 0x10;

/// sigprocmask `how` values (`sys/signal.h:174-179`).
pub const SIG_BLOCK: i32 = 1;
pub const SIG_UNBLOCK: i32 = 2;
pub const SIG_SETMASK: i32 = 3;
pub const SIG_INQUIRE: i32 = 10;

/// Mask of unkillable signals (`SIGKILL` + `SIGSTOP`).
///
/// C: `signal.c:80-81/124-125/141-142/166-167/186-187` five `sigdelset(KILL/STOP)` sites
/// converged to one method (D3). Bit `signo-1`.
pub const UNKILLABLE_MASK: SigSet = (1u64 << (SIGKILL as u64 - 1)) | (1u64 << (SIGSTOP as u64 - 1));

/// Extension for `SigSet` without unkillable signals.
pub trait SigSetExt {
    fn without_unkillable(self) -> SigSet;
}

impl SigSetExt for SigSet {
    #[inline]
    fn without_unkillable(self) -> SigSet {
        self & !UNKILLABLE_MASK
    }
}

/// Signal set (64-bit unsigned integer).
///
/// In 64-bit systems, supports up to 64 signals.
///
/// # Bit Semantics
/// Bit `signo - 1` corresponds to signal `signo` (1-based), matching Minix3's
/// `__sigismember(s, n)` which uses bit `(n - 1) & 31` (`sys/sigtypes.h`).
pub type SigSet = u64;

/// Number of signals.
pub const _NSIG: usize = 64;

/// Signal handling state.
///
/// Stores process signal-related information.
///
/// # Minix3 Mapping
/// - `mp_ignore` → `ignored`
/// - `mp_catch` → `caught`
/// - `mp_sigmask` → `mask`
/// - `mp_sigmask2` → `mask_saved`
/// - `mp_sigpending` → `pending`
/// - `mp_ksigpending` → `kernel_pending`
/// - `mp_sigtrace` → `trace_mask`
/// - `SIGSUSPENDED` → `suspended`
/// - `mp_sigreturn` → `sigreturn_addr`
/// - `mp_sigact[]` → `actions`
#[derive(Debug, Clone)]
pub struct SignalState {
    /// Signals set to be ignored (mp_ignore).
    ///
    /// Maintained by `do_sigaction` (signal.c:68-77): SIG_IGN adds, SIG_DFL
    /// and user handlers remove.
    pub ignored: SigSet,
    /// Signals with a user handler (mp_catch).
    ///
    /// Maintained by `do_sigaction`; consumed by `exec.c:179-180` (reset on
    /// exec) and `check_sig` (signal.c:505-509).
    pub caught: SigSet,
    /// Signal mask (blocked signals).
    pub mask: SigSet,
    /// Saved signal mask (for sigsuspend restore).
    pub mask_saved: SigSet,
    /// Pending signals.
    pub pending: SigSet,
    /// Kernel pending signals.
    pub kernel_pending: SigSet,
    /// Trace signal mask.
    pub trace_mask: SigSet,
    /// Whether in sigsuspend state (SIGSUSPENDED).
    pub suspended: bool,
    /// sigreturn function address.
    pub sigreturn_addr: VirBytes,
    /// Signal actions (corresponds to Minix3's `mp_sigact[]`).
    ///
    /// Stored on heap to avoid large stack allocations.
    pub actions: Box<[SigAction; _NSIG]>,
}

impl Default for SignalState {
    fn default() -> Self {
        Self {
            ignored: 0,
            caught: 0,
            mask: 0,
            mask_saved: 0,
            pending: 0,
            kernel_pending: 0,
            trace_mask: 0,
            suspended: false,
            sigreturn_addr: VirBytes(0),
            actions: Box::new([SigAction::default(); _NSIG]),
        }
    }
}

/// Signal handling action.
///
/// Corresponds to C's `struct sigaction`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SigAction {
    /// Signal handler address or special value.
    ///
    /// - `0` (SIG_DFL): Default handling
    /// - `1` (SIG_IGN): Ignore
    /// - Other: User-defined handler address
    pub sa_handler: usize,
    /// Signals blocked during handling.
    pub sa_mask: SigSet,
    /// Signal handling flags.
    pub sa_flags: i32,
}

impl Default for SigAction {
    fn default() -> Self {
        Self {
            sa_handler: 0,
            sa_mask: 0,
            sa_flags: 0,
        }
    }
}

/// Handler disposition (`SIG_DFL=0` / `SIG_IGN=1` / handler address).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigHandler {
    Default,
    Ignore,
    Catch(VirBytes),
}

impl SigHandler {
    pub fn from_raw(handler: usize) -> Self {
        match handler {
            0 => Self::Default,
            1 => Self::Ignore,
            v => Self::Catch(VirBytes(v as u64)),
        }
    }
    pub fn to_raw(self) -> usize {
        match self {
            Self::Default => 0,
            Self::Ignore => 1,
            Self::Catch(v) => v.0 as usize,
        }
    }
}

/// `sigprocmask` operation (`signal.c:122-152` D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigMaskOp {
    Block,
    Unblock,
    SetMask,
    Inquire,
}

impl SigMaskOp {
    pub fn try_from_how(how: i32) -> Option<Self> {
        match how {
            SIG_BLOCK => Some(Self::Block),
            SIG_UNBLOCK => Some(Self::Unblock),
            SIG_SETMASK => Some(Self::SetMask),
            SIG_INQUIRE => Some(Self::Inquire),
            _ => None,
        }
    }
}

/// Effect of `apply_mask_op` (whether `check_pending` is needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskOpEffect {
    Unchanged,
    Changed { needs_check: bool },
}

/// Sigmsg forwarded to kernel (`type.h:73` D7/D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigMsg {
    pub mask: SigSet,
    pub signo: i32,
    pub handler: VirBytes,
    pub sigreturn: VirBytes,
}

impl SignalState {
    /// Creates new signal state (default: no blocked signals).
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs handler for `signo` (D2: atomic four-bitmap transaction).
    ///
    /// C: `signal.c:67-78` (`SIG_IGN` four-link, `SIG_DFL` two-clear, `catch`).
    /// `signo` is 1-based (1..64). Returns `false` if out of range.
    pub fn install(&mut self, signo: u32, handler: SigHandler, mask: SigSet, flags: i32, sigreturn: VirBytes) -> bool {
        if signo == 0 || signo > 64 {
            return false;
        }
        let bit = 1u64 << (signo - 1);
        match handler {
            SigHandler::Ignore => {
                self.ignored |= bit;
                self.pending &= !bit;
                self.kernel_pending &= !bit;
                self.caught &= !bit;
            }
            SigHandler::Default => {
                self.ignored &= !bit;
                self.caught &= !bit;
            }
            SigHandler::Catch(_) => {
                self.ignored &= !bit;
                self.caught |= bit;
            }
        }
        // Store action (mask without unkillable per D3).
        let idx = (signo - 1) as usize;
        if idx < _NSIG {
            self.actions[idx].sa_handler = handler.to_raw();
            self.actions[idx].sa_mask = mask.without_unkillable();
            self.actions[idx].sa_flags = flags;
        }
        self.sigreturn_addr = sigreturn;
        true
    }

    /// Applies mask operation (D4).
    pub fn apply_mask_op(&mut self, op: SigMaskOp, set: SigSet) -> MaskOpEffect {
        match op {
            SigMaskOp::Block => {
                let stripped = set.without_unkillable();
                self.mask |= stripped;
                MaskOpEffect::Changed { needs_check: false }
            }
            SigMaskOp::Unblock => {
                // Unblock does not strip KILL/STOP (see 1.4).
                for i in 1..=64 {
                    if (set & (1u64 << (i - 1))) != 0 {
                        self.mask &= !(1u64 << (i - 1));
                    }
                }
                MaskOpEffect::Changed { needs_check: true }
            }
            SigMaskOp::SetMask => {
                self.mask = set.without_unkillable();
                MaskOpEffect::Changed { needs_check: true }
            }
            SigMaskOp::Inquire => MaskOpEffect::Unchanged,
        }
    }

    /// Prepares for `sigsuspend` (D5: save mask2 + new mask + suspended).
    pub fn prepare_suspend(&mut self, new_mask: SigSet) {
        self.mask_saved = self.mask;
        self.mask = new_mask.without_unkillable();
        self.suspended = true;
    }

    /// Restores mask for `sigreturn` (D6).
    pub fn restore_for_sigreturn(&mut self, set: SigSet) {
        self.mask = set.without_unkillable();
    }

    /// Prepares `SigMsg` for `sig_send` (D7: four-step mask evolution).
    ///
    /// Returns `SigMsg` and mutates `caught/pending` per `SA_RESETHAND`/`pending` clear.
    /// Caller must hold `PROC_STOPPED` (asserted in `signal_handlers.rs`).
    pub fn prepare_sigmsg(&mut self, signo: u32) -> Option<SigMsg> {
        if signo == 0 || signo > 64 {
            return None;
        }
        let signo_i = signo as i32;
        let idx = (signo - 1) as usize;
        if idx >= _NSIG {
            return None;
        }
        let action = self.actions[idx];
        let bit = 1u64 << (signo - 1);

        // Step 1: base mask (mask2 if suspended else mask, signal.c:792-795).
        let mut sm_mask = if self.suspended {
            self.mask_saved
        } else {
            self.mask
        };
        // Step 2: overlay sa_mask (800-803).
        sm_mask |= action.sa_mask;

        // Step 3: SA_NODEFER vs default current-signal block (805-808).
        if (action.sa_flags & SA_NODEFER) != 0 {
            sm_mask &= !bit;
        } else {
            sm_mask |= bit;
        }

        // Step 4: SA_RESETHAND (810-813) — mutate caught/handler.
        if (action.sa_flags & SA_RESETHAND) != 0 {
            self.caught &= !bit;
            self.actions[idx].sa_handler = 0; // SIG_DFL
        }

        // Clear pending (814-815) before sys_sigsend.
        self.pending &= !bit;
        self.kernel_pending &= !bit;

        Some(SigMsg {
            mask: sm_mask,
            signo: signo_i,
            handler: VirBytes(action.sa_handler as u64),
            sigreturn: self.sigreturn_addr,
        })
    }

    /// Snapshot of pending set (for `do_sigpending`).
    #[inline]
    pub fn pending_snapshot(&self) -> SigSet {
        self.pending
    }

    /// Finds smallest `pending & !mask` signal (1..64) with its `ksig` flag.
    ///
    /// C: `signal.c:664-667` `for i=1.._NSIG if pending && !mask` + `ksig=ksigpending(i)`.
    #[inline]
    pub fn next_unblocked(&self) -> Option<(u32, bool)> {
        let unblocked = self.pending & !self.mask;
        if unblocked == 0 {
            return None;
        }
        let signo = unblocked.trailing_zeros() + 1;
        let ksig = (self.kernel_pending >> (signo - 1) & 1) != 0;
        Some((signo as u32, ksig))
    }

    /// Clears `pending` and `kernel_pending` for `signo` (668-669).
    #[inline]
    pub fn take_pending(&mut self, signo: u32) {
        if signo == 0 || signo > 64 {
            return;
        }
        let b = 1u64 << (signo - 1);
        self.pending &= !b;
        self.kernel_pending &= !b;
    }

    /// `exec` resets caught handlers (`exec.c:178-184`, D4, A-2).
    pub fn reset_caught_for_exec(&mut self) {
        for sn in 1..64 {
            let bit = 1u64 << (sn - 1);
            if (self.caught & bit) != 0 {
                self.caught &= !bit;
                let idx = (sn - 1) as usize;
                self.actions[idx].sa_handler = 0; // SIG_DFL
                self.actions[idx].sa_mask = 0;
                self.actions[idx].sa_flags = 0;
            }
        }
    }
    
    /// Checks if there are pending signals.
    pub fn has_pending(&self) -> bool {
        self.pending != 0 || self.kernel_pending != 0
    }
    
    /// Checks if signal is blocked.
    ///
    /// # Parameters
    /// - `signo`: Signal number (1-64)
    ///
    /// # Returns
    /// Returns `true` if signal is blocked.
    pub fn is_blocked(&self, signo: u32) -> bool {
        if signo == 0 || signo > 64 {
            return false;
        }
        (self.mask & (1u64 << (signo - 1))) != 0
    }
    
    /// Checks if signal is set to be ignored (mp_ignore).
    pub fn is_ignored(&self, signo: u32) -> bool {
        if signo == 0 || signo > 64 {
            return false;
        }
        (self.ignored & (1u64 << (signo - 1))) != 0
    }
    
    /// Checks if signal has a user handler (mp_catch).
    pub fn is_caught(&self, signo: u32) -> bool {
        if signo == 0 || signo > 64 {
            return false;
        }
        (self.caught & (1u64 << (signo - 1))) != 0
    }
    
    /// Adds pending signal.
    ///
    /// # Parameters
    /// - `signo`: Signal number (1-64)
    /// - `from_kernel`: Whether from kernel
    pub fn add_pending(&mut self, signo: u32, from_kernel: bool) {
        if signo == 0 || signo > 64 {
            return;
        }
        let bit = 1u64 << (signo - 1);
        self.pending |= bit;
        if from_kernel {
            self.kernel_pending |= bit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signal_state_default() {
        let state = SignalState::default();
        assert_eq!(state.mask, 0);
        assert_eq!(state.pending, 0);
        assert_eq!(state.ignored, 0);
        assert_eq!(state.caught, 0);
        assert!(!state.has_pending());
        assert!(!state.suspended);
    }

    #[test]
    fn test_without_unkillable_removes_kill_stop() {
        let mask = (1u64 << (SIGKILL as u64 - 1)) | (1u64 << (SIGSTOP as u64 - 1)) | (1u64 << 5);
        let stripped = mask.without_unkillable();
        assert_eq!(stripped & (1u64 << (SIGKILL as u64 - 1)), 0);
        assert_eq!(stripped & (1u64 << (SIGSTOP as u64 - 1)), 0);
        assert_ne!(stripped & (1u64 << 5), 0);
        assert_eq!(UNKILLABLE_MASK, (1u64 << 8) | (1u64 << 16));
    }

    #[test]
    fn test_sigaction_ignore_clears_pending_and_catch() {
        let mut state = SignalState::default();
        state.pending = 1u64 << 2;
        state.kernel_pending = 1u64 << 2;
        state.caught = 1u64 << 2;
        state.install(3, SigHandler::Ignore, 0, 0, VirBytes(0));
        assert!(state.is_ignored(3));
        assert!(!state.is_caught(3));
        assert_eq!(state.pending & (1u64 << 2), 0);
        assert_eq!(state.kernel_pending & (1u64 << 2), 0);
    }

    #[test]
    fn test_sigaction_dfl_keeps_pending() {
        let mut state = SignalState::default();
        state.pending = 1u64 << 2;
        state.caught = 1u64 << 2;
        state.install(3, SigHandler::Default, 0, 0, VirBytes(0));
        assert!(!state.is_ignored(3));
        assert!(!state.is_caught(3));
        assert_ne!(state.pending & (1u64 << 2), 0);
    }

    #[test]
    fn test_sigaction_catch_sets_caught() {
        let mut state = SignalState::default();
        state.ignored = 1u64 << 2;
        state.install(3, SigHandler::Catch(VirBytes(0x1000)), 0, 0, VirBytes(0));
        assert!(state.is_caught(3));
        assert!(!state.is_ignored(3));
    }

    #[test]
    fn test_apply_mask_op_block_no_check() {
        let mut state = SignalState::default();
        let eff = state.apply_mask_op(SigMaskOp::Block, 1u64 << 5);
        assert_eq!(eff, MaskOpEffect::Changed { needs_check: false });
        assert!(state.is_blocked(6));
    }

    #[test]
    fn test_apply_mask_op_unblock_needs_check() {
        let mut state = SignalState::default();
        state.mask = 1u64 << 5;
        let eff = state.apply_mask_op(SigMaskOp::Unblock, 1u64 << 5);
        assert_eq!(eff, MaskOpEffect::Changed { needs_check: true });
        assert!(!state.is_blocked(6));
    }

    #[test]
    fn test_apply_mask_op_setmask() {
        let mut state = SignalState::default();
        let eff = state.apply_mask_op(SigMaskOp::SetMask, 1u64 << 5);
        assert_eq!(eff, MaskOpEffect::Changed { needs_check: true });
        assert!(state.is_blocked(6));
    }

    #[test]
    fn test_apply_mask_op_inquire() {
        let mut state = SignalState::default();
        state.mask = 1u64 << 5;
        let eff = state.apply_mask_op(SigMaskOp::Inquire, 0);
        assert_eq!(eff, MaskOpEffect::Unchanged);
        assert!(state.is_blocked(6));
    }

    #[test]
    fn test_prepare_suspend_saves_mask2() {
        let mut state = SignalState::default();
        state.mask = 1u64 << 5;
        state.prepare_suspend(1u64 << 6);
        assert_eq!(state.mask_saved, 1u64 << 5);
        assert!(state.suspended);
        assert!(state.is_blocked(7));
        assert!(!state.is_blocked(6));
    }

    #[test]
    fn test_prepare_sigmsg_uses_mask2_when_suspended() {
        let mut state = SignalState::default();
        state.mask = 1u64 << 5;
        state.mask_saved = 1u64 << 6;
        state.suspended = true;
        state.actions[2].sa_handler = 0x2000;
        state.actions[2].sa_mask = 0;
        state.actions[2].sa_flags = 0;
        let msg = state.prepare_sigmsg(3).unwrap();
        // base is mask_saved (bit 6), plus current signal bit 2
        assert!(msg.mask & (1u64 << 6) != 0);
        assert!(msg.mask & (1u64 << 2) != 0);
    }

    #[test]
    fn test_prepare_sigmsg_sa_nodefer_and_resethand() {
        let mut state = SignalState::default();
        state.caught = 1u64 << 2;
        state.actions[2].sa_handler = 0x3000;
        state.actions[2].sa_mask = 0;
        state.actions[2].sa_flags = SA_NODEFER | SA_RESETHAND;
        state.pending = 1u64 << 2;
        state.kernel_pending = 1u64 << 2;
        let msg = state.prepare_sigmsg(3).unwrap();
        // NODEFER clears current signal bit
        assert_eq!(msg.mask & (1u64 << 2), 0);
        // RESETHAND clears caught and handler
        assert!(!state.is_caught(3));
        assert_eq!(state.actions[2].sa_handler, 0);
        // pending cleared
        assert_eq!(state.pending & (1u64 << 2), 0);
        assert_eq!(state.kernel_pending & (1u64 << 2), 0);
    }

    #[test]
    fn test_sig_handler_default_ignore_catch() {
        assert_eq!(SigHandler::from_raw(0), SigHandler::Default);
        assert_eq!(SigHandler::from_raw(1), SigHandler::Ignore);
        assert_eq!(SigHandler::from_raw(0x1000), SigHandler::Catch(VirBytes(0x1000)));
        assert_eq!(SigHandler::Default.to_raw(), 0);
        assert_eq!(SigHandler::Ignore.to_raw(), 1);
    }
    
    #[test]
    fn test_is_blocked() {
        let mut state = SignalState::default();
        
        assert!(!state.is_blocked(1));
        assert!(!state.is_blocked(0));
        assert!(!state.is_blocked(65));
        
        state.mask = 0b101;
        assert!(state.is_blocked(1));
        assert!(!state.is_blocked(2));
        assert!(state.is_blocked(3));
    }
    
    #[test]
    fn test_ignored_caught() {
        let mut state = SignalState::default();
        
        assert!(!state.is_ignored(1));
        assert!(!state.is_caught(1));
        assert!(!state.is_ignored(0));
        assert!(!state.is_caught(65));
        
        // bit (signo - 1): signal 2 → bit 1
        state.ignored = 0b10;
        state.caught = 0b100;
        assert!(state.is_ignored(2));
        assert!(!state.is_ignored(1));
        assert!(state.is_caught(3));
        assert!(!state.is_caught(2));
    }
    
    #[test]
    fn test_add_pending() {
        let mut state = SignalState::default();
        
        state.add_pending(1, false);
        assert!(state.has_pending());
        assert_eq!(state.pending, 1);
        assert_eq!(state.kernel_pending, 0);
        
        state.add_pending(2, true);
        assert_eq!(state.pending, 0b11);
        assert_eq!(state.kernel_pending, 0b10);
    }
    
    #[test]
    fn test_add_pending_invalid() {
        let mut state = SignalState::default();
        
        state.add_pending(0, false);
        state.add_pending(65, false);
        assert!(!state.has_pending());
    }
    
    #[test]
    fn test_sig_action() {
        let action = SigAction {
            sa_handler: 0x1000,
            sa_mask: 0xFF,
            sa_flags: 0,
        };
        assert_eq!(action.sa_handler, 0x1000);
        assert_eq!(action.sa_mask, 0xFF);
    }
}
