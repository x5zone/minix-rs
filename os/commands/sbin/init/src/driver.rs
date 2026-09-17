//! The transition driver: dispatch, shared machine state, runlevel
//! ledger.
//!
//! Covers `minix3/sbin/init/init.c:624-640` (`transition`) and the
//! per-state plumbing C scatters through globals (boot flags, the
//! session table, the ttys lines, the chroot state).
//! Design contract: `.design/02-design.v1.md §1.1-§1.4`.
//!
//! C's `transition` is seven lines: call the state function, write the
//! run-level ledger entry, repeat forever. The Rust driver keeps that
//! skeleton — the `match` below is the dispatch table the C compiler
//! built from `state_t` function pointers — and everything the states
//! share lives in one [`DriverState`] the driver owns, sliced into
//! per-state views ([`ChildCollector`], [`Ledger`]) so the C globals
//! stay single-owned instead of captured ad hoc. That delete completes
//! P1-7: the earlier `TransitionDriver` trait could only test
//! arbitrary fake graphs; the real graph is testable by driving the
//! real state functions over a scripted host.

use crate::entry::RuncomMode;
use crate::host::InitHost;
use crate::multi_user::multi_user;
use crate::session::Session;
use crate::signal_state::SignalState;
use crate::shutdown::{death, ShutdownDeps};
use crate::single_user::{single_user, SingleUserDeps, SingleUserOutcome};
use crate::state_machine::StateKind;
use crate::utmp::{runlevel_for, runlevel_for_optional};
use crate::ttys::{parse_ttys_line, TtysLine};
use crate::utmp::{logwtmpx, utmpx_set_runlevel, RecordType};
use crate::wait::WaitStatus;
use minix_sys::Pid;
use std::sync::Arc;

/// The ttys table (C: `_PATH_TTYS`; `minix3/etc/ttys` is the sample).
pub const TTYS_PATH: &str = "/etc/ttys";

/// Everything the state functions share, owned by the driver.
pub struct DriverState {
    /// Parsed boot flags (C: the `-s` seed and `runcom_mode`).
    pub boot_args: crate::entry::BootArgs,
    /// The current `/etc/rc` mode (C: `runcom_mode`, init.c:151).
    pub mode: RuncomMode,
    /// Whether the console probe succeeded (C: init.c:269-270).
    pub console_ok: bool,
    /// Whether the console ttys line carries TTY_SECURE (doc 04 gate).
    pub console_secure: bool,
    /// Security level as seen at entry (C: `from_securitylevel`).
    pub from_securitylevel: i32,
    /// The session table, in ttys order.
    pub sessions: Vec<Session>,
    /// The session database (pid → index).
    pub db: crate::session_db::HashMapDb,
    /// Async-written signal state (C: `clang` +
    /// `requested_transition` + the hook requests).
    pub signals: Arc<SignalState>,
    /// Whether any session was ever built — gates the runlevel ledger
    /// (C: `sessions != NULL`, init.c:1438-1441).
    pub sessions_seen: bool,
    /// The chroot the boot performed (C: `did_multiuser_chroot`,
    /// doc 12).
    pub did_multiuser_chroot: bool,
    /// The chroot root (C: `rootdir`, doc 12).
    pub rootdir: String,
    /// Root's password verifier, built once at startup from the passwd
    /// hash (C: the crypt comparison; ARCH A-12). `None` = no usable
    /// hash, the gate does not run.
    pub root_verify: Option<&'static dyn Fn(&str) -> bool>,
}

impl DriverState {
    /// Parse `/etc/ttys` through the host seam. An unreadable file is
    /// an empty table — C's getttyent yields nothing and every session
    /// would retire.
    pub fn ttys_lines(&self, host: &dyn InitHost) -> Vec<TtysLine> {
        match host.read_file(TTYS_PATH) {
            Ok(text) => text.lines().filter_map(parse_ttys_line).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Build the child reaper over the table and DB.
    pub fn collector(&mut self) -> ChildCollector<'_> {
        ChildCollector {
            sessions: &mut self.sessions,
            db: &mut self.db,
            did_multiuser_chroot: self.did_multiuser_chroot,
            rootdir: self.rootdir.clone(),
        }
    }

    /// The runlevel ledger writer over this state.
    pub fn ledger(&mut self) -> Ledger<'_> {
        Ledger::new(&mut self.sessions_seen)
    }
}

/// Reaps children against the table and DB — C's
/// `collect_child(wpid, status)` with its global reads bundled. Each
/// collect also flushes the DEAD ledger entries the reap produced,
/// synchronously through the host (C: `clear_session_logs` runs at
/// reap time, init.c:1466).
pub struct ChildCollector<'a> {
    pub sessions: &'a mut Vec<Session>,
    pub db: &'a mut crate::session_db::HashMapDb,
    pub did_multiuser_chroot: bool,
    pub rootdir: String,
}

impl ChildCollector<'_> {
    pub fn collect(
        &mut self,
        host: &mut dyn InitHost,
        pid: Pid,
        status: &WaitStatus,
    ) -> crate::multi_user::CollectAction {
        let mut dead: Vec<(String, i32)> = Vec::new();
        let action = crate::multi_user::collect_child(
            host,
            self.sessions,
            &mut *self.db,
            &mut |device: &str, _pid: Pid, s: &WaitStatus| {
                let code = match s {
                    WaitStatus::Exited { code } => *code,
                    WaitStatus::Signaled { termsig, .. } => *termsig,
                    WaitStatus::Stopped { .. } => 0,
                };
                dead.push((device.to_string(), code));
            },
            self.did_multiuser_chroot,
            &self.rootdir,
            pid,
            status,
        );
        let now = host.now_secs().unwrap_or(0);
        for (device, code) in &dead {
            let dead = crate::utmp::dead_record(device, pid, *code, now);
            let _ = crate::utmp::append_record(host, crate::utmp::UTMPX_PATH, &dead);
            let _ = crate::utmp::append_record(host, crate::utmp::WTMPX_PATH, &dead);
        }
        action
    }
}

/// The runlevel/reboot/shutdown ledger writer (C: the
/// `utmpx_set_runlevel`/`logwtmpx` call sites). Holds only the
/// sessions-seen gate; the host travels as an argument so the ledger
/// never aliases the driver's own `&mut`.
pub struct Ledger<'a> {
    pub(crate) sessions_seen: &'a mut bool,
}

impl<'a> Ledger<'a> {
    pub(crate) fn new(sessions_seen: &'a mut bool) -> Ledger<'a> {
        Ledger { sessions_seen }
    }

    /// C: `utmpx_set_runlevel(current, next)` at each transition
    /// (init.c:631-634), gated on the session table existing.
    pub fn runlevel(
        &self,
        host: &mut dyn InitHost,
        old: Option<StateKind>,
        new: StateKind,
    ) {
        if !*self.sessions_seen {
            return;
        }
        let now = host.now_secs().unwrap_or(0);
        let _ = utmpx_set_runlevel(
            host,
            runlevel_for_optional(old),
            runlevel_for(new),
            true,
            now,
        );
    }

    /// C: `logwtmpx("~", "reboot", ...)` after a clean boot
    /// (init.c:1008).
    pub fn reboot(&self, host: &mut dyn InitHost) {
        let now = host.now_secs().unwrap_or(0);
        let _ = logwtmpx(host, "~", "reboot", RecordType::Init, now);
    }

    /// C: `logwtmpx("~", "shutdown", ...)` when death begins
    /// (init.c:1674).
    pub fn shutdown(&self, host: &mut dyn InitHost) {
        let now = host.now_secs().unwrap_or(0);
        let _ = logwtmpx(host, "~", "shutdown", RecordType::Init, now);
    }
}

/// Drive the machine from `first` forever (C: `transition`,
/// init.c:624-640).
///
/// Every loop turn writes the run-level ledger entry for the
/// transition and then executes the state. The only exit is the
/// SIGKILL quiet-wait — C's `sigfillset`/`sigsuspend` end, which parks
/// in a quiet reap and never advances.
pub fn run_transition(host: &mut dyn InitHost, state: &mut DriverState, first: StateKind) -> ! {
    let mut current: Option<StateKind> = None;
    let mut next = first;

    loop {
        let ledger = state.ledger();
        ledger.runlevel(host, current, next);
        current = Some(next);

        next = step(host, state, next);
    }
}

/// One state execution — the C switch table.
pub(crate) fn step(host: &mut dyn InitHost, state: &mut DriverState, current: StateKind) -> StateKind {
    match current {
        StateKind::Death => {
            let DriverState {
                sessions,
                db,
                signals,
                did_multiuser_chroot,
                rootdir,
                ..
            } = state;
            let mut collector = ChildCollector {
                sessions,
                db,
                did_multiuser_chroot: *did_multiuser_chroot,
                rootdir: rootdir.clone(),
            };
            let clang = signals.clang.clone();
            let mut ledger = Ledger::new(&mut state.sessions_seen);
            let mut deps = ShutdownDeps {
                collector: &mut collector,
                ledger: &mut ledger,
                clang,
            };
            death(host, &mut deps)
        }
        StateKind::SingleUser => {
            let DriverState {
                console_secure,
                from_securitylevel,
                root_verify,
                sessions,
                db,
                signals,
                did_multiuser_chroot,
                rootdir,
                ..
            } = state;
            let mut collector = ChildCollector {
                sessions,
                db,
                did_multiuser_chroot: *did_multiuser_chroot,
                rootdir: rootdir.clone(),
            };
            let deps = SingleUserDeps {
                verify_password: *root_verify,
                console_secure: *console_secure,
                from_securitylevel: *from_securitylevel,
                collector: &mut collector,
                requested: signals,
            };
            match single_user(host, deps) {
                SingleUserOutcome::Restart => StateKind::SingleUser,
                SingleUserOutcome::ProceedRuncomFastboot => {
                    state.mode = RuncomMode::Fastboot;
                    StateKind::Runcom
                }
                SingleUserOutcome::AwaitReboot => quiet_wait(host),
                SingleUserOutcome::Transition(t) => t,
            }
        }
        StateKind::Runcom => {
            let DriverState {
                mode,
                rootdir,
                sessions,
                db,
                signals,
                did_multiuser_chroot,
                ..
            } = state;
            let mut collector = ChildCollector {
                sessions,
                db,
                did_multiuser_chroot: *did_multiuser_chroot,
                rootdir: rootdir.clone(),
            };
            let mut ledger = Ledger::new(&mut state.sessions_seen);
            let deps = crate::runcom::RuncomDeps {
                mode: *mode,
                rootdir,
                signals,
            };
            match crate::runcom::runcom(host, &mut collector, &mut ledger, &deps) {
                crate::runcom::RuncomResult::SingleUser => StateKind::SingleUser,
                crate::runcom::RuncomResult::AwaitReboot => quiet_wait(host),
                crate::runcom::RuncomResult::Booted { did_multiuser_chroot } => {
                    state.did_multiuser_chroot = did_multiuser_chroot;
                    state.mode = RuncomMode::Autoboot; // C: init.c:1004
                    read_ttys_step(host, state)
                }
            }
        }
        StateKind::ReadTtys => read_ttys_step(host, state),
        StateKind::MultiUser => {
            let DriverState {
                sessions,
                db,
                signals,
                did_multiuser_chroot,
                rootdir,
                ..
            } = state;
            let mut collector = ChildCollector {
                sessions,
                db,
                did_multiuser_chroot: *did_multiuser_chroot,
                rootdir: rootdir.clone(),
            };
            multi_user(host, &mut collector, signals)
        }
        StateKind::CleanTtys => {
            let lines = state.ttys_lines(host);
            crate::clean_ttys::clean_ttys(host, &mut state.sessions, &mut state.db, &lines)
        }
        StateKind::Catatonia => crate::shutdown::catatonia(&mut state.sessions),
    }
}

/// C: `read_ttys` (init.c:1222-1284) as a driver step: parse the file,
/// build sessions, and route on the result.
fn read_ttys_step(host: &mut dyn InitHost, state: &mut DriverState) -> StateKind {
    let lines = state.ttys_lines(host);
    state.sessions.clear();
    let mut built = 0;
    for line in &lines {
        if let Ok(sp) = crate::session::build_session(
            built + 1,
            &line.name,
            &line.getty,
            line.window.as_deref(),
            line.status.on,
        ) {
            state.sessions.push(sp);
            built += 1;
        }
    }
    if built > 0 {
        state.sessions_seen = true;
    }
    match crate::ttys::plan_read_ttys(&lines, true, state.did_multiuser_chroot) {
        crate::ttys::ReadTtysNext::MultiUser { .. } => StateKind::MultiUser,
        crate::ttys::ReadTtysNext::SingleUser => StateKind::SingleUser,
        crate::ttys::ReadTtysNext::Death => StateKind::Death,
    }
}

/// The SIGKILL end state: C `sigfillset` + `for(;;) sigsuspend` — a
/// quiet reap that never advances (init.c:850-856).
fn quiet_wait(host: &mut dyn InitHost) -> ! {
    loop {
        let _ = host.waitpid(-1, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;
    use crate::session_db::SessionDb;
    use minix_sys::Errno;

    fn fresh_state() -> DriverState {
        DriverState {
            boot_args: crate::entry::BootArgs::default(),
            mode: RuncomMode::Autoboot,
            console_ok: true,
            console_secure: false,
            from_securitylevel: 0,
            sessions: Vec::new(),
            db: crate::session_db::HashMapDb::default(),
            signals: Arc::new(SignalState::default()),
            sessions_seen: false,
            did_multiuser_chroot: false,
            rootdir: "/".to_string(),
            root_verify: None,
        }
    }

    #[test]
    fn test_ttys_lines_parse_through_host() {
        let mut host = ScriptHost::default();
        host.files.push((
            TTYS_PATH.into(),
            "tty1 /sbin/getty vt100 on\n# comment\ntty2 /sbin/getty vt100 off\n".into(),
        ));
        let state = fresh_state();
        let lines = state.ttys_lines(&host);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].status.on);
        assert!(!lines[1].status.on);

        // Unreadable file: empty table.
        let bare = ScriptHost::default();
        assert!(fresh_state().ttys_lines(&bare).is_empty());
    }

    #[test]
    fn test_read_ttys_step_builds_sessions_and_marks_seen() {
        let mut host = ScriptHost::default();
        host.files
            .push((TTYS_PATH.into(), "tty1 /sbin/getty vt100 on\n".into()));
        let mut state = fresh_state();
        assert!(!state.sessions_seen);
        assert_eq!(read_ttys_step(&mut host, &mut state), StateKind::MultiUser);
        assert_eq!(state.sessions.len(), 1);
        assert!(state.sessions_seen);
    }

    #[test]
    fn test_ledger_gates_on_sessions_seen() {
        let mut host = ScriptHost::default();
        host.now = 77;
        let mut state = fresh_state();
        let ledger = state.ledger();
        ledger.runlevel(&mut host, None, StateKind::MultiUser);
        assert!(host.appends.is_empty(), "no sessions yet → no write");

        state.sessions_seen = true;
        let ledger = state.ledger();
        ledger.runlevel(&mut host, Some(StateKind::SingleUser), StateKind::MultiUser);
        assert_eq!(host.appends.len(), 1);
        assert_eq!(host.appends[0].0, crate::utmp::UTMPX_PATH);
        assert!(host.appends[0].1.contains("run-level m"));
        assert!(host.appends[0].1.contains("|115|109")); // 's'→'m'
    }

    #[test]
    fn test_boot_chain_walks_every_boundary_to_multi_user() {
        // The 's'→'r'→'t'→'m' spine over the real state functions: rc
        // exits clean (runcom), the ttys table builds one session
        // (read_ttys), multi_user spawns its getty.
        let mut host = ScriptHost::default();
        host.files.push((
            TTYS_PATH.into(),
            "tty1 /sbin/getty vt100 on\n".into(),
        ));
        host.fork_outcomes.push(Ok(30)); // rc child
        host.wait_outcomes.push(Ok((30, WaitStatus::Exited { code: 0 })));
        host.fork_outcomes.push(Ok(31)); // getty child
        host.wait_errors.push(Errno::ESRCH); // ends the reap loop
        host.now = 9;
        let mut state = fresh_state();
        // Boot 成功时 runcom 直接接过 read_ttys 一步（C 分两轮，Rust
        // 驱动合并为一个分派边界），交付 MultiUser 与已建会话表。
        assert_eq!(step(&mut host, &mut state, StateKind::Runcom), StateKind::MultiUser);
        assert!(state.sessions_seen);
        // 多用户首轮为唯一会话拉起 getty（pid 31），随后脚本以 ESRCH
        // 收割循环收尾——无请求时 C 的 unwrap 语义即 clean_ttys。
        assert_eq!(step(&mut host, &mut state, StateKind::MultiUser), StateKind::CleanTtys);
        assert_eq!(state.sessions[0].process, Some(31));
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].process, Some(31));
    }

    #[test]
    fn test_boot_chain_rc_failure_falls_back_to_single_user() {
        let mut host = ScriptHost::default();
        host.files.push((TTYS_PATH.into(), String::new()));
        host.fork_outcomes.push(Ok(30));
        host.wait_outcomes.push(Ok((30, WaitStatus::Exited { code: 1 })));
        let mut state = fresh_state();
        assert_eq!(step(&mut host, &mut state, StateKind::Runcom), StateKind::SingleUser);
    }

    #[test]
    fn test_boot_chain_single_user_then_runcom_fastboot() {
        let mut host = ScriptHost::default();
        host.files.push((TTYS_PATH.into(), String::new()));
        // shell (pid 7) exits normally — FASTBOOT to runcom; rc then
        // succeeds — read_ttys.
        host.fork_outcomes.push(Ok(7));
        host.wait_outcomes.push(Ok((7, WaitStatus::Exited { code: 0 })));
        host.fork_outcomes.push(Ok(8));
        host.wait_outcomes.push(Ok((8, WaitStatus::Exited { code: 0 })));
        let mut state = fresh_state();
        state.mode = RuncomMode::Fastboot;
        assert_eq!(step(&mut host, &mut state, StateKind::SingleUser), StateKind::Runcom);
        assert_eq!(state.mode, RuncomMode::Fastboot);
        // rc 干净退出：驱动把 read_ttys 合并进同一边界。
        assert_eq!(step(&mut host, &mut state, StateKind::Runcom), StateKind::MultiUser);
    }

    #[test]
    fn test_boot_chain_clean_ttys_and_death_and_catatonia_boundaries() {
        // 'T' on an empty re-read retires everything but still hands
        // back multi-user; catatonia marks and returns multi-user;
        // death with nobody alive lands single-user.
        let mut host = ScriptHost::default();
        let mut state = fresh_state();
        state.sessions.push(
            crate::session::build_session(1, "tty1", "/sbin/getty", None, true).unwrap(),
        );
        assert_eq!(step(&mut host, &mut state, StateKind::CleanTtys), StateKind::MultiUser);
        assert_eq!(step(&mut host, &mut state, StateKind::Catatonia), StateKind::MultiUser);
        assert!(state.sessions[0].flags.contains(crate::session::SE_SHUTDOWN));
        // death: kill(-1, SIGHUP) hits ESRCH — straight to single-user.
        host.kill_errors.push(Errno::ESRCH);
        assert_eq!(step(&mut host, &mut state, StateKind::Death), StateKind::SingleUser);
    }

    #[test]
    fn test_collector_flushes_dead_ledger_through_host() {
        let mut host = ScriptHost::default();
        host.now = 500;
        let mut sessions = vec![
            crate::session::build_session(1, "tty1", "/sbin/getty", None, true).unwrap(),
        ];
        let mut db = crate::session_db::HashMapDb::default();
        db.open().unwrap();
        db.insert(4, 1);
        sessions[0].process = Some(4);
        host.fork_outcomes.push(Ok(9)); // the respawned getty
        let mut collector = ChildCollector {
            sessions: &mut sessions,
            db: &mut db,
            did_multiuser_chroot: false,
            rootdir: "/".into(),
        };
        collector.collect(&mut host, 4, &WaitStatus::Exited { code: 3 });
        // The DEAD entries went out through the seam synchronously.
        assert_eq!(host.appends.len(), 2);
        assert_eq!(host.appends[0].0, crate::utmp::UTMPX_PATH);
        assert!(host.appends[0].1.contains("Dead"));
    }
}
