//! Substitution command parsing and application for `sed`.
//!
//! Ground truth: `minix3/usr.bin/sed/compile.c` (the `s` command dispatch
//! at line 122, address parsing from line 189, `compile_subst` at line 480)
//! and `minix3/usr.bin/sed/process.c` (substitution execution around lines
//! 403 to 432, including the empty match advance that keeps global
//! substitution from looping forever).
//!
//! A substitution has the shape `s<delim>pattern<delim>replacement<delim>flags`
//! where the delimiter is any character (the classic slash is convention,
//! not law) and flags mix `g` (replace every match, not just the first),
//! `p` (print the line when a replacement happened), and a decimal number
//! (replace only the nth match). Backslash escapes the delimiter inside
//! either half. The replacement replays the whole match for `&` and group
//! `n` for `\n` (a backslash before any other character yields that
//! character).

use crate::RegexError;
use crate::pattern::{Captures, Pattern};

/// Maximum pattern plus replacement bytes handled in one command.
pub const MAX_SUBST_TEXT: usize = 256;

/// Which matches a substitution replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstScope {
    /// Replace the first match only (no flag).
    First,
    /// Replace every match (`g`).
    Global,
    /// Replace only the nth match (1 based).
    Nth(u32),
}

/// One parsed `s` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subst<'a> {
    /// The pattern text between the first two delimiters.
    pub pattern_text: &'a str,
    /// The replacement text between the last two delimiters.
    pub replacement: &'a str,
    /// Which matches to replace.
    pub scope: SubstScope,
    /// The `p` flag: print the line when a replacement happened.
    pub print: bool,
}

/// Parse one `s` command (the leading `s` already consumed).
///
/// The byte after `s` is the delimiter. Returns the substitution plus how
/// many bytes of `text` it consumed, so the caller can continue with
/// chained commands (`s/a/b/;s/c/d/`).
pub fn parse_subst<'a>(text: &'a str) -> Result<(Subst<'a>, usize), RegexError> {
    let bytes = text.as_bytes();
    let delim = *bytes.first().ok_or(RegexError::InvalidPattern)?;
    let mut pattern_text: Option<&'a str> = None;
    let mut replacement: Option<&'a str> = None;
    let mut pos = 1;
    for slot in [&mut pattern_text, &mut replacement].into_iter() {
        let start = pos;
        loop {
            if pos >= bytes.len() {
                return Err(RegexError::InvalidPattern);
            }
            if bytes[pos] == b'\\' {
                pos += 2;
                continue;
            }
            if bytes[pos] == delim {
                *slot = Some(&text[start..pos]);
                pos += 1;
                break;
            }
            pos += 1;
        }
    }
    let (Some(pattern_text), Some(replacement)) = (pattern_text, replacement) else {
        return Err(RegexError::InvalidPattern);
    };
    let mut scope = SubstScope::First;
    let mut print = false;
    let mut consumed = pos;
    while consumed < bytes.len() {
        match bytes[consumed] {
            b'g' => {
                scope = SubstScope::Global;
                consumed += 1;
            }
            b'p' => {
                print = true;
                consumed += 1;
            }
            b'0'..=b'9' => {
                let mut number: u32 = 0;
                while consumed < bytes.len() && bytes[consumed].is_ascii_digit() {
                    number = number
                        .checked_mul(10)
                        .and_then(|v| v.checked_add((bytes[consumed] - b'0') as u32))
                        .ok_or(RegexError::InvalidPattern)?;
                    consumed += 1;
                }
                if number == 0 {
                    return Err(RegexError::InvalidPattern);
                }
                scope = SubstScope::Nth(number);
            }
            b';' | b'\n' | b' ' | b'\t' => break,
            _ => return Err(RegexError::InvalidPattern),
        }
    }
    if pattern_text.len() + replacement.len() > MAX_SUBST_TEXT {
        return Err(RegexError::TooComplex);
    }
    Ok((
        Subst {
            pattern_text,
            replacement,
            scope,
            print,
        },
        consumed,
    ))
}

/// Apply one substitution to `haystack`, writing the result into `out`.
///
/// Returns `(bytes_written, replacement_happened)`. Global substitution
/// advances past empty matches by one character, mirroring
/// `process.c:418-422`, so `s/x*/-/g` terminates. Output truncates (never
/// overflows) when the result exceeds `out`.
pub fn apply(
    pattern: &Pattern,
    subst: &Subst<'_>,
    haystack: &str,
    out: &mut [u8],
) -> (usize, bool) {
    let bytes = haystack.as_bytes();
    let mut written = 0usize;
    let mut emit = |slice: &[u8]| {
        for byte in slice {
            if written >= out.len() {
                break;
            }
            out[written] = *byte;
            written += 1;
        }
    };
    let mut pos = 0;
    let mut ordinal = 0u32;
    let mut replaced = false;
    while pos <= bytes.len() {
        let Some((start, end, caps)) = pattern.find_bytes(bytes, pos) else {
            emit(&bytes[pos..]);
            break;
        };
        ordinal += 1;
        let wanted = match subst.scope {
            SubstScope::First => ordinal == 1,
            SubstScope::Global => true,
            SubstScope::Nth(n) => ordinal == n,
        };
        emit(&bytes[pos..start]);
        if wanted {
            replaced = true;
            splice_replacement(subst.replacement, &caps, bytes, &mut emit);
        } else {
            emit(&bytes[start..end]);
        }
        if end == start {
            // Empty match: copy one character raw so a global walk always
            // makes progress (process.c:418-422).
            if start < bytes.len() {
                let advance = char_len_at(bytes, start);
                emit(&bytes[start..start + advance]);
                pos = start + advance;
            } else {
                pos = start + 1;
            }
        } else {
            pos = end;
        }
        if wanted && !matches!(subst.scope, SubstScope::Global) {
            emit(&bytes[pos..]);
            break;
        }
    }
    (written, replaced)
}

/// Copy `replacement` to `emit`, replaying `&` (whole match) and `\n`
/// (group n) from `caps` against `haystack`. Unset groups expand to
/// nothing; a backslash before a non digit yields that character.
fn splice_replacement(
    replacement: &str,
    caps: &Captures,
    haystack: &[u8],
    emit: &mut dyn FnMut(&[u8]),
) {
    let bytes = replacement.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'&' => {
                if let Some((start, end)) = caps.spans[0] {
                    emit(&haystack[start as usize..end as usize]);
                }
                index += 1;
            }
            b'\\' if index + 1 < bytes.len() => {
                let next = bytes[index + 1];
                if next.is_ascii_digit() {
                    let slot = (next - b'0') as usize;
                    if let Some(Some((start, end))) = caps.spans.get(slot) {
                        emit(&haystack[*start as usize..*end as usize]);
                    }
                    index += 2;
                } else {
                    emit(&bytes[index + 1..index + 2]);
                    index += 2;
                }
            }
            _ => {
                emit(&bytes[index..index + 1]);
                index += 1;
            }
        }
    }
}

/// Length of the character at `pos` (multibyte aware, never zero).
fn char_len_at(bytes: &[u8], pos: usize) -> usize {
    if pos >= bytes.len() {
        return 1;
    }
    let lead = bytes[pos];
    if lead < 0x80 {
        1
    } else if lead >> 5 == 0b110 {
        2.min(bytes.len() - pos)
    } else if lead >> 4 == 0b1110 {
        3.min(bytes.len() - pos)
    } else if lead >> 3 == 0b11110 {
        4.min(bytes.len() - pos)
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{compile_basic, compile_extended};

    fn run(pattern_text: &str, extended: bool, command: &str, line: &str) -> (String, bool) {
        let pattern = if extended {
            compile_extended(pattern_text).unwrap()
        } else {
            compile_basic(pattern_text).unwrap()
        };
        let (subst, _) = parse_subst(command).unwrap();
        let mut out = [0u8; 512];
        let (len, replaced) = apply(&pattern, &subst, line, &mut out);
        (
            String::from_utf8_lossy(&out[..len]).into_owned(),
            replaced,
        )
    }

    #[test]
    fn test_first_only_by_default() {
        let (text, replaced) = run("o", false, "/o/0/", "foo boo");
        assert_eq!(text, "f0o boo");
        assert!(replaced);
    }

    #[test]
    fn test_global_flag() {
        let (text, _) = run("o", false, "/o/0/g", "foo boo");
        assert_eq!(text, "f00 b00");
    }

    #[test]
    fn test_nth_match_only() {
        // Matches sit at 1, 2, 5; replacing the second one turns
        // "foo boo" into "fo0 boo".
        let (text, _) = run("o", false, "/o/0/2", "foo boo");
        assert_eq!(text, "fo0 boo");
    }

    #[test]
    fn test_ampersand_replays_match() {
        let (text, _) = run("o+", true, "/o+/<&>/g", "foo boo");
        assert_eq!(text, "f<oo> b<oo>");
    }

    #[test]
    fn test_group_reference() {
        // Group 1 holds the user, group 2 the host; the replacement swaps
        // them around a dot.
        let (text, _) = run("\\([a-z]*\\)@\\([a-z]*\\)", false, "/@/...\\2.\\1/", "u@h");
        assert_eq!(text, "...h.u");
    }

    #[test]
    fn test_custom_delimiter() {
        let (subst, consumed) = parse_subst("#a#b#g").unwrap();
        assert_eq!(subst.pattern_text, "a");
        assert_eq!(subst.replacement, "b");
        assert_eq!(subst.scope, SubstScope::Global);
        assert_eq!(consumed, 6);
    }

    #[test]
    fn test_print_flag() {
        let (subst, _) = parse_subst("/a/b/p").unwrap();
        assert!(subst.print);
        assert_eq!(subst.scope, SubstScope::First);
    }

    #[test]
    fn test_missing_delimiter_rejected() {
        assert_eq!(parse_subst("/ab"), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_zero_nth_rejected() {
        assert_eq!(parse_subst("/a/b/0"), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_no_match_copies_input() {
        let (text, replaced) = run("z", false, "/z/q/", "abc");
        assert_eq!(text, "abc");
        assert!(!replaced);
    }

    #[test]
    fn test_empty_match_walk_terminates() {
        let (text, _) = run("x*", false, "/x*/-/g", "ab");
        assert_eq!(text, "-a-b-");
    }
}
