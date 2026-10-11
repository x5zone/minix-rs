//! Minix-RS `chmod` — the doing half over `minix_fileops::mode`.
//!
//! Ground truth: `minix3/bin/chmod/chmod.c`. The mode-text→bits translation
//! (BSD `setmode`/`getmode`) already lives in the library as
//! [`minix_fileops::mode::apply_mode`], octal and symbolic alike; this program
//! supplies the missing syscall leg: for each operand it reads the file's
//! current mode (`stat`), folds the spec onto it (`apply_mode`), and writes the
//! result (`chmod`), which is exactly C's `getmode(set, st_mode)` then
//! `change_mode(...)` (`chmod.c:215-216`).
//!
//! Wired: `chmod [-f] [-v] mode file ...` and the same with `-R`, which walks
//! each operand directory. A bare `-[rwxstugo]` (the entire argument) is the
//! *mode*, not a flag — reproduced from C's `--optind` correction
//! (`chmod.c:127-133`), so `chmod -r file` drops the read bit. `-f` continues
//! past errors (only sets the exit status, `chmod.c:215-219`), `-v` prints each
//! applied mode (this tree's extension; C's option string has no `v`).
//! Real-machine effect is the writable-root gate (`C-57`/E5), as for every
//! write-side command; the parser is host-tested in `mode.rs`.
//!
//! The recursive walk replaces fts(3) with this crate's own directory decoder
//! (`minix_fileops::dents`), the same substitution `rm -r` and `cp -R` made, and
//! keeps C's order of decisions (`chmod.c:186-219`): a directory is changed on
//! the way in and its post-order visit is skipped, so nothing is chmod'ed twice;
//! the links a physical walk meets are passed over entirely (`chmod.c:201-210`),
//! which is also why `-R` needs no `lchmod` — the only flag that would change a
//! link is `-h`, and C forbids combining it with `-R` (`chmod.c:145-150`); an
//! unreadable directory is reported and still chmod'ed (`FTS_DNR`,
//! `chmod.c:190-193`); an operand whose status cannot be had is reported and
//! skipped (`FTS_NS`, `chmod.c:196-200`).
//!
//! Declared residual: `-h`/`-H`/`-L`/`-P` (the link itself and the traversal
//! modes, needing `lchmod` and fts's follow decisions) and `--reference` are
//! refused up front. Two open findings in the option face are registered rather
//! than fixed here: `X` is missing from the bare-mode letter set although C
//! lists it (`chmod.c:128`), and this tree's mode grammar folds `X` onto plain
//! `x` instead of testing "directory or already executable".

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp;
use minix_fileops::dents;
use minix_fileops::mode::{ChmodFlags, ChmodOption, apply_mode, chmods_entry};
use minix_sys::{Fd, Stat, chmod, close, getdents, lstat, open, stat};

/// The refill buffer for the recursive walk (C's fts keeps its own; the size is
/// not observable, it only bounds how many records arrive per call).
const WALK_BUF: usize = 4096;

fn warn(bytes: &[u8]) {
    let mut line = String::from("chmod: ");
    line.push_str(core::str::from_utf8(bytes).unwrap_or("<invalid>"));
    line.push('\n');
    support::warn(line.as_bytes());
}

fn usage() -> ! {
    support::warn(b"usage: chmod [-R] [-f] [-v] mode file ...\n");
    support::terminate(1);
}

/// The status the walk needs for one path: `lstat` for the physical `-R` scan
/// (`chmod.c:144`), `stat` for the default scan that follows its operand
/// (`chmod.c:157-158`).
fn operand_stat(path: &str, physical: bool) -> Option<Stat> {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid bit
    // pattern that the call overwrites on success.
    let mut st: Stat = unsafe { core::mem::zeroed() };
    let got = if physical {
        lstat(path, &mut st)
    } else {
        stat(path, &mut st)
    };
    got.ok().map(|()| st)
}

/// `chmod.c:215-216`: fold the spec onto the bits this entry already has and
/// write the result. Either leg failing is the same reported failure.
fn apply_one(path: &str, st: &Stat, spec: &str) -> Result<(), ()> {
    let base = (st.st_mode & 0o7777) as u16;
    let mode = apply_mode(base, spec).map_err(|_| ())?;
    chmod(path, mode as u32).map_err(|_| ())
}

/// One visit (`chmod.c:186-219`): decide from the type bits whether this entry
/// is touched at all, apply the mode, then — with `-R` — walk the directory's
/// children. A directory is changed on the way in only, matching C's `FTS_DP`
/// `continue`; the descent runs whether or not the change succeeded, as it does
/// in C, where the queue is already built.
fn chmod_one(path: &str, st: &Stat, spec: &str, flags: &ChmodFlags) -> bool {
    if !chmods_entry(st.st_mode, flags.recursive) {
        return false; // passed over, not a failure
    }
    let mut failed = match apply_one(path, st, spec) {
        Ok(()) => {
            if flags.verbose {
                let mut msg = String::from(path);
                msg.push_str(" mode set to ");
                msg.push_str(spec);
                msg.push('\n');
                support::emit(msg.as_bytes());
            }
            false
        }
        Err(()) => {
            if !flags.force {
                let mut msg = String::from(path);
                msg.push_str(": cannot change mode");
                warn(msg.as_bytes());
            }
            true
        }
    };
    if flags.recursive && cp::is_dir(st.st_mode) {
        failed |= descend_chmod(path, spec, flags);
    }
    failed
}

/// Walk one directory's entries and change each of them. An unopenable
/// directory is C's `FTS_DNR` (`chmod.c:190-193`): reported, counted, and the
/// directory itself has already been chmod'ed by the caller.
fn descend_chmod(dir: &str, spec: &str, flags: &ChmodFlags) -> bool {
    let fd: Fd = match open(dir, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => {
            warn(dir.as_bytes());
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
                // C ends the run on an `fts_read` error (`chmod.c:221-224`);
                // this tree reports the directory and keeps going, so the
                // failure belongs to this operand rather than killing the rest.
                warn(dir.as_bytes());
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
            let child = dents::join_path(dir, name);
            match operand_stat(&child, true) {
                Some(st) => failed |= chmod_one(&child, &st, spec, flags),
                None => {
                    // `chmod.c:196-200` (`FTS_NS`): warn, count it, keep going.
                    warn(child.as_bytes());
                    failed = true;
                }
            }
        }
    }
    let _ = close(fd);
    failed
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match ChmodFlags::parse(&args[1.min(args.len())..]) {
        Ok(v) => v,
        Err(ChmodOption::NotWired(c)) => {
            warn(format!("-{c} not wired (lchmod, symlink traversal)").as_bytes());
            support::terminate(1);
        }
        Err(ChmodOption::Unknown(_)) => usage(),
    };

    // chmod.c:141-142: the mode plus at least one file, or usage.
    if operands.len() < 2 {
        usage();
    }
    let spec = operands[0];

    let mut any_failed = false;
    for path in &operands[1..] {
        match operand_stat(path, flags.recursive) {
            Some(st) => any_failed |= chmod_one(path, &st, spec, &flags),
            None => {
                warn(path.as_bytes());
                any_failed = true;
            }
        }
    }
    support::terminate(if any_failed { 1 } else { 0 });
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
