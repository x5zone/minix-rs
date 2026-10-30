//! Column removal for `colrm`.
//!
//! (Module note: `Vec` here comes from `alloc` via the crate root.)
//!
//! Ground truth: `minix3/usr.bin/colrm/colrm.c` (NetBSD), main at lines
//! 61-121: `colrm [start [stop]]`. Every input byte advances a display
//! column — printable bytes add one, a tab rounds up to the next
//! multiple of eight (`TAB`, line 55), a backspace pulls one back
//! (never below zero), a newline resets — and the byte survives when
//! the *new* column lands outside `[start, stop]`. With only `start`,
//! columns from `start` onward are removed; with no operands
//! everything passes through. Column one is the first byte of the line.
//!
//! The operand face (`colrm.c:76-95`) is here too: which operand gets
//! rejected, in what order, and what `strtol` accepts. The option ring
//! is the crate-wide empty-optstring one (`crate::empty_optstring_operands_bytes`)
//! because `colrm.c:67` writes `getopt(argc, argv, "")` exactly like
//! `rev.c:70`.

use alloc::vec::Vec;

/// Removes columns `[start, stop]` (1 based; `stop` zero means through
/// end of line) from the input, mirroring the C byte walk: the display
/// column tracks tabs (next multiple of 8), backspaces (one back, not
/// below zero), and newlines (reset), and a byte is kept exactly when
/// its new column is outside the removed range.
pub fn filter_column(input: &[u8], start: usize, stop: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut column = 0usize;
    for &byte in input {
        let new_column = match byte {
            0x08 => column.saturating_sub(1),
            b'\n' => 0,
            b'\t' => (column + 8) & !7,
            _ => column + 1,
        };
        let keep = start == 0 || new_column < start || (stop != 0 && new_column > stop);
        if keep {
            out.push(byte);
        }
        column = new_column;
    }
    out
}

/// 操作数段的三种拒收，交回给壳去印各自的句子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColrmOperand<'a> {
    /// `colrm.c:90-91`：操作数多于两个，落 `default: usage()`（一行、退 1、没有别的句子）。
    TooManyOperands,
    /// `colrm.c:94-95`：`err(1, "illegal start and stop columns")`。`err` 而不是 `errx`，
    /// 所以那一行还缀一句 `strerror(errno)`，而此刻 `errno` 是 0。
    StartAfterStop,
    /// `colrm.c:80-86`：`errx(1, "illegal column -- %s", <操作数原文>)`。`errx` 不缀
    /// `strerror`，报的是操作数原文（参照件量过：`colrm: illegal column -- 3x`）。
    IllegalColumn(&'a [u8]),
}

/// C `colrm.c:76-92` 的操作数段：交回 `(start, stop)`，`stop` 为 0 表示「到行末」。
///
/// 两支都坏时报的是**后**写的那支：C 的 `case 2:` 先对 `argv[1]` 做 `strtol` 并检查，
/// 然后 `/* FALLTHROUGH */` 才轮到 `argv[0]`（`:78-87`），所以 `colrm abc def` 印的是
/// `def`。这一条是本函数最容易写反的地方，参照件逐形量过。
pub fn parse_columns<'a>(argv: &[&'a [u8]]) -> Result<(usize, usize), ColrmOperand<'a>> {
    match argv.len() {
        0 => Ok((0, 0)),
        1 => match column_value(argv[0]) {
            Some(start) => Ok((start, 0)),
            None => Err(ColrmOperand::IllegalColumn(argv[0])),
        },
        2 => {
            let stop = match column_value(argv[1]) {
                Some(stop) => stop,
                None => return Err(ColrmOperand::IllegalColumn(argv[1])),
            };
            let start = match column_value(argv[0]) {
                Some(start) => start,
                None => return Err(ColrmOperand::IllegalColumn(argv[0])),
            };
            // C 写的是 `if (stop && start > stop)`（`:94`）：`stop` 为 0 时不比，
            // 而本节里 `stop` 只可能来自两个操作数，`column_value` 已经排除了 0。
            if start > stop {
                Err(ColrmOperand::StartAfterStop)
            } else {
                Ok((start, stop))
            }
        }
        _ => Err(ColrmOperand::TooManyOperands),
    }
}

/// C 的 `strtol(word, &p, 10)` 加一句 `if (<值> <= 0 || *p)` 的合并判定。
///
/// 接受面比「一串数字」宽，也比它窄，两头都得照 C：
/// ① 前导空白跳过、可带 `+`/`-` 号、至少一位十进制数字（`colrm " 3"`、`colrm +3` 都收，
/// 参照件量过）；② 数字之后不许剩字符（`3x`、`0x10`、`1e2`、`3 ` 全部拒收，因为 `*p`
/// 非零）；③ 溢出饱和到 `long` 的范围而不是报错（`99999999999999999999999` 被接受）；
/// ④ `colrm.c:63` 把结果存进 `u_long`，所以 C 的 `<= 0` 实际只挡得住 0——负数回绕成
/// 极大值后被接受，并且因为列号远小于它而整行透传（`colrm -- -5` 退 0，参照件量过）。
/// 本函数把这条回绕如实搬过来，不替 C「修正」。
fn column_value(word: &[u8]) -> Option<usize> {
    let bytes = word;
    let mut cursor = 0;
    while matches!(
        bytes.get(cursor),
        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(0x0b) | Some(0x0c) | Some(b'\r')
    ) {
        cursor += 1;
    }
    let negative = match bytes.get(cursor) {
        Some(b'+') => {
            cursor += 1;
            false
        }
        Some(b'-') => {
            cursor += 1;
            true
        }
        _ => false,
    };
    let digits_begin = cursor;
    let mut magnitude: u64 = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        magnitude = magnitude
            .saturating_mul(10)
            .saturating_add(u64::from(bytes[cursor] - b'0'));
        cursor += 1;
    }
    // 一位数字都没有、或数字之后还剩别的字符，都落 C 的 `*p` 非零那一支。
    if cursor == digits_begin || cursor != bytes.len() {
        return None;
    }
    // `long` 的饱和范围：溢出时 `strtol` 交回 `LONG_MAX`/`LONG_MIN`（绝对值差一，
    // 两边各自封顶），随后 C 把它存进 `u_long` 就是这张二补码脸。取这些极大值后，
    // 列号判断一律「比任何真实列号都大」，与参照件的整行透传一致。
    let saturated: u64 = if negative {
        magnitude.min(1u64 << 63)
    } else {
        magnitude.min(i64::MAX as u64)
    };
    let bits = if negative {
        0u64.wrapping_sub(saturated)
    } else {
        saturated
    };
    // 三块目标板与宿主都是 64 位，所以 `u_long` 的宽度就是 `usize` 的宽度。
    let as_ulong = bits as usize;
    if as_ulong == 0 {
        return None;
    }
    Some(as_ulong)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_columns_names_a_raw_ff_operand_by_its_own_bytes() {
        assert_eq!(
            parse_columns(&[&b"1\xff3"[..]]),
            Err(ColrmOperand::IllegalColumn(b"1\xff3".as_slice())),
            "回显带的是原字节，不是替换字符的三字节"
        );
    }

    #[test]
    fn column_value_rejects_a_raw_ff_byte() {
        assert_eq!(
            column_value(b"3\xff".as_slice()),
            None,
            "数字之后剩字节就是 *p 非零"
        );
    }

    /// 字节形入口的测试适配器：夹具仍写文本，只在进解析口前折一次。
    fn argv_of<'w>(list: &[&'w str]) -> Vec<&'w [u8]> {
        list.iter().map(|word| word.as_bytes()).collect()
    }

    #[test]
    fn test_range_removal() {
        // Columns 2-4 (b, c, d) drop; a, e, f survive.
        assert_eq!(filter_column(b"abcdef", 2, 4), b"aef");
    }

    #[test]
    fn test_start_only_removes_to_end_of_line() {
        // colrm 2 — no stop means "to end of line": stop stays zero.
        assert_eq!(filter_column(b"abcdef", 2, 0), b"a");
    }

    #[test]
    fn test_no_operands_passes_everything_through() {
        assert_eq!(filter_column(b"abc\tdef\n", 0, 0), b"abc\tdef\n");
    }

    #[test]
    fn test_tab_quantizes_the_column() {
        // 'a' sits at column 1; the tab itself rounds the display column
        // to 8 (inside the removed range 2..8) and is dropped, while 'b'
        // lands at column 9 and survives.
        assert_eq!(filter_column(b"a\tb", 2, 8), b"ab");
    }

    #[test]
    fn test_backspace_pulls_the_column_back() {
        // ab at columns 1-2, two backspaces pull the column back to 0,
        // then X lands at column 1: only b is in the removed range 2...
        assert_eq!(filter_column(b"ab\x08\x08X", 2, 0), b"a\x08\x08X");
    }

    #[test]
    fn test_newline_resets_the_column() {
        // The newline itself is column zero and survives; line two's
        // columns restart at one, so only d (column two) is removed.
        assert_eq!(filter_column(b"ab\ncd", 2, 3), b"a\nc");
    }

    #[test]
    fn test_column_one_boundary_keeps_nothing_in_range() {
        assert_eq!(filter_column(b"abc", 1, 1), b"bc");
    }

    #[test]
    fn test_empty_input_yields_empty_output() {
        assert!(filter_column(b"", 1, 4).is_empty());
    }

    /// 操作数的形状逐个钉死：零拒收、空白与 `+` 号接受、尾巴有字符拒收、
    /// 溢出饱和后被接受，负数经 `u_long` 回绕后也被接受（C 的怪癖，如实搬）。
    #[test]
    fn column_operands_follow_strtol_rather_than_a_digit_string() {
        assert_eq!(column_value(b"3".as_slice()), Some(3));
        assert_eq!(column_value(b"03".as_slice()), Some(3));
        assert_eq!(
            column_value(b" 3".as_slice()),
            Some(3),
            "前导空白是 strtol 的一部分"
        );
        assert_eq!(
            column_value(b"\t\n3".as_slice()),
            Some(3),
            "isspace 全集都跳"
        );
        assert_eq!(
            column_value(b"+3".as_slice()),
            Some(3),
            "显式正号 strtol 收"
        );
        assert_eq!(column_value(b"100".as_slice()), Some(100));
        // 溢出饱和：strtol 交回 LONG_MAX 而 errno=ERANGE，C 的两个检查都不拦。
        assert_eq!(
            column_value(b"99999999999999999999999".as_slice()),
            Some(i64::MAX as usize)
        );
        // 负数回绕成 u_long 的极大值——`<= 0` 挡不住它（参照件 `colrm -- -5` 退 0）。
        assert_eq!(column_value(b"-5".as_slice()), Some(-5i64 as usize));
        assert_eq!(
            column_value(b"-99999999999999999999999".as_slice()),
            Some(i64::MAX as usize + 1),
            "饱和到 LONG_MIN 后回绕"
        );
        // 拒收的四形：零、什么都没有、尾巴有字符、数字之后跟的不是结尾。
        assert_eq!(column_value(b"0".as_slice()), None);
        assert_eq!(column_value(b"+0".as_slice()), None);
        assert_eq!(column_value(b"-0".as_slice()), None);
        assert_eq!(column_value(b"".as_slice()), None);
        assert_eq!(column_value(b"3x".as_slice()), None);
        assert_eq!(column_value(b"0x10".as_slice()), None);
        assert_eq!(column_value(b"1e2".as_slice()), None);
        assert_eq!(
            column_value(b"3 ".as_slice()),
            None,
            "尾随空白不在 strtol 的接受面里"
        );
        assert_eq!(column_value(b"abc".as_slice()), None);
        assert_eq!(column_value(b"-".as_slice()), None);
        assert_eq!(column_value(b"-Z".as_slice()), None);
    }

    /// 两个操作数都坏时报后写的那支（C 先算 `argv[1]` 再 FALLTHROUGH 到 `argv[0]`），
    /// 以及三个出口各自的归类：多于两个是 usage，起始列大于结束列是 `err(1,...)`。
    #[test]
    fn the_operand_face_reports_the_stop_column_first_and_the_three_exits_apart() {
        assert_eq!(parse_columns(&[]), Ok((0, 0)));
        assert_eq!(
            parse_columns(&argv_of(&["3"])),
            Ok((3, 0)),
            "只给起始列时 stop 留 0"
        );
        assert_eq!(parse_columns(&argv_of(&["2", "4"])), Ok((2, 4)));
        assert_eq!(
            parse_columns(&argv_of(&["abc", "def"])),
            Err(ColrmOperand::IllegalColumn(b"def".as_slice())),
            "C 的 case 2 先算 stop，所以报的是后写的操作数"
        );
        assert_eq!(
            parse_columns(&argv_of(&["abc", "2"])),
            Err(ColrmOperand::IllegalColumn(b"abc".as_slice())),
            "stop 合法、start 坏，报 start"
        );
        assert_eq!(
            parse_columns(&argv_of(&["2", "abc"])),
            Err(ColrmOperand::IllegalColumn(b"abc".as_slice()))
        );
        assert_eq!(
            parse_columns(&argv_of(&["0", "-1"])),
            Err(ColrmOperand::IllegalColumn(b"0".as_slice())),
            "stop 回绕成极大值而被接受，start 是 0 才报错"
        );
        assert_eq!(
            parse_columns(&argv_of(&["4", "2"])),
            Err(ColrmOperand::StartAfterStop)
        );
        assert_eq!(parse_columns(&argv_of(&["2", "2"])), Ok((2, 2)));
        assert_eq!(
            parse_columns(&argv_of(&["2", "4", "6"])),
            Err(ColrmOperand::TooManyOperands)
        );
    }
}
