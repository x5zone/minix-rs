//! Minix-RS sort — key-based line sorting.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The deciding half —
//! `-k` key specifications, the `b d f i M n r` modifiers, `-n` numeric
//! comparison, `-t` delimiters, `-r`, `-u`, and `-c` — is the library's
//! `sort` module; the doing half collects the operands, orders the lines
//! in memory (the C's external merge over temporary files is the
//! execution layer for inputs beyond memory), and emits lines with a
//! trailing newline. Each input operand is read in file order as its own
//! run (a `-` names standard input; no operand reads standard input); the
//! default concatenates and sorts them, and `-m` merges the runs assuming
//! each is already ordered (`merge_sorted`). `-o` (output file) is rejected
//! by the option parser as not yet modelled.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::sort::{compare, merge_sorted, parse_options};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let (options, operands) = match parse_options(&words) {
        Ok((options, operands)) => (options, operands),
        Err(_) => {
            support::warn(b"sort: bad option\n");
            support::terminate(2);
        }
    };
    // C reads each input operand in file order (`-` / an unrecognised leading
    // dash names standard input) and concatenates their lines; no operand reads
    // standard input. `-c` accepts at most one input file (sort.c too-many guard).
    if options.check && operands.len() > 1 {
        support::warn(b"sort: too many input files for -c option\n");
        support::terminate(2);
    }
    let mut runs: Vec<Vec<String>> = Vec::new();
    if operands.is_empty() {
        runs.push(split_lines(&support::read_stdin()));
    } else {
        for operand in operands {
            let input = if operand == "-" {
                support::read_stdin()
            } else {
                read_file(operand)
            };
            runs.push(split_lines(&input));
        }
    }
    let mut lines: Vec<String> = runs.iter().flatten().cloned().collect();

    if options.check {
        check(&lines, &options);
        support::terminate(0);
    }

    if options.merge {
        let refs: Vec<&[String]> = runs.iter().map(|r| r.as_slice()).collect();
        lines = merge_sorted(&refs, &options);
    } else {
        lines.sort_by(|a, b| compare(a, b, &options));
    }
    if options.unique {
        lines.dedup_by(|a, b| compare(a, b, &options) == core::cmp::Ordering::Equal);
    }
    for line in &lines {
        let mut out = line.as_bytes().to_vec();
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
}

/// Splits collected input into lines, dropping the trailing newline of
/// every line and the final empty piece after a terminating newline.
fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

/// Read an input file to the end; an unopenable file is a status-2 error
/// (the C `access`/`fopen` failure path).
fn read_file(path: &str) -> Vec<u8> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("sort: {path}\n").as_bytes());
            support::terminate(2);
        }
    };
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
    data
}

/// The `-c` mode: every adjacent pair must be in order, and with `-u`
/// no pair may compare equal. The first violation is reported by name
/// and the status leaves 1 (the C's `warn` + `exit(1)` shape).
fn check(lines: &[String], options: &minix_textfilter::sort::Options) {
    let mut previous: Option<&str> = None;
    for line in lines {
        if let Some(prev) = previous {
            match compare(prev, line, options) {
                core::cmp::Ordering::Greater => {
                    support::warn(b"sort: disorder: ");
                    support::warn(line.as_bytes());
                    support::warn(b"\n");
                    support::terminate(1);
                }
                core::cmp::Ordering::Equal if options.unique => {
                    support::warn(b"sort: duplicate: ");
                    support::warn(line.as_bytes());
                    support::warn(b"\n");
                    support::terminate(1);
                }
                _ => {}
            }
        }
        previous = Some(line);
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
