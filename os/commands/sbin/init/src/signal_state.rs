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
use crate::state_machine::sig;
use crate::state_machine::{signal_to_state, StateKind};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Process-wide signal state — what the real kernel-facing trampoline
/// touches. Handlers may only do lock-free atomic stores, so the state
/// is a plain atomic triple and the process-wide instance is a static;
/// C has the same shape (one set of globals, one address, forever).
#[derive(Debug, Default)]
pub struct SignalState {
    clang: AtomicBool,
    requested: AtomicU8,
    shutdown_request: AtomicU8,
}

impl SignalState {
    /// Const constructor: `main` builds the process instance and the
    /// trampoline operates on the static one below.
    pub const fn const_new() -> Self {
        Self {
            clang: AtomicBool::new(false),
            requested: AtomicU8::new(0),
            shutdown_request: AtomicU8::new(0),
        }
    }

    /// The alarm flag (C: `clang`).
    pub fn clang(&self) -> &AtomicBool {
        &self.clang
    }

    /// Handler side of `transition_handler` (init.c:1503-1521): map
    /// the signum and store the requested state; unknown signums store
    /// nothing (C: `requested_transition = 0`). Also routes the alarm
    /// (clang) and the minix hooks so ONE trampoline serves every
    /// catch-style handler.
    pub fn note_signal(&self, signum: i32) {
        match signum {
            sig::SIGNAL_ALARM => self.clang.store(true, Ordering::SeqCst),
            sig::SIGNAL_ABORT => self.note_shutdown_request(ShutdownRequest::Reboot),
            sig::SIGNAL_USER_1 => self.note_shutdown_request(ShutdownRequest::Powerdown),
            _ => {
                if let Some(state) =
                    signal_to_state(crate::state_machine::Signal::from_signum(signum))
                {
                    self.requested.store(state.as_char() as u8, Ordering::SeqCst);
                }
            }
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

    /// Handler-visible take for the death watch (C: reading `clang`).
    pub fn take_clang(&self) -> bool {
        self.clang.swap(false, Ordering::SeqCst)
    }
}

/// The process-wide instance the trampoline writes and the driver
/// drains. Const-built: no runtime initialization, no order races.
pub static SIGNAL_STATE: SignalState = SignalState::const_new();

/// The one catch-all trampoline whose address goes into `sigaction`.
///
/// C installs a distinct handler per signal (`transition_handler`,
/// `alrm_handler`, `minixreboot`, ...); the Rust model routes them all
/// through this single `extern "C"` entry, which only atomically notes
/// the signum — the driver performs every action (spawn
/// `/sbin/shutdown`, run `disaster`) at the next wait boundary
/// (async-signal-safe split, see the module head).
pub extern "C" fn trampoline(signum: i32) {
    SIGNAL_STATE.note_signal(signum);
}

/// The trampoline address for [`SignalSpec`]-style installation.
pub fn trampoline_address() -> usize {
    trampoline as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_signal_stores_transition() {
        let state = SignalState::const_new();
        state.note_signal(1); // SIGHUP → clean_ttys
        assert_eq!(state.take_requested(), Some(StateKind::CleanTtys));
        assert_eq!(state.take_requested(), None);
    }

    #[test]
    fn test_note_signal_ignores_unknown() {
        let state = SignalState::const_new();
        state.note_signal(99);
        assert_eq!(state.take_requested(), None);
    }

    #[test]
    fn test_shutdown_request_roundtrip() {
        let state = SignalState::const_new();
        state.note_shutdown_request(ShutdownRequest::Powerdown);
        assert_eq!(state.take_shutdown_request(), Some(ShutdownRequest::Powerdown));
        assert_eq!(state.take_shutdown_request(), None);
    }

    #[test]
    fn test_clang_flag_set_and_take() {
        let state = SignalState::const_new();
        state.clang().store(true, Ordering::SeqCst);
        assert!(state.take_clang());
        assert!(!state.take_clang());
    }

    #[test]
    fn test_trampoline_routes_alarm_and_hooks() {
        // The extern "C" entry is pure atomic writes — call it directly
        // the way the kernel would.
        trampoline(14); // SIGALRM → clang
        assert!(SIGNAL_STATE.take_clang());
        trampoline(30); // SIGUSR1 → powerdown request
        assert_eq!(SIGNAL_STATE.take_shutdown_request(), Some(ShutdownRequest::Powerdown));
        trampoline(1); // SIGHUP → clean_ttys request
        assert_eq!(SIGNAL_STATE.take_requested(), Some(StateKind::CleanTtys));
    }
}
