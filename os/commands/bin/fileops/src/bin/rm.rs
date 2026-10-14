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
//! facilities not wrapped in this tree. The `override` question is wired: it
//! needs `strmode` (now `minix_fileops::mode::strmode`) and prints the numeric
//! owner and group, the decimal fallback C's `user_from_uid`/`group_from_gid`
//! take when the password database has no entry (`pwcache.c:296-306`) — this
//! tree has no password lookup at all, so that is the only form. C's own `-P`
//! branch inside `check` (`rm.c:563-568`, "could not be overwritten") stays
//! unreachable because `-P` is refused before any operand is touched.
//! C's `warn`/`warnx` `strerror` suffixes are not reproduced here: the errno→text
//! table exists (`minix_types::Errno::message`, landed with the games family), while
//! this shell is not wired to it yet — a registered per-command follow-up.
//! Real-machine effect is the writable-root gate (`C-57`/E5).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp;
use minix_fileops::dents;
use minix_fileops::mode::strmode;
use minix_fileops::rm::{CheckFlavor, RmFlags, check_flavor, nonexistent, strip_dots};
use minix_sys::{Errno, Stat, access, close, getdents, isatty, lstat, open, rmdir, unlink};

/// The refill buffer for the recursive walk (C's fts keeps its own; the size is
/// not observable, it only bounds how many records arrive per call).
const WALK_BUF: usize = 4096;

// `minix3/sys/sys/unistd.h:170` (`W_OK 0x02`) and `stat.h:144` (`_S_IFLNK`).
const W_OK: i32 = 0x02;
const S_IFLNK: u32 = 0o120000;
// `minix3/sys/sys/errno.h:68` `ETXTBSY 26` ("Text file busy"): C's `check`
// treats that one denial as "not a permission problem" and does not ask.
// `Errno` carries no associated constant for it, so the value is local.
const ETXTBSY: i32 = 26;

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

/// C `check` (`rm.c:541-578`): decide via `check_flavor` whether anything is
/// asked, print that question the way C does, and take the answer from the
/// first character of the line (draining the rest, `rm.c:573-576`). Returns
/// true when the operand may be removed.
fn check(path: &str, sb: &Stat, flags: &RmFlags) -> bool {
    let denial = access(path, W_OK).err();
    match check_flavor(
        flags.interactive,
        isatty(support::STDIN),
        sb.st_mode & S_IFMT == S_IFLNK,
        denial.is_some(),
        denial == Some(Errno::from_i32(ETXTBSY)),
    ) {
        CheckFlavor::None => return true,
        // Both prompts are bare `fprintf(stderr, ...)`: no program name, and
        // the path in single quotes (rm.c:552, rm.c:569-572).
        CheckFlavor::Remove => {
            support::warn(format!("remove '{path}'? ").as_bytes());
        }
        CheckFlavor::Override => {
            let rendered = strmode(sb.st_mode);
            let perms = core::str::from_utf8(&rendered[1..10]).unwrap_or_default();
            let sep = if rendered[9] == b' ' { "" } else { " " };
            support::warn(
                format!(
                    "override {perms}{sep}{}:{} for '{path}'? ",
                    sb.st_uid, sb.st_gid
                )
                .as_bytes(),
            );
        }
    }
    support::answer_yes()
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
    if !flags.force && !check(path, &sb, flags) {
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
        support::warn(b"rm: -P/-W/-x not wired (overwrite, whiteout, FTS_XDEV)\n");
        support::terminate(1);
    }

    // POSIX "." / ".." rejection (rm.c checkdot); complain once if any dropped.
    let (kept, mut eval) = strip_dots(&operands);
    if eval {
        support::warn(b"rm: \".\" and \"..\" may not be removed\n");
    }

    for path in &kept {
        // rm.c:157-160: `-r` selects the hierarchy walk, everything else the
        // single-entry removal.
        eval |= if flags.recursive {
            rm_tree_one(path, &flags)
        } else {
            rm_one(path, &flags)
        };
    }
    support::terminate(if eval { 1 } else { 0 });
}

/// C `rm_tree` (`rm.c:167-281`) for one operand. The fts(3) walk is replaced by
/// this crate's own directory decoder (`minix_fileops::dents`), keeping C's
/// order of decisions: `lstat` first, a pre-order question for a directory that
/// can skip the whole subtree, children removed before the directory itself
/// (post-order `rmdir`), symlinks unlinked rather than followed, and `-f`
/// hiding only the nonexistent family.
///
/// Returns true when this operand contributed to the exit status.
fn rm_tree_one(path: &str, flags: &RmFlags) -> bool {
    let mut sb = zeroed();
    if let Err(e) = lstat(path, &mut sb) {
        // The root of the walk cannot be stat'ed: C's FTS_NS arm (rm.c:200-213)
        // — `-f` swallows the nonexistent family, otherwise it is reported and
        // nothing is removed.
        if flags.force && nonexistent(e) {
            return false;
        }
        warn_err(path);
        return true;
    }

    if !s_isdir(sb.st_mode) {
        // C's default arm (rm.c:226-229): ask, then unlink.
        if !flags.force && !check(path, &sb, flags) {
            return false;
        }
        return remove_entry(path, flags);
    }

    // FTS_D pre-order (rm.c:217-222): the question covers the whole subtree; a
    // declined answer marks the directory skipped and its post-order visit does
    // nothing at all.
    if !flags.force && !check(path, &sb, flags) {
        return false;
    }

    let failed = descend_children(path, flags);

    // FTS_DP post-order (rm.c:236-244): the directory itself, now empty or
    // never readable. C still attempts the rmdir after a failed descent, and
    // under `-f` a vanished directory is not an error.
    match rmdir(path) {
        Ok(()) => {
            echo_removed(path, flags);
            failed
        }
        Err(e) => {
            if flags.force && e == Errno::ENOENT {
                return failed;
            }
            warn_err(path);
            true
        }
    }
}

/// Walk one directory's entries and remove each of them. Returns true when any
/// child failed; the unreadable-directory case is C's `FTS_DNR` (rm.c:190-197):
/// report it (unless `-f` and the directory simply vanished) and let the caller
/// still try the `rmdir`.
fn descend_children(dir: &str, flags: &RmFlags) -> bool {
    let fd = match open(dir, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(e) => {
            if !(flags.force && nonexistent(e)) {
                warn_err(dir);
            }
            return true;
        }
    };

    let mut failed = false;
    let mut buf = [0u8; WALK_BUF];
    loop {
        let transferred = match getdents(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                // C ends the whole run on an fts_read error (`rm.c:278-280`,
                // `err(1, "fts_read")`); this tree reports the directory and
                // still lets the caller try its rmdir, so a read error is a
                // failure for this operand rather than a hard exit.
                warn_err(dir);
                failed = true;
                break;
            }
        };
        let mut cursor = 0;
        while cursor < transferred {
            let entry = match dents::next_record(&buf, cursor) {
                Ok(Some((entry, next))) => {
                    cursor = next;
                    entry
                }
                // Buffer exhausted, or a record whose lengths contradict it:
                // stop this refill exactly where C's readdir returns NULL.
                Ok(None) | Err(_) => break,
            };
            if entry.ino == 0 {
                // A deleted slot: libc's readdir skips it (readdir.c:83-84).
                continue;
            }
            let name = match core::str::from_utf8(entry.name) {
                Ok(name) => name,
                Err(_) => continue,
            };
            if !dents::is_descendable(name) {
                continue;
            }
            failed |= rm_tree_one(&dents::join_path(dir, name), flags);
        }
    }
    // The descriptor cannot be meaningfully reported on here: the removal
    // outcome is already decided by the entries walked.
    let _ = close(fd);
    failed
}

/// Unlink one non-directory entry, with C's `-f` nonexistent hiding
/// (`rm.c:264-269`) and the `-v` echo of a removed name (`rm.c:271-274`).
fn remove_entry(path: &str, flags: &RmFlags) -> bool {
    match unlink(path) {
        Ok(()) => {
            echo_removed(path, flags);
            false
        }
        Err(e) => {
            if flags.force && nonexistent(e) {
                return false;
            }
            warn_err(path);
            true
        }
    }
}

/// `rm.c:271-274`: one name per successfully removed entry when `-v` is set.
fn echo_removed(path: &str, flags: &RmFlags) {
    if flags.verbose {
        support::emit(format!("{path}\n").as_bytes());
    }
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
