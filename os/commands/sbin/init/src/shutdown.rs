//! Shutdown states ('c' catatonia, 'd' death).
//!
//! Covers `minix3/sbin/init/init.c:1634-1698`.
//! Design contract: `.design/11-design.v1.md §1.1-§1.2`.

use crate::host::InitHost;
use crate::log::warning;
use crate::session::{Session, SE_SHUTDOWN};
use crate::state_machine::{sig, StateKind};
use std::sync::atomic::{AtomicBool, Ordering};

/// Seconds per death round (C: `DEATH_WATCH`, init.c:96).
pub const DEATH_WATCH_SECS: u64 = 10;

/// Kill escalation sequence (C: `death_sigs`, init.c:1667):
/// SIGHUP, SIGTERM, SIGKILL — values from the Minix3 numbering authority.
pub const DEATH_SEQUENCE: [i32; 3] =
    [sig::SIGNAL_HANGUP, sig::SIGNAL_TERMINATE, sig::SIGNAL_KILL];

/// C: `ESRCH 3` (`sys/errno.h:5`) — nobody left to signal.
const ESRCH: i32 = 3;
/// C: `ECHILD 10` (`sys/errno.h:10`) — no children left to reap.
const ECHILD: i32 = 10;

/// Inputs for the two shutdown states.
pub struct ShutdownDeps<'a, 'b> {
    /// The child reaper over the session table and DB (C: the
    /// `sessions` list plus `collect_child`).
    pub collector: &'a mut crate::driver::ChildCollector<'b>,
    /// The ledger writer (C: `logwtmpx("~", "shutdown", ...)`,
    /// init.c:1674).
    pub ledger: &'a mut crate::driver::Ledger<'a>,
    /// The SIGALRM flag (C: `clang`, init.c:173) — shared with the
    /// real handler so an asynchronous set is visible here.
    pub clang: &'a AtomicBool,
}

/// Block further logins: mark every session and return to multi-user,
/// where the dying sessions are reaped (C: `catatonia`,
/// init.c:1634-1643).
pub fn catatonia(sessions: &mut [Session]) -> StateKind {
    for sp in sessions.iter_mut() {
        sp.flags.set(SE_SHUTDOWN);
    }
    StateKind::MultiUser
}

/// Bring the system down to single user (C: `death`,
/// init.c:1662-1698).
///
/// Three escalating rounds of `kill(-1, sig)`; each round reaps until
/// the alarm rings (`clang`) or the children run out (ECHILD, which
/// means done — straight to single-user). A process group that
/// survives all three rounds gets the ps-axl warning; single-user is
/// the only exit state, exactly as in C.
pub fn death(host: &mut dyn InitHost, deps: &mut ShutdownDeps) -> StateKind {
    // C: mark everything (init.c:1666-1668).
    for sp in deps.collector.sessions.iter_mut() {
        sp.flags.set(SE_SHUTDOWN);
    }
    // C: the shutdown ledger entry, once (init.c:1672-1677).
    deps.ledger.shutdown(host);

    for &signum in DEATH_SEQUENCE.iter() {
        // C: nobody received the signal — done early (init.c:1679-1681).
        if let Err(e) = host.kill(-1, signum)
            && e.to_i32() == ESRCH {
                return StateKind::SingleUser;
            }

        deps.clang.swap(false, Ordering::SeqCst); // clang = 0
        let _ = host.alarm(DEATH_WATCH_SECS as u32);

        let mut children_exhausted = false;
        loop {
            match host.waitpid(-1, 0) {
                Ok((pid, status)) => {
                    deps.collector.collect(host, pid, &status);
                }
                Err(e) => {
                    if e.to_i32() == ECHILD {
                        children_exhausted = true;
                    }
                    // C's do-while checks clang and ECHILD; other
                    // errnos (EINTR among them) keep the reap going.
                    if children_exhausted || deps.clang.swap(false, Ordering::SeqCst) {
                        break;
                    }
                    continue;
                }
            }
            if deps.clang.swap(false, Ordering::SeqCst) {
                break;
            }
        }

        if children_exhausted {
            return StateKind::SingleUser;
        }
    }

    warning(host, "some processes would not die; ps axl advised");
    StateKind::SingleUser
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{ChildCollector, Ledger};
    use crate::host::ScriptHost;
    use crate::session::build_session;
    use crate::session_db::SessionDb;
    use crate::wait::WaitStatus;
    use minix_sys::Errno;

    fn sample(index: usize, name: &str) -> Session {
        build_session(index, name, "/sbin/getty", None, true).unwrap()
    }

    #[test]
    fn test_death_sequence_order() {
        assert_eq!(DEATH_SEQUENCE, [1, 15, 9]);
    }




    #[test]
    fn test_catatonia_marks_all_sessions() {
        let mut sessions = vec![sample(1, "tty1"), sample(2, "tty2")];
        assert_eq!(catatonia(&mut sessions), StateKind::MultiUser);
        for sp in &sessions {
            assert!(sp.flags.contains(SE_SHUTDOWN));
        }
    }

    #[test]
    fn test_death_esrch_on_first_round_goes_single_user() {
        // kill(-1) with nobody left: done before the first alarm.
        let mut host = ScriptHost::default();
        host.kill_errors.push(Errno::ESRCH);
        let mut sessions = vec![sample(1, "tty1")];
        let mut db = crate::session_db::HashMapDb::default();
        db.open().unwrap();
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let clang = std::sync::atomic::AtomicBool::new(false);
        let mut deps = ShutdownDeps {
            collector: &mut collector,
            ledger: &mut ledger,
            clang: &clang,
        };
        assert_eq!(death(&mut host, &mut deps), StateKind::SingleUser);
        // The ledger entry is written before the rounds, regardless.
        assert_eq!(host.appends.len(), 1);
        assert!(host.appends[0].1.contains("shutdown"));
        assert!(host.alarms.is_empty()); // never armed
    }

    #[test]
    fn test_death_childless_round_ends_early() {
        // kill succeeds; the reap hits ECHILD — done, single-user,
        // with the alarm armed exactly once.
        let mut host = ScriptHost::default();
        host.wait_errors.push(Errno::from_i32(ECHILD));
        let mut sessions = vec![sample(1, "tty1")];
        let mut db = crate::session_db::HashMapDb::default();
        db.open().unwrap();
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let clang = std::sync::atomic::AtomicBool::new(false);
        let mut deps = ShutdownDeps {
            collector: &mut collector,
            ledger: &mut ledger,
            clang: &clang,
        };
        assert_eq!(death(&mut host, &mut deps), StateKind::SingleUser);
        assert_eq!(host.alarms, vec![DEATH_WATCH_SECS as u32]);
    }

    #[test]
    fn test_death_all_rounds_exhausted_warns() {
        // The scripted alarm sets clang (the kernel's SIGALRM would),
        // so every round times out; three rounds in, the warning fires.
        let mut host = ScriptHost::default();
        let mut sessions = vec![sample(1, "tty1")];
        let mut db = crate::session_db::HashMapDb::default();
        db.open().unwrap();
        let clang = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        host.alarm_sets_clang = Some(clang.clone());
        for _ in 0..3 {
            host.wait_outcomes
                .push(Ok((99, WaitStatus::Exited { code: 0 })));
        }
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let mut deps = ShutdownDeps {
            collector: &mut collector,
            ledger: &mut ledger,
            clang: &clang,
        };
        assert_eq!(death(&mut host, &mut deps), StateKind::SingleUser);
        assert!(host
            .console
            .iter()
            .any(|(_, m)| m.contains("would not die")));
    }
}
