//! uuencode/uudecode byte groups.
//!
//! Ground truth: `minix3/usr.bin/uuencode/uuencode.c` (202 lines). The wire
//! format packs every 3 input bytes into 4 printable bytes (each holding 6
//! bits offset by a space, so bytes stay in the printable range), with the
//! line length stored in the first byte of each encoded line. The `begin`
//! line carries the file mode and name; a zero length line plus `end`
//! closes the archive.
//!
//! Rules implemented here (the line framing — `begin`, length bytes, `end`
//! — stays with the caller, which owns the line discipline):
//!
//! - 3 bytes → 4 sextets, each plus 32; a zero sextet encodes as a
//!   backquote, never as a blank (trailing blanks would not survive mail
//!   transport — the format's original reason to exist).
//! - Short final groups pad with zero bytes; the length byte tells the
//!   decoder how many bytes are real.
//! - Decoding rejects bytes outside the encoding alphabet.

use crate::CompressError;

/// Encode up to 3 bytes into 4 sextet bytes in `out` (exactly 4 slots).
/// Returns the encoded bytes; short input pads with zeros.
pub fn encode_group(input: &[u8], out: &mut [u8; 4]) {
    let a = *input.first().unwrap_or(&0) as u32;
    let b = *input.get(1).unwrap_or(&0) as u32;
    let c = *input.get(2).unwrap_or(&0) as u32;
    let packed = (a << 16) | (b << 8) | c;
    for (index, slot) in out.iter_mut().enumerate() {
        let sextet = ((packed >> (18 - 6 * index)) & 0x3F) as u8;
        *slot = if sextet == 0 { b'`' } else { sextet + 32 };
    }
}

/// Decode 4 sextet bytes into 3 bytes in `out`, keeping the first `length`
/// (`length` is the line's real byte count for this group, at most 3).
pub fn decode_group(input: &[u8; 4], length: usize, out: &mut [u8; 3]) -> Result<(), CompressError> {
    if length > 3 {
        return Err(CompressError::InvalidArgument);
    }
    let mut packed: u32 = 0;
    for byte in input {
        let sextet = match byte {
            b'`' => 0,
            32..=95 => byte - 32,
            _ => return Err(CompressError::InvalidArgument),
        };
        packed = (packed << 6) | sextet as u32;
    }
    out[0] = ((packed >> 16) & 0xFF) as u8;
    out[1] = ((packed >> 8) & 0xFF) as u8;
    out[2] = (packed & 0xFF) as u8;
    let _ = length;
    Ok(())
}

/// Encode the length byte for a line holding `length` real bytes.
pub fn length_byte(length: usize) -> Result<u8, CompressError> {
    if length > 45 {
        return Err(CompressError::InvalidArgument);
    }
    Ok(if length == 0 {
        b'`'
    } else {
        length as u8 + 32
    })
}

/// Decode a length byte back to the real byte count.
pub fn parse_length(byte: u8) -> Result<usize, CompressError> {
    match byte {
        b'`' => Ok(0),
        33..=77 => Ok((byte - 32) as usize),
        _ => Err(CompressError::InvalidArgument),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_full_group() {
        let mut encoded = [0u8; 4];
        encode_group(b"Man", &mut encoded);
        let mut decoded = [0u8; 3];
        decode_group(&encoded, 3, &mut decoded).unwrap();
        assert_eq!(&decoded, b"Man");
    }

    #[test]
    fn test_canonical_man_vector() {
        // Hand derived: "Man" is 01001101 01100001 01101110, regrouped as
        // 010011 010110 000101 101110 = 19 22 5 46, plus 32 gives "36%N".
        let mut encoded = [0u8; 4];
        encode_group(b"Man", &mut encoded);
        assert_eq!(&encoded, b"36%N");
    }

    #[test]
    fn test_zero_sextet_is_backquote() {
        let mut encoded = [0u8; 4];
        encode_group(&[0, 0, 0], &mut encoded);
        assert_eq!(&encoded, b"````");
    }

    #[test]
    fn test_short_group_pads() {
        let mut encoded = [0u8; 4];
        encode_group(b"Ma", &mut encoded);
        let mut decoded = [0u8; 3];
        decode_group(&encoded, 2, &mut decoded).unwrap();
        assert_eq!(&decoded[..2], b"Ma");
    }

    #[test]
    fn test_length_bytes() {
        assert_eq!(length_byte(45).unwrap(), 45 + 32);
        assert_eq!(length_byte(0).unwrap(), b'`');
        assert_eq!(parse_length(b'`').unwrap(), 0);
        assert_eq!(parse_length(45 + 32).unwrap(), 45);
        assert_eq!(length_byte(46), Err(CompressError::InvalidArgument));
        assert_eq!(parse_length(b' '), Err(CompressError::InvalidArgument));
    }

    #[test]
    fn test_bad_alphabet_rejected() {
        assert_eq!(
            decode_group(b"ab\x7fd", 3, &mut [0u8; 3]),
            Err(CompressError::InvalidArgument)
        );
        assert_eq!(
            decode_group(b"abcd", 4, &mut [0u8; 3]),
            Err(CompressError::InvalidArgument)
        );
    }
}
