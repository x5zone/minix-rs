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

/// The two kinds of conversion failure for a bound string. C splits them the
/// same way at `primes.c:136-148`, and **only one kind appends a sentence**:
/// when `errno` is set it goes through `err(1, "%s", argv[i])`, and the only
/// path this command can take there is `ERANGE` (overflow and underflow) from
/// `_strtol.h:157`/`:172`. "Not one digit converted" is not in that group — the
/// real source sets no errno (the `EINVAL` at `_strtol.h:69` is the
/// invalid-base branch, unreachable since base is always 0) and only leaves
/// `endptr` at the original `nptr` (`:184-186`'s `any ? s - 1 : nptr`), so it
/// belongs with "converted a prefix, then a non-terminator" in the
/// `*p != '\0'` branch, taking `errx(1, "%s: illegal numeric format.")`
/// (`primes.c:140`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundError {
    /// Converted but outside the representable range: `errno = ERANGE` at
    /// `_strtol.h:157`/`:172`.
    OutOfRange,
    /// `endptr` did not land on the terminator: either a trailing character or
    /// a shape where **not one digit was converted** (the real source leaves
    /// `endptr` in place, so from `*p` the two are indistinguishable). Takes the
    /// `errx` sentence.
    TrailingJunk,
}

/// Parse a bound the way C's `strtoumax(word, &p, 0)` does — **base 0**, so `0x1f` is
/// 31, a leading `0` is octal (`010` = 8) and anything else is decimal — and, like C,
/// refuse a word with trailing characters (`*p != '\0'`, `primes.c:139`/`:146`).
///
/// The size check C performs on `stop` (`stop > SPSPMAX`, `primes.c:149`) is not
/// applied here: it lives in `main`, so it stays with `over_cap`.
pub fn parse_bound(word: &str) -> Result<u64, BoundError> {
    let (value, consumed, overflowed) = scan_unsigned_base0(word);
    if overflowed {
        return Err(BoundError::OutOfRange);
    }
    // No conversion (consumed == 0) means, in the real source, "endptr left in
    // place" = `*p != '\0'` holds, the same branch as trailing characters, so
    // report TrailingJunk rather than inventing another sentence.
    if consumed != word.len() {
        return Err(BoundError::TrailingJunk);
    }
    Ok(value)
}

/// The value part of C `strtoumax(text, &end, 0)`: skip leading whitespace,
/// allow one sign, then look at the prefix — `0x`/`0X` goes hexadecimal, a
/// single leading `0` followed by octal digits goes octal, everything else goes
/// decimal. Returns (value, bytes consumed, overflowed). **When not one digit
/// is consumed it returns `(0, 0, false)`** (matching C: `end` stays at the
/// original `nptr`), whereas this repo's real source `_strtol.h:69` sets
/// `EINVAL` on "no conversion", so the layer above uses `consumed == 0` as that
/// error (host glibc does not set this errno, so the reference binary lands
/// elsewhere on the `abc` shape; the real source wins).
fn scan_unsigned_base0(text: &str) -> (u64, usize, bool) {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while matches!(
        bytes.get(at),
        Some(b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
    ) {
        at += 1;
    }
    let minus = bytes.get(at) == Some(&b'-');
    if matches!(bytes.get(at), Some(b'+') | Some(b'-')) {
        at += 1;
    }
    // Prefix detection mirrors `_strtol.h:102-109` byte for byte: `0x`/`0X`
    // skips two characters and goes hexadecimal (**without** requiring an
    // actual hexadecimal digit after it — the `0x` shape lands back on
    // "endptr returned to its original place" via "not one digit consumed");
    // otherwise a current character of `0` switches to octal and anything else
    // uses decimal. The octal branch does not consume that `0` up front, so it
    // enters the loop as a legal digit and `08` stops at `8` (same shape as the
    // real source: `i >= base` breaks).
    let radix: u64 =
        if bytes.get(at) == Some(&b'0') && matches!(bytes.get(at + 1), Some(b'x') | Some(b'X')) {
            at += 2;
            16
        } else if bytes.get(at) == Some(&b'0') {
            8
        } else {
            10
        };
    // The start counts **after the prefix**: the real source's `any` is set
    // only by "accumulated one digit" (`:170`/`:178`), and skipping the `0x`
    // prefix alone does not count, so the `0x` shape still returns endptr to
    // its original place (consumed = 0).
    let digits_start = at;
    let mut value: u64 = 0;
    let mut overflow = false;
    while let Some(&byte) = bytes.get(at) {
        let Some(digit) = (byte as char).to_digit(radix as u32) else {
            break;
        };
        match value
            .checked_mul(radix)
            .and_then(|scaled| scaled.checked_add(u64::from(digit)))
        {
            Some(next) => value = next,
            None => {
                // C's `strtoumax` saturates to `UINTMAX_MAX` and sets `ERANGE`
                // (`_strtol.h:157`/`:172`).
                overflow = true;
            }
        }
        at += 1;
    }
    if at == digits_start {
        // Not one digit converted: the real source's `any` is still 0 ->
        // `*endptr = nptr` (`_strtol.h:184-186`); even leading whitespace and
        // the sign do not count as consumed, and **errno is untouched**; the
        // caller hands it back to the `errx` sentence through "endptr did not
        // land on the terminator", so it is not a separate branch.
        return (0, 0, false);
    }
    if minus {
        // `strtoumax` meets a negative value: `__INT` is unsigned, so the
        // underflow criterion at `_strtol.h:152-157`
        // (`acc == cutoff && i > cutlim`) holds on the first digit -> returns 0
        // and sets `ERANGE`. (glibc wraps silently here, so the reference
        // binary lands elsewhere than the real source.) Both primes roads
        // reject the minus sign before converting (`primes.c:133`/`:193`), so
        // this is unreachable from the command road; it exists only to keep the
        // library consistent with the real source.
        return (0, at, true);
    }
    (value, at, overflow)
}

/// C `read_num_buf`'s verdict on one `buf` (`primes.c:186-204`). This function
/// only decides which branch this leg takes; reading and writing stay in the
/// shell, so all four legs can be tested purely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumVerdict<'a> {
    /// Blank line (or just a newline): C `continue`s and reads the next line.
    SkipLine,
    /// After skipping whitespace the line starts with `-`:
    /// `errx(1, "negative numbers aren't permitted.")` (`:193-194`).
    Negative,
    /// The conversion set errno, so it goes through `err(1, "%s", buf)`
    /// (`:198-199`) — a suffixed sentence, and `buf` **contains the newline**,
    /// so that line breaks into two physical lines. This leg carries no error
    /// payload: the only thing in this command that can set errno is
    /// `strtoumax`'s out-of-range case (see the [`BoundError`] block); no
    /// conversion sets nothing.
    RangeError,
    /// Digits converted but not followed by a newline:
    /// `errx(1, "%s: illegal numeric format.", buf)` (`:200-201`). Note that C
    /// passes the whole `buf` here too, not just the remaining characters.
    TrailingJunk(&'a str),
    /// Accepted a starting value.
    Start(u64),
}

/// See [`NumVerdict`]. `line` is C's `buf`: it includes the trailing newline
/// (except for the last line).
pub fn judge_number_line(line: &str) -> NumVerdict<'_> {
    let trimmed = line.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    if trimmed.is_empty() || trimmed.starts_with('\n') {
        return NumVerdict::SkipLine;
    }
    if trimmed.starts_with('-') {
        return NumVerdict::Negative;
    }
    let (value, consumed, overflowed) = scan_unsigned_base0(trimmed);
    if overflowed {
        return NumVerdict::RangeError;
    }
    // No conversion is not a separate branch: the real source leaves `endptr`
    // in place, so the "not a newline" check below catches it naturally.
    let rest = &trimmed[consumed..];
    if !rest.starts_with('\n') {
        // C compares `*p != '\n'`: the last chunk without a trailing newline,
        // or anything after the digits, both count as this branch.
        return NumVerdict::TrailingJunk(line);
    }
    NumVerdict::Start(value)
}

/// C's upper-bound check (`primes.c:149`: `stop > SPSPMAX`). This tree's cap is
/// `MAX_BOUND`, not C's near-2^32 (a declared difference), but the message
/// shape is the same.
#[must_use]
pub fn over_cap(value: u64) -> bool {
    value > MAX_BOUND as u64
}

/// C's last stored prime (`pr_limit` in `minix3/games/primes/pr_tbl.c` points
/// at the end of the table, and the largest number measured in it is 65537).
/// It is not this tree's capacity parameter but the pivot of the `-d` shape's
/// split: C's printing has two roads — the in-table road
/// (`primes.c:251-262`, condition `start <= *pr_limit`) and the sieve road
/// (`:322-332`), the latter guarded by `dflag && (prev || start <= *pr_limit)`.
/// When the sieve road is reached, `start` has either been rewritten by the
/// in-table road to `*pr_limit+2` or was already outside the table, so half the
/// condition is false either way and the guard is effectively just
/// `prev != 0`. Two shapes checked against the reference process:
/// `primes -d 65537 65560` prints `65537 (16)` (in table, has a predecessor),
/// and `primes -d 70000 70100` prints the first `70001` **without the delta
/// column**.
pub const TABLE_LAST_PRIME: u32 = 65_537;

/// The "previous prime" used by the `-d` delta column, aligned cell by cell
/// with C's `prev` (`primes.c:220` starts at 0 and the in-table road sets
/// `prev = factor` on each step, `:261`). So the delta of the table's first
/// prime 2 is `2 - 0 = 2` — the reference process running `primes -d 2 10`
/// prints `2 (2)|3 (1)|5 (2)|7 (2)`, and that `2 (2)`, which looks wrong, is
/// C's own shape, not an oversight in this tree. A prime above 2 takes the
/// largest prime below it; the table is the consecutive prime sequence starting
/// at 2, so "the previous one in the table" and "the true predecessor" are the
/// same thing.
#[must_use]
pub fn previous_prime(value: u32) -> u32 {
    if value <= 2 {
        return 0;
    }
    let mut candidate = value - 1;
    if candidate > 2 && candidate.is_multiple_of(2) {
        candidate -= 1;
    }
    while candidate >= 3 {
        if is_prime(candidate) {
            return candidate;
        }
        candidate -= 2;
    }
    2
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
        // An empty string is a **legal value 0** in C: `strtoumax("")` sets no
        // errno and `endptr` stops right on the terminator, so `*p != '\0'`
        // does not hold (the reference `primes "" 5` -> 2/3, the same reading
        // as this tree).
        assert_eq!(parse_bound(""), Ok(0));
        assert_eq!(parse_bound("12a"), Err(BoundError::TrailingJunk));
        // With base 0 and a sign C reports no error and sets no errno:
        // `strtoumax` wraps directly when `__INT` is unsigned
        // (`_strtol.h:129-157` uses the unsigned boundary for its cutoff), so
        // the library returning the wrapped value is the real-source behavior.
        // The operand road cannot see this shape, because C checks
        // `argv[i][0] == '-'` before converting (`primes.c:133`/`:155`); that
        // guard lives in the shell and is pinned end to end (`primes -5` falls
        // into usage).
        assert_eq!(parse_bound("-5"), Err(BoundError::OutOfRange));
        assert_eq!(parse_bound("+12"), Ok(12));
        assert_eq!(parse_bound("2000000"), Ok(2_000_000));
        assert!(over_cap(2_000_000) && !over_cap(MAX_BOUND as u64));
        assert_eq!(
            parse_bound("18446744073709551616"),
            Err(BoundError::OutOfRange)
        );
        let mut out = [0u32; 4];
        assert_eq!(sieve(1, 20, &mut out), Err(GameError::OutOfRange));
        assert_eq!(sieve(0, 2_000_000, &mut out), Err(GameError::OutOfRange));
    }

    #[test]
    fn test_difference_predecessors() {
        // Each line matches the `-d` readings of the reference process (the
        // host-compiled `minix3/games/primes/*.c`).
        assert_eq!(previous_prime(2), 0); // -> `2 (2)`: C's prev starts at 0; that odd-looking shape is C's own
        assert_eq!(previous_prime(3), 2); // → `3 (1)`
        assert_eq!(previous_prime(5), 3); // → `5 (2)`
        assert_eq!(previous_prime(7), 5);
        assert_eq!(previous_prime(100), 97); // 101 prints `101 (4)`
        assert_eq!(previous_prime(TABLE_LAST_PRIME), 65_521); // the last table entry prints `65537 (16)`
    }

    #[test]
    fn test_base_zero_conversions() {
        // Both C roads use `strtoumax(..., 0)`, so all three carry prefixes must
        // be recognized. The readings come one by one from the reference process
        // (`primes 0x10 0x20` and `primes 010 014` live runs).
        assert_eq!(parse_bound("0x1f"), Ok(31));
        assert_eq!(parse_bound("0X1F"), Ok(31));
        assert_eq!(parse_bound("010"), Ok(8));
        assert_eq!(parse_bound("0"), Ok(0));
        assert_eq!(parse_bound("  7"), Ok(7));
        // No hexadecimal digit after `0x`: C only consumes the `0` and treats
        // the remaining `x` as a trailing character (not "no conversion").
        assert_eq!(parse_bound("0x"), Err(BoundError::TrailingJunk));
        assert_eq!(parse_bound("08"), Err(BoundError::TrailingJunk));
    }

    #[test]
    fn test_stdin_line_legs_split_as_c_does() {
        // The four branches of `read_num_buf`, verbatim against
        // `primes.c:186-204`.
        assert_eq!(judge_number_line(""), NumVerdict::SkipLine);
        assert_eq!(judge_number_line("\n"), NumVerdict::SkipLine);
        assert_eq!(judge_number_line("  \n"), NumVerdict::SkipLine);
        assert_eq!(judge_number_line("-5\n"), NumVerdict::Negative);
        assert_eq!(judge_number_line(" -5\n"), NumVerdict::Negative);
        assert_eq!(
            judge_number_line("abc\n"),
            NumVerdict::TrailingJunk("abc\n")
        );
        assert_eq!(judge_number_line("0x\n"), NumVerdict::TrailingJunk("0x\n"));
        assert_eq!(
            judge_number_line("999999999999999999999999\n"),
            NumVerdict::RangeError
        );
        // Both branches receive the whole `buf` (newline included), not just
        // the remaining characters.
        assert_eq!(
            judge_number_line("10x\n"),
            NumVerdict::TrailingJunk("10x\n")
        );
        assert_eq!(
            judge_number_line("12 34\n"),
            NumVerdict::TrailingJunk("12 34\n")
        );
        // The last chunk with no trailing newline also counts as "not followed
        // by a newline" (C compares against `'\n'`).
        assert_eq!(judge_number_line("12"), NumVerdict::TrailingJunk("12"));
        assert_eq!(judge_number_line("12\n"), NumVerdict::Start(12));
        assert_eq!(judge_number_line("0x10\n"), NumVerdict::Start(16));
        assert_eq!(judge_number_line("010\n"), NumVerdict::Start(8));
        assert_eq!(judge_number_line("  7\n"), NumVerdict::Start(7));
    }
}
