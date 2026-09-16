//! Paper tape punch and reader for `ppt`.
//!
//! Ground truth: `minix3/games/ppt/ppt.c` (NetBSD 1.19). Punching one byte
//! (`putppt`, lines 129 to 145) prints an eleven-column tape line:
//! `|`, the eight data bits from bit 7 down with `o` for a hole and blank
//! for none, a `.` feed hole inserted before bit 2, then `|` and a newline.
//! The tape is framed by `EDGE` (line 52, eleven underscores). Reading one
//! tape line back (`getppt`, lines 147 to 175) locates the `.` feed hole
//! and reassembles the byte from the eight surrounding columns: one to
//! three after it carry bits 2 down to 0, one to five before it carry bits
//! 3 up to 7; a line without a feed hole reports loss of sync. The doing
//! half lives in `src/bin/ppt.rs` (`buf[132]` line buffer, line 68).

use crate::GameError;

/// Tape edge rule (C: `EDGE`, ppt.c:52), printed with a newline around the
/// punched block.
pub const EDGE: &[u8] = b"___________";

/// Column order of the eight data bits around the feed hole: `bit_at[k]`
/// is the byte bit carried by the tape column `dot + offset[k]`.
const BIT_AT_OFFSET: [(isize, u8); 8] = [
    (-5, 0x80),
    (-4, 0x40),
    (-3, 0x20),
    (-2, 0x10),
    (-1, 0x08),
    (1, 0x04),
    (2, 0x02),
    (3, 0x01),
];

/// Renders one punched tape line for `byte` into `out`, returning the
/// written length (eleven bytes; `Err(OutOfRange)` when `out` is smaller).
///
/// Faithful to `putppt` (ppt.c:129-145), including the feed hole column
/// printed between bit 3 and bit 2.
pub fn punch_byte(byte: u8, out: &mut [u8]) -> Result<usize, GameError> {
    let mut cursor = 0usize;
    let mut put = |b: u8| -> Result<(), GameError> {
        if cursor >= out.len() {
            return Err(GameError::OutOfRange);
        }
        out[cursor] = b;
        cursor += 1;
        Ok(())
    };
    put(b'|')?;
    for bit in (0..8).rev() {
        if bit == 2 {
            put(b'.')?;
        }
        put(if byte & (1 << bit) != 0 { b'o' } else { b' ' })?;
    }
    put(b'|')?;
    put(b'\n')?;
    Ok(cursor)
}

/// Reads one punched tape line back into a byte.
///
/// Faithful to `getppt` (ppt.c:147-175): the feed hole `.` anchors the
/// eight data columns; a blank column carries no hole, anything else does.
/// `None` marks a line without a feed hole (loss of sync). Columns that
/// fall outside the line count as blank — the C version reads them in the
/// fixed 132-byte buffer, where they are always NUL.
pub fn decode_line(line: &[u8]) -> Option<u8> {
    let dot = line.iter().position(|&b| b == b'.')?;
    let at = |offset: isize| -> u8 {
        let index = dot as isize + offset;
        if index < 0 || index as usize >= line.len() {
            b' '
        } else {
            line[index as usize]
        }
    };
    let mut byte = 0u8;
    for &(offset, bit) in BIT_AT_OFFSET.iter() {
        if at(offset) != b' ' {
            byte |= bit;
        }
    }
    Some(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_punch_line_shape() {
        let mut out = [0u8; 16];
        let len = punch_byte(b'A', &mut out).unwrap();
        assert_eq!(&out[..len], b"| o   .  o|\n");
    }

    #[test]
    fn test_punch_all_zero_and_all_one() {
        let mut out = [0u8; 16];
        let len = punch_byte(0, &mut out).unwrap();
        assert_eq!(&out[..len], b"|     .   |\n");
        let len = punch_byte(0xFF, &mut out).unwrap();
        assert_eq!(&out[..len], b"|ooooo.ooo|\n");
    }

    #[test]
    fn test_punch_reports_small_output() {
        let mut tiny = [0u8; 4];
        assert_eq!(punch_byte(b'A', &mut tiny), Err(GameError::OutOfRange));
    }

    #[test]
    fn test_decode_reverses_punch() {
        for byte in [0u8, 0x01, 0x0A, b'A', b'z', 0xFF] {
            let mut out = [0u8; 16];
            let len = punch_byte(byte, &mut out).unwrap();
            assert_eq!(decode_line(&out[..len]), Some(byte));
        }
    }

    #[test]
    fn test_decode_line_without_feed_hole_is_none() {
        assert_eq!(decode_line(b"___________"), None);
        assert_eq!(decode_line(b""), None);
    }

    #[test]
    fn test_decode_treats_anything_but_blank_as_hole() {
        // The reader marks holes with 'o' but accepts any non-blank.
        assert_eq!(decode_line(b"|  @@#.  @|"), Some(0x39));
    }
}
