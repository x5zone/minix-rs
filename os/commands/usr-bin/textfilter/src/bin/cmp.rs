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

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::cmp::{compare_bytes, format_list_entry, report_eof, report_first_diff};

const ERR_EXIT: i32 = 2;
const DIFF_EXIT: i32 = 1;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut list_all = false;
    let mut silent = false;
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if arg == "-l" {
            list_all = true;
        } else if arg == "-s" {
            silent = true;
        } else if !arg.starts_with('-') || arg.len() == 1 {
            operands.push(arg);
        } else {
            usage();
        }
    }
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
    let name1 = if stdin1 { "stdin" } else { file1 };
    let name2 = if stdin2 { "stdin" } else { file2 };
    let skip1 = operands.get(2).map_or(0, |s| match parse_skip(s) {
        Ok(value) => value,
        Err(()) => usage(),
    });
    let skip2 = operands.get(3).map_or(0, |s| match parse_skip(s) {
        Ok(value) => value,
        Err(()) => usage(),
    });

    let data1 = if stdin1 {
        support::read_stdin()
    } else {
        match read_file_or_none(file1) {
            Some(data) => data,
            None => {
                support::warn(format!("cmp: {name1}\n").as_bytes());
                support::terminate(ERR_EXIT);
            }
        }
    };
    let data2 = if stdin2 {
        support::read_stdin()
    } else {
        match read_file_or_none(file2) {
            Some(data) => data,
            None => {
                support::warn(format!("cmp: {name2}\n").as_bytes());
                support::terminate(ERR_EXIT);
            }
        }
    };

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
            support::warn(format!("cmp: {}\n", report_eof(file, byte, line, true)).as_bytes());
            DIFF_EXIT
        } else if outcome.equal {
            0
        } else {
            DIFF_EXIT
        }
    } else if let Some((byte, line)) = outcome.first_difference {
        support::emit(report_first_diff(name1, name2, byte, line).as_bytes());
        DIFF_EXIT
    } else if let Some((which, byte, line)) = outcome.eof_on {
        let _ = (byte, line);
        let file = if which == 1 { name1 } else { name2 };
        support::warn(format!("cmp: {}\n", report_eof(file, 0, 0, false)).as_bytes());
        DIFF_EXIT
    } else {
        0
    };
    support::terminate(status)
}

/// Open, read to the end, and close a file, or `None` if it cannot be opened.
fn read_file_or_none(path: &str) -> Option<Vec<u8>> {
    let fd = open(path, 0, 0).ok()?;
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => data.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    //[SD19-gate] SD-19: close 后 fd 不再使用
    #[allow(clippy::let_underscore_must_use)]
    let _ = close(fd);
    Some(data)
}

/// Which of the two operands read standard input, faithful to C
/// `cmp.c:95-113`: a `-` names stdin, and both operands naming it is an error
/// (`Err(())`).
fn uses_stdin(file1: &str, file2: &str) -> Result<[bool; 2], ()> {
    let stdin1 = file1 == "-";
    let stdin2 = file2 == "-";
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
fn parse_skip(word: &str) -> Result<i64, ()> {
    let text = word.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    // Base 0 decides the radix from the prefix. Note the asymmetry C keeps: with
    // `0x`/`0X` a missing hex digit means no conversion at all, while a bare
    // leading `0` is itself a converted digit (so `08` is zero, ep left at `8`).
    let (radix, digits, zero_counts) =
        match rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            Some(hex) => (16, hex, false),
            None => match rest.strip_prefix('0') {
                Some(octal) => (8, octal, true),
                None => (10, rest, false),
            },
        };
    let run = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    if run == 0 && !zero_counts {
        return Err(());
    }
    // A zero-length run is only reachable through the bare leading `0`, which is
    // itself the digit C converted: the value is zero and the offset stops there.
    let value = if run == 0 {
        0
    } else {
        i64::from_str_radix(&digits[..run], radix).map_err(|_| ())?
    };
    Ok(if negative { -value } else { value })
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
fn skip_eof(file: &str, len: usize, list_all: bool, silent: bool) -> ! {
    if !silent {
        let byte = len as u64 + 1;
        support::warn(format!("cmp: {}\n", report_eof(file, byte, 0, list_all)).as_bytes());
    }
    support::terminate(DIFF_EXIT)
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
        assert_eq!(uses_stdin("-", "f"), Ok([true, false]));
        assert_eq!(uses_stdin("f", "-"), Ok([false, true]));
        assert_eq!(uses_stdin("a", "b"), Ok([false, false]));
        // 两个操数都是 `-`：C `cmp.c:110` 报“只能指定一次标准输入”。
        assert_eq!(uses_stdin("-", "-"), Err(()));
    }

    #[test]
    fn skip_reads_the_radix_from_its_digits() {
        // C `strtoq(…, 0)`: `0x`/`0X` hexadecimal, a leading `0` octal, otherwise
        // decimal.
        assert_eq!(parse_skip("10"), Ok(10));
        assert_eq!(parse_skip("010"), Ok(8));
        assert_eq!(parse_skip("0x10"), Ok(16));
        assert_eq!(parse_skip("0X1f"), Ok(31));
        assert_eq!(parse_skip("0"), Ok(0));
        // An octal prefix that stops early still converts the `0` it consumed:
        // C leaves `ep` at the offending character and only errors when `ep`
        // never moved.
        assert_eq!(parse_skip("08"), Ok(0));
        assert_eq!(parse_skip("0779"), Ok(63));
    }

    #[test]
    fn skip_accepts_sign_blanks_and_trailing_garbage() {
        assert_eq!(parse_skip("  12"), Ok(12));
        assert_eq!(parse_skip("+4"), Ok(4));
        assert_eq!(parse_skip("-3"), Ok(-3));
        assert_eq!(parse_skip("-0"), Ok(0));
        // C checks only `errno` and `ep == argv[n]`; a tail that is not a digit
        // stops the conversion without failing it (`cmp.c:126`).
        assert_eq!(parse_skip("12xyz"), Ok(12));
        assert_eq!(parse_skip("0x1Fz"), Ok(31));
    }

    #[test]
    fn skip_errors_only_when_nothing_was_converted_or_out_of_range() {
        assert!(parse_skip("xyz").is_err());
        assert!(parse_skip("").is_err());
        assert!(parse_skip("-").is_err());
        assert!(parse_skip("+").is_err());
        // `0x` with no hex digits converts nothing at all — C's `ep == argv[n]`.
        assert!(parse_skip("0x").is_err());
        assert!(parse_skip("0xg1").is_err());
        // Out of range is C's `errno` arm: both ends of the `off_t` span and
        // anything beyond it.
        assert!(parse_skip("99999999999999999999999").is_err());
        assert!(parse_skip("-99999999999999999999999").is_err());
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
