//! Minix-RS `rmdir` — the doing half over `minix_fileops::rmdir`.
//!
//! Ground truth: `minix3/bin/rmdir/rmdir.c`. Each operand is removed with
//! `rmdir(2)` (the kernel rejects a non-empty directory). With `-p`, once the
//! operand itself is gone, the ancestors are removed innermost first until a
//! removal fails; the root is never removed. The pure ancestor walk lives in
//! [`minix_fileops::rmdir::for_each_ancestor`]; this shell owns the system
//! calls, the warning channel, and the exit code.
//!
//! A write command: removing directories needs a writable root, so the real
//! run is gated with the write-class commands (`C-57`/E5). The decision half and
//! the operand walk are host/compile verified here.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_sys::rmdir;

fn usage() -> ! {
    support::warn(b"usage: rmdir [-p] directory ...\n");
    support::terminate(1);
}

/// `rmdir.c` `rm_path`: remove the ancestors of `path` innermost first, stopping
/// at the first failure (the C loop returns 1 on the first `rmdir` error).
fn remove_ancestors(path: &str) -> bool {
    let mut ok = true;
    minix_fileops::rmdir::for_each_ancestor(path, &mut |ancestor| {
        if !ok {
            return;
        }
        if rmdir(ancestor).is_err() {
            support::warn(format!("rmdir: {ancestor}\n").as_bytes());
            ok = false;
        }
    });
    ok
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // getopt "p": only the `-p` flag is an option; anything else after a `-`
    // is a usage error, and a bare operand ends option scanning.
    let mut pflag = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.len() >= 2 && a.starts_with('-') {
            for byte in a.as_bytes()[1..].iter() {
                match byte {
                    b'p' => pflag = true,
                    _ => usage(),
                }
            }
            i += 1;
        } else {
            break;
        }
    }

    if i >= args.len() {
        usage();
    }

    // C: `for (errors = 0; *argv; argv++) { if rmdir fails → warn, errors=1;
    // else if pflag → errors |= rm_path }`. A leaf failure skips rm_path.
    let mut errors = 0;
    for operand in &args[i..] {
        if rmdir(operand).is_err() {
            support::warn(format!("rmdir: {operand}\n").as_bytes());
            errors = 1;
        } else if pflag && !remove_ancestors(operand) {
            errors = 1;
        }
    }
    support::terminate(errors)
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
