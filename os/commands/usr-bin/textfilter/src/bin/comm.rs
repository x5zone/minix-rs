//! Minix-RS comm — three-column merge of two sorted streams.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 64-160: `-1`/`-2`/`-3` suppress a column, `-f` folds the comparison,
//! column two carries one tab and column three two, both streams drain
//! at end of input, and the status is 0 (usage leaves 1). The merge is
//! the library's `comm` module. File operands wait for the gated
//! open-existing call, so this build compares two reads of stdin —
//! which compare equal — only structurally.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::comm::{comm, CommOptions};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = CommOptions::default();
    let mut index = 1;
    while index < argv.len() {
        match argv[index].as_str() {
            "-1" => options.suppress1 = true,
            "-2" => options.suppress2 = true,
            "-3" => options.suppress3 = true,
            "-f" => options.fold = true,
            _ => {
                support::warn(b"comm: file operands wait for the open call (edge E-CMDSYSFACE)\n");
                support::terminate(2);
            }
        }
        index += 1;
    }

    // Two reads of standard input cannot be replayed, so the doing half
    // collects one stream and merges it against itself: the common
    // column carries every line.
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let lines = split_lines(&input);
    let mirror: Vec<String> = lines.clone();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mirrored: Vec<&str> = mirror.iter().map(String::as_str).collect();
    let merged = comm(&borrowed, &mirrored, &options);
    let tabs = ["", "\t", "\t\t"];
    for entry in &merged {
        let mut out = tabs[(entry.column - 1) as usize].as_bytes().to_vec();
        out.extend_from_slice(entry.text.as_bytes());
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
}

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


#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
