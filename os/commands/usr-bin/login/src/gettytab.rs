//! Terminal capability table format (`/etc/gettytab`).
//!
//! Ground truth: `minix3/etc/gettytab` (termcap style entries: one or more
//! names joined by `|`, then colon separated capabilities; a capability is a
//! flag (`ce`), a number (`sp#9600`), or a string (`im=...`)) read by the
//! `getty` terminal setup in `minix3/libexec/getty/subr.c`.
//!
//! This module parses one *logical* line (the caller joins backslash
//! continued physical lines first): names plus a bounded list of raw
//! capability texts. Interpreting individual capabilities stays with the
//! terminal layer.

use crate::LoginError;

/// Maximum capabilities kept per entry; more is a malformed line.
pub const MAX_CAPABILITIES: usize = 32;

/// Maximum names kept per entry.
pub const MAX_NAMES: usize = 8;

/// One parsed `gettytab` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GettytabEntry<'a> {
    /// Entry names (`default`, `std.9600`, `9600-baud`, ...).
    pub names: [&'a str; MAX_NAMES],
    /// How many of `names` are used.
    pub name_count: usize,
    /// Raw capability texts (`ce`, `sp#9600`, `im=...`, ...).
    pub capabilities: [&'a str; MAX_CAPABILITIES],
    /// How many of `capabilities` are used.
    pub capability_count: usize,
}

impl<'a> GettytabEntry<'a> {
    /// The names as a slice.
    pub fn name_list(&self) -> &[&'a str] {
        &self.names[..self.name_count]
    }

    /// The capabilities as a slice.
    pub fn capability_list(&self) -> &[&'a str] {
        &self.capabilities[..self.capability_count]
    }

    /// True when the entry carries a capability with this flag or key name.
    ///
    /// A capability matches when it equals `key` (flag), starts with
    /// `key#` (number), or starts with `key=` (string).
    pub fn has(&self, key: &str) -> bool {
        self.capability_list().iter().any(|cap| {
            *cap == key
                || cap.strip_prefix(key).is_some_and(|rest| {
                    rest.starts_with('#') || rest.starts_with('=')
                })
        })
    }
}

/// Parse one logical `gettytab` line.
///
/// Comment lines (`#` first) and blank lines yield `Ok(None)`. The caller
/// joins backslash continued physical lines first (the live
/// `minix3/etc/gettytab` wraps every entry over several lines): this
/// function sees one logical line only. The names part ends at the first
/// colon; every following colon separated word is one capability. Empty
/// capability words (from a trailing colon) are skipped. A line needs at
/// least one name and at least one capability.
pub fn parse_gettytab_line<'a>(line: &'a str) -> Result<Option<GettytabEntry<'a>>, LoginError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let (names_part, caps_part) = trimmed.split_once(':').ok_or(LoginError::InvalidArgument)?;
    let mut entry = GettytabEntry {
        names: [""; MAX_NAMES],
        name_count: 0,
        capabilities: [""; MAX_CAPABILITIES],
        capability_count: 0,
    };
    for name in names_part.split('|') {
        let name = name.trim();
        if name.is_empty() || entry.name_count >= MAX_NAMES {
            return Err(LoginError::InvalidArgument);
        }
        entry.names[entry.name_count] = name;
        entry.name_count += 1;
    }
    if entry.name_count == 0 {
        return Err(LoginError::InvalidArgument);
    }
    for cap in caps_part.split(':') {
        let cap = cap.trim();
        if cap.is_empty() {
            continue;
        }
        if entry.capability_count >= MAX_CAPABILITIES {
            return Err(LoginError::InvalidArgument);
        }
        entry.capabilities[entry.capability_count] = cap;
        entry.capability_count += 1;
    }
    if entry.capability_count == 0 {
        return Err(LoginError::InvalidArgument);
    }
    Ok(Some(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_joined_default_entry() {
        let entry = parse_gettytab_line("default:ce:ck:np:im=hello:")
            .unwrap()
            .unwrap();
        assert_eq!(entry.name_list(), &["default"]);
        assert!(entry.has("ce"));
        assert!(entry.has("ck"));
        assert!(entry.has("np"));
        assert!(entry.has("im"));
        assert!(!entry.has("sp"));
    }

    #[test]
    fn test_speed_entry_names_and_number() {
        let entry = parse_gettytab_line("std.9600|9600-baud:sp#9600:")
            .unwrap()
            .unwrap();
        assert_eq!(entry.name_list(), &["std.9600", "9600-baud"]);
        assert!(entry.has("sp"));
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_gettytab_line("# comment"), Ok(None));
        assert_eq!(parse_gettytab_line(""), Ok(None));
    }

    #[test]
    fn test_missing_colon_rejected() {
        assert_eq!(
            parse_gettytab_line("default"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_missing_capabilities_rejected() {
        assert_eq!(
            parse_gettytab_line("default:"),
            Err(LoginError::InvalidArgument)
        );
    }

    #[test]
    fn test_prefix_without_separator_does_not_match() {
        let entry = parse_gettytab_line("default:celery:").unwrap().unwrap();
        assert!(!entry.has("ce"));
    }
}
