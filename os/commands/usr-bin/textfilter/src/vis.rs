//! Visible encoding and decoding for `vis` and `unvis`.
//!
//! Ground truth: `minix3/lib/libc/gen/vis.c` (the vis(3) engine the
//! command drives) and `minix3/usr.bin/unvis/unvis.c` (the decoder).
//! Default mode: graphic bytes (0x21..=0x7E) pass through; anything
//! else gains a backslash (unless no-slash), a `M` prefix with the
//! eighth bit stripped when set, then a control byte renders as `^X`
//! (`?` for 0x7F) and a non-control as `-x`. `-c` selects C-style
//! escapes (`\n`, `\r`, `\b`, `\a`, `\v`, `\t`, `\f`, `\s`, `\0`, and
//! `\c` for graphic non-octal bytes, with the C's skip list of letters
//! that must fall through, vis.c:210-267). `-o` selects three-digit
//! octal for every byte. The decoder accepts the same grammar —
//! `M`-meta prefixes, `^X` controls, and backslash-octal runs — which
//! is what the encoder produces.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Encoder/decoder configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisOptions {
    /// `-c`: C-style escapes for known characters.
    pub cstyle: bool,
    /// `-o`: three-digit octal for every byte.
    pub octal: bool,
    /// `-n`: no backslash before the escape sequences.
    pub no_slash: bool,
}

/// Encodes the input per the options.
pub fn encode(input: &[u8], options: &VisOptions) -> String {
    let mut out = String::new();
    for (index, &byte) in input.iter().enumerate() {
        let next = input.get(index + 1).copied().unwrap_or(0);
        encode_byte(byte, next, options, &mut out);
    }
    out
}

fn encode_byte(byte: u8, next: u8, options: &VisOptions, out: &mut String) {
    // do_svis passes graphic bytes and whitespace through unchanged
    // (lib/libc/gen/vis.c:311-315: `!iswextra && (ISGRAPH || iswwhite)`;
    // the command's extra list defaults to empty, so iswextra is false).
    if byte.is_ascii_graphic() || byte == b' ' || byte == b'\t' || byte == b'\n' {
        out.push(byte as char);
        return;
    }
    if options.octal {
        push_octal(byte, out);
        return;
    }
    if options.cstyle {
        if let Some(escape) = cstyle_escape(byte, next) {
            out.push_str(&escape);
            return;
        }
    }
    // Default M-notation (vis.c:270-292): graphic bytes pass through.
    if (0x21..=0x7E).contains(&byte) {
        out.push(byte as char);
        return;
    }
    if !options.no_slash {
        out.push('\\');
    }
    let mut stripped = byte;
    if stripped & 0x80 != 0 {
        stripped &= 0x7F;
        out.push('M');
    }
    if stripped < 0x20 || stripped == 0x7F {
        out.push('^');
        out.push(if stripped == 0x7F {
            '?'
        } else {
            char::from(stripped + 0x40)
        });
    } else {
        out.push('-');
        out.push(char::from(stripped));
    }
}

/// C-style escape for a byte, when the `-c` rules apply to it
/// (vis.c:210-267): named escapes for controls, `\c` for graphic
/// non-octal bytes with the skip list respected, `\0` doubled before
/// an octal digit. `None` means fall through to the default notation.
fn cstyle_escape(byte: u8, next: u8) -> Option<String> {
    let named: Option<&str> = match byte {
        b'\n' => Some("\\n"),
        b'\r' => Some("\\r"),
        0x08 => Some("\\b"),
        0x07 => Some("\\a"),
        0x0B => Some("\\v"),
        b'\t' => Some("\\t"),
        0x0C => Some("\\f"),
        b' ' => Some("\\s"),
        0x00 => Some("\\0"),
        _ => None,
    };
    if let Some(escape) = named {
        // A `\0` before an octal digit doubles the zeros.
        if byte == 0x00 && (0x30..=0x37).contains(&next) {
            return Some("\\000".to_string());
        }
        return Some(escape.to_string());
    }
    // Graphic bytes that are not octal digits take `\c` — except the
    // skip list letters whose escape form would be ambiguous.
    let skip = matches!(
        byte,
        b'n' | b'r'
            | b'b'
            | b'a'
            | b'v'
            | b't'
            | b'f'
            | b's'
            | b'0'
            | b'M'
            | b'^'
            | b'$'
    );
    if !skip && byte.is_ascii_graphic() && !(b'0'..=b'7').contains(&byte) {
        let mut out = String::new();
        out.push('\\');
        out.push(byte as char);
        return Some(out);
    }
    None
}

fn push_octal(byte: u8, out: &mut String) {
    out.push('\\');
    out.push((b'0' + ((byte >> 6) & 0o3)) as char);
    out.push((b'0' + ((byte >> 3) & 0o7)) as char);
    out.push((b'0' + (byte & 0o7)) as char);
}

/// Decodes visible text back into bytes, mirroring the C `unvis`
/// state machine (lib/libc/gen/unvis.c:217-330). Returns the decoded
/// bytes. Incomplete escapes at the end are dropped (the C's
/// end-of-input flush).
pub fn decode(input: &str) -> Vec<u8> {
    #[derive(Debug, PartialEq, Clone, Copy)]
    enum St {
        Ground,
        Start,
        Meta,
        Meta1,
        Ctrl,
        Octal2,
        Octal3,
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut st = St::Ground;
    let mut value: u8 = 0;
    let mut cursor = 0usize;
    #[cfg(test)]
    eprintln!("DBG decode input={:?} len={}", input, bytes.len());
    while cursor < bytes.len() {
        let c = bytes[cursor];
        cursor += 1;
        #[cfg(test)]
        eprintln!("DBG st={:?} c={:?}", st, c);
        match st {
            St::Ground => {
                if c == b'\\' {
                    st = St::Start;
                } else {
                    out.push(c);
                }
            }
            St::Start => match c {
                b'\\' => {
                    out.push(b'\\');
                    st = St::Ground;
                }
                b'0'..=b'7' => {
                    value = c - b'0';
                    st = St::Octal2;
                }
                b'M' => {
                    value = 0o200;
                    st = St::Meta;
                }
                b'^' => st = St::Ctrl,
                b'n' => {
                    out.push(b'\n');
                    st = St::Ground;
                }
                b'r' => {
                    out.push(b'\r');
                    st = St::Ground;
                }
                b'b' => {
                    out.push(0x08);
                    st = St::Ground;
                }
                b'a' => {
                    out.push(0x07);
                    st = St::Ground;
                }
                b'v' => {
                    out.push(0x0B);
                    st = St::Ground;
                }
                b't' => {
                    out.push(b'\t');
                    st = St::Ground;
                }
                b'f' => {
                    out.push(0x0C);
                    st = St::Ground;
                }
                b's' => {
                    out.push(b' ');
                    st = St::Ground;
                }
                b'E' => {
                    out.push(0x1B);
                    st = St::Ground;
                }
                // A hidden newline or marker emits nothing.
                b'\n' | b'$' => st = St::Ground,
                other if other.is_ascii_graphic() => {
                    out.push(other);
                    st = St::Ground;
                }
                // Non-graphic bytes after a backslash are dropped (the
                // C's default arm requires isgraph).
                _ => st = St::Ground,
            },
            St::Meta => match c {
                b'-' => st = St::Meta1,
                b'^' => st = St::Ctrl,
                _ => {
                    out.push(c | 0o200);
                    st = St::Ground;
                }
            },
            St::Meta1 => {
                out.push(c | 0o200);
                st = St::Ground;
            }
            St::Ctrl => {
                let produced = if c == b'?' { 0x7F } else { c & 0x1F };
                // The pending 0200 from a preceding `M` joins here.
                out.push(value | produced);
                st = St::Ground;
            }
            St::Octal2 => {
                if (b'0'..=b'7').contains(&c) {
                    value = (value << 3) | (c - b'0');
                    st = St::Octal3;
                } else {
                    // Non-digit: the one-digit value emits and the byte
                    // is reprocessed (the C's ungetc arm).
                    out.push(value);
                    cursor -= 1;
                    st = St::Ground;
                }
            }
            St::Octal3 => {
                if (b'0'..=b'7').contains(&c) {
                    value = (value << 3) | (c - b'0');
                    // A completed octal run emits its byte right away
                    // and stays ready for the next one.
                    out.push(value);
                    st = St::Ground;
                } else {
                    out.push(value);
                    cursor -= 1;
                    st = St::Ground;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphic_bytes_and_whitespace_pass_through() {
        // do_svis passes graphic bytes and whitespace (space/tab/newline)
        // through before any flag-specific encoding applies
        // (lib/libc/gen/vis.c:311-315).
        assert_eq!(encode(b"abc XYZ\n", &VisOptions::default()), "abc XYZ\n");
    }

    #[test]
    fn test_control_bytes_render_as_caret_notation() {
        assert_eq!(encode(b"\x00", &VisOptions::default()), "\\^@");
        assert_eq!(encode(b"\x1b", &VisOptions::default()), "\\^[");
        assert_eq!(encode(b"\x7f", &VisOptions::default()), "\\^?");
    }

    #[test]
    fn test_high_bit_gains_the_meta_prefix() {
        // 0xFF: M + ^? ; 0xE9 (é): M + - i.
        assert_eq!(encode(b"\xff", &VisOptions::default()), "\\M^?");
        assert_eq!(encode(b"\xe9", &VisOptions::default()), "\\M-i");
    }

    #[test]
    fn test_whitespace_passes_through() {
        // iswwhite = space/tab/newline: all three pass through.
        assert_eq!(encode(b"a b\tc\nd", &VisOptions::default()), "a b\tc\nd");
    }

    #[test]
    fn test_no_slash_drops_the_backslash() {
        let options = VisOptions {
            no_slash: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"\x00", &options), "^@");
    }

    #[test]
    fn test_octal_style_applies_to_encoded_bytes() {
        // Graphic bytes still pass through with `-o`; only encoded bytes
        // (like NUL) take the three-digit octal form.
        let options = VisOptions {
            octal: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\x00", &options), "a\\000");
    }

    #[test]
    fn test_cstyle_named_escapes() {
        let options = VisOptions {
            cstyle: true,
            ..VisOptions::default()
        };
        // Tab and newline pass through (iswwhite); CR is a control and
        // takes the named escape; NUL takes the zero escape.
        assert_eq!(encode(b"a\tb\nc\rd\0", &options), "a\tb\nc\\rd\\0");
    }

    #[test]
    fn test_cstyle_graphic_passes_through_too() {
        // do_svis passes graphic bytes through BEFORE the cstyle switch
        // runs, so even with `-c` a graphic byte stays itself.
        let options = VisOptions {
            cstyle: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"|", &options), "|");
        assert_eq!(encode(b"t", &options), "t");
    }

    #[test]
    fn test_decode_reverses_the_encoding() {
        // 0x5C (backslash) is graphic and passes through unescaped, so
        // `unvis` reads it as an escape start — the one byte the
        // default mode cannot round-trip (the same asymmetry exists in
        // the C pair).
        for byte in 0x00..=0xFFu8 {
            if byte == 0x5C {
                continue;
            }
            let options = VisOptions::default();
            let encoded = encode(&[byte], &options);
            let decoded = decode(&encoded);
            assert_eq!(decoded, vec![byte], "byte {:#04x} via {:?}", byte, encoded);
        }
    }

    #[test]
    fn test_decode_octal_runs() {
        assert_eq!(decode("\\101\\010"), vec![0x41, 0x08]);
    }

    #[test]
    fn test_decode_meta_and_caret_need_the_backslash() {
        // unvis only starts an escape on the backslash; a bare "M-^?"
        // is four literal characters (the C ground state emits every
        // non-backslash byte unchanged).
        assert_eq!(decode("M-^?"), b"M-^?");
        // The canonical 0xFF encoding: backslash, M (meta), caret, ?.
        let escaped = "\\M^?".as_bytes();
        assert_eq!(decode(std::str::from_utf8(&escaped).unwrap()), vec![0xFF]);
        // The dashed form decodes as two bytes per S_META1.
        assert_eq!(decode("\\M-^?"), vec![0xDE, 0x3F]);
        assert_eq!(decode("\\^I"), vec![0x09]);
    }
}
