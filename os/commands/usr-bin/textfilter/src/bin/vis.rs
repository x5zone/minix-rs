//! Minix-RS vis — visible encoding of stdin.
//!
//! Ground truth: `minix3/usr.bin/vis/vis.c` (NetBSD). The deciding half
//! (the M-notation grammar, C-style named escapes, and the octal form)
//! is the library's `vis` module; this program reads stdin, encodes,
//! and prints. `-c` selects C-style escapes, `-o` three-digit octal,
//! and `-n` drops the backslash prefix — matching the C's getopt
//! `bcde:F:fhlMmNnoSstw` subset (vis.c:73). File operands wait for the
//! gated open-existing call.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::vis::{encode, VisOptions};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = VisOptions::default();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-c" {
            options.cstyle = true;
        } else if arg == "-o" {
            options.octal = true;
        } else if arg == "-n" {
            options.no_slash = true;
        } else {
            support::warn(b"vis: file operands and the remaining flags wait for their batches\n");
            support::terminate(1);
        }
        index += 1;
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
    let encoded = encode(&input, &options);
    support::emit(encoded.as_bytes());
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
