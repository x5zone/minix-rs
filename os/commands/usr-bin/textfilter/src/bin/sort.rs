//! Minix-RS sort — key-based line sorting.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The deciding half —
//! `-k` key specifications, the `b d f i M n r` modifiers, `-n` numeric
//! comparison, `-t` delimiters, `-r`, `-u`, and `-c` — is the library's
//! `sort` module; the doing half collects stdin, orders the lines
//! in memory (the C's external merge over temporary files is the
//! execution layer for inputs beyond memory), and emits lines with a
//! trailing newline. File operands wait for the gated open-existing
//! call; `-m` (merge) and `-o` (output file) land with the files face.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::sort::{compare, parse_options};
use minix_sys::read;

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
    if !operands.is_empty() {
        support::warn(b"sort: file operands wait for the open call (edge E-CMDSYSFACE)\n");
        support::terminate(2);
    }

    let input = support::read_stdin();
    let mut lines = split_lines(&input);

    if options.check {
        check(&lines, &options);
        support::terminate(0);
    }

    lines.sort_by(|a, b| compare(a, b, &options));
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
