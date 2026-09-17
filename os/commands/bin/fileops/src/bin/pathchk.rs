//! Minix-RS pathchk — path portability checking.
//!
//! Ground truth: `minix3/usr.bin/pathchk/pathchk.c` (NetBSD 1.2), main at
//! lines 53 to 80: `pathchk [-p] pathname...`; every operand is checked
//! and the exit status is 1 when any failed. The `-p` mode is fully
//! served by the library's `check_portable` (the `_POSIX_NAME_MAX` and
//! `_POSIX_PATH_MAX` limits and the portable character set are compile
//! time constants, so no system query is involved); diagnostics render to
//! standard error in the C `warnx` shapes.
//!
//! The default mode probes the live system through `pathconf` and `stat`
//! (pathchk.c:103-107, 124-128, 151-155), which this build does not carry,
//! so it is rejected with an explicit message rather than answered from
//! guessed limits. File operands do not apply here — every operand is a
//! path to check.

#[path = "../bin_support.rs"]
mod support;

use minix_fileops::pathchk::{check_portable, PathFault};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut portable = false;
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if arg.starts_with('-') && arg.len() > 1 {
            for flag in arg[1..].bytes() {
                match flag {
                    b'p' => portable = true,
                    _ => usage(),
                }
            }
            continue;
        }
        operands.push(arg);
    }
    if operands.is_empty() {
        usage();
    }
    if !portable {
        support::warn(b"pathchk: the default mode waits for stat and pathconf (edge E-CMDSYSFACE); -p is served\n");
        support::terminate(1);
    }

    let mut status = 0;
    for path in &operands {
        if let Err(fault) = check_portable(path) {
            diagnose(path, &fault);
            status = 1;
        }
    }
    support::terminate(status);
}

/// Renders one fault in the C `warnx` shapes (pathchk.c:119, 131, 159).
fn diagnose(path: &str, fault: &PathFault<'_>) {
    let mut line: Vec<u8> = Vec::new();
    line.extend_from_slice(path.as_bytes());
    line.extend_from_slice(b": ");
    match *fault {
        PathFault::ComponentTooLong { component } => {
            line.extend_from_slice(component.as_bytes());
            line.extend_from_slice(b": component too long (limit 14)\n");
        }
        PathFault::NonPortableByte { component, byte } => {
            line.extend_from_slice(component.as_bytes());
            line.extend_from_slice(b": component contains non-portable character `");
            // The C prints the raw byte with %c; non-ASCII bytes ride as-is.
            line.push(byte);
            line.extend_from_slice(b"'\n");
        }
        PathFault::PathTooLong => {
            line.extend_from_slice(b"path too long (limit 255)\n");
        }
    }
    support::warn(&line);
}

fn usage() -> ! {
    support::warn(b"usage: pathchk [-p] pathname...\n");
    support::terminate(1);
}
