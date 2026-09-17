//! Async-written signal state (C: the `clang` and
//! `requested_transition` globals plus the minix hooks).
//!
//! Covers `minix3/sbin/init/init.c:1502-1522` (`transition_handler`),
//! `1649-1655` (`alrm_handler`), and the hook ladders of
//! `minixreboot`/`minixpowerdown` (517-538).
//! Design contract: `.design/02-design.v1.md §1.1-§1.4`, and
//! `.design/14-design.v1.md` for the hooks.
//!
//! The C handlers do real work in signal context — `minixreboot`
//! forks and execs `/sbin/shutdown` from inside the handler. That is
//! the one place this rewrite deliberately diverges in mechanism
//! (external behavior unchanged): a real handler may only touch
//! atomics, so handlers here set pending requests on this state block
//! and the main loop performs the spawn. Same observable order — the
//! request is acted on at the next wait boundary — without the
//! fork-in-handler hazard.

use crate::contracts::ShutdownRequest;
use crate::state_machine::{signal_to_state, StateKind};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

/// Everything a signal handler may touch, shared between the real
/// handler trampolines and the driver loop.
#[derive(Debug, Default)]
pub struct SignalState {
    /// The alarm flag (C: `clang`, init.c:173 — set by
    /// `alrm_handler`). Also used by `death`'s watch.
    pub clang: Arc<AlarmFlagInner>,
    /// Pending transition (C: `requested_transition`, init.c:152):
    /// `Some` state char encoded as its ASCII value.
    requested: AtomicU8,
    /// Pending minix hook (C: what `minixreboot`/`minixpowerdown`
    /// would fork for): 0 none, 1 reboot, 2 powerdown.
    shutdown_request: AtomicU8,
}

/// The alarm flag, arc-shared so handlers and drivers see one bit.
#[derive(Debug, Default)]
pub struct AlarmFlagInner {
    set: AtomicBool,
}

impl AlarmFlagInner {
    pub fn set(&self) {
        self.set.store(true, Ordering::SeqCst);
    }

    /// Read-and-clear.
    pub fn take(&self) -> bool {
        self.set.swap(false, Ordering::SeqCst)
    }
}

impl SignalState {
    /// Handler side of `transition_handler` (init.c:1503-1521): map
    /// the signum and store the requested state; unknown signums
    /// store nothing (C: `requested_transition = 0`).
    pub fn note_signal(&self, signum: i32) {
        if let Some(state) = signal_to_state(crate::state_machine::Signal::from_signum(signum)) {
            self.requested
                .store(state.as_char() as u8, Ordering::SeqCst);
        }
    }

    /// Handler side of `minixreboot`/`minixpowerdown` (init.c:517-538):
    /// note the shutdown flavor; the driver spawns `/sbin/shutdown`.
    pub fn note_shutdown_request(&self, request: ShutdownRequest) {
        let code = match request {
            ShutdownRequest::Reboot => 1,
            ShutdownRequest::Powerdown => 2,
        };
        self.shutdown_request.store(code, Ordering::SeqCst);
    }

    /// Driver side: drain the pending shutdown request, if any.
    pub fn take_shutdown_request(&self) -> Option<ShutdownRequest> {
        match self.shutdown_request.swap(0, Ordering::SeqCst) {
            1 => Some(ShutdownRequest::Reboot),
            2 => Some(ShutdownRequest::Powerdown),
            _ => None,
        }
    }

    /// Driver side: the pending transition, if a signal asked for one
    /// (C: the `requested_transition` reads scattered through the wait
    /// loops).
    pub fn take_requested(&self) -> Option<StateKind> {
        let raw = self.requested.swap(0, Ordering::SeqCst);
        if raw == 0 {
            None
        } else {
            StateKind::from_char(raw as char)
        }
    }

    /// Driver side: peek without draining (some loops only observe).
    pub fn peek_requested(&self) -> Option<StateKind> {
        let raw = self.requested.load(Ordering::SeqCst);
        if raw == 0 {
            None
        } else {
            StateKind::from_char(raw as char)
        }
    }
}

/// Convenience alias for the arc-shared flag used across the crate.
pub type AlarmFlag = AlarmFlagInner;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_signal_stores_transition() {
        let state = SignalState::default();
        state.note_signal(1); // SIGHUP → clean_ttys
        assert_eq!(state.take_requested(), Some(StateKind::CleanTtys));
        assert_eq!(state.take_requested(), None);
    }

    #[test]
    fn test_note_signal_ignores_unknown() {
        let state = SignalState::default();
        state.note_signal(99);
        assert_eq!(state.take_requested(), None);
    }

    #[test]
    fn test_shutdown_request_roundtrip() {
        let state = SignalState::default();
        state.note_shutdown_request(ShutdownRequest::Powerdown);
        assert_eq!(state.take_shutdown_request(), Some(ShutdownRequest::Powerdown));
        assert_eq!(state.take_shutdown_request(), None);
    }

    #[test]
    fn test_clang_flag_set_and_take() {
        let state = SignalState::default();
        state.clang.set();
        assert!(state.clang.take());
        assert!(!state.clang.take());
    }
}
