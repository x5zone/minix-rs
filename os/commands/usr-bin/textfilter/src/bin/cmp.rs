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
//! `skip1`/`skip2` are parsed as decimal byte counts (C's `strtoq` also accepts
//! `0x`/leading-`0` radix prefixes; that base-0 auto-detect is not carried).

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
    let skip1 = operands.get(2).map_or(0, |s| parse_skip(s));
    let skip2 = operands.get(3).map_or(0, |s| parse_skip(s));

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

    let a = &data1[(skip1.min(data1.len() as u64)) as usize..];
    let b = &data2[(skip2.min(data2.len() as u64)) as usize..];
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

fn parse_skip(word: &str) -> u64 {
    match word.parse::<u64>() {
        Ok(value) => value,
        Err(_) => usage(),
    }
}

fn usage() -> ! {
    support::warn(b"usage: cmp [-l | -s] file1 file2 [skip1 [skip2]]\n");
    support::terminate(ERR_EXIT)
}

#[cfg(test)]
mod tests {
    use super::uses_stdin;

    #[test]
    fn a_dash_operand_reads_stdin() {
        assert_eq!(uses_stdin("-", "f"), Ok([true, false]));
        assert_eq!(uses_stdin("f", "-"), Ok([false, true]));
        assert_eq!(uses_stdin("a", "b"), Ok([false, false]));
        // 两个操数都是 `-`：C `cmp.c:110` 报“只能指定一次标准输入”。
        assert_eq!(uses_stdin("-", "-"), Err(()));
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
