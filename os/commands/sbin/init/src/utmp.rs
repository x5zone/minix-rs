//! Session accounting ledger (ARCH A-2: backend deferred, semantics
//! complete).
//!
//! Covers `minix3/sbin/init/init.c:1372-1451` and `647-662`.
//! Design contract: `.design/13-design.v1.md §1.1-§1.2`.
//!
//! Minix3 keeps two ledger files — the active utmpx and the growing
//! wtmpx history (`minix3/include/utmpx.h:39-40`). The writes behind
//! `pututxline`/`logoutx`/`logwtmpx` are modeled as whole-record
//! appends through the [`InitHost`] seam: the record content is exact,
//! the on-disk encoding is deliberately provisional (see
//! `UtmpxRecord::encode`) until the ABI layout is pinned by a shared
//! consumer — the C layout is not the format anything reads yet
//! (`utmpx.h:83` notes the same for Minix3's own tools).

use alloc::string::ToString;
use alloc::string::String;
use crate::host::InitHost;
use crate::state_machine::StateKind;
use crate::wait::WaitStatus;
/// C: `_PATH_UTMPX` (`minix3/include/utmpx.h:39`).
pub const UTMPX_PATH: &str = "/var/run/utmpx";
/// C: `_PATH_WTMPX` (`minix3/include/utmpx.h:40`).
pub const WTMPX_PATH: &str = "/var/log/wtmpx";
/// C: `RUNLVL_MSG` (`minix3/include/utmpx.h:74`).
pub const RUNLVL_MSG: &str = "run-level %c";

/// The utmpx entry types init writes (C: `utmpx.h:57-64` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    /// INIT_PROCESS: a process init itself started.
    Init,
    /// LOGIN_PROCESS: a getty waiting for a login.
    Login,
    /// DEAD_PROCESS: a process that exited.
    Dead,
    /// RUN_LVL: the run-level marker.
    RunLevel,
}

impl RecordType {
    /// The C numeric value (`utmpx.h:57-64`).
    pub fn as_number(self) -> i32 {
        match self {
            RecordType::Init => 5,      // INIT_PROCESS
            RecordType::Login => 6,     // LOGIN_PROCESS
            RecordType::Dead => 7,      // DEAD_PROCESS
            RecordType::RunLevel => 1,  // RUN_LVL
        }
    }
}

/// One ledger entry (C: `struct utmpx` fields init touches).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtmpxRecord {
    pub record_type: RecordType,
    pub pid: i32,
    /// Terminal line (no `/dev/` prefix), or `~` for init itself.
    pub line: String,
    /// Short id: the trailing characters of the line (init.c:1401-1404).
    pub id: String,
    pub user: String,
    /// C: `ut_exit.e_exit` (the previous run level for RUN_LVL).
    pub exit_old: i32,
    /// C: `ut_exit.e_termination` (the new run level for RUN_LVL).
    pub exit_new: i32,
    pub tv_secs: i64,
}

impl UtmpxRecord {
    /// Provisional serialization.
    ///
    /// C's `pututxline` writes the binary `struct utmpx` ABI; nothing
    /// in minix-rs reads these files yet, so the ledger emits a stable
    /// one-line text form until an ABI consumer exists (ARCH A-2
    /// residual). The record fields above are the contract.
    pub fn encode(&self) -> String {
        format!(
            "{:?}|{}|{}|{}|{}|{}|{}\n",
            self.record_type, self.pid, self.line, self.id, self.user, self.exit_old, self.exit_new
        )
    }
}

/// Build one record (C: `make_utmpx`, init.c:1384-1409).
pub fn make_utmpx(
    name: &str,
    line: &str,
    record_type: RecordType,
    pid: i32,
    tv_secs: i64,
    exit_old: i32,
    exit_new: i32,
) -> UtmpxRecord {
    UtmpxRecord {
        record_type,
        pid,
        line: line.to_string(),
        id: line_id_suffix(line, 4).to_string(),
        user: name.to_string(),
        exit_old,
        exit_new,
        tv_secs,
    }
}

/// Build the record for one session (C: `session_utmpx`,
/// init.c:1372-1381).
pub fn session_utmpx(
    getty: Option<&str>,
    window: Option<&str>,
    device: &str,
    pid: i32,
    tv_secs: i64,
    login: bool,
) -> UtmpxRecord {
    // C prefers the getty name and falls back to the window system.
    let name = getty
        .filter(|s| !s.is_empty())
        .or_else(|| window.filter(|s| !s.is_empty()))
        .unwrap_or("");
    let line = device.strip_prefix("/dev/").unwrap_or(device);
    make_utmpx(
        name,
        line,
        if login {
            RecordType::Login
        } else {
            RecordType::Dead
        },
        pid,
        tv_secs,
        0,
        0,
    )
}

/// Trailing-id rule: ut_id takes the line suffix (C: init.c:1401-1404).
pub fn line_id_suffix(line: &str, width: usize) -> &str {
    if line.len() >= width {
        &line[line.len() - width..]
    } else {
        line
    }
}

/// Map a state to its runlevel character (C: `get_runlevel`,
/// init.c:1411-1427).
pub fn runlevel_for(state: StateKind) -> char {
    state.as_char()
}

/// Map an optional state (unknown → DEATH, C: init.c:1426).
pub fn runlevel_for_optional(state: Option<StateKind>) -> char {
    state.map_or(StateKind::Death.as_char(), |s| s.as_char())
}

/// Write the run-level marker (C: `utmpx_set_runlevel`,
/// init.c:1429-1451).
///
/// The gate is C's: until the first transition to read_ttys there is
/// no read-write `/var`, so with no sessions recorded the write is
/// skipped entirely (init.c:1438-1441, `sessions == NULL`).
pub fn utmpx_set_runlevel(
    host: &mut dyn InitHost,
    old: char,
    new: char,
    sessions_seen: bool,
    tv_secs: i64,
) -> bool {
    if !sessions_seen {
        return false;
    }
    let line = RUNLVL_MSG.replace("%c", &new.to_string());
    let rec = make_utmpx("", &line, RecordType::RunLevel, 0, tv_secs, old as i32, new as i32);
    append_record(host, UTMPX_PATH, &rec)
}

/// Append to the wtmpx history (C: `logwtmpx`, the boot and shutdown
/// ledger points at init.c:1008 and init.c:1674).
pub fn logwtmpx(host: &mut dyn InitHost, line: &str, user: &str, record_type: RecordType, tv_secs: i64) -> bool {
    let rec = make_utmpx(user, line, record_type, 0, tv_secs, 0, 0);
    append_record(host, WTMPX_PATH, &rec)
}

/// Clear a session's ledger on death (C: `clear_session_logs`,
/// init.c:647-662): a DEAD_PROCESS logout into utmpx, and only when
/// that succeeds, the matching wtmpx history entry.
pub fn clear_session_logs(host: &mut dyn InitHost, device: &str, pid: i32, status: &WaitStatus, tv_secs: i64) -> bool {
    let line = device.strip_prefix("/dev/").unwrap_or(device);
    let code = match status {
        WaitStatus::Exited { code } => *code,
        WaitStatus::Signaled { termsig, .. } => *termsig,
        WaitStatus::Stopped { .. } => 0,
    };
    let mut dead = make_utmpx("", line, RecordType::Dead, pid, tv_secs, code, 0);
    dead.user = String::new();
    if !append_record(host, UTMPX_PATH, &dead) {
        return false;
    }
    append_record(host, WTMPX_PATH, &dead)
}

/// A DEAD_PROCESS entry for a dead session line (C: `logoutx`'s
/// record, built here so the driver can flush it synchronously).
pub fn dead_record(device: &str, pid: i32, code: i32, tv_secs: i64) -> UtmpxRecord {
    let line = device.strip_prefix("/dev/").unwrap_or(device);
    let mut rec = make_utmpx("", line, RecordType::Dead, pid, tv_secs, code, 0);
    rec.user = String::new();
    rec
}

/// Serialize and append one record; `false` is the C "ledger write
/// failed" answer callers warn about and otherwise ignore.
pub fn append_record(host: &mut dyn InitHost, path: &str, rec: &UtmpxRecord) -> bool {
    host.append_file(path, rec.encode().as_bytes()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;
    use minix_sys::Errno;

    #[test]
    fn test_runlevel_maps_all_states() {
        assert_eq!(runlevel_for(StateKind::SingleUser), 's');
        assert_eq!(runlevel_for(StateKind::Runcom), 'r');
        assert_eq!(runlevel_for(StateKind::ReadTtys), 't');
        assert_eq!(runlevel_for(StateKind::MultiUser), 'm');
        assert_eq!(runlevel_for(StateKind::CleanTtys), 'T');
        assert_eq!(runlevel_for(StateKind::Catatonia), 'c');
        assert_eq!(runlevel_for(StateKind::Death), 'd');
    }

    #[test]
    fn test_runlevel_unknown_falls_to_death() {
        assert_eq!(runlevel_for_optional(None), 'd');
    }

    #[test]
    fn test_session_record_login_vs_dead() {
        let login = session_utmpx(Some("getty"), None, "/dev/tty1", 7, 100, true);
        assert_eq!(login.record_type, RecordType::Login);
        assert_eq!(login.line, "tty1");
        let dead = session_utmpx(Some("getty"), None, "/dev/tty1", 7, 100, false);
        assert_eq!(dead.record_type, RecordType::Dead);
    }

    #[test]
    fn test_runlevel_write_skipped_without_sessions() {
        // C: sessions == NULL short-circuits (init.c:1438-1441) — no
        // read-write /var yet.
        let mut host = ScriptHost::default();
        assert!(!utmpx_set_runlevel(&mut host, 's', '2', false, 0));
        assert!(host.appends.is_empty());
    }

    #[test]
    fn test_runlevel_write_record_fields() {
        let mut host = ScriptHost::default();
        assert!(utmpx_set_runlevel(&mut host, 's', '2', true, 55));
        assert_eq!(host.appends.len(), 1);
        let (path, text) = &host.appends[0];
        assert_eq!(path, UTMPX_PATH);
        assert!(text.contains("RunLevel"));
        assert!(text.contains("run-level 2"));
        // e_exit carries the old level, e_termination the new one
        // (init.c:1444-1445): 's' = 115, '2' = 50.
        assert!(text.contains("|115|50"));
    }

    #[test]
    fn test_logwtmpx_reboot_goes_to_wtmpx() {
        let mut host = ScriptHost::default();
        assert!(logwtmpx(&mut host, "~", "reboot", RecordType::Init, 42));
        let (path, text) = &host.appends[0];
        assert_eq!(path, WTMPX_PATH);
        assert!(text.contains("~|") && text.contains("reboot"));
    }

    #[test]
    fn test_clear_session_logs_writes_dead_then_history() {
        let mut host = ScriptHost::default();
        assert!(clear_session_logs(
            &mut host,
            "/dev/tty1",
            7,
            &WaitStatus::Exited { code: 0 },
            90,
        ));
        assert_eq!(host.appends.len(), 2);
        assert_eq!(host.appends[0].0, UTMPX_PATH);
        assert_eq!(host.appends[1].0, WTMPX_PATH);
        assert!(host.appends[0].1.contains("Dead"));
    }

    #[test]
    fn test_append_failure_is_a_false_not_a_panic() {
        // C: `if (pututxline(&ut) == NULL) warning(...)` — a failed
        // ledger write is an ordinary value (init.c:1446-1447).
        let mut host = ScriptHost::default();
        host.append_errors.push(Errno::EPERM);
        assert!(!clear_session_logs(
            &mut host,
            "/dev/tty1",
            7,
            &WaitStatus::Exited { code: 0 },
            90,
        ));
        // The failed utmpx append means the history entry is skipped.
        assert!(host.appends.is_empty());
    }

    #[test]
    fn test_line_suffix_id() {
        assert_eq!(line_id_suffix("tty12345", 4), "2345");
        assert_eq!(line_id_suffix("tty", 4), "tty");
    }
}
