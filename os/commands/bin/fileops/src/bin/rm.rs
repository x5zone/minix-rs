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
//! command family shares (reached as `minix_sys::err_line`, whose shape comes from
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
//! Real-machine effect is the writable-root gate (`E5`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp;
use minix_fileops::dents;
use minix_fileops::du;
use minix_fileops::mode::strmode;
use minix_fileops::rm::{CheckFlavor, RmFlags, check_flavor, nonexistent, strip_dots};
use minix_sys::{
    Errno, Stat, access, close, fstat, getdents, isatty, lseek, lstat, open, read, rmdir, sync,
    unlink,
};

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

    // `-P` overwrites a regular file's blocks before the unlink
    // (`rm.c:341-344`); a failed overwrite reports and skips the removal
    // (`continue` in C). Directories are untouched by the pass.
    if flags.overwrite && !s_isdir(sb.st_mode) && rm_overwrite(prog, path, &sb) {
        return true;
    }

    // Directory (with -d) via rmdir, otherwise unlink.
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
            let mut sb = zeroed();
            // Each operand roots its own traversal, so `-x` compares against
            // this operand's device (fts roots one walk per operand). A
            // missing operand is reported by the walk itself.
            let root_dev = lstat(path, &mut sb).map(|_| sb.st_dev).unwrap_or(0);
            rm_tree_one(prog, path, &flags, root_dev)
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
/// C's `rm_overwrite` (`rm.c:361-451`): the DSS "clear then sanitize" pattern
/// over every addressable byte of a regular file — one pass of a random
/// character, `0xff`, `0x00` (verified by reading back), then random
/// 32-bit words. The victim must stay the same regular file the walk
/// stat'ed (device and inode re-checked through the opened descriptor);
/// any failure reports `warn("%s", file)` and returns true, which the
/// caller treats as "skip the removal". The random words come from a
/// clock-seeded xorshift — C's `arc4random` source is itself unspecified
/// output, and the bytes are never observed again.
fn rm_overwrite(prog: &str, file: &str, sbp: &Stat) -> bool {
    const PASS_BUF: usize = 8 * 1024;
    if !cp::is_reg(sbp.st_mode) {
        return false;
    }
    let flags = cp::O_RDWR | cp::O_SYNC | cp::O_RSYNC | cp::O_NOFOLLOW;
    let fd = match open(file, flags, 0) {
        Ok(fd) => fd,
        Err(e) => {
            warn_err(prog, file, e);
            return true;
        }
    };
    let mut sb2 = zeroed();
    if fstat(fd, &mut sb2).is_err() {
        warn_err(prog, file, Errno::EIO);
        let _ = close(fd);
        return true;
    }
    if sb2.st_dev != sbp.st_dev || sb2.st_ino != sbp.st_ino || !cp::is_reg(sb2.st_mode) {
        // C sets errno = EPERM for a swapped victim (`rm.c:118-122`).
        warn_err(prog, file, Errno::EPERM);
        let _ = close(fd);
        return true;
    }

    let (secs, nsec) = minix_sys::gettimeofday().unwrap_or((0, 0));
    let mut state = ((secs as u64) << 20 ^ nsec as u64) | 1;
    let mut next_random = move || -> u64 {
        // xorshift64*: three of the four random passes only need bytes the
        // caller discards, so a self-contained generator keeps the seam free.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    let randchar = (next_random() & 0xff) as u8;
    let mut buf = alloc::vec![0u8; PASS_BUF];
    enum Fill {
        Byte(u8),
        Random,
    }
    // WRITE_PASS (`rm.c:124-141`): fsync, rewind, then the chosen bytes over
    // the whole recorded size; `sync()` pokes the hidden caches.
    macro_rules! write_pass {
        ($fill:expr) => {{
            if minix_sys::fsync(fd).is_err() || lseek(fd, 0, 0 /* SEEK_SET */).is_err() {
                warn_err(prog, file, Errno::EIO);
                let _ = close(fd);
                return true;
            }
            match $fill {
                Fill::Byte(byte) => buf.fill(byte),
                Fill::Random => {
                    for chunk in buf.chunks_mut(4) {
                        let word = (next_random() & 0xffff_ffff) as u32;
                        for (i, byte) in chunk.iter_mut().enumerate() {
                            *byte = (word >> (i * 8)) as u8;
                        }
                    }
                }
            }
            let mut len = sbp.st_size as i64;
            while len > 0 {
                let wlen = core::cmp::min(len as usize, PASS_BUF);
                if minix_sys::write(fd, &buf[..wlen]).map(|n| n) != Ok(wlen) {
                    warn_err(prog, file, Errno::EIO);
                    let _ = close(fd);
                    return true;
                }
                len -= wlen as i64;
            }
            sync();
        }};
    }
    // 1. "a character" (a random one, drawn once, `rm.c:169-172`).
    write_pass!(Fill::Byte(randchar));
    // 2. "its complement" pass: 0xff.
    write_pass!(Fill::Byte(0xff));
    // 3. the zero pass, and 4. ...
    write_pass!(Fill::Byte(0x00));

    // READ_PASS (verify the zero pass really landed, `rm.c:144-163`).
    if minix_sys::fsync(fd).is_err() || lseek(fd, 0, 0 /* SEEK_SET */).is_err() {
        warn_err(prog, file, Errno::EIO);
        let _ = close(fd);
        return true;
    }
    buf.fill(0x00);
    let mut len = sbp.st_size;
    while len > 0 {
        let rlen = core::cmp::min(len as usize, PASS_BUF);
        match read(fd, &mut buf[..rlen]) {
            Ok(n) if n == rlen => {}
            _ => {
                warn_err(prog, file, Errno::EIO);
                let _ = close(fd);
                return true;
            }
        }
        if buf[..rlen].iter().any(|&b| b != 0x00) {
            warn_err(prog, file, Errno::EIO);
            let _ = close(fd);
            return true;
        }
        len -= rlen as i64;
    }
    sync();

    // Pass 4: random 32-bit words, deliberately unverified (`rm.c:168-172`).
    write_pass!(Fill::Random);

    if close(fd) == Err(Errno::EIO) {
        warn_err(prog, file, Errno::EIO);
        return true;
    }
    false
}

fn rm_tree_one(prog: &str, path: &str, flags: &RmFlags, root_dev: u64) -> bool {
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

    // `-x` (`rm.c:193-195`, fts's `FTS_XDEV`): a directory on another device
    // is never visited at all — fts prunes it silently, and the parent's
    // rmdir then answers ENOTEMPTY, which the caller already reports.
    if du::crosses_device(flags.cross_fs, sb.st_dev, root_dev) {
        return false;
    }

    if !s_isdir(sb.st_mode) {
        // C's default arm (`rm.c:238-242`, unlink at `rm.c:266-274`): ask, then unlink.
        if !flags.force && !check(path, &sb, flags) {
            return false;
        }
        // `-P` rides the recursive arm too (`rm.c:267-270`); a failed
        // overwrite skips this file's removal and counts as a failure.
        if flags.overwrite && rm_overwrite(prog, path, &sb) {
            return true;
        }
        return remove_entry(prog, path, flags);
    }

    // FTS_D pre-order (`rm.c:225-232`): the question covers the whole subtree; a
    // declined answer marks the directory skipped and its post-order visit does
    // nothing at all.
    if !flags.force && !check(path, &sb, flags) {
        return false;
    }

    let failed = descend_children(prog, path, flags, root_dev);

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
fn descend_children(prog: &str, dir: &str, flags: &RmFlags, root_dev: u64) -> bool {
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
            failed |= rm_tree_one(prog, &dents::join_path(dir, name), flags, root_dev);
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
