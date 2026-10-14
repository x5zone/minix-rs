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
//! C's `warn`/`warnx`/`err` legs are wired to the one renderer the whole
//! command family shares (`minix_types::err_line`, whose shape comes from
//! `minix3/lib/libc/gen/verrc.c`): the program name comes from `argv[0]`
//! through `getprogname()`'s rule, and the trailing sentence is that errno's
//! `strerror` text. Two shapes are at stake and they are not the same line:
//! `warn("%s", f)` (`rm.c:311`, `:348`) prints `rm: <operand>: <sentence>`,
//! while `warnx("%s: is a directory", f)` (`rm.c:323`) prints `rm: <operand>:
//! is a directory` — a fixed phrase, no sentence. C's hierarchy walk ends the
//! whole run on a failed directory read (`err(1, "fts_read")`, `rm.c:284-285`),
//! and this program now does the same rather than reporting the directory and
//! carrying on.
//!
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

/// C's `warn("%s", f)` (`rm.c:311`, `rm.c:348`): the program name, the operand,
/// then that errno's sentence — and the run continues, since `vwarn` does not exit.
fn warn_err(prog: &str, path: &str, code: Errno) {
    support::warn_line(prog, path, code);
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
        // the path in single quotes (`rm.c:549`, `rm.c:567-570`).
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
fn rm_one(prog: &str, path: &str, flags: &RmFlags) -> bool {
    // "Assume if can't stat the file, can't unlink it" (rm.c:302). -f hides a
    // nonexistent operand; anything else is reported.
    let mut sb = zeroed();
    if let Err(e) = lstat(path, &mut sb) {
        if !(flags.force && nonexistent(e)) {
            warn_err(prog, path, e);
            return true;
        }
        return false;
    }

    // POSIX: removing a directory without -d is an error (rm.c:322-326). C's
    // `warnx("%s: is a directory", f)` carries the program name but no errno
    // sentence, so it is not a `warn_err` call.
    if s_isdir(sb.st_mode) && !flags.dflag {
        support::warn(format!("{prog}: {path}: is a directory\n").as_bytes());
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
            warn_err(prog, path, e);
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
    // C calls `setprogname(argv[0])` first thing (`rm.c:104`); every `warn`/`warnx`/
    // `err` line and the usage line start from that one string.
    let prog = support::prog_name(&argv);

    let (flags, operands) = match RmFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => usage(prog),
    };

    // C rm.c:144-148: with no operand, -f exits 0, otherwise usage.
    if operands.is_empty() {
        if flags.force {
            support::terminate(0);
        }
        usage(prog);
    }

    // Refuse options that hinge on unwrapped facilities before doing work.
    if flags.needs_unwired() {
        support::warn(b"rm: -P/-W/-x not wired (overwrite, whiteout, FTS_XDEV)\n");
        support::terminate(1);
    }

    // POSIX "." / ".." rejection (rm.c checkdot); complain once if any dropped.
    // C's line is `warnx("\".\" and \"..\" may not be removed")` (`rm.c:610`) — program
    // name plus that fixed phrase, no errno sentence behind it.
    let (kept, mut eval) = strip_dots(&operands);
    if eval {
        support::warn(format!("{prog}: \".\" and \"..\" may not be removed\n").as_bytes());
    }

    for path in &kept {
        // rm.c:157-160: `-r` selects the hierarchy walk, everything else the
        // single-entry removal.
        eval |= if flags.recursive {
            rm_tree_one(prog, path, &flags)
        } else {
            rm_one(prog, path, &flags)
        };
    }
    support::terminate(if eval { 1 } else { 0 });
}

/// C `rm_tree` (`rm.c:167-287`) for one operand. The fts(3) walk is replaced by
/// this crate's own directory decoder (`minix_fileops::dents`), keeping C's
/// order of decisions: `lstat` first, a pre-order question for a directory that
/// can skip the whole subtree, children removed before the directory itself
/// (post-order `rmdir`), symlinks unlinked rather than followed, and `-f`
/// hiding only the nonexistent family.
///
/// Returns true when this operand contributed to the exit status.
fn rm_tree_one(prog: &str, path: &str, flags: &RmFlags) -> bool {
    let mut sb = zeroed();
    if let Err(e) = lstat(path, &mut sb) {
        // The root of the walk cannot be stat'ed: C's FTS_NS arm (`rm.c:211-224`)
        // — `-f` swallows the nonexistent family, otherwise it is reported and
        // nothing is removed. C's line is `warnx("%s: %s", path, strerror(errno))`,
        // which renders to the same shape `warn_err` already produces.
        if flags.force && nonexistent(e) {
            return false;
        }
        warn_err(prog, path, e);
        return true;
    }

    if !s_isdir(sb.st_mode) {
        // C's default arm (`rm.c:238-242`, unlink at `rm.c:266-274`): ask, then unlink.
        if !flags.force && !check(path, &sb, flags) {
            return false;
        }
        return remove_entry(prog, path, flags);
    }

    // FTS_D pre-order (`rm.c:225-232`): the question covers the whole subtree; a
    // declined answer marks the directory skipped and its post-order visit does
    // nothing at all.
    if !flags.force && !check(path, &sb, flags) {
        return false;
    }

    let failed = descend_children(prog, path, flags);

    // FTS_DP post-order (`rm.c:233-237`, rmdir at `rm.c:250-256`): the directory
    // itself, now empty or never readable. C still attempts the rmdir after a
    // failed descent, and under `-f` a vanished directory is not an error.
    match rmdir(path) {
        Ok(()) => {
            echo_removed(path, flags);
            failed
        }
        Err(e) => {
            if flags.force && e == Errno::ENOENT {
                return failed;
            }
            warn_err(prog, path, e);
            true
        }
    }
}

/// Walk one directory's entries and remove each of them. Returns true when any
/// child failed; the unreadable-directory case is C's `FTS_DNR` (`rm.c:200-206`):
/// report it (unless `-f` and the directory simply vanished) and let the caller
/// still try the `rmdir`.
fn descend_children(prog: &str, dir: &str, flags: &RmFlags) -> bool {
    let fd = match open(dir, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(e) => {
            // C's FTS_DNR arm (`rm.c:200-206`): report `rm: <dir>: <sentence>` and
            // let the caller still try the rmdir, unless `-f` says the directory
            // simply vanished.
            if !(flags.force && nonexistent(e)) {
                warn_err(prog, dir, e);
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
            Err(e) => {
                // `rm.c:284-285`: after the walk loop C checks `errno` and calls
                // `err(1, "fts_read")` — program name, the C name of the failing
                // step, that errno's sentence, then exit 1. The walk does not
                // carry on, so the rmdir of this directory never happens.
                support::warn_line(prog, "fts_read", e);
                support::terminate(1);
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
            failed |= rm_tree_one(prog, &dents::join_path(dir, name), flags);
        }
    }
    // The descriptor cannot be meaningfully reported on here: the removal
    // outcome is already decided by the entries walked.
    let _ = close(fd);
    failed
}

/// Unlink one non-directory entry, with C's `-f` nonexistent hiding
/// (`rm.c:271-274`) and the `-v` echo of a removed name (`rm.c:279-282`).
fn remove_entry(prog: &str, path: &str, flags: &RmFlags) -> bool {
    match unlink(path) {
        Ok(()) => {
            echo_removed(path, flags);
            false
        }
        Err(e) => {
            if flags.force && nonexistent(e) {
                return false;
            }
            warn_err(prog, path, e);
            true
        }
    }
}

/// `rm.c:279-282`: one name per successfully removed entry when `-v` is set.
fn echo_removed(path: &str, flags: &RmFlags) {
    if flags.verbose {
        support::emit(format!("{path}\n").as_bytes());
    }
}

fn usage(prog: &str) -> ! {
    // `rm.c:620-628`: `fprintf(stderr, "usage: %s [-f|-i] [-dPRrvWx] file ...\n",
    // getprogname())` — the program name rides inside the line, it is not a
    // `warn`-style prefix, and the option groups keep C's unspaced `|`.
    support::warn(format!("usage: {prog} [-f|-i] [-dPRrvWx] file ...\n").as_bytes());
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
