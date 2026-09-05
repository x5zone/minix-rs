//! Number factoring.
//!
//! Ground truth: `minix3/games/factor/factor.c`. Output shape is
//! `number: factor1 factor1 factor2 ...` with factors in non decreasing order
//! (header comment near line 59). Small values divide out against a prime
//! table (the factor printing function near line 184); large values take the
//! probabilistic path (near line 110). Usage is `factor [value ...]` (near
//! line 268); with no arguments numbers come from the standard input stream.
//! This module owns decimal parsing, trial division, and line rendering; the
//! probabilistic path stays with later work.

use crate::GameError;

/// Small primes for the trial division wheel (all primes below 100).
pub const SMALL_PRIMES: &[u64] = &[
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73,
    79, 83, 89, 97,
];

/// Parse a decimal value (digits only, no sign, no blanks).
pub fn parse_value(word: &str) -> Result<u64, GameError> {
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
    Ok(value)
}

/// Divide out factors of `value` into `out` in non decreasing order.
///
/// Trial division runs over the small prime table first, then continues with
/// the six step wheel (candidates congruent to 1 and 5 modulo 6) up to the
/// square root of the remainder. The remainder above one is prime. Returns
/// the factor count. An input below two yields zero factors.
pub fn trial_divide(mut value: u64, out: &mut [u64]) -> Result<usize, GameError> {
    if value < 2 {
        return Ok(0);
    }
    let mut count = 0;
    for prime in SMALL_PRIMES {
        while value.is_multiple_of(*prime) {
            if count >= out.len() {
                return Err(GameError::OutOfRange);
            }
            out[count] = *prime;
            count += 1;
            value /= prime;
        }
        if value == 1 {
            return Ok(count);
        }
    }
    let mut candidate: u64 = 101;
    while candidate <= value / candidate {
        if value.is_multiple_of(candidate) {
            if count >= out.len() {
                return Err(GameError::OutOfRange);
            }
            out[count] = candidate;
            count += 1;
            value /= candidate;
        } else {
            // Six step wheel: after 101 (2 mod 6) alternate +4, +2.
            candidate += if candidate % 6 == 5 { 2 } else { 4 };
        }
    }
    if value > 1 {
        if count >= out.len() {
            return Err(GameError::OutOfRange);
        }
        out[count] = value;
        count += 1;
    }
    Ok(count)
}

/// Render one output line (`number: factor ...`) into `out`.
pub fn render_factors(number: u64, factors: &[u64], out: &mut [u8]) -> Result<usize, GameError> {
    let mut cursor = 0;
    emit_number(out, &mut cursor, number)?;
    emit_byte(out, &mut cursor, b':')?;
    for factor in factors {
        emit_byte(out, &mut cursor, b' ')?;
        emit_number(out, &mut cursor, *factor)?;
    }
    Ok(cursor)
}

fn emit_byte(out: &mut [u8], cursor: &mut usize, byte: u8) -> Result<(), GameError> {
    if *cursor >= out.len() {
        return Err(GameError::OutOfRange);
    }
    out[*cursor] = byte;
    *cursor += 1;
    Ok(())
}

fn emit_number(out: &mut [u8], cursor: &mut usize, mut value: u64) -> Result<(), GameError> {
    if value == 0 {
        emit_byte(out, cursor, b'0')?;
        return Ok(());
    }
    let mut digits = [0u8; 20];
    let mut len = 0;
    while value > 0 {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    while len > 0 {
        len -= 1;
        emit_byte(out, cursor, digits[len])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factors_of(value: u64) -> [u64; 16] {
        let mut out = [0u64; 16];
        let count = trial_divide(value, &mut out).unwrap();
        let mut trimmed = [0u64; 16];
        trimmed[..count].copy_from_slice(&out[..count]);
        trimmed
    }

    fn line(number: u64) -> String {
        let mut factors = [0u64; 16];
        let count = trial_divide(number, &mut factors).unwrap();
        let mut out = [0u8; 96];
        let len = render_factors(number, &factors[..count], &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_values_parse() {
        assert_eq!(parse_value("12"), Ok(12));
        assert_eq!(parse_value("0"), Ok(0));
        assert_eq!(parse_value(""), Err(GameError::InvalidArgument));
        assert_eq!(parse_value("-5"), Err(GameError::InvalidArgument));
        assert_eq!(
            parse_value("18446744073709551616"),
            Err(GameError::OutOfRange)
        );
    }

    #[test]
    fn test_small_factoring() {
        assert_eq!(&factors_of(12)[..4], &[2, 2, 3, 0]);
        assert_eq!(&factors_of(13)[..1], &[13]);
        assert_eq!(&factors_of(1)[..0], &[]);
    }

    #[test]
    fn test_large_prime_beyond_table() {
        // 104729 is the ten thousandth prime, past the small table.
        assert_eq!(&factors_of(104729)[..1], &[104729]);
        assert_eq!(&factors_of(104729 * 2)[..2], &[2, 104729]);
    }

    #[test]
    fn test_line_shape() {
        assert_eq!(line(12), "12: 2 2 3");
        assert_eq!(line(13), "13: 13");
    }

    #[test]
    fn test_small_buffer_rejected() {
        let mut out = [0u64; 1];
        assert_eq!(trial_divide(2 * 3 * 5 * 7, &mut out), Err(GameError::OutOfRange));
        let mut bytes = [0u8; 3];
        assert_eq!(
            render_factors(12, &[2, 2, 3], &mut bytes),
            Err(GameError::OutOfRange)
        );
    }
}
