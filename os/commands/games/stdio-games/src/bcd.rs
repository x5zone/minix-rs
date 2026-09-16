//! Punched-card display for `bcd`.
//!
//! Ground truth: `minix3/games/bcd/bcd.c` (NetBSD 1.17). Each ASCII byte
//! carries a twelve-row hole pattern in the `holes[256]` table (lines 87 to
//! 120); a card is the 48-column frame printed by `printcard` (lines 151 to
//! 220): a top rule, one text row (the character itself when it has a hole
//! pattern, blank otherwise), twelve hole rows filling each column with the
//! row filler `"   123456789"` and marking holes as `]`, and a bottom rule.
//! Input is upper-cased and truncated at 48 columns (lines 160 to 170).
//! The doing half lives in `src/bin/bcd.rs` (`cardline[80]` input buffer,
//! line 132).

use crate::GameError;

/// Columns per card (C: `COLUMNS`, bcd.c:151).
pub const COLUMNS: usize = 48;

/// Row fillers, rows zero through eleven (C: `rowchars`, bcd.c:156).
const ROW_CHARS: [u8; 12] = *b"   123456789";

/// The punch-hole table, transcribed from bcd.c lines 87 to 120: entry
/// `i` holds twelve hole bits for byte `i` (bit 11 is the top row).
const HOLE_TABLE: [u16; 256] = [
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x206, 0x20A, 0x042, 0x442, 0x222, 0x800, 0x406,
    0x812, 0x412, 0x422, 0xA00, 0x242, 0x400, 0x842, 0x300,
    0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x008, 0x004,
    0x002, 0x001, 0x012, 0x40A, 0x80A, 0x212, 0x00A, 0x006,
    0x022, 0x900, 0x880, 0x840, 0x820, 0x810, 0x808, 0x804,
    0x802, 0x801, 0x500, 0x480, 0x440, 0x420, 0x410, 0x408,
    0x404, 0x402, 0x401, 0x280, 0x240, 0x220, 0x210, 0x208,
    0x204, 0x202, 0x201, 0x082, 0x822, 0x600, 0x282, 0x30F,
    0x900, 0x880, 0x840, 0x820, 0x810, 0x808, 0x804, 0x802,
    0x801, 0x500, 0x480, 0x440, 0x420, 0x410, 0x408, 0x404,
    0x402, 0x401, 0x280, 0x240, 0x220, 0x210, 0x208, 0x204,
    0x202, 0x201, 0x082, 0x806, 0x822, 0x600, 0x282, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
    0x206, 0x20A, 0x042, 0x442, 0x222, 0x800, 0x406, 0x812,
    0x412, 0x422, 0xA00, 0x242, 0x400, 0x842, 0x300, 0x200,
    0x100, 0x080, 0x040, 0x020, 0x010, 0x008, 0x004, 0x002,
    0x001, 0x012, 0x40A, 0x80A, 0x212, 0x00A, 0x006, 0x022,
    0x900, 0x880, 0x840, 0x820, 0x810, 0x808, 0x804, 0x802,
    0x801, 0x500, 0x480, 0x440, 0x420, 0x410, 0x408, 0x404,
    0x402, 0x401, 0x280, 0x240, 0x220, 0x210, 0x208, 0x204,
    0x202, 0x201, 0x082, 0x806, 0x822, 0x600, 0x282, 0x30F,
    0x900, 0x880, 0x840, 0x820, 0x810, 0x808, 0x804, 0x802,
    0x801, 0x500, 0x480, 0x440, 0x420, 0x410, 0x408, 0x404,
    0x402, 0x401, 0x280, 0x240, 0x220, 0x210, 0x208, 0x204,
    0x202, 0x201, 0x082, 0x806, 0x822, 0x600, 0x282, 0x0,
];

/// The twelve-row hole pattern for one input byte.
pub fn hole_mask(byte: u8) -> u16 {
    HOLE_TABLE[byte as usize]
}

/// Renders one punched card for `text` into `out`, returning the written
/// length (`Err(OutOfRange)` when `out` is smaller than the 14-line frame).
///
/// Faithful to `printcard` (bcd.c:153-220): the trailing newline is
/// dropped, the text is truncated at [`COLUMNS`] bytes and ASCII
/// lower-case letters are upper-cased; characters without a hole pattern
/// leave a blank in the text row.
pub fn render_card(text: &[u8], out: &mut [u8]) -> Result<usize, GameError> {
    let mut body = [0u8; COLUMNS];
    let mut length = 0;
    for &byte in text {
        if byte == b'\n' {
            break;
        }
        if length == COLUMNS {
            break;
        }
        body[length] = byte;
        length += 1;
    }
    for byte in &mut body[..length] {
        if byte.is_ascii_lowercase() {
            byte.make_ascii_uppercase();
        }
    }

    let mut cursor = 0usize;
    let mut put = |bytes: &[u8]| -> Result<(), GameError> {
        if cursor + bytes.len() > out.len() {
            return Err(GameError::OutOfRange);
        }
        out[cursor..cursor + bytes.len()].copy_from_slice(bytes);
        cursor += bytes.len();
        Ok(())
    };

    // Top rule (bcd.c:173-176): blank + 48 underscores + newline.
    put(b" ")?;
    for _ in 0..COLUMNS {
        put(b"_")?;
    }
    put(b"\n")?;

    // Text row (bcd.c:183-192): "/" + character or blank + filler + "|".
    put(b"/")?;
    for &byte in &body[..length] {
        if hole_mask(byte) != 0 {
            put(&[byte])?;
        } else {
            put(b" ")?;
        }
    }
    for _ in length..COLUMNS {
        put(b" ")?;
    }
    put(b"|\n")?;

    // Twelve hole rows (bcd.c:200-212): holes as "]", filler otherwise.
    for row in 0..12u32 {
        put(b"|")?;
        for &byte in &body[..length] {
            if hole_mask(byte) & (1 << (11 - row)) != 0 {
                put(b"]")?;
            } else {
                put(&[ROW_CHARS[row as usize]])?;
            }
        }
        for _ in length..COLUMNS {
            put(&[ROW_CHARS[row as usize]])?;
        }
        put(b"|\n")?;
    }

    // Bottom rule (bcd.c:215-219).
    put(b"|")?;
    for _ in 0..COLUMNS {
        put(b"_")?;
    }
    put(b"|\n")?;

    Ok(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(text: &[u8]) -> String {
        let mut out = [0u8; 800];
        let len = render_card(text, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_frame_geometry() {
        let card = frame(b"A");
        let lines: Vec<&str> = card.split('\n').collect();
        // 14 lines (top rule, text row, twelve hole rows, bottom rule)
        // plus the empty trailing artifact of the final newline.
        assert_eq!(lines.len(), 16);
        assert_eq!(lines[0], format!(" {}", "_".repeat(COLUMNS)));
        assert_eq!(lines[1].len(), COLUMNS + 2);
        assert!(lines[1].starts_with("/A"));
        assert!(lines[1].ends_with('|'));
        assert_eq!(lines[14], format!("|{}|", "_".repeat(COLUMNS)));
    }

    #[test]
    fn test_hole_marks_against_the_table() {
        // '1' carries the single hole bit 0x080 (bcd.c line 94, row of
        // index 49), so exactly one of the twelve rows shows "]".
        let card = frame(b"1");
        let lines: Vec<&str> = card.split('\n').collect();
        let mut hole_rows = 0;
        for line in &lines[2..14] {
            assert_eq!(line.len(), COLUMNS + 2);
            if line.contains(']') {
                hole_rows += 1;
            }
        }
        assert_eq!(hole_rows, 1);
    }

    #[test]
    fn test_lowercase_becomes_uppercase() {
        // The table genuinely differs for the two cases (holes['A'] is
        // 0x900, holes['a'] 0x880), so the equality that matters is in the
        // rendered card: the text row shows the upper-cased letter.
        let card = frame(b"a");
        assert!(card.contains("/A"));
    }

    #[test]
    fn test_truncation_at_48_columns() {
        let long = [b'x'; 60];
        let card = frame(&long);
        // Same frame for 60 and 48 input bytes: everything past 48 is cut.
        assert_eq!(card, frame(&long[..48]));
    }

    #[test]
    fn test_trailing_newline_is_dropped() {
        assert_eq!(frame(b"HI\n"), frame(b"HI"));
    }

    #[test]
    fn test_newline_inside_text_terminates_the_line() {
        let card = frame(b"AB\nCD");
        assert!(card.contains("/AB"));
        assert!(!card.contains("CD"));
    }

    #[test]
    fn test_oversized_output_reports_out_of_range() {
        let mut tiny = [0u8; 16];
        assert_eq!(render_card(b"A", &mut tiny), Err(GameError::OutOfRange));
    }
}
