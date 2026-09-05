//! Prime table generation.
//!
//! Ground truth: `minix3/games/primes/primes.c` (table generation between two
//! bounds), `pattern.c` (wheel patterns), `pr_tbl.c` (stored tables), and
//! `spsp.c` (spaced printing). Generation prints every prime from the lower
//! bound to the upper bound inclusive. This module owns sieving; display
//! spacing stays with the execution layer.

use crate::GameError;

/// Largest bound accepted (keeps the sieve working set small).
pub const MAX_BOUND: u32 = 1_000_000;

/// Parse a decimal bound.
pub fn parse_bound(word: &str) -> Result<u32, GameError> {
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
    if value > MAX_BOUND as u64 {
        return Err(GameError::OutOfRange);
    }
    Ok(value as u32)
}

/// Generate primes from `lo` to `hi` inclusive into `out`.
///
/// Each odd candidate is trial divided by known small factors up to its square
/// root (the wheel skips even candidates). Returns the prime count. An
/// inverted interval yields zero primes. Trial division is slower than a
/// sieved table but needs no working set, which suits short lived command
/// processes and small test bounds.
pub fn sieve(lo: u32, hi: u32, out: &mut [u32]) -> Result<usize, GameError> {
    if hi > MAX_BOUND {
        return Err(GameError::OutOfRange);
    }
    if lo > hi {
        return Ok(0);
    }
    let mut count = 0;
    if lo <= 2 && 2 <= hi {
        if count >= out.len() {
            return Err(GameError::OutOfRange);
        }
        out[count] = 2;
        count += 1;
    }
    let mut candidate = if lo <= 3 { 3 } else { lo | 1 };
    while candidate <= hi {
        if is_prime(candidate) {
            if count >= out.len() {
                return Err(GameError::OutOfRange);
            }
            out[count] = candidate;
            count += 1;
        }
        if candidate > hi - 2 {
            break;
        }
        candidate += 2;
    }
    Ok(count)
}

/// True when `value` (odd, at least 3) has no divisor up to its root.
fn is_prime(value: u32) -> bool {
    if value < 3 || value.is_multiple_of(2) {
        return value == 2;
    }
    let mut factor: u32 = 3;
    while factor <= value / factor {
        if value.is_multiple_of(factor) {
            return false;
        }
        factor += 2;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(lo: u32, hi: u32) -> Vec<u32> {
        let mut out = [0u32; 256];
        let count = sieve(lo, hi, &mut out).unwrap();
        out[..count].to_vec()
    }

    #[test]
    fn test_first_primes() {
        assert_eq!(collect(1, 20), vec![2, 3, 5, 7, 11, 13, 17, 19]);
    }

    #[test]
    fn test_inverted_interval_empty() {
        assert_eq!(collect(20, 10), Vec::<u32>::new());
    }

    #[test]
    fn test_single_values() {
        assert_eq!(collect(13, 13), vec![13]);
        assert_eq!(collect(14, 14), Vec::<u32>::new());
        assert_eq!(collect(2, 2), vec![2]);
    }

    #[test]
    fn test_bounds_checked() {
        assert_eq!(parse_bound("100"), Ok(100));
        assert_eq!(parse_bound(""), Err(GameError::InvalidArgument));
        assert_eq!(parse_bound("12a"), Err(GameError::InvalidArgument));
        assert_eq!(parse_bound("2000000"), Err(GameError::OutOfRange));
        let mut out = [0u32; 4];
        assert_eq!(sieve(1, 20, &mut out), Err(GameError::OutOfRange));
        assert_eq!(sieve(0, 2_000_000, &mut out), Err(GameError::OutOfRange));
    }
}
