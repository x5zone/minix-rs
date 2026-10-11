//! Minix-RS paste — merge lines of files side by side.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD), `paste [-s]
//! [-d list] file ...`. With no operand a single standard-input stream is
//! read; each `-` operand also names standard input, and any other operand is
//! opened (an unopenable file is fatal, matching C `fpp == NULL → err`). In
//! parallel (default) mode one line is taken from each stream per output row,
//! shorter streams contributing an empty cell; in `-s` mode each stream's lines
//! are joined into a single row before the next stream. The delimiter list
//! cycles per gap and restarts each row/serial group. Both merges are the
//! library's tested `join_parallel` / `join_serial`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::paste::{join_parallel, join_serial, parse_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let (options, operands) = match parse_args(&words) {
        Ok(parsed) => parsed,
        Err(_) => {
            usage();
        }
    };

    // Gather one line vector per stream: no operands is one stdin stream; each
    // operand is `-` (stdin) or a file. An unopenable file is fatal (C err).
    let mut streams: Vec<Vec<String>> = Vec::new();
    if operands.is_empty() {
        streams.push(split_lines(&support::read_stdin()));
    } else {
        for operand in &operands {
            let input = if *operand == "-" {
                support::read_stdin()
            } else {
                read_file(operand)
            };
            streams.push(split_lines(&input));
        }
    }

    let refs: Vec<Vec<&str>> = streams
        .iter()
        .map(|stream| stream.iter().map(String::as_str).collect())
        .collect();

    if options.serial {
        for stream in &refs {
            support::emit(join_serial(stream, &options).as_bytes());
        }
    } else {
        let slices: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
        support::emit(join_parallel(&slices, &options).as_bytes());
    }
    support::terminate(0)
}

/// Read a named operand; an unopenable file is fatal (C `fpp == NULL` err).
fn read_file(path: &str) -> Vec<u8> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("paste: {path}\n").as_bytes());
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

/// Split bytes into newline-stripped lines, dropping the empty piece after a
/// trailing newline (interior blank lines stay as empty cells).
fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

fn usage() -> ! {
    support::warn(b"paste: [-s] [-d delimiters] file ...\n");
    support::terminate(2)
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
