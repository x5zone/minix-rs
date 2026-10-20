//! Byte comparison for `cmp`.
//!
//! Ground truth: `minix3/usr.bin/cmp/` (NetBSD): the compare loop
//! (`regular.c:70-118`) walks both byte streams from the skip offsets
//! with byte and line counters starting at one; the first difference
//! prints "%s %s differ: char %lld, line %lld" and exits 1 (`diffmsg`,
//! misc.c:78-85) unless `-l` lists every difference as
//! "%6lld %3o %3o" (regular.c:98) with only the status leaving; `-s`
//! prints nothing at all. A shorter stream is reported by `eofmsg`
//! (misc.c:59-73): "EOF on %s", or with `-l` "EOF on %s: char %lld,
//! line %lld" — and the EOF names the *shorter* file
//! (`regular.c:111`: `len1 > len2 ? file2 : file1`). Line numbers count
//! the first stream's newlines.

use alloc::string::String;
use alloc::vec::Vec;

/// Everything the doing half needs to render one `cmp` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmpOutcome {
    /// True when the compared ranges are byte identical.
    pub equal: bool,
    /// The first difference as (byte offset from one, line from one).
    pub first_difference: Option<(u64, u64)>,
    /// Every difference as (byte, first-stream byte, second-stream
    /// byte) — populated only when `list_all` was set.
    pub byte_list: Vec<(u64, u8, u8)>,
    /// A stream ended early: (which stream, 1 or 2, plus the byte and
    /// line where the longer one continued).
    pub eof_on: Option<(u8, u64, u64)>,
}

/// Compares two byte streams after their skip offsets have been applied
/// by the caller. `list_all` collects every difference (the `-l`
/// report); without it only the first difference is recorded.
pub fn compare_bytes(a: &[u8], b: &[u8], list_all: bool) -> CmpOutcome {
    let mut outcome = CmpOutcome {
        equal: true,
        first_difference: None,
        byte_list: Vec::new(),
        eof_on: None,
    };
    let mut byte: u64 = 1;
    let mut line: u64 = 1;
    let length = a.len().min(b.len());
    for index in 0..length {
        let (ca, cb) = (a[index], b[index]);
        if ca != cb {
            outcome.equal = false;
            if outcome.first_difference.is_none() {
                outcome.first_difference = Some((byte, line));
            }
            if list_all {
                outcome.byte_list.push((byte, ca, cb));
            } else {
                return outcome;
            }
        }
        if ca == b'\n' {
            line += 1;
        }
        byte += 1;
    }
    if a.len() != b.len() {
        outcome.equal = false;
        let shorter = if a.len() > b.len() { 2 } else { 1 };
        outcome.eof_on = Some((shorter, byte, line));
    }
    outcome
}

/// The `-l` line for one difference: `%6lld %3o %3o` (regular.c:98).
pub fn format_list_entry(entry: (u64, u8, u8), out: &mut String) {
    use core::fmt::Write;
    let (byte, left, right) = entry;
    //[SD19-gate] String 格式化仅内存分配可失败；C regular.c:98 对应 printf 亦不分支处理
    #[allow(clippy::let_underscore_must_use)]
    let _ = write!(out, "{:6} {:3o} {:3o}\n", byte, left, right);
}

/// The first-difference line `misc.c` `diffmsg` sends to standard output:
/// `"%s %s differ: char %lld, line %lld\n"` (with a trailing newline).
pub fn report_first_diff(file1: &[u8], file2: &[u8], byte: u64, line: u64) -> Vec<u8> {
    let tail = alloc::format!(" differ: char {byte}, line {line}\n");
    let mut line = Vec::with_capacity(file1.len() + file2.len() + tail.len() + 1);
    line.extend_from_slice(file1);
    line.push(b' ');
    line.extend_from_slice(file2);
    line.extend_from_slice(tail.as_bytes());
    line
}

/// The early-EOF message `misc.c` `eofmsg` sends to standard error (before the
/// `cmp: ` program prefix the caller adds): without `-l` just `"EOF on <file>"`;
/// with `-l` the byte and — when a line count is meaningful — the line too.
pub fn report_eof(file: &[u8], byte: u64, line: u64, list_all: bool) -> Vec<u8> {
    let tail = if !list_all {
        String::new()
    } else if line > 0 {
        alloc::format!(": char {byte}, line {line}")
    } else {
        alloc::format!(": char {byte}")
    };
    let mut out = Vec::with_capacity("EOF on ".len() + file.len() + tail.len());
    out.extend_from_slice(b"EOF on ");
    out.extend_from_slice(file);
    out.extend_from_slice(tail.as_bytes());
    out
}

/// `cmp` 选项环的读数：两面旗标加**环之后剩下的全部 token**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmpFlags<'a> {
    /// `-l`：把每一处差异逐字节列出来。
    pub list_all: bool,
    /// `-s`：什么都不打，只把结论放在退出码上。
    pub silent: bool,
    /// 选项环交回的 token 序列：`file1 file2 [skip1 [skip2]]`，个数由调用者判。
    pub operands: Vec<&'a [u8]>,
}

/// 选项环的失败形状。`cmp.c:74` 的 optstring 是 `"ls"`——**两个字母都不带值**，
/// 所以 libc 那句 `option requires an argument` 在本支到不了，只可能交回集外字母。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpArgError {
    /// `getopt.c:95-104`：集外字母（libc 那一行由调用者经 [`crate::GetoptReport`] 渲染）。
    UnknownOption(char),
}

/// 把 `cmp` 的 argv 尾部按 `getopt(argc, argv, "ls")`（`cmp.c:74`）拆成 [`CmpFlags`]。
///
/// 逐字母聚簇：`-ls` 与 `-sl` 都是两面旗标同时立起（C 接着用
/// `if (lflag && sflag) errx(...)` 报**一行**、不接 usage，`cmp.c:87-88`），
/// 而 `-lq` 在第二个字母上就落集外一支。旧前门只比整个 token
/// （`arg == "-l"`），所以 `-ls` 掉进「未知选项」并打出 usage——两出口的行数与
/// 句子都不同。`-l -l` 重复给同一面旗标是合法的（C 只是再赋一次 1）。
///
/// **不重排**（`getopt.c:74-78`）：非选项 token 一到就返回 -1 且不推进 `optind`，
/// 它和后面的 token 整个交回当操作数，所以 `cmp 档1 -l 档2` 是**三个操作数**，
/// 那个 `-l` 是第二个档名（参照件现量 `cmp: -l: No such file or directory`、退 2）。
/// `--` 结束环而自身不是操作数；裸 `-` 长度为一，是操作数（`cmp.c:95-113` 认它作标准输入）。
///
/// 操作数个数（`< 2 || > 4`）与 `-l`/`-s` 互斥这一判留在调用者手里：C 的次序是
/// 「选项环 → 互斥 → 个数 → 开档」，句子各归各的出口，换序会把一行的 `errx`
/// 支与两行的 usage 支弄混。
pub fn parse_cmp_flags<'a>(args: &'a [&'a [u8]]) -> Result<CmpFlags<'a>, CmpArgError> {
    let mut flags = CmpFlags {
        list_all: false,
        silent: false,
        operands: Vec::new(),
    };
    let mut index = 0usize;
    flags.operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        if arg == b"--" {
            // `getopt.c:81-85`：先 `++optind` 才返回 -1，所以 `--` 自身不是操作数。
            break args[index + 1..].to_vec();
        }
        if arg.len() < 2 || !arg.starts_with(b"-") {
            // 包括裸 `-`：`getopt.c:87-92` 在 optstring 没有 `-` 时结束环而**不推进**。
            break args[index..].to_vec();
        }
        // 环走**字节**：C 的 `optopt` 是一枚 `char`（`getopt.c:79` 的 `optopt = *place++` 一次进
        // 一个字节），所以 `-é`（`c3 a9`）报的是 `c3` 而不是整个字符。之前过 `chars()` 时把一
        // 个字符的两字节当成一枚字母交回，报文再经 UTF-8 渲染就成了两个字节。
        for &byte in &arg[1..] {
            match byte {
                b'l' => flags.list_all = true,
                b's' => flags.silent = true,
                other => return Err(CmpArgError::UnknownOption(char::from(other))),
            }
        }
        index += 1;
    };
    Ok(flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 集外字母交回的是短横后的**第一个字节**，不是第一个字符：`-\u{e9}`（`c3 a9`）落在
    /// `char` 域里是 U+00C3。参照件 `tmp/cmp-410/cmp_ref` 现量那一行的字母位是单个 `c3`。
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_cmp_flags(&[b"-\xc3\xa9", b"a", b"b"]),
            Err(CmpArgError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // 聚簇同理：`l` 合法先吃掉，报的是其后第一个字节。
        assert_eq!(
            parse_cmp_flags(&[b"-l\xc3\xa9", b"a", b"b"]),
            Err(CmpArgError::UnknownOption('\u{00c3}')),
            "-l\u{e9}"
        );
        // ASCII 那一形不许被这次改动波及。
        assert_eq!(
            parse_cmp_flags(&[b"-q", b"a", b"b"]),
            Err(CmpArgError::UnknownOption('q')),
            "-q"
        );
    }

    #[test]
    fn test_equal_streams() {
        let outcome = compare_bytes(b"same\n", b"same\n", false);
        assert!(outcome.equal);
        assert!(outcome.first_difference.is_none());
        assert!(outcome.eof_on.is_none());
    }

    #[test]
    fn test_first_difference_reports_byte_and_line() {
        let outcome = compare_bytes(b"one\ntwo\n", b"one\nTWO\n", false);
        assert!(!outcome.equal);
        // 't' of two versus 'T' of TWO: byte 5, on line 2.
        assert_eq!(outcome.first_difference, Some((5, 2)));
        assert!(outcome.byte_list.is_empty());
    }

    #[test]
    fn test_report_first_diff_matches_netbsd_format() {
        assert_eq!(
            report_first_diff(b"a", b"b", 5, 2),
            b"a b differ: char 5, line 2\n"
        );
    }

    #[test]
    fn test_report_eof_branches() {
        // Without -l: just the file. With -l: char, and line when nonzero.
        assert_eq!(report_eof(b"b", 3, 0, false), b"EOF on b");
        assert_eq!(report_eof(b"b", 3, 0, true), b"EOF on b: char 3");
        assert_eq!(report_eof(b"b", 9, 2, true), b"EOF on b: char 9, line 2");
    }

    #[test]
    fn test_list_all_collects_every_difference() {
        let outcome = compare_bytes(b"aXcYe\n", b"aBcDe\n", true);
        assert!(!outcome.equal);
        assert_eq!(outcome.byte_list, vec![(2, b'X', b'B'), (4, b'Y', b'D')]);
    }

    #[test]
    fn test_eof_names_the_shorter_stream() {
        // regular.c:111 names the shorter stream: the first stream here.
        let outcome = compare_bytes(b"ab", b"abc", false);
        assert_eq!(outcome.eof_on, Some((1, 3, 1)));
        // Second stream shorter: the EOF names stream two
        // (regular.c:111's ternary picks file2 when len1 > len2).
        let outcome = compare_bytes(b"abc\n", b"ab", false);
        assert_eq!(outcome.eof_on, Some((2, 3, 1)));
    }

    #[test]
    fn test_line_counter_follows_the_first_stream() {
        let outcome = compare_bytes(b"a\nb\nc", b"a\nb\nc\nd\n", true);
        // Five bytes compared (byte counter now 6); the two newlines of
        // stream one put the counter on line 3.
        assert_eq!(outcome.eof_on, Some((1, 6, 3)));
    }

    /// 聚簇逐字母：`-ls` 与 `-sl` 都立起两面旗标（调用者据此走 `errx` 那一行），
    /// `-lq` 在第二个字母上报集外；`-l -l` 重复给同一面旗标合法。
    #[test]
    fn parse_flags_cluster_letter_by_letter() {
        let both = parse_cmp_flags(&[b"-ls", b"a", b"b"]).unwrap();
        assert!(both.list_all && both.silent);
        assert_eq!(both.operands, [b"a".as_slice(), b"b".as_slice()]);
        assert_eq!(
            parse_cmp_flags(&[b"-sl", b"a", b"b"]).unwrap(),
            parse_cmp_flags(&[b"-ls", b"a", b"b"]).unwrap()
        );
        assert_eq!(
            parse_cmp_flags(&[b"-lq", b"a", b"b"]).err(),
            Some(CmpArgError::UnknownOption('q'))
        );
        assert_eq!(
            parse_cmp_flags(&[b"--x"]).err(),
            Some(CmpArgError::UnknownOption('-'))
        );
        let twice = parse_cmp_flags(&[b"-l", b"-l", b"a", b"b"]).unwrap();
        assert!(twice.list_all && !twice.silent);
    }

    /// 环不重排，`--` 结束环而自身不是操作数，裸 `-` 是操作数。
    #[test]
    fn parse_ring_stops_at_the_first_operand() {
        // 参照件：`cmp 档1 -l 档2` 是三个操作数，`-l` 是第二个档名。
        assert_eq!(
            parse_cmp_flags(&[b"f1", b"-l", b"f2"]).unwrap().operands,
            [b"f1".as_slice(), b"-l".as_slice(), b"f2".as_slice()]
        );
        assert_eq!(
            parse_cmp_flags(&[b"-l", b"f1", b"-l", b"f2"])
                .unwrap()
                .operands,
            [b"f1".as_slice(), b"-l".as_slice(), b"f2".as_slice()]
        );
        assert!(!parse_cmp_flags(&[b"--", b"-l", b"-s"]).unwrap().list_all);
        assert_eq!(
            parse_cmp_flags(&[b"--", b"-l", b"-s"]).unwrap().operands,
            [b"-l".as_slice(), b"-s".as_slice()]
        );
        assert_eq!(
            parse_cmp_flags(&[b"-", b"-"]).unwrap().operands,
            [b"-".as_slice(), b"-".as_slice()]
        );
    }
}
