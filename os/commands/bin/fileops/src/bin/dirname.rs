//! Minix-RS dirname — strip the last component.
//!
//! Ground truth: `minix3/usr.bin/dirname/dirname.c` (NetBSD), main at
//! lines 55 to 74: `dirname string`, exactly one operand; a malformed
//! invocation or an unsplittable path is a failure. The splitting rules
//! live in the library's `path` module (the deciding half, fully tested);
//! this program gathers argv and prints the result.

#[path = "../bin_support.rs"]
mod support;

use minix_fileops::path::dirname;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if operands.is_empty() && arg.starts_with('-') && arg.len() > 1 {
            support::emit(b"usage: dirname string\n");
            support::terminate(1);
        }
        operands.push(arg);
    }
    if operands.len() != 1 {
        support::emit(b"usage: dirname string\n");
        support::terminate(1);
    }

    let directory = match dirname(operands[0]) {
        Ok(directory) => directory,
        Err(_) => support::terminate(1),
    };
    let mut line = directory.as_bytes().to_vec();
    line.push(b'\n');
    support::emit(&line);
    support::terminate(0);
}
