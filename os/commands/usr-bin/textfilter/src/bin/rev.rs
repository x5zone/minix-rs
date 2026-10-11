//! Minix-RS rev — reverse each line's characters.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 59-109:
//! file operands or stdin, every line's characters print in reverse with
//! a newline, and an unopenable file warns and is skipped (exit status stays
//! nonzero, non-fatal). `rev` takes no options (C `getopt("")`). The
//! character-reversal walk (multibyte kept intact) is the library's tested
//! `reversed_lines`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::rev::reversed_lines;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let operands: Vec<&str> = argv[1..].iter().map(String::as_str).collect();

    // C `do { if (*argv) fopen or warn+continue; while(fgetwln reverse) } while(*argv)`.
    let mut status = 0;
    if operands.is_empty() {
        let input = read_stream(support::STDIN);
        emit_reversed(&input);
        support::terminate(0);
    }
    for operand in operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("rev: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        let input = read_stream(fd);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
        emit_reversed(&input);
    }
    support::terminate(status)
}

/// Read a descriptor to the end.
fn read_stream(fd: i32) -> Vec<u8> {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    input
}

/// Emit each reversed line with its newline.
fn emit_reversed(input: &[u8]) {
    for line in reversed_lines(input) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
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
