//! Minix-RS `what` — the doing half over `minix_textfilter::what`.
//!
//! Ground truth: `minix3/usr.bin/what/what.c`. The `@(#)` scan lives in the
//! library (`what_matches`); this program opens each file operand, prints the
//! operand name, and lists each identification string tab-indented on its own
//! line, tracking the total number of matches across all files for the exit
//! status.
//!
//! Wired: one or more file operands (`what` has no stdin mode, `what.c:81`);
//! each file's name is echoed, then its matches. `-s` keeps only the first
//! match per file (C breaks the scan loop, `what.c:117-118`). Exit status is 0
//! if *any* file matched, 1 otherwise (`what.c:93`). A file that cannot be
//! opened stops the run and exits by the matches gathered so far
//! (`what.c:84-86`). Declared residual: the exact `perror` system-message text
//! on an open failure is not reproduced (only the operand name is reported).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_textfilter::what::what_matches;

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut sflag = false;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            files.extend(args[i + 1..].iter().map(|s| s.to_string()));
            break;
        }
        if a == "-s" {
            sflag = true;
            i += 1;
            continue;
        }
        if !a.starts_with('-') || a.len() == 1 {
            files.push(a.to_string());
            i += 1;
            continue;
        }
        // Any other flag is a usage error (C's getopt '?'/default).
        usage();
    }

    if files.is_empty() {
        usage();
    }

    let mut total = 0usize;
    for name in &files {
        let data = match support::read_file(name) {
            Ok(d) => d,
            Err(_) => {
                let mut msg = String::from(name.as_str());
                msg.push_str(": cannot open\n");
                support::warn(msg.as_bytes());
                support::terminate(if total > 0 { 0 } else { 1 });
            }
        };
        let mut out: Vec<u8> = Vec::new();
        out.extend(name.as_bytes());
        out.push(b'\n');
        let mut runs = what_matches(&data);
        if sflag {
            runs.truncate(1);
        }
        for run in &runs {
            out.push(b'\t');
            out.extend_from_slice(run);
            out.push(b'\n');
        }
        total += runs.len();
        support::emit(&out);
    }

    support::terminate(if total > 0 { 0 } else { 1 });
}

fn usage() -> ! {
    support::warn(b"usage: what [-s] file ...\n");
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
