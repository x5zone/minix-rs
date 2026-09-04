//! State machine skeleton for init.
//!
//! Covers `minix3/sbin/init/init.c:130-140` (state chars), `369-409`
//! (`handle`/`delset`), `624-640` (`transition`), `1502-1522`
//! (`transition_handler`), `1649-1655` (`alrm_handler`).
//! Design contract: `.design/02-design.v1.md §1.1-§1.4`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Signals relevant to the transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    Sighup,
    Sigterm,
    Sigstp,
    Sigalrm,
    Sigabrt,
    Sigusr1,
    Other(i32),
}

/// Which handler owns a signal (C: one `handle()` line per group).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    Transition,
    Alarm,
    Reboot,
    Powerdown,
    Disaster,
}

/// Map a signal to the requested state (C: `transition_handler`, init.c:1502-1522).
///
/// `SIGHUP → CleanTtys`, `SIGTERM → Death`, `SIGTSTP → Catatonia`;
/// anything else yields `None` (C: `requested_transition = 0`).
pub fn signal_to_state(sig: Signal) -> Option<StateKind> {
    match sig {
        Signal::Sighup => Some(StateKind::CleanTtys),
        Signal::Sigterm => Some(StateKind::Death),
        Signal::Sigstp => Some(StateKind::Catatonia),
        _ => None,
    }
}

/// Registration errors (defer until `minix_sys` signal syscalls land).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    /// Live registration needs real `sigaction` (ARCH A-8 gap).
    Deferred,
}

/// Signal registration boundary (C: `handle`/`delset`, init.c:369-405).
pub trait SignalRegistry {
    fn register(&mut self, handler: HandlerKind, sigs: &[Signal]) -> Result<(), RegistryError>;
    fn block_all_except(&mut self, except: &[Signal]);
    fn registered(&self, sig: Signal) -> Option<HandlerKind>;
}

/// In-memory fake registry for tests.
#[derive(Debug, Default)]
pub struct FakeSignalRegistry {
    table: HashMap<Signal, HandlerKind>,
    pub blocked_except: Vec<Signal>,
}

impl SignalRegistry for FakeSignalRegistry {
    fn register(&mut self, handler: HandlerKind, sigs: &[Signal]) -> Result<(), RegistryError> {
        for sig in sigs {
            self.table.insert(*sig, handler);
        }
        Ok(())
    }

    fn block_all_except(&mut self, except: &[Signal]) {
        self.blocked_except = except.to_vec();
    }

    fn registered(&self, sig: Signal) -> Option<HandlerKind> {
        self.table.get(&sig).copied()
    }
}

/// Live registry placeholder (needs real `sigaction`; ARCH A-8).
#[derive(Debug, Default)]
pub struct LiveSignalRegistry {
    table: HashMap<Signal, HandlerKind>,
}

impl SignalRegistry for LiveSignalRegistry {
    fn register(&mut self, handler: HandlerKind, sigs: &[Signal]) -> Result<(), RegistryError> {
        // Real implementation will call sigaction per signal with
        // sa_mask = full set and SA_NOCLDSTOP for SIGCHLD-equivalents
        // (init.c:380-386). Unavailable until minix_sys lands.
        // Record intent so callers can observe it, then report deferral.
        for sig in sigs {
            self.table.insert(*sig, handler);
        }
        Err(RegistryError::Deferred)
    }

    fn block_all_except(&mut self, _except: &[Signal]) {
        // Deferred with register(); no observable state yet.
    }

    fn registered(&self, sig: Signal) -> Option<HandlerKind> {
        self.table.get(&sig).copied()
    }
}

/// Alarm flag (C: `clang`, init.c:173; set by `alrm_handler`, init.c:1649-1655).
///
/// Atomic so a real signal handler can set it asynchronously.
#[derive(Debug, Default)]
pub struct AlarmFlag {
    inner: AtomicBool,
}

impl AlarmFlag {
    pub fn set(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }

    pub fn take(&self) -> bool {
        self.inner.swap(false, Ordering::SeqCst)
    }
}

/// Drives the `transition()` loop (C: init.c:624-640).
///
/// `step` runs one state function and returns the next state, or `None`
/// to stop (C: state function returning NULL). `run` iterates up to
/// `max_steps` so tests can bound the infinite loop.
pub trait TransitionDriver {
    fn step(&mut self, current: StateKind) -> Option<StateKind>;

    fn run(&mut self, initial: StateKind, max_steps: Option<usize>) -> Vec<StateKind> {
        let mut trace = vec![initial];
        let mut current = initial;
        let limit = max_steps.unwrap_or(usize::MAX);
        for _ in 0..limit {
            match self.step(current) {
                Some(next) => {
                    trace.push(next);
                    current = next;
                }
                None => break,
            }
        }
        trace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptDriver {
        script: HashMap<StateKind, Option<StateKind>>,
    }

    impl TransitionDriver for ScriptDriver {
        fn step(&mut self, current: StateKind) -> Option<StateKind> {
            self.script.get(&current).copied().flatten()
        }
    }

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
        assert_eq!(signal_to_state(Signal::Sigstp), Some(StateKind::Catatonia));
    }

    #[test]
    fn test_signal_to_state_default_none() {
        assert_eq!(signal_to_state(Signal::Sigalrm), None);
        assert_eq!(signal_to_state(Signal::Other(99)), None);
    }

    #[test]
    fn test_fake_registry_records() {
        let mut reg = FakeSignalRegistry::default();
        reg.register(HandlerKind::Transition, &[Signal::Sighup, Signal::Sigterm])
            .unwrap();
        assert_eq!(reg.registered(Signal::Sighup), Some(HandlerKind::Transition));
        reg.block_all_except(&[Signal::Sighup]);
        assert_eq!(reg.blocked_except, vec![Signal::Sighup]);
    }

    #[test]
    fn test_driver_runs_fixed_steps() {
        let mut driver = ScriptDriver {
            script: HashMap::from([
                (StateKind::Runcom, Some(StateKind::ReadTtys)),
                (StateKind::ReadTtys, Some(StateKind::MultiUser)),
                (StateKind::MultiUser, Some(StateKind::MultiUser)),
            ]),
        };
        let trace = driver.run(StateKind::Runcom, Some(2));
        assert_eq!(
            trace,
            vec![StateKind::Runcom, StateKind::ReadTtys, StateKind::MultiUser]
        );
    }

    #[test]
    fn test_driver_stops_on_none() {
        let mut driver = ScriptDriver {
            script: HashMap::from([(StateKind::Death, None)]),
        };
        let trace = driver.run(StateKind::Death, None);
        assert_eq!(trace, vec![StateKind::Death]);
    }

    #[test]
    fn test_alarm_flag_set_and_clear() {
        let flag = AlarmFlag::default();
        assert!(!flag.take());
        flag.set();
        assert!(flag.take());
        assert!(!flag.take());
    }
}
