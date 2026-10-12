//! Minix-RS comm — three-column merge of two sorted streams.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 66-164: `-1`/`-2`/`-3` suppress a column, `-f` folds the comparison,
//! column two carries one tab and column three two, both streams drain
//! at end of input, and the status is 0 (usage and an unopenable file
//! leave 1). Exactly two operands are required; each `-` names standard
//! input. The merge itself is the library's tested `comm` module.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::comm::{comm, parse_comm_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (options, files) = match parse_comm_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(_) => usage(),
    };

    let lines1 = operand_lines(files[0]);
    let lines2 = operand_lines(files[1]);
    let a: Vec<&str> = lines1.iter().map(String::as_str).collect();
    let b: Vec<&str> = lines2.iter().map(String::as_str).collect();
    let merged = comm(&a, &b, &options);
    let tabs = ["", "\t", "\t\t"];
    for entry in &merged {
        let mut out = tabs[(entry.column - 1) as usize].as_bytes().to_vec();
        out.extend_from_slice(entry.text.as_bytes());
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0)
}

/// Read one operand (`-` is standard input) and split it into newline-stripped
/// lines. An unopenable named file is fatal (C `file()` `err(1)`).
fn operand_lines(name: &str) -> Vec<String> {
    let input = if name == "-" {
        support::read_stdin()
    } else {
        read_file(name)
    };
    split_lines(&input)
}

/// Open, read to the end, and close a file, or exit 1 (matching C `file()`).
fn read_file(path: &str) -> Vec<u8> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("comm: {path}\n").as_bytes());
            support::terminate(1);
        }
    };
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => data.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    //[SD19-gate] SD-19: close 后 fd 不再使用
    #[allow(clippy::let_underscore_must_use)]
    let _ = close(fd);
    data
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

fn usage() -> ! {
    support::warn(b"usage: comm [-123f] file1 file2\n");
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
