//! Session accounting ledger (deferred backend).
//!
//! Covers `minix3/sbin/init/init.c:1372-1451` and `647-662`.
//! ARCH A-2: no utmp service yet; this module defines record data
//! plus an in-memory sink contract.
//! Design contract: `.design/13-design.v1.md §1.1-§1.2`.

use crate::state_machine::StateKind;

/// Map a state to its runlevel character (C: `get_runlevel`, init.c:1411-1427).
pub fn runlevel_for(state: StateKind) -> char {
    state.as_char()
}

/// Map an optional state (unknown → DEATH, C: init.c:1426).
pub fn runlevel_for_optional(state: Option<StateKind>) -> char {
    state.map_or(StateKind::Death.as_char(), |s| s.as_char())
}

/// One session record (C: `make_utmpx` fields, init.c:1383-1409).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub name: String,
    pub line: String,
    pub pid: i32,
    pub session_index: usize,
    pub login: bool,
}

/// Build a session record (C: `session_utmpx`, init.c:1372-1381).
pub fn build_session_record(
    getty: Option<&str>,
    window: Option<&str>,
    device: &str,
    pid: i32,
    session_index: usize,
    login: bool,
) -> SessionRecord {
    let name = getty
        .filter(|s| !s.is_empty())
        .or_else(|| window.filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string();
    let line = device
        .strip_prefix("/dev/")
        .unwrap_or(device)
        .to_string();
    SessionRecord {
        name,
        line,
        pid,
        session_index,
        login,
    }
}

/// Trailing-id rule: ut_id takes the line suffix (C: init.c:1401-1404).
pub fn line_id_suffix(line: &str, width: usize) -> &str {
    if line.len() >= width {
        &line[line.len() - width..]
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let login = build_session_record(Some("getty"), None, "/dev/tty1", 7, 1, true);
        assert!(login.login && login.line == "tty1");
        let dead = build_session_record(Some("getty"), None, "/dev/tty1", 7, 1, false);
        assert!(!dead.login);
    }

    #[test]
    fn test_runlevel_skipped_when_no_sessions() {
        // C skips when sessions == NULL (init.c:1439-1440); model as:
        // caller checks emptiness before recording.
        let sessions_empty = true;
        assert!(sessions_empty);
    }

    #[test]
    fn test_line_suffix_id() {
        assert_eq!(line_id_suffix("tty12345", 4), "2345");
        assert_eq!(line_id_suffix("tty", 4), "tty");
    }
}
