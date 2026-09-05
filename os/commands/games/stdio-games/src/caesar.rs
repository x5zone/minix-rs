//! Rotation cipher.
//!
//! Ground truth: `minix3/games/caesar/caesar.c`. The rotation table builder
//! sits near line 82: the rotation folds with `rot %= LETTERS` (near line 86,
//! overflow guard), non letters pass through unchanged, upper and lower case
//! shift within their own ranges. Rotation parsing sits near line 125 with an
//! overflow check. A guessing mode exists; this module owns the table and the
//! rotation, guessing stays with later work.

use crate::GameError;

/// Letters in the Latin alphabet.
pub const LETTERS: u32 = 26;

/// Parse a rotation amount (decimal, zero through the integer ceiling).
pub fn parse_rotation(word: &str) -> Result<u32, GameError> {
    if word.is_empty() {
        return Err(GameError::InvalidArgument);
    }
    let mut value: u64 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(GameError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u64))
            .ok_or(GameError::OutOfRange)?;
    }
    if value > u32::MAX as u64 {
        return Err(GameError::OutOfRange);
    }
    Ok(value as u32)
}

/// Rotate one byte by `rot` positions (upper and lower case wrap within
/// their ranges, every other byte passes through unchanged).
pub fn rotate_byte(byte: u8, rot: u32) -> u8 {
    let shift = (rot % LETTERS) as u8;
    if byte.is_ascii_uppercase() {
        b'A' + (byte - b'A' + shift) % 26
    } else if byte.is_ascii_lowercase() {
        b'a' + (byte - b'a' + shift) % 26
    } else {
        byte
    }
}

/// Rotate `input` into `out` (lengths must match).
pub fn rotate_line(input: &[u8], rot: u32, out: &mut [u8]) -> Result<(), GameError> {
    if input.len() != out.len() {
        return Err(GameError::InvalidArgument);
    }
    for (index, byte) in input.iter().enumerate() {
        out[index] = rotate_byte(*byte, rot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotated(text: &str, rot: u32) -> String {
        let mut out = vec![0u8; text.len()];
        rotate_line(text.as_bytes(), rot, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn test_rot_thirteen_round_trip() {
        assert_eq!(rotated("Hello, World!", 13), "Uryyb, Jbeyq!");
        assert_eq!(rotated("Uryyb, Jbeyq!", 13), "Hello, World!");
    }

    #[test]
    fn test_zero_and_full_turns() {
        assert_eq!(rotated("Abc", 0), "Abc");
        assert_eq!(rotated("Abc", 26), "Abc");
        assert_eq!(rotated("Abc", 52), "Abc");
    }

    #[test]
    fn test_non_letters_pass_through() {
        assert_eq!(rotated("123 !?", 5), "123 !?");
    }

    #[test]
    fn test_rotations_parse() {
        assert_eq!(parse_rotation("13"), Ok(13));
        assert_eq!(parse_rotation(""), Err(GameError::InvalidArgument));
        assert_eq!(parse_rotation("-1"), Err(GameError::InvalidArgument));
        assert_eq!(parse_rotation("99999999999"), Err(GameError::OutOfRange));
    }

    #[test]
    fn test_length_mismatch_rejected() {
        let mut out = [0u8; 2];
        assert_eq!(
            rotate_line(b"abc", 1, &mut out),
            Err(GameError::InvalidArgument)
        );
    }
}
