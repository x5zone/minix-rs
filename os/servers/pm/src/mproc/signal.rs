//! Signal handling state definition.
//!
//! Provides process signal mask, pending signals, and other management.

use alloc::boxed::Box;
use minix_types::VirBytes;

/// Signal set (64-bit unsigned integer).
///
/// In 64-bit systems, supports up to 64 signals.
pub type SigSet = u64;

/// Number of signals.
pub const _NSIG: usize = 64;

/// Signal handling state.
///
/// Stores process signal-related information.
///
/// # Minix3 Mapping
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

impl SignalState {
    /// Creates new signal state (default: no blocked signals).
    pub fn new() -> Self {
        Self::default()
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
        assert!(!state.has_pending());
        assert!(!state.suspended);
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
