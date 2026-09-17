//! Clean-ttys state ('T'): re-read /etc/ttys.
//!
//! Covers `minix3/sbin/init/init.c:1569-1629` (`clean_ttys`).
//! Design contract: `.design/10-design.v1.md §1.1`.
//!
//! The diff is mark-then-sweep: every session loses SE_PRESENT, every
//! line still in the file re-marks its session (and may retire it via
//! SE_SHUTDOWN when the line went off or lost its getty), lines no
//! longer present retire, new lines become sessions. Nobody is reaped
//! here — the SE_SHUTDOWN sessions die and are collected in the
//! multi-user loop (doc 09).

use crate::host::InitHost;
use crate::log::warning;
use crate::session::{build_session, split_command, ParsedCommand, Session, SE_PRESENT, SE_SHUTDOWN};
use crate::session_db::SessionDb;
use crate::state_machine::{sig, StateKind};
use crate::ttys::TtysLine;
use minix_sys::Pid;

/// What to do with one session line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineAction {
    Keep,
    ShutdownHup,
    CreateNew,
    RetireHup,
}

/// Diff one line (pure).
///
/// `known`: session exists; `in_file`: line still present;
/// `on`: TTY_ON and non-empty getty.
pub fn diff_line(known: bool, in_file: bool, on: bool) -> LineAction {
    if !known {
        return LineAction::CreateNew;
    }
    if !in_file {
        return LineAction::RetireHup;
    }
    if !on {
        return LineAction::ShutdownHup;
    }
    LineAction::Keep
}

/// The 'T' state: reconcile the session table with the new `/etc/ttys`
/// and return to multi-user (C: `clean_ttys`, init.c:1569-1629).
///
/// `lines` is the freshly parsed file — the caller reads it through
/// the host seam; an unreadable file parses as empty, which retires
/// every session, exactly as C behaves when `getttyent` yields
/// nothing.
pub fn clean_ttys(
    host: &mut dyn InitHost,
    sessions: &mut Vec<Session>,
    db: &mut dyn SessionDb,
    lines: &[TtysLine],
) -> StateKind {
    // C: clear PRESENT everywhere first (init.c:1575-1577).
    for sp in sessions.iter_mut() {
        sp.flags.clear(SE_PRESENT);
    }

    for (position, line) in lines.iter().enumerate() {
        let session_index = position + 1;
        let name = line.name.as_str();

        let pos = sessions
            .iter()
            .position(|sp| strip_dev(&sp.device) == name);
        match pos {
            Some(pos) => {
                let sp = &mut sessions[pos];
                sp.flags.set(SE_PRESENT);
                // C: a moved line changes its utmp index — warn and
                // follow (init.c:1590-1596).
                if sp.index != session_index {
                    warning(
                        host,
                        &format!(
                            "port `{}' changed utmp index from {} to {}",
                            sp.device, sp.index, session_index
                        ),
                    );
                    sp.index = session_index;
                }
                if !line.status.on || line.getty.is_empty() {
                    shutdown_session(host, sp);
                    continue;
                }
                sp.flags.clear(SE_SHUTDOWN);
                // C: re-parse the getty line; a parse failure shuts the
                // session down (init.c:1601-1611, `setupargv`).
                match rebuild_command(&line.getty, name) {
                    Some(getty) => {
                        sp.getty = Some(getty);
                        sp.window = line.window.as_ref().and_then(|w| {
                            split_command(w).map(ParsedCommand::path_is_first_word)
                        });
                    }
                    None => {
                        warning(
                            host,
                            &format!("can't parse getty for port `{}'", sp.device),
                        );
                        shutdown_session(host, sp);
                    }
                }
            }
            None => {
                // C: brand-new line → a new session joins the table
                // (init.c:1620, `new_session`); off lines are refused
                // by the builder and dropped.
                if let Ok(sp) = build_session(
                    session_index,
                    name,
                    &line.getty,
                    line.window.as_deref(),
                    line.status.on,
                ) {
                    sessions.push(sp);
                }
            }
        }
    }

    // C: sessions the file no longer mentions retire too
    // (init.c:1622-1628).
    for sp in sessions.iter_mut() {
        if !sp.flags.contains(SE_PRESENT) {
            shutdown_session(host, sp);
        }
    }

    StateKind::MultiUser
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;
    use crate::session::build_session;
    use crate::session_db::HashMapDb;

    fn no_kill(_pid: Pid, _sig: i32) {}

    #[test]
    fn test_known_on_keeps() {
        assert_eq!(diff_line(true, true, true), LineAction::Keep);
    }

    #[test]
    fn test_known_off_shutdowns() {
        assert_eq!(diff_line(true, true, false), LineAction::ShutdownHup);
    }

    #[test]
    fn test_unknown_creates() {
        assert_eq!(diff_line(false, true, true), LineAction::CreateNew);
    }

    #[test]
    fn test_missing_retires() {
        assert_eq!(diff_line(true, false, true), LineAction::RetireHup);
    }

    #[test]
    fn test_entity_index_change_warns_and_updates() {
        // Session for tty2 first in the table; the new file lists tty2
        // second — the utmp index follows the file, with a warning.
        let mut host = ScriptHost::default();
        let mut sessions = vec![build_session(1, "tty2", "/sbin/getty", None, true).unwrap()];
        let lines = vec![
            crate::ttys::parse_ttys_line("tty1 /sbin/getty vt100 on").unwrap(),
            crate::ttys::parse_ttys_line("tty2 /sbin/getty vt100 on").unwrap(),
        ];
        let mut db = HashMapDb::default();
        db.open().unwrap();
        assert_eq!(
            clean_ttys(&mut host, &mut sessions, &mut db, &lines),
            StateKind::MultiUser
        );
        assert!(host.console.iter().any(|(_, m)| m.contains("changed utmp index")));
        assert_eq!(sessions.len(), 2);
        // The pre-existing tty2 session picked up index 2; the new
        // tty1 session (appended) took index 1.
        assert_eq!(sessions[0].index, 2);
        assert_eq!(sessions[1].device, "/dev/tty1");
    }

    #[test]
    fn test_entity_off_line_shuts_down_and_hups() {
        let mut host = ScriptHost::default();
        let mut sessions = vec![build_session(1, "tty1", "/sbin/getty", None, true).unwrap()];
        sessions[0].process = Some(4);
        let lines = vec![
            crate::ttys::parse_ttys_line("tty1 /sbin/getty vt100 off").unwrap(),
        ];
        let mut db = HashMapDb::default();
        db.open().unwrap();
        clean_ttys(&mut host, &mut sessions, &mut db, &lines);
        assert!(sessions[0].flags.contains(SE_SHUTDOWN));
        assert_eq!(host.kills, vec![(4, sig::SIGNAL_HANGUP)]);
    }

    #[test]
    fn test_entity_absent_line_retires() {
        let mut host = ScriptHost::default();
        let mut sessions = vec![build_session(1, "tty1", "/sbin/getty", None, true).unwrap()];
        sessions[0].process = Some(4);
        let lines: Vec<TtysLine> = Vec::new();
        let mut db = HashMapDb::default();
        db.open().unwrap();
        clean_ttys(&mut host, &mut sessions, &mut db, &lines);
        assert!(sessions[0].flags.contains(SE_SHUTDOWN));
        assert_eq!(host.kills, vec![(4, sig::SIGNAL_HANGUP)]);
    }

    #[test]
    fn test_entity_new_line_creates_session() {
        let mut host = ScriptHost::default();
        let mut sessions: Vec<Session> = Vec::new();
        let lines = vec![
            crate::ttys::parse_ttys_line("tty9 /sbin/getty vt100 on secure").unwrap(),
        ];
        let mut db = HashMapDb::default();
        db.open().unwrap();
        clean_ttys(&mut host, &mut sessions, &mut db, &lines);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].device, "/dev/tty9");
        assert_eq!(sessions[0].index, 1);
    }

    #[test]
    fn test_entity_kept_line_clears_shutdown_flag() {
        let mut host = ScriptHost::default();
        let mut sessions = vec![build_session(1, "tty1", "/sbin/getty", None, true).unwrap()];
        sessions[0].flags.set(SE_SHUTDOWN);
        let lines = vec![
            crate::ttys::parse_ttys_line("tty1 /sbin/getty vt100 on").unwrap(),
        ];
        let mut db = HashMapDb::default();
        db.open().unwrap();
        clean_ttys(&mut host, &mut sessions, &mut db, &lines);
        assert!(!sessions[0].flags.contains(SE_SHUTDOWN));
        assert!(host.kills.is_empty());
    }

    #[test]
    fn test_entity_unreadable_file_retires_everything() {
        // C: an unreadable ttys makes getttyent yield nothing, which
        // retires every session (with the SIGHUP).
        let mut host = ScriptHost::default();
        let mut sessions = vec![build_session(1, "tty1", "/sbin/getty", None, true).unwrap()];
        sessions[0].process = Some(4);
        let lines: Vec<TtysLine> = Vec::new();
        let mut db = HashMapDb::default();
        db.open().unwrap();
        clean_ttys(&mut host, &mut sessions, &mut db, &lines);
        assert!(sessions[0].flags.contains(SE_SHUTDOWN));
        assert_eq!(host.kills, vec![(4, sig::SIGNAL_HANGUP)]);
    }
}

/// Mark SE_SHUTDOWN and hang up a live process (C: init.c:1598-1600
/// and friends).
fn shutdown_session(host: &mut dyn InitHost, sp: &mut Session) {
    sp.flags.set(SE_SHUTDOWN);
    if let Some(pid) = sp.process {
        let _ = host.kill(pid, sig::SIGNAL_HANGUP);
    }
}

/// Rebuild a getty command from a ttys word (C: `setupargv`,
/// init.c:1185-1217 — "<getty> <ttyname>" split on blanks).
fn rebuild_command(getty: &str, name: &str) -> Option<ParsedCommand> {
    let combined = format!("{getty} {name}");
    let argv = split_command(&combined)?;
    Some(ParsedCommand::path_is_first_word(argv))
}

/// The device name without the `/dev/` prefix (C: `sp->se_device +
/// devlen`, init.c:1588).
fn strip_dev(device: &str) -> &str {
    device.strip_prefix("/dev/").unwrap_or(device)
}
