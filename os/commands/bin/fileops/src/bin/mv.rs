//! Minix-RS `mv` — the doing half over `minix_fileops::mv`.
//!
//! Ground truth: `minix3/bin/mv/mv.c`. The option set and operand shape live in
//! `minix_fileops::mv`; this program supplies the filesystem leg: `do_move` is
//! C's `mv.c` rename attempt (`mv.c:218`), reached from the two operand shapes
//! (`mv.c:113-159`) with the `-i` overwrite prompt and `-v` echo.
//!
//! Wired: the same-filesystem move, which is a single `rename(2)` (POSIX's
//! "fast" path, `mv.c:218`). `mv [-f|-i] [-v] name target` renames; if `target`
//! is an existing directory the source lands under its basename there;
//! `mv name ... directory` moves each source into the directory (which must be
//! one, else usage as `mv.c:118`). `-f` suppresses the overwrite prompt, `-i`
//! asks per existing target, `-v` prints `source -> target`.
//!
//! Declared residual: when `rename` fails with `EXDEV` the C program falls back
//! to a full copy (`copy`/`fastcopy`, `mv.c:256-409` — data copy plus mode/time
//! preservation, `open`/`fstat`/`utimens`/`unlink`) and only then unlinks the
//! source. That copy engine is a separate multi-round facility not wired in
//! this tree, so a cross-device move is reported and counted as a failure
//! rather than silently doing nothing — a move that did not happen must not
//! masquerade as success. Real-machine effect is the writable-root gate
//! (`C-57`/E5), as for every write-side command; `strerror` suffixes are not
//! reproduced.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::mv::{MvFlags, MvPlan, dest_name, plan_operands};
use minix_sys::{Errno, Stat, read, rename, stat};

// POSIX file-type bits and EXDEV, matching C `sys/sys/stat.h:138,141`
// (`_S_IFMT 0170000`, `_S_IFDIR 0040000`) and `sys/errno.h` (`EXDEV 18`).
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const EXDEV: i32 = 18;

fn s_isdir(m: u32) -> bool {
    m & S_IFMT == S_IFDIR
}

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid
    // bit pattern that `stat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

fn is_dir(path: &str) -> bool {
    let mut sb = zeroed();
    stat(path, &mut sb).is_ok() && s_isdir(sb.st_mode)
}

/// C `do_move`/`mv`'s rename attempt plus the `-i` overwrite prompt. Returns
/// true on failure (the operand contributed to the exit status).
fn do_move(source: &str, target: &str, flags: &MvFlags) -> bool {
    // -i (and not -f): ask before clobbering an existing target.
    if flags.interactive && !flags.force && is_dir_or_file(target) {
        support::warn(format!("mv: overwrite '{target}'? ").as_bytes());
        let mut buf = [0u8; 64];
        let n = read(0, &mut buf).unwrap_or(0);
        let yes = n > 0 && (buf[0] == b'y' || buf[0] == b'Y');
        if !yes {
            return false; // declined: not an error, just skipped
        }
    }

    match rename(source, target) {
        Ok(()) => {
            if flags.verbose {
                support::emit(format!("{source} -> {target}\n").as_bytes());
            }
            false
        }
        Err(e) if e == Errno::from_i32(EXDEV) => {
            support::warn(b"mv: cross-device move not wired (copy engine)\n");
            true
        }
        Err(_) => {
            support::warn(format!("mv: rename {source} to {target}\n").as_bytes());
            true
        }
    }
}

/// Whether `target` exists at all (a symlink to a missing file counts).
fn is_dir_or_file(path: &str) -> bool {
    let mut sb = zeroed();
    stat(path, &mut sb).is_ok()
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match MvFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => usage(),
    };

    let eval = match plan_operands(&operands) {
        MvPlan::Usage => usage(),
        MvPlan::Pair { source, target } => {
            // A directory target means "move into it" under the basename.
            if is_dir(target) {
                let dest = format!("{target}/{}", dest_name(source));
                do_move(source, &dest, &flags)
            } else {
                do_move(source, target, &flags)
            }
        }
        MvPlan::ToDirectory { sources, dir } => {
            if !is_dir(dir) {
                usage(); // C: >2 operands with a non-directory target -> usage
            }
            let mut eval = false;
            for src in &sources {
                let dest = format!("{dir}/{}", dest_name(src));
                eval |= do_move(src, &dest, &flags);
            }
            eval
        }
    };

    support::terminate(if eval { 1 } else { 0 });
}

fn usage() -> ! {
    support::warn(
        b"usage: mv [-f | -i] [-v] source target\n       mv [-f | -i] [-v] source ... directory\n",
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
