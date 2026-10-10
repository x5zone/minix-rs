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

/// The parsed header of a uu/base64 archive (`begin`/`begin-base64` line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UuBegin {
    /// The octal mode after the keyword.
    pub mode: u32,
    /// The embedded output file name (bytes, newline already cut).
    pub name: Vec<u8>,
    /// `true` when the header was `begin-base64` (base64 body follows).
    pub base64: bool,
}

/// Whitespace that separates the mode from the file name (`isspace` subset).
fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c)
}

/// Parse a `begin`/`begin-base64` header line (`uudecode.c:118-150`): detect
/// the keyword, read the octal mode (strtol base 8 — skip leading whitespace,
/// then octal digits), require a whitespace separator, then take the file name
/// up to the line's newline. Returns `None` when the line is not a begin line
/// or is missing the mode or name, which is what C reports as an error.
pub fn parse_begin(line: &[u8]) -> Option<UuBegin> {
    let (base64, mut i) = if line.starts_with(b"begin-base64 ") {
        (true, 13usize)
    } else if line.starts_with(b"begin ") {
        (false, 6usize)
    } else {
        return None;
    };
    while i < line.len() && is_ws(line[i]) {
        i += 1;
    }
    let start = i;
    while i < line.len() && (b'0'..=b'7').contains(&line[i]) {
        i += 1;
    }
    if i == start {
        return None; // no mode digits (C: fn == p)
    }
    if i >= line.len() || !is_ws(line[i]) {
        return None; // mode not followed by whitespace
    }
    let mut mode: u32 = 0;
    for &d in &line[start..i] {
        mode = mode * 8 + (d - b'0') as u32;
    }
    while i < line.len() && is_ws(line[i]) {
        i += 1;
    }
    if i >= line.len() {
        return None; // no file name
    }
    let end = line[i..].iter().position(|&b| b == b'\n').map(|p| i + p).unwrap_or(line.len());
    let name = line[i..end].to_vec();
    if name.is_empty() {
        return None;
    }
    Some(UuBegin { mode, name, base64 })
}

/// The C `DEC` macro (`uudecode.c:237`): `((c - ' ') & 077)`, masking so any
/// byte lands in 0..=63 (never negative), matching C's silent masking rather
/// than rejecting off-alphabet bytes.
fn dec(c: u8) -> i32 {
    ((c as i32) - 32) & 0o77
}

/// Decode the uu body lines (`uudecode.c:231-267`): each line's first byte is a
/// `DEC`-masked length; 0 ends the data; otherwise ceil(length/3) four-char
/// groups yield the bytes, and only `length` of them are real (the rest is
/// padding). The caller supplies the body lines (framing — the `begin` header
/// and the trailing `end` line — stays with the caller).
pub fn decode_uu_stream(lines: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let n = dec(line[0]) as usize;
        if n == 0 {
            break; // zero-length line ends the archive
        }
        let mut rest = n;
        let mut p = &line[1..];
        while rest > 0 {
            let g0 = dec(*p.first().unwrap_or(&b'`'));
            let g1 = dec(*p.get(1).unwrap_or(&b'`'));
            let g2 = dec(*p.get(2).unwrap_or(&b'`'));
            let g3 = dec(*p.get(3).unwrap_or(&b'`'));
            let b0 = ((g0 << 2) | (g1 >> 4)) as u8;
            let b1 = ((g1 << 4) | (g2 >> 2)) as u8;
            let b2 = ((g2 << 6) | g3) as u8;
            out.push(b0);
            if rest >= 2 {
                out.push(b1);
            }
            if rest >= 3 {
                out.push(b2);
            }
            if p.len() >= 4 {
                p = &p[4..];
            } else {
                break;
            }
            rest = rest.saturating_sub(3);
        }
    }
    out
}

/// Decode a base64 sextet char to its 6-bit value (the inverse of the
/// `base64_stream` alphabet).
fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode the base64 body (`uudecode.c:290-314`): collect sextets across the
/// body lines, stopping at the first `=` (final-quantum padding or the `====`
/// terminator); whitespace is skipped; a non-alphabet byte also stops. Then
/// regroup 4 sextets → up to 3 bytes, dropping the padding-only tail bytes.
pub fn decode_base64_stream(lines: &[&[u8]]) -> Vec<u8> {
    let mut sextets: Vec<u8> = Vec::new();
    let mut stop = false;
    for line in lines {
        if stop {
            break;
        }
        for &c in *line {
            if c == b'=' {
                stop = true;
                break;
            }
            match b64_val(c) {
                Some(v) => sextets.push(v),
                None if c == b' ' || c == b'\t' || c == b'\r' => {}
                None => {
                    stop = true;
                    break;
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < sextets.len() {
        let take = (sextets.len() - i).min(4);
        let mut packed: u32 = 0;
        for k in 0..take {
            packed = (packed << 6) | sextets[i + k] as u32;
        }
        packed <<= (4 - take) * 6;
        let bytes = take * 6 / 8;
        out.push((packed >> 16) as u8);
        if bytes >= 2 {
            out.push((packed >> 8) as u8);
        }
        if bytes >= 3 {
            out.push(packed as u8);
        }
        i += 4;
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

    /// Split an encoder body into non-empty lines for the decoders.
    fn body_lines(enc: &[u8]) -> Vec<&[u8]> {
        enc.split(|&b| b == b'\n').filter(|l| !l.is_empty()).collect()
    }

    #[test]
    fn test_parse_begin_uu() {
        let b = parse_begin(b"begin 644 foo.txt").unwrap();
        assert_eq!(b.mode, 0o644);
        assert_eq!(b.name, b"foo.txt");
        assert!(!b.base64);
    }

    #[test]
    fn test_parse_begin_base64_and_spaced_name() {
        let b = parse_begin(b"begin-base64 755 my file.bin").unwrap();
        assert_eq!(b.mode, 0o755);
        assert_eq!(b.name, b"my file.bin");
        assert!(b.base64);
    }

    #[test]
    fn test_parse_begin_rejects_bad_headers() {
        assert!(parse_begin(b"not a begin line").is_none());
        assert!(parse_begin(b"begin 644").is_none()); // no file name
        assert!(parse_begin(b"begin abc foo").is_none()); // mode not octal-digits
    }

    #[test]
    fn test_decode_uu_canonical_man() {
        assert_eq!(decode_uu_stream(&[b"#36%N".as_ref(), b"`".as_ref()]), b"Man");
    }

    #[test]
    fn test_decode_uu_round_trips() {
        for data in [&b""[..], &b"M"[..], &b"Ma"[..], &b"Man"[..], &b"Hello, World! 234"[..], &[b'x'; 100][..]] {
            let enc = encode_stream(data);
            assert_eq!(decode_uu_stream(&body_lines(&enc)), data, "uu round trip");
        }
    }

    #[test]
    fn test_decode_base64_canonical() {
        assert_eq!(decode_base64_stream(&[b"TWFu".as_ref()]), b"Man");
        assert_eq!(decode_base64_stream(&[b"TQ==".as_ref()]), b"M");
        assert_eq!(decode_base64_stream(&[b"TWE=".as_ref()]), b"Ma");
    }

    #[test]
    fn test_decode_base64_round_trips() {
        for data in [&b""[..], &b"M"[..], &b"Ma"[..], &b"Man"[..], &b"the quick brown fox"[..], &[b'z'; 200][..]] {
            let enc = base64_stream(data);
            assert_eq!(decode_base64_stream(&body_lines(&enc)), data, "base64 round trip");
        }
    }
}
