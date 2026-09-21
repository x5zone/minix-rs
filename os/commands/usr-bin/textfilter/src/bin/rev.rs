//! Minix-RS rev — reverse each line's characters.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 57-95:
//! file operands or stdin, every line's characters print in reverse with
//! a newline, open or read failures set status 1, and the usage line is
//! `rev [file ...]`. File operands wait for the gated open-existing
//! call, so this build serves stdin.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::rev::reversed_lines;
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.len() > 1 {
        usage();
    }
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    for line in reversed_lines(&input) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: rev [file ...]\n");
    support::terminate(1);
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
