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
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
];

/// C's two fixed diagnostics (`errx` calls at `factor.c:174`, `:168`, `:219`).
pub const NUMBERS_MSG: &str = "numbers <= 1 aren't permitted.";
pub const NEGATIVE_MSG: &str = "negative numbers aren't permitted.";

/// What C makes of one value token: factor it, skip the line, or leave with a sentence.
///
/// The rejection sentences come from different places in C and are **not** interchangeable
/// (`factor.c:156-179`, `:218-219`):
///
/// * `NotPermitted` is `pr_fact`'s firewall for a value of 0 or 1 (`:218-219`) — and, by a
///   wording slip in C, also the answer for an operand that starts with `-` (`:173-174`),
///   so `factor -` and `factor 1` print the same sentence.
/// * `Negative` is the stdin-only check made *after* the blank scan (`:166-168`), so a
///   piped `-6` gets the other sentence instead.
/// * `Illegal` is the parse verdict: `BN_dec2bn` accepts an optional sign and digits, and
///   the value is valid only when nothing but the line's newline follows (`:119-128`).
///   A token with no digits at all is **not** illegal — `strtoul` yields 0, which the
///   firewall answers as `NotPermitted`.
/// * `Skip` exists only on the stdin path: a line of blanks ends at `\n` or `\0` right
///   away and C reads the next one without printing (`:163-165`).
///
/// Two deliberate divergences, both registered in the stage ledger: overflow is answered as
/// `Illegal` here while C leaves through `err(1, "%s", str)` with a `strerror` suffix
/// (`:123-124`) — same exit status, different sentence; and the minus test runs after the
/// blank scan on both paths, where C tests an operand's very first byte (`:173`), so an
/// operand like `" -6"` (which C would hand to `strtoul` as a wrapped huge value) gets a
/// sentence here.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A value to factor.
    Value(u64),
    /// Nothing but blanks on a stdin line.
    Skip,
    /// C's `numbers <= 1 aren't permitted.`.
    NotPermitted,
    /// C's `negative numbers aren't permitted.`, stdin path only.
    Negative,
    /// C's `<value>: illegal numeric format.`.
    Illegal,
}

/// Decide what C would do with one token, blank-for-blank (`:119-128`, `:156-179`).
///
/// `word` may carry the line's trailing newline (the stdin path keeps it in C's `buf`);
/// both the blank scan and the end-of-value test treat it as C does.
pub fn verdict(word: &str, from_stdin: bool) -> Verdict {
    let bytes = word.as_bytes();
    let mut cursor = 0usize;
    // `isblank` in the C locale is space and horizontal tab, and `strtoul` skips the same
    // set in front of a number, so both paths land on the first real byte here.
    while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }
    if cursor == bytes.len() || matches!(bytes[cursor], b'\n' | b'\r') {
        // C's blank scan stops a stdin line here without printing (`factor.c:163-165`); the
        // same text as an operand simply yields no digits, which `strtoul` reads as 0 and
        // `pr_fact` rejects (`:218-219`).
        return if from_stdin {
            Verdict::Skip
        } else {
            Verdict::NotPermitted
        };
    }
    if bytes[cursor] == b'-' {
        // The two sentences differ by path, not by magnitude — see the type's doc.
        return if from_stdin {
            Verdict::Negative
        } else {
            Verdict::NotPermitted
        };
    }
    if bytes[cursor] == b'+' {
        cursor += 1;
    }
    let mut value: u64 = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        let scaled = value
            .checked_mul(10)
            .and_then(|wide| wide.checked_add((bytes[cursor] - b'0') as u64));
        match scaled {
            Some(next) => value = next,
            // C saturates and sets `ERANGE`, which leaves through a different sentence.
            None => return Verdict::Illegal,
        }
        cursor += 1;
    }
    // C's validity test is `*p == '\n' || *p == '\0'` after `strtoul`, and `strtoul` handed
    // nothing but a sign yields the value 0 — which the firewall below rejects as ≤ 1.
    if cursor < bytes.len() && !matches!(bytes[cursor], b'\n' | b'\r') {
        return Verdict::Illegal;
    }
    if value < 2 {
        return Verdict::NotPermitted;
    }
    Verdict::Value(value)
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
    fn verdicts_accept_signs_and_blanks_the_way_strtoul_does() {
        // C parses through `strtoul` and then demands that nothing but the line's newline
        // follows (`factor.c:119-128`), so a sign and leading blanks are accepted while
        // trailing text is not.
        assert_eq!(verdict("12", false), Verdict::Value(12));
        assert_eq!(verdict("+12", false), Verdict::Value(12));
        assert_eq!(verdict("  12", true), Verdict::Value(12));
        assert_eq!(verdict("12\n", true), Verdict::Value(12));
        assert_eq!(verdict("12x", false), Verdict::Illegal);
        assert_eq!(
            verdict("", false),
            Verdict::NotPermitted,
            "no digits is the value 0"
        );
        assert_eq!(verdict("+", false), Verdict::NotPermitted);
    }

    #[test]
    fn verdicts_split_the_three_sentences_by_path_and_magnitude() {
        // 0 and 1 fall to `pr_fact`'s firewall (`:218-219`); a leading `-` depends on which
        // loop saw it (`:173-174` for operands, `:166-168` for standard input); and a line of
        // only blanks is skipped without a word (`:163-165`).
        assert_eq!(verdict("1", false), Verdict::NotPermitted);
        assert_eq!(verdict("0", true), Verdict::NotPermitted);
        assert_eq!(verdict("-6", false), Verdict::NotPermitted);
        assert_eq!(verdict("-6\n", true), Verdict::Negative);
        assert_eq!(verdict("   \n", true), Verdict::Skip);
        assert_eq!(verdict("\t", true), Verdict::Skip);
        assert_eq!(
            verdict("  ", false),
            Verdict::NotPermitted,
            "an empty operand is 0"
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
        assert_eq!(
            trial_divide(2 * 3 * 5 * 7, &mut out),
            Err(GameError::OutOfRange)
        );
        let mut bytes = [0u8; 3];
        assert_eq!(
            render_factors(12, &[2, 2, 3], &mut bytes),
            Err(GameError::OutOfRange)
        );
    }
}
