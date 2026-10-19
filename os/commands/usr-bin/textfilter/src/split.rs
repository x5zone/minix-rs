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
//! all wired here (`lines_bytes` / `chunks_bytes` / `chunk_bytes` / `chunks_n`).
//! Both line-mode faces are byte-addressed, as `split2` is: it scans the read
//! buffer for `\n` and never checks an encoding, so undecodable bytes are
//! ordinary data. The two streaming cutters below ([`ByteCutter`] for `-b`,
//! [`LineCutter`] for the line face) carry only counters across reads, so
//! neither one needs the whole input resident.

use alloc::string::String;
use alloc::vec::Vec;

/// Default lines per chunk (`DEFLINE`, split.c:56).
pub const DEFAULT_LINES: usize = 1000;

/// Default suffix length (`sfxlen`, split.c:61).
pub const DEFAULT_SFXLEN: usize = 2;

/// Splits raw input into lines, each one **keeping** the `\n` that ends it
/// (`split.c:262-267`: `split2` writes `Ce - Cs + 1` bytes, so the terminator
/// travels with its line). A tail without a terminator stays without one, and
/// input ending in a newline yields no empty trailing line.
pub fn lines_bytes(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (index, byte) in data.iter().enumerate() {
        if *byte == b'\n' {
            out.push(&data[start..index + 1]);
            start = index + 1;
        }
    }
    if start < data.len() {
        out.push(&data[start..]);
    }
    out
}

/// Computes the chunks: each entry holds that chunk's lines, terminators and
/// all (the last one may be short). `lines_per_chunk` of zero is a caller error
/// the doing half rejects.
pub fn chunks_bytes<'a>(lines: &[&'a [u8]], lines_per_chunk: usize) -> Vec<Vec<&'a [u8]>> {
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
/// short); `n == 0` or empty input yields no chunks. The fixed-size cousin
/// of [`chunks_bytes`], mirroring `split.c` `split1(bytecnt, …)` behind `-b`.
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
                // `default: usage()` 接第二行。递出去的是 `-` 后面的**第一个字节**
                // （聚簇与 `--x` 都只报第一个：量过 `-Zb` 报 `Z`、`--x` 报 `-`；
                // `getopt.c:79` 的 `optopt = *place++` 一次进一个字节、`:103` 的 `%c`
                // 把它原样写出，所以 `é`（`c3 a9`）报的是 `c3`）。
                _ => {
                    return Err(SplitError::UnknownOption(char::from(bytes[1])));
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

    // 第三支操作数在这一腿**不判**。C 的先后是：先把输入开出来（`split.c:135-139`，
    // 开不成当场 `err(1, "%s", *argv)`），再取前缀（`:143`），最后才
    // `if (*argv != NULL) usage()`（`:148-149`）。取数这一层看不见开档的成败，
    // 在这里判就会把 `split 开不了的档名 前缀 第三支` 报成 usage，而参照件报的是那个档名
    // （读数 `tmp/split-427/ref-shapes.log`）。这一判因此在壳侧、开档之后。
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

/// 按字节定长切分的**流式**切块器：把一块一块读进来的输入切成「写给当前分片」与
/// 「另起一枚分片」两种片段，语义照 `minix3/usr.bin/split/split.c:175-236` 的 `split1`。
///
/// C 跨两次 `read` 留下的只有一个计数器（`bcnt`：当前分片已经写了多少字节），
/// 不留任何数据，所以这一层要复写的也只有这两个数：已写字节与「当前分片是否还开着」。
/// 一条关键细节来自 C 的 `else file_open = 0`（`split.c:228-230`）：**正好切满**之后当前
/// 分片就算关掉了，下一次读到的头几个字节属于**新的一枚**分片，而不是接着往刚写完的文件里灌。
pub struct ByteCutter {
    size: usize,
    filled: usize,
    open: bool,
}

/// 一段要写出去的字节，以及它该落在哪一枚分片：`opens_new_file` 为真就先把上一枚
/// 关掉、后缀推进一位再新建（C 的 `newfile()`，`split.c:309-335`）。
#[derive(Debug, PartialEq, Eq)]
pub struct CutPiece<'a> {
    pub bytes: &'a [u8],
    pub opens_new_file: bool,
}

impl ByteCutter {
    /// 以每片 `size` 字节建一个切块器。`size` 为零按调用方的错处理，
    /// 与 [`chunk_bytes`] 对 `n == 0` 交出空结果同一个口径。
    pub fn new(size: usize) -> Self {
        Self {
            size,
            filled: 0,
            open: false,
        }
    }

    /// 把一次读到的 `piece` 切成若干片段，顺序就是写出顺序。
    pub fn take<'a>(&mut self, piece: &'a [u8]) -> Vec<CutPiece<'a>> {
        let mut out = Vec::new();
        if self.size == 0 {
            return out;
        }
        let mut rest = piece;
        while !rest.is_empty() {
            let opens_new_file = !self.open;
            if opens_new_file {
                self.open = true;
                self.filled = 0;
            }
            let room = self.size - self.filled;
            let give = room.min(rest.len());
            out.push(CutPiece {
                bytes: &rest[..give],
                opens_new_file,
            });
            self.filled += give;
            rest = &rest[give..];
            if self.filled == self.size {
                self.open = false;
                self.filled = 0;
            }
        }
        out
    }
}

/// 按行数定长切分的**流式**切块器：把一块一块读进来的输入切成「写给当前分片」与
/// 「另起一枚分片」两种片段，语义照 `minix3/usr.bin/split/split.c:242-282` 的 `split2`。
///
/// 与 [`ByteCutter`] 同一个协议，只是切点从字节计数换成换行计数。跨两次 `read`
/// 留下的还是两个数（当前分片已数到几整行、当前分片是否还开着），**一段数据都不留**：
/// 横跨两个缓冲的那一行分两次写进同一枚还开着的分片，所以这里没有、也不该有
/// 拼接缓存——那会把驻留量重新拉回「最长那一行」。
pub struct LineCutter {
    numlines: usize,
    lines: usize,
    open: bool,
}

impl LineCutter {
    /// 以每片 `numlines` 行建一个切块器。`numlines` 为零按调用方的错处理，
    /// 与 [`ByteCutter::new`] 对 `size` 为零同一个口径。
    pub fn new(numlines: usize) -> Self {
        Self {
            numlines,
            lines: 0,
            open: false,
        }
    }

    /// 把一次读到的 `piece` 切成若干片段，顺序就是写出顺序。
    pub fn take<'a>(&mut self, piece: &'a [u8]) -> Vec<CutPiece<'a>> {
        let mut out = Vec::new();
        if self.numlines == 0 {
            return out;
        }
        let mut start = 0usize;
        for (index, byte) in piece.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            self.lines += 1;
            if self.lines != self.numlines {
                continue;
            }
            self.lines = 0;
            out.push(CutPiece {
                bytes: &piece[start..index + 1],
                opens_new_file: !self.open,
            });
            // 切点之后就不再有「还开着的分片」了：下一段自己按 `!open` 决定要不要
            // 另起一枚，于是「切点落在片段正中」与「切点正好压在片段末」共用一条路。
            self.open = false;
            start = index + 1;
        }
        if start < piece.len() {
            out.push(CutPiece {
                bytes: &piece[start..],
                opens_new_file: !self.open,
            });
            self.open = true;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_chunks_of_thousand() {
        let mut data = Vec::new();
        for _ in 0..2500 {
            data.extend_from_slice(b"line\n");
        }
        let lines = lines_bytes(&data);
        let chunks = chunks_bytes(&lines, DEFAULT_LINES);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 1000);
        assert_eq!(chunks[1].len(), 1000);
        assert_eq!(chunks[2].len(), 500);
        assert_eq!(chunks[2][499], &data[data.len() - 5..]);
    }

    #[test]
    fn test_explicit_count_and_empty_input() {
        let data = *b"1\n2\n3\n4\n5\n";
        let lines = lines_bytes(&data);
        let groups = chunks_bytes(&lines, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[2], vec![&b"5\n"[..]]);
        assert!(chunks_bytes(&[], 10).is_empty());
    }

    #[test]
    fn test_zero_count_is_no_chunks() {
        let lines = lines_bytes(b"a\n");
        assert!(chunks_bytes(&lines, 0).is_empty());
    }

    #[test]
    fn lines_bytes_carries_the_terminator_with_its_line() {
        assert_eq!(lines_bytes(b"a\nb\n"), vec![&b"a\n"[..], &b"b\n"[..]]);
        // 空行是它自己的一行，终止符就是它的全部内容；末行不带换行则保持不带。
        assert_eq!(
            lines_bytes(b"a\n\nb\n"),
            vec![&b"a\n"[..], &b"\n"[..], &b"b\n"[..]]
        );
        assert_eq!(lines_bytes(b"a\nb"), vec![&b"a\n"[..], &b"b"[..]]);
        assert_eq!(lines_bytes(b"a"), vec![&b"a"[..]]);
        assert!(lines_bytes(b"").is_empty());
    }

    #[test]
    fn lines_bytes_is_not_an_encoding_check() {
        // 不可解码的字节只是普通数据（C 数的是读缓冲里的换行）。
        assert_eq!(
            lines_bytes(b"aa\xffbb\ncc\xfe\n"),
            vec![&b"aa\xffbb\n"[..], &b"cc\xfe\n"[..]]
        );
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

    /// 集外字母交回的是短横后的**第一个字节**而不是第一个字符：`-\u{e9}`（`c3 a9`）在
    /// `char` 域里是 U+00C3。绝对读数有两处：机制层是 `getopt.c:79` 的
    /// `optopt = *place++`（步进在 `char *` 上，一次一个字节）与 `:103` 的 `%c`，
    /// 参照件那侧是 `tmp/split-413/split_ref` 现量 `-é` 打的是单个 `303`
    /// （逐字节门在 `tests/split_face.rs`）。
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_split_args(&["-\u{e9}"]).err(),
            Some(SplitError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // 环停在第一个坏字母上：`-Zé` 只报 `Z`。
        assert_eq!(
            parse_split_args(&["-Z\u{e9}"]).err(),
            Some(SplitError::UnknownOption('Z'))
        );
        // ASCII 那一形不许被这次改动波及。
        assert_eq!(
            parse_split_args(&["-q"]).err(),
            Some(SplitError::UnknownOption('q'))
        );
        // 反面：带值字母把其后的字节整个当值收走，轮不到字母位（参照件同句
        // `split: é: illegal line count.`，退 1、不接 usage）。
        assert_eq!(
            parse_split_args(&["-l\u{e9}"]).err(),
            Some(SplitError::IllegalLineCount("é"))
        );
    }

    #[test]
    fn parse_error_paths() {
        // 集外字母交回字母本身，由壳先印 libc 那一行再接 usage（参照件：`-Z` 两行）。
        assert_eq!(
            parse_split_args(&["-z", "f"]).err(),
            Some(SplitError::UnknownOption('z'))
        );
        // 聚簇与 `--x` 只报第一个字节（参照件：`-Zb` 报 `Z`、`--x` 报 `-`）。
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
            parse_split_args(&["in", "-l", "3"]).unwrap().operands,
            vec!["in", "-l", "3"],
            "不重排之后这是**三个**操作数：档名、前缀、多出来的一支。取数这一层不判它，\
             因为 C 的 `usage()`（`split.c:148-149`）排在开档（`:135-139`）之后，\
             而开档的成败只有壳侧知道"
        );
        assert_eq!(
            parse_split_args(&["a", "b", "c"]).unwrap().operands,
            vec!["a", "b", "c"],
            "多于两个操作数在库里只是三个操作数，那一记 usage 由壳侧在开档之后落下"
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
    }

    /// 切块器与整档切块必须逐字节等价：同一份输入，随便按哪些边界切成几次读，
    /// 把片段按 `opens_new_file` 归成文件，结果要与 `chunk_bytes` 一次切完完全相同。
    /// 这一枚就是流式化「不改可见行为」的证明，读边界取的是「跨片」「正好切满」
    /// 「一次读横跨好几片」三种形状。
    #[test]
    fn the_cutter_agrees_with_the_whole_buffer_cut_at_any_read_boundary() {
        let data: Vec<u8> = (0u8..40).collect();
        for size in [1usize, 3, 5, 8, 40, 41] {
            for boundaries in [
                vec![data.len()],
                vec![1, 1, 1, data.len() - 3],
                vec![7, 7, 7, 7, 7, 5],
                vec![5, 5, 5, 5, 5, 5, 5, 5],
                vec![13, 13, 14],
                vec![40],
                vec![20, 20],
                vec![41],
            ] {
                let mut cutter = ByteCutter::new(size);
                let mut files: Vec<Vec<u8>> = Vec::new();
                let mut offset = 0usize;
                for step in &boundaries {
                    let piece = &data[offset..(offset + step).min(data.len())];
                    offset += step;
                    for part in cutter.take(piece) {
                        if part.opens_new_file || files.is_empty() {
                            files.push(Vec::new());
                        }
                        files.last_mut().unwrap().extend_from_slice(part.bytes);
                    }
                    if offset >= data.len() {
                        break;
                    }
                }
                let whole = chunk_bytes(&data, size)
                    .into_iter()
                    .map(|chunk| chunk.to_vec())
                    .collect::<Vec<Vec<u8>>>();
                assert_eq!(files, whole, "size={size} boundaries={boundaries:?}");
            }
        }
    }

    /// 正好切满之后当前分片就算关掉：下一次读到的头几个字节必须属于**新的一枚**分片。
    /// 这条来自 `split.c:228-230` 的 `else file_open = 0`，是本层最容易写错的一形。
    #[test]
    fn an_exact_fill_closes_the_file_so_the_next_read_starts_a_new_one() {
        let head = [1u8, 2, 3, 4];
        let tail = [5u8, 6];
        let mut cutter = ByteCutter::new(4);
        let first = cutter.take(&head);
        assert_eq!(
            first,
            vec![CutPiece {
                bytes: &[1, 2, 3, 4],
                opens_new_file: true
            }]
        );
        let second = cutter.take(&tail);
        assert_eq!(
            second,
            vec![CutPiece {
                bytes: &[5, 6],
                opens_new_file: true
            }],
            "切满之后再读，不许接着灌上一枚分片"
        );
    }

    /// 一次读横跨好几片时每片各起一枚文件，且**不留零长度的尾巴**
    /// （C 的 `if (len)` 就是这个意思，`split.c:221-230`）。
    #[test]
    fn one_read_spanning_several_chunks_leaves_no_empty_tail() {
        let mut cutter = ByteCutter::new(4);
        let ten: Vec<u8> = (0u8..10).collect();
        let pieces = cutter.take(&ten);
        assert_eq!(pieces.len(), 3, "10 字节按 4 切是 4/4/2 三段");
        assert!(pieces.iter().all(|p| p.opens_new_file));
        assert_eq!(pieces[0].bytes.len(), 4);
        assert_eq!(pieces[1].bytes.len(), 4);
        assert_eq!(pieces[2].bytes.len(), 2);
        assert!(!pieces.iter().any(|p| p.bytes.is_empty()));
    }

    /// 读进来的不足一片时，后来的那一块要**接在同一枚**分片里：
    /// 这是跨读留存面唯一真正起作用的一形（C 用 `bcnt` 记这一步）。
    #[test]
    fn a_short_read_continues_inside_the_same_chunk_file() {
        let head = [1u8, 2, 3];
        let rest = [4u8, 5, 6, 7, 8, 9, 10];
        let mut cutter = ByteCutter::new(8);
        let first = cutter.take(&head);
        assert_eq!(first.len(), 1);
        assert!(first[0].opens_new_file);
        let second = cutter.take(&rest);
        assert_eq!(second.len(), 2, "补足 8 之后再起新的一枚");
        assert!(!second[0].opens_new_file);
        assert_eq!(second[0].bytes, &[4, 5, 6, 7, 8]);
        assert!(second[1].opens_new_file);
        assert_eq!(second[1].bytes, &[9, 10]);
    }

    /// `size` 为零是调用方的错，与 `chunk_bytes` 同一个口径：什么都切不出来，
    /// 而不是把输入静默吃掉或原地打转。
    #[test]
    fn a_zero_sized_cutter_produces_no_pieces() {
        let mut cutter = ByteCutter::new(0);
        assert!(cutter.take(&[1, 2, 3]).is_empty());
        assert_eq!(chunk_bytes(&[1u8, 2, 3], 0), Vec::<&[u8]>::new());
    }

    /// 把 [`LineCutter`] 逐片交回的片段**严格按 `opens_new_file`** 归成文件。字节腿那枚
    /// 等价门留了 `|| files.is_empty()` 的逃逸，这里不留：第一片就该自己说得出「另起
    /// 一枚」，否则「一枚文件都没开」与「开档时机写错了」在门里长得一样。
    fn streaming_files(data: &[u8], piece_len: usize, numlines: usize) -> Vec<Vec<u8>> {
        let mut cutter = LineCutter::new(numlines);
        let mut files: Vec<Vec<u8>> = Vec::new();
        let mut offset = 0usize;
        while offset < data.len() {
            let piece = &data[offset..(offset + piece_len).min(data.len())];
            offset += piece_len;
            for part in cutter.take(piece) {
                assert!(!part.bytes.is_empty(), "切块器不许交回零长度的片段");
                if part.opens_new_file {
                    files.push(Vec::new());
                }
                files
                    .last_mut()
                    .expect("片段必属某一枚分片")
                    .extend_from_slice(part.bytes);
            }
        }
        files
    }

    /// 整档一次切完的文件序列，即流式那条腿的参照半（与 `chunk_bytes` 给字节腿
    /// 当参照同一个办法）。
    fn whole_files(data: &[u8], numlines: usize) -> Vec<Vec<u8>> {
        chunks_bytes(&lines_bytes(data), numlines)
            .into_iter()
            .map(|chunk| chunk.concat())
            .collect()
    }

    /// 形状集合：空、无换行的一行、单个空行、末行不带换行、正好切在换行上、
    /// 空行夹在中间、不可解码的字节当数据。这一组长度都在 40 字节以内，
    /// 所以下面那枚门能把片长**逐个**试过来。
    fn small_shapes() -> Vec<Vec<u8>> {
        vec![
            Vec::new(),
            b"a".to_vec(),
            b"\n".to_vec(),
            b"a\nb\nc".to_vec(),
            b"a\nb\nc\n".to_vec(),
            b"a\n\nb\n".to_vec(),
            b"aa\xffbb\ncc\xfe\ndd\n".to_vec(),
            (0..40u8)
                .map(|i| if i % 4 == 3 { b'\n' } else { b'x' })
                .collect(),
        ]
    }

    /// 等价门（流式化「不改可见行为」的证明，小形状逐个片长扫）：同一份输入，
    /// 随便按哪个边界成块读进来，交回的文件必须与整档一次切完**逐字节**相同。
    /// 片长取遍 `1..=len+1`，于是「切点正好压在片段末」「一行跨两片」「末字节就是
    /// 换行」这些形状不需要人挨个点。
    #[test]
    fn the_line_cutter_agrees_with_the_whole_buffer_cut_at_any_read_boundary() {
        for data in small_shapes() {
            for numlines in [1usize, 2, 3, 5] {
                let expected = whole_files(&data, numlines);
                for piece_len in 1..=data.len() + 1 {
                    assert_eq!(
                        streaming_files(&data, piece_len, numlines),
                        expected,
                        "numlines={numlines} piece_len={piece_len} len={}",
                        data.len()
                    );
                }
            }
        }
    }

    /// 同一件事在大形状上再走一遍：这些形状长到没法逐个试片长，于是取一组
    /// 有代表性的读边界——一字节一读、跨过切点、正好压在行末、跨读块、比输入还长。
    /// 其中 97 字节一行与 60 字节一行是专门留给「一行横跨两次读」的。小形状不在
    /// 此列：上一枚门已把它们按每个片长扫过，重复扫一遍只是把同一件事测两次。
    #[test]
    fn the_line_cutter_agrees_on_the_big_shapes_at_the_awkward_read_sizes() {
        let lines = |width: usize, count: usize, mark: u8| -> Vec<u8> {
            (0..(width * count))
                .map(|i| if i % width == width - 1 { b'\n' } else { mark })
                .collect()
        };
        let shapes = vec![
            (0..401u32)
                .map(|i| if i % 100 == 99 { b'\n' } else { b'q' })
                .collect(),
            lines(97, 41, b'l'),
            lines(60, 1200, b'w'),
        ];
        for data in &shapes {
            for numlines in [1usize, 3, 64, 100, 1000] {
                let expected = whole_files(data, numlines);
                for piece_len in [
                    1usize,
                    2,
                    3,
                    5,
                    7,
                    8,
                    13,
                    16,
                    40,
                    64,
                    97,
                    100,
                    128,
                    1000,
                    4096,
                    65536,
                    65537,
                    data.len(),
                    data.len() + 1,
                ] {
                    assert_eq!(
                        streaming_files(data, piece_len, numlines),
                        expected,
                        "numlines={numlines} piece_len={piece_len} len={}",
                        data.len()
                    );
                }
            }
        }
    }

    /// 真实读块形状：C 每次 `read` 一个 `MAXBSIZE`（本壳 `STREAM_CHUNK` = 64 KiB），
    /// 1 KiB 的行凑满 64 行正好是 64 KiB。于是切点**恰好**压在两次的读边界上——
    /// 这一形只能有两枚分片，多出第三枚（空尾）就是把「切点占末字节」写错了。
    #[test]
    fn the_real_read_block_shape_makes_two_files_not_three() {
        let data: Vec<u8> = (0..(1024 * 128u32))
            .map(|i| if i % 1024 == 1023 { b'\n' } else { b'z' })
            .collect();
        let files = streaming_files(&data, 64 * 1024, 64);
        assert_eq!(files.len(), 2, "128 KiB 的 1 KiB 行按 64 行切是两枚");
        assert!(files.iter().all(|file| file.len() == 64 * 1024));
        assert_eq!(files, whole_files(&data, 64));
    }

    /// 一行横跨两次读：两截必须写进**同一枚**还开着的分片（C 的 `if (Cs < Ce)`
    /// 就是这一形，`split.c:276-280`），不许因为换行还没来就另起一枚。
    #[test]
    fn a_line_straddling_two_reads_stays_in_one_chunk_file() {
        let mut cutter = LineCutter::new(1);
        let head = cutter.take(b"a");
        assert_eq!(
            head,
            vec![CutPiece {
                bytes: b"a",
                opens_new_file: true
            }]
        );
        let body = cutter.take(b"b");
        assert_eq!(
            body,
            vec![CutPiece {
                bytes: b"b",
                opens_new_file: false
            }],
            "行还没结束，不许另起一枚"
        );
        let tail = cutter.take(b"c\n");
        assert_eq!(
            tail,
            vec![CutPiece {
                bytes: b"c\n",
                opens_new_file: false
            }],
            "换行落在这一次读里，收尾仍属同一枚分片"
        );
    }

    /// 切点正好压在片段末：此刻下一枚分片**还不该开**，要等下一次真读到字节才开
    /// （C 的 `if (len) newfile(); else file_open = 0;`，`split.c:271-274`）。
    /// 读到这里就失手也就停在这一枚上，不会多出一枚空文件。
    #[test]
    fn a_cut_on_the_last_byte_of_a_read_leaves_the_next_file_unopened() {
        let mut cutter = LineCutter::new(1);
        assert_eq!(
            cutter.take(b"a\n"),
            vec![CutPiece {
                bytes: b"a\n",
                opens_new_file: true
            }]
        );
        assert!(
            cutter.take(b"").is_empty(),
            "没有字节进来就不该开下一枚分片"
        );
        assert_eq!(
            cutter.take(b"b\n"),
            vec![CutPiece {
                bytes: b"b\n",
                opens_new_file: true
            }],
            "下一次真读到字节，才由那一段自己宣布另起一枚"
        );
    }

    /// 一次读里没有换行：整段灌进当前分片，且只有当前没有开着的分片时才另起一枚。
    #[test]
    fn a_piece_without_any_newline_fills_the_open_chunk() {
        let mut cutter = LineCutter::new(2);
        assert_eq!(
            cutter.take(b"ab"),
            vec![CutPiece {
                bytes: b"ab",
                opens_new_file: true
            }]
        );
        assert_eq!(
            cutter.take(b"cd"),
            vec![CutPiece {
                bytes: b"cd",
                opens_new_file: false
            }]
        );
    }

    /// 一次读横跨好几个切点时每个切点各起一枚文件，且段段非空
    /// （`numlines` 为一是最容易写出零长度尾巴的形状）。
    #[test]
    fn several_cuts_in_one_piece_open_one_file_each() {
        let mut cutter = LineCutter::new(1);
        let pieces = cutter.take(b"a\nb\nc\n");
        assert_eq!(pieces.len(), 3);
        assert!(
            pieces.iter().all(|part| part.opens_new_file),
            "三个切点各起一枚"
        );
        assert!(pieces.iter().all(|part| !part.bytes.is_empty()));
        assert_eq!(
            pieces.iter().map(|part| part.bytes).collect::<Vec<_>>(),
            vec![&b"a\n"[..], &b"b\n"[..], &b"c\n"[..]]
        );
    }

    /// `numlines` 为零是调用方的错，与字节腿同一个口径：什么都切不出来。
    #[test]
    fn a_zero_line_count_cutter_produces_no_pieces() {
        let mut cutter = LineCutter::new(0);
        assert!(cutter.take(b"a\nb\n").is_empty());
        assert!(whole_files(b"a\nb\n", 0).is_empty());
    }
}
