//! Minix-RS basename — strip directory and optional suffix.
//!
//! Ground truth: `minix3/usr.bin/basename/basename.c` (NetBSD), main at
//! lines 55 to 81: `basename string [suffix]`; an empty string prints an
//! empty line and succeeds (line 73); a malformed invocation or an
//! unsplittable path is a failure (the C `err(1)` and `usage`). The
//! splitting and suffix rules live in the library's `path` module (the
//! deciding half, fully tested); this program gathers argv and prints the
//! result.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::path::basename;

/// The whole program body; both `main` forms call it and it never
/// returns (every path ends in [`support::terminate`]). The two-form
/// entry contract is documented once in `echo.rs`, this crate's
/// template binary.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    // BSD getopt with an empty option string stops at the first
    // non-option; any option-looking argument is a usage failure.
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if operands.is_empty() && arg.starts_with('-') && arg.len() > 1 {
            support::emit(b"usage: basename string [suffix]\n");
            support::terminate(1);
        }
        operands.push(arg);
    }
    if operands.is_empty() || operands.len() > 2 {
        support::emit(b"usage: basename string [suffix]\n");
        support::terminate(1);
    }

    // C: an empty string prints an empty line and succeeds (basename.c:73).
    if operands[0].is_empty() {
        support::emit(b"\n");
        support::terminate(0);
    }

    let suffix = operands.get(1).map_or("", |s| *s);
    let stem = match basename(operands[0], suffix) {
        Ok(stem) => stem,
        Err(_) => support::terminate(1),
    };
    let mut line = stem.as_bytes().to_vec();
    line.push(b'\n');
    support::emit(&line);
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
