//! hexdump — ASCII, decimal, hex, octal dump (minix3/usr.bin/hexdump/).
//!
//! Deciding half: the seven canonical option formats (default/-b/-c/-C
//! /-d/-o/-x) rendered through the same mechanics as parse.c +
//! display.c: 16-byte blocks, per-group trailing-space suppression on
//! the final repetition, blank `%<width>s` fields past end-of-data,
//! zero-width `%_p` pads, `*` for duplicate full blocks and the final
//! address line. The `-e`/`-f` format mini-language is a registered
//! remaining corner.
use alloc::vec::Vec;
use alloc::string::{String, ToString};
use alloc::{format, vec};

/// Output format presets, one per hexsyntax option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpFormat {
    /// No option: 8/2 "%04x ".
    Default,
    /// `-b`: 16/1 "%03o ".
    OctalBytes,
    /// `-c`: 16/1 "%3_c ".
    Chars,
    /// `-C`: canonical hex+ASCII, gutter width 8.
    Canonical,
    /// `-d`: 8/2 "  %05u ".
    Decimal,
    /// `-o`: 8/2 " %06o ".
    OctalShort,
    /// `-x`: 8/2 "   %04x ".
    Hex,
}

/// Dump options other than the format.
#[derive(Debug, Clone, Default)]
pub struct HexOptions {
    /// `-v`: print every block, no `*` suppression.
    pub verbose: bool,
    /// `-n length`: how many input bytes to show.
    pub length: Option<usize>,
}

pub const BLOCKSIZE: usize = 16;

#[derive(Debug, PartialEq)]
pub enum HexError {
    BadLength(String),
    BadSkip(String),
    /// `-e`/`-f` are not implemented (registered corner).
    FormatLanguageUnsupported,
}

/// A per-group unit field: how many units, bytes each, rendered width
/// (including the format's trailing space), leading pad spaces, and
/// the value renderer.
pub struct GroupSpec {
    count: usize,
    bytes: usize,
    /// True when this group re-reads from the block start (a fresh FS
    /// in the C, like `-C`'s ASCII half).
    pub restart: bool,
    /// Whether the format's own trailing space separates units
    /// (`"%02x "` yes, `"%_p"` no).
    pub trailing_space: bool,
    /// Spaces printed before the unit value inside the field.
    lead: usize,
    /// Digits/chars after the lead (the "%04x" part), 0-width pad-capable.
    body: UnitBody,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum UnitBody {
    HexLower(usize),
    Octal(usize),
    Decimal(usize),
    CharEscape,
    Printable,
}

impl GroupSpec {
    /// Field width including the trailing space; `%_p` pads to zero
    /// width (glibc prints "" for the bpad'd "%_s").
    fn field_width(&self) -> usize {
        match self.body {
            UnitBody::Printable => 0,
            _ => self.lead + self.body_digits() + 1,
        }
    }
    fn body_digits(&self) -> usize {
        match self.body {
            UnitBody::HexLower(w) | UnitBody::Octal(w) | UnitBody::Decimal(w) => w,
            UnitBody::CharEscape => 3,
            UnitBody::Printable => 1,
        }
    }
}

/// A row preset: gutter width, gutter trailing spaces, then the field
/// groups with text spacers.
pub struct RowSpec {
    pub gutter_width: usize,
    gutter_pad: usize,
    pub groups: Vec<(GroupSpec, &'static str)>,
}

pub fn row_spec(format: DumpFormat) -> RowSpec {
    match format {
        DumpFormat::Default => RowSpec {
            gutter_width: 7,
            gutter_pad: 1,
            groups: vec![(GroupSpec { count: 8, bytes: 2, lead: 0, body: UnitBody::HexLower(4), restart: false, trailing_space: true }, "\n")],
        },
        DumpFormat::Hex => RowSpec {
            gutter_width: 7,
            gutter_pad: 1,
            groups: vec![(GroupSpec { count: 8, bytes: 2, lead: 3, body: UnitBody::HexLower(4), restart: false, trailing_space: true }, "\n")],
        },
        DumpFormat::OctalShort => RowSpec {
            gutter_width: 7,
            gutter_pad: 1,
            groups: vec![(GroupSpec { count: 8, bytes: 2, lead: 1, body: UnitBody::Octal(6), restart: false, trailing_space: true }, "\n")],
        },
        DumpFormat::Decimal => RowSpec {
            gutter_width: 7,
            gutter_pad: 1,
            groups: vec![(GroupSpec { count: 8, bytes: 2, lead: 2, body: UnitBody::Decimal(5), restart: false, trailing_space: true }, "\n")],
        },
        DumpFormat::OctalBytes => RowSpec {
            gutter_width: 7,
            gutter_pad: 1,
            groups: vec![(GroupSpec { count: 16, bytes: 1, lead: 0, body: UnitBody::Octal(3), restart: false, trailing_space: true }, "\n")],
        },
        DumpFormat::Chars => RowSpec {
            gutter_width: 7,
            gutter_pad: 1,
            groups: vec![(GroupSpec { count: 16, bytes: 1, lead: 0, body: UnitBody::CharEscape, restart: false, trailing_space: true }, "\n")],
        },
        DumpFormat::Canonical => RowSpec {
            gutter_width: 8,
            gutter_pad: 2,
            groups: vec![
                (GroupSpec { count: 8, bytes: 1, lead: 0, body: UnitBody::HexLower(2), restart: true, trailing_space: true }, "  "),
                (GroupSpec { count: 8, bytes: 1, lead: 0, body: UnitBody::HexLower(2), restart: false, trailing_space: true }, "  |"),
                (GroupSpec { count: 16, bytes: 1, lead: 0, body: UnitBody::Printable, restart: true, trailing_space: false }, "|\n"),
            ],
        },
    }
}

/// conv_c: the `%3_c` body (conv.c:51-97).
pub fn char_escape(b: u8) -> String {
    match b {
        0x00 => " \\0".to_string(),
        0x07 => " \\a".to_string(),
        0x08 => " \\b".to_string(),
        0x0c => " \\f".to_string(),
        0x0a => " \\n".to_string(),
        0x0d => " \\r".to_string(),
        0x09 => " \\t".to_string(),
        0x0b => " \\v".to_string(),
        _ if (0x20..=0x7e).contains(&b) => format!("  {}", b as char),
        _ => format!("{:03o}", b),
    }
}

fn render_unit(group: &GroupSpec, bytes: &[u8]) -> String {
    let body = match group.body {
        UnitBody::HexLower(w) => {
            // Little-endian word, as the C memcpy's into u2/u4.
            let mut v = 0u64;
            for (i, b) in bytes.iter().enumerate().take(group.bytes) {
                v |= (*b as u64) << (8 * i);
            }
            format!("{:0w$x}", v, w = w)
        }
        UnitBody::Octal(w) => {
            let mut v = 0u64;
            for (i, b) in bytes.iter().enumerate().take(group.bytes) {
                v |= (*b as u64) << (8 * i);
            }
            format!("{:0w$o}", v, w = w)
        }
        UnitBody::Decimal(w) => {
            let mut v = 0u64;
            for (i, b) in bytes.iter().enumerate().take(group.bytes) {
                v |= (*b as u64) << (8 * i);
            }
            format!("{:0w$}", v, w = w)
        }
        UnitBody::CharEscape => char_escape(bytes[0]),
        UnitBody::Printable => {
            let b = bytes[0];
            if (0x20..=0x7e).contains(&b) {
                (b as char).to_string()
            } else {
                ".".to_string()
            }
        }
    };
    format!("{}{}", " ".repeat(group.lead), body)
}

/// Parses the `-s` argument: strtol base 0 with b/k/m block suffixes.
pub fn parse_skip(arg: &str) -> Result<u64, HexError> {
    let lower = arg.as_bytes();
    let mut i = 0;
    let neg = if i < lower.len() && (lower[i] == b'+' || lower[i] == b'-') {
        let n = lower[i] == b'-';
        i += 1;
        n
    } else {
        false
    };
    let (value, used) = if i + 1 < lower.len() && lower[i] == b'0' && (lower[i + 1] | 0x20) == b'x' {
        let ds = i + 2;
        let mut j = ds;
        while j < lower.len() && lower[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j == ds {
            return Err(HexError::BadSkip(arg.to_string()));
        }
        let v = u64::from_str_radix(&arg[ds..j], 16).map_err(|_| HexError::BadSkip(arg.to_string()))?;
        (v, j)
    } else {
        let ds = i;
        let mut j = ds;
        while j < lower.len() && lower[j].is_ascii_digit() {
            j += 1;
        }
        if j == ds {
            return Err(HexError::BadSkip(arg.to_string()));
        }
        (arg[ds..j].parse::<u64>().map_err(|_| HexError::BadSkip(arg.to_string()))?, j)
    };
    let mut value = if neg { 0 } else { value };
    if used < lower.len() {
        value = match lower[used] {
            b'b' => value.wrapping_mul(512),
            b'k' => value.wrapping_mul(1024),
            b'm' => value.wrapping_mul(1048576),
            _ => return Err(HexError::BadSkip(arg.to_string())),
        };
    }
    Ok(value)
}

/// One step of the get() walk: a block to render (all formats print
/// one row each per block, like display.c's fs loop) or a `*`
/// standing in for suppressed duplicates.
#[derive(Debug, PartialEq)]
pub enum BlockEvent<'a> {
    Block { addr: usize, bytes: &'a [u8] },
    Star,
}

/// Walks the blocks with get()'s dedup state machine (First → Wait →
/// Dup); the final partial block always passes through.
pub fn walk_blocks(data: &[u8], verbose: bool) -> Vec<BlockEvent<'_>> {
    let mut events: Vec<BlockEvent> = Vec::new();
    if data.is_empty() {
        return events;
    }
    #[derive(PartialEq)]
    enum VFlag {
        First,
        Wait,
        Dup,
        All,
    }
    let mut vflag = if verbose { VFlag::All } else { VFlag::First };
    let mut last_block: Vec<u8> = Vec::new();
    let mut addr = 0usize;
    while addr < data.len() {
        let end = (addr + BLOCKSIZE).min(data.len());
        let block = &data[addr..end];
        let full = end - addr == BLOCKSIZE;
        if full {
            let same = last_block.len() == BLOCKSIZE && block == last_block.as_slice();
            if same && vflag != VFlag::All {
                if vflag == VFlag::Wait {
                    events.push(BlockEvent::Star);
                }
                vflag = VFlag::Dup;
                addr += BLOCKSIZE;
                continue;
            }
        }
        events.push(BlockEvent::Block { addr, bytes: block });
        if full {
            last_block.clear();
            last_block.extend_from_slice(block);
            if vflag == VFlag::First || vflag == VFlag::Dup {
                vflag = VFlag::Wait;
            }
        }
        addr += BLOCKSIZE;
    }
    events
}

/// Dumps `input` (already skip-applied by the caller) as text bytes:
/// every block renders one row, then the final address line.
pub fn dump(input: &[u8], format: DumpFormat, options: &HexOptions) -> Vec<u8> {
    let data: &[u8] = match options.length {
        Some(n) => &input[..n.min(input.len())],
        None => input,
    };
    let spec = row_spec(format);
    let mut out: Vec<u8> = Vec::new();
    for event in walk_blocks(data, options.verbose) {
        match event {
            BlockEvent::Block { addr, bytes } => {
                render_row(bytes, addr, data.len(), &spec, &mut out)
            }
            BlockEvent::Star => out.extend_from_slice(b"*\n"),
        }
    }
    if !data.is_empty() {
        // endfu: the final address line, only when there was data.
        let line = format!("{:0w$x}\n", data.len(), w = spec.gutter_width);
        out.extend_from_slice(line.as_bytes());
    }
    out
}

/// Renders one block row: gutter, groups with final-space suppression
/// on each group's last unit, blank fields past end-of-data.
pub fn render_row(block: &[u8], addr: usize, total: usize, spec: &RowSpec, out: &mut Vec<u8>) {
    let gutter = format!("{:0w$x}", addr, w = spec.gutter_width);
    out.extend_from_slice(gutter.as_bytes());
    for _ in 0..spec.gutter_pad {
        out.push(b' ');
    }
    let mut offset = 0usize;
    for (group, tail) in &spec.groups {
        if group.restart {
            offset = 0;
        }
        for i in 0..group.count {
            let start = offset;
            let end = (start + group.bytes).min(block.len());
            let is_last = i + 1 == group.count;
            if start >= block.len() || start + addr >= total {
                // Past end-of-data: blank field ("%<w>s " with "").
                let width = group.field_width();
                let spaces = if is_last { width.saturating_sub(1) } else { width };
                for _ in 0..spaces {
                    out.push(b' ');
                }
            } else {
                out.extend_from_slice(render_unit(group, &block[start..end]).as_bytes());
                if group.trailing_space && !is_last {
                    out.push(b' ');
                }
            }
            offset = start + group.bytes;
        }
        out.extend_from_slice(tail.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(out: Vec<u8>) -> String {
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn test_default_dump_partial_row_pads_to_fixed_width() {
        // Reference: hexdump of "hi\n" (3 bytes).
        let out = dump(b"hi\n", DumpFormat::Default, &HexOptions::default());
        let s = text(out);
        let mut lines = s.lines();
        let row = lines.next().unwrap();
        assert_eq!(row.len(), 47);
        assert!(row.starts_with("0000000 6968 000a "));
        assert_eq!(lines.next(), Some("0000003"));
    }

    #[test]
    fn test_full_row_and_repeats() {
        // 16 A's: one full row (8 units, last trimmed), then the
        // final address 0000010.
        let data = vec![b'A'; 16];
        let out = dump(&data, DumpFormat::Default, &HexOptions::default());
        let s = text(out);
        let mut lines = s.lines();
        let row = lines.next().unwrap();
        assert_eq!(row, "0000000 4141 4141 4141 4141 4141 4141 4141 4141");
        assert_eq!(lines.next(), Some("0000010"));
    }

    #[test]
    fn test_duplicate_blocks_collapse_to_star() {
        let data = vec![b'A'; 32];
        let out = dump(&data, DumpFormat::Default, &HexOptions::default());
        let s = text(out);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("0000000"));
        assert_eq!(lines[1], "*");
        assert_eq!(lines[2], "0000020");
    }

    #[test]
    fn test_verbose_prints_duplicates() {
        let data = vec![b'A'; 32];
        let out = dump(&data, DumpFormat::Default, &HexOptions {
            verbose: true,
            ..HexOptions::default()
        });
        let text = text(out);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("0000010"));
    }

    #[test]
    fn test_canonical_partial_row_geometry() {
        // Byte-exact reference: hexdump -C of "ab\ncd\n".
        let out = dump(b"ab\ncd\n", DumpFormat::Canonical, &HexOptions::default());
        let s = text(out);
        let mut lines = s.lines();
        let row = lines.next().unwrap();
        assert_eq!(row.len(), 68);
        assert_eq!(&row[..28], "00000000  61 62 0a 63 64 0a ");
        assert_eq!(&row[60..], "|ab.cd.|");
        assert_eq!(lines.next(), Some("00000006"));
    }

    #[test]
    fn test_canonical_full_row_is_78_columns() {
        let data = b"AAAABBBBCCCCDDDD".repeat(1);
        let out = dump(&data, DumpFormat::Canonical, &HexOptions::default());
        let row = text(out).lines().next().unwrap().to_string();
        assert_eq!(row.len(), 78);
        assert_eq!(&row[60..], "|AAAABBBBCCCCDDDD|");
    }

    #[test]
    fn test_octal_bytes_and_chars_rows() {
        let out = dump(b"ab\n", DumpFormat::OctalBytes, &HexOptions::default());
        let row = text(out).lines().next().unwrap().to_string();
        assert_eq!(row.len(), 71);
        assert!(row.starts_with("0000000 141 142 012 "));

        let out = dump(b"ab\n", DumpFormat::Chars, &HexOptions::default());
        let row = text(out).lines().next().unwrap().to_string();
        assert!(row.starts_with("0000000   a   b  \\n "));
    }

    #[test]
    fn test_decimal_octal_hex_word_rows() {
        // "ab\n" = words 0x6261, 0x0a62... little-endian u16s:
        // 6261 then 0a62? bytes a, b, \n → u16s 6261, 0a62? No: two
        // full u16s need 4 bytes; 3 bytes → words "6261" then "0a62"?
        // b"\n" pairs with a zero pad → 0x000a.
        let out = dump(b"ab\n", DumpFormat::Decimal, &HexOptions::default());
        let row = text(out).lines().next().unwrap().to_string();
        assert!(row.starts_with("0000000   25185   00010"));

        let out = dump(b"ab\n", DumpFormat::OctalShort, &HexOptions::default());
        let row = text(out).lines().next().unwrap().to_string();
        // Words 0x6261 and 0x000a (the trailing \n pairs with a zero).
        assert!(row.starts_with("0000000  061141  000012"));

        let out = dump(b"ab\n", DumpFormat::Hex, &HexOptions::default());
        let row = text(out).lines().next().unwrap().to_string();
        assert!(row.starts_with("0000000    6261    000a"));
    }

    #[test]
    fn test_length_clamps_input() {
        let out = dump(b"abcdef", DumpFormat::Default, &HexOptions {
            length: Some(2),
            ..HexOptions::default()
        });
        let s = text(out);
        assert!(s.lines().next().unwrap().starts_with("0000000 6261 "));
        assert_eq!(s.lines().last(), Some("0000002"));
    }

    #[test]
    fn test_empty_input_no_output() {
        let out = dump(b"", DumpFormat::Default, &HexOptions::default());
        assert!(out.is_empty());
    }

    #[test]
    fn test_skip_suffixes() {
        assert_eq!(parse_skip("16").unwrap(), 16);
        assert_eq!(parse_skip("0x10").unwrap(), 16);
        assert_eq!(parse_skip("2b").unwrap(), 1024);
        assert_eq!(parse_skip("2k").unwrap(), 2048);
        assert_eq!(parse_skip("1m").unwrap(), 1048576);
        assert!(parse_skip("x").is_err());
        assert!(parse_skip("1q").is_err());
    }

    #[test]
    fn test_char_escape_table() {
        assert_eq!(char_escape(0x00), " \\0");
        assert_eq!(char_escape(0x0a), " \\n");
        assert_eq!(char_escape(b'a'), "  a");
        assert_eq!(char_escape(0x01), "001");
        assert_eq!(char_escape(0xff), "377");
    }
}
