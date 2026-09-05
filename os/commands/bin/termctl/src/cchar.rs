//! Control character names, caret notation, and `undef`.
//!
//! Ground truth: `minix3/bin/stty/cchar.c` (primary names in `cchars1` with
//! old aliases in `cchars2`: `brk`, `flush`, `rprnt`), slot indices from
//! `minix3/sys/sys/termios.h` lines 50 to 79 (`VEOF` 0 through `VSTATUS`
//! 18), and default bytes from `minix3/sys/sys/ttydefaults.h` (`CTRL`
//! macro plus per character defaults). Two Minix notes worth remembering:
//! erase defaults to Control-H (`^H`, not DEL), and end of line defaults
//! to disabled.
//!
//! Users write values three ways: a literal byte, caret notation (`^C`
//! means Control-C, `^?` means DEL), or `undef` (disable the function).

/// (Name, slot index, default byte) triples in `cchar.c` order: 18 primary
/// names plus 3 old aliases (`brk`, `flush`, `rprnt`, behaving exactly like
/// their primaries).
pub const CONTROL_CHARS: [(&str, u8, u8); 21] = [
    ("discard", 15, 0x0F),
    ("dsusp", 11, 0x19),
    ("eof", 0, 0x04),
    ("eol", 1, 0xFF),
    ("eol2", 2, 0xFF),
    ("erase", 3, 0x08),
    ("intr", 8, 0x03),
    ("kill", 5, 0x15),
    ("lnext", 14, 0x16),
    ("min", 16, 0x01),
    ("quit", 9, 0x1C),
    ("reprint", 6, 0x12),
    ("start", 12, 0x11),
    ("status", 18, 0x14),
    ("stop", 13, 0x13),
    ("susp", 10, 0x1A),
    ("time", 17, 0x00),
    ("werase", 4, 0x17),
    ("brk", 1, 0xFF),
    ("flush", 15, 0x0F),
    ("rprnt", 6, 0x12),
];

/// Disabled control character marker (`_POSIX_VDISABLE` value).
pub const DISABLED: u8 = 0xFF;

/// Look up a control character by name (aliases included).
pub(crate) fn lookup(name: &str) -> Result<(u8, u8), crate::TermError> {
    for (known, slot, default) in CONTROL_CHARS {
        if known == name {
            return Ok((slot, default));
        }
    }
    Err(crate::TermError::InvalidArgument)
}

/// Parse a control character value: `undef` disables, `^X` is caret
/// notation (`^?` is DEL, `^` followed by anything else is Control plus
/// that letter), anything else must be exactly one byte.
pub(crate) fn parse_value(text: &str) -> Result<u8, crate::TermError> {
    if text == "undef" {
        return Ok(DISABLED);
    }
    let bytes = text.as_bytes();
    if bytes.len() == 2 && bytes[0] == b'^' {
        return Ok(if bytes[1] == b'?' {
            0x7F
        } else {
            bytes[1] & 0x1F
        });
    }
    if bytes.len() == 1 {
        return Ok(bytes[0]);
    }
    Err(crate::TermError::InvalidArgument)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_resolve() {
        assert_eq!(lookup("intr"), Ok((8, 0x03)));
        assert_eq!(lookup("erase"), Ok((3, 0x08)));
        assert_eq!(lookup("eof"), Ok((0, 0x04)));
        assert_eq!(lookup("min"), Ok((16, 0x01)));
        assert_eq!(lookup("bogus"), Err(crate::TermError::InvalidArgument));
    }

    #[test]
    fn test_aliases_match_primaries() {
        assert_eq!(lookup("brk"), lookup("eol"));
        assert_eq!(lookup("flush"), lookup("discard"));
        assert_eq!(lookup("rprnt"), lookup("reprint"));
    }

    #[test]
    fn test_table_has_all_entries() {
        // 18 primary names plus 3 aliases, mirroring cchars1/cchars2.
        assert_eq!(CONTROL_CHARS.len(), 21);
    }

    #[test]
    fn test_caret_notation() {
        assert_eq!(parse_value("^C"), Ok(0x03));
        assert_eq!(parse_value("^?"), Ok(0x7F));
        assert_eq!(parse_value("x"), Ok(b'x'));
    }

    #[test]
    fn test_undef_disables() {
        assert_eq!(parse_value("undef"), Ok(DISABLED));
    }

    #[test]
    fn test_long_values_rejected() {
        assert_eq!(parse_value("ab"), Err(crate::TermError::InvalidArgument));
        assert_eq!(parse_value(""), Err(crate::TermError::InvalidArgument));
    }
}
