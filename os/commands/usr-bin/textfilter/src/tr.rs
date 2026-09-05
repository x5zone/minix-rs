//! Character set translation, deletion, and squeezing behind `tr`.
//!
//! Ground truth: `minix3/usr.bin/tr/tr.c` (283 lines; usage collected
//! around lines 58 to 125). Three operations over two character sets:
//!
//! - Translation: each byte in set 1 becomes the corresponding byte of set
//!   2 (a short set 2 repeats its last byte, matching the C tool).
//! - Deletion (`-d`): bytes in set 1 vanish.
//! - Squeezing (`-s`): runs of bytes in the squeeze set collapse to one.
//!
//! Sets are built from literals, ranges (`a-z`), repeats (`[a*5]`), and
//! classes (`[:alpha:]` and friends — recognised by name, ASCII only).
//! Membership tests go through [`CharClass`] with one implementation per
//! set shape, so callers never special case ranges versus lists.

use crate::TextError;

/// Maximum members in an explicit byte list set.
pub const MAX_MEMBERS: usize = 64;

/// One character set flavour: how bytes join the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    /// Explicit member bytes (up to 64).
    List(CharList),
    /// Inclusive byte range.
    Range(u8, u8),
    /// Named class (`alpha`, `digit`, `space`, `upper`, `lower`, `alnum`).
    Named(NamedClass),
}

/// Explicit member list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharList {
    /// Member bytes.
    pub members: [u8; MAX_MEMBERS],
    /// How many of `members` are used.
    pub count: u8,
}

/// Named character classes (ASCII only, C locale behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedClass {
    /// Letters.
    Alpha,
    /// Letters and digits.
    Alnum,
    /// Digits.
    Digit,
    /// Blanks (space and tab).
    Blank,
    /// Whitespace (space, tab, newline, carriage return, vertical tab,
    /// form feed).
    Space,
    /// Uppercase letters.
    Upper,
    /// Lowercase letters.
    Lower,
}

impl CharClass {
    /// Decide whether `byte` belongs to the set.
    pub fn contains(self, byte: u8) -> bool {
        match self {
            CharClass::List(list) => list.members[..list.count as usize].contains(&byte),
            CharClass::Range(low, high) => low <= byte && byte <= high,
            CharClass::Named(class) => match class {
                NamedClass::Alpha => byte.is_ascii_alphabetic(),
                NamedClass::Alnum => byte.is_ascii_alphanumeric(),
                NamedClass::Digit => byte.is_ascii_digit(),
                NamedClass::Blank => matches!(byte, b' ' | b'\t'),
                NamedClass::Space => matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C),
                NamedClass::Upper => byte.is_ascii_uppercase(),
                NamedClass::Lower => byte.is_ascii_lowercase(),
            },
        }
    }
}

/// Parse one set expression (`abc`, `a-z`, `[a*5]`, `[:alpha:]`, escapes
/// like `\n`) into up to 8 classes, expanding to at most 256 member bytes
/// in `expanded` (returns the used count).
///
/// Repeat counts that would overflow the 256 table are an error, matching
/// the C tool refusing absurd sets rather than wrapping.
pub fn parse_set(text: &str, expanded: &mut [u8; 256]) -> Result<(usize, [CharClass; 8], usize), TextError> {
    let bytes = text.as_bytes();
    let mut classes: [CharClass; 8] = [CharClass::Range(0, 0); 8];
    let mut class_count = 0;
    let mut pos = 0;
    let mut used = 0;
    while pos < bytes.len() {
        if bytes[pos] == b'[' && pos + 1 < bytes.len() && bytes[pos + 1] == b':' {
            let start = pos + 2;
            let mut end = start;
            while end + 1 < bytes.len() && !(bytes[end] == b':' && bytes[end + 1] == b']') {
                end += 1;
            }
            if end + 1 >= bytes.len() {
                return Err(TextError::InvalidArgument);
            }
            let name = &text[start..end];
            let class = match name {
                "alpha" => NamedClass::Alpha,
                "alnum" => NamedClass::Alnum,
                "digit" => NamedClass::Digit,
                "blank" => NamedClass::Blank,
                "space" => NamedClass::Space,
                "upper" => NamedClass::Upper,
                "lower" => NamedClass::Lower,
                _ => return Err(TextError::InvalidArgument),
            };
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Named(class);
            class_count += 1;
            for byte in 0..=255u16 {
                let byte = byte as u8;
                if CharClass::Named(class).contains(byte) {
                    push_byte(expanded, &mut used, byte)?;
                }
            }
            pos = end + 2;
        } else if bytes[pos] == b'[' && pos + 2 < bytes.len() {
            // Repeat `[c*n]`: find the closing bracket first.
            let mut end = pos + 1;
            while end < bytes.len() && bytes[end] != b']' {
                end += 1;
            }
            if end >= bytes.len() {
                return Err(TextError::InvalidArgument);
            }
            let (member, count) = parse_repeat(&text[pos..=end])?;
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(member, member);
            class_count += 1;
            for _ in 0..count {
                push_byte(expanded, &mut used, member)?;
            }
            pos = end + 1;
        } else if pos + 2 < bytes.len() && bytes[pos + 1] == b'-' {
            let (low, high) = (bytes[pos], bytes[pos + 2]);
            if high < low {
                return Err(TextError::InvalidArgument);
            }
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(low, high);
            class_count += 1;
            let mut byte = low;
            loop {
                push_byte(expanded, &mut used, byte)?;
                if byte == high {
                    break;
                }
                byte += 1;
            }
            pos += 3;
        } else if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
            let byte = match bytes[pos + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'\\' => b'\\',
                other => other,
            };
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(byte, byte);
            class_count += 1;
            push_byte(expanded, &mut used, byte)?;
            pos += 2;
        } else {
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(bytes[pos], bytes[pos]);
            class_count += 1;
            push_byte(expanded, &mut used, bytes[pos])?;
            pos += 1;
        }
    }
    Ok((used, classes, class_count))
}

/// Append one expanded member byte; the 256 table bounds every set.
fn push_byte(expanded: &mut [u8; 256], used: &mut usize, byte: u8) -> Result<(), TextError> {
    if *used >= expanded.len() {
        return Err(TextError::InvalidArgument);
    }
    expanded[*used] = byte;
    *used += 1;
    Ok(())
}

/// Parse a `[c*n]` repeat expression (brackets included).
fn parse_repeat(text: &str) -> Result<(u8, usize), TextError> {
    let inner = text.strip_prefix('[').and_then(|s| s.strip_suffix(']')).ok_or(TextError::InvalidArgument)?;
    let (member_text, count_text) = inner.split_once('*').ok_or(TextError::InvalidArgument)?;
    let member_bytes = member_text.as_bytes();
    if member_bytes.len() != 1 {
        return Err(TextError::InvalidArgument);
    }
    let mut count: usize = 0;
    if count_text.is_empty() || !count_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TextError::InvalidArgument);
    }
    for byte in count_text.bytes() {
        count = count
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as usize))
            .ok_or(TextError::InvalidArgument)?;
    }
    Ok((member_bytes[0], count))
}

/// Translate `input` through two expanded sets into `out`.
///
/// `set1`/`set2` are the expanded member lists with their lengths; a short
/// second set repeats its last byte. Returns the used byte count.
pub fn translate(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    out: &mut [u8],
) -> Result<usize, TextError> {
    if set1.is_empty() || set2.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    // Build the 256 byte map once: identity, then set1 positions point at
    // the corresponding (or repeated last) set2 byte.
    let mut map = [0u8; 256];
    for (index, slot) in map.iter_mut().enumerate() {
        *slot = index as u8;
    }
    for (index, byte) in set1.iter().enumerate() {
        let replacement = if index < set2.len() {
            set2[index]
        } else {
            set2[set2.len() - 1]
        };
        map[*byte as usize] = replacement;
    }
    if input.len() > out.len() {
        return Err(TextError::TooLong);
    }
    for (index, byte) in input.iter().enumerate() {
        out[index] = map[*byte as usize];
    }
    Ok(input.len())
}

/// Delete every byte of `set` from `input` into `out`.
pub fn delete(input: &[u8], set: &[u8], out: &mut [u8]) -> Result<usize, TextError> {
    let mut table = [false; 256];
    for byte in set {
        table[*byte as usize] = true;
    }
    let mut written = 0;
    for byte in input {
        if !table[*byte as usize] {
            if written >= out.len() {
                return Err(TextError::TooLong);
            }
            out[written] = *byte;
            written += 1;
        }
    }
    Ok(written)
}

/// Squeeze runs of `set` bytes in `input` down to one each.
pub fn squeeze(input: &[u8], set: &[u8], out: &mut [u8]) -> Result<usize, TextError> {
    let mut table = [false; 256];
    for byte in set {
        table[*byte as usize] = true;
    }
    let mut written = 0;
    let mut previous: Option<u8> = None;
    for byte in input {
        if table[*byte as usize] && previous == Some(*byte) {
            continue;
        }
        if written >= out.len() {
            return Err(TextError::TooLong);
        }
        out[written] = *byte;
        written += 1;
        previous = Some(*byte);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded_set(text: &str) -> ([u8; 256], usize) {
        let mut expanded = [0u8; 256];
        let (used, _, _) = parse_set(text, &mut expanded).unwrap();
        (expanded, used)
    }

    #[test]
    fn test_translate_basic() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("XYZ");
        let mut out = [0u8; 16];
        let len = translate(b"axbxc", &set1[..len1], &set2[..len2], &mut out).unwrap();
        // Only set members translate; `x` passes through twice.
        assert_eq!(&out[..len], b"XxYxZ");
    }

    #[test]
    fn test_short_second_set_repeats_last() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("X");
        let mut out = [0u8; 16];
        let len = translate(b"abc", &set1[..len1], &set2[..len2], &mut out).unwrap();
        assert_eq!(&out[..len], b"XXX");
    }

    #[test]
    fn test_range_and_repeat_sets() {
        let (set1, len1) = expanded_set("a-c");
        assert_eq!(len1, 3);
        let (set2, len2) = expanded_set("[x*2]");
        assert_eq!((len2, set2[0], set2[1]), (2, b'x', b'x'));
        let _ = set1;
    }

    #[test]
    fn test_named_class() {
        let (set, len) = expanded_set("[:digit:]");
        assert_eq!(len, 10);
        assert!(set[..len].contains(&b'5'));
    }

    #[test]
    fn test_delete_and_squeeze() {
        let (vowels, vowels_len) = expanded_set("aeiou");
        let mut out = [0u8; 16];
        let len = delete(b"hello", &vowels[..vowels_len], &mut out).unwrap();
        assert_eq!(&out[..len], b"hll");
        let (spaces, spaces_len) = expanded_set(" ");
        let len = squeeze(b"a  b   c", &spaces[..spaces_len], &mut out).unwrap();
        assert_eq!(&out[..len], b"a b c");
    }

    #[test]
    fn test_class_strategies_agree() {
        let list = CharClass::List(CharList {
            members: {
                let mut members = [0u8; MAX_MEMBERS];
                members[0] = b'a';
                members[1] = b'b';
                members
            },
            count: 2,
        });
        let range = CharClass::Range(b'a', b'b');
        for byte in 0..=255u16 {
            assert_eq!(list.contains(byte as u8), range.contains(byte as u8));
        }
    }

    #[test]
    fn test_bad_sets_rejected() {
        let mut expanded = [0u8; 256];
        assert_eq!(parse_set("z-a", &mut expanded).map(|_| ()), Err(TextError::InvalidArgument));
        assert_eq!(parse_set("[:nope:]", &mut expanded).map(|_| ()), Err(TextError::InvalidArgument));
        assert_eq!(parse_set("[ab*2]", &mut expanded).map(|_| ()), Err(TextError::InvalidArgument));
    }
}
