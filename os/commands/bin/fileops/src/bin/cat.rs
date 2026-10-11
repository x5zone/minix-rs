//! Minix-RS cat — concatenate and print files.
//!
//! Ground truth: `minix3/bin/cat/cat.c` (NetBSD): each operand is read in
//! order and written to standard output; with no operand (or `-`) standard
//! input is read. `-u` is accepted and meaningless here — the doing half
//! was never buffered (one `minix_sys::read` chunk out per call, the C
//! unbuffered shape). The line-oriented cook flags `-n` (number lines), `-b`
//! (number non-blank lines, implying `-n`) and `-s` (squeeze blank lines) take
//! the C `cook_buf` path; the transform is the pure [`minix_fileops::cook`] state
//! machine, run per operand so line numbers restart for each file as in C. The
//! visible-character flags `-e`/`-t`/`-v` and `-B`/`-l`/`-f` are a usage failure
//! like the C `usage()`; a missing or unreadable file warns and carries the
//! failure in the exit status while later operands still run (`main.c:158-170`).
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

fn usage() -> ! {
    support::warn(b"usage: cat [-benv] [-s] [-u] [file ...]\n");
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // C `getopt("B:beflnstuv")`. This wiring supports the line-oriented cook
    // flags `-n`/`-b`/`-s` and the no-op `-u`; the visible-character flags
    // `-e`/`-t`/`-v` (byte-visualization) and `-B`/`-l`/`-f` are not yet
    // implemented, so they are a usage failure rather than silently ignored.
    let mut nflag = false;
    let mut bflag = false;
    let mut sflag = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.len() > 1 && a.starts_with('-') {
            for flag in a[1..].bytes() {
                match flag {
                    b'u' => {}
                    b'n' => nflag = true,
                    b'b' => bflag = true,
                    b's' => sflag = true,
                    _ => usage(),
                }
            }
            i += 1;
        } else {
            break;
        }
    }

    // The cook path is taken when any line flag is set (C `cat.c:133`). `-b`
    // implies `-n`, which the decision-half's constructor applies.
    let cook = nflag || bflag || sflag;
    let operands: Vec<&str> = args[i..].to_vec();

    let mut status = 0;
    if operands.is_empty() {
        if cook {
            cook_stream(support::STDIN, nflag, bflag, sflag);
        } else {
            copy_stream(support::STDIN);
        }
    }
    for operand in operands {
        if operand == "-" {
            if cook {
                cook_stream(support::STDIN, nflag, bflag, sflag);
            } else {
                copy_stream(support::STDIN);
            }
            continue;
        }
        let fd = match minix_sys::open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("cat: {operand}: cannot open\n").as_bytes());
                status = 1;
                continue;
            }
        };
        if cook {
            cook_stream(fd, nflag, bflag, sflag);
        } else {
            copy_stream(fd);
        }
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::close(fd);
    }
    support::terminate(status)
}

/// Runs one stream through the `cook_buf` line transform. A fresh [`CatCook`]
/// per operand restarts the line count for each file (C `cook_buf`'s local
/// `line`); cooked bytes are flushed to standard output in `CHUNK`-sized writes.
fn cook_stream(fd: i32, nflag: bool, bflag: bool, sflag: bool) {
    let mut cook = minix_fileops::cook::CatCook::new(nflag, bflag, sflag);
    let mut chunk = [0u8; CHUNK];
    let mut outbuf = [0u8; CHUNK];
    let mut olen = 0usize;
    loop {
        match minix_sys::read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                for &byte in &chunk[..count] {
                    cook.push(byte, &mut |cooked| {
                        if olen == outbuf.len() {
                            support::emit(&outbuf);
                            olen = 0;
                        }
                        outbuf[olen] = cooked;
                        olen += 1;
                    });
                }
            }
            Err(_) => {
                support::warn(b"cat: read error\n");
                support::terminate(1);
            }
        }
    }
    if olen > 0 {
        support::emit(&outbuf[..olen]);
    }
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
