//! Terminal capability database (`termcap`) parsing and lookup.
//!
//! Ground truth: `minix3/etc/termcap*` (termcap format: names joined by
//! `|`, then colon separated capabilities — flags like `am`, numbers like
//! `co#80`, strings like `cl=\E[H\E[J`), queried by `term`/`tget`.
//! The format is the sibling of `gettytab` (see the login crate); the
//! parser below accepts the same shape with terminal oriented bounds.
//!
//! The capability decision behind `[ARCH] A-2` (port the terminfo data and
//! parser versus wrap escape sequences) is recorded in the stage document;
//! this module is the shared foundation either outcome builds on: names
//! plus flag/number/string lookup over borrowed text.

use crate::TermError;

/// Maximum names per entry.
pub const MAX_NAMES: usize = 8;
/// Maximum capabilities per entry.
pub const MAX_CAPABILITIES: usize = 64;

/// One parsed termcap entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermEntry<'a> {
    /// Entry names (`minix`, `minix console`, ...).
    pub names: [&'a str; MAX_NAMES],
    /// How many of `names` are used.
    pub name_count: usize,
    /// Raw capability texts (`am`, `co#80`, `cl=...`, ...).
    pub capabilities: [&'a str; MAX_CAPABILITIES],
    /// How many of `capabilities` are used.
    pub capability_count: usize,
}

impl<'a> TermEntry<'a> {
    /// The names as a slice.
    pub fn name_list(&self) -> &[&'a str] {
        &self.names[..self.name_count]
    }

    /// The capabilities as a slice.
    pub fn capability_list(&self) -> &[&'a str] {
        &self.capabilities[..self.capability_count]
    }

    /// True when the entry carries the flag `key` exactly.
    pub fn has_flag(&self, key: &str) -> bool {
        self.capability_list().iter().any(|cap| *cap == key)
    }

    /// The number of capability `key` (`co#80` answers 80), or `None`.
    pub fn number(&self, key: &str) -> Option<u32> {
        self.capability_list().iter().find_map(|cap| {
            let rest = cap.strip_prefix(key)?;
            let digits = rest.strip_prefix('#')?;
            parse_number(digits)
        })
    }

    /// The string of capability `key` (`cl=...` answers the text), or
    /// `None`. Backslash escapes stay encoded (decoding them is the
    /// terminal layer's job, which owns the output encoding).
    pub fn string(&self, key: &str) -> Option<&'a str> {
        self.capability_list().iter().find_map(|cap| {
            let rest = cap.strip_prefix(key)?;
            rest.strip_prefix('=')
        })
    }
}

/// Parse one logical termcap line (the caller joins backslash continued
/// physical lines first). Comment lines (`#` first) and blank lines yield
/// `Ok(None)`. A line needs at least one name and one capability.
pub fn parse_termcap_line<'a>(line: &'a str) -> Result<Option<TermEntry<'a>>, TermError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let (names_part, caps_part) = trimmed.split_once(':').ok_or(TermError::InvalidArgument)?;
    let mut entry = TermEntry {
        names: [" "; MAX_NAMES],
        name_count: 0,
        capabilities: [" "; MAX_CAPABILITIES],
        capability_count: 0,
    };
    for name in names_part.split('|') {
        let name = name.trim();
        if name.is_empty() || entry.name_count >= MAX_NAMES {
            return Err(TermError::InvalidArgument);
        }
        entry.names[entry.name_count] = name;
        entry.name_count += 1;
    }
    if entry.name_count == 0 {
        return Err(TermError::InvalidArgument);
    }
    for cap in caps_part.split(':') {
        let cap = cap.trim();
        if cap.is_empty() {
            continue;
        }
        if entry.capability_count >= MAX_CAPABILITIES {
            return Err(TermError::InvalidArgument);
        }
        entry.capabilities[entry.capability_count] = cap;
        entry.capability_count += 1;
    }
    if entry.capability_count == 0 {
        return Err(TermError::InvalidArgument);
    }
    Ok(Some(entry))
}

fn parse_number(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

/// Read only terminal database lookup.
pub trait TermcapSource {
    /// Find the entry naming `name` (primary or alias), or `None`.
    fn lookup(&self, name: &str) -> Option<TermEntry<'_>>;
}

/// A database that knows no terminals: every lookup misses. The honest
/// starting point until the capability data shipment lands (see `[ARCH]`
/// A-2 in the stage document).
pub struct EmptyTermcap;

impl TermcapSource for EmptyTermcap {
    fn lookup(&self, _name: &str) -> Option<TermEntry<'_>> {
        None
    }
}

/// A database over in memory termcap text lines.
///
/// Corrupt lines are skipped, never fatal. The first name match wins.
pub struct SliceTermcap<'a> {
    /// Raw termcap lines searched in order.
    pub lines: &'a [&'a str],
}

impl TermcapSource for SliceTermcap<'_> {
    fn lookup(&self, name: &str) -> Option<TermEntry<'_>> {
        self.lines.iter().find_map(|line| {
            let entry = parse_termcap_line(line).ok()??;
            entry.name_list().contains(&name).then_some(entry)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Joined first capabilities of the real `minix` entry
    // (`etc/termcap.big`: `minix|minix console`, `am`, 80 columns,
    // 25 lines, clear sequence).
    const MINIX: &str = "minix|minix console:am:co#80:li#25:cl=\\E[H\\E[0J:";

    #[test]
    fn test_minix_entry() {
        let entry = parse_termcap_line(MINIX).unwrap().unwrap();
        assert_eq!(entry.name_list(), &["minix", "minix console"]);
        assert!(entry.has_flag("am"));
        assert_eq!(entry.number("co"), Some(80));
        assert_eq!(entry.number("li"), Some(25));
        assert_eq!(entry.string("cl"), Some("\\E[H\\E[0J"));
        assert!(!entry.has_flag("co"));
        assert_eq!(entry.number("xx"), None);
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_termcap_line("# comment"), Ok(None));
        assert_eq!(parse_termcap_line(""), Ok(None));
    }

    #[test]
    fn test_missing_parts_rejected() {
        assert_eq!(
            parse_termcap_line("minix"),
            Err(TermError::InvalidArgument)
        );
        assert_eq!(
            parse_termcap_line("minix:"),
            Err(TermError::InvalidArgument)
        );
    }

    #[test]
    fn test_lookup_by_alias() {
        let db = SliceTermcap { lines: &[MINIX] };
        assert_eq!(db.lookup("minix console").unwrap().number("co"), Some(80));
        assert_eq!(db.lookup("ghost"), None);
    }

    #[test]
    fn test_empty_db_misses() {
        assert_eq!(EmptyTermcap.lookup("minix"), None);
    }
}
