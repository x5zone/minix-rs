//! Random filtering: one over denominator selection.
//!
//! Ground truth: `minix3/games/random/random.c`. Seeding mixes the clock with
//! the process identifier (near line 107). Filtering prints each input line
//! with probability one over the denominator (near line 126: selected when a
//! draw below the denominator equals zero); exit status mode returns a value
//! between zero and the denominator minus one. Drawing stays with the
//! execution layer; this module owns the denominator rule.

use crate::TextGameError;

/// Parse a denominator (strictly positive).
pub fn parse_denominator(word: &str) -> Result<u32, TextGameError> {
    if word.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(TextGameError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(TextGameError::OutOfRange)?;
    }
    if value == 0 {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(value)
}

/// True when a uniform `draw` below `denominator` selects the line (the draw
/// equals zero, which happens with probability one over the denominator).
pub fn is_selected(draw: u32, denominator: u32) -> Result<bool, TextGameError> {
    if denominator == 0 || draw >= denominator {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(draw == 0)
}

/// Mix a clock reading with a process identifier into a seed (mirrors the
/// seeding blend of time plus process in the original source).
pub fn mix_seed(seconds: u64, micros: u32, pid: u32) -> u64 {
    seconds
        .wrapping_add(micros as u64)
        .wrapping_add(pid as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_denominators_parse() {
        assert_eq!(parse_denominator("6"), Ok(6));
        assert_eq!(parse_denominator("0"), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_denominator(""), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_denominator("6x"), Err(TextGameError::InvalidArgument));
    }

    #[test]
    fn test_selection_rule() {
        assert!(is_selected(0, 6).unwrap());
        assert!(!is_selected(5, 6).unwrap());
        assert_eq!(is_selected(6, 6).map(|_| ()), Err(TextGameError::InvalidArgument));
        assert_eq!(is_selected(0, 0).map(|_| ()), Err(TextGameError::InvalidArgument));
    }

    #[test]
    fn test_seed_mixes() {
        assert_ne!(mix_seed(100, 5, 7), mix_seed(100, 5, 8));
        assert_ne!(mix_seed(100, 5, 7), mix_seed(101, 5, 7));
    }
}
