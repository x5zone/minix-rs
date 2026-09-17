//! State machine skeleton for init.
//!
//! Covers `minix3/sbin/init/init.c:130-140` (state chars), `369-409`
//! (`handle`/`delset`), `624-640` (`transition`), `1502-1522`
//! (`transition_handler`), `1649-1655` (`alrm_handler`).
//! Design contract: `.design/02-design.v1.md §1.1-§1.4`.

/// Minix3 signal numbers init reacts to.
///
/// Names and values mirror the single authority
/// `minix-types/src/types/signal.rs` (anchored to
/// `minix3/sys/sys/signal.h`), held locally because command crates read
/// shared constants through `minix-sys`, which does not re-export the
/// family yet — the switch to `minix_sys::signal` is registered as an
/// E-INITSYS follow-up and is a one-line import change here.
pub mod sig {
    /// C: `SIGHUP 1` (`signal.h:52`).
    pub const SIGNAL_HANGUP: i32 = 1;
    /// C: `SIGABRT 6` (`signal.h:57`).
    pub const SIGNAL_ABORT: i32 = 6;
    /// C: `SIGKILL 9` (`signal.h:61`).
    pub const SIGNAL_KILL: i32 = 9;
    /// C: `SIGALRM 14` (`signal.h:66`).
    pub const SIGNAL_ALARM: i32 = 14;
    /// C: `SIGTERM 15` (`signal.h:67`).
    pub const SIGNAL_TERMINATE: i32 = 15;
    /// C: `SIGTSTP 18` (`signal.h:70`).
    pub const SIGNAL_TERMINAL_STOP: i32 = 18;
    /// C: `SIGCONT 19` (`signal.h:71`) — resume a stopped shell.
    pub const SIGNAL_CONTINUE: i32 = 19;
    /// C: `SIGUSR1 30` (`signal.h:82`) — Minix3 numbering; 10 is SIGBUS.
    pub const SIGNAL_USER_1: i32 = 30;
}

use sig::{SIGNAL_ALARM, SIGNAL_ABORT, SIGNAL_HANGUP, SIGNAL_TERMINAL_STOP, SIGNAL_TERMINATE, SIGNAL_USER_1};

/// The seven init states (C: `DEATH`..`CATATONIA`, init.c:133-139).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateKind {
    Death,
    SingleUser,
    Runcom,
    ReadTtys,
    MultiUser,
    CleanTtys,
    Catatonia,
}

impl StateKind {
    /// C character for this state (init.c:133-139).
    pub fn as_char(self) -> char {
        match self {
            StateKind::Death => 'd',
            StateKind::SingleUser => 's',
            StateKind::Runcom => 'r',
            StateKind::ReadTtys => 't',
            StateKind::MultiUser => 'm',
            StateKind::CleanTtys => 'T',
            StateKind::Catatonia => 'c',
        }
    }

    /// Inverse of [`StateKind::as_char`]; unknown chars yield `None`.
    pub fn from_char(c: char) -> Option<StateKind> {
        match c {
            'd' => Some(StateKind::Death),
            's' => Some(StateKind::SingleUser),
            'r' => Some(StateKind::Runcom),
            't' => Some(StateKind::ReadTtys),
            'm' => Some(StateKind::MultiUser),
            'T' => Some(StateKind::CleanTtys),
            'c' => Some(StateKind::Catatonia),
            _ => None,
        }
    }
}

/// Signals init reacts to, plus a bucket for the rest.
///
/// Signum values come from the Minix3 `<sys/signal.h>` authority (see
/// [`sig`], e.g. SIGUSR1 = 30 — not the Linux x86 value 10); variants
/// carry no numbers themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    Sighup,
    Sigterm,
    Sigtstp,
    Sigalrm,
    Sigabrt,
    Sigusr1,
    Other(i32),
}

impl Signal {
    /// The Minix3 signum of this signal.
    pub fn signum(self) -> i32 {
        match self {
            Signal::Sighup => SIGNAL_HANGUP,
            Signal::Sigterm => SIGNAL_TERMINATE,
            Signal::Sigtstp => SIGNAL_TERMINAL_STOP,
            Signal::Sigalrm => SIGNAL_ALARM,
            Signal::Sigabrt => SIGNAL_ABORT,
            Signal::Sigusr1 => SIGNAL_USER_1,
            Signal::Other(n) => n,
        }
    }

    /// Classify a raw signum; unknown numbers stay in [`Signal::Other`].
    pub fn from_signum(n: i32) -> Signal {
        match n {
            SIGNAL_HANGUP => Signal::Sighup,
            SIGNAL_TERMINATE => Signal::Sigterm,
            SIGNAL_TERMINAL_STOP => Signal::Sigtstp,
            SIGNAL_ALARM => Signal::Sigalrm,
            SIGNAL_ABORT => Signal::Sigabrt,
            SIGNAL_USER_1 => Signal::Sigusr1,
            other => Signal::Other(other),
        }
    }
}

/// Which handler owns a signal (C: one `handle()` line per group, or
/// `SIG_IGN` in the pre-fork windows of `single_user`/`runetcrc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    Transition,
    Alarm,
    Reboot,
    Powerdown,
    Disaster,
    /// C: `sa_handler = SIG_IGN` — a temporary ignore, not a handler
    /// table entry.
    Ignore,
}

/// Map a signal to the requested state (C: `transition_handler`, init.c:1502-1522).
///
/// `SIGHUP → CleanTtys`, `SIGTERM → Death`, `SIGTSTP → Catatonia`;
/// anything else yields `None` (C: `requested_transition = 0`).
pub fn signal_to_state(sig: Signal) -> Option<StateKind> {
    match sig {
        Signal::Sighup => Some(StateKind::CleanTtys),
        Signal::Sigterm => Some(StateKind::Death),
        Signal::Sigtstp => Some(StateKind::Catatonia),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_chars_roundtrip() {
        for state in [
            StateKind::Death,
            StateKind::SingleUser,
            StateKind::Runcom,
            StateKind::ReadTtys,
            StateKind::MultiUser,
            StateKind::CleanTtys,
            StateKind::Catatonia,
        ] {
            assert_eq!(StateKind::from_char(state.as_char()), Some(state));
        }
        assert_eq!(StateKind::from_char('x'), None);
    }

    #[test]
    fn test_signal_to_state_maps() {
        assert_eq!(signal_to_state(Signal::Sighup), Some(StateKind::CleanTtys));
        assert_eq!(signal_to_state(Signal::Sigterm), Some(StateKind::Death));
        assert_eq!(signal_to_state(Signal::Sigtstp), Some(StateKind::Catatonia));
    }

    #[test]
    fn test_signal_to_state_default_none() {
        assert_eq!(signal_to_state(Signal::Sigalrm), None);
        assert_eq!(signal_to_state(Signal::Other(99)), None);
    }

    #[test]
    fn test_signum_roundtrip_matches_minix3_numbering() {
        // Minix3 <sys/signal.h>: SIGUSR1 is 30 (SIGBUS owns 10), the
        // value Linux x86 numbers differently.
        assert_eq!(Signal::Sighup.signum(), 1);
        assert_eq!(Signal::Sigtstp.signum(), 18);
        assert_eq!(Signal::Sigusr1.signum(), 30);
        for sig in [
            Signal::Sighup,
            Signal::Sigterm,
            Signal::Sigtstp,
            Signal::Sigalrm,
            Signal::Sigabrt,
            Signal::Sigusr1,
        ] {
            assert_eq!(Signal::from_signum(sig.signum()), sig);
        }
        assert_eq!(Signal::from_signum(99), Signal::Other(99));
    }

}
