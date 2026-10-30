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

/// The three operand-level rejections, handed back to the shell so it can print
/// each message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColrmOperand<'a> {
    /// `colrm.c:90-91`: more than two operands, falling into `default: usage()`
    /// (one line, exit 1, no other message).
    TooManyOperands,
    /// `colrm.c:94-95`: `err(1, "illegal start and stop columns")`.  It is `err`
    /// rather than `errx`, so that line also appends a `strerror(errno)` sentence,
    /// and `errno` is 0 at that point.
    StartAfterStop,
    /// `colrm.c:80-86`: `errx(1, "illegal column -- %s", <operand text>)`.  `errx`
    /// appends no `strerror` and reports the operand text as given (a reference
    /// binary measured `colrm: illegal column -- 3x`).
    IllegalColumn(&'a [u8]),
}

/// The operand section of C `colrm.c:76-92`: returns `(start, stop)`, where a
/// `stop` of 0 means "through end of line".
///
/// When both operands are bad, the **later** one is reported: C's `case 2:`
/// first runs `strtol` on `argv[1]` and validates it, and only then does
/// `/* FALLTHROUGH */` reach `argv[0]` (`:78-87`), so `colrm abc def` prints
/// `def`.  This is the easiest place in this function to get backwards, and a
/// reference binary was measured for every shape.
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
            // C writes `if (stop && start > stop)` (`:94`): no comparison when
            // `stop` is 0, and in this arm `stop` can only come from two operands,
            // with `column_value` having already excluded 0.
            if start > stop {
                Err(ColrmOperand::StartAfterStop)
            } else {
                Ok((start, stop))
            }
        }
        _ => Err(ColrmOperand::TooManyOperands),
    }
}

/// The combined judgement of C's `strtol(word, &p, 10)` plus `if (<value> <= 0 || *p)`.
///
/// The accepted set is both wider and narrower than "a string of digits", and
/// both ends must follow C: (1) leading whitespace is skipped, one `+`/`-` sign
/// is allowed, and at least one decimal digit is required (`colrm " 3"` and
/// `colrm +3` are both accepted — measured against a reference binary); (2) no
/// characters may remain after the digits (`3x`, `0x10`, `1e2`, `3 ` are all
/// rejected because `*p` is non-zero); (3) an overflow saturates to the `long`
/// range instead of erroring (`99999999999999999999999` is accepted); (4)
/// `colrm.c:63` stores the result in a `u_long`, so C's `<= 0` really only
/// rejects 0 — a negative value wraps to a huge number and is accepted, and
/// because no column number comes close to it the whole line passes through
/// (`colrm -- -5` exits 0 — measured against a reference binary).  This function
/// reproduces that wrap faithfully instead of "fixing" C.
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
    // No digit at all, or characters left after the digits, both land in C's
    // "`*p` is non-zero" branch.
    if cursor == digits_begin || cursor != bytes.len() {
        return None;
    }
    // The saturating range of `long`: on overflow `strtol` returns
    // `LONG_MAX`/`LONG_MIN` (the magnitudes differ by one, each capped on its own
    // side), and storing that in C's `u_long` yields this two's-complement face.
    // With these huge values, every column comparison comes out "larger than any
    // real column", matching the reference binary's whole-line passthrough.
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
    // All three target boards and the host are 64-bit, so `u_long` is as wide as `usize`.
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

    /// Test adapter for the byte-shaped entry point: fixtures are still written as
    /// text and are folded once right before entering the parser.
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

    /// Pin each operand shape: zero is rejected, whitespace and a `+` sign are
    /// accepted, trailing characters are rejected, an overflowing value is accepted
    /// after saturation, and a negative value is accepted after wrapping through
    /// `u_long` (a C quirk reproduced faithfully).
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
        // Overflow saturation: strtol returns LONG_MAX with errno=ERANGE, and
        // neither of C's two checks stops it.
        assert_eq!(
            column_value(b"99999999999999999999999".as_slice()),
            Some(i64::MAX as usize)
        );
        // A negative value wraps into a huge u_long — `<= 0` cannot stop it (a
        // reference binary exits 0 for `colrm -- -5`).
        assert_eq!(column_value(b"-5".as_slice()), Some(-5i64 as usize));
        assert_eq!(
            column_value(b"-99999999999999999999999".as_slice()),
            Some(i64::MAX as usize + 1),
            "饱和到 LONG_MIN 后回绕"
        );
        // The rejected shapes: zero, nothing at all, trailing characters, and a
        // non-terminating byte after the digits.
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

    /// When both operands are bad the later one is reported (C evaluates `argv[1]`
    /// first and then FALLTHROUGHs to `argv[0]`), and the three exits are kept
    /// apart: more than two operands is usage, start greater than stop is
    /// `err(1,...)`.
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
