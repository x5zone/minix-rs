//! Minix-RS `join` — the doing half over `minix_textfilter::join`.
//!
//! Ground truth: `minix3/usr.bin/join/join.c`. The merge and field logic are
//! the library (`join_lines`/`JoinOptions`); this program parses the option
//! words, reads the two file operands (a `-` names standard input), feeds the
//! newline-stripped lines to the library, and emits the result.
//!
//! Wired: default join, `-t` (delimiter), `-1`/`-2` (per-file join field),
//! `-a 1`/`-a 2` (unmatched lines). Declared residual: `-o`, `-e`, `-j`,
//! `-v`, and the obsolete one-argument form — requesting them reports
//! "option not wired" and exits 1 rather than emitting a wrong join.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::join::{JoinOptions, join_lines};

/// Read a whole standard-input stream (a `-` operand).
fn read_stdin() -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    out
}

/// Split a buffer into newline-stripped lines, dropping the empty tail that
/// a newline-terminated file leaves.
fn line_refs(buf: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = buf.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }
    lines
}

fn fail(message: &str) -> ! {
    let mut line = String::from("join: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut opts = JoinOptions::default();
    let mut files: Vec<&str> = Vec::new();
    let mut i = 1;
    // Take the next argv word as an option's value (empty if none remains).
    macro_rules! value {
        () => {{
            i += 1;
            args.get(i).copied().unwrap_or("")
        }};
    }
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            files.extend(args[i + 1..].iter().copied());
            break;
        }
        if a == "-" || !a.starts_with('-') || a.len() == 1 {
            files.push(a);
            i += 1;
            continue;
        }
        let flag = &a[1..];
        match flag {
            "t" => {
                let v = value!();
                if let Some(&b) = v.as_bytes().first() {
                    opts.delim = b;
                }
            }
            "a" => match value!() {
                "1" => opts.unmatched1 = true,
                "2" => opts.unmatched2 = true,
                _ => fail("-a option file number not 1 or 2"),
            },
            "1" | "2" => {
                let v = value!();
                let n: usize = v.parse().unwrap_or(0);
                if n == 0 {
                    fail("option field number less than 1");
                }
                if flag == "1" {
                    opts.field1 = n - 1;
                } else {
                    opts.field2 = n - 1;
                }
            }
            "o" | "e" | "j" | "v" => {
                // Consume the value so it is not mistaken for a file, then
                // decline the unwired residual face.
                let _ = value!();
                fail("option not wired (residual: -o/-e/-j/-v)");
            }
            _ => fail("unknown option"),
        }
        i += 1;
    }

    if files.len() != 2 {
        fail("usage: join [-a fileno] [-1 field] [-2 field] [-t char] file1 file2");
    }

    let b1 = if files[0] == "-" {
        read_stdin()
    } else {
        support::read_file(files[0]).unwrap_or_else(|_| fail("cannot open file 1"))
    };
    let b2 = if files[1] == "-" {
        read_stdin()
    } else {
        support::read_file(files[1]).unwrap_or_else(|_| fail("cannot open file 2"))
    };

    let out = join_lines(&line_refs(&b1), &line_refs(&b2), &opts);
    let mut rendered = Vec::new();
    for line in &out {
        rendered.extend_from_slice(line);
        rendered.push(b'\n');
    }
    support::emit(&rendered);
    support::terminate(0);
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
