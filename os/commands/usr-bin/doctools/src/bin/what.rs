//! Minix-RS `what` — the doing half over `minix_doctools::what`.
//!
//! Ground truth: `minix3/usr.bin/what/what.c`. The option scan and the
//! `@(#)` identifier extractor live in `minix_doctools::what`; this program
//! supplies the file leg — for each operand, `open`/`read` the bytes
//! (`what.c:84`, `freopen`), print the operand name (`what.c:88`), then print
//! each identifier the extractor finds, tab-indented (`search()`'s
//! `putchar('\t') … putchar('\n')`).
//!
//! Faithful corners: a file that cannot be opened stops the whole program, not
//! just the current operand — C's `perror(*argv); exit(matches ? EXIT_SUCCESS :
//! 1)` at `what.c:84-87` — and the exit status is `0` if any identifier was
//! found in any file, else `1` (`what.c:93`, where "the standard explicitly
//! specifies an exit status of 1" when none matched). Read-only, so like
//! `whatis`/`which` it is not gated on the writable-root E5 check; real-machine
//! per-file `open` is the same C-57/E5 delivery gate as every guest binary.
//! C's `perror` `strerror` suffix is not reproduced (no errno→text table).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_doctools::what::{WhatFlags, extract_ids};
use minix_sys::{Fd, close, open, read};

fn usage() -> ! {
    support::warn(b"usage: what [-s] file ...\n");
    support::terminate(1);
}

/// Read a whole file into memory (C's `freopen` + `getchar` stream).
fn read_file(path: &str) -> Option<Vec<u8>> {
    let fd: Fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => return None,
    };
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => {
                let _ = close(fd);
                return None;
            }
        }
    }
    let _ = close(fd);
    Some(image)
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match WhatFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => usage(),
    };
    if operands.is_empty() {
        usage();
    }

    let mut matched = false;
    for name in &operands {
        // C exits the whole program on the first unopenable operand
        // (what.c:84-87); success if it had already found something.
        let image = match read_file(name) {
            Some(v) => v,
            None => {
                support::warn(format!("what: {name}: cannot open\n").as_bytes());
                support::terminate(if matched { 0 } else { 1 });
            }
        };
        support::emit(format!("{name}\n").as_bytes());
        let ids = extract_ids(&image, flags.stop_first);
        if !ids.is_empty() {
            matched = true;
        }
        for id in ids {
            let line = format!("\t{}\n", String::from_utf8_lossy(id));
            support::emit(line.as_bytes());
        }
    }

    support::terminate(if matched { 0 } else { 1 });
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
