//! Minix-RS uniq — adjacent duplicate filter.
//!
//! Ground truth: `minix3/usr.bin/uniq/uniq.c` (NetBSD), `uniq [-c | -d |
//! -u] [-i] [-f fields] [-s chars] [input [output]]`. The run grouping,
//! mode selection, and count formatting are the library's (`Uniq`,
//! `UniqMode`, `Run::format`, which prints the count prefix without the
//! trailing newline — the doing half adds it). Input is stdin, split into
//! newline-stripped lines before the run handler sees them. The
//! case-fold (`-i`) and field-skip (`-f`, `-s`) comparisons are not in
//! the library yet and are reported as usage failures rather than being
//! silently ignored. The input operand (read a file instead of stdin) is wired;
//! the second operand (an output file, a create-and-write path) is reported as
//! not modelled rather than truncating a file, so use a shell redirect.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::uniq::{Uniq, UniqMode};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut mode = UniqMode::All;
    let mut mode_seen = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let option = argv[index].as_str();
        if !option.starts_with('-') || option.len() == 1 {
            operands.push(option);
            index += 1;
            continue;
        }
        for flag in option[1..].bytes() {
            let next = match flag {
                b'c' => UniqMode::Counted,
                b'd' => UniqMode::Repeated,
                b'u' => UniqMode::Unique,
                // -i, -f, -s land here: comparisons the library does not
                // model yet (edge of this batch, see 07 §4.4).
                _ => support::terminate(1),
            };
            if mode_seen && mode != next {
                // Composed modes (-c with -d/-u) need library support.
                support::terminate(1);
            }
            mode = next;
            mode_seen = true;
        }
        index += 1;
    }

    // C `switch (argc) { 0: stdin/stdout; 1: file(argv[0])/stdout;
    // 2: file(argv[0])/file(argv[1]); default: usage }`. The output-file form
    // (a create-and-write path) is not modelled; the input file and stdout are.
    if operands.len() > 2 {
        usage();
    }
    if operands.len() == 2 {
        support::warn(b"uniq: the output-file operand is not modelled; redirect with the shell\n");
        support::terminate(1);
    }
    let input = match operands.first() {
        None => support::read_stdin(),
        Some(path) => read_file(path),
    };
    let lines = support::lines_of(&input);
    let mut handler = Uniq::new(mode);
    let mut out = [0u8; 4096];
    for line in &lines {
        if let Some(run) = handler.push(line) {
            report(&mut handler, &run, &mut out);
        }
    }
    if let Some(run) = handler.finish() {
        report(&mut handler, &run, &mut out);
    }
    support::terminate(0);
}

fn report(handler: &mut Uniq<'_>, run: &minix_textfilter::uniq::Run<'_>, out: &mut [u8]) {
    if !handler.keep(run) {
        return;
    }
    match handler.format(run, out) {
        Ok(len) => {
            support::emit(&out[..len]);
            support::emit(b"\n");
        }
        Err(_) => support::terminate(1),
    }
}

/// Read an input file to the end; an unopenable file is fatal (C `file()`
/// `err`s), matching the input-only path.
fn read_file(path: &str) -> Vec<u8> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("uniq: {path}\n").as_bytes());
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

fn usage() -> ! {
    support::warn(b"usage: uniq [-c | -d | -u] [input [output]]\n");
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
