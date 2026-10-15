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

/// 一支边界串的两类转换失手。C 在 `primes.c:136-148` 就按这两类分岔，而**只有一类会缀句子**：
/// `errno` 被置起时走 `err(1, "%s", argv[i])`，而本命令能走到那里的只有 `_strtol.h:157`/`:172`
/// 的 `ERANGE`（溢出与下溢）。「一个数字也转不出」不在这里——真源不置 errno（`_strtol.h:69` 那个
/// `EINVAL` 是 base 参数非法的分支，base 恒为 0 根到不了），只把 `endptr` 停在最初的 `nptr`
/// （`:184-186` 的 `any ? s - 1 : nptr`），因此它与「转出了前缀而后可非终止符」同属
/// `*p != '\0'` 那一支，走 `errx(1, "%s: illegal numeric format.")`（`primes.c:140`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundError {
    /// 转出来了但超出可表示范围：`_strtol.h:157`/`:172` 的 `errno = ERANGE`。
    OutOfRange,
    /// `endptr` 没有落在终止符上：可能是尾字符，也可能是**一个位都没转出的**形
    /// （真源把 `endptr` 留在原处，所以从 `*p` 看来两者不二）。走 `errx` 那句。
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
    // 无转换（consumed == 0）在真源里就是「endptr 停在原处」= `*p != '\0'` 成立，
    // 与尾字符同属一支，所以交回 TrailingJunk 而不是另造一句。
    if consumed != word.len() {
        return Err(BoundError::TrailingJunk);
    }
    Ok(value)
}

/// C `strtoumax(text, &end, 0)` 的取值部分：跳前导空白、可带一个符号，再看前缀——
/// `0x`/`0X` 走十六进制、单一个前导 `0` 后接八进制位走八进制、其余走十进制。
/// 返回（值，吃掉的字节数，是否溢出）。**一个数字也没吃到时返回 `(0, 0, false)`**（与 C 一致：
/// `end` 停在最初的 `nptr`），而本仓真源 `_strtol.h:69` 在「无转换」时置 `EINVAL`，
/// 所以上层用 `consumed == 0` 当作那个错（宿主 glibc 不置这个号，参照件在 `abc` 那形
/// 因此与真源不同落点，以真源为准）。
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
    // 前缀判定逐字对位 `_strtol.h:102-109`：`0x`/`0X` 直接跳两位走十六进制（**不要求**
    // 后面真有十六进制位——`0x` 那形靠「一个位都没吃」落到 endptr 回原处）；否则当前字符
    // 是 `0` 就切八进制，不是才用十进制。八进制分支不预先吃掉那个 `0`，让它作为合法位
    // 进循环，于是 `08` 停在 `8` 上（真源同形：`i >= base` 就 break）。
    let radix: u64 =
        if bytes.get(at) == Some(&b'0') && matches!(bytes.get(at + 1), Some(b'x') | Some(b'X')) {
            at += 2;
            16
        } else if bytes.get(at) == Some(&b'0') {
            8
        } else {
            10
        };
    // 起点算在**前缀之后**：真源的 `any` 只由「累计到一个位」置起（`:170`/`:178`），单跳掉
    // `0x` 前缀不算，所以 `0x` 那形的 endptr 仍回原处（consumed = 0）。
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
                // C 的 `strtoumax` 饱和到 `UINTMAX_MAX` 并置 `ERANGE`（`_strtol.h:157`/`:172`）。
                overflow = true;
            }
        }
        at += 1;
    }
    if at == digits_start {
        // 一个位也没转出：真源 `any` 仍为 0 → `*endptr = nptr`（`_strtol.h:184-186`），
        // 连前导空白与符号都不算已消费，而且**不动 errno**；调用方靠「`endptr` 没落在
        // 终止符上」把它交回 `errx` 那一句，因此不单独成支。
        return (0, 0, false);
    }
    if minus {
        // `strtoumax` 遇到负值：`__INT` 是无符号型，所以 `_strtol.h:152-157` 的下溢判据
        // `acc == cutoff && i > cutlim` 首枚位就成立 → 返 0 并置 `ERANGE`。
        // （glibc 在这里是安静回绕，参照件因此与真源不同落点。）primes 两条路都在转换前先拒
        // 负号（`primes.c:133`/`:193`），所以命令路上不可达；这里只为让库本身与真源一致。
        return (0, at, true);
    }
    (value, at, overflow)
}

/// C `read_num_buf` 对一块 `buf` 的判定（`primes.c:186-204`）。本函数只算这一条腿该走
/// 哪个分支，读与写都在壳里，所以四条腿都能用纯测抛真。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumVerdict<'a> {
    /// 空白行（或只有一个换行）：C `continue` 再读下一行。
    SkipLine,
    /// 跳完空白后行首是 `-`：`errx(1, "negative numbers aren't permitted.")`（`:193-194`）。
    Negative,
    /// 转换置起了 errno，因此走 `err(1, "%s", buf)`（`:198-199`）——缀句子，且 `buf`
    /// **含换行**，那一行会断成两物理行。本条腿不带错型载荷：本命令里能置起 errno 的
    /// 只有 `strtoumax` 的越范围一种（见 [`BoundError`] 那段），无转换不置号。
    RangeError,
    /// 转出了数字但紧跟的不是换行：`errx(1, "%s: illegal numeric format.", buf)`（`:200-201`）。
    /// 注意 C 这一支的实参也是整块 `buf`，不是剩下的那几个字符。
    TrailingJunk(&'a str),
    /// 收下一个起始值。
    Start(u64),
}

/// 见 [`NumVerdict`]。`line` 是 C 的 `buf`：包含结尾的换行（上一行除外）。
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
    // 无转换不单独成支：真源把 `endptr` 留在原处，所以下面那道「不是换行」的判据天然接住它。
    let rest = &trimmed[consumed..];
    if !rest.starts_with('\n') {
        // C 比的是 `*p != '\n'`：行尾无换行的最后一块、或数字后还有任何东西，都算此支。
        return NumVerdict::TrailingJunk(line);
    }
    NumVerdict::Start(value)
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
        // 空串在 C 里是**合法值 0**：`strtoumax("")` 不置 errno 且 `endptr` 就停在终止符上，
        // 所以 `*p != '\0'` 不成立（参照件 `primes "" 5` → 2/3，与本树同读数）。
        assert_eq!(parse_bound(""), Ok(0));
        assert_eq!(parse_bound("12a"), Err(BoundError::TrailingJunk));
        // base 0 带符号时 C 不报错也不置 errno：`strtoumax` 在 `__INT` 就是无符号型的
        // 情形下直接回绕（`_strtol.h:129-157` 的 cutoff 用的就是无符号边界），所以库函数
        // 交出回绕值才是真源行为。操数路看不到这一形，因为 C 在转换前就查 `argv[i][0] ==
        // '-'`（`primes.c:133`/`:155`），那一守卫在壳里，端到端门已钉（`primes -5` 落 usage）。
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
        // 逐条对位参照进程（宿主编译的 `minix3/games/primes/*.c`）的 `-d` 读数。
        assert_eq!(previous_prime(2), 0); // → `2 (2)`：C 的 prev 从 0 起，那个看着像错的形状是它自己的
        assert_eq!(previous_prime(3), 2); // → `3 (1)`
        assert_eq!(previous_prime(5), 3); // → `5 (2)`
        assert_eq!(previous_prime(7), 5);
        assert_eq!(previous_prime(100), 97); // 101 印 `101 (4)`
        assert_eq!(previous_prime(TABLE_LAST_PRIME), 65_521); // 表尾印 `65537 (16)`
    }

    #[test]
    fn test_base_zero_conversions() {
        // C 两条路用的都是 `strtoumax(…, 0)`，所以三个进位前缀都得认。
        // 读数逐条来自参照进程（`primes 0x10 0x20` 与 `primes 010 014` 的实跑）。
        assert_eq!(parse_bound("0x1f"), Ok(31));
        assert_eq!(parse_bound("0X1F"), Ok(31));
        assert_eq!(parse_bound("010"), Ok(8));
        assert_eq!(parse_bound("0"), Ok(0));
        assert_eq!(parse_bound("  7"), Ok(7));
        // `0x` 后无十六进制位：C 只吃掉那个 `0`，剩下 `x` 算尾字符（不是「无转换」）。
        assert_eq!(parse_bound("0x"), Err(BoundError::TrailingJunk));
        assert_eq!(parse_bound("08"), Err(BoundError::TrailingJunk));
    }

    #[test]
    fn test_stdin_line_legs_split_as_c_does() {
        // `read_num_buf` 的四支，逐字对位 `primes.c:186-204`。
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
        // 两支持的都是整块 `buf`（含换行），不是剩下的那几个字符。
        assert_eq!(
            judge_number_line("10x\n"),
            NumVerdict::TrailingJunk("10x\n")
        );
        assert_eq!(
            judge_number_line("12 34\n"),
            NumVerdict::TrailingJunk("12 34\n")
        );
        // 行尾无换行的最后一块也算「后面不是换行」（C 比的就是 `'\n'`）。
        assert_eq!(judge_number_line("12"), NumVerdict::TrailingJunk("12"));
        assert_eq!(judge_number_line("12\n"), NumVerdict::Start(12));
        assert_eq!(judge_number_line("0x10\n"), NumVerdict::Start(16));
        assert_eq!(judge_number_line("010\n"), NumVerdict::Start(8));
        assert_eq!(judge_number_line("  7\n"), NumVerdict::Start(7));
    }
}
