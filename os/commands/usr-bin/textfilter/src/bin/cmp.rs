//! Minix-RS cmp — byte comparison of two streams.
//!
//! Ground truth: `minix3/usr.bin/cmp/` (NetBSD). The deciding half — the byte
//! walk, the `-l` difference list, the EOF detection, and the report strings
//! (`misc.c` `diffmsg`/`eofmsg`) — is the library's fully tested `cmp` module.
//! This program takes two operands (a `-` names standard input, per the C
//! backward-compatibility path `cmp.c:95-113`; both may not be `-`), applies the
//! optional `skip1`/`skip2`
//! byte offsets, compares, and reports exactly as the C: without flags the
//! first difference `"%s %s differ: char %lld, line %lld"` to standard output
//! (or `EOF on <file>` when one stream ends first); with `-l` one line per
//! differing byte (`%6lld %3o %3o`); with `-s` nothing. Exit status matches the
//! C `ERR_EXIT`/`DIFF_EXIT`: 0 equal, 1 different, 2 usage or open trouble.
//!
//! `skip1`/`skip2` are parsed the way C's `strtoq(…, 0)` does it (`cmp.c:125`
//! and `cmp.c:130`): leading blanks skipped, an optional sign, then a radix read
//! from the digits — `0x`/`0X` is hexadecimal, a leading `0` is octal, anything
//! else decimal. C reports an error only when nothing was converted or the value
//! overflowed, so trailing garbage is accepted (`12xyz` skips twelve bytes). The
//! offset is then applied at C's own site: `c_regular` reports `EOF on <file>`
//! when the offset is past the file (and a negative offset walks the stream to
//! its end through `c_special`, landing on the same report).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::cmp::{
    CmpArgError, compare_bytes, format_list_entry, parse_cmp_flags, report_eof, report_first_diff,
};

const ERR_EXIT: i32 = 2;
const DIFF_EXIT: i32 = 1;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // 程序名那一腿交给 `warn_line` 与 `GetoptReport::line_bytes` 的 `&str` 口（登记
    // `ARGV-BYTE-PATH-IN-LIBS`），所以臂内折一次；操作数与报文的名字位不跟着折。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C 的次序是「选项环 → `-l`/`-s` 互斥 → 操作数个数 → 开档」（`cmp.c:74-100`），
    // 三处出口的形制各不相同：集外字母是 libc 一行**加** usage 一行；互斥是一句 `errx`
    // 就完（没有 usage）；个数不对只有 usage 一行。都退 `ERR_EXIT` = 2。
    let flags = match parse_cmp_flags(&refs[1..]) {
        Ok(flags) => flags,
        Err(CmpArgError::UnknownOption(letter)) => {
            // 字母位走 `line_bytes`：取法已在 `src/cmp.rs` 改成按字节，两边同形才成立。
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(&prog));
            usage();
        }
    };
    let (list_all, silent) = (flags.list_all, flags.silent);
    let operands = flags.operands;
    if list_all && silent {
        support::warn(b"cmp: only one of -l and -s may be specified\n");
        support::terminate(ERR_EXIT);
    }
    if operands.len() < 2 || operands.len() > 4 {
        usage();
    }
    let (file1, file2) = (operands[0], operands[1]);
    // `cmp.c:95-113` backward compatibility: a `-` operand reads standard
    // input; both operands may not name it. The label in messages becomes
    // "stdin" (C `file1 = "stdin"`). Previously `-` was opened as a literal
    // file, so `cmp - file` (e.g. `cat x | cmp - file`) failed instead of
    // comparing standard input.
    let [stdin1, stdin2] = match uses_stdin(file1, file2) {
        Ok(both) => both,
        Err(()) => {
            support::warn(b"cmp: standard input may only be specified once\n");
            support::terminate(ERR_EXIT);
        }
    };
    let name1 = if stdin1 { b"stdin".as_slice() } else { file1 };
    let name2 = if stdin2 { b"stdin".as_slice() } else { file2 };
    // C opens both operands *before* it parses the skip operands (`cmp.c:95-119`
    // then `:121-134`), so when a file cannot be opened that is what the user
    // hears about, even if a skip value is also malformed.
    let data1 = if stdin1 {
        support::read_stdin()
    } else {
        // 开档那一腿仍要 `&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`），所以折一次；
        // 折出来的串同时喂给 `warn_line`——报文腿与开档腿同一条，两条腿不会分叉。
        let path = String::from_utf8_lossy(file1);
        match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // C `cmp.c:101-104`：`if (!sflag) warn("%s", file1);` 然后
                // `exit(ERR_EXIT)`——`-s` 之下**整句不打**，只留退出码 2。
                if !silent {
                    support::warn_line(&prog, &path, code);
                }
                support::terminate(ERR_EXIT);
            }
        }
    };
    let data2 = if stdin2 {
        support::read_stdin()
    } else {
        let path = String::from_utf8_lossy(file2);
        match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // 同上：第二支的 `warn` 也被 `sflag` 管着（`cmp.c:113-117`）。
                if !silent {
                    support::warn_line(&prog, &path, code);
                }
                support::terminate(ERR_EXIT);
            }
        }
    };
    let skip1 = operands.get(2).map_or(0, |s| match parse_skip(s) {
        Ok(value) => value,
        Err(()) => usage(),
    });
    let skip2 = operands.get(3).map_or(0, |s| match parse_skip(s) {
        Ok(value) => value,
        Err(()) => usage(),
    });

    // C reaches this only through `c_regular`, whose very first statement is
    // `if (sflag && len1 != len2) exit(1);` — the *unadjusted* sizes, before any
    // skip is applied and before a single byte is compared. So a silent run of
    // two files of different sizes reports "different" even when the skipped
    // windows would have matched. `special` in `cmp.c:136-146` means a pipe or a
    // `-` operand takes `c_special` instead and never sees this test, which is why
    // the guard below is limited to the two-file case.
    if silent && !stdin1 && !stdin2 && data1.len() != data2.len() {
        support::terminate(DIFF_EXIT);
    }

    // C applies each offset before comparing and checks the two files in order:
    // `c_regular`'s `if (skip1 > len1) eofmsg(file1, len1 + 1, 0)` then the same
    // test for file2; an offset that is negative takes `c_special`'s
    // `for (...; skip1--; ...)` walk, which consumes the stream to its end and
    // reports the same `EOF on` line.
    let a = match skip_window(&data1, skip1) {
        Some(window) => window,
        None => skip_eof(name1, data1.len(), list_all, silent),
    };
    let b = match skip_window(&data2, skip2) {
        Some(window) => window,
        None => skip_eof(name2, data2.len(), list_all, silent),
    };
    let outcome = compare_bytes(a, b, list_all);

    let status = if silent {
        if outcome.equal { 0 } else { DIFF_EXIT }
    } else if list_all {
        let mut text = String::new();
        for entry in &outcome.byte_list {
            format_list_entry(*entry, &mut text);
        }
        if !text.is_empty() {
            support::emit(text.as_bytes());
        }
        if let Some((which, byte, line)) = outcome.eof_on {
            let file = if which == 1 { name1 } else { name2 };
            support::warn(&eof_line(file, byte, line, true));
            DIFF_EXIT
        } else if outcome.equal {
            0
        } else {
            DIFF_EXIT
        }
    } else if let Some((byte, line)) = outcome.first_difference {
        support::emit(&report_first_diff(name1, name2, byte, line));
        DIFF_EXIT
    } else if let Some((which, byte, line)) = outcome.eof_on {
        let _ = (byte, line);
        let file = if which == 1 { name1 } else { name2 };
        support::warn(&eof_line(file, 0, 0, false));
        DIFF_EXIT
    } else {
        0
    };
    support::terminate(status)
}

/// Which of the two operands read standard input, faithful to C
/// `cmp.c:95-113`: a `-` names stdin, and both operands naming it is an error
/// (`Err(())`).
fn uses_stdin(file1: &[u8], file2: &[u8]) -> Result<[bool; 2], ()> {
    let stdin1 = file1 == b"-";
    let stdin2 = file2 == b"-";
    if stdin1 && stdin2 {
        Err(())
    } else {
        Ok([stdin1, stdin2])
    }
}

/// C `strtoq(word, &ep, 0)` (`cmp.c:125`/`cmp.c:130`): skip leading blanks, take
/// an optional sign, then read the radix from the digits — `0x`/`0X` hexadecimal,
/// a leading `0` octal, otherwise decimal. The longest run of digits valid in
/// that radix is consumed and anything after it is ignored, because C's only two
/// failure tests are `errno` (overflow) and `ep == argv[n]` (nothing converted).
/// `Err(())` is exactly those two cases.
fn parse_skip(word: &[u8]) -> Result<i64, ()> {
    let blank = word
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(word.len());
    let text = &word[blank..];
    let (negative, rest) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    // Base 0 decides the radix from the prefix. Note the asymmetry C keeps: with
    // `0x`/`0X` a missing hex digit means no conversion at all, while a bare
    // leading `0` is itself a converted digit (so `08` is zero, ep left at `8`).
    let (radix, digits, zero_counts) = match rest
        .strip_prefix(b"0x")
        .or_else(|| rest.strip_prefix(b"0X"))
    {
        Some(hex) => (16, hex, false),
        None => match rest.strip_prefix(b"0") {
            Some(octal) => (8, octal, true),
            None => (10, rest, false),
        },
    };
    let run = digits
        .iter()
        .position(|b| skip_digit(*b, radix).is_none())
        .unwrap_or(digits.len());
    if run == 0 && !zero_counts {
        return Err(());
    }
    // A zero-length run is only reachable through the bare leading `0`, which is
    // itself the digit C converted: the value is zero and the offset stops there.
    // 溢出走 `checked_*`，对位 C 的 `errno == ERANGE` 那一判。
    let value = digits[..run].iter().try_fold(0i64, |acc, b| {
        let digit = skip_digit(*b, radix).ok_or(())?;
        acc.checked_mul(radix as i64)
            .and_then(|shifted| shifted.checked_add(digit as i64))
            .ok_or(())
    })?;
    Ok(if negative { -value } else { value })
}

/// `strtoq` 在 base 0 下认的那一枚数字：十进制只到 `9`，十六进制才收 `a`-`f`
/// （大小写同价）。按字节判，是为了让非法字节在数字串里扮演 C 的那个停点，
/// 而不是先把整词折成文本再猜它是不是数字。
fn skip_digit(byte: u8, radix: u32) -> Option<u32> {
    if byte.is_ascii_digit() && ((byte - b'0') as u32) < radix {
        return Some((byte - b'0') as u32);
    }
    let letter = byte | 0x20;
    if (b'a'..=b'f').contains(&letter) && (letter - b'a') as u32 + 10 < radix {
        return Some((letter - b'a') as u32 + 10);
    }
    None
}

/// The bytes left after applying a skip, or `None` when C would report EOF:
/// `c_regular`'s test is strict (`skip > len`), so an offset exactly at the end
/// of the file is not an EOF report but an empty window.
fn skip_window(data: &[u8], skip: i64) -> Option<&[u8]> {
    if skip < 0 || skip as u64 > data.len() as u64 {
        return None;
    }
    Some(&data[skip as usize..])
}

/// C `misc.c`'s `eofmsg(file, len + 1, 0)`: silent prints nothing, the plain
/// form is `EOF on <file>`, and `-l` adds `: char <len + 1>` because the `line`
/// argument is zero. The exit status is `DIFF_EXIT` in all three cases.
fn skip_eof(file: &[u8], len: usize, list_all: bool, silent: bool) -> ! {
    if !silent {
        let byte = len as u64 + 1;
        support::warn(&eof_line(file, byte, 0, list_all));
    }
    support::terminate(DIFF_EXIT)
}

/// C 的 `errx`/`warn` 那一前缀：`cmp: ` 加 `eofmsg` 的那一句再加换行。
/// 名字位是操作数原字节，所以整行按字节拼，不借 `format!` 的 `{}`。
fn eof_line(file: &[u8], byte: u64, line: u64, list_all: bool) -> Vec<u8> {
    let mut text = b"cmp: ".to_vec();
    text.extend_from_slice(&report_eof(file, byte, line, list_all));
    text.push(b'\n');
    text
}

fn usage() -> ! {
    support::warn(b"usage: cmp [-l | -s] file1 file2 [skip1 [skip2]]\n");
    support::terminate(ERR_EXIT)
}

#[cfg(test)]
mod tests {
    use super::{parse_skip, skip_window, uses_stdin};

    #[test]
    fn a_dash_operand_reads_stdin() {
        assert_eq!(uses_stdin(b"-", b"f"), Ok([true, false]));
        assert_eq!(uses_stdin(b"f", b"-"), Ok([false, true]));
        assert_eq!(uses_stdin(b"a", b"b"), Ok([false, false]));
        // 两个操作数都是 `-`：C `cmp.c:110` 报“只能指定一次标准输入”。
        assert_eq!(uses_stdin(b"-", b"-"), Err(()));
    }

    #[test]
    fn skip_reads_the_radix_from_its_digits() {
        // C `strtoq(…, 0)`: `0x`/`0X` hexadecimal, a leading `0` octal, otherwise
        // decimal.
        assert_eq!(parse_skip(b"10"), Ok(10));
        assert_eq!(parse_skip(b"010"), Ok(8));
        assert_eq!(parse_skip(b"0x10"), Ok(16));
        assert_eq!(parse_skip(b"0X1f"), Ok(31));
        assert_eq!(parse_skip(b"0"), Ok(0));
        // An octal prefix that stops early still converts the `0` it consumed:
        // C leaves `ep` at the offending character and only errors when `ep`
        // never moved.
        assert_eq!(parse_skip(b"08"), Ok(0));
        assert_eq!(parse_skip(b"0779"), Ok(63));
    }

    #[test]
    fn skip_accepts_sign_blanks_and_trailing_garbage() {
        assert_eq!(parse_skip(b"  12"), Ok(12));
        assert_eq!(parse_skip(b"+4"), Ok(4));
        assert_eq!(parse_skip(b"-3"), Ok(-3));
        assert_eq!(parse_skip(b"-0"), Ok(0));
        // C checks only `errno` and `ep == argv[n]`; a tail that is not a digit
        // stops the conversion without failing it (`cmp.c:126`).
        assert_eq!(parse_skip(b"12xyz"), Ok(12));
        assert_eq!(parse_skip(b"0x1Fz"), Ok(31));
    }

    #[test]
    fn skip_errors_only_when_nothing_was_converted_or_out_of_range() {
        assert!(parse_skip(b"xyz").is_err());
        assert!(parse_skip(b"").is_err());
        assert!(parse_skip(b"-").is_err());
        assert!(parse_skip(b"+").is_err());
        // `0x` with no hex digits converts nothing at all — C's `ep == argv[n]`.
        assert!(parse_skip(b"0x").is_err());
        assert!(parse_skip(b"0xg1").is_err());
        // Out of range is C's `errno` arm: both ends of the `off_t` span and
        // anything beyond it.
        assert!(parse_skip(b"99999999999999999999999").is_err());
        assert!(parse_skip(b"-99999999999999999999999").is_err());
    }

    #[test]
    fn skip_past_the_end_is_the_eof_side() {
        let data = [b'a', b'b', b'c'];
        assert_eq!(skip_window(&data, 1), Some(&data[1..]));
        // `c_regular`'s test is strict, so an offset at the very end is an empty
        // window, not an EOF report (`skip1 > len1` is false there).
        assert_eq!(skip_window(&data, 3), Some(&[][..]));
        assert_eq!(skip_window(&data, 4), None);
        // A negative offset takes C's `c_special` walk (`for (...; skip1--;)`),
        // which consumes the stream to its end and reports the same EOF line.
        assert_eq!(skip_window(&data, -1), None);
        assert_eq!(skip_window(&[], 0), Some(&[][..]));
        assert_eq!(skip_window(&[], 1), None);
    }
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
