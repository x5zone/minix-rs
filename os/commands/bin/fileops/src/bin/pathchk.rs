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
//! The default mode asks the live system the same three questions C asks —
//! `pathconf(_PC_NAME_MAX)` twice and `pathconf(_PC_PATH_MAX)` once, plus a
//! `stat` per component (pathchk.c:103-161) — through the [`SysProbe`] below;
//! the judging itself lives in `pathchk::check_system`. On a host build there
//! is no MINIX file system to interrogate, so the two `pathconf` answers come
//! back `ENOSYS` and C's own fallback to `NAME_MAX`/`PATH_MAX` applies — the
//! same numbers MINIX would answer, because its `pathconf` reports those two
//! variables as header constants (`minix3/minix/lib/libc/sys/fpathconf.c`).
//! File operands do not apply here — every operand is a path to check.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::pathchk::{PathFault, PathProbe, SystemFault, check_portable, check_system};

/// The whole program body; both `main` forms call it and it never
/// returns (every path ends in [`support::terminate`]). The two-form
/// entry contract is documented once in `echo.rs`, this crate's
/// template binary.
fn run() -> ! {
    let argv: Vec<String> = support::args();
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
    let prog = support::prog_name(&argv);
    let mut status = 0;
    for path in &operands {
        if portable {
            if let Err(fault) = check_portable(path) {
                diagnose(prog, path, &fault);
                status = 1;
            }
            continue;
        }
        let mut probe = SysProbe;
        if let Err(fault) = check_system(path, &mut probe) {
            system_diagnose(prog, path, fault);
            status = 1;
        }
    }
    support::terminate(status);
}

/// The system face of C's default mode: the two `pathconf` questions and the
/// `stat` whose failure C reports.
///
/// On the target both probes are the real calls (`pathconf` opens the path and
/// asks the file service, `stat` fills a buffer only to have its errno tested).
/// On a host build there is no MINIX file service to interrogate, so the two
/// probes answer `ENOSYS` — which is precisely C's "failed with an errno set",
/// making the shell take C's own fallback to `NAME_MAX`/`PATH_MAX`, the same
/// numbers a MINIX file service answers (they are header constants there).
/// The `stat` question does have a host counterpart and uses it, because that
/// is what makes the `ENOTDIR` branch reachable from a host test.
pub struct SysProbe;

impl PathProbe for SysProbe {
    #[cfg(all(not(test), target_os = "none"))]
    fn name_max(&mut self, path: &str) -> Result<i64, minix_sys::Errno> {
        minix_sys::pathconf(path, minix_sys::PC_NAME_MAX)
    }

    #[cfg(any(test, not(target_os = "none")))]
    fn name_max(&mut self, _path: &str) -> Result<i64, minix_sys::Errno> {
        Err(minix_sys::Errno::ENOSYS)
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn path_max(&mut self, path: &str) -> Result<i64, minix_sys::Errno> {
        minix_sys::pathconf(path, minix_sys::PC_PATH_MAX)
    }

    #[cfg(any(test, not(target_os = "none")))]
    fn path_max(&mut self, _path: &str) -> Result<i64, minix_sys::Errno> {
        Err(minix_sys::Errno::ENOSYS)
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn stat(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
        // SAFETY: `Stat` is a plain byte struct of integers; all-zero is a valid
        // bit pattern that `stat` overwrites on success (the `du` shell's precedent).
        let mut buf: minix_sys::Stat = unsafe { core::mem::zeroed() };
        minix_sys::stat(path, &mut buf)
    }

    #[cfg(any(test, not(target_os = "none")))]
    fn stat(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
        std::fs::metadata(path)
            .map(|_| ())
            .map_err(|error| minix_sys::Errno::from_i32(raw_number(&error)))
    }
}

/// The errno a failed host `stat` left behind; an error without one is not a
/// system failure at all, and C has no such case here.
#[cfg(any(test, not(target_os = "none")))]
fn raw_number(error: &std::io::Error) -> i32 {
    error
        .raw_os_error()
        .unwrap_or(minix_sys::Errno::EIO.to_i32())
}

/// Renders one default-mode fault. C's `warnx` lines carry `getprogname()`
/// (`vwarnx.c`), and the `stat` failure is a `warn` (`pathchk.c:125-127`), so
/// that one line also carries the number's sentence.
fn system_diagnose(prog: &str, path: &str, fault: SystemFault<'_>) {
    match fault {
        SystemFault::ComponentTooLong { component, limit } => {
            let mut line = Vec::new();
            push(
                &mut line,
                &format!("{prog}: {path}: {component}: component too long (limit "),
            );
            push_int(&mut line, limit);
            line.extend_from_slice(b")\n");
            support::warn(&line);
        }
        SystemFault::StatFailed {
            prefix,
            whole,
            errno,
        } => {
            // C's `warn("%s: %.*s", path, strlen(pathd) - complen - 1, pathd)`.
            // A negative precision is C's "no precision", which prints the whole
            // string; `prefix` already carries whichever of the two applies.
            let shown = if whole { path } else { prefix };
            support::warn_line(prog, &format!("{path}: {shown}"), errno);
        }
        SystemFault::PathTooLong { limit } => {
            let mut line = Vec::new();
            push(&mut line, &format!("{prog}: {path}: path too long (limit "));
            push_int(&mut line, limit);
            line.extend_from_slice(b")\n");
            support::warn(&line);
        }
    }
}

/// Appends a string to a byte buffer without a format pass of its own.
fn push(line: &mut Vec<u8>, text: &str) {
    line.extend_from_slice(text.as_bytes());
}

/// The `%ld` of C's limit fields: the answer is `pathconf`'s, so it is signed
/// and can exceed what `utoa` would fit.
fn push_int(line: &mut Vec<u8>, value: i64) {
    let digits = if value < 0 {
        line.push(b'-');
        (value as u64).wrapping_neg()
    } else {
        value as u64
    };
    let mut stack = [0u8; 20];
    let mut filled = 0usize;
    let mut rest = digits;
    loop {
        stack[filled] = b'0' + (rest % 10) as u8;
        filled += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    while filled > 0 {
        filled -= 1;
        line.push(stack[filled]);
    }
}

/// Renders one fault in the C `warnx` shapes (pathchk.c:119, 131, 159). C's
/// `warnx` opens with `getprogname()` and `": "` (`vwarnx.c`), which this shell
/// used to leave off — then `<operand>: <component>: …` reads like a different
/// program's line, and a renamed copy would print the wrong name.
fn diagnose(prog: &str, path: &str, fault: &PathFault<'_>) {
    let mut line: Vec<u8> = Vec::new();
    push(&mut line, &format!("{prog}: {path}: "));
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

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
