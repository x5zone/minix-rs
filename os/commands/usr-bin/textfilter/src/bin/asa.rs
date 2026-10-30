//! Minix-RS `asa` — the doing half over `minix_textfilter::asa`.
//!
//! Ground truth: `minix3/usr.bin/asa/asa.c`. The carriage-control decoding is
//! the library (`asa`); this program reads standard input when given no operand,
//! otherwise processes each file operand *independently* (as C does, calling
//! `asa(fp)` per file, so each file keeps its own first-line special case and
//! terminating newline) and writes the reformatted text to standard output.
//!
//! Wired: zero operands → standard input; one or more file operands → each is
//! opened and formatted in order. A file that cannot be opened reports a warning
//! and is skipped; `asa` always exits 0 (`asa.c:66`), matching C. Declared
//! residual: the `warn` system-message text on an open failure is not reproduced
//! (only the operand name is reported).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::asa::asa;

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let files: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();

    if files.is_empty() {
        let input = read_stdin();
        support::emit(&asa(&input));
    } else {
        for name in &files {
            // The open entry wants `&str` (registered under `ARGV-BYTE-PATH-IN-LIBS`),
            // so the fold happens only at this point; the message still reports the
            // raw bytes.
            let shown = String::from_utf8_lossy(name).into_owned();
            match support::read_file(&shown) {
                Ok(data) => support::emit(&asa(&data)),
                Err(_) => {
                    let mut msg: Vec<u8> = name.to_vec();
                    msg.extend_from_slice(b": cannot open\n");
                    support::warn(&msg);
                }
            }
        }
    }

    // C returns 0 unconditionally (`asa.c:66`), even when a file failed to open.
    support::terminate(0);
}

fn read_stdin() -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    out
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
