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
//! this crate. Wired and running for real: fork/waitpid/kill, the file
//! family (open/stat/read/write/append/close), the signal family
//! (sigaction/sigprocmask), uid/setsid, the file-backed probes, exec
//! (PM_EXEC with the initial-stack frame, see [`crate::execve`]),
//! alarm (PM_ITIMER), the wall clock (PM_GETTIMEOFDAY), and
//! set_controlling_tty (open + TIOCSCTTY + dup2). Still honest `ENOSYS`
//! from [`MinixSysHost`]: the kernel mib trio only (see below for the
//! client-face dependency). Callers branch on the error
//! instead of on a compiled-out feature — the same policy `minix-sys`
//! uses for open-existing (`libs/minix-sys/src/lib.rs:186-190`).

use alloc::{string::String, string::ToString, vec::Vec};
use crate::session::ParsedCommand;
use crate::state_machine::sig;
use crate::state_machine::HandlerKind;
use crate::wait::{from_raw, WaitStatus};
use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::{self, Errno, Pid};
use minix_sys::pm::SigActionWire;

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

    /// The caller's real uid (C: getuid, init.c:242). Wired through
    /// E-INITSYS ②'s client wrapper.
    fn getuid(&self) -> Result<u32, Errno>;

    /// Become a session leader (C: setsid, init.c:255). Wired through
    /// E-INITSYS ②'s client wrapper.
    fn setsid(&mut self) -> Result<Pid, Errno>;

    /// Make `device` the controlling terminal on fds 0-2 (C:
    /// `setctty`, init.c:669-689: open O_RDWR, TIOCSCTTY, dup2 onto
    /// 0/1/2). ENOSYS until E-INITSYS ②.
    fn set_controlling_tty(&mut self, device: &str) -> Result<(), Errno>;

    /// Close fds 0, 1, 2 (C: init.c:339-341) so the console probe
    /// starts from a clean slate.
    fn close_std_fds(&mut self) -> Result<(), Errno>;

    // ── kernel mib (securelevel; ARCH A-4/A-5) ──
    //
    // The mib trio below stays honest `ENOSYS`. Correction (NS11 recon,
    // 2026-09-21): the earlier verdict here — "C reaches sysctl through
    // the kernel's SYS_GETMIB call" — contradicts the C ground truth.
    // C's sysctl(2) is a blocking SENDREC of MIB_SYSCTL to the MIB
    // service itself (com.h:1026; MIB_PROC_NR com.h:66; the service
    // decodes `m_lc_mib_sysctl` and gates on SENDREC, mib/main.c:292-
    // 306) — no kernel dispatch arm exists in C either. The real gap is
    // client-side: minix-sys has the constants (misc.rs) and minix-types
    // has both wire overlays (MessLcMibSysctl / the reply lane), but no
    // request-builder + sendrec + reply-decode function, and `init.root`
    // reads by name additionally need the sysctlbyname half (nametomib
    // via CTL_QUERY, lib libc gen/sysctl.c). Registered as NS11-A
    // (new_edge3 新登记): shared-library work in edge2 territory, to be
    // picked up on the claim board after the in-flight minix-sys claims
    // drain. Until that face lands these three stay ENOSYS — the C
    // behavior when a question cannot be asked is a warning and a
    // default, never a fabricated answer.

    /// Read the kernel security level; `Ok(None)` means the node does
    /// not exist (C: `getsecuritylevel` returning -1, init.c:569-587).
    /// ENOSYS until the minix-sys MIB_SYSCTL client face lands (NS11-A).
    fn securitylevel(&self) -> Result<Option<i32>, Errno>;

    /// Lower the security level; `Ok(false)` means unsupported or a
    /// no-op same-value set (C: `setsecuritylevel`, init.c:595-618).
    fn set_securitylevel(&mut self, level: i32) -> Result<bool, Errno>;

    /// Read the `init.root` chroot prefix (C: `shouldchroot`'s sysctl
    /// read, init.c:1859-1900). `Ok(None)` = node absent. ENOSYS until
    /// the minix-sys MIB_SYSCTL client face lands (NS11-A).
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

    /// Append bytes to a file (C: the utmp/wtmp ledger appends behind
    /// `pututxline`/`logwtmpx`, init.c:1446/1008). ENOSYS until
    /// E-INITSYS ② lands the open-existing path.
    fn append_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), Errno>;

    /// Read a whole file as text (C: fopen/getttyent/getpwnam reads of
    /// `/etc/ttys` and `/etc/passwd`). ENOSYS until E-INITSYS ②.
    fn read_file(&self, path: &str) -> Result<String, Errno>;
}

/// The real machine, over `minix-sys`.
///
/// Methods whose client wrappers do not exist yet return
/// `Err(Errno::ENOSYS)` — honest failure a caller can branch on, never
/// a panic and never a fake success (see the E-SYSCALL-SIGN lesson in
/// edge_todo.md).
#[derive(Debug, Default)]
pub struct MinixSysHost {
    /// The process environment this seam maintains (C: `environ`).
    /// `set_env` writes it (`setenv("PATH", INIT_PATH, 1)`, init.c:801)
    /// and `exec` folds it over the birth environment to make the child
    /// envp (`execv(shell, argv)` inherits `environ`, init.c:803).
    env: Vec<(String, String)>,
}

impl MinixSysHost {
    /// The sigreturn restore stub for the sigaction request's `ret`
    /// lane (C: `sigaction.c:18` `m.m_lc_pm_sig.ret = __sigreturn`).
    /// The stub lives in minix-rt (`signals::__sigreturn`, the NL3③
    /// deliverable, x86_64 leg); other arches keep the honest 0 until
    /// their delivery legs land, same as the stub module itself.
    #[cfg(target_arch = "x86_64")]
    fn sigreturn_stub() -> u64 {
        // fn 指针取址两步走（clippy function_casts_as_integer 建议形）：
        // 直接 `as u64` 是新告警面。
        minix_rt::signals::__sigreturn as *const () as u64
    }

    /// Non-x86_64 counterpart: no stub leg yet — carry 0 rather than
    /// invent an address the frame contract cannot honor.
    #[cfg(not(target_arch = "x86_64"))]
    fn sigreturn_stub() -> u64 {
        0
    }

    /// One sigaction round trip over `transport`: handler address in,
    /// sigreturn stub in the `ret` lane (C: `sigaction.c:15-19`). A
    /// free-standing associated function so tests can drive the same
    /// wire through a canned transport instead of the direct trap.
    fn send_sigaction(
        transport: &impl IpcTransport,
        signum: i32,
        handler: usize,
        sigreturn: u64,
    ) -> Result<(), Errno> {
        let act = SigActionWire {
            sa_handler: handler,
            sa_mask: [0; 4],
            sa_flags: 0,
            _pad: [0; 4],
        };
        minix_sys::pm::sigaction_via(transport, signum, Some(&act), None, sigreturn)
    }
}

impl InitHost for MinixSysHost {
    fn fork(&mut self) -> Result<Pid, Errno> {
        minix_sys::fork()
    }

    fn exec(&mut self, cmd: &ParsedCommand) -> Errno {
        // Real PM_EXEC (execve.c:33-58): the execve module builds the
        // initial-stack frame (argv/envp slots + strings + ps_strings)
        // and fills the five-field message. Hosted builds fail honest at
        // the kerninfo query — the new-image stack top only exists where
        // a kernel published it (EIO fallback, E1 slice 5) — and a real
        // machine answers with the PM verdict.
        crate::execve::exec_command(&self.env, cmd)
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
        minix_sys::pm::getuid_via(&DirectTrapTransport)
            .map(|(ruid, _euid)| ruid as u32)
    }

    fn setsid(&mut self) -> Result<Pid, Errno> {
        minix_sys::pm::setsid_via(&DirectTrapTransport)
    }

    fn set_controlling_tty(&mut self, device: &str) -> Result<(), Errno> {
        // C setctty (init.c:669-689) + login_tty (minix3/lib/libutil/
        // login_tty.c:46-61), folded: setsid (failure warned away,
        // init.c:674), the DTR-low gap (250 ms, init.c:98/680), open
        // O_RDWR, TIOCSCTTY, dup2 onto 0/1/2, close the working fd.
        let _ = minix_sys::pm::setsid_via(&DirectTrapTransport);
        let _ = minix_sys::misc::nanosleep_via(
            &DirectTrapTransport,
            Some(minix_sys::misc::SleepRequest {
                seconds: 0,
                nanoseconds: 250_000_000,
            }),
        );
        let fd = minix_sys::open(device, O_RDWR, 0)?;
        // login_tty.c:56-58: the ioctl failure IS the login_tty failure;
        // everything after it is best-effort.
        minix_sys::ioctl(fd, minix_sys::tty::TIOCSCTTY, 0)?;
        for std_fd in [0, 1, 2] {
            let _ = minix_sys::vfs::dup2_via(&DirectTrapTransport, fd, std_fd);
        }
        if fd > 2 {
            let _ = minix_sys::close(fd);
        }
        Ok(())
    }

    fn close_std_fds(&mut self) -> Result<(), Errno> {
        // C init.c:339-341: close(0), close(1), close(2) so the console
        // probe starts from a clean slate. The first failure is the
        // answer — C's `close(0); close(1); close(2);` ignores errors,
        // and the caller treats any failure as "console unavailable".
        for fd in [0, 1, 2] {
            minix_sys::close(fd)?;
        }
        Ok(())
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
        // Real installation: one sigaction per entry with the shared
        // trampoline as handler and minix-rt's `__sigreturn` as the
        // restore stub (C: libc folds the stub into every sigaction,
        // sigaction.c:18). The round trip is contract-complete: PM's
        // PM_SIGACTION arm stores the stub per process (`install` →
        // `sigreturn_addr`, mproc/signal.rs:236) and the delivery
        // snapshot publishes it back to the kernel frame planter
        // (mproc/signal.rs:335); the physical frame plant on the real
        // machine is the T2 power-on surface.
        let tramp = crate::signal_state::trampoline_address();
        let sigreturn = Self::sigreturn_stub();
        for (signum, kind) in &spec.handlers {
            let handler = match kind {
                HandlerKind::Ignore => 1usize, // SIG_IGN
                _ => tramp,
            };
            Self::send_sigaction(&DirectTrapTransport, *signum, handler, sigreturn)?;
        }
        if let Some(except) = &spec.blocked_except {
            minix_sys::pm::sigprocmask_via(&DirectTrapTransport, SIG_SETMASK, Some(&sigset_full_minus(except)))?;
        }
        Ok(())
    }

    fn alarm(&mut self, secs: u32) -> Result<(), Errno> {
        // C alarm(3) is the backwards-compatible setitimer shape
        // (minix3/lib/libc/gen/alarm.c:54-70): ITIMER_REAL, zero interval,
        // value = {secs, 0}. init discards the old value (init.c:1685), so
        // ovalue travels as NULL — PM reads that as "no get" (alarm.c:108).
        let timer = minix_sys::pm::ItimervalWire {
            it_interval: minix_sys::pm::TimevalWire { tv_sec: 0, tv_usec: 0 },
            it_value: minix_sys::pm::TimevalWire {
                tv_sec: secs as i64,
                tv_usec: 0,
            },
        };
        minix_sys::pm::setitimer_via(
            &DirectTrapTransport,
            minix_sys::pm::ITIMER_REAL,
            Some(&timer),
            None,
        )
    }

    fn now_secs(&self) -> Result<i64, Errno> {
        // C reads the wall clock through gettimeofday (init.c:1352 getty
        // spacing, the utmp timestamps); only whole seconds matter here.
        minix_sys::pm::gettimeofday_via(&DirectTrapTransport).map(|(sec, _nsec)| sec)
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
        // C probes paths with fopen/access (init.c:262 console, init.c:800
        // /bin/sh, init.c:962 /etc/rc); stat is the same existence
        // question without the open. ENOENT is the "no" answer; any other
        // error travels (a caller can distinguish "absent" from "broken").
        // C 的 stat 缓冲是 memset 零初始化：全整数域的 repr(C)，零是每个
        // 字段的合法位型。
        let mut buf: minix_sys::Stat = unsafe { core::mem::zeroed() };
        match minix_sys::stat(path, &mut buf) {
            Ok(()) => Ok(true),
            Err(Errno::ENOENT) => Ok(false),
            Err(other) => Err(other),
        }
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
        // C: setenv("PATH", INIT_PATH, 1) (init.c:799-801). Pure process
        // state — no machine round trip — so hosted builds and a real
        // machine agree; the child sees the value through the exec
        // frame's envp (`execve::compose_envp`).
        crate::execve::apply_env_override(&mut self.env, key, value)
    }

    fn chroot(&mut self, root: &str) -> Result<(), Errno> {
        // C: chroot(rootdir) before the chroot run of /etc/rc
        // (init.c:903); the path travels inline, the open shape.
        let mut path = Vec::with_capacity(root.len() + 1);
        path.extend_from_slice(root.as_bytes());
        path.push(0);
        minix_sys::vfs::chroot_via(&DirectTrapTransport, path.as_ptr() as u64, path.len())
    }

    fn append_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), Errno> {
        // The utmpx/wtmpx ledger appends (C: pututxline/logwtmpx write to
        // the accounting files, init.c:1446/1008): open for append
        // (O_WRONLY|O_APPEND, fcntl.h:65/:82), write the record whole,
        // close. The fd never outlives the call.
        let fd = minix_sys::open(path, O_WRONLY | O_APPEND, 0)?;
        let write_rv = minix_sys::write(fd, bytes);
        let _ = minix_sys::close(fd);
        write_rv.map(|_| ())
    }

    fn read_file(&self, path: &str) -> Result<String, Errno> {
        // C reads /etc/ttys and /etc/passwd whole (fopen/fgets loops,
        // init.c:698/733). Read into a growing buffer until EOF, then
        // decode UTF-8 lossily — init only matches ASCII structure, and
        // a lossy swap cannot lose a delimiter.
        let fd = minix_sys::open(path, O_RDONLY, 0)?;
        let mut body: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            match minix_sys::read(fd, &mut chunk) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&chunk[..n]),
                Err(e) => {
                    let _ = minix_sys::close(fd);
                    return Err(e);
                }
            }
        }
        let _ = minix_sys::close(fd);
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}

/// `open(2)` flag words this host uses (C `sys/sys/fcntl.h:64,65,66,82`).
const O_RDONLY: i32 = 0x0000;
const O_WRONLY: i32 = 0x0001;
const O_RDWR: i32 = 0x0002;
const O_APPEND: i32 = 0x0008;

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
    /// Recorded (path, text) appends.
    pub appends: Vec<(String, String)>,
    /// (path, contents) answers for `read_file`; missing path = ENOSYS.
    pub files: Vec<(String, String)>,
    /// Queue of append failures; empty queue = every append succeeds.
    pub append_errors: Vec<Errno>,
    /// Queue of kill failures; empty queue = every kill succeeds.
    pub kill_errors: Vec<Errno>,
    /// Queue of waitpid failures, served before `wait_outcomes`.
    pub wait_errors: Vec<Errno>,
    /// When set, `alarm` arms the shared clang flag — the scripted
    /// shape of "kernel raises SIGALRM, the handler sets clang".
    pub alarm_sets_clang: Option<alloc::sync::Arc<core::sync::atomic::AtomicBool>>,
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
        if !self.wait_outcomes.is_empty() {
            return self.wait_outcomes.remove(0);
        }
        if !self.wait_errors.is_empty() {
            return Err(self.wait_errors.remove(0));
        }
        panic!("script: waitpid requested but queue empty");
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
            clang.store(true, core::sync::atomic::Ordering::SeqCst);
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

    fn append_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), Errno> {
        if !self.append_errors.is_empty() {
            return Err(self.append_errors.remove(0));
        }
        self.appends
            .push((path.to_string(), String::from_utf8_lossy(bytes).into_owned()));
        Ok(())
    }

    fn read_file(&self, path: &str) -> Result<String, Errno> {
        for (candidate, contents) in &self.files {
            if candidate == path {
                return Ok(contents.clone());
            }
        }
        Err(Errno::ENOSYS)
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


/// C: `SIG_SETMASK 3` (`minix3/sys/sys/signal.h:176`).
const SIG_SETMASK: i32 = 3;

/// 全信号集（1..=64;C `sigfillset`,128 位 sigset_t 的高 64 位不用）。
fn sigset_full() -> [u32; 4] {
    let mut set = [0u32; 4];
    for sig in 1..=64usize {
        set[(sig - 1) / 32] |= 1 << ((sig - 1) % 32);
    }
    set
}

/// 全集减去 `except`（C: `delset` 变参循环,init.c:324-327）。
fn sigset_full_minus(except: &[i32]) -> [u32; 4] {
    let mut set = sigset_full();
    for &sig in except {
        if (1..=64).contains(&sig) {
            set[((sig - 1) / 32) as usize] &= !(1 << ((sig - 1) % 32));
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigaction_wire_carries_sigreturn_stub() {
        // C: libc folds the restore stub into every sigaction request
        // (`sigaction.c:18` `.ret = __sigreturn`). The rewrite's stub is
        // minix-rt's `__sigreturn` (NL3③); the install path must carry
        // that address in the `ret` lane — a zero slot was the NS11 gap.
        let canned = minix_sys::ipc::CannedTransport::new();
        let stub = MinixSysHost::sigreturn_stub();
        assert_ne!(stub, 0, "x86_64 测试面上桩地址必须非零");
        MinixSysHost::send_sigaction(&canned, 14, 0x5000, stub).unwrap_err();
        let (dest, sent) = canned.sent.borrow().last().cloned().unwrap();
        assert_eq!(dest, minix_sys::pm::pm_endpoint());
        assert_eq!(sent.m_type, minix_sys::pm::PM_CALL_SIGACTION);
        // SAFETY: byte-level read of the union overlay lane for the test
        // assertion only.
        let ret = unsafe { sent.m_u.m_lc_pm_sig.ret };
        assert_eq!(ret, stub);
        assert_eq!(
            ret,
            minix_rt::signals::__sigreturn as *const () as u64,
            "ret 槽必须精确等于 minix-rt 的 __sigreturn 符号地址"
        );
    }

    #[test]
    fn test_minix_host_honest_enosys_for_missing_wrappers() {
        let mut host = MinixSysHost::default();
        // 本测试分两栏：**仍缺封装**的接缝必须诚实回 ENOSYS（不假成功）；
        // **已接线**的接缝在宿主 trap 断链下诚实回 EIO（E1 切片 5 的 hosted
        // fallback；rt-birth 同款注记）——两栏都不许把失败装成功。
        // 仍缺封装：内核 mib 三件——缺的是 minix-sys 的 MIB_SYSCTL
        // 客户端面（NS11-A 登记），不是内核臂（C 亦无 GETMIB 系统调用，
        // sysctl(2) = SENDREC 直达 MIB 服务，见 trait 上方更正注记）。
        // （set_controlling_tty/alarm/time/chroot 已接线，见下；set_env
        // 是本地环境表面——无机器往返，见第三栏。）
        assert_eq!(host.securitylevel(), Err(Errno::ENOSYS));
        assert_eq!(host.set_securitylevel(0), Err(Errno::ENOSYS));
        assert_eq!(host.init_root(), Err(Errno::ENOSYS));
        // chroot 已接线（VFS_CHROOT 面）：宿主 trap 断链诚实回 EIO。
        assert_eq!(host.chroot("/"), Err(Errno::EIO));
        // set_env 已接线（本地环境表，setenv 语义）：成功路径 Ok，
        // 畸形名 EINVAL（C setenv.c:70-74）；exec 帧消费见 execve.rs。
        assert_eq!(host.set_env("PATH", "/sbin"), Ok(()));
        assert_eq!(host.set_env("", "/sbin"), Err(Errno::EINVAL));
        assert_eq!(host.set_env("PA=TH", "/sbin"), Err(Errno::EINVAL));
        // 已接线（宿主 trap 断链 → EIO）：文件族三件 + 信号安装。
        assert_eq!(host.path_exists("/dev/console"), Err(Errno::EIO));
        assert_eq!(host.read_file("/etc/ttys"), Err(Errno::EIO));
        assert_eq!(host.append_file("/var/run/utmpx", b"x"), Err(Errno::EIO));
        assert_eq!(host.close_std_fds(), Err(Errno::EIO));
        // 信号安装已是真实封装（E-INITSYS ①）：宿主 trap 断链诚实回
        // EIO（E1 切片 5 的 hosted fallback；rt-birth 同款注记）——不伪造
        // 成功。真机上 PM 服务该调用号（S3 的 dispatch 臂）后回真实结果，
        // main 告警后继续——两条路径都如实。
        let spec = default_signal_spec();
        assert!(
            matches!(host.register_handlers(&spec), Err(e) if e == Errno::EIO),
            "宿主 trap 断链 → EIO（不伪造成功）"
        );
        // alarm/time/set_controlling_tty 已接线（PM_ITIMER /
        // PM_GETTIMEOFDAY / open+TIOCSCTTY+dup2 面）：宿主 trap 断链
        // 诚实回 EIO。
        assert_eq!(host.alarm(10), Err(Errno::EIO));
        assert_eq!(host.now_secs(), Err(Errno::EIO));
        assert_eq!(
            host.set_controlling_tty("/dev/console"),
            Err(Errno::EIO)
        );
        // exec 已接线（PM_EXEC 面，execve.rs）：宿主 kerninfo 断链 →
        // 帧的 vsp 无从取值 → 诚实 EIO（不伪造成功）。
        assert_eq!(
            host.exec(&ParsedCommand {
                exec_path: "/bin/sh".into(),
                argv: vec!["sh".into(), "/etc/rc".into()],
            }),
            Errno::EIO
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
