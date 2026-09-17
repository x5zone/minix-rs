//! /etc/ttys parsing and session-list rebuild.
//!
//! Covers `minix3/sbin/init/init.c:1222-1285` (`read_ttys`) and
//! `1792-1806` (`do_setttyent`). libc `getttyent` is replaced by a
//! native parser (ARCH A-6) that mirrors the field semantics of
//! `minix3/lib/libc/gen/getttyent.c`: quoted fields, exact status
//! tokens, and `window=` value options.
//! Design contract: `.design/06-design.v1.md §1.1-§1.3`.

/// Terminal status flags (C: `ty_status`, `minix3/include/ttyent.h:57-58`).
///
/// init only tests TTY_ON and TTY_SECURE. The libc parser knows more
/// tokens (`local`, `rtscts`, `dtrcts`, ...); they are parsed and
/// ignored here, exactly as `getttyent` stores bits that init never
/// reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TtyStatus {
    /// TTY_ON: the line allows login.
    pub on: bool,
    /// TTY_SECURE: root login allowed on this line.
    pub secure: bool,
}

/// One parsed `/etc/ttys` line (C: `struct ttyent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtysLine {
    pub name: String,
    /// Getty command for the line; empty when absent or quoted-empty
    /// (C: `ty_getty == NULL` when the field is empty). A quoted field
    /// keeps its inner spaces, so `"/usr/libexec/getty default"` is one
    /// argument, matching `minix3/etc/ttys`.
    pub getty: String,
    pub status: TtyStatus,
    /// Raw `window=` option value (C: `ty_window` stores the raw
    /// string; `construct_argv` splits it later, see module 07).
    pub window: Option<String>,
}

/// Cut the line at the first `#` outside quotes (the rest is a
/// comment), mirroring `getttyent.c` where an unquoted `#` becomes the
/// terminator (`zapchar == '#'`) and everything after it is the
/// comment. Quotes inside a field toggle quoting and never open a
/// comment.
fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quoted = false;
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'"' => quoted = !quoted,
            b'#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Read one blank-separated field starting at `start`.
///
/// Mirrors `getttyent.c` `skip()` (getttyent.c:178-206): a `"` anywhere
/// toggles in-quote mode and is removed from the field, so quoted
/// spaces stay inside one field and quoted-empty (`""`) yields an
/// empty field; inside quotes `\"` copies a literal quote; a blank
/// outside quotes ends the field. Returns the field text (quotes
/// stripped) and the position just past the delimiter, or `None` at
/// end of line.
fn next_field(line: &str, start: usize) -> Option<(String, usize)> {
    let bytes = line.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let mut out = String::new();
    let mut seg = i;
    let mut quoted = false;
    loop {
        if i >= bytes.len() {
            out.push_str(&line[seg..]);
            return Some((out, i));
        }
        let c = bytes[i];
        if c == b'"' {
            quoted = !quoted;
            out.push_str(&line[seg..i]);
            i += 1;
            seg = i;
        } else if quoted && c == b'\\' && bytes.get(i + 1) == Some(&b'"') {
            out.push_str(&line[seg..i]);
            out.push('"');
            i += 2;
            seg = i;
        } else if !quoted && c.is_ascii_whitespace() {
            out.push_str(&line[seg..i]);
            return Some((out, i + 1));
        } else {
            i += 1;
        }
    }
}

/// Parse one `/etc/ttys` line.
///
/// Blank lines and comment lines yield `None` (C: `fparseln` skip in
/// getttyent.c:97-101). A line needs only a name: missing fields
/// become empty/absent and the status defaults to off, which is how
/// libc yields a statusless entry — the session builder (module 07)
/// rejects it exactly as `new_session` rejects an off line.
///
/// Status tokens match exactly, in order (C: `scmp`, getttyent.c:124):
/// `on` sets TTY_ON, `off` clears it, `secure` sets TTY_SECURE.
/// Substrings do not count, so `ondemand` is not `on` and `insecure`
/// is not `secure`. `window=value` captures the rest of the option as
/// the raw window command, with surrounding quotes stripped by the
/// field scanner like any quoted text. Unknown tokens are ignored.
pub fn parse_ttys_line(line: &str) -> Option<TtysLine> {
    let content = strip_inline_comment(line);
    if content.trim().is_empty() {
        return None;
    }
    let (name, mut pos) = next_field(content, 0)?;
    let (getty, next) = match next_field(content, pos) {
        Some(field) => field,
        None => (String::new(), pos),
    };
    pos = next;
    let mut status = TtyStatus::default();
    let mut window = None;
    while let Some((token, next)) = next_field(content, pos) {
        pos = next;
        match token.as_str() {
            "on" => status.on = true,
            "off" => status.on = false,
            "secure" => status.secure = true,
            other => {
                if let Some(value) = other.strip_prefix("window=") {
                    window = Some(value.to_string());
                }
            }
        }
    }
    Some(TtysLine {
        name,
        getty,
        status,
        window,
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
///
/// The session count covers only lines a session would actually be
/// built for — TTY_ON set and both name and getty present — mirroring
/// the triple condition in `new_session` (init.c:1147-1149) that
/// `read_ttys` applies per line (init.c:1279-1282). Off and getty-less
/// lines parse but never become sessions.
pub fn plan_read_ttys(lines: &[TtysLine], db_ok: bool, did_chroot: bool) -> ReadTtysNext {
    if !db_ok {
        if did_chroot {
            return ReadTtysNext::Death;
        }
        return ReadTtysNext::SingleUser;
    }
    let sessions = lines
        .iter()
        .filter(|line| line.status.on && !line.name.is_empty() && !line.getty.is_empty())
        .count();
    ReadTtysNext::MultiUser { sessions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_normal_line() {
        let line = parse_ttys_line("tty1 /sbin/getty vt100 on").unwrap();
        assert_eq!(line.name, "tty1");
        assert_eq!(line.getty, "/sbin/getty");
        assert!(line.status.on);
        assert!(!line.status.secure);
    }

    #[test]
    fn test_parse_secure_flag() {
        let line = parse_ttys_line("console /sbin/getty vt100 on secure").unwrap();
        assert!(line.status.secure && line.status.on);
    }

    #[test]
    fn test_parse_comment_and_empty_skipped() {
        assert_eq!(parse_ttys_line("# comment"), None);
        assert_eq!(parse_ttys_line("   "), None);
    }

    #[test]
    fn test_statusless_line_parses_as_off() {
        // C: a line with no status tokens yields status 0 (off), and
        // the session builder rejects it; the parser keeps it.
        let line = parse_ttys_line("tty2 /sbin/getty").unwrap();
        assert_eq!(line.getty, "/sbin/getty");
        assert!(!line.status.on);
    }

    #[test]
    fn test_parse_off_line_still_parsed() {
        let line = parse_ttys_line("tty2 /sbin/getty vt100 off").unwrap();
        assert!(!line.status.on);
    }

    #[test]
    fn test_quoted_getty_keeps_inner_space() {
        // Real sample from minix3/etc/ttys line 5: the getty field is
        // quoted and contains a space; it must stay one field.
        let line =
            parse_ttys_line("console \"/usr/libexec/getty default\"\tminix\ton secure").unwrap();
        assert_eq!(line.name, "console");
        assert_eq!(line.getty, "/usr/libexec/getty default");
        assert!(line.status.on && line.status.secure);
    }

    #[test]
    fn test_quoted_empty_getty_is_off_sample() {
        // Real sample from minix3/etc/ttys line 9.
        let line = parse_ttys_line("tty00 \"\"\t\tunknown\toff secure").unwrap();
        assert_eq!(line.getty, "");
        assert!(!line.status.on);
        assert!(line.status.secure);
    }

    #[test]
    fn test_substring_tokens_do_not_match() {
        let line = parse_ttys_line("tty1 /sbin/getty vt100 ondemand insecure").unwrap();
        assert!(!line.status.on);
        assert!(!line.status.secure);
    }

    #[test]
    fn test_off_after_on_clears_in_order() {
        // C: the token loop applies "off" as an explicit clear, so the
        // last relevant token in scan order wins.
        let on_then_off = parse_ttys_line("tty1 /sbin/getty vt100 on off").unwrap();
        assert!(!on_then_off.status.on);
        let off_then_on = parse_ttys_line("tty1 /sbin/getty vt100 off on").unwrap();
        assert!(off_then_on.status.on);
    }

    #[test]
    fn test_window_option_captured() {
        let line = parse_ttys_line(
            "tty1 /sbin/getty vt100 on window=\"/usr/X11R6/bin/xinit\"",
        )
        .unwrap();
        assert_eq!(line.window.as_deref(), Some("/usr/X11R6/bin/xinit"));
    }

    #[test]
    fn test_escaped_quote_inside_quoted_field() {
        let line = parse_ttys_line("tty1 \"/sbin/getty a\\\"b\" vt100 on").unwrap();
        assert_eq!(line.getty, "/sbin/getty a\"b");
    }

    #[test]
    fn test_trailing_comment_ignored() {
        let line = parse_ttys_line("tty1 /sbin/getty vt100 on secure # root console").unwrap();
        assert!(line.status.on && line.status.secure);
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

    #[test]
    fn test_plan_counts_exclude_off_and_gettyless_lines() {
        // C: new_session rejects off lines and empty gettys
        // (init.c:1147-1149), so the session count covers neither.
        let lines = vec![
            parse_ttys_line("tty1 /sbin/getty vt100 on").unwrap(),
            parse_ttys_line("tty2 /sbin/getty vt100 off").unwrap(),
            parse_ttys_line("tty3 \"\" vt100 on").unwrap(),
        ];
        assert_eq!(
            plan_read_ttys(&lines, true, false),
            ReadTtysNext::MultiUser { sessions: 1 }
        );
    }
}
