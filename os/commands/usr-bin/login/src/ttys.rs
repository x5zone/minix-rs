//! Terminal line table format (`/etc/ttys`).
//!
//! Ground truth: `minix3/etc/ttys` (whitespace separated columns: terminal
//! name, the `getty` command to run, terminal type, on/off status, optional
//! `secure` flag, trailing comment) consumed by the init state machine
//! (`minix3/sbin/init/init.c:read_ttys`, around line 870 onward, which
//! re-reads the file on every clean pass).
//!
//! Each line tells the first process what to do with one terminal line:
//! spawn the named program when the line is on, leave it alone when off.

use crate::LoginError;

/// Whether the line is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    /// The terminal is served: init keeps a program running on it.
    On,
    /// The terminal is idle: init runs nothing on it.
    Off,
}

/// One parsed `ttys` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtysEntry<'a> {
    /// Terminal name, for example `console` or `ttyc1`.
    pub name: &'a str,
    /// Program to run, for example `"/usr/libexec/getty default"`.
    /// Empty means no program even when the line is on.
    pub getty_command: &'a str,
    /// Terminal type handed to the terminal capability lookup.
    pub terminal_type: &'a str,
    /// On/off status.
    pub status: LineStatus,
    /// True when the `secure` flag is present (root may log in here).
    pub secure: bool,
}

/// Parse one `ttys` line.
///
/// Comment lines (`#` first) and blank lines yield `Ok(None)`. A quoted
/// getty command (`"/usr/libexec/getty default"`) keeps its inner spaces as
/// one column. A line needs at least four columns; the fifth (`secure`) is
/// optional.
pub fn parse_ttys_line<'a>(line: &'a str) -> Result<Option<TtysEntry<'a>>, LoginError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let (columns, column_count) = split_columns(trimmed)?;
    if column_count < 4 {
        return Err(LoginError::InvalidArgument);
    }
    let status = match columns[3] {
        "on" => LineStatus::On,
        "off" => LineStatus::Off,
        _ => return Err(LoginError::InvalidArgument),
    };
    let secure = columns[..column_count]
        .iter()
        .skip(4)
        .any(|column| *column == "secure");
    Ok(Some(TtysEntry {
        name: check_name(columns[0])?,
        getty_command: columns[1],
        terminal_type: columns[2],
        status,
        secure,
    }))
}

/// Split a line into whitespace separated columns, honouring one pair of
/// double quotes around the getty command column.
fn split_columns<'a>(line: &'a str) -> Result<([&'a str; 6], usize), LoginError> {
    let mut columns: [&'a str; 6] = [""; 6];
    let mut count = 0;
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        if count >= 6 {
            return Err(LoginError::InvalidArgument);
        }
        if bytes[index] == b'"' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'"' {
                end += 1;
            }
            if end >= bytes.len() {
                return Err(LoginError::InvalidArgument);
            }
            columns[count] = &line[start..end];
            count += 1;
            index = end + 1;
        } else {
            let start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            columns[count] = &line[start..index];
            count += 1;
        }
    }
    if count > 6 {
        return Err(LoginError::InvalidArgument);
    }
    Ok((columns, count))
}

fn check_name(name: &str) -> Result<&str, LoginError> {
    if name.is_empty() {
        return Err(LoginError::InvalidArgument);
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_line_from_etc_ttys() {
        let entry = parse_ttys_line(
            "console\t\"/usr/libexec/getty default\"\tminix\ton secure",
        )
        .unwrap()
        .unwrap();
        assert_eq!(entry.name, "console");
        assert_eq!(entry.getty_command, "/usr/libexec/getty default");
        assert_eq!(entry.terminal_type, "minix");
        assert_eq!(entry.status, LineStatus::On);
        assert!(entry.secure);
    }

    #[test]
    fn test_off_line_without_secure() {
        let entry = parse_ttys_line("tty00\t\"\"\t\tunknown\toff secure")
            .unwrap()
            .unwrap();
        assert_eq!(entry.status, LineStatus::Off);
        assert_eq!(entry.getty_command, "");
        assert!(entry.secure);
    }

    #[test]
    fn test_network_line_parses() {
        let entry = parse_ttys_line("ttyp0\t\"\"\t\tnetwork\toff")
            .unwrap()
            .unwrap();
        assert_eq!(entry.terminal_type, "network");
        assert!(!entry.secure);
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_ttys_line("# name getty type status"), Ok(None));
        assert_eq!(parse_ttys_line(""), Ok(None));
    }

    #[test]
    fn test_too_few_columns_rejected() {
        assert_eq!(
            parse_ttys_line("console minix on"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_bad_status_rejected() {
        assert_eq!(
            parse_ttys_line("console \"\" minix maybe"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_unterminated_quote_rejected() {
        assert_eq!(
            parse_ttys_line("console \"/usr/libexec/getty minix on"),
            Err(LoginError::InvalidArgument)
        );
    }
}
