//! Minix-RS column — columnate lines into a table.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). The
//! deciding half (entry collection, the two columnate layouts, and the
//! `-t` table alignment) is the library's `column` module; this program
//! reads stdin, lays the entries out, and prints. File operands wait
//! for the gated open-existing call; the terminal width comes from
//! `-c` (the C queries the window size, which belongs to the terminal
//! face).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::column::{columnate, parse_entries, ColumnOptions};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = ColumnOptions::default();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-t" {
            options.table = true;
        } else if arg == "-x" {
            options.fill_rows_first = true;
        } else if arg == "-s" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            options.separator = argv[index].clone();
        } else if arg == "-c" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match argv[index].parse::<usize>() {
                // pr.c:1875-1878's style guard: the width must be sane.
                Ok(width) if width > 0 => options.termwidth = width,
                _ => usage(),
            }
        } else {
            support::warn(b"column: file operands wait for the open call (edge E-CMDSYSFACE)\n");
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
    let text = core::str::from_utf8(&input).unwrap_or("");
    let lines: Vec<&str> = text.split('\n').collect();
    let entries = parse_entries(&lines);
    for line in columnate(&entries, &options) {
        support::emit(line.as_bytes());
        support::emit(b"\n");
    }
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: column [-tx] [-c width] [-s separator]\n");
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
