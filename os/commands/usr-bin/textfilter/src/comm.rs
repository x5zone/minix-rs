//! The three-way line merge behind `comm`.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 64-160: both operand files must already be sorted; the merge walks
//! them in lockstep — equal lines print in column three, the smaller
//! line prints in its own column and only that file advances — and at
//! either end of input the remaining lines drain into the surviving
//! column. Columns carry tab offsets: one tab for column two, two for
//! column three (comm.c:56, 105-110). `-1`/`-2`/`-3` suppress a column;
//! `-f` compares case-insensitively (`strcasecmp`, 一张**按字节**的 ctype 表). Both files always
//! reach end of input (the loop reads on). [`parse_comm_args`] mirrors
//! C's `getopt` option scanning (`comm.c:80`): clustered letters, the `--`
//! terminator, a lone `-` operand, and an `UnknownOption` for any letter
//! outside `123f` — so an unrecognized flag is never mistaken for a file.

/// Column suppression flags (`-1`/`-2`/`-3`) and the `-f` fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommOptions {
    /// Suppress column one (lines only in the first file).
    pub suppress1: bool,
    /// Suppress column two (lines only in the second file).
    pub suppress2: bool,
    /// Suppress column three (common lines).
    pub suppress3: bool,
    /// `-f`: compare case-insensitively (C: `strcasecmp`).
    pub fold: bool,
}

/// One output line: which column it belongs to and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommLine<'a> {
    /// 1, 2, or 3 — the column the line prints in.
    pub column: u8,
    /// The line itself, **as bytes**: C 的行腿 `getnextln` 用 `getc` 一字节一字节填缓冲
    /// （`comm.c:200-205`）再 `printf("%s%s\n", col, line)`（`:140`），全程不解码。
    pub text: &'a [u8],
}

use alloc::vec::Vec;

/// Merges two sorted line lists. Lines must be newline-stripped; both
/// lists are consumed to the end regardless of order violations (the C
/// assumes sorted input and simply advances the smaller side).
pub fn comm<'a>(a: &'a [&'a [u8]], b: &'a [&'a [u8]], options: &CommOptions) -> Vec<CommLine<'a>> {
    let mut out = Vec::new();
    let mut ia = 0usize;
    let mut ib = 0usize;
    while ia < a.len() || ib < b.len() {
        if ia >= a.len() {
            push(&mut out, 2, options, b[ib]);
            ib += 1;
            continue;
        }
        if ib >= b.len() {
            push(&mut out, 1, options, a[ia]);
            ia += 1;
            continue;
        }
        let ordering = line_cmp(a[ia], b[ib], options.fold);
        match ordering {
            core::cmp::Ordering::Equal => {
                push(&mut out, 3, options, a[ia]);
                ia += 1;
                ib += 1;
            }
            core::cmp::Ordering::Less => {
                push(&mut out, 1, options, a[ia]);
                ia += 1;
            }
            core::cmp::Ordering::Greater => {
                push(&mut out, 2, options, b[ib]);
                ib += 1;
            }
        }
    }
    out
}

fn push<'a>(out: &mut Vec<CommLine<'a>>, column: u8, options: &CommOptions, text: &'a [u8]) {
    let allowed = match column {
        1 => !options.suppress1,
        2 => !options.suppress2,
        _ => !options.suppress3,
    };
    if allowed {
        out.push(CommLine { column, text });
    }
}

/// The comparison: case-folded bytes with `-f`, plain bytes otherwise
/// (the C's `strcoll` is byte order in the C locale).
///
/// `-f` 走的是 `strcasecmp`：两侧各取一字节、按 `unsigned char` 查 `tolower` 表比
/// （`minix3/common/lib/libc/string/strcasecmp.c:58-67`），C locale 下那张表只把
/// `A`–`Z` 换成 `a`–`z`，`0x80`–`0xFF` 原样返回。字符域的 `to_lowercase` 是 **Unicode**
/// 的大小写表：`c3 89`（É）与 `c3 a9`（é）在它眼里是同一对字符，在 C 的那张表里差一位。
fn line_cmp(a: &[u8], b: &[u8], fold: bool) -> core::cmp::Ordering {
    if fold {
        a.iter()
            .map(u8::to_ascii_lowercase)
            .cmp(b.iter().map(u8::to_ascii_lowercase))
    } else {
        a.cmp(b)
    }
}

/// Argument-splitting failure for `comm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommArgError {
    /// An option letter outside the `123f` set (C `getopt` returns `?`).
    UnknownOption(char),
    /// Not exactly the two file operands `comm` requires: C shifts
    /// `argc -= optind` (`comm.c:98`) then `if (argc != 2) usage()`
    /// (`comm.c:101`).
    NotTwoOperands,
}

/// Split `comm`'s arguments (the argv tail, without the program name) into
/// `(options, [file1, file2])`, faithful to C's `getopt(argc, argv, "123f")`
/// (`comm.c:80`): option letters cluster after one dash (`-12`, `-f1`), `--`
/// ends option scanning so everything after it is a file operand, a lone `-`
/// is itself a file operand (standard input), and any other option letter is
/// [`CommArgError::UnknownOption`] rather than a leaked file name. Fewer or more
/// than two operands is [`CommArgError::NotTwoOperands`]. The old front door
/// matched only whole `-1`/`-2`/`-3`/`-f` tokens and pushed everything else
/// (including an unknown `-z`) as an operand, so `comm -z f` opened a file
/// literally named `-z`; this parser routes that to `UnknownOption`.
///
/// 选项环**不重排**（`getopt.c:74-78`）：非选项 token 一到环就结束，它后面的
/// `-1`/`-f` 都是档名，所以 `comm a -1 b` 是三个操作数而不是两个操作数加一个旗标。
/// 这一支的 optstring 里没有带值字母，所以 libc 那句 `option requires an argument`
/// 到不了（同 `rev`/`colrm` 的空 optstring 只有集外字母一形的道理一样：可达出口
/// 由 optstring 定，不由「同族规律」定）。
pub fn parse_comm_args<'a>(
    args: &'a [&'a str],
) -> Result<(CommOptions, [&'a str; 2]), CommArgError> {
    let mut options = CommOptions::default();
    let mut index = 0;
    let operands: Vec<&'a str> = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        if arg == "--" {
            // `getopt.c:81-85`：先 `++optind` 再返回 -1，所以 `--` 自身不是操作数。
            break args[index + 1..].to_vec();
        }
        if arg.len() < 2 || !arg.starts_with('-') {
            // `getopt.c:74-78`：非选项 token 一到就返回 -1 且**不推进** `optind`，
            // 于是它连同后面的 token 全是操作数（旧写法把它推进了环继续扫，所以
            // `comm a -1 b` 里的 `-1` 被当旗标吃掉，参照件量的是三个操作数 → usage）。
            break args[index..].to_vec();
        }
        // 环走**字节**：C 的 `optopt` 是一个 `char`（`getopt.c:79` 的 `optopt = *place++` 一次
        // 进一个字节），所以 `-é`（`c3 a9`）报的是 `c3` 而不是整个字符。之前这里过 `chars()`，
        // 把一个字符的两个字节当成一枚字母交回，报文经 UTF-8 渲染后比 C 多一个字节。
        for byte in arg[1..].bytes() {
            match byte {
                b'1' => options.suppress1 = true,
                b'2' => options.suppress2 = true,
                b'3' => options.suppress3 = true,
                b'f' => options.fold = true,
                other => return Err(CommArgError::UnknownOption(char::from(other))),
            }
        }
        index += 1;
    };
    if operands.len() != 2 {
        return Err(CommArgError::NotTwoOperands);
    }
    Ok((options, [operands[0], operands[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(result: &[CommLine<'_>]) -> Vec<(u8, Vec<u8>)> {
        result
            .iter()
            .map(|line| (line.column, line.text.to_vec()))
            .collect()
    }

    #[test]
    fn test_three_way_merge() {
        let a: [&[u8]; 3] = [b"apple", b"banana", b"cherry"];
        let b: [&[u8]; 3] = [b"banana", b"date", b"fig"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, b"apple".to_vec()),
                (3, b"banana".to_vec()),
                (1, b"cherry".to_vec()),
                (2, b"date".to_vec()),
                (2, b"fig".to_vec()),
            ]
        );
    }

    #[test]
    fn test_suppression_flags() {
        let a: [&[u8]; 2] = [b"common", b"only1"];
        let b: [&[u8]; 2] = [b"common", b"only2"];
        let options = CommOptions {
            suppress1: true,
            suppress3: true,
            ..CommOptions::default()
        };
        assert_eq!(texts(&comm(&a, &b, &options)), vec![(2, b"only2".to_vec())]);
    }

    #[test]
    fn test_fold_compares_case_insensitively() {
        let a: [&[u8]; 1] = [b"Apple"];
        let b: [&[u8]; 2] = [b"apple", b"zebra"];
        let options = CommOptions {
            fold: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![(3, b"Apple".to_vec()), (2, b"zebra".to_vec())]
        );
    }

    #[test]
    fn test_drains_surviving_file_at_either_end() {
        let a: [&[u8]; 1] = [b"early"];
        let b: [&[u8]; 3] = [b"mid1", b"mid2", b"mid3"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, b"early".to_vec()),
                (2, b"mid1".to_vec()),
                (2, b"mid2".to_vec()),
                (2, b"mid3".to_vec()),
            ]
        );
        let a: [&[u8]; 2] = [b"late1", b"late2"];
        let b: [&[u8]; 1] = [b"early"];
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (2, b"early".to_vec()),
                (1, b"late1".to_vec()),
                (1, b"late2".to_vec()),
            ]
        );
    }

    /// 行是字节串：一枚非 UTF-8 字节让**整个档**的行消失是旧写法
    /// （壳里 `from_utf8(input).unwrap_or("")`）的失败形状，C 那边那一行照样是记录
    /// （`comm.c:204-215` 的 `getnextln` 逐 `getc` 读，不解码）。
    #[test]
    fn a_byte_that_is_not_utf8_is_still_a_comparable_line() {
        let left: [&[u8]; 2] = [b"a\xffb", b"common"];
        let right: [&[u8]; 1] = [b"a\xffb"];
        assert_eq!(
            texts(&comm(&left, &right, &CommOptions::default())),
            vec![(3, b"a\xffb".to_vec()), (1, b"common".to_vec())]
        );
    }

    /// `-f` 的折叠是**逐字节查 ctype 表**，不是 Unicode 小写：`strcasecmp` 两侧各按
    /// `unsigned char` 取 `tolower`（`minix3/common/lib/libc/string/strcasecmp.c:58-67`），
    /// 于是 `c3 89`（É）与 `c3 a9`（é）在第二位分开——参照件
    /// `tmp/comm-409/comm_ref` 现量这两行**不**相等（各落自己那列），旧写法经
    /// `str::to_lowercase` 判成公共行落在列三。
    #[test]
    fn the_fold_is_a_per_byte_ctype_lookup_not_a_unicode_lowercase() {
        let left: [&[u8]; 1] = [b"\xc3\x89"];
        let right: [&[u8]; 1] = [b"\xc3\xa9"];
        let fold = CommOptions {
            fold: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&left, &right, &fold)),
            vec![(1, b"\xc3\x89".to_vec()), (2, b"\xc3\xa9".to_vec())]
        );
        // ASCII 那一半不许被波及：`Apple` 与 `apple` 折叠后仍是同一行。
        let left: [&[u8]; 1] = [b"Apple"];
        let right: [&[u8]; 1] = [b"apple"];
        assert_eq!(
            texts(&comm(&left, &right, &fold)),
            vec![(3, b"Apple".to_vec())]
        );
    }

    #[test]
    fn test_parse_two_plain_operands() {
        assert_eq!(
            parse_comm_args(&["a", "b"]),
            Ok((CommOptions::default(), ["a", "b"]))
        );
    }

    #[test]
    fn test_parse_separate_and_clustered_flags() {
        assert_eq!(
            parse_comm_args(&["-1", "-2", "a", "b"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    suppress2: true,
                    ..CommOptions::default()
                },
                ["a", "b"]
            ))
        );
        // getopt clusters valueless letters: `-12f` == `-1 -2 -f`.
        assert_eq!(
            parse_comm_args(&["-12f", "a", "b"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    suppress2: true,
                    fold: true,
                    ..CommOptions::default()
                },
                ["a", "b"]
            ))
        );
    }

    #[test]
    fn test_parse_dash_and_double_dash_are_operands_or_terminator() {
        // A lone `-` is a file operand (standard input).
        assert_eq!(
            parse_comm_args(&["-", "b"]),
            Ok((CommOptions::default(), ["-", "b"]))
        );
        // `--` ends option scanning; the dashes after it are file names.
        assert_eq!(
            parse_comm_args(&["--", "-1", "-2"]),
            Ok((CommOptions::default(), ["-1", "-2"]))
        );
    }

    /// 集外字母交回的是短横后的**第一个字节**，不是第一个字符：`-\u{e9}`（`c3 a9`）落在
    /// `char` 域里是 U+00C3。参照件 `tmp/comm-409/comm_ref` 现量那一行的字母位是单个 `c3`。
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_comm_args(&["-\u{e9}", "a", "b"]),
            Err(CommArgError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // 聚簇同理：`1` 合法先吃掉，报的是其后第一个字节。
        assert_eq!(
            parse_comm_args(&["-1\u{e9}", "a", "b"]),
            Err(CommArgError::UnknownOption('\u{00c3}')),
            "-1é"
        );
        // ASCII 不许被这次改动波及。
        assert_eq!(
            parse_comm_args(&["-Z", "a", "b"]),
            Err(CommArgError::UnknownOption('Z')),
            "-Z"
        );
    }

    #[test]
    fn test_parse_unknown_option_is_not_a_leaked_operand() {
        // The regression: `comm -z file` must reject `-z` (C `getopt` returns
        // `?` → usage), NOT open a file literally named `-z`.
        assert_eq!(
            parse_comm_args(&["-z", "file"]),
            Err(CommArgError::UnknownOption('z'))
        );
        // Even with a full operand count the bad option still errors first.
        assert_eq!(
            parse_comm_args(&["-z", "a", "b"]),
            Err(CommArgError::UnknownOption('z'))
        );
    }

    #[test]
    fn test_parse_requires_exactly_two_operands() {
        assert_eq!(parse_comm_args(&[]), Err(CommArgError::NotTwoOperands));
        assert_eq!(parse_comm_args(&["a"]), Err(CommArgError::NotTwoOperands));
        assert_eq!(
            parse_comm_args(&["a", "b", "c"]),
            Err(CommArgError::NotTwoOperands)
        );
    }

    /// 不重排：第一个非选项 token 一到，选项环当场结束，它后面的 `-1` 是**档名**，
    /// 于是三个 token 全是操作数 → `NotTwoOperands`（参照件 `comm a -1 b` 现量：
    /// 只一行 usage、退 1，那个 `-1` 既没被当旗标、也没被当成坏选项）。
    #[test]
    fn test_parse_stops_at_the_first_operand_without_reordering() {
        assert_eq!(
            parse_comm_args(&["a", "-1", "b"]),
            Err(CommArgError::NotTwoOperands)
        );
        // 两个操作数在前、旗标在后也一样：三个 token 全落进操作数。
        assert_eq!(
            parse_comm_args(&["-", "b", "-f"]),
            Err(CommArgError::NotTwoOperands)
        );
        // 环内的聚簇仍逐字母生效，且 `--` 之后全是档名。
        assert_eq!(
            parse_comm_args(&["-1f", "--", "-2", "-3"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    fold: true,
                    ..CommOptions::default()
                },
                ["-2", "-3"]
            ))
        );
    }
}
