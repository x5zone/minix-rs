//! /etc/ttys parsing and session-list rebuild.
//!
//! Covers `minix3/sbin/init/init.c:1222-1285` (`read_ttys`) and
//! `1792-1806` (`do_setttyent`). libc `getttyent` is replaced by a
//! small native parser (ARCH A-6).
//! Design contract: `.design/06-design.v1.md §1.1-§1.3`.

/// One parsed `/etc/ttys` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtysLine {
    pub name: String,
    pub getty: String,
    pub status_on: bool,
    pub secure: bool,
}

/// Parse one `/etc/ttys` line: `name getty type status [window]`.
///
/// Comments (`#...`), blank lines, and lines with fewer than four
/// whitespace-separated fields yield `None` (matching `getttyent`
/// skip semantics). Status tokens are fields[3..] joined and matched
/// case-insensitively: containing `on` means on, containing `secure`
/// means secure (covers `on secure` two-token form).
pub fn parse_ttys_line(line: &str) -> Option<TtysLine> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }
    let status = fields[3..].join(" ").to_ascii_lowercase();
    Some(TtysLine {
        name: fields[0].to_string(),
        getty: fields[1].to_string(),
        status_on: status.contains("on"),
        secure: status.contains("secure"),
    })
}

/// Where `read_ttys` goes next (C: init.c:1262-1284).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadTtysNext {
    MultiUser { sessions: usize },
    SingleUser,
    Death,
}

/// Decide the next state from parsed lines and the DB outcome.
pub fn plan_read_ttys(lines: &[TtysLine], db_ok: bool, did_chroot: bool) -> ReadTtysNext {
    if !db_ok {
        if did_chroot {
            return ReadTtysNext::Death;
        }
        return ReadTtysNext::SingleUser;
    }
    ReadTtysNext::MultiUser {
        sessions: lines.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_normal_line() {
        let line = parse_ttys_line("tty1 /sbin/getty vt100 on").unwrap();
        assert_eq!(line.name, "tty1");
        assert!(line.status_on);
    }

    #[test]
    fn test_parse_secure_flag() {
        let line = parse_ttys_line("console /sbin/getty vt100 on secure").unwrap();
        assert!(line.secure && line.status_on);
    }

    #[test]
    fn test_parse_comment_and_empty_skipped() {
        assert_eq!(parse_ttys_line("# comment"), None);
        assert_eq!(parse_ttys_line("   "), None);
        assert_eq!(parse_ttys_line("tty1 /sbin/getty"), None);
    }

    #[test]
    fn test_parse_off_line_still_parsed() {
        // Filtering of off lines lives in new_session (07); the parser
        // keeps them so the plan can count faithfully.
        let line = parse_ttys_line("tty2 /sbin/getty vt100 off").unwrap();
        assert!(!line.status_on);
    }

    #[test]
    fn test_plan_db_failure_goes_single_user() {
        assert_eq!(
            plan_read_ttys(&[], false, false),
            ReadTtysNext::SingleUser
        );
    }

    #[test]
    fn test_plan_db_failure_chrooted_goes_death() {
        assert_eq!(plan_read_ttys(&[], false, true), ReadTtysNext::Death);
    }

    #[test]
    fn test_plan_counts_sessions() {
        let lines = vec![
            parse_ttys_line("tty1 /sbin/getty vt100 on").unwrap(),
            parse_ttys_line("tty2 /sbin/getty vt100 on").unwrap(),
        ];
        assert_eq!(
            plan_read_ttys(&lines, true, false),
            ReadTtysNext::MultiUser { sessions: 2 }
        );
    }
}
