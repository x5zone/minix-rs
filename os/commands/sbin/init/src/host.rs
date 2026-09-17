//! The single seam between init's decision logic and the machine.
//!
//! **[ARCH: init-host-seam]** — architecture evolution recorded on all
//! three levels (plan.md §4, stage docs, this file): the seven earlier
//! per-concern traits (`DeviceProbe`, `SignalRegistry`, `TransitionDriver`,
//! `LogSink`, `Clock`, `SessionDb`, `SecureLevel`) collapse into one
//! `InitHost` surface for side effects, while decision logic stays in
//! free functions. The design borrows the embedded-hal shape — one trait
//! per consumer, many providers — and replaces per-concern fakes with a
//! single scripted host, so no trait in this crate exists with a single
//! behaviorally distinct implementation.
//!
//! Which methods are live today is a property of `minix-sys`, not of
//! this crate: fork/waitpid/kill/write/sleep have wrappers and run for
//! real; the rest (path exec, signals, setsid, controlling tty, uid,
//! alarm, clock, path probe) return honest `ENOSYS` from
//! [`MinixSysHost`] — the same policy `minix-sys` uses for
//! open-existing (`libs/minix-sys/src/lib.rs:186-190`). Callers branch
//! on the error instead of on a compiled-out feature. Closing those
//! gaps is edge E-INITSYS, owned by the shared-infrastructure lane.

use crate::session::ParsedCommand;
use crate::state_machine::sig;
use crate::state_machine::HandlerKind;
use crate::wait::{from_raw, WaitStatus};
use minix_sys::ipc::DirectTrapTransport;
use minix_sys::{self, Errno, Pid};

/// Which signals want which handler, plus the block mask.
///
/// Data, not closures: real signal handlers are `extern "C"`
/// trampolines that only set atomic flags, so the wiring can be
/// described as a table and installed by the host (C: the `handle`
/// and `delset` call sites, init.c:310-334).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignalSpec {
    /// (signum, handler) pairs, in registration order (C: one
    /// `handle()` line per group). Entries install incrementally:
    /// signums not listed keep their current disposition.
    pub handlers: Vec<(i32, HandlerKind)>,
    /// `None` leaves the signal mask alone; `Some(except)` sets the
    /// mask to "block everything except `except`" — `Some(vec![])` is
    /// the unblock-all of the post-fork child (C: `sigprocmask`
    /// SIG_SETMASK with an empty set, init.c:793-795).
    pub blocked_except: Option<Vec<i32>>,
}

/// Everything init does to the machine, in one place.
///
/// Decision logic never calls `minix-sys` directly — it calls this
/// trait, which makes every state function unit-testable against a
/// scripted host and keeps the syscall boundary answerable by reading
/// one impl.
pub trait InitHost {
    // ── process control ──

    /// Fork a child. `Ok(0)` means "you are the child", per fork
    /// semantics; the state functions match on it like C does.
    fn fork(&mut self) -> Result<Pid, Errno>;

    /// Exec a prepared spawn request. On success this never returns;
    /// the returned error is why the image did not start.
    fn exec(&mut self, cmd: &ParsedCommand) -> Errno;

    /// Terminate the calling process; only meaningful on the child
    /// branch of a fork, before exec replaces the image.
    fn exit_process(&mut self, status: i32) -> !;

    /// Wait for a child, decoding the raw status into a
    /// [`WaitStatus`]. `pid` may be -1 for any child; `options` takes
    /// [`crate::wait::WUNTRACED`]/[`crate::wait::WNOHANG`].
    fn waitpid(&mut self, pid: Pid, options: i32) -> Result<(Pid, WaitStatus), Errno>;

    /// Send a signal; `pid` may be -1 for "every process" (C:
    /// `kill(-1, sig)` in `death`, init.c:1681).
    fn kill(&mut self, pid: Pid, signum: i32) -> Result<(), Errno>;

    // ── identity and session leadership ──

    /// The caller's pid (C: getpid, used by the identity gate,
    /// init.c:248).
    fn getpid(&self) -> Result<Pid, Errno>;

    /// The caller's real uid (C: getuid, init.c:242). ENOSYS until
    /// E-INITSYS ② lands the client wrapper.
    fn getuid(&self) -> Result<u32, Errno>;

    /// Become a session leader (C: setsid, init.c:255). ENOSYS until
    /// E-INITSYS ②.
    fn setsid(&mut self) -> Result<Pid, Errno>;

    /// Make `device` the controlling terminal on fds 0-2 (C:
    /// `setctty`, init.c:669-689: open O_RDWR, TIOCSCTTY, dup2 onto
    /// 0/1/2). ENOSYS until E-INITSYS ②.
    fn set_controlling_tty(&mut self, device: &str) -> Result<(), Errno>;

    /// Close fds 0, 1, 2 (C: init.c:339-341) so the console probe
    /// starts from a clean slate.
    fn close_std_fds(&mut self) -> Result<(), Errno>;

    // ── kernel mib (securelevel; ARCH A-4/A-5) ──

    /// Read the kernel security level; `Ok(None)` means the node does
    /// not exist (C: `getsecuritylevel` returning -1, init.c:569-587).
    /// ENOSYS until the kernel mib face lands.
    fn securitylevel(&self) -> Result<Option<i32>, Errno>;

    /// Lower the security level; `Ok(false)` means unsupported or a
    /// no-op same-value set (C: `setsecuritylevel`, init.c:595-618).
    fn set_securitylevel(&mut self, level: i32) -> Result<bool, Errno>;

    /// Read the `init.root` chroot prefix (C: `shouldchroot`'s sysctl
    /// read, init.c:1859-1900). `Ok(None)` = node absent. ENOSYS until
    /// the kernel mib face lands.
    fn init_root(&self) -> Result<Option<String>, Errno>;

    // ── signals and time ──

    /// Install the handler table and block mask (C: `handle`/`delset`,
    /// init.c:310-334). ENOSYS until E-INITSYS ①.
    fn register_handlers(&mut self, spec: &SignalSpec) -> Result<(), Errno>;

    /// Ask for SIGALRM after `secs` seconds (C: alarm, init.c:1685).
    /// ENOSYS until E-INITSYS ②.
    fn alarm(&mut self, secs: u32) -> Result<(), Errno>;

    /// Wall-clock seconds (C: gettimeofday, used for getty spacing at
    /// init.c:1352 and the utmp timestamps). An error means "no
    /// clock"; callers degrade (skip the debounce) rather than stall.
    fn now_secs(&self) -> Result<i64, Errno>;

    /// Sleep whole seconds (C: sleep(3) — STALL_TIMEOUT, GETTY_SLEEP,
    /// DEATH_WATCH).
    fn sleep_secs(&mut self, secs: u64) -> Result<(), Errno>;

    // ── filesystem and console ──

    /// Whether a path exists (C: stat/access in the console probe,
    /// init.c:1729). ENOSYS until E-INITSYS ② lands open/stat.
    fn path_exists(&self, path: &str) -> Result<bool, Errno>;

    /// Write a line to the console (C: the syslog fallback path; the
    /// `#if 0` `print_console` stays unmodelled per doc 03).
    fn console_write(&mut self, severity: crate::log::Severity, message: &str);

    /// Read one line from the console (fd 0): the single-user password
    /// and alt-shell prompts (C: `getpass`/`fgets`, init.c:756/782).
    /// `None` is EOF — the ^D "go multi-user" answer.
    fn read_line(&mut self) -> Option<String>;

    /// Set an environment variable in this process (C: `setenv("PATH",
    /// INIT_PATH, 1)`, init.c:801) so the exec'd child inherits it.
    fn set_env(&mut self, key: &str, value: &str) -> Result<(), Errno>;

    /// Change the process root (C: `chroot(rootdir)` in the chroot run
    /// of `/etc/rc`, init.c:903). ENOSYS until E-INITSYS ②.
    fn chroot(&mut self, root: &str) -> Result<(), Errno>;
}

/// The real machine, over `minix-sys`.
///
/// Methods whose client wrappers do not exist yet return
/// `Err(Errno::ENOSYS)` — honest failure a caller can branch on, never
/// a panic and never a fake success (see the E-SYSCALL-SIGN lesson in
/// edge_todo.md).
#[derive(Debug, Default)]
pub struct MinixSysHost;

impl InitHost for MinixSysHost {
    fn fork(&mut self) -> Result<Pid, Errno> {
        minix_sys::fork()
    }

    fn exec(&mut self, cmd: &ParsedCommand) -> Errno {
        // Path-based exec is a PM call (PM_EXEC) whose client wrapper
        // does not exist yet; the prepared-image exec in minix-sys is
        // for boot-procedure images, not for spawning /bin/sh.
        let _ = cmd;
        Errno::ENOSYS
    }

    fn exit_process(&mut self, status: i32) -> ! {
        minix_sys::exit(status)
    }

    fn waitpid(&mut self, pid: Pid, options: i32) -> Result<(Pid, WaitStatus), Errno> {
        let mut raw = 0;
        let child = minix_sys::waitpid(pid, &mut raw, options)?;
        Ok((child, from_raw(raw)))
    }

    fn kill(&mut self, pid: Pid, signum: i32) -> Result<(), Errno> {
        minix_sys::kill(pid, signum)
    }

    fn getpid(&self) -> Result<Pid, Errno> {
        minix_sys::pm::getpid_via(&DirectTrapTransport)
    }

    fn getuid(&self) -> Result<u32, Errno> {
        Err(Errno::ENOSYS)
    }

    fn setsid(&mut self) -> Result<Pid, Errno> {
        Err(Errno::ENOSYS)
    }

    fn set_controlling_tty(&mut self, device: &str) -> Result<(), Errno> {
        let _ = device;
        Err(Errno::ENOSYS)
    }

    fn close_std_fds(&mut self) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn securitylevel(&self) -> Result<Option<i32>, Errno> {
        Err(Errno::ENOSYS)
    }

    fn set_securitylevel(&mut self, level: i32) -> Result<bool, Errno> {
        let _ = level;
        Err(Errno::ENOSYS)
    }

    fn init_root(&self) -> Result<Option<String>, Errno> {
        Err(Errno::ENOSYS)
    }

    fn register_handlers(&mut self, spec: &SignalSpec) -> Result<(), Errno> {
        let _ = spec;
        Err(Errno::ENOSYS)
    }

    fn alarm(&mut self, secs: u32) -> Result<(), Errno> {
        let _ = secs;
        Err(Errno::ENOSYS)
    }

    fn now_secs(&self) -> Result<i64, Errno> {
        Err(Errno::ENOSYS)
    }

    fn sleep_secs(&mut self, secs: u64) -> Result<(), Errno> {
        minix_sys::misc::nanosleep_via(
            &DirectTrapTransport,
            Some(minix_sys::misc::SleepRequest {
                seconds: secs as i64,
                nanoseconds: 0,
            }),
        )
    }

    fn path_exists(&self, path: &str) -> Result<bool, Errno> {
        let _ = path;
        Err(Errno::ENOSYS)
    }

    fn console_write(&mut self, severity: crate::log::Severity, message: &str) {
        let _ = severity;
        // fd 1 is whatever the boot environment left attached; a
        // failed console write has nowhere better to go.
        let _ = minix_sys::write(1, message.as_bytes());
        let _ = minix_sys::write(1, b"\n");
    }

    fn read_line(&mut self) -> Option<String> {
        // Byte-at-a-time fd-0 read until newline; EOF before any byte
        // is None (the ^D path). No echo and no history — getpass
        // semantics, not readline.
        let mut line = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            match minix_sys::read(0, &mut byte) {
                Ok(0) | Err(_) => {
                    return if line.is_empty() { None } else { Some(String::from_utf8_lossy(&line).into_owned()) }
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        return Some(String::from_utf8_lossy(&line).into_owned());
                    }
                    line.push(byte[0]);
                }
            }
        }
    }

    fn set_env(&mut self, key: &str, value: &str) -> Result<(), Errno> {
        // minix-rt has no env mutation yet (E-CMDSYSFACE); the child
        // would exec without the PATH override.
        let _ = (key, value);
        Err(Errno::ENOSYS)
    }

    fn chroot(&mut self, root: &str) -> Result<(), Errno> {
        let _ = root;
        Err(Errno::ENOSYS)
    }
}

/// A scripted host for tests: every effect is a queued outcome or a
/// recorded observation.
#[derive(Debug, Default)]
pub struct ScriptHost {
    /// Queue of fork outcomes; `Ok(0)` sends the caller down the child
    /// branch, `Ok(n)` is the parent's view.
    pub fork_outcomes: Vec<Result<Pid, Errno>>,
    /// Queue of waitpid outcomes.
    pub wait_outcomes: Vec<Result<(Pid, WaitStatus), Errno>>,
    /// Queue of exec outcomes (only reached on a child branch that
    /// survives to exec, which tests use to assert spawn requests).
    pub exec_outcomes: Vec<Errno>,
    pub kills: Vec<(Pid, i32)>,
    pub alarms: Vec<u32>,
    pub slept: Vec<u64>,
    pub console: Vec<(crate::log::Severity, String)>,
    pub exec_requests: Vec<ParsedCommand>,
    pub now: i64,
    pub registered: Option<SignalSpec>,
    pub paths: Vec<(String, bool)>,
    /// `None` = node absent (has_securelevel false); `Some(n)` = level n.
    pub securitylevel: Option<i32>,
    pub securitylevel_sets: Vec<i32>,
    pub root: Option<String>,
    /// Queue of console input lines (prompts); empty queue = EOF.
    pub input_lines: Vec<Option<String>>,
    pub env_sets: Vec<(String, String)>,
    /// Statuses handed to `exit_process` before the (test-side) panic.
    pub exits: Vec<i32>,
    pub chroots: Vec<String>,
    /// Whether scripted `chroot` calls succeed (default true).
    pub chroot_ok: bool,
    /// Queue of kill failures; empty queue = every kill succeeds.
    pub kill_errors: Vec<Errno>,
    /// Queue of waitpid failures, served before `wait_outcomes`.
    pub wait_errors: Vec<Errno>,
    /// When set, `alarm` arms the shared clang flag — the scripted
    /// shape of "kernel raises SIGALRM, the handler sets clang".
    pub alarm_sets_clang: Option<std::sync::Arc<crate::signal_state::AlarmFlag>>,
}

impl ScriptHost {
    /// A host whose fork reports the parent side with child pid 7 —
    /// the pid parent-branch test scripts should wait for.
    pub fn parent_only() -> ScriptHost {
        ScriptHost {
            fork_outcomes: vec![Ok(7)],
            ..ScriptHost::default()
        }
    }
}

impl InitHost for ScriptHost {
    fn fork(&mut self) -> Result<Pid, Errno> {
        if self.fork_outcomes.is_empty() {
            panic!("script: fork requested but queue empty");
        }
        self.fork_outcomes.remove(0)
    }

    fn exec(&mut self, cmd: &ParsedCommand) -> Errno {
        self.exec_requests.push(cmd.clone());
        if self.exec_outcomes.is_empty() {
            panic!("script: exec requested but queue empty");
        }
        self.exec_outcomes.remove(0)
    }

    fn exit_process(&mut self, status: i32) -> ! {
        self.exits.push(status);
        panic!("script: child exit({status}) — end of a child branch")
    }

    fn waitpid(&mut self, pid: Pid, options: i32) -> Result<(Pid, WaitStatus), Errno> {
        let _ = (pid, options);
        if !self.wait_errors.is_empty() {
            return Err(self.wait_errors.remove(0));
        }
        if self.wait_outcomes.is_empty() {
            panic!("script: waitpid requested but queue empty");
        }
        self.wait_outcomes.remove(0)
    }

    fn kill(&mut self, pid: Pid, signum: i32) -> Result<(), Errno> {
        self.kills.push((pid, signum));
        if !self.kill_errors.is_empty() {
            return Err(self.kill_errors.remove(0));
        }
        Ok(())
    }

    fn getpid(&self) -> Result<Pid, Errno> {
        Ok(1)
    }

    fn getuid(&self) -> Result<u32, Errno> {
        Ok(0)
    }

    fn setsid(&mut self) -> Result<Pid, Errno> {
        Ok(1)
    }

    fn set_controlling_tty(&mut self, device: &str) -> Result<(), Errno> {
        let _ = device;
        Ok(())
    }

    fn close_std_fds(&mut self) -> Result<(), Errno> {
        Ok(())
    }

    fn securitylevel(&self) -> Result<Option<i32>, Errno> {
        Ok(self.securitylevel)
    }

    fn set_securitylevel(&mut self, level: i32) -> Result<bool, Errno> {
        match self.securitylevel {
            None => Ok(false),
            Some(current) if current == level => Ok(false),
            Some(_) => {
                self.securitylevel = Some(level);
                self.securitylevel_sets.push(level);
                Ok(true)
            }
        }
    }

    fn init_root(&self) -> Result<Option<String>, Errno> {
        Ok(self.root.clone())
    }

    fn register_handlers(&mut self, spec: &SignalSpec) -> Result<(), Errno> {
        self.registered = Some(spec.clone());
        Ok(())
    }

    fn alarm(&mut self, secs: u32) -> Result<(), Errno> {
        self.alarms.push(secs);
        if let Some(clang) = &self.alarm_sets_clang {
            clang.set();
        }
        Ok(())
    }

    fn now_secs(&self) -> Result<i64, Errno> {
        Ok(self.now)
    }

    fn sleep_secs(&mut self, secs: u64) -> Result<(), Errno> {
        self.slept.push(secs);
        Ok(())
    }

    fn path_exists(&self, path: &str) -> Result<bool, Errno> {
        for (candidate, exists) in &self.paths {
            if candidate == path {
                return Ok(*exists);
            }
        }
        Ok(false)
    }

    fn console_write(&mut self, severity: crate::log::Severity, message: &str) {
        self.console.push((severity, message.to_string()));
    }

    fn read_line(&mut self) -> Option<String> {
        if self.input_lines.is_empty() {
            None
        } else {
            self.input_lines.remove(0)
        }
    }

    fn set_env(&mut self, key: &str, value: &str) -> Result<(), Errno> {
        self.env_sets.push((key.to_string(), value.to_string()));
        Ok(())
    }

    fn chroot(&mut self, root: &str) -> Result<(), Errno> {
        self.chroots.push(root.to_string());
        if self.chroot_ok {
            Ok(())
        } else {
            Err(Errno::EPERM)
        }
    }
}

/// The signal set init registers, as a [`SignalSpec`] (C: the `handle`
/// calls at init.c:319-334 and `delset` at init.c:324-327).
///
/// Lives here so the spec, the seam that consumes it, and the
/// transition mapping stay in one place.
pub fn default_signal_spec() -> SignalSpec {
    SignalSpec {
        handlers: vec![
            (sig::SIGNAL_HANGUP, HandlerKind::Transition),
            (sig::SIGNAL_TERMINATE, HandlerKind::Transition),
            (sig::SIGNAL_TERMINAL_STOP, HandlerKind::Transition),
            (sig::SIGNAL_ALARM, HandlerKind::Alarm),
            (sig::SIGNAL_ABORT, HandlerKind::Reboot),
            (sig::SIGNAL_USER_1, HandlerKind::Powerdown),
        ],
        blocked_except: Some(vec![
            sig::SIGNAL_HANGUP,
            sig::SIGNAL_TERMINATE,
            sig::SIGNAL_TERMINAL_STOP,
            sig::SIGNAL_ALARM,
        ]),
    }
}

/// Install a few handler entries without touching the mask — the
/// pre-fork SIG_IGN window and its post-loop restore (C: the
/// `sigaction(SIGTSTP/SIGHUP, ...)` pairs in `single_user`/`runetcrc`).
pub fn ignore_spec(signums: &[i32]) -> SignalSpec {
    SignalSpec {
        handlers: signums.iter().map(|&s| (s, HandlerKind::Ignore)).collect(),
        blocked_except: None,
    }
}

/// Re-install the default disposition of `signums` (the C pattern of
/// restoring the saved `satstp`/`sahup` actions, init.c:858-862).
pub fn restore_spec(signums: &[i32]) -> SignalSpec {
    let default = default_signal_spec();
    SignalSpec {
        handlers: default
            .handlers
            .into_iter()
            .filter(|(s, _)| signums.contains(s))
            .collect(),
        blocked_except: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minix_host_honest_enosys_for_missing_wrappers() {
        let mut host = MinixSysHost;
        assert_eq!(host.getuid(), Err(Errno::ENOSYS));
        assert_eq!(host.setsid(), Err(Errno::ENOSYS));
        assert_eq!(host.alarm(10), Err(Errno::ENOSYS));
        assert_eq!(host.path_exists("/dev/console"), Err(Errno::ENOSYS));
        assert_eq!(host.securitylevel(), Err(Errno::ENOSYS));
        assert_eq!(host.set_securitylevel(0), Err(Errno::ENOSYS));
        assert_eq!(host.init_root(), Err(Errno::ENOSYS));
        let spec = default_signal_spec();
        assert_eq!(host.register_handlers(&spec), Err(Errno::ENOSYS));
        assert_eq!(
            host.exec(&ParsedCommand {
                exec_path: "/bin/sh".into(),
                argv: vec!["sh".into(), "/etc/rc".into()],
            }),
            Errno::ENOSYS
        );
    }

    #[test]
    fn test_default_spec_matches_c_registration() {
        // init.c:319 (transition trio), init.c:320-326 (alarm/reboot/
        // powerdown/disaster groups), init.c:324-327 (delset exempt
        // SIGHUP/SIGTERM/SIGTSTP/SIGALRM).
        let spec = default_signal_spec();
        assert_eq!(spec.handlers.len(), 6);
        assert!(spec.handlers.contains(&(1, HandlerKind::Transition)));
        assert!(spec.handlers.contains(&(15, HandlerKind::Transition)));
        assert!(spec.handlers.contains(&(18, HandlerKind::Transition)));
        assert!(spec.handlers.contains(&(14, HandlerKind::Alarm)));
        assert!(spec.handlers.contains(&(6, HandlerKind::Reboot)));
        assert!(spec.handlers.contains(&(30, HandlerKind::Powerdown)));
        assert_eq!(spec.blocked_except, Some(vec![1, 15, 18, 14]));
    }

    #[test]
    fn test_script_host_records_and_plays_back() {
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        host.paths.push(("/dev/console".into(), true));

        assert_eq!(host.fork(), Ok(7));
        assert_eq!(host.waitpid(-1, crate::wait::WUNTRACED), Ok((7, WaitStatus::Exited { code: 0 })));
        assert_eq!(host.path_exists("/dev/console"), Ok(true));
        assert_eq!(host.path_exists("/nope"), Ok(false));

        host.console_write(crate::log::Severity::Alert, "boot");
        assert_eq!(host.console.len(), 1);
        assert_eq!(host.slept.len(), 0);
        host.sleep_secs(30);
        assert_eq!(host.slept, vec![30]);
    }

    #[test]
    fn test_script_host_child_branch_scripts_fork_zero() {
        let mut host = ScriptHost::default();
        host.fork_outcomes.push(Ok(0));
        host.exec_outcomes.push(Errno::EPERM);
        assert_eq!(host.fork(), Ok(0));
        let cmd = ParsedCommand {
            exec_path: "/bin/sh".into(),
            argv: vec!["sh".into()],
        };
        assert_eq!(host.exec(&cmd), Errno::EPERM);
        assert_eq!(host.exec_requests.len(), 1);
    }
}
