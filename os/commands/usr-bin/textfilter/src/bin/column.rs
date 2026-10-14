//! Minix-RS column — columnate lines into a table.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). The
//! deciding half (entry collection, the two columnate layouts, and the
//! `-t` table alignment) is the library's `column` module; this program
//! reads stdin or file operands, lays the entries out, and prints. With
//! file operands each is opened in turn and fed through one shared entry
//! pool (`column.c:114-123`); an unopenable operand warns and only sets
//! the exit status, and the run continues to the next file. A bare `-` is
//! a literal file name (`fopen`), not a stdin alias. The terminal width
//! comes from `-c` (the C queries the window size, which belongs to the
//! terminal face).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::column::{ColumnError, columnate, parse_column_args, parse_entries};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (options, operands) = match parse_column_args(&args[1..]) {
        Ok(a) => (a.options, a.operands),
        Err(ColumnError::Usage) => usage(),
    };

    // `column.c:114-123`: no operands reads stdin; otherwise each operand is
    // opened and fed through the same entry pool, and an unopenable operand
    // warns, flags the exit status (`eval = 1`) and lets the run continue.
    let mut entries: Vec<String> = Vec::new();
    let mut rval = 0;
    if operands.is_empty() {
        entries.extend(parse_from(&support::read_stdin()));
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(bytes) => entries.extend(parse_from(&bytes)),
                Err(_) => {
                    support::warn(format!("column: Cannot open `{operand}'\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    for line in columnate(&entries, &options) {
        support::emit(line.as_bytes());
        support::emit(b"\n");
    }
    support::terminate(rval);
}

/// Collects one source's bytes into the entry pool: decode (drop the whole
/// input on invalid UTF-8, matching the stdin path), split on newlines, then
/// keep the non-blank trimmed entries (the C `add`/`input` behaviour).
fn parse_from(bytes: &[u8]) -> Vec<String> {
    let text = core::str::from_utf8(bytes).unwrap_or("");
    let lines: Vec<&str> = text.split('\n').collect();
    parse_entries(&lines)
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
