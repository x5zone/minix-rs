//! Runcom state ('r'): execute /etc/rc.
//!
//! Covers `minix3/sbin/init/init.c:879-1014` (`runetcrc`, `runcom`).
//! Design contract: `.design/05-design.v1.md §1.1-§1.3`.
//!
//! Two layers, as in C: `runetcrc` is one attempt — fork a child that
//! claims the console and execs the shell on `/etc/rc` — and `runcom`
//! is the policy around it: run once, maybe again inside a chroot,
//! then reset the boot mode and record the reboot in the session
//! ledger. The wait loop inside `runetcrc` is the copy C itself
//! labels "copied from single_user(); this is a bit paranoid" — minus
//! the transition poll, which runetcrc never had.

use alloc::string::ToString;
use crate::entry::RuncomMode;
use crate::host::{ignore_spec, InitHost, SignalSpec};
use crate::log::{emergency, stall, warning};
use crate::session::ParsedCommand;
use crate::state_machine::{sig, StateKind};
use crate::wait::{EINTR, WNOHANG, WUNTRACED, WaitStatus};

/// Absolute path of the rc shell (C: `INIT_BSHELL` = `_PATH_BSHELL`,
/// init.c:105, exec'd at init.c:913).
pub const RC_SHELL_PATH: &str = "/bin/sh";
/// The startup script (C: `_PATH_RUNCOM`, `pathnames.h:40`).
pub const RUNCOM_SCRIPT: &str = "/etc/rc";

/// Assemble the `sh /etc/rc [autoboot]` spawn request (C: the argv at
/// init.c:899-900, exec'd via `execv(INIT_BSHELL, ...)` at init.c:913).
///
/// The exec path is the shell binary; argv[0] is the bare "sh" the
/// child sees — the distinction ParsedCommand exists for.
pub fn rc_argv(mode: RuncomMode) -> ParsedCommand {
    let mut argv = vec!["sh".to_string(), RUNCOM_SCRIPT.to_string()];
    if mode == RuncomMode::Autoboot {
        argv.push("autoboot".to_string());
    }
    ParsedCommand {
        exec_path: RC_SHELL_PATH.to_string(),
        argv,
    }
}

/// What one `runcom` state execution hands back to the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuncomResult {
    /// C: `return single_user` — fork failure, rc crashed or exited
    /// nonzero, or the wait loop broke.
    SingleUser,
    /// C: SIGTERM + catatonia — the rc script ran `/sbin/reboot`;
    /// there is no next state, only the quiet wait (init.c:949-957).
    AwaitReboot,
    /// `/etc/rc` finished cleanly (directly, or again inside the
    /// chroot). `did_multiuser_chroot` feeds the ttys path decision in
    /// doc 12; the caller resets the boot mode to AUTOBOOT here
    /// (init.c:1004).
    Booted { did_multiuser_chroot: bool },
}

/// One attempt's verdict (C: the `state_func_t` values inside
/// `runetcrc` itself — `read_ttys` on success included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    SingleUser,
    AwaitReboot,
    ReadTtys,
}

/// Inputs for the runcom state (C: the `runetcrc` locals plus the two
/// globals it reads).
pub struct RuncomDeps<'a> {
    /// Boot mode for the argv (C: `runcom_mode`, init.c:900).
    pub mode: RuncomMode,
    /// The `init.root` value for the chroot attempt (C: the `rootdir`
    /// global, init.c:903); only consulted when `trychroot` is set.
    pub rootdir: &'a str,
    /// Async-written signal state (C: `requested_transition` — the
    /// catatonia check at init.c:951).
    pub signals: &'a crate::signal_state::SignalState,
}

/// The 'r' state: run the startup script, maybe twice (chroot), then
/// hand over to read_ttys (C: `runcom`, init.c:976-1014).
pub fn runcom(
    host: &mut dyn InitHost,
    collector: &mut crate::driver::ChildCollector,
    ledger: &mut crate::driver::Ledger,
    deps: &RuncomDeps,
) -> RuncomResult {
    // C: the first run is always outside the chroot (init.c:986).
    match runetcrc(host, false, collector, deps) {
        Attempt::ReadTtys => {}
        Attempt::SingleUser => return RuncomResult::SingleUser,
        Attempt::AwaitReboot => return RuncomResult::AwaitReboot,
    }

    // C: shouldchroot() gates the second, chrooted run — the Real
    // /etc/rc (init.c:992-999).
    let did_multiuser_chroot = if crate::sysctl::should_chroot(host) {
        match runetcrc(host, true, collector, deps) {
            Attempt::ReadTtys => true,
            Attempt::SingleUser => return RuncomResult::SingleUser,
            Attempt::AwaitReboot => return RuncomResult::AwaitReboot,
        }
    } else {
        false
    };

    // C: the boot succeeded — reset the mode and write the ledger
    // entry, regardless of chroot (init.c:1004-1012).
    ledger.reboot(host);
    RuncomResult::Booted { did_multiuser_chroot }
}

/// One `/etc/rc` attempt (C: `runetcrc(trychroot)`, init.c:880-972).
fn runetcrc(
    host: &mut dyn InitHost,
    trychroot: bool,
    collector: &mut crate::driver::ChildCollector,
    deps: &RuncomDeps,
) -> Attempt {
    // C: the child ignores SIGHUP/SIGTSTP (init.c:886-891); the parent
    // never touched its own dispositions in this state.
    let _ = host.register_handlers(&ignore_spec(&[
        sig::SIGNAL_HANGUP,
        sig::SIGNAL_TERMINAL_STOP,
    ]));

    let pid = match host.fork() {
        Ok(0) => {
            // Child: console, argv, unblock, optional chroot, exec
            // (init.c:884-910).
            let _ = host.set_controlling_tty(crate::single_user::CONSOLE_PATH);
            let cmd = rc_argv(deps.mode);
            let _ = host.register_handlers(&SignalSpec {
                handlers: vec![],
                blocked_except: Some(vec![]),
            });
            if trychroot
                && let Err(e) = host.chroot(deps.rootdir) {
                    warning(
                        host,
                        &format!("failed to chroot to `{}': {}", deps.rootdir, e),
                    );
                    host.exit_process(4); // force single user mode
                }
            let err = host.exec(&cmd);
            stall(
                host,
                &format!("can't exec `{RC_SHELL_PATH}' for `{RUNCOM_SCRIPT}': {err}"),
            );
            host.exit_process(5); // force single user mode
        }
        Err(_) => {
            // C: "can't fork" — reap, one stall of sleep, single-user
            // (init.c:911-922). The sleep: single_user's fork failure
            // does not have one.
            emergency(
                host,
                &format!("can't fork for `{RC_SHELL_PATH}' on `{RUNCOM_SCRIPT}'"),
            );
            // C: init.c:920 `while (waitpid(-1, NULL, WNOHANG) > 0)` — reap
            // only already-exited children. A WNOHANG answer of 0 (live
            // children, none changed) or an error (ECHILD) ends the loop.
            // `.is_ok()` would spin forever on `Ok((0, _))` — the aarch64
            // §1.117 livelock (INIT flooded PM with ~60k WAIT4/40s).
            while matches!(host.waitpid(-1, WNOHANG), Ok((pid, _)) if pid > 0) {}
            let _ = host.sleep_secs(crate::log::STALL_TIMEOUT_SECS);
            return Attempt::SingleUser;
        }
        Ok(pid) => pid,
    };

    let status = loop {
        match host.waitpid(-1, WUNTRACED) {
            Ok((wpid, status)) => {
                collector.collect(host, wpid, &status);
                if wpid == pid {
                    if status.stopped() {
                        warning(
                            host,
                            &format!(
                                "`{RC_SHELL_PATH}' on `{RUNCOM_SCRIPT}' stopped, restarting"
                            ),
                        );
                        let _ = host.kill(pid, sig::SIGNAL_CONTINUE);
                        continue;
                    }
                    break status;
                }
            }
            Err(err) if err.to_i32() == EINTR => continue,
            Err(_) => {
                warning(
                    host,
                    &format!(
                        "wait for `{RC_SHELL_PATH}' on `{RUNCOM_SCRIPT}' failed; \
                         going to single user mode"
                    ),
                );
                return Attempt::SingleUser;
            }
        }
    };

    // C: /etc/rc executed /sbin/reboot — wait for the end quietly
    // (init.c:949-957).
    if status.signaled_by(sig::SIGNAL_TERMINATE)
        && deps.signals.peek_requested() == Some(StateKind::Catatonia)
    {
        return Attempt::AwaitReboot;
    }
    // C: abnormal death — single user (init.c:959-963).
    if status.signaled() {
        warning(
            host,
            &format!(
                "`{RC_SHELL_PATH}' on `{RUNCOM_SCRIPT}' terminated abnormally, \
                 going to single user mode"
            ),
        );
        return Attempt::SingleUser;
    }
    // C: nonzero exit — single user; zero — read_ttys (init.c:965-968).
    match status.exit_code() {
        Some(0) => Attempt::ReadTtys,
        _ => Attempt::SingleUser,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{ChildCollector, Ledger};
    use crate::host::ScriptHost;
    use crate::session::Session;
    use crate::signal_state::SignalState;
    use minix_sys::Errno;
    use std::sync::Arc;

    fn fixture() -> (ScriptHost, Vec<Session>, crate::session_db::SessionMapDb, Arc<SignalState>) {
        (
            ScriptHost::default(),
            Vec::new(),
            crate::session_db::SessionMapDb::default(),
            Arc::new(SignalState::default()),
        )
    }

    fn collector<'a>(
        sessions: &'a mut Vec<Session>,
        db: &'a mut crate::session_db::SessionMapDb,
        rootdir: &str,
    ) -> ChildCollector<'a> {
        ChildCollector {
            sessions,
            db,
            did_multiuser_chroot: false,
            rootdir: rootdir.to_string(),
        }
    }

    #[test]
    fn test_rc_argv_autoboot_has_third() {
        let cmd = rc_argv(RuncomMode::Autoboot);
        assert_eq!(cmd.argv.len(), 3);
        assert_eq!(cmd.argv[2], "autoboot");
    }

    #[test]
    fn test_rc_argv_fastboot_truncated() {
        assert_eq!(rc_argv(RuncomMode::Fastboot).argv.len(), 2);
    }

    #[test]
    fn test_rc_exec_path_is_shell_binary_argv0_is_sh() {
        let cmd = rc_argv(RuncomMode::Autoboot);
        assert_eq!(cmd.exec_path, "/bin/sh");
        assert_eq!(cmd.argv[0], "sh");
        assert_eq!(cmd.argv[1], "/etc/rc");
    }







    #[test]
    fn test_runetcrc_child_exec_request_carries_autoboot() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Ok(0));
        host.exec_outcomes.push(Errno::EPERM);
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            runetcrc(&mut host, false, &mut collector, &deps)
        }));
        assert_eq!(host.exec_requests.len(), 1);
        let cmd = &host.exec_requests[0];
        assert_eq!(cmd.exec_path, RC_SHELL_PATH);
        assert_eq!(cmd.argv, vec!["sh", "/etc/rc", "autoboot"]);
        // C: exec failure stalls 30 s then _exit(5) (init.c:907-909).
        assert_eq!(host.slept, vec![30]);
        assert_eq!(host.exits, vec![5]);
    }

    #[test]
    fn test_runetcrc_chroot_failure_exits_four() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.chroot_ok = false;
        host.fork_outcomes.push(Ok(0));
        let mut collector = collector(&mut sessions, &mut db, "/newroot");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Fastboot,
            rootdir: "/newroot",
            signals: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            runetcrc(&mut host, true, &mut collector, &deps)
        }));
        assert_eq!(host.chroots, vec!["/newroot"]);
        assert_eq!(host.exits, vec![4]);
        assert!(host.exec_requests.is_empty());
    }

    #[test]
    fn test_runetcrc_chrooted_child_execs_after_chroot() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.chroot_ok = true;
        host.fork_outcomes.push(Ok(0));
        host.exec_outcomes.push(Errno::EPERM);
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/newroot".into(),
        };
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Fastboot,
            rootdir: "/newroot",
            signals: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            runetcrc(&mut host, true, &mut collector, &deps)
        }));
        // chroot lands before the exec, and fastboot omits "autoboot".
        assert_eq!(host.chroots, vec!["/newroot"]);
        assert_eq!(host.exec_requests[0].argv, vec!["sh", "/etc/rc"]);
    }

    #[test]
    fn test_runetcrc_fork_failure_sleeps_then_single_user() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Err(Errno::EAGAIN));
        host.wait_errors.push(Errno::ESRCH); // reap loop ends
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut collector, &deps),
            Attempt::SingleUser
        );
        assert_eq!(host.slept, vec![30]);
    }

    #[test]
    fn test_runetcrc_fork_failure_reap_loop_exits_on_zero() {
        // NK4-C §1.117: the WNOHANG reap loop must stop the moment no more
        // children have exited. A single `Ok((0, _))` (a live child, none
        // changed) is the exit signal (C `waitpid(...) > 0`); the old
        // `.is_ok()` guard would call waitpid again, draining the empty mock
        // queue and panicking — i.e. an unbounded spin. Passing without a
        // second waitpid proves the loop broke on pid == 0.
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Err(Errno::EAGAIN));
        host.wait_outcomes.push(Ok((0, WaitStatus::Exited { code: 0 })));
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut collector, &deps),
            Attempt::SingleUser
        );
        assert_eq!(host.slept, vec![30]);
    }

    #[test]
    fn test_runetcrc_stopped_shell_continues_then_succeeds() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Stopped { stopsig: 18 })));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut collector, &deps),
            Attempt::ReadTtys
        );
        assert_eq!(host.kills, vec![(7, sig::SIGNAL_CONTINUE)]);
    }

    #[test]
    fn test_runetcrc_sigterm_with_catatonia_awaits_reboot() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        signals.note_signal(18); // SIGTSTP → catatonia request
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((
            7,
            WaitStatus::Signaled { termsig: sig::SIGNAL_TERMINATE, core_dumped: false },
        )));
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut collector, &deps),
            Attempt::AwaitReboot
        );
    }

    #[test]
    fn test_runetcrc_sigterm_without_catatonia_is_single_user() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((
            7,
            WaitStatus::Signaled { termsig: sig::SIGNAL_TERMINATE, core_dumped: false },
        )));
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut collector, &deps),
            Attempt::SingleUser
        );
    }

    #[test]
    fn test_runcom_double_run_inside_chroot() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.root = Some("/newroot".into());
        for _ in 0..2 {
            host.fork_outcomes.push(Ok(7));
            host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        }
        let mut collector = collector(&mut sessions, &mut db, "/newroot");
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/newroot",
            signals: &signals,
        };
        assert_eq!(
            runcom(&mut host, &mut collector, &mut ledger, &deps),
            RuncomResult::Booted { did_multiuser_chroot: true }
        );
        // The chroot itself runs in each child (asserted in
        // test_runetcrc_chrooted_child_execs_after_chroot).
        // C: exactly one reboot ledger entry, after the last success.
        assert_eq!(host.appends.len(), 1);
        assert!(host.appends[0].1.contains("reboot"));
    }

    #[test]
    fn test_runcom_single_run_without_chroot() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Fastboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runcom(&mut host, &mut collector, &mut ledger, &deps),
            RuncomResult::Booted { did_multiuser_chroot: false }
        );
        assert_eq!(host.appends.len(), 1);
    }

    #[test]
    fn test_runcom_rc_failure_propagates_without_ledger() {
        let (mut host, mut sessions, mut db, signals) = fixture();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 2 })));
        let mut collector = collector(&mut sessions, &mut db, "/");
        let mut seen = false;
        let mut ledger = Ledger::new(&mut seen);
        let deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            rootdir: "/",
            signals: &signals,
        };
        assert_eq!(
            runcom(&mut host, &mut collector, &mut ledger, &deps),
            RuncomResult::SingleUser
        );
        assert!(host.appends.is_empty());
    }
}
