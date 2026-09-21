//! Minix-RS colrm — remove columns from each line.
//!
//! Ground truth: `minix3/usr.bin/colrm/colrm.c` (NetBSD), main at lines
//! 61-121: `colrm [start [stop]]`; every input byte advances a display
//! column (tab rounds up to the next multiple of eight, backspace pulls
//! one back, newline resets) and bytes whose new column falls inside
//! the removed range are dropped. The deciding half (the column walk
//! and the keep/drop rule) is the library's `colrm` module; this
//! program parses the operands and runs the filter over stdin. File
//! operands are rejected — the C takes none.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::colrm::filter_column;
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if arg.starts_with('-') && arg.len() > 1 {
            usage();
        }
        operands.push(arg);
    }
    let (start, stop) = match operands.len() {
        0 => (0, 0),
        1 => (parse_column(operands[0]), (0, 0).1),
        2 => {
            let start = parse_column(operands[0]);
            let stop = parse_column(operands[1]);
            if start > stop {
                support::warn(b"colrm: illegal start and stop columns\n");
                support::terminate(1);
            }
            (start, stop)
        }
        _ => usage(),
    };

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
    support::emit(&filter_column(text.as_bytes(), start, stop));
    support::terminate(0);
}

/// Parses one column operand (the C's `strtol` semantics: digits only,
/// strictly positive).
fn parse_column(word: &str) -> usize {
    match word.parse::<usize>() {
        Ok(value) if value > 0 => value,
        _ => {
            support::warn(b"colrm: illegal column\n");
            support::terminate(1);
        }
    }
}

fn usage() -> ! {
    support::warn(b"usage: colrm [start [stop]]\n");
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
