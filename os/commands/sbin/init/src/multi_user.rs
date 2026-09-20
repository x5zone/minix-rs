//! Multi-user steady state ('m').
//!
//! Covers `minix3/sbin/init/init.c:1528-1564` (`multi_user`),
//! `1321-1370` (`start_getty`), `1290-1316` (`start_window_system`),
//! `669-689` (`setctty`), `1460-1497` (`collect_child`).
//! Design contract: `.design/09-design.v1.md §1.1-§1.3`.
//!
//! This is the one long-lived state: every ttys line without a live
//! process gets a getty, and the loop then reaps forever, spawning
//! replacements through `collect_child` until a signal asks for
//! another state. Note the wait uses plain options — no WUNTRACED
//! here (init.c:1559), unlike the rescue states.

use alloc::vec::Vec;
use crate::host::InitHost;
use crate::log::{emergency, stall, warning};
use crate::session::{Session, SE_SHUTDOWN};
use crate::session_db::SessionDb;
use crate::state_machine::StateKind;
use crate::wait::WaitStatus;
use minix_sys::Pid;

/// Minimum getty spacing in seconds (C: `GETTY_SPACING`, init.c:92).
pub const GETTY_SPACING_SECS: i64 = 5;
/// Sleep after spacing violation (C: `GETTY_SLEEP`, init.c:93).
pub const GETTY_SLEEP_SECS: u64 = 30;
/// Wait after starting window system (C: `WINDOW_WAIT`, init.c:94).
pub const WINDOW_WAIT_SECS: u64 = 3;

/// How long to delay a getty start (C: init.c:1350-1355).
pub fn getty_delay_secs(now_secs: i64, started_secs: i64) -> u64 {
    if now_secs > started_secs && now_secs - started_secs < GETTY_SPACING_SECS {
        GETTY_SLEEP_SECS
    } else {
        0
    }
}

/// What `collect_child` decided (C: init.c:1466-1495).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectAction {
    Ignore,
    RemoveSession,
    RestartSession,
    RequestCleanTtys,
}

/// Claim `device` as the child's controlling terminal (C: `setctty`,
/// init.c:669-689, Minix branch: setsid, leave DTR low, open O_RDWR,
/// `login_tty` onto fds 0-2). The seam folds those into one call; the
/// two failure ladders keep their C exit codes here.
pub fn setctty(host: &mut dyn InitHost, device: &str) {
    if host.set_controlling_tty(device).is_err() {
        stall(host, &format!("can't open {device} or make it controlling"));
        host.exit_process(1);
    }
}

/// Spawn the window system, if the line has one (C:
/// `start_window_system`, init.c:1290-1316).
///
/// Parent side is a no-op even on fork failure — C hopes the getty
/// fails so the restart machinery tries again. The child unblocks,
/// leaves the session, execs, and `_exit(6)` on failure.
fn start_window_system(host: &mut dyn InitHost, session: &Session) {
    let window = match &session.window {
        Some(w) => w,
        None => return,
    };
    match host.fork() {
        Err(_) => {
            emergency(
                host,
                &format!("can't fork for window system on port `{}'", session.device),
            );
        }
        Ok(0) => {
            if host.setsid().is_err() {
                emergency(host, "setsid failed (window)");
            }
            let err = host.exec(window);
            stall(
                host,
                &format!(
                    "can't exec window system `{}' for port `{}': {err}",
                    window.exec_path, session.device
                ),
            );
            host.exit_process(6);
        }
        Ok(_) => {}
    }
}

/// Start one login session (C: `start_getty`, init.c:1321-1370).
///
/// `Ok(pid)` hands the live child back to the caller; `Err(())` is the
/// C `-1` "serious trouble" answer. The child honours the chroot when
/// multi-user booted inside one, sleeps off a too-fast restart, starts
/// the optional window system, then execs the getty (`_exit(8)` on
/// failure).
pub fn start_getty(
    host: &mut dyn InitHost,
    session: &mut Session,
    did_multiuser_chroot: bool,
    rootdir: &str,
) -> Result<Pid, ()> {
    let pid = match host.fork() {
        Ok(pid) => pid,
        Err(_) => {
            emergency(
                host,
                &format!("can't fork for getty on port `{}'", session.device),
            );
            return Err(());
        }
    };
    if pid != 0 {
        return Ok(pid);
    }

    // C: the getty follows the boot into the chroot (init.c:1336-1342).
    if did_multiuser_chroot
        && let Err(e) = host.chroot(rootdir) {
            stall(
                host,
                &format!("can't chroot getty `{}' inside `{}': {e}", session.device, rootdir),
            );
            host.exit_process(7);
        }

    // C: too-fast restart sleeps in the child, delaying the exec
    // (init.c:1344-1350). No readable clock (ENOSYS) counts as "slow
    // enough" — the debounce degrades, the getty still runs.
    if let Ok(now) = host.now_secs()
        && getty_delay_secs(now, session.started_secs) > 0 {
            warning(
                host,
                &format!("getty repeating too quickly on port `{}', sleeping", session.device),
            );
            let _ = host.sleep_secs(GETTY_SLEEP_SECS);
        }

    if session.window.is_some() {
        start_window_system(host, session);
        let _ = host.sleep_secs(WINDOW_WAIT_SECS);
    }

    let getty = session.getty.as_ref().expect("validated by build_session");
    let err = host.exec(getty);
    stall(
        host,
        &format!("can't exec getty `{}' for port `{}': {err}", getty.exec_path, session.device),
    );
    host.exit_process(8);
}

/// The 'm' state: spawn gettys, reap forever, hand back the requested
/// transition (C: `multi_user`, init.c:1528-1564).
///
/// A `None` return is not possible: the C loop only leaves via a
/// requested transition. Fork trouble during a respawn requests
/// clean_ttys instead of aborting.
pub fn multi_user(
    host: &mut dyn InitHost,
    collector: &mut crate::driver::ChildCollector,
    signals: &crate::signal_state::SignalState,
) -> StateKind {
    // C: level 0 means "kernel should enter secure mode" — raise it to
    // 1 (init.c:1540-1544). Note the == test, not >.
    if host.securitylevel().ok().flatten() == Some(0) {
        let _ = host.set_securitylevel(1);
    }

    // C: every line without a live process gets a getty; a fork error
    // requests clean_ttys and stops the sweep (init.c:1546-1555).
    let mut requested: Option<StateKind> = None;
    for index in 0..collector.sessions.len() {
        if collector.sessions[index].process.is_some() {
            continue;
        }
        let spawn = start_getty(
            host,
            &mut collector.sessions[index],
            collector.did_multiuser_chroot,
            &collector.rootdir,
        );
        match spawn {
            Ok(pid) => {
                collector.sessions[index].process = Some(pid);
                collector.sessions[index].started_secs = host.now_secs().unwrap_or(0);
                collector.db.insert(pid, index + 1);
            }
            Err(()) => {
                requested = Some(StateKind::CleanTtys);
                break;
            }
        }
    }

    // C: reap until asked otherwise; plain waitpid options (init.c:1557-1562).
    while requested.is_none() {
        match host.waitpid(-1, 0) {
            Ok((pid, status)) => {
                if collector.collect(host, pid, &status)
                    == CollectAction::RequestCleanTtys
                {
                    requested = Some(StateKind::CleanTtys);
                    break;
                }
            }
            Err(_) => break,
        }
        requested = signals.take_requested();
    }

    requested.unwrap_or(StateKind::CleanTtys)
}

/// Reap one child (C: `collect_child`, init.c:1460-1497).
///
/// Unknown pids (the driver's own shell children, say) are ignored. A
/// known session first loses its ledger entries, then either retires
/// (SE_SHUTDOWN was set by catatonia/clean_ttys) or restarts its
/// getty; a restart failure asks for clean_ttys.
#[allow(clippy::too_many_arguments)]
pub fn collect_child(
    host: &mut dyn InitHost,
    sessions: &mut Vec<Session>,
    db: &mut dyn SessionDb,
    clear_logs: &mut dyn FnMut(&str, Pid, &WaitStatus),
    did_multiuser_chroot: bool,
    rootdir: &str,
    pid: Pid,
    status: &WaitStatus,
) -> CollectAction {
    let index = match db.find(pid) {
        Some(index) => index,
        None => return CollectAction::Ignore,
    };
    let pos = match sessions.iter().position(|sp| sp.index == index) {
        Some(pos) => pos,
        None => return CollectAction::Ignore,
    };
    clear_logs(&sessions[pos].device, pid, status);
    db.remove(pid);

    let shutting_down = sessions[pos].flags.contains(SE_SHUTDOWN);
    if shutting_down {
        sessions.remove(pos);
        return CollectAction::RemoveSession;
    }
    sessions[pos].process = None;

    match start_getty(host, &mut sessions[pos], did_multiuser_chroot, rootdir) {
        Err(()) => CollectAction::RequestCleanTtys,
        Ok(new_pid) => {
            let sp = &mut sessions[pos];
            sp.process = Some(new_pid);
            sp.started_secs = host.now_secs().unwrap_or(0);
            sp.flags.clear(SE_SHUTDOWN);
            let _ = SE_SHUTDOWN; // flag bit reserved for the shutdown path
            db.insert(new_pid, sp.index);
            CollectAction::RestartSession
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::ChildCollector;
    use crate::host::ScriptHost;
    use crate::session::build_session;
    use minix_sys::Errno;

    fn no_clear(_device: &str, _pid: Pid, _status: &WaitStatus) {}
    fn sample(index: usize, name: &str) -> Session {
        build_session(index, name, "/sbin/getty", None, true).unwrap()
    }

    #[test]
    fn test_spacing_triggers_sleep() {
        assert_eq!(getty_delay_secs(100, 98), GETTY_SLEEP_SECS);
    }

    #[test]
    fn test_spacing_no_sleep() {
        assert_eq!(getty_delay_secs(100, 90), 0);
        assert_eq!(getty_delay_secs(100, 100), 0);
    }





    #[test]
    fn test_collect_child_ignores_unknown_pid() {
        let mut host = ScriptHost::default();
        let mut sessions = vec![sample_session(1, "tty1")];
        let mut db = crate::session_db::SessionMapDb::default();
        db.open().unwrap();
        let mut cleared: Vec<(String, i32)> = Vec::new();
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        assert_eq!(
            collector.collect(&mut host, 999, &WaitStatus::Exited { code: 0 }),
            CollectAction::Ignore
        );
        let _ = (&mut no_clear, &mut cleared);
        assert_eq!(collector.sessions.len(), 1);
    }

    #[test]
    fn test_collect_child_restarts_and_reindexes() {
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(9)); // the respawned getty
        host.now = 500;
        let mut sessions = vec![sample_session(1, "tty1")];
        let mut db = crate::session_db::SessionMapDb::default();
        db.open().unwrap();
        db.insert(4, 1);
        sessions[0].process = Some(4);
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        assert_eq!(
            collector.collect(&mut host, 4, &WaitStatus::Exited { code: 0 }),
            CollectAction::RestartSession
        );
        assert_eq!(collector.sessions[0].process, Some(9));
        assert_eq!(collector.sessions[0].started_secs, 500);
        assert_eq!(collector.db.find(9), Some(1));
        assert_eq!(collector.db.find(4), None);
    }

    #[test]
    fn test_collect_child_shutdown_removes_session() {
        let mut host = ScriptHost::default();
        let mut sessions = vec![sample_session(1, "tty1")];
        sessions[0].flags.set(SE_SHUTDOWN);
        sessions[0].process = Some(4);
        let mut db = crate::session_db::SessionMapDb::default();
        db.open().unwrap();
        db.insert(4, 1);
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        assert_eq!(
            collector.collect(&mut host, 4, &WaitStatus::Exited { code: 0 }),
            CollectAction::RemoveSession
        );
        assert!(collector.sessions.is_empty());
        assert_eq!(collector.db.find(4), None);
    }

    #[test]
    fn test_multi_user_spawns_all_and_reaps_until_requested() {
        let mut host = ScriptHost::default();
        host.securitylevel = Some(0);
        host.fork_outcomes.push(Ok(10));
        host.fork_outcomes.push(Ok(11));
        host.fork_outcomes.push(Ok(12)); // respawn of the reaped getty
        host.now = 42;
        host.wait_outcomes.push(Ok((11, WaitStatus::Exited { code: 0 })));
        host.wait_errors.push(Errno::ESRCH); // ends the reap loop
        let mut sessions = vec![sample_session(1, "tty1"), sample_session(2, "tty2")];
        let mut db = crate::session_db::SessionMapDb::default();
        db.open().unwrap();
        let signals = std::sync::Arc::new(crate::signal_state::SignalState::default());
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        assert_eq!(
            multi_user(&mut host, &mut collector, &signals),
            StateKind::CleanTtys
        );
        assert_eq!(host.securitylevel_sets, vec![1]); // level 0 → 1
        // Both gettys spawned plus the respawn of the reaped one.
        assert!(collector.db.find(12).is_some());
    }

    #[test]
    fn test_multi_user_spawn_failure_requests_clean_ttys() {
        let mut host = ScriptHost::default();
        host.securitylevel = Some(1);
        host.fork_outcomes.push(Err(Errno::EAGAIN));
        let mut sessions = vec![sample_session(1, "tty1"), sample_session(2, "tty2")];
        let mut db = crate::session_db::SessionMapDb::default();
        db.open().unwrap();
        let signals = std::sync::Arc::new(crate::signal_state::SignalState::default());
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        assert_eq!(
            multi_user(&mut host, &mut collector, &signals),
            StateKind::CleanTtys
        );
        // Level 1 stays untouched — C only raises level 0.
        assert!(host.securitylevel_sets.is_empty());
    }

    fn sample_session(index: usize, name: &str) -> Session {
        sample(index, name)
    }

    #[test]
    fn test_getty_child_debounce_window_sleeps_before_exec() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.now = 100;
        host.fork_outcomes.push(Ok(0)); // child branch
        host.exec_outcomes.push(Errno::EPERM);
        let mut session = sample(1, "tty1");
        session.started_secs = 98; // 2 s ago — inside the 5 s window
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _ = start_getty(&mut host, &mut session, false, "/");
        }));
        // Debounce sleep in the child, then the stall sleep after the
        // exec failure — both 30 s (GETTY_SLEEP, STALL_TIMEOUT).
        assert_eq!(
            host.slept,
            vec![GETTY_SLEEP_SECS, crate::log::STALL_TIMEOUT_SECS]
        );
        assert_eq!(host.exec_requests.len(), 1);
        assert_eq!(host.exits, vec![8]);
    }

    #[test]
    fn test_getty_child_starts_window_then_getty_when_configured() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.now = 200;
        // First fork = getty (we script the child); second = window
        // system, whose parent side is the getty child.
        host.fork_outcomes.push(Ok(0));
        host.fork_outcomes.push(Ok(21));
        host.exec_outcomes.push(Errno::EPERM);
        let getty = build_session(1, "tty1", "/sbin/getty", Some("/usr/bin/X :0"), true).unwrap();
        let mut session = getty;
        session.started_secs = 190;
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _ = start_getty(&mut host, &mut session, false, "/");
        }));
        // Window system spawned first (parent side here), then the
        // WINDOW_WAIT pause, then the getty exec whose failure runs
        // the _exit(8) epilogue.
        assert_eq!(host.exits, vec![8]);
        assert!(host.slept.contains(&WINDOW_WAIT_SECS));
        assert_eq!(host.exec_requests.len(), 1);
        assert_eq!(host.exec_requests[0].exec_path, "/sbin/getty");
    }
}
