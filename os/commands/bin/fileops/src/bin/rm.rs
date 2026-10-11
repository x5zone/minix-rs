//! Minix-RS `rm` — the doing half over `minix_fileops::rm`.
//!
//! Ground truth: `minix3/bin/rm/rm.c`. The option set, the `-f` `NONEXISTENT`
//! triage, and the POSIX `.`/`..` rejection live in `minix_fileops::rm`; this
//! program supplies the filesystem leg — `main`'s argument handling plus
//! `rm_file`'s `lstat`/`unlink`/`rmdir` sequence and the `-i` prompt
//! (`rm.c:290-354`, `rm.c:541-578`).
//!
//! Wired: `rm [-d|-f|-i] [-v] file ...`. The non-recursive default is exactly
//! POSIX `rm` — a directory operand without `-d` is diagnosed "is a
//! directory", `-f` forces and hides nonexistent operands (and exits 0 when no
//! operand is given), `-i` prompts, `-v` echoes each removal.
//!
//! Declared residual (refused up front, like `chmod`'s `-R`): `-r`/`-R`
//! (recursive `fts` walk), `-P` (secure overwrite via `arc4random`/
//! `O_NOFOLLOW`), `-W` (whiteout `undelete`), and `-x` (`FTS_XDEV`) hinge on
//! facilities not wrapped in this tree. The tty override prompt also needs
//! `strmode` + `user_from_uid`/`group_from_gid`; under the scripted/non-tty
//! path this tree runs, `rm.c` proceeds without prompting (`!stdin_ok`).
//! C's `warn`/`warnx` `strerror` suffixes are not reproduced (no errno→text
//! table); real-machine effect is the writable-root gate (`C-57`/E5).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::rm::{RmFlags, nonexistent, strip_dots};
use minix_sys::{Stat, lstat, read, rmdir, unlink};

// POSIX file-type bits, matching C `sys/sys/stat.h`: `_S_IFMT 0170000`,
// `_S_IFDIR 0040000`.
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

fn s_isdir(m: u32) -> bool {
    m & S_IFMT == S_IFDIR
}

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid
    // bit pattern that `lstat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

fn warn_err(path: &str) {
    support::warn(format!("rm: {path}\n").as_bytes());
}

/// C `check` (`rm.c:541-578`), restricted to the wired `-i` path: with
/// `iflag` it asks `remove '<path>'? ` and takes the answer from the first
/// character of the line (C reads `getchar` until newline/EOF, returning
/// `first == 'y' || 'Y'`). Without `-i` on this tree's non-tty path, C returns
/// proceed (`!stdin_ok`, `rm.c:557`); the tty override prompt that would ask
/// before removing an unwritable regular file needs `strmode` +
/// `user_from_uid`/`group_from_gid` and is a declared residual.
fn check(path: &str, flags: &RmFlags) -> bool {
    if !flags.interactive {
        return true;
    }
    support::warn(format!("remove '{path}'? ").as_bytes());
    let mut buf = [0u8; 64];
    let n = read(0, &mut buf).unwrap_or(0);
    n > 0 && (buf[0] == b'y' || buf[0] == b'Y')
}

/// C `rm_file`'s per-operand body (`rm.c:300-353`). Returns true on failure
/// (the operand contributed to the exit status), matching `eval = 1`.
fn rm_one(path: &str, flags: &RmFlags) -> bool {
    // "Assume if can't stat the file, can't unlink it" (rm.c:302). -f hides a
    // nonexistent operand; anything else is reported.
    let mut sb = zeroed();
    if let Err(e) = lstat(path, &mut sb) {
        if !(flags.force && nonexistent(e)) {
            warn_err(path);
            return true;
        }
        return false;
    }

    // POSIX: removing a directory without -d is an error (rm.c:322-326).
    if s_isdir(sb.st_mode) && !flags.dflag {
        support::warn(format!("rm: {path}: is a directory\n").as_bytes());
        return true;
    }

    // Interactive / override check before touching the entry (rm.c:330).
    if !flags.force && !check(path, flags) {
        return false;
    }

    // Directory (with -d) via rmdir, otherwise unlink. -P overwrite is a
    // declared residual (rm.c:341-344).
    let removed = if s_isdir(sb.st_mode) {
        rmdir(path)
    } else {
        unlink(path)
    };
    if let Err(e) = removed {
        if !(flags.force && nonexistent(e)) {
            warn_err(path);
            return true;
        }
    } else if flags.verbose {
        support::emit(format!("{path}\n").as_bytes());
    }
    false
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match RmFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => usage(),
    };

    // C rm.c:144-148: with no operand, -f exits 0, otherwise usage.
    if operands.is_empty() {
        if flags.force {
            support::terminate(0);
        }
        usage();
    }

    // Refuse options that hinge on unwrapped facilities before doing work.
    if flags.needs_unwired() {
        support::warn(b"rm: -r/-R/-P/-W/-x not wired (fts, overwrite, whiteout)\n");
        support::terminate(1);
    }

    // POSIX "." / ".." rejection (rm.c checkdot); complain once if any dropped.
    let (kept, mut eval) = strip_dots(&operands);
    if eval {
        support::warn(b"rm: \".\" and \"..\" may not be removed\n");
    }

    for path in &kept {
        eval |= rm_one(path, &flags);
    }
    support::terminate(if eval { 1 } else { 0 });
}

fn usage() -> ! {
    support::warn(b"usage: rm [-f | -i] [-dPRrvWx] file ...\n");
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
