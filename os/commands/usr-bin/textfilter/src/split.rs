//! Chunk computation for `split`.
//!
//! Ground truth: `minix3/usr.bin/split/split.c` (NetBSD): the input
//! splits into chunks of `DEFLINE` (1000, split.c:56) lines — `-l`
//! moves the count — and the chunk files are named by a suffix that
//! enumerates `aa`, `ab`, ... `zz` (`sfxlen` defaults to 2,
//! split.c:61). This module computes the chunks (which lines go to
//! which chunk) and the suffix sequence; the doing half's file writes
//! wait for the gated create/write face on the host, so the doing half
//! can only report what it would create. The `-a` suffix-length, `-b` byte
//! count (`split.c:99-108`), and `-n` chunk-count (`split.c:123-127`) flags are
//! all wired here (`chunks` / `chunk_bytes` / `chunks_n`).

use alloc::string::String;
use alloc::vec::Vec;

/// Default lines per chunk (`DEFLINE`, split.c:56).
pub const DEFAULT_LINES: usize = 1000;

/// Default suffix length (`sfxlen`, split.c:61).
pub const DEFAULT_SFXLEN: usize = 2;

/// Computes the chunks: each entry holds that chunk's lines (the last
/// one may be short). `lines_per_chunk` of zero is a caller error the
/// doing half rejects.
pub fn chunks(lines: &[String], lines_per_chunk: usize) -> Vec<Vec<String>> {
    let mut chunks = Vec::new();
    if lines_per_chunk == 0 {
        return chunks;
    }
    let mut cursor = 0usize;
    while cursor < lines.len() {
        let end = (cursor + lines_per_chunk).min(lines.len());
        chunks.push(lines[cursor..end].to_vec());
        cursor = end;
    }
    chunks
}

/// Splits raw input into fixed-size byte chunks of `n` bytes (the last may be
/// short); `n == 0` or empty input yields no chunks. The byte-addressed cousin
/// of [`chunks`], mirroring `split.c` `split1(bytecnt, …)` behind `-b`.
pub fn chunk_bytes(data: &[u8], n: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let mut cursor = 0usize;
    while cursor < data.len() {
        let end = (cursor + n).min(data.len());
        out.push(&data[cursor..end]);
        cursor = end;
    }
    out
}

/// Splits `data` into exactly `n` byte chunks (`-n`, C `split3` → `split1
/// (size/n, n)`): the first `n-1` chunks are `size/n` bytes and the last
/// absorbs the remainder, so all `n` files are non-empty. `None` when `n == 0`
/// or `n > data.len()` (C `errx("can't split into more than %d files")`).
pub fn chunks_n(data: &[u8], n: usize) -> Option<Vec<&[u8]>> {
    if n == 0 || n > data.len() {
        return None;
    }
    let base = data.len() / n;
    let mut out = Vec::with_capacity(n);
    let mut cursor = 0usize;
    for _ in 0..n - 1 {
        out.push(&data[cursor..cursor + base]);
        cursor += base;
    }
    out.push(&data[cursor..]);
    Some(out)
}

/// The file suffix for chunk `index` (0 based) at width `width`: `width`
/// lowercase letters enumerating the base-26 space (`aa`..`zz` for the C
/// default `sfxlen` 2). `None` when the sequence is exhausted (past the
/// `26^width`-th chunk) or `width` is zero.
pub fn suffix(index: usize, width: usize) -> Option<String> {
    if width == 0 {
        return None;
    }
    let capacity = 26usize.checked_pow(width as u32)?;
    if index >= capacity {
        return None;
    }
    let mut out = Vec::with_capacity(width);
    let mut value = index;
    for place in (0..width).rev() {
        let divisor = 26usize.pow(place as u32);
        out.push(b'a' + (value / divisor) as u8);
        value %= divisor;
    }
    Some(out.into_iter().map(|b| b as char).collect())
}

/// Parsed `split` command line: the resolved line count and the file operands
/// in order (a bare `-` names standard input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitArgs<'a> {
    pub lines_per_chunk: usize,
    /// `-b` byte count per chunk (`bytecnt`, split.c:76); `Some` selects the
    /// byte-addressed mode over the line `lines_per_chunk` default.
    pub byte_size: Option<usize>,
    /// `-n` chunk count (`chunks`, split.c:77); `Some` selects `split3`
    /// (exactly this many byte chunks), mutually exclusive with `-b`/`-l`.
    pub chunk_count: Option<usize>,
    /// `-a` suffix width (`sfxlen`, split.c:61 default 2).
    pub sfxlen: usize,
    pub operands: Vec<&'a str>,
}

/// `split` argument failure, each arm mirroring a distinct C path so the caller
/// can reproduce C's message and exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError<'a> {
    /// `split.c:80-130` 的 `default: usage()`（两个都算过：`-l 3 -l 4` 的
    /// `if (numlines != 0) usage()`、多于两个操作数的 `if (*argv != NULL) usage()`）。
    Usage,
    /// 集外字母：libc 在返回 `'?'` **之前**先印一行（`getopt.c:101-104`），C 的
    /// `default:` 才接 usage —— 于是**两行**。`-Z`、`-Zb`、`--x`（报 `-`）都走这一支。
    UnknownOption(char),
    /// 带值选项缺值（`-l`/`-b`/`-a`/`-n` 后面没有下一段）：libc 印
    /// `option requires an argument -- <字母>`（`getopt.c:132-136`）后返回 `'?'`，
    /// 调用者的 `default:` 再接 usage —— 同样两行。
    MissingValue(char),
    /// A `-l`/`-N` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal line count.", …)`; carries the offending value for the message.
    IllegalLineCount(&'a str),
    /// A `-b` value that is not decimal (with optional `k`/`m` unit) or is
    /// zero: C `errx(1, "%s: illegal byte count.", …)` (split.c:99-108).
    IllegalByteCount(&'a str),
    /// A `-n` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal number of chunks.", …)` (split.c:123-127).
    IllegalChunkCount(&'a str),
    /// A `-a` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal suffix length.", …)` (split.c:117-121).
    IllegalSuffixLength(&'a str),
}

/// An all-decimal, nonzero line count, or `None` (the C `strtoull` + `*ep !=
/// '\0'` + `== 0` rejection). An over-wide digit run (past `usize`) is also
/// `None` here — an absurd input C would saturate, treated as illegal rather
/// than panicking.
fn line_count(v: &str) -> Option<usize> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match v.parse::<usize>() {
        Ok(n) if n > 0 => Some(n),
        _ => None,
    }
}

/// A `-b` byte count: decimal with an optional lowercase `k`/`m` unit
/// (×1024 / ×1048576), nonzero (`split.c:99-108`). `None` for an illegal or
/// zero count, or a product that overflows `usize` (absurd input).
fn byte_count(v: &str) -> Option<usize> {
    let (digits, mul) = match v.as_bytes().last()? {
        b'k' => (&v[..v.len() - 1], 1024usize),
        b'm' => (&v[..v.len() - 1], 1024 * 1024),
        _ => (v, 1usize),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .and_then(|n| n.checked_mul(mul))
}

/// Fold `split`'s argv tail faithfully to BSD `getopt("0123456789b:l:a:n:")`
/// (`split.c:80`).
///
/// Parity over the old whole-token matcher (`arg == "-l"`, value forced onto a
/// separate word): `-l` now takes its value either attached (`-l100`) or as the
/// next word (`-l 100`), and an all-digit `-N` is the obsolete line-count
/// kludge (`split.c:82-98`, applied only while the count is unset so a later
/// `-l` still trips the `numlines != 0` usage as C does). `--` ends option
/// scanning; a bare `-` and any non-option token are operands. `-a` sets the
/// suffix width, `-b` the byte chunk size, and `-n` the chunk count
/// (attached or next word); the three split modes are mutually exclusive.
pub fn parse_split_args<'a>(args: &'a [&'a str]) -> Result<SplitArgs<'a>, SplitError<'a>> {
    let mut lines_per_chunk = DEFAULT_LINES;
    let mut sfxlen = DEFAULT_SFXLEN;
    let mut byte_size: Option<usize> = None;
    let mut chunk_count: Option<usize> = None;
    let mut set = false;
    let mut operands: Vec<&'a str> = Vec::new();

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let tail = &arg[1..];
            // C 的遗留形 `-<数字>`：optstring 里 `0-9` 六个都是**合法字母**，第一个数字回来后
            // `split.c:86-90` 从 `argv[optind] + 1`（整段 token 去掉那个 `-`）取数字，
            // 所以 `-1000` 的列数是 1000、`-12x` 落 `12x: illegal line count.`，
            // 而 `-3b5` 的整段 `3b5` 一起交给 `strtoull`（不是只认「全数字尾巴」）。
            if bytes[1].is_ascii_digit() {
                if !set {
                    match line_count(tail) {
                        Some(n) => {
                            lines_per_chunk = n;
                            set = true;
                        }
                        None => return Err(SplitError::IllegalLineCount(tail)),
                    }
                }
                index += 1;
                continue;
            }
            match bytes[1] {
                b'l' => {
                    // C `case 'l'`: `if (numlines != 0) usage()`。
                    if set {
                        return Err(SplitError::Usage);
                    }
                    // Value attached after `l` (`-l100`) or the next word
                    // (`-l 100`).
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            // `-l` 后面没有下一段：libc 先印一行缺值（`getopt.c:132-136`）。
                            None => return Err(SplitError::MissingValue('l')),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => lines_per_chunk = n,
                        None => return Err(SplitError::IllegalLineCount(value)),
                    }
                    set = true;
                }
                b'a' => {
                    // C `case 'a'`: value attached (`-a3`) or next word (`-a 3`),
                    // all-digit and nonzero, else "illegal suffix length".
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::MissingValue('a')),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => sfxlen = n,
                        None => return Err(SplitError::IllegalSuffixLength(value)),
                    }
                }
                b'b' => {
                    // C `case 'b'`: value attached (`-b1000`) or next word
                    // (`-b 1000`), decimal with optional k/m, nonzero.
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::MissingValue('b')),
                        }
                    } else {
                        rest
                    };
                    match byte_count(value) {
                        Some(n) => byte_size = Some(n),
                        None => return Err(SplitError::IllegalByteCount(value)),
                    }
                }
                b'n' => {
                    // C `case 'n'`: all-decimal, nonzero, no trailing char
                    // (no k/m unit) — `split.c:123-127`.
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::MissingValue('n')),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => chunk_count = Some(n),
                        None => return Err(SplitError::IllegalChunkCount(value)),
                    }
                }
                // 集外字母：`getopt.c:95-104` 先印一行，再由 `split.c:128-129` 的
                // `default: usage()` 接第二行。递出去的是 `-` 后面的**第一个字符**
                // （聚簇与 `--x` 都只报第一个：量过 `-Zb` 报 `Z`、`--x` 报 `-`）。
                _ => {
                    return Err(SplitError::UnknownOption(
                        tail.chars().next().unwrap_or('-'),
                    ));
                }
            }
            index += 1;
            continue;
        }
        // **不重排**：非选项 token 一到，C 的选项环就当场结束（`getopt.c:74-78` 见不到
        // `-` 即返 -1），它和它后面的 token 整个落进 `argv += optind` 的那一段——
        // 第一个是输入档、第二个是前缀、还有第三个就 usage（`split.c:135-150`）。
        // 所以 `split in -Z` 里的 `-Z` 是**前缀**而不是选项（参照件现量：它去开 `in`
        // 而报 `split: in: No such file or directory`）。
        operands.extend_from_slice(&args[index..]);
        break;
    }

    // C reads one input operand then the output prefix (`split.c:135-143`);
    // more than two operands is rejected here (the old shell's guard, kept).
    if operands.len() > 2 {
        return Err(SplitError::Usage);
    }
    // C: `if (numlines == 0) numlines = DEFLINE; else if (bytecnt || chunks) usage()`;
    // `if (bytecnt && chunks) usage()` — the three modes are mutually exclusive.
    if set && (byte_size.is_some() || chunk_count.is_some()) {
        return Err(SplitError::Usage);
    }
    if byte_size.is_some() && chunk_count.is_some() {
        return Err(SplitError::Usage);
    }
    Ok(SplitArgs {
        lines_per_chunk,
        byte_size,
        chunk_count,
        sfxlen,
        operands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_chunks_of_thousand() {
        let lines: Vec<String> = (1..=2500).map(|n| n.to_string()).collect();
        let chunks = chunks(&lines, DEFAULT_LINES);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 1000);
        assert_eq!(chunks[1].len(), 1000);
        assert_eq!(chunks[2].len(), 500);
    }

    #[test]
    fn test_explicit_count_and_empty_input() {
        let lines: Vec<String> = (1..=5).map(|n| n.to_string()).collect();
        let groups = chunks(&lines, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[2], vec!["5".to_string()]);
        assert!(chunks(&[], 10).is_empty());
    }

    #[test]
    fn test_zero_count_is_no_chunks() {
        let lines = vec!["a".to_string()];
        assert!(chunks(&lines, 0).is_empty());
    }

    #[test]
    fn test_suffix_enumerates_two_letters() {
        // 默认 width=2：aa..zz 逐字不变（零回归铉）。
        assert_eq!(suffix(0, 2).as_deref(), Some("aa"));
        assert_eq!(suffix(1, 2).as_deref(), Some("ab"));
        assert_eq!(suffix(25, 2).as_deref(), Some("az"));
        assert_eq!(suffix(26, 2).as_deref(), Some("ba"));
        assert_eq!(suffix(675, 2).as_deref(), Some("zz"));
        assert_eq!(suffix(676, 2), None);
    }

    #[test]
    fn test_suffix_width_follows_sfxlen() {
        // -a 1：单字母 a..z（容量 26）。
        assert_eq!(suffix(0, 1).as_deref(), Some("a"));
        assert_eq!(suffix(25, 1).as_deref(), Some("z"));
        assert_eq!(suffix(26, 1), None);
        // -a 3：三字母 aaa..；index 26 = "aab"。
        assert_eq!(suffix(0, 3).as_deref(), Some("aaa"));
        assert_eq!(suffix(1, 3).as_deref(), Some("aab"));
        assert_eq!(suffix(26 * 26 - 1, 3).as_deref(), Some("azz"));
        assert_eq!(suffix(26 * 26 * 26, 3), None); // 超 26^3 穷举
        // width 0 非法 → None。
        assert_eq!(suffix(0, 0), None);
    }

    #[test]
    fn parse_default_and_separated_value() {
        // 无参：缺省 1000 行。分离值（向后兼容）。
        let d = parse_split_args(&[]).unwrap();
        assert_eq!(d.lines_per_chunk, DEFAULT_LINES);
        assert!(d.operands.is_empty());
        let a = parse_split_args(&["-l", "100", "f"]).unwrap();
        assert_eq!(a.lines_per_chunk, 100);
        assert_eq!(a.operands, vec!["f"]);
    }

    #[test]
    fn parse_attached_and_obsolete_accepted() {
        // 关键回归：旧前门拒这两个合法形。
        let a = parse_split_args(&["-l100", "f"]).unwrap();
        assert_eq!(a.lines_per_chunk, 100);
        assert_eq!(a.operands, vec!["f"]);
        let b = parse_split_args(&["-1000", "in", "out"]).unwrap();
        assert_eq!(b.lines_per_chunk, 1000);
        assert_eq!(b.operands, vec!["in", "out"]);
        // 裸 `-` 为操数、`--` 终止扫描。
        let c = parse_split_args(&["-l", "5", "-", "pfx"]).unwrap();
        assert_eq!(c.operands, vec!["-", "pfx"]);
        let e = parse_split_args(&["--", "-l", "-x"]).unwrap();
        assert_eq!(e.operands, vec!["-l", "-x"]);
        // obsolete 在 `-l` 之后再出现被忽略（C 只 `if (numlines == 0)` 才解）。
        let f = parse_split_args(&["-l", "3", "-5"]).unwrap();
        assert_eq!(f.lines_per_chunk, 3);
    }

    #[test]
    fn parse_suffix_length_minus_a() {
        // -a 接值：附值 `-a3` 与分词 `-a 3` 都置 sfxlen=3；默认 2。
        assert_eq!(parse_split_args(&["-a3", "f"]).unwrap().sfxlen, 3);
        assert_eq!(parse_split_args(&["-a", "3", "f"]).unwrap().sfxlen, 3);
        // -a 吃值后，`3` 不泄为操作数：`-a3 f` 只剩一个操数 `f`。
        assert_eq!(parse_split_args(&["-a3", "f"]).unwrap().operands, vec!["f"]);
        // 未给 -a 时 sfxlen 默认 2。
        assert_eq!(parse_split_args(&["f"]).unwrap().sfxlen, DEFAULT_SFXLEN);
        // 非法后缀长度（0 / 非全数字）：对位 C "illegal suffix length"。
        assert_eq!(
            parse_split_args(&["-a0"]).err(),
            Some(SplitError::IllegalSuffixLength("0"))
        );
        assert_eq!(
            parse_split_args(&["-a", "x"]).err(),
            Some(SplitError::IllegalSuffixLength("x"))
        );
    }

    #[test]
    fn parse_byte_count_minus_b() {
        // -b 接值：附值/分词、k/m 单位。行数保持默认（字节模式不数行）。
        assert_eq!(
            parse_split_args(&["-b", "100"]).unwrap().byte_size,
            Some(100)
        );
        assert_eq!(parse_split_args(&["-b1000"]).unwrap().byte_size, Some(1000));
        assert_eq!(
            parse_split_args(&["-b", "4k"]).unwrap().byte_size,
            Some(4096)
        );
        assert_eq!(
            parse_split_args(&["-b2m"]).unwrap().byte_size,
            Some(2 * 1024 * 1024)
        );
        // 未给 -b 时 byte_size 为 None（行模式）。
        assert_eq!(parse_split_args(&["f"]).unwrap().byte_size, None);
        // 非法字节数（0 / 非数字）："illegal byte count"。
        assert_eq!(
            parse_split_args(&["-b0"]).err(),
            Some(SplitError::IllegalByteCount("0"))
        );
        assert_eq!(
            parse_split_args(&["-b", "x"]).err(),
            Some(SplitError::IllegalByteCount("x"))
        );
        // -l 与 -b 并存（任一侧）：C 在收尾 `else if (bytecnt…)` 判 usage。
        assert_eq!(
            parse_split_args(&["-l", "3", "-b", "10"]).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&["-b", "10", "-l", "3"]).err(),
            Some(SplitError::Usage)
        );
    }

    #[test]
    fn chunk_bytes_splits_fixed_and_last_short() {
        let data = *b"abcdefghij";
        let c = chunk_bytes(&data, 4);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], b"abcd");
        assert_eq!(c[1], b"efgh");
        assert_eq!(c[2], b"ij");
        assert!(chunk_bytes(&data, 0).is_empty());
        assert!(chunk_bytes(b"", 5).is_empty());
        assert_eq!(chunk_bytes(&data, 20), vec![&data[..]]);
    }

    #[test]
    fn chunks_n_makes_exactly_n_with_remainder_in_last() {
        let data = *b"abcdefghij"; // 10 bytes
        let c = chunks_n(&data, 3).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], b"abc"); // 10/3 = 3
        assert_eq!(c[1], b"def");
        assert_eq!(c[2], b"ghij"); // 末段吸收余数
        // n == len：逐字节。
        assert_eq!(
            chunks_n(b"abc", 3).unwrap(),
            vec![&b"a"[..], &b"b"[..], &b"c"[..]]
        );
        // n > len → None（C "can't split into more than %d files"）。
        assert_eq!(chunks_n(b"ab", 3), None);
        assert_eq!(chunks_n(b"", 1), None);
        assert_eq!(chunks_n(b"abc", 0), None);
    }

    #[test]
    fn parse_chunk_count_minus_n() {
        assert_eq!(parse_split_args(&["-n", "4"]).unwrap().chunk_count, Some(4));
        assert_eq!(parse_split_args(&["-n4"]).unwrap().chunk_count, Some(4));
        assert_eq!(parse_split_args(&["f"]).unwrap().chunk_count, None);
        assert_eq!(
            parse_split_args(&["-n0"]).err(),
            Some(SplitError::IllegalChunkCount("0"))
        );
        // 三模式两两互斥：-n 与 -l / -b 并存均 usage。
        assert_eq!(
            parse_split_args(&["-l", "3", "-n", "4"]).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&["-b", "10", "-n", "4"]).err(),
            Some(SplitError::Usage)
        );
    }

    #[test]
    fn parse_error_paths() {
        // 集外字母交回字母本身，由壳先印 libc 那一行再接 usage（参照件：`-Z` 两行）。
        assert_eq!(
            parse_split_args(&["-z", "f"]).err(),
            Some(SplitError::UnknownOption('z'))
        );
        // 聚簇与 `--x` 只报第一个字符（参照件：`-Zb` 报 `Z`、`--x` 报 `-`）。
        assert_eq!(
            parse_split_args(&["-Zb"]).err(),
            Some(SplitError::UnknownOption('Z'))
        );
        assert_eq!(
            parse_split_args(&["--x"]).err(),
            Some(SplitError::UnknownOption('-'))
        );
        // 缺值是 libc 的另一句（`getopt.c:132-136`），不是 usage 自己那一行。
        assert_eq!(
            parse_split_args(&["-l"]).err(),
            Some(SplitError::MissingValue('l'))
        );
        assert_eq!(
            parse_split_args(&["-b"]).err(),
            Some(SplitError::MissingValue('b'))
        );
        assert_eq!(
            parse_split_args(&["-a"]).err(),
            Some(SplitError::MissingValue('a'))
        );
        assert_eq!(
            parse_split_args(&["-n"]).err(),
            Some(SplitError::MissingValue('n'))
        );
        // 遗留形取的是整段 token 去掉那个 `-`（C 的 `argv[optind] + 1`）：尾巴有杂字符
        // 就整段报成 illegal line count，而不是当成集外字母。
        assert_eq!(
            parse_split_args(&["-12x"]).err(),
            Some(SplitError::IllegalLineCount("12x"))
        );
        assert_eq!(
            parse_split_args(&["-3b5"]).err(),
            Some(SplitError::IllegalLineCount("3b5"))
        );
        // **不重排**：非选项 token 一到，它和后面的整个交回当操作数，
        // 所以 `split in -Z` 里的 `-Z` 是前缀而不是选项（参照件现量）。
        assert_eq!(
            parse_split_args(&["in", "-Z"]).unwrap().operands,
            vec!["in", "-Z"]
        );
        assert_eq!(
            parse_split_args(&["in", "-l"]).unwrap().operands,
            vec!["in", "-l"],
            "操作数之后的 `-l` 不再被当选项读，而是当输出前缀"
        );
        assert_eq!(
            parse_split_args(&["in", "-l"]).unwrap().lines_per_chunk,
            DEFAULT_LINES,
            "没被当选项读就不改列数（C 的 `getopt` 见非选项即返 -1）"
        );
        assert_eq!(
            parse_split_args(&["in", "-l", "3"]).err(),
            Some(SplitError::Usage),
            "不重排之后这是**三个**操作数：档名、前缀、多出来的一支（`split.c:148-149`）"
        );
        assert_eq!(
            parse_split_args(&["a", "b", "c"]).err(),
            Some(SplitError::Usage),
            "多于两个操作数才是 usage 那一支"
        );
        assert_eq!(
            parse_split_args(&["-l0"]).err(),
            Some(SplitError::IllegalLineCount("0"))
        );
        assert_eq!(
            parse_split_args(&["-lx"]).err(),
            Some(SplitError::IllegalLineCount("x"))
        );
        assert_eq!(
            parse_split_args(&["-l", "3", "-l", "4"]).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&["-5", "-l", "3"]).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&["a", "b", "c"]).err(),
            Some(SplitError::Usage)
        ); // >2 操数
    }
}
