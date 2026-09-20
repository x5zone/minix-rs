//! Minix-RS cat — concatenate and print files.
//!
//! Ground truth: `minix3/bin/cat/cat.c` (NetBSD): each operand is read in
//! order and written to standard output; with no operand (or `-`) standard
//! input is read. `-u` is accepted and meaningless here — the doing half
//! was never buffered (one `minix_sys::read` chunk out per call, the C
//! unbuffered shape). Any other option is a usage failure like the C
//! `usage()`; a missing or unreadable file warns and carries the failure
//! in the exit status while later operands still run (`main.c:158-170`).
//!
//! The output channel follows the stage contract (99-global-concepts.md
//! §1): no stdio library, writes through `minix_sys::write`. The
//! hosted-versus-target split rides the crate's shared seams
//! (`../bin_support.rs`); the entry contract is documented once in
//! `echo.rs`, this crate's template binary.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;

/// The read chunk size (the C `cat` reads in `BUFSIZ` steps; one read out
/// per chunk is the unbuffered contract).
const CHUNK: usize = 4096;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut operands: Vec<String> = Vec::new();
    for arg in argv.iter().skip(1) {
        match arg.as_str() {
            "-u" => {}
            "--" => continue,
            _ if arg.starts_with('-') && arg.len() > 1 => {
                support::warn(b"usage: cat [-u] [file ...]\n");
                support::terminate(1);
            }
            _ => operands.push(arg.clone()),
        }
    }

    let mut status = 0;
    if operands.is_empty() {
        copy_stream(support::STDIN);
    }
    for operand in operands {
        if operand == "-" {
            copy_stream(support::STDIN);
            continue;
        }
        let fd = match minix_sys::open(&operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("cat: {operand}: cannot open\n").as_bytes());
                status = 1;
                continue;
            }
        };
        copy_stream(fd);
        let _ = minix_sys::close(fd);
    }
    support::terminate(status)
}

/// Reads `fd` to the end and writes every byte out; a read error warns and
/// marks the failure (C `cat`'s `read_error`, cat.c:117-121 — the run
/// continues, the status stays nonzero).
fn copy_stream(fd: i32) {
    let mut chunk = [0u8; CHUNK];
    loop {
        match minix_sys::read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => support::emit(&chunk[..n]),
            Err(_) => {
                support::warn(b"cat: read error\n");
                support::terminate(1);
            }
        }
    }
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
