//! Minix-RS `fpr` — the doing half over `minix_doctools::fpr`.
//!
//! Ground truth: `minix3/usr.bin/fpr/fpr.c`. The program takes no operands
//! and no options: it reads standard input, expands Fortran carriage control
//! and overstrikes (the whole transform is `fpr::pretty_print`), writes the
//! result to standard output, and — if any line had an illegal carriage-control
//! character — reports the count on standard error (`fpr.c:166-168`). It
//! always exits 0.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::vec::Vec;
use minix_doctools::fpr::pretty_print;
use minix_sys::read;

fn run() -> ! {
    // C reads only standard input (getchar); no operands, no getopt.
    let mut input: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(0, &mut chunk) {
            Ok(0) => break,
            Ok(n) => input.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }

    let (out, errors) = pretty_print(&input);

    if errors > 0 {
        let mut msg: Vec<u8> = Vec::new();
        msg.extend_from_slice(b"Illegal carriage control - ");
        msg.extend_from_slice(alloc::format!("{}", errors).as_bytes());
        msg.extend_from_slice(if errors == 1 {
            b" line.\n"
        } else {
            b" lines.\n"
        });
        support::warn(&msg);
    }

    support::emit(&out);
    support::terminate(0);
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
