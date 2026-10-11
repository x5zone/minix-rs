//! Minix-RS `truncate` — the doing half over `minix_fileops::truncate`.
//!
//! Ground truth: `minix3/minix/commands/truncate/truncate.c`. The decisions —
//! which size, from where, with or without creating — live in
//! `minix_fileops::truncate`; this program supplies the `stat`, `open`, `fstat`,
//! `ftruncate` and `close` legs in C's order: the reference file is looked at
//! once before the loop (`truncate.c:91-100`), then each operand is opened,
//! measured when the size is relative, clamped and truncated (`:108-141`).
//!
//! Wired in full: `truncate [-c] -s [+|-]size[K|k|M|m|G|g] file ...` and
//! `truncate [-c] -r rfile file ...`. This command has no declared residual —
//! every option C takes is taken here, the size grammar included (the multiplier
//! arms fall through, so `1G` is a gibibyte: `truncate.c:176-190`).
//!
//! Three deliberate differences, all in the reporting and failure paths:
//! - C prints an unparseable `-s` value and a failed reference `stat` with a bare
//!   `fprintf` and no program-name prefix (`truncate.c:67-71`, `:92-95`); that
//!   shape is reproduced. The per-file failures keep this crate's conventional
//!   `truncate: <path>` line, because there is no `strerror` table here to print
//!   the suffix C's `perror` would add.
//! - C counts an open failure only when `errno` is not `ENOENT`
//!   (`truncate.c:109-114`), so `truncate -c absent` exits 0 in silence. The same
//!   triage is kept, which is why the failure is tracked per operand.
//! - C leaks the descriptor when `fstat` or `ftruncate` fails, since `close(fd)`
//!   sits after the success path (`truncate.c:117-141`). Closing on every exit is
//!   done instead: the leak is invisible to the command's observable behavior and
//!   is not worth preserving.
//!
//! Real-machine effect is the writable-root gate (`C-57`/E5), as for every
//! write-side command: the root image is the read-only boot-shim `imgrd`, so the
//! `ftruncate` leg cannot be exercised on the machine yet.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::truncate::{
    CREATE_MODE, SizeSource, TruncateArgError, TruncatePlan, clamp, open_flags, parse_args,
    relative_target,
};
use minix_sys::{Errno, Fd, Stat, close, fstat, ftruncate, open, stat};

/// Where the length comes from once the reference file has been measured.
enum Resolved {
    /// Absolute, or the size of the `-r` file: the same number for every operand.
    Fixed(i64),
    /// `-s +N` / `-s -N`: folded onto each operand's own length.
    Relative(i64),
}

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid bit
    // pattern that `stat`/`fstat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

fn report(path: &str) {
    support::warn(format!("truncate: {path}\n").as_bytes());
}

/// `truncate.c:91-100`: settle the size before touching any operand.
fn resolve(plan: &TruncatePlan) -> Resolved {
    match &plan.source {
        SizeSource::Absolute(sz) => Resolved::Fixed(clamp(*sz)),
        SizeSource::Relative(delta) => Resolved::Relative(*delta),
        SizeSource::Reference(rname) => {
            let mut sb = zeroed();
            match stat(rname, &mut sb) {
                Ok(()) => Resolved::Fixed(clamp(sb.st_size)),
                Err(_) => {
                    // `truncate.c:92-95`: the name alone, then exit — no operand
                    // is opened at all.
                    support::warn(format!("{rname}\n").as_bytes());
                    support::terminate(1);
                }
            }
        }
    }
}

/// `truncate.c:108-141` for one operand. Returns true when this operand
/// contributed to the exit status.
fn truncate_one(fname: &str, plan: &TruncatePlan, resolved: &Resolved) -> bool {
    let fd: Fd = match open(fname, open_flags(plan.no_create), CREATE_MODE) {
        Ok(fd) => fd,
        Err(e) => {
            // `truncate.c:109-114`: a vanished operand is passed over without a
            // word; anything else is reported and counted. Neither path truncates.
            if e != Errno::ENOENT {
                report(fname);
                return true;
            }
            return false;
        }
    };

    let size = match resolved {
        Resolved::Fixed(sz) => *sz,
        Resolved::Relative(delta) => {
            let mut sb = zeroed();
            if fstat(fd, &mut sb).is_err() {
                report(fname);
                let _ = close(fd);
                return true;
            }
            relative_target(sb.st_size, *delta)
        }
    };

    if ftruncate(fd, size).is_err() {
        report(fname);
        let _ = close(fd);
        return true;
    }
    let _ = close(fd);
    false
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.is_empty() {
        usage();
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let plan = match parse_args(&args[1..]) {
        Ok(p) => p,
        Err(TruncateArgError::Usage) => usage(),
        Err(TruncateArgError::InvalidSize(text)) => {
            // `truncate.c:67-71`: the offending text is quoted, there is no
            // program-name prefix, and the exit happens before any file is named.
            support::warn(format!("invalid size argument `{text}'\n").as_bytes());
            support::terminate(1);
        }
    };

    let resolved = resolve(&plan);
    // `truncate.c:108-143`: the operands are handled in the order given and the
    // status is "did anything fail" — `return error ? EXIT_FAILURE :
    // EXIT_SUCCESS` at `:143`.
    let mut any_failed = false;
    for fname in &plan.files {
        any_failed |= truncate_one(fname, &plan, &resolved);
    }
    support::terminate(if any_failed { 1 } else { 0 });
}

fn usage() -> ! {
    support::warn(
        b"usage: truncate [-c] -s [+|-]size[K|k|M|m|G|g] file ...\n\
                truncate [-c] -r rfile file ...\n",
    );
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
