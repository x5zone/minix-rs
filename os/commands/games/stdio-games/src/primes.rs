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

/// 一支边界串的三种转换失手。C 在 `primes.c:136-148` 就按这三种分岔：`errno` 被置起
/// 就直走 `err(1, "%s", argv[i])`（`minix3/common/lib/libc/stdlib/_strtol.h:69` 的
/// `EINVAL` 与 `:157`/`:172` 的 `ERANGE`），只有「转出了数字但后面还有字符」（`*p != '\0'`，
/// 此时 `errno` 未置）才走 `errx(1, "%s: illegal numeric format.")`（`primes.c:140`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundError {
    /// 一个数字都转不出来：`_strtol.h:69` 的 `errno = EINVAL`。
    NoConversion,
    /// 转出来了但超出可表示范围：`_strtol.h:157`/`:172` 的 `errno = ERANGE`。
    OutOfRange,
    /// 转出了前缀而后有尾字符：`primes.c:140` 那一支。
    TrailingJunk,
}

/// Parse a decimal bound the way `strtoumax(argv[i], &p, 0)` does for the
/// digits-only case, **without** applying any bound of its own: the size check
/// C performs (`stop > SPSPMAX`, `primes.c:149`) lives in `main`, so it stays
/// with the execution layer. Undocumented by C's face: this accepts a plain
/// decimal string only — the `0x`/octal prefixes, leading blanks and sign that
/// base 0 would take are not ported (registered in 22 篇).
pub fn parse_bound(word: &str) -> Result<u64, BoundError> {
    let bytes = word.as_bytes();
    if !matches!(bytes.first(), Some(b'0'..=b'9')) {
        // 空串与非数字开头同归 C 的「无转换」支（`errno = EINVAL`）。
        return Err(BoundError::NoConversion);
    }
    let mut value: u64 = 0;
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((bytes[index] - b'0') as u64))
            .ok_or(BoundError::OutOfRange)?;
        index += 1;
    }
    if index < bytes.len() {
        return Err(BoundError::TrailingJunk);
    }
    Ok(value)
}

/// C 的那支上限检查（`primes.c:149`：`stop > SPSPMAX`）。本树的上限是
/// `MAX_BOUND`，不是 C 的近 2^32（声明的差异，见 22 篇），但报文形状同形。
#[must_use]
pub fn over_cap(value: u64) -> bool {
    value > MAX_BOUND as u64
}

/// C 存表的最后一个素数（`minix3/games/primes/pr_tbl.c` 的 `pr_limit` 指向表尾，
/// 实测表里最大的数就是 65537）。它不是本树的容量参数，而是 `-d` 一条形分岔的支点：
/// C 的打印分两条路——表内路（`primes.c:251-262`，条件 `start <= *pr_limit`）与筛子路
/// （`:322-332`），后者守卫是 `dflag && (prev || start <= *pr_limit)`，而走到筛子路时
/// `start` 要么被表内路改写成 `*pr_limit+2`、要么本来就在表外，两种情况下半个条件都为假，
/// 所以守卫实际只剩 `prev != 0`。拿参照进程验过的两个形：`primes -d 65537 65560` 打
/// `65537 (16)`（表内，有前驱），`primes -d 70000 70100` 第一个 `70001` **不带差值列**。
pub const TABLE_LAST_PRIME: u32 = 65_537;

/// `-d` 差值列用的「前一个素数」，逐格对位 C 的 `prev`（`primes.c:220` 起值为 0，
/// 表内路每走一格 `prev = factor`，`:261`）。因此整张表的第一个素数 2 的差是
/// `2 - 0 = 2`——参照进程实跑 `primes -d 2 10` 得 `2 (2)|3 (1)|5 (2)|7 (2)`，那个看着
/// 像错的 `2 (2)` 是 C 自己的形状，不是本树的疏漏。大于 2 的素数取它下面最大的素数；
/// 表是 2 开始的连续素数序列，所以「表里上一个」与「真前驱」同一件事。
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
        assert_eq!(parse_bound(""), Err(BoundError::NoConversion));
        assert_eq!(parse_bound("12a"), Err(BoundError::TrailingJunk));
        assert_eq!(parse_bound("-5"), Err(BoundError::NoConversion));
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
        // 逐条对位参照进程（宿主编译的 `minix3/games/primes/*.c`）的 `-d` 读数。
        assert_eq!(previous_prime(2), 0); // → `2 (2)`：C 的 prev 从 0 起，那个看着像错的形状是它自己的
        assert_eq!(previous_prime(3), 2); // → `3 (1)`
        assert_eq!(previous_prime(5), 3); // → `5 (2)`
        assert_eq!(previous_prime(7), 5);
        assert_eq!(previous_prime(100), 97); // 101 印 `101 (4)`
        assert_eq!(previous_prime(TABLE_LAST_PRIME), 65_521); // 表尾印 `65537 (16)`
    }
}
