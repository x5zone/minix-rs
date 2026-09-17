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

use crate::entry::RuncomMode;
use crate::host::{ignore_spec, InitHost, SignalSpec};
use crate::log::{emergency, stall, warning};
use crate::session::ParsedCommand;
use crate::state_machine::sig;
use crate::wait::{EINTR, WNOHANG, WUNTRACED, WaitStatus};
use minix_sys::{Errno, Pid};

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

/// Where a finished `/etc/rc` run goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RcOutcome {
    SingleUser,
    Continue,
    RebootQuiet,
    ReadTtys,
}

/// Classify one wait observation for the rc child (C: init.c:949-968).
///
/// A stopped rc keeps the loop going; SIGTERM arriving together with a
/// catatonia request means "shut down, quietly" (init.c:949-957); any
/// other death — signal or nonzero exit — falls back to single-user
/// (init.c:959-966); only a clean zero exit reaches read_ttys
/// (init.c:968).
pub fn classify_rc_exit(status: WaitStatus, catatonia_requested: bool) -> RcOutcome {
    if status.stopped() {
        return RcOutcome::Continue;
    }
    if catatonia_requested && status.signaled_by(sig::SIGNAL_TERMINATE) {
        return RcOutcome::RebootQuiet;
    }
    match status {
        WaitStatus::Exited { code } if code == 0 => RcOutcome::ReadTtys,
        _ => RcOutcome::SingleUser,
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
    /// Whether the wait loop observed a catatonia request (C:
    /// `requested_transition == catatonia`, init.c:951).
    pub catatonia_requested: &'a dyn Fn() -> bool,
    /// Called for every reaped child (C: init.c:939).
    pub collect: &'a mut dyn FnMut(Pid, &WaitStatus),
    /// The `init.root` value for the chroot attempt (C: the `rootdir`
    /// global, init.c:903); only consulted when `trychroot` is set.
    pub rootdir: &'a str,
    /// The reboot ledger entry `runcom` writes on success (C:
    /// `logwtmpx("~", "reboot", ...)`, init.c:1008) — wired to the
    /// utmp sink by the caller.
    pub record_reboot: &'a mut dyn FnMut(),
}

/// The 'r' state: run the startup script, maybe twice (chroot), then
/// hand over to read_ttys (C: `runcom`, init.c:976-1014).
pub fn runcom(host: &mut dyn InitHost, mut deps: RuncomDeps) -> RuncomResult {
    // C: the first run is always outside the chroot (init.c:986).
    match runetcrc(host, false, &mut deps) {
        Attempt::ReadTtys => {}
        Attempt::SingleUser => return RuncomResult::SingleUser,
        Attempt::AwaitReboot => return RuncomResult::AwaitReboot,
    }

    // C: shouldchroot() gates the second, chrooted run — the Real
    // /etc/rc (init.c:992-999).
    let did_multiuser_chroot = if crate::sysctl::should_chroot(host) {
        match runetcrc(host, true, &mut deps) {
            Attempt::ReadTtys => true,
            Attempt::SingleUser => return RuncomResult::SingleUser,
            Attempt::AwaitReboot => return RuncomResult::AwaitReboot,
        }
    } else {
        false
    };

    // C: the boot succeeded — reset the mode and write the ledger
    // entry, regardless of chroot (init.c:1004-1012).
    (deps.record_reboot)();
    RuncomResult::Booted { did_multiuser_chroot }
}

/// One `/etc/rc` attempt (C: `runetcrc(trychroot)`, init.c:880-972).
fn runetcrc(host: &mut dyn InitHost, trychroot: bool, deps: &mut RuncomDeps) -> Attempt {
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
            if trychroot {
                if let Err(e) = host.chroot(deps.rootdir) {
                    warning(
                        host,
                        &format!("failed to chroot to `{}': {}", deps.rootdir, e),
                    );
                    host.exit_process(4); // force single user mode
                }
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
            while host.waitpid(-1, WNOHANG).is_ok() {}
            let _ = host.sleep_secs(crate::log::STALL_TIMEOUT_SECS);
            return Attempt::SingleUser;
        }
        Ok(pid) => pid,
    };

    let status = loop {
        match host.waitpid(-1, WUNTRACED) {
            Ok((wpid, status)) => {
                (deps.collect)(wpid, &status);
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
    if status.signaled_by(sig::SIGNAL_TERMINATE) && (deps.catatonia_requested)() {
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
    use crate::host::ScriptHost;

    fn no_collect(_pid: Pid, _status: &WaitStatus) {}
    fn no_catatonia() -> bool {
        false
    }
    fn noop_record() {}

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
        // C: execv(INIT_BSHELL, argv) with argv[0] = "sh"
        // (init.c:899-900, 913) — path and argv[0] differ.
        let cmd = rc_argv(RuncomMode::Autoboot);
        assert_eq!(cmd.exec_path, "/bin/sh");
        assert_eq!(cmd.argv[0], "sh");
        assert_eq!(cmd.argv[1], "/etc/rc");
    }

    #[test]
    fn test_zero_exit_goes_read_ttys() {
        assert_eq!(
            classify_rc_exit(WaitStatus::Exited { code: 0 }, false),
            RcOutcome::ReadTtys
        );
    }

    #[test]
    fn test_nonzero_goes_single_user() {
        assert_eq!(
            classify_rc_exit(WaitStatus::Exited { code: 1 }, false),
            RcOutcome::SingleUser
        );
    }

    #[test]
    fn test_abnormal_goes_single_user() {
        let killed = WaitStatus::Signaled { termsig: 11, core_dumped: true };
        assert_eq!(classify_rc_exit(killed, false), RcOutcome::SingleUser);
    }

    #[test]
    fn test_catatonia_sigterm_quiets() {
        let term = WaitStatus::Signaled { termsig: 15, core_dumped: false };
        assert_eq!(classify_rc_exit(term, true), RcOutcome::RebootQuiet);
    }

    #[test]
    fn test_catatonia_without_sigterm_still_single_user() {
        // The quiet path needs BOTH the catatonia request and SIGTERM.
        let kill = WaitStatus::Signaled { termsig: 9, core_dumped: false };
        assert_eq!(classify_rc_exit(kill, true), RcOutcome::SingleUser);
    }

    #[test]
    fn test_stopped_rc_continues() {
        let stopped = WaitStatus::Stopped { stopsig: 18 };
        assert_eq!(classify_rc_exit(stopped, false), RcOutcome::Continue);
    }

    #[test]
    fn test_runetcrc_child_exec_request_carries_autoboot() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.exec_outcomes.push(Errno::EPERM);
        let mut recorded = 0;
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut || recorded += 1,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| runetcrc(&mut host, false, &mut deps)));
        assert_eq!(host.exec_requests.len(), 1);
        let cmd = &host.exec_requests[0];
        assert_eq!(cmd.exec_path, RC_SHELL_PATH);
        assert_eq!(cmd.argv, vec!["sh", "/etc/rc", "autoboot"]);
        // C: exec failure stalls 30 s then _exit(5) (init.c:907-909).
        assert_eq!(host.slept, vec![30]);
        assert_eq!(host.exits, vec![5]);
        assert_eq!(recorded, 0);
    }

    #[test]
    fn test_runetcrc_chroot_failure_exits_four() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.chroot_ok = false;
        host.fork_outcomes.push(Ok(0));
        let mut deps = RuncomDeps {
            mode: RuncomMode::Fastboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/newroot",
            record_reboot: &mut noop_record,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| runetcrc(&mut host, true, &mut deps)));
        assert_eq!(host.chroots, vec!["/newroot"]);
        assert_eq!(host.exits, vec![4]);
        assert!(host.exec_requests.is_empty());
    }

    #[test]
    fn test_runetcrc_chrooted_child_execs_after_chroot() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.chroot_ok = true;
        host.fork_outcomes.push(Ok(0));
        host.exec_outcomes.push(Errno::EPERM);
        let mut deps = RuncomDeps {
            mode: RuncomMode::Fastboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/newroot",
            record_reboot: &mut noop_record,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| runetcrc(&mut host, true, &mut deps)));
        // chroot lands before the exec, and fastboot omits "autoboot".
        assert_eq!(host.chroots, vec!["/newroot"]);
        assert_eq!(host.exec_requests[0].argv, vec!["sh", "/etc/rc"]);
    }

    #[test]
    fn test_runetcrc_fork_failure_sleeps_then_single_user() {
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Err(Errno::EAGAIN));
        host.wait_outcomes.push(Err(Errno::ESRCH)); // reap loop ends
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut noop_record,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut deps),
            Attempt::SingleUser
        );
        assert_eq!(host.slept, vec![30]);
    }

    #[test]
    fn test_runetcrc_stopped_shell_continues_then_succeeds() {
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((7, WaitStatus::Stopped { stopsig: 18 })));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut noop_record,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut deps),
            Attempt::ReadTtys
        );
        assert_eq!(host.kills, vec![(7, sig::SIGNAL_CONTINUE)]);
    }

    #[test]
    fn test_runetcrc_sigterm_with_catatonia_awaits_reboot() {
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((
            7,
            WaitStatus::Signaled { termsig: sig::SIGNAL_TERMINATE, core_dumped: false },
        )));
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &|| true,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut noop_record,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut deps),
            Attempt::AwaitReboot
        );
    }

    #[test]
    fn test_runetcrc_sigterm_without_catatonia_is_single_user() {
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((
            7,
            WaitStatus::Signaled { termsig: sig::SIGNAL_TERMINATE, core_dumped: false },
        )));
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut noop_record,
        };
        assert_eq!(
            runetcrc(&mut host, false, &mut deps),
            Attempt::SingleUser
        );
    }

    #[test]
    fn test_runcom_double_run_inside_chroot() {
        let mut host = ScriptHost::parent_only();
        host.root = Some("/newroot".into());
        host.fork_outcomes.push(Ok(7)); // first (plain) run
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        host.fork_outcomes.push(Ok(7)); // chrooted run
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut records = 0;
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/newroot",
            record_reboot: &mut || records += 1,
        };
        assert_eq!(
            runcom(&mut host, deps),
            RuncomResult::Booted { did_multiuser_chroot: true }
        );
        // The chroot itself is child-branch behavior — asserted in
        // test_runetcrc_chrooted_child_execs_after_chroot. Here we
        // assert the parent saw two clean attempts and one ledger
        // entry (C: init.c:1004-1012).
        assert_eq!(records, 1);
    }

    #[test]
    fn test_runcom_single_run_without_chroot() {
        let mut host = ScriptHost::parent_only();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut records = 0;
        let mut deps = RuncomDeps {
            mode: RuncomMode::Fastboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut || records += 1,
        };
        assert_eq!(
            runcom(&mut host, deps),
            RuncomResult::Booted { did_multiuser_chroot: false }
        );
        assert_eq!(records, 1);
    }

    #[test]
    fn test_runcom_rc_failure_propagates_without_ledger() {
        let mut host = ScriptHost::parent_only();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 2 })));
        let mut records = 0;
        let mut deps = RuncomDeps {
            mode: RuncomMode::Autoboot,
            catatonia_requested: &no_catatonia,
            collect: &mut no_collect,
            rootdir: "/",
            record_reboot: &mut || records += 1,
        };
        assert_eq!(runcom(&mut host, deps), RuncomResult::SingleUser);
        assert_eq!(records, 0);
    }
}
