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
//! The report line is C's `perror(*argv)` rendered by the one authority in
//! `minix-types`: `<operand>: <strerror(errno)>` — no program name (Minix3's
//! `perror` prints none, unlike `warn(3)`/`err(3)`) and no invented phrase, and the
//! run leaves right after it. A *read* error is not that leg: C's `getchar()` reports
//! end of file for it too, so this program keeps what it read and moves on.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_doctools::what::{WhatFlags, extract_ids};
use minix_sys::{Errno, Fd, close, open, read};

fn usage() -> ! {
    support::warn(b"usage: what [-s] file ...\n");
    support::terminate(1);
}

/// Read a whole file into memory (C's `freopen` + `getchar` stream). Only the open
/// is a reportable failure: a read error is what C's `getchar()` turns into end of
/// file, so the bytes already read are the answer and the operand is not a failure.
fn read_file(path: &str) -> Result<Vec<u8>, Errno> {
    let fd: Fd = open(path, 0, 0)?;
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    let _ = close(fd);
    Ok(image)
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
            Ok(v) => v,
            Err(code) => {
                // `what.c:84-87`: `perror(*argv)` then `exit(matches ? EXIT_SUCCESS : 1)`.
                support::perror_line(name, code);
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
