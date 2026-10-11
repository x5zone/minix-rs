//! Minix-RS `which` — the doing half over `minix_doctools::which`.
//!
//! Ground truth: `minix3/usr.bin/whereis/whereis.c`, the `which` personality
//! selected by `strcmp(getprogname(), "which")` (`whereis.c:66`). The option
//! scan, path splitting, absolute-name test, and exit rule live in
//! `minix_doctools::which`; this program supplies the filesystem leg — read
//! `$PATH`, and for each operand `stat` + `access(X_OK)` the candidates
//! (`whereis.c:113-156`), printing the first match per name (or all under
//! `-a`) and exiting `3`/`0`/`2`.
//!
//! Read-only (no file creation, no exec) so it is not gated on the
//! writable-root E5 check — like `pwd`/`printenv`/`env` it runs for real once
//! the guest binary is delivered.
//!
//! Declared residuals: the `whereis` personality (its default search source is
//! `sysctl user.cs_path`, not `$PATH`, and it reports matches across all path
//! elements) is a separate command entry not wired here; C's `setgid(egid)`/
//! `setuid(euid)` prologue (`whereis.c:71-75`), which only matters so that
//! `access(2)` checks the effective ids for setuid-script lookups, is not
//! reproduced (a normal command already checks against its own real ids).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_doctools::which::{WhichFlags, compute_status, is_absolute, path_candidates};
use minix_sys::{Stat, access, stat};

// File-type bits, matching C `sys/sys/stat.h:138,143` (`_S_IFMT 0170000`,
// `_S_IFREG 0100000`); `X_OK` is C `sys/sys/unistd.h:169` (0x01).
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const X_OK: i32 = 0x01;

fn s_isreg(m: u32) -> bool {
    m & S_IFMT == S_IFREG
}

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid
    // bit pattern that `stat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

/// C's per-candidate acceptance: it exists, is a regular file, and is
/// executable (`whereis.c:141-146`: `stat` → `S_ISREG` → `access(X_OK)`).
fn is_executable(path: &str) -> bool {
    let mut sb = zeroed();
    stat(path, &mut sb).is_ok() && s_isreg(sb.st_mode) && access(path, X_OK).is_ok()
}

/// The value of `PATH` in the environment, or `None` when unset (C errors out
/// in that case, `whereis.c:97-99`).
fn lookup_path(envs: &[Vec<u8>]) -> Option<String> {
    for entry in envs {
        if let Some(rest) = entry.strip_prefix(b"PATH=") {
            return Some(String::from_utf8_lossy(rest).into_owned());
        }
    }
    None
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match WhichFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => usage(),
    };
    if operands.is_empty() {
        usage();
    }

    // The `which` personality always uses the environment PATH (`useenvpath`
    // defaults to the `which` flag, whereis.c:67).
    let search_path = match lookup_path(&support::envs()) {
        Some(p) => p,
        None => {
            support::warn(b"which: PATH environment variable is not set\n");
            support::terminate(1);
        }
    };

    let mut found = 0usize;
    for &name in &operands {
        if is_absolute(name) {
            // Absolute operand: check the file itself, no path search.
            if is_executable(name) {
                support::emit(format!("{name}\n").as_bytes());
                found += 1;
            }
        } else {
            for cand in path_candidates(&search_path, name) {
                if is_executable(&cand) {
                    support::emit(format!("{cand}\n").as_bytes());
                    found += 1;
                    if !flags.show_all {
                        break; // `which` stops at the first match per operand
                    }
                }
            }
        }
    }

    support::terminate(compute_status(found, operands.len()));
}

fn usage() -> ! {
    support::warn(b"Usage: which [-ap] program [...]\n");
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
