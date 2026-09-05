//! Lempel-Ziv-Welch compression with clear codes.
//!
//! Ground truth: `minix3/minix/commands/compress/compress.c` (1618 lines:
//! "Modified Lempel-Ziv encoding", options `-d`/`-f`/`-v`/`-c`/`-b`
//! documented in its header comment at lines 20 to 42). The algorithm
//! essence kept here: byte alphabet (codes 0 to 255), `CLEAR` (256)
//! resetting the dictionary, end of information (257), first free code
//! 258, most significant bit first variable width codes starting at 9
//! bits and growing to the chosen maximum.
//!
//! The container framing is this crate's own (one leading byte naming the
//! maximum width, then the code stream): it is NOT byte compatible with
//! `.Z` files, whose headers, block modes, and bit order quirks belong to
//! a file format stage. The code assignment and clear discipline match the
//! C tool, so the compression behaviour (ratios, clear points) transfers;
//! only the wrapping differs. That split is recorded as an architecture
//! note in the stage document.
//!
//! Bounds: maximum width 9 to 12 (4096 codes); 12 is the default. Tables
//! are fixed arrays sized for 12 bits regardless of the chosen width.

use crate::CompressError;

/// Dictionary reset code.
pub const CLEAR: u16 = 256;
/// End of information code.
pub const END: u16 = 257;
/// First code assigned to a new string.
pub const FIRST_FREE: u16 = 258;
/// Largest maximum width supported (4096 codes).
pub const MAX_MAXBITS: u8 = 12;
/// Default maximum width.
pub const DEFAULT_MAXBITS: u8 = 12;

/// Encoder hash table slots (a prime above twice the code space, so open
/// addressing stays sparse even when full).
const HASH_SIZE: usize = 8191;
/// Empty hash slot marker (no valid key reaches it: keys pack prefix and
/// suffix below bit 20).
const HASH_EMPTY: u32 = 0xFFFF_FFFF;

/// Compress `input` into `out`, returning the used byte count.
///
/// The first output byte names the maximum width; the rest is the code
/// stream. The output must be generous (incompressible input grows slightly
/// through framing): callers size it at twice the input plus a margin, and
/// `TooLong` reports a short buffer instead of truncating.
pub fn compress(input: &[u8], maxbits: u8, out: &mut [u8]) -> Result<usize, CompressError> {
    if !(9..=MAX_MAXBITS).contains(&maxbits) {
        return Err(CompressError::InvalidArgument);
    }
    if out.is_empty() {
        return Err(CompressError::TooLong);
    }
    out[0] = maxbits;
    let mut writer = BitWriter::new(&mut out[1..]);
    let mut keys = [HASH_EMPTY; HASH_SIZE];
    let mut codes = [0u16; HASH_SIZE];
    let mut nbits = 9u32;
    let maxcode = 1u32 << maxbits;
    let mut free = FIRST_FREE as u32;
    // Block mode opens with a clear, like the C tool.
    writer.put(CLEAR as u32, nbits)?;
    let mut iter = input.iter();
    let Some(&first) = iter.next() else {
        writer.put(END as u32, nbits)?;
        return Ok(1 + writer.finish()?);
    };
    let mut prefix = first as u32;
    for byte in iter {
        let suffix = *byte as u32;
        let key = (prefix << 8) | suffix;
        match hash_find(&keys, &codes, key) {
            Some(code) => prefix = code,
            None => {
                writer.put(prefix, nbits)?;
                if free < maxcode {
                    hash_insert(&mut keys, &mut codes, key, free as u16);
                    free += 1;
                    if free == (1 << nbits) && nbits < maxbits as u32 {
                        nbits += 1;
                    }
                } else {
                    // Dictionary full: clear and restart (block mode).
                    writer.put(CLEAR as u32, nbits)?;
                    keys.fill(HASH_EMPTY);
                    free = FIRST_FREE as u32;
                    nbits = 9;
                }
                prefix = suffix;
            }
        }
    }
    writer.put(prefix, nbits)?;
    writer.put(END as u32, nbits)?;
    Ok(1 + writer.finish()?)
}

/// Decompress a stream produced by [`compress`] into `out`, returning the
/// used byte count. A short buffer reports `TooLong`; trailing bits after
/// the end code are ignored (encoders pad the last byte).
pub fn decompress(input: &[u8], out: &mut [u8]) -> Result<usize, CompressError> {
    let (maxbits, mut reader) = match input.split_first() {
        Some((&maxbits, rest)) if (9..=MAX_MAXBITS).contains(&maxbits) => {
            (maxbits, BitReader::new(rest))
        }
        _ => return Err(CompressError::InvalidArgument),
    };
    let maxcode = 1u32 << maxbits;
    let mut prefix = [0u16; 4096];
    let mut suffix = [0u8; 4096];
    for code in 0..256 {
        suffix[code] = code as u8;
    }
    let mut nbits = 9u32;
    let mut free = FIRST_FREE as u32;
    let mut written = 0;
    // The stream must open with a clear (our encoder always emits one).
    if reader.get(nbits)? != CLEAR as u32 {
        return Err(CompressError::InvalidArgument);
    }
    let mut previous: Option<u32> = None;
    loop {
        let code = reader.get(nbits)?;
        if code == END as u32 {
            break;
        }
        if code == CLEAR as u32 {
            free = FIRST_FREE as u32;
            nbits = 9;
            previous = None;
            continue;
        }
        // Resolve the code to its byte string. Two shapes: a known code
        // replays its string; the next free code is legal exactly once —
        // right after the previous string, meaning previous plus its own
        // first byte (the KwKwK case). Anything else is corruption.
        let first = if code < free {
            resolve(code, &prefix, &suffix)?
        } else if code == free && previous.is_some() {
            let prev = previous.ok_or(CompressError::InvalidArgument)?;
            let byte = first_byte(prev, &prefix, &suffix)?;
            written = emit_string(prev, &prefix, &suffix, out, written)?;
            if written >= out.len() {
                return Err(CompressError::TooLong);
            }
            out[written] = byte;
            written += 1;
            byte
        } else {
            return Err(CompressError::InvalidArgument);
        };
        if code < free {
            written = emit_string(code, &prefix, &suffix, out, written)?;
        }
        if let Some(prev) = previous
            && free < maxcode
        {
            prefix[free as usize] = prev as u16;
            suffix[free as usize] = first;
            free += 1;
            // Width step: the decoder files each pair one code later
            // than the encoder (when its second byte is read, not when
            // emitted), so its table lags by exactly one entry. To
            // switch widths on the same stream code, step at 2^n - 1
            // instead of 2^n. Monotonic filling hits each level once
            // per clear epoch.
            if nbits < maxbits as u32 && free == (1 << nbits) - 1 {
                nbits += 1;
            }
        }
        previous = Some(code);
    }
    Ok(written)
}

/// First byte of the string `code` names.
fn first_byte(code: u32, prefix: &[u16; 4096], suffix: &[u8; 4096]) -> Result<u8, CompressError> {
    let mut code = code;
    while code >= 256 {
        code = prefix[code as usize] as u32;
    }
    Ok(suffix[code as usize])
}

/// Validate that `code` names a complete string (all links resolve).
fn resolve(code: u32, prefix: &[u16; 4096], suffix: &[u8; 4096]) -> Result<u8, CompressError> {
    if code >= 4096 {
        return Err(CompressError::InvalidArgument);
    }
    first_byte(code, prefix, suffix)
}

/// Emit the string `code` names into `out`, returning the new cursor.
fn emit_string(
    code: u32,
    prefix: &[u16; 4096],
    suffix: &[u8; 4096],
    out: &mut [u8],
    mut written: usize,
) -> Result<usize, CompressError> {
    // Collect backwards into a scratch run, then copy forward.
    let mut run = [0u8; 4096];
    let mut len = 0;
    let mut code = code;
    while code >= 256 {
        if len >= run.len() {
            return Err(CompressError::InvalidArgument);
        }
        run[len] = suffix[code as usize];
        len += 1;
        code = prefix[code as usize] as u32;
    }
    if len >= run.len() {
        return Err(CompressError::InvalidArgument);
    }
    run[len] = suffix[code as usize];
    len += 1;
    if written + len > out.len() {
        return Err(CompressError::TooLong);
    }
    for index in 0..len {
        out[written + index] = run[len - 1 - index];
    }
    written += len;
    Ok(written)
}

/// Most significant bit first bit writer over a byte slice.
struct BitWriter<'a> {
    out: &'a mut [u8],
    used: usize,
    buffer: u32,
    held: u32,
}

impl<'a> BitWriter<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        BitWriter {
            out,
            used: 0,
            buffer: 0,
            held: 0,
        }
    }

    fn put(&mut self, code: u32, nbits: u32) -> Result<(), CompressError> {
        self.buffer = (self.buffer << nbits) | (code & ((1 << nbits) - 1));
        self.held += nbits;
        while self.held >= 8 {
            if self.used >= self.out.len() {
                return Err(CompressError::TooLong);
            }
            self.held -= 8;
            self.out[self.used] = (self.buffer >> self.held) as u8;
            self.used += 1;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<usize, CompressError> {
        if self.held > 0 {
            if self.used >= self.out.len() {
                return Err(CompressError::TooLong);
            }
            self.out[self.used] = (self.buffer << (8 - self.held)) as u8;
            self.used += 1;
        }
        Ok(self.used)
    }
}

/// Most significant bit first bit reader over a byte slice.
struct BitReader<'a> {
    input: &'a [u8],
    pos: usize,
    buffer: u32,
    held: u32,
}

impl<'a> BitReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        BitReader {
            input,
            pos: 0,
            buffer: 0,
            held: 0,
        }
    }

    fn get(&mut self, nbits: u32) -> Result<u32, CompressError> {
        while self.held < nbits {
            let byte = *self.input.get(self.pos).ok_or(CompressError::InvalidArgument)?;
            self.pos += 1;
            self.buffer = (self.buffer << 8) | byte as u32;
            self.held += 8;
        }
        self.held -= nbits;
        Ok((self.buffer >> self.held) & ((1 << nbits) - 1))
    }
}

fn hash_mix(key: u32) -> usize {
    ((key ^ (key >> 7)).wrapping_mul(0x9E37) as usize) % HASH_SIZE
}

fn hash_find(keys: &[u32; HASH_SIZE], codes: &[u16; HASH_SIZE], key: u32) -> Option<u32> {
    let mut slot = hash_mix(key);
    loop {
        if keys[slot] == HASH_EMPTY {
            return None;
        }
        if keys[slot] == key {
            return Some(codes[slot] as u32);
        }
        slot = (slot + 1) % HASH_SIZE;
    }
}

fn hash_insert(keys: &mut [u32; HASH_SIZE], codes: &mut [u16; HASH_SIZE], key: u32, code: u16) {
    let mut slot = hash_mix(key);
    loop {
        if keys[slot] == HASH_EMPTY {
            keys[slot] = key;
            codes[slot] = code;
            return;
        }
        // Duplicate inserts never happen (callers check first); still,
        // overwrite instead of looping forever if they do.
        if keys[slot] == key {
            codes[slot] = code;
            return;
        }
        slot = (slot + 1) % HASH_SIZE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(input: &[u8], maxbits: u8) -> Vec<u8> {
        let mut packed = [0u8; 8192];
        let len = compress(input, maxbits, &mut packed).unwrap();
        let mut plain = [0u8; 8192];
        let out = decompress(&packed[..len], &mut plain).unwrap();
        plain[..out].to_vec()
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(round_trip(b"", 12), b"");
    }

    #[test]
    fn test_single_byte() {
        assert_eq!(round_trip(b"A", 12), b"A");
    }

    #[test]
    fn test_text_round_trip() {
        let text = b"TOBEORNOTTOBEORTOBEORNOTTOBEORNOT";
        assert_eq!(round_trip(text, 12), text);
    }

    #[test]
    fn test_all_bytes_round_trip() {
        let all: Vec<u8> = (0..=255u16).map(|b| b as u8).collect();
        assert_eq!(round_trip(&all, 12), all);
    }

    #[test]
    fn test_repetitive_input_compresses() {
        let input = [b'a'; 1000];
        let mut packed = [0u8; 8192];
        let len = compress(&input, 12, &mut packed).unwrap();
        assert!(len < 1000, "1000 a's packed into {len} bytes");
        assert_eq!(round_trip(&input, 12), input);
    }

    #[test]
    fn test_small_width_forces_clear() {
        // Nine bits fill after 512 codes: pseudo random bytes force
        // several clear cycles before the stream ends.
        let mut input = [0u8; 3000];
        let mut state = 0x1234_5678u32;
        for slot in input.iter_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *slot = (state >> 24) as u8;
        }
        assert_eq!(round_trip(&input, 9), input);
    }

    #[test]
    fn test_bad_maxbits_rejected() {
        let mut out = [0u8; 16];
        assert_eq!(
            compress(b"a", 8, &mut out),
            Err(CompressError::InvalidArgument)
        );
        assert_eq!(
            compress(b"a", 13, &mut out),
            Err(CompressError::InvalidArgument)
        );
    }

    #[test]
    fn test_garbage_rejected() {
        let mut out = [0u8; 16];
        assert_eq!(
            decompress(&[12, 0xFF, 0xFF, 0xFF], &mut out).map(|_| ()),
            Err(CompressError::InvalidArgument)
        );
        assert_eq!(
            decompress(&[7, 0], &mut out).map(|_| ()),
            Err(CompressError::InvalidArgument)
        );
    }

    #[test]
    fn test_pseudorandom_round_trips() {
        // Deterministic generator (no test dependency on randomness):
        // mixed runs and noise cross width growth and clear points at
        // every supported maximum width.
        for maxbits in 9..=12u8 {
            let mut state = 0x9E37_79B9u32;
            let mut input = [0u8; 4000];
            for slot in input.iter_mut() {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                // Mostly a tiny alphabet (runs and matches), sometimes a
                // wild byte: exercises matches, clears, and growth.
                *slot = if state & 7 == 0 {
                    (state >> 24) as u8
                } else {
                    (state & 3) as u8
                };
            }
            assert_eq!(round_trip(&input, maxbits), input, "width {maxbits}");
        }
    }
}
