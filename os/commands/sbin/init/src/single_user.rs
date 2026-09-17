//! Single-user rescue state ('s').
//!
//! Covers `minix3/sbin/init/init.c:694-877` (`single_user`).
//! Design contract: `.design/04-design.v1.md §1.1-§1.4`.
//!
//! The C function reads as three acts on one stage: the parent drops
//! the security level, silences SIGHUP/SIGTSTP, and forks; the child
//! claims the console, asks for the root password and an alternate
//! shell, then execs; the parent waits, collecting any other dying
//! child along the way and restarting the shell if it merely stopped.
//! Those acts are one state function here, driven through the
//! [`InitHost`] seam with the fork semantics intact — `Ok(0)` means
//! this very call now runs the child branch.

use crate::host::{default_signal_spec, ignore_spec, restore_spec, InitHost, SignalSpec};
use crate::log::{emergency, warning};
use crate::state_machine::{sig, HandlerKind, StateKind};
use crate::wait::{EINTR, WNOHANG, WUNTRACED};
use minix_sys::Errno;
use minix_sys::Pid;

/// Absolute path of the rc shell (C: `INIT_BSHELL` = `_PATH_BSHELL`,
/// init.c:105, exec'd at init.c:913).
pub const RC_SHELL_PATH: &str = "/bin/sh";
/// C: `_PATH_CONSTTY` (`minix3/include/paths.h:63`).
pub const CONSTTY_PATH: &str = "/dev/constty";
/// C: `_PATH_CONSOLE` (`minix3/include/paths.h:62`).
pub const CONSOLE_PATH: &str = "/dev/console";
/// C: `_PATH_STDPATH` (`minix3/include/paths.h:53-54`) — the child's
/// PATH (init.c:801).
pub const INIT_PATH: &str =
    "/usr/bin:/bin:/usr/sbin:/sbin:/usr/pkg/bin:/usr/pkg/sbin:/usr/local/bin:/usr/local/sbin";

/// Whether the password gate must prompt.
pub fn password_gate_required(
    console_secure: bool,
    from_securitylevel: i32,
    root_has_password: bool,
) -> bool {
    // C: typ && (from_securitylevel >= 2 || !(typ->ty_status & TTY_SECURE))
    //     && pp && *pw_passwd != '\0' (init.c:749-750).
    root_has_password && (from_securitylevel >= 2 || !console_secure)
}

/// One password prompt outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordAttempt {
    Success,
    /// Empty input (^D): exit 0 → proceed to multi-user path.
    EmptyExit,
    Retry,
}

/// Classify one prompt attempt (C: init.c:754-762).
pub fn classify_attempt(input_empty: bool, matches: bool) -> PasswordAttempt {
    if input_empty {
        PasswordAttempt::EmptyExit
    } else if matches {
        PasswordAttempt::Success
    } else {
        PasswordAttempt::Retry
    }
}

/// Choose the shell path (C: ALTSHELL block, init.c:781-782).
pub fn choose_shell(altshell_input: &str, default: &str) -> String {
    let trimmed = altshell_input.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// What `single_user` hands back to the transition loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    Continue,
    Transition(StateKind),
    RestartSingleUser,
    RebootQuiet,
    ProceedRuncomFastboot,
}

/// Classify one wait observation (pure; I/O stays in the driver).
///
/// The outcome order mirrors the C wait loop: a stopped child (only
/// reported under WUNTRACED, init.c:827) keeps the loop going, an
/// externally requested transition wins next, then the fatal-signal
/// ladder — SIGKILL means the operator killed the shell and init
/// reboots quietly (init.c:849-856), any other signal restarts the
/// shell — and any normal exit proceeds to `/etc/rc` with FASTBOOT
/// (init.c:866-870); the exit code is not consulted.
pub fn classify_wait(status: crate::wait::WaitStatus, requested: Option<StateKind>) -> WaitOutcome {
    use crate::wait::WaitStatus;
    if status.stopped() {
        return WaitOutcome::Continue;
    }
    if let Some(state) = requested {
        return WaitOutcome::Transition(state);
    }
    if status.signaled() {
        if status.signaled_by(sig::SIGNAL_KILL) {
            return WaitOutcome::RebootQuiet;
        }
        return WaitOutcome::RestartSingleUser;
    }
    if status.exited() {
        return WaitOutcome::ProceedRuncomFastboot;
    }
    WaitOutcome::RestartSingleUser
}

/// What `single_user` hands back to the driver (C: the `state_func_t`
/// returns, plus the SIGKILL ladder where C never returns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleUserOutcome {
    /// C: `return single_user` — fork failure, wait error, or the
    /// shell killed by a signal other than SIGKILL.
    Restart,
    /// C: `runcom_mode = FASTBOOT; return runcom` (init.c:871-874).
    ProceedRuncomFastboot,
    /// C: SIGKILL death — `sigfillset` + `for(;;) sigsuspend`
    /// (init.c:850-856). The operator ran `/sbin/reboot`; there is no
    /// next state, only a quiet wait for the end. The driver parks.
    AwaitReboot,
    /// C: `return requested_transition` — a signal asked for another
    /// state while the shell ran.
    Transition(StateKind),
}

/// Inputs the state function cannot compute itself.
pub struct SingleUserDeps<'a> {
    /// Matches a typed password against root's hash (C: the
    /// crypt+strcmp pair, init.c:766-768). `None` means no usable
    /// hash, so no gate — the C shape when `pw_passwd` is empty or
    /// getpwnam fails. Built by the password module (ARCH A-12).
    pub verify_password: Option<&'a dyn Fn(&str) -> bool>,
    /// Whether the console ttys line carries TTY_SECURE (C:
    /// `typ->ty_status`, init.c:749).
    pub console_secure: bool,
    /// Security level as read before the downgrade (C:
    /// `from_securitylevel`, init.c:711 — the gate looks at the value
    /// the kernel had, not the downgraded one).
    pub from_securitylevel: i32,
    /// The child reaper over the session table (C: `collect_child`,
    /// init.c:833 — the shell's own pid included; the table ignores
    /// unknown pids).
    pub collector: &'a mut crate::driver::ChildCollector<'a>,
    /// The externally requested transition (C: the
    /// `requested_transition` global).
    pub requested: &'a crate::signal_state::SignalState,
}

/// The 's' state: a rescue shell on the console, then FASTBOOT.
pub fn single_user(host: &mut dyn InitHost, deps: SingleUserDeps) -> SingleUserOutcome {
    // C: downgrade an active security level first (init.c:711-713);
    // the gate compares against the pre-downgrade value.
    if deps.from_securitylevel > 0 {
        host.set_securitylevel(0);
    }

    // C: SIG_IGN for SIGHUP/SIGTSTP around the shell (init.c:715-719);
    // restored from the default table on every exit path below.
    let ignore = ignore_spec(&[sig::SIGNAL_HANGUP, sig::SIGNAL_TERMINAL_STOP]);
    let _ = host.register_handlers(&ignore);

    let pid = match host.fork() {
        Ok(pid) => pid,
        Err(_) => {
            // C: "seriously hosed" — reap what is already dead and try
            // the whole state again (init.c:814-821).
            emergency(host, "can't fork single-user shell, trying again");
            while host.waitpid(-1, WNOHANG).is_ok() {}
            let restore = restore_spec(&[sig::SIGNAL_HANGUP, sig::SIGNAL_TERMINAL_STOP]);
            let _ = host.register_handlers(&restore);
            return SingleUserOutcome::Restart;
        }
    };

    if pid == 0 {
        child_shell(host, deps.verify_password, deps.console_secure, deps.from_securitylevel);
        // child_shell only returns on exec-failure paths where C calls
        // _exit(3); the seam's exit_process does the terminating.
        host.exit_process(3);
    }

    // C: `requested_transition = 0` before the wait loop (init.c:824).
    loop {
        let wpid_status = host.waitpid(-1, WUNTRACED);
        let mut shell_done: Option<crate::wait::WaitStatus> = None;
        match wpid_status {
            Ok((wpid, status)) => {
                deps.collector.collect(host, wpid, &status);
                if wpid == pid {
                    if status.stopped() {
                        // C: stopped shell — SIGCONT it and keep
                        // waiting (init.c:843-847).
                        warning(host, "shell stopped, restarting");
                        let _ = host.kill(pid, sig::SIGNAL_CONTINUE);
                    } else {
                        shell_done = Some(status);
                    }
                }
            }
            Err(err) if err.to_i32() == EINTR => {
                // C: EINTR continues the wait (init.c:831-832); the
                // request poll happens below, as in the C condition.
            }
            Err(err) if err.to_i32() == EINTR => continue,
            Err(_) => {
                warning(host, "wait for single-user shell failed; restarting");
                restore_after(host);
                return SingleUserOutcome::Restart;
            }
        }

        if let Some(status) = shell_done {
            if let Some(state) = deps.requested.take_requested() {
                restore_after(host);
                return SingleUserOutcome::Transition(state);
            }
            restore_after(host);
            return finish_shell(host, status);
        }

        if let Some(state) = deps.requested.take_requested() {
            restore_after(host);
            return SingleUserOutcome::Transition(state);
        }
    }
}

/// The post-loop ladder for the shell's own death (C: init.c:846-874).
fn finish_shell(host: &mut dyn InitHost, status: crate::wait::WaitStatus) -> SingleUserOutcome {
    if status.signaled() {
        if status.signaled_by(sig::SIGNAL_KILL) {
            // The operator ran /sbin/reboot; wait for the end quietly.
            return SingleUserOutcome::AwaitReboot;
        }
        warning(host, "single user shell terminated, restarting");
        return SingleUserOutcome::Restart;
    }
    // Any exit — 0 or not — proceeds to /etc/rc with FASTBOOT; C does
    // not read WEXITSTATUS here (init.c:871-874).
    SingleUserOutcome::ProceedRuncomFastboot
}

fn restore_after(host: &mut dyn InitHost) {
    let restore = restore_spec(&[sig::SIGNAL_HANGUP, sig::SIGNAL_TERMINAL_STOP]);
    let _ = host.register_handlers(&restore);
}

/// The child branch: claim the console, gate on the root password,
/// offer an alternate shell, then exec (C: init.c:721-813).
///
/// Never returns normally — exec replaces the image or the C `_exit(3)`
/// epilogue runs via [`InitHost::exit_process`].
fn child_shell(
    host: &mut dyn InitHost,
    verify_password: Option<&dyn Fn(&str) -> bool>,
    console_secure: bool,
    from_securitylevel: i32,
) {
    // setctty: constty when present, else the console (init.c:722-725).
    let tty = match host.path_exists(CONSTTY_PATH) {
        Ok(true) => CONSTTY_PATH,
        _ => CONSOLE_PATH,
    };
    let _ = host.set_controlling_tty(tty);

    // The SECURE gate: prompt until a password matches or ^D goes
    // multi-user (init.c:731-771). The gate runs only when a hash
    // exists and the console is not secure (or the level was >= 2 —
    // see `password_gate_required`).
    if let Some(verify) = verify_password {
        if password_gate_required(console_secure, from_securitylevel, true) {
            host.console_write(
                crate::log::Severity::Emerg,
                "Enter root password, or ^D to go multi-user",
            );
            loop {
                        match host.read_line() {
                    None => host.exit_process(0),
                    Some(clear) => {
                        if clear.is_empty() {
                            host.exit_process(0);
                        }
                        if verify(&clear) {
                            break;
                        }
                        warning(host, "single-user login failed");
                    }
                }
            }
        }
    }

    // ALTSHELL: offer an alternate shell path (init.c:779-789).
    host.console_write(
        crate::log::Severity::Emerg,
        &format!("Enter pathname of shell or RETURN for {RC_SHELL_PATH}: "),
    );
    let altshell = host.read_line().unwrap_or_default();
    let shell = choose_shell(&altshell, RC_SHELL_PATH);
    let mut argv0 = "-sh".to_string();
    if !altshell.trim().is_empty() {
        argv0 = shell.clone();
    }

    // C: setenv PATH, then the exec chain (init.c:795-813). The
    // fallback is unconditional: after the first failure C resets
    // argv[0] to "-sh" and retries INIT_BSHELL — even when the first
    // attempt already was INIT_BSHELL.
    let _ = host.set_env("PATH", INIT_PATH);
    let err = host.exec(&crate::session::ParsedCommand {
        exec_path: shell.clone(),
        argv: vec![argv0],
    });
    emergency(host, &format!("can't exec `{shell}' for single user: {err}"));
    let err = host.exec(&crate::session::ParsedCommand {
        exec_path: RC_SHELL_PATH.to_string(),
        argv: vec!["-sh".to_string()],
    });
    emergency(host, &format!("can't exec `{RC_SHELL_PATH}' for single user: {err}"));
    // C: plain `sleep(STALL_TIMEOUT)` — no log line (init.c:811).
    let _ = host.sleep_secs(crate::log::STALL_TIMEOUT_SECS);
    host.exit_process(3);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::ChildCollector;
    use crate::host::ScriptHost;
    use crate::session::Session;
    use crate::session_db::HashMapDb;
    use crate::signal_state::SignalState;
    use crate::wait::WaitStatus;

    fn fresh_collector<'a>(
        sessions: &'a mut Vec<Session>,
        db: &'a mut HashMapDb,
    ) -> ChildCollector<'a> {
        ChildCollector {
            sessions,
            db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        }
    }

    #[test]
    fn test_gate_requires_password_matrix() {
        assert!(password_gate_required(false, 0, true));
        assert!(password_gate_required(true, 2, true));
        assert!(!password_gate_required(true, 0, true));
        assert!(!password_gate_required(false, 0, false));
    }

    #[test]
    fn test_empty_input_exits() {
        assert_eq!(classify_attempt(true, false), PasswordAttempt::EmptyExit);
        assert_eq!(classify_attempt(false, true), PasswordAttempt::Success);
        assert_eq!(classify_attempt(false, false), PasswordAttempt::Retry);
    }

    #[test]
    fn test_choose_shell_default_and_alt() {
        assert_eq!(choose_shell("", RC_SHELL_PATH), RC_SHELL_PATH);
        assert_eq!(choose_shell("  ", RC_SHELL_PATH), RC_SHELL_PATH);
        assert_eq!(choose_shell("/bin/ksh\n", RC_SHELL_PATH), "/bin/ksh");
    }

    #[test]
    fn test_wait_stop_continues() {
        let stopped = WaitStatus::Stopped { stopsig: 18 };
        assert_eq!(classify_wait(stopped, None), WaitOutcome::Continue);
    }

    #[test]
    fn test_wait_requested_transitions() {
        assert_eq!(
            classify_wait(WaitStatus::Exited { code: 0 }, Some(StateKind::Death)),
            WaitOutcome::Transition(StateKind::Death)
        );
    }

    #[test]
    fn test_wait_sigkill_quiets() {
        let killed = WaitStatus::Signaled { termsig: 9, core_dumped: false };
        assert_eq!(classify_wait(killed, None), WaitOutcome::RebootQuiet);
    }

    #[test]
    fn test_wait_other_signal_restarts_single_user() {
        let hup = WaitStatus::Signaled { termsig: 1, core_dumped: false };
        assert_eq!(classify_wait(hup, None), WaitOutcome::RestartSingleUser);
    }

    #[test]
    fn test_wait_normal_proceeds_runcom_fastboot() {
        assert_eq!(
            classify_wait(WaitStatus::Exited { code: 0 }, None),
            WaitOutcome::ProceedRuncomFastboot
        );
        // Exit code is not consulted — C reads only WIFEXITED here.
        assert_eq!(
            classify_wait(WaitStatus::Exited { code: 7 }, None),
            WaitOutcome::ProceedRuncomFastboot
        );
    }

    #[test]
    fn test_entity_happy_path_runs_shell_then_fastboot() {
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: None,
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        assert_eq!(single_user(&mut host, deps), SingleUserOutcome::ProceedRuncomFastboot);
    }

    #[test]
    fn test_entity_downgrades_securitylevel_before_fork() {
        let mut host = ScriptHost::parent_only();
        host.securitylevel = Some(2);
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: None,
            console_secure: false,
            from_securitylevel: 2,
            collector: &mut collector,
            requested: &signals,
        };
        let _ = single_user(&mut host, deps);
        assert_eq!(host.securitylevel_sets, vec![0]);
    }

    #[test]
    fn test_entity_fork_failure_retries() {
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Err(Errno::EAGAIN));
        host.wait_errors.push(Errno::ESRCH); // the WNOHANG reap ends
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: None,
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        assert_eq!(single_user(&mut host, deps), SingleUserOutcome::Restart);
        assert!(host.console.iter().any(|(_, m)| m.contains("can't fork")));
    }

    #[test]
    fn test_entity_ignores_foreign_children_until_shell_exits() {
        // A getty-shaped orphan dies first; the loop keeps waiting for
        // the shell (C: `while (wpid != pid && !requested_transition)`).
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((42, WaitStatus::Exited { code: 1 })));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: None,
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        assert_eq!(single_user(&mut host, deps), SingleUserOutcome::ProceedRuncomFastboot);
    }

    #[test]
    fn test_entity_sigkill_death_awaits_reboot() {
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((
            7,
            WaitStatus::Signaled { termsig: 9, core_dumped: false },
        )));
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: None,
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        assert_eq!(single_user(&mut host, deps), SingleUserOutcome::AwaitReboot);
    }

    #[test]
    fn test_entity_requested_transition_wins_after_shell_exit() {
        let mut host = ScriptHost::parent_only();
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        signals.note_signal(20 - 2); // SIGTSTP(18) → catatonia
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: None,
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        assert_eq!(
            single_user(&mut host, deps),
            SingleUserOutcome::Transition(StateKind::Catatonia)
        );
    }

    #[test]
    fn test_entity_child_branch_gates_then_execs_default_shell() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        // fork says "you are the child"; the password opens the gate
        // and RETURN answers the alt-shell prompt — the first exec
        // request must carry the default shell with argv[0] "-sh".
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.paths.push((CONSTTY_PATH.into(), false));
        host.input_lines.push(Some("swordfish".into())); // gate opens
        host.input_lines.push(Some(String::new())); // altshell RETURN
        host.exec_outcomes.push(Errno::EPERM);
        host.exec_outcomes.push(Errno::EPERM);
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: Some(&|clear| clear == "swordfish"),
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| single_user(&mut host, deps)));
        assert_eq!(host.exec_requests.len(), 2);
        let cmd = &host.exec_requests[0];
        assert_eq!(cmd.exec_path, RC_SHELL_PATH);
        assert_eq!(cmd.argv, vec!["-sh"]);
        assert_eq!(host.env_sets, vec![("PATH".to_string(), INIT_PATH.to_string())]);
        // C: sleep(STALL_TIMEOUT) then _exit(3) after both execs fail.
        assert_eq!(host.slept, vec![30]);
        assert_eq!(host.exits, vec![3]);
    }

    #[test]
    fn test_entity_child_branch_matching_password_reaches_altshell_prompt() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.paths.push((CONSTTY_PATH.into(), true)); // constty preferred
        host.input_lines.push(Some("swordfish".into())); // gate opens
        host.input_lines.push(Some("/bin/ksh\n".into())); // alt shell
        host.exec_outcomes.push(Errno::EPERM);
        host.exec_outcomes.push(Errno::EPERM);
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: Some(&|clear| clear == "swordfish"),
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| single_user(&mut host, deps)));
        // First attempt execs the operator's shell; the fallback resets
        // argv[0] to "-sh" on INIT_BSHELL (init.c:803-808).
        assert_eq!(host.exec_requests.len(), 2);
        assert_eq!(host.exec_requests[0].argv[0], "/bin/ksh");
        assert_eq!(host.exec_requests[1].argv[0], "-sh");
        assert!(!host.console.iter().any(|(_, m)| m.contains("login failed")));
    }

    #[test]
    fn test_entity_child_branch_wrong_password_reprompts() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.input_lines.push(Some("wrong".into()));
        host.input_lines.push(Some("swordfish".into()));
        host.input_lines.push(Some(String::new())); // altshell RETURN
        host.exec_outcomes.push(Errno::EPERM);
        host.exec_outcomes.push(Errno::EPERM);
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: Some(&|clear| clear == "swordfish"),
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| single_user(&mut host, deps)));
        assert_eq!(
            host.console.iter().filter(|(_, m)| m.contains("login failed")).count(),
            1
        );
    }

    #[test]
    fn test_entity_child_branch_eof_on_password_exits_zero() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        // ^D at the password gate: C _exit(0)s and the parent proceeds
        // to runcom — no exec ever happens (init.c:758-760).
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.input_lines.push(None); // EOF = ^D
        let mut sessions: Vec<Session> = Vec::new();
        let mut db = HashMapDb::default();
        let signals = std::sync::Arc::new(SignalState::default());
        let mut collector = fresh_collector(&mut sessions, &mut db);
        let deps = SingleUserDeps {
            verify_password: Some(&|_| false),
            console_secure: false,
            from_securitylevel: 0,
            collector: &mut collector,
            requested: &signals,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| single_user(&mut host, deps)));
        assert_eq!(host.exits, vec![0]);
        assert!(host.exec_requests.is_empty());
    }

    #[test]
    fn test_ignore_then_restore_shapes_are_data() {
        // The SIG_IGN window and the restore are spec data the host
        // installs — the pattern C implements with satstp/sahup.
        let ignore = ignore_spec(&[sig::SIGNAL_HANGUP, sig::SIGNAL_TERMINAL_STOP]);
        assert_eq!(
            ignore.handlers,
            vec![
                (sig::SIGNAL_HANGUP, HandlerKind::Ignore),
                (sig::SIGNAL_TERMINAL_STOP, HandlerKind::Ignore),
            ]
        );
        assert_eq!(ignore.blocked_except, None);
        let restore = restore_spec(&[sig::SIGNAL_HANGUP, sig::SIGNAL_TERMINAL_STOP]);
        assert_eq!(
            restore.handlers,
            vec![
                (sig::SIGNAL_HANGUP, HandlerKind::Transition),
                (sig::SIGNAL_TERMINAL_STOP, HandlerKind::Transition),
            ]
        );
        assert_eq!(default_signal_spec().handlers.len(), 6);
    }
}
