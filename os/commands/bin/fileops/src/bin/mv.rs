//! Minix-RS `mv` — the doing half over `minix_fileops::mv`.
//!
//! Ground truth: `minix3/bin/mv/mv.c`. The option set, the operand shape, the
//! cross-device decisions and the ask ladder live in `minix_fileops::mv`; this
//! program supplies the filesystem legs: `do_move` (`mv.c:153-253`) reached from
//! the two operand shapes (`mv.c:113-159`), `fastcopy` (`mv.c:256-354`) for the
//! `EXDEV` case, and the `-v` echo.
//!
//! Wired: the same-filesystem move (a single `rename(2)`, POSIX's "fast" path,
//! `mv.c:218`) and the cross-device move of a plain file. `mv [-f|-i] [-v] name
//! target` renames; if `target` is an existing directory the source lands under
//! its basename there; `mv name ... directory` moves each source into the
//! directory (which must be one, else usage as `mv.c:117-118`). The questions
//! follow `mv.c:172-203` exactly: nothing is asked when `-f` is set or the
//! target does not exist; `-i` on an existing target asks `overwrite <target>? `
//! (and `mv.c:92` sets `iflg` without consulting the terminal, so unlike `cp`
//! it asks on a pipe too); with no flags at all, a terminal standard input plus
//! a target this process cannot write asks `override <perms> <uid>/<gid> for
//! <target>? `. A "no" answer skips that operand without counting as a failure
//! (`mv.c:200-201`), and `-v` prints `source -> target`.
//!
//! Declared residual: a cross-device move of a **plain file** is wired — the
//! `EXDEV` leg runs `fastcopy` (`mv.c:256-354`: copy the bytes, stamp the
//! source's times/owner/mode, then remove the source). For anything else
//! (directory, symlink, device, FIFO) the C program does not copy by hand at
//! all: `copy` (`mv.c:357-399`) execs `/bin/cp -PRp` on the pair and then
//! `/bin/rm -rf` on the source, so that leg needs the exec delivery chain
//! (`NS5`/`C-21`) and is reported rather than faked. Two mechanical deviations
//! inside the wired legs, both invisible for a plain-file destination: C stamps
//! owner and mode through `fchown`/`fchmod` on the open descriptor
//! (`mv.c:328-334`) and this tree has no `f*` wrappers, so those two go by path
//! while the descriptor is still open; and C reads through a `malloc`ed buffer
//! of `st_blksize` bytes (`mv.c:277`) while this copy uses a fixed chunk (the
//! chunk size is not observable). The `override` question prints the numeric
//! user and group identifiers: C takes them from `user_from_uid` and
//! `group_from_gid`, whose documented behaviour when the password database has
//! no entry (`pwcache.c:296-306`) is exactly that decimal fallback, and this
//! tree has no password lookup at all. Real-machine effect of every write-side
//! command, this one included, is the writable-root gate (`C-57`/E5); `strerror`
//! suffixes are not reproduced.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp::{self, CopyError, CopyIo};
use minix_fileops::mode::strmode;
use minix_fileops::mv::{
    Ask, CrossDevice, MvFlags, MvPlan, RemovalCall, ask_about, cross_device_plan, dest_name,
    existing_removal, move_create_mode, plan_operands,
};
use minix_sys::{
    Errno, Fd, Stat, access, chmod, chown, close, futimens, isatty, lstat, open, read, rename,
    rmdir, stat, unlink, write,
};

// POSIX file-type bits and EXDEV, matching C `sys/sys/stat.h:138,141`
// (`_S_IFMT 0170000`, `_S_IFDIR 0040000`) and `sys/errno.h` (`EXDEV 18`).
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const EXDEV: i32 = 18;

// The `access(2)` modes C's `do_move` tests with, from
// `minix3/sys/sys/unistd.h:168-171` (`F_OK 0`, `X_OK 0x01`, `W_OK 0x02`,
// `R_OK 0x04`); this tree re-exports no names for them.
const F_OK: i32 = 0;
const W_OK: i32 = 0x02;

/// The copy chunk. C sizes its buffer from `st_blksize` (`mv.c:277`); the size
/// of a copy chunk is not part of what anyone can observe, so the same fixed
/// chunk `cp` uses here too.
const CHUNK: usize = 8192;

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

/// C `do_move` (`mv.c:153-253`): the ask ladder, the `rename` attempt, and on
/// `EXDEV` the cross-device leg. Returns true on failure (the operand
/// contributed to the exit status).
fn do_move(source: &str, target: &str, flags: &MvFlags) -> bool {
    // mv.c:172-203, through the pure `ask_about` decision. The three probes C
    // makes are kept separate because they answer different questions:
    // `access(to, F_OK)` gates the block, `access(to, W_OK)` drives the
    // override branch, and `stat(to)` supplies the mode and the owner names.
    let mut tsb = zeroed();
    let ask = ask_about(
        flags.force,
        flags.interactive,
        access(target, F_OK).is_ok(),
        isatty(support::STDIN),
        access(target, W_OK).is_ok(),
        stat(target, &mut tsb).is_ok(),
        access(source, F_OK).is_ok(),
    );
    match ask {
        Ask::None => {}
        Ask::SourceUnreachable => {
            // mv.c:177-180: warn("rename %s", from) and count it.
            support::warn(format!("mv: rename {source}\n").as_bytes());
            return true;
        }
        Ask::Overwrite => {
            // mv.c:181: a bare fprintf, so the question carries no program name
            // and no quotes around the target.
            support::warn(format!("overwrite {target}? ").as_bytes());
            if !answer_yes() {
                // mv.c:200-201: a "no" answer skips this operand, not an error.
                return false;
            }
        }
        Ask::Override => {
            // `ask_about` only answers Override when the target could be stat'd,
            // which is also when tsb holds the mode and owner mv.c:188 prints.
            override_prompt(target, &tsb);
            if !answer_yes() {
                return false;
            }
        }
    }

    match rename(source, target) {
        Ok(()) => {
            if flags.verbose {
                support::emit(format!("{source} -> {target}\n").as_bytes());
            }
            false
        }
        Err(e) if e == Errno::from_i32(EXDEV) => cross_device_move(source, target, flags),
        Err(_) => {
            support::warn(format!("mv: rename {source} to {target}\n").as_bytes());
            true
        }
    }
}

/// The `EXDEV` leg of C's `do_move` (`mv.c:229-253`): clear whatever the
/// destination still is, look at the source, then either copy it in place or
/// hand the job to the subprocess form the C program uses.
fn cross_device_move(source: &str, target: &str, flags: &MvFlags) -> bool {
    // mv.c:235-240: `lstat` the target; if it exists, remove it first, and a
    // failed removal is the whole error (the source is left alone).
    let mut tsb = zeroed();
    if lstat(target, &mut tsb).is_ok() {
        let failed = match existing_removal(tsb.st_mode) {
            RemovalCall::Rmdir => rmdir(target).is_err(),
            RemovalCall::Unlink => unlink(target).is_err(),
        };
        if failed {
            support::warn(format!("mv: can't remove {target}\n").as_bytes());
            return true;
        }
    }

    // mv.c:246-249: the source type decides which copy routine runs.
    let mut sb = zeroed();
    if lstat(source, &mut sb).is_err() {
        support::warn(format!("mv: {source}\n").as_bytes());
        return true;
    }
    match cross_device_plan(sb.st_mode) {
        CrossDevice::InlineCopy => fastcopy(source, target, &sb, flags),
        CrossDevice::NeedsSubprocess => {
            // mv.c:362-398: the C program execs /bin/cp -PRp and then
            // /bin/rm -rf. No exec delivery chain here yet (NS5/C-21), so the
            // move is reported as not done.
            support::warn(
                b"mv: cross-device move of a non-regular file needs /bin/cp plus /bin/rm (exec delivery chain, NS5)\n",
            );
            true
        }
    }
}

/// The two open descriptors behind the copy loop. Twin of the adapter
/// `src/bin/cp.rs` defines; each binary owns its own because the deciding
/// library stays free of any system-call import.
struct Files {
    from: Fd,
    to: Fd,
}

impl CopyIo for Files {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, CopyError> {
        read(self.from, buf).map_err(|_| CopyError::Read)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), CopyError> {
        // mv.c:285: `write(to_fd, bp, nread) != nread` is the failure test.
        match write(self.to, buf) {
            Ok(n) if n == buf.len() => Ok(()),
            _ => Err(CopyError::Write),
        }
    }
}

/// `mv.c:256-354` `fastcopy`: move a plain file across devices by copying it,
/// stamping the source's times, owner and mode onto the copy, and only then
/// removing the source. Returns true when the move did not happen.
fn fastcopy(from: &str, to: &str, sb: &Stat, flags: &MvFlags) -> bool {
    let from_fd = match open(from, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("mv: {from}\n").as_bytes());
            return true;
        }
    };
    let to_fd = match open(
        to,
        cp::O_CREAT | cp::O_TRUNC | cp::O_WRONLY,
        move_create_mode(sb.st_mode),
    ) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("mv: {to}\n").as_bytes());
            let _ = close(from_fd);
            return true;
        }
    };

    let mut copy_failed = false;
    let mut scratch = alloc::vec![0u8; CHUNK];
    let mut io = Files {
        from: from_fd,
        to: to_fd,
    };
    match cp::pump(&mut io, &mut scratch) {
        Ok(_) => {}
        Err(CopyError::Write) => {
            support::warn(format!("mv: {to}\n").as_bytes());
            copy_failed = true;
        }
        Err(CopyError::Read) => {
            support::warn(format!("mv: {from}\n").as_bytes());
            copy_failed = true;
        }
    }
    if copy_failed {
        // mv.c:291-295: the half-written destination is removed, and even a
        // failed removal is reported — a partial file left behind must not
        // outlive the source.
        if unlink(to).is_err() {
            support::warn(format!("mv: {to}: remove\n").as_bytes());
        }
        let _ = close(from_fd);
        let _ = close(to_fd);
        return true;
    }
    let _ = close(from_fd); // mv.c:303

    // mv.c:304-327: the timestamps ride on the descriptor through futimens.
    let times = [
        (sb.st_atim.tv_sec, sb.st_atim.tv_nsec),
        (sb.st_mtim.tv_sec, sb.st_mtim.tv_nsec),
    ];
    if futimens(to_fd, Some(times)).is_err() {
        support::warn(format!("mv: {to}: set times\n").as_bytes());
    }

    // mv.c:328-334: owner, then mode. A failed chown always costs the set-id
    // bits; only a failure other than EPERM is reported (mv.c:329-331).
    let mut mode = sb.st_mode;
    if let Err(e) = chown(to, sb.st_uid, sb.st_gid) {
        if e != Errno::EPERM {
            support::warn(format!("mv: {to}: set owner/group\n").as_bytes());
        }
        mode &= !(cp::S_ISUID | cp::S_ISGID);
    }
    if chmod(to, mode).is_err() {
        support::warn(format!("mv: {to}: set mode\n").as_bytes());
    }

    if close(to_fd).is_err() {
        support::warn(format!("mv: {to}\n").as_bytes());
        return true;
    }
    // mv.c:345-348: the source goes last, so a failure here leaves both names.
    if unlink(from).is_err() {
        support::warn(format!("mv: {from}: remove\n").as_bytes());
        return true;
    }
    // mv.c:350-351: the echo happens here, not in do_move.
    if flags.verbose {
        support::emit(format!("{from} -> {to}\n").as_bytes());
    }
    false
}

/// mv.c:187-191: the "override" question. It prints the nine permission
/// characters (`modep + 1`), a separator C adds unless the tenth character is
/// already a space, then the owner and group names.
///
/// C gets those names from `user_from_uid`/`group_from_gid` (`pwcache.c`),
/// which fall back to the numeric identifier in decimal when the password
/// database has no entry (`pwcache.c:296-306`). This tree has no password
/// lookup at all, so it always takes that fallback branch: the message carries
/// the numbers. The separator is computed from the rendered string exactly as C
/// tests `modep[9] == ' '`, so it matches whatever layout `strmode` produces.
fn override_prompt(target: &str, sb: &Stat) {
    let rendered = strmode(sb.st_mode);
    let perms = core::str::from_utf8(&rendered[1..10]).unwrap_or_default();
    let sep = if rendered[9] == b' ' { "" } else { " " };
    support::warn(
        format!(
            "override {perms}{sep}{}/{} for {target}? ",
            sb.st_uid, sb.st_gid
        )
        .as_bytes(),
    );
}

/// mv.c:195-201: the first character answers, the rest of the line is drained,
/// and end-of-file counts as "no".
fn answer_yes() -> bool {
    let mut buf = [0u8; 64];
    let first = match read(support::STDIN, &mut buf) {
        Ok(0) | Err(_) => return false,
        Ok(n) => buf[0..n].first().copied().unwrap_or(b'\n'),
    };
    if first != b'\n' {
        loop {
            match read(support::STDIN, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf[..n].contains(&b'\n') {
                        break;
                    }
                }
            }
        }
    }
    first == b'y' || first == b'Y'
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
