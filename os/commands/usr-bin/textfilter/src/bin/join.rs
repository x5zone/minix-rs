//! Minix-RS `join` — the doing half over `minix_textfilter::join`.
//!
//! Ground truth: `minix3/usr.bin/join/join.c`. The merge and field logic are
//! the library (`join_lines`/`JoinOptions`); this program parses the option
//! words, reads the two file operands (a `-` names standard input), feeds the
//! newline-stripped lines to the library, and emits the result.
//!
//! Wired: default join, `-t` (delimiter), `-1`/`-2` (per-file join field),
//! `-a 1`/`-a 2` (unmatched lines), `-o` (output field list, `0`/`F.N`), `-e`
//! (empty-field replacement), `-j field` (both join fields at once — the BSD
//! alias for `-1 field -2 field`) and `-v fileno` (that file's unpairable lines
//! only, with matched output suppressed). `-a` and `-v` are mutually exclusive
//! (`join.c:234-235`). Option values take either form the BSD `getopt`
//! (`join.c:133`) allows: attached (`-t,`, `-a1`, `-o0,1`) or as the following
//! argument (`-t ,`). Declared residual: the C `obsolete(argv)` pre-pass
//! (`join.c:132`) that rewrites legacy bare `-a` (both files) and `-j[12]` forms
//! — not modelled here.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::join::{JoinError, join_lines, parse_join_args};

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

/// Map a [`JoinError`] to C's `warnx` line for that branch (each `join.c` arm
/// warns then calls `usage()`), then exit 1. The two-line `warnx`+`usage` C emits
/// is collapsed to the specific message — this shell wires neither the `strerror`
/// sentence (the table itself lives in `minix_types::Errno::message`) nor the usage
/// line (declared
/// difference, same as the other command bins).
fn join_usage_fail(e: JoinError) -> ! {
    match e {
        JoinError::Usage => fail(
            "usage: join [-a fileno | -v fileno] [-1 field1] [-2 field2] [-e empty] [-j field] [-o field] [-t char] file1 file2",
        ),
        JoinError::FieldLessThan1 => fail("option field number less than 1"),
        JoinError::IllegalFieldNumber => fail("illegal field number"),
        JoinError::FileNumberNot12 => fail("option file number not 1 or 2"),
        JoinError::IllegalFileNumber => fail("illegal file number"),
        JoinError::BadTabChar => fail("illegal tab character specification"),
        JoinError::BadOutputField => fail("malformed -o option field"),
        JoinError::AAndVMutuallyExclusive => fail("-a and -v options mutually exclusive"),
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (opts, files) = match parse_join_args(&args[1..]) {
        Ok(a) => (a.opts, a.files),
        Err(e) => join_usage_fail(e),
    };

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
