//! Minix-RS `colcrt` — the doing half over `minix_textfilter::colcrt`.
//!
//! Ground truth: `minix3/usr.bin/colcrt/colcrt.c`. The CRT screen model is the
//! library (`colcrt`); this program parses the flags, reads standard input when
//! given no operand, and otherwise processes each file operand as a separate
//! archive (matching C, which flushes the screen at each file's end via its
//! shared-globals loop). Output is written to standard output.
//!
//! Wired: `-` suppresses underlining (`suppresul`), `-2` prints every half-line
//! (`printall`) — exactly C's leading-`-` option loop (`colcrt.c:87-103`); an
//! unrecognised flag prints the usage line and exits 1. A file that cannot be
//! opened prints an error and exits 1 (C `perror` + `exit(1)`, `:107-111`).
//! Declared residual: the exact `perror` system-message text is not reproduced
//! (only the operand name is reported).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_textfilter::colcrt::{ColcrtOptions, colcrt};

fn fail(message: &str) -> ! {
    let mut line = String::from("colcrt: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1);
}

fn usage() -> ! {
    support::warn(b"usage: colcrt [ - ] [ -2 ] [ file ... ]\n");
    support::terminate(1);
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut opts = ColcrtOptions::default();
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;
    // C parses leading `-`-prefixed operands as flags, then treats the rest as
    // file names (the first non-flag stops the option loop).
    while i < args.len() {
        let a = args[i];
        if !a.starts_with('-') || a == "-" {
            break;
        }
        match &a[1..] {
            "" => opts.suppress_underline = true,
            "2" => opts.print_all = true,
            _ => usage(),
        }
        i += 1;
    }
    files.extend(args[i..].iter().map(|s| s.to_string()));

    let mut out: Vec<u8> = Vec::new();
    if files.is_empty() {
        out.extend(colcrt(&read_stdin(), &opts));
    } else {
        for f in &files {
            let data = support::read_file(f).unwrap_or_else(|_| fail("cannot open input"));
            out.extend(colcrt(&data, &opts));
        }
    }
    support::emit(&out);
    support::terminate(0);
}

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

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
