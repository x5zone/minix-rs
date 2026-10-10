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
use alloc::vec::Vec;

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

/// Encode a whole input into the classic `uuencode` body: each physical line
/// carries the byte count as a [`length_byte`], then one [`encode_group`]
/// (4 sextets) per 3-byte group, terminated by a newline; a final zero-length
/// line (a lone backquote) closes the body. This mirrors `encode()` in
/// `minix3/usr.bin/uuencode/uuencode.c:156-193`; the `begin`/`end` framing and
/// the file mode stay with the caller.
///
/// A short trailing group is padded with zero sextets (rendered backquotes).
/// C leaves stale bytes in its reused buffer there, but the length byte tells
/// the decoder to drop them, so the decoded stream is identical either way.
pub fn encode_stream(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in input.chunks(45) {
        out.push(length_byte(chunk.len()).unwrap_or(b'`'));
        for group in chunk.chunks(3) {
            let mut encoded = [0u8; 4];
            encode_group(group, &mut encoded);
            out.extend_from_slice(&encoded);
        }
        out.push(b'\n');
    }
    out.push(b'`');
    out.push(b'\n');
    out
}

/// Encode a whole input into the `uuencode -m` base64 body (`
/// base64_encode()` at `uuencode.c:128-151`): standard `A-Za-z0-9+/` with
/// `=` padding, four characters per 3-byte group, wrapped after every 16
/// groups (64 characters). Empty input yields an empty body, matching C (the
/// read loop never runs and `sequence % GROUPS` is zero, so no newline).
pub fn base64_stream(input: &[u8]) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // GROUPS from uuencode.c:134 = ((70 / 4) - 1) = 16 quads per line.
    const GROUPS: usize = 16;
    let mut out = Vec::new();
    let mut sequence = 0usize;
    for chunk in input.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let packed = (a << 16) | (b << 8) | c;
        let sextet = |index: usize| ALPHABET[((packed >> (18 - 6 * index)) & 0x3F) as usize];
        out.push(sextet(0));
        out.push(sextet(1));
        out.push(if chunk.len() >= 2 { sextet(2) } else { b'=' });
        out.push(if chunk.len() >= 3 { sextet(3) } else { b'=' });
        sequence += 1;
        if sequence.is_multiple_of(GROUPS) {
            out.push(b'\n');
        }
    }
    if !sequence.is_multiple_of(GROUPS) {
        out.push(b'\n');
    }
    out
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

    #[test]
    fn test_encode_stream_canonical_man() {
        // "Man" -> length 3 ('#') + "36%N" + newline, then the closing "`\n".
        assert_eq!(encode_stream(b"Man"), b"#36%N\n`\n".to_vec());
    }

    #[test]
    fn test_encode_stream_single_byte_pads() {
        // 1 byte -> length '!' (33), group "M\0\0" -> sextets 19,16,0,0 = "30``".
        assert_eq!(encode_stream(b"M"), b"!30``\n`\n".to_vec());
    }

    #[test]
    fn test_encode_stream_empty_is_backquote_line() {
        assert_eq!(encode_stream(b""), b"`\n".to_vec());
    }

    #[test]
    fn test_encode_stream_splits_at_45() {
        // 46 bytes -> a full 45-byte line (length 'm' = 77) then a 1-byte line.
        let out = encode_stream(&[b'a'; 46]);
        assert_eq!(out[0], 45 + 32); // 'm'
        assert_eq!(out[61], b'\n'); // end of the 45-byte line (1 + 60 + 1)
        assert_eq!(out[62], 1 + 32); // '!' length of the leftover 1 byte
    }

    #[test]
    fn test_base64_stream_canonical() {
        assert_eq!(base64_stream(b"Man"), b"TWFu\n".to_vec());
        assert_eq!(base64_stream(b"M"), b"TQ==\n".to_vec());
        assert_eq!(base64_stream(b"Ma"), b"TWE=\n".to_vec());
    }

    #[test]
    fn test_base64_stream_empty_has_no_newline() {
        assert!(base64_stream(b"").is_empty());
    }

    #[test]
    fn test_base64_stream_wraps_at_16_groups() {
        // 48 bytes = 16 quads = 64 chars, then a newline (no extra final one).
        let out = base64_stream(&[b'a'; 48]);
        assert_eq!(out.len(), 65);
        assert_eq!(out[64], b'\n');
        // 49 bytes = 17 quads: 64 + '\n' + 4 + '\n'.
        let out = base64_stream(&[b'a'; 49]);
        assert_eq!(out.len(), 70);
        assert_eq!(out[64], b'\n');
        assert_eq!(out[69], b'\n');
    }
}
