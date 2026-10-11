//! Minix-RS `cp` — the doing half over `minix_fileops::cp`.
//!
//! Ground truth: `minix3/bin/cp/cp.c` (option scan, target classification,
//! the per-source loop in `copy()`) and `minix3/bin/cp/utils.c`
//! (`copy_file`, `setfile`, `set_utimes`). The decisions — which shape, which
//! path, which mode — live in `minix_fileops::cp`; this program supplies the
//! `stat`, `open`, `read`, `write`, `chown`, `chmod`, `utimens` legs.
//!
//! Wired: `cp [-f|-i] [-p] [-v] src target` and `cp [-f|-i] [-p] [-v] src ...
//! directory`, where every source is a plain file (or something `stat` reports
//! with a size, which the pump reads the way the C loop does). `-i` only asks
//! when standard input is a terminal (`cp.c:148`), `-f` overwrites without
//! asking and retries through an unlink (`utils.c:150-156`), `-p` reproduces
//! ownership, permissions and both timestamps (`utils.c:364-404`), `-v` prints
//! `src -> dst` for each file that succeeded.
//!
//! Declared residual: the recursive family (`-R`, `-r`, `-a`) walks the tree
//! with fts(3), which this tree has not landed (the same residual `rm -r`
//! declares), so those letters are refused with a message instead of copying
//! one level and calling it done. `-l` (hard-link instead of copy), `-N`,
//! `-H`/`-L`/`-P` (symlink traversal modes, and with them `copy_link`) ride on
//! that same walk. Two mechanical deviations inside the wired path, both
//! invisible for a regular-file destination: the C code keeps `fchmod` for the
//! setuid-restore branch (`utils.c:288`) and `fchown`/`fchmod` in `setfile`
//! (`utils.c:376-381`), while this tree has no `f*` wrappers, so those bits are
//! set by path with `chmod`/`chown` before the descriptor is closed. The real
//! machine effect of every write-side command, this one included, is the
//! writable-root gate (`C-57`/E5); `strerror` suffixes are not reproduced.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp::{
    self, CopyError, CopyIo, CpFlags, CpOption, CpShapeError, FileIds, TargetState, classify,
    create_mode, has_trailing_slash, is_identical, is_unwanted_directory, join_target,
    preserved_mode, pump, pumps_data, regains_set_bits, retained_mode, strip_trailing_slashes,
};
use minix_sys::{
    Errno, Fd, Stat, chmod, chown, close, fstat, getuid, isatty, open, read, stat, umask, unlink,
    utimens, write,
};

/// The C pump's block size is `MAXBSIZE` (`utils.c:96`, one static buffer).
/// The buffer is not part of the observable behavior — the loop writes what it
/// read, block by block, at any chunk size — so the copy uses a small heap
/// chunk instead of a megabyte of `.bss` in every command image.
const CHUNK: usize = 8192;

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid bit
    // pattern that `stat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

/// Read the status of a path, `None` when it cannot be had.
fn stat_of(path: &str) -> Option<Stat> {
    let mut sb = zeroed();
    stat(path, &mut sb).ok().map(|()| sb)
}

fn ids_of(sb: &Stat) -> FileIds {
    FileIds {
        dev: sb.st_dev,
        ino: sb.st_ino,
        mode: sb.st_mode,
        uid: sb.st_uid,
        gid: sb.st_gid,
    }
}

/// The two open descriptors behind the pure pump (`utils.c:244-255`).
struct Files {
    from: Fd,
    to: Fd,
}

impl CopyIo for Files {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, CopyError> {
        read(self.from, buf).map_err(|_| CopyError::Read)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), CopyError> {
        // A short write is a failure exactly as C's `rcount != wcount` test is
        // (`utils.c:247`); no retry loop, because the C program has none.
        match write(self.to, buf) {
            Ok(n) if n == buf.len() => Ok(()),
            _ => Err(CopyError::Write),
        }
    }
}

/// `utils.c:93-296` `copy_file`. Returns true when the file was not copied.
#[allow(clippy::too_many_arguments)]
fn copy_file(
    from: &str,
    to: &str,
    fs: &Stat,
    flags: &CpFlags,
    dne: bool,
    myuid: u32,
    myumask: u32,
) -> bool {
    let from_fd = match open(from, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("cp: {from}\n").as_bytes());
            return true;
        }
    };

    // The destination: an existing file is truncated (`utils.c:143`), a missing
    // one is created with the source mode minus the set-id bits
    // (`utils.c:145-146`). `tolnk` (`utils.c:109`) is only set by `-P`/`-R`,
    // which this program refuses, so the link half of the branch below is not
    // reachable — but C's second `stat` of the target (`utils.c:138-142`) is
    // reproduced anyway: it is what makes `cp -f a b` refuse rather than create
    // `b` when `b` disappears between the two looks.
    let mut to_fd = if !dne {
        if flags.interactive && !overwrite_ok(to) {
            // utils.c:123-131: declining is not a failure, just a skip.
            let _ = close(from_fd);
            return false;
        }
        if stat_of(to).is_none() {
            support::warn(format!("cp: stat: {to}\n").as_bytes());
            let _ = close(from_fd);
            return true;
        }
        open(to, cp::O_WRONLY | cp::O_TRUNC, 0)
    } else {
        open(
            to,
            cp::O_WRONLY | cp::O_TRUNC | cp::O_CREAT,
            create_mode(fs.st_mode),
        )
    };

    if to_fd.is_err() && flags.force {
        // utils.c:148-156: remove the existing name and create a fresh file.
        let _ = unlink(to);
        to_fd = open(
            to,
            cp::O_WRONLY | cp::O_TRUNC | cp::O_CREAT,
            create_mode(fs.st_mode),
        );
    }

    let to_fd = match to_fd {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("cp: {to}\n").as_bytes());
            let _ = close(from_fd);
            return true;
        }
    };

    let mut failed = false;

    // utils.c:183: a zero-size source is opened and closed without one read.
    if pumps_data(fs.st_size) {
        let mut scratch = alloc::vec![0u8; CHUNK];
        let mut io = Files {
            from: from_fd,
            to: to_fd,
        };
        match pump(&mut io, &mut scratch) {
            Ok(_) => {}
            Err(CopyError::Write) => {
                support::warn(format!("cp: {to}\n").as_bytes());
                failed = true;
            }
            Err(CopyError::Read) => {
                support::warn(format!("cp: {from}\n").as_bytes());
                failed = true;
            }
        }
    }
    let _ = close(from_fd);

    if failed {
        let _ = close(to_fd);
        return true;
    }

    // utils.c:270: -p reproduces owner, mode and times. setfile is the fd form
    // in C; this tree has no fchown/fchmod, so it runs by path while the
    // descriptor is still open (the same file either way).
    if flags.preserve && setfile(to, fs) {
        failed = true;
    }

    // utils.c:282-290: without -p, a freshly created copy of a set-id file the
    // caller owns gets its bits back if the group came out the same.
    if regains_set_bits(dne, flags.preserve, fs.st_mode, fs.st_uid, myuid)
        && restore_set_bits(to, to_fd, fs, myumask)
    {
        failed = true;
    }

    if close(to_fd).is_err() {
        support::warn(format!("cp: {to}\n").as_bytes());
        failed = true;
    }

    // utils.c:294-297: the timestamps are stamped after the close, because the
    // data written below them would otherwise bump the modification time again.
    if flags.preserve && set_utimes(to, fs) {
        failed = true;
    }
    failed
}

/// `utils.c:119-131`: the `-i` question before clobbering an existing target.
///
/// C prints it with a bare `fprintf(stderr, "overwrite %s? ")` — no program-name
/// prefix and no quotes around the target — and reads the answer as the first
/// character of the line, draining the rest (`utils.c:126-127`). Both halves are
/// the shared `bin_support::answer_yes`, which `mv` and `rm` use as well.
fn overwrite_ok(to: &str) -> bool {
    support::warn(format!("overwrite {to}? ").as_bytes());
    support::answer_yes()
}

/// `utils.c:283-290` inside the retain branch: read back what the create got,
/// and if the group came out as the source's, re-apply the retained bits. C
/// reports both failure sites with the destination name and `rval = 1`; this
/// returns true for exactly those two cases and stays silent otherwise (a
/// group mismatch simply means the branch does nothing, `utils.c:286`).
fn restore_set_bits(to: &str, to_fd: Fd, fs: &Stat, myumask: u32) -> bool {
    let mut after = zeroed();
    if fstat(to_fd, &mut after).is_err() {
        support::warn(format!("cp: {to}\n").as_bytes());
        return true;
    }
    if fs.st_gid == after.st_gid && chmod(to, retained_mode(fs.st_mode, myumask)).is_err() {
        support::warn(format!("cp: {to}\n").as_bytes());
        return true;
    }
    false
}

/// `utils.c:364-404` `setfile` in its path form: owner first (a failed chown
/// loses the set-id bits), then the mode minus the sticky bit. Returns true
/// when the mode could not be applied (`utils.c:380-382`).
fn setfile(path: &str, fs: &Stat) -> bool {
    let mut mode = preserved_mode(fs.st_mode);
    if let Err(e) = chown(path, fs.st_uid, fs.st_gid) {
        // utils.c:376-387: EPERM is expected (we are not root) and silent, any
        // other failure is reported. Either way the set-id bits are lost.
        if e != Errno::EPERM {
            support::warn(format!("cp: chown: {path}\n").as_bytes());
        }
        mode &= !(cp::S_ISUID | cp::S_ISGID);
    }
    if chmod(path, mode).is_err() {
        support::warn(format!("cp: chmod: {path}\n").as_bytes());
        return true;
    }
    false
}

/// `utils.c:64-76` `set_utimes`: both timestamps, access then modification.
fn set_utimes(path: &str, fs: &Stat) -> bool {
    let times = [
        (fs.st_atim.tv_sec, fs.st_atim.tv_nsec),
        (fs.st_mtim.tv_sec, fs.st_mtim.tv_nsec),
    ];
    if utimens(path, Some(times)).is_err() {
        support::warn(format!("cp: utimens: {path}\n").as_bytes());
        return true;
    }
    false
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.is_empty() {
        usage();
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match CpFlags::parse(&args[1..], isatty(support::STDIN)) {
        Ok(v) => v,
        Err(CpOption::NotWired(c)) => {
            support::warn(format!("cp: -{c} not wired (fts directory walk)\n").as_bytes());
            support::terminate(1);
        }
        Err(CpOption::Unknown(_)) => usage(),
    };

    if operands.len() < 2 {
        usage(); // cp.c:171
    }

    // cp.c:209-216: the last operand is the target; its trailing slash is
    // remembered and then stripped. cp.c:293-297 strips the sources' too.
    let target_raw = operands[operands.len() - 1];
    let have_trailing_slash = has_trailing_slash(target_raw);
    let target = String::from(strip_trailing_slashes(target_raw));
    let sources: Vec<String> = operands[..operands.len() - 1]
        .iter()
        .map(|s| String::from(strip_trailing_slashes(s)))
        .collect();

    // cp.c:237-244: anything but ENOENT on the target is fatal.
    let mut target_sb = zeroed();
    let target_state = match stat(&target, &mut target_sb) {
        Ok(()) => TargetState::Exists {
            is_dir: cp::is_dir(target_sb.st_mode),
        },
        Err(e) if e == Errno::ENOENT => TargetState::Missing,
        Err(_) => {
            support::warn(format!("cp: {target}\n").as_bytes());
            support::terminate(1);
        }
    };

    let shape = match classify(sources.len(), target_state, have_trailing_slash) {
        Ok(s) => s,
        Err(CpShapeError::TooManySources) => usage(),
        Err(CpShapeError::SlashOnNonDir { target_exists }) => {
            if target_exists {
                support::warn(format!("cp: {target} is not a directory\n").as_bytes());
            } else {
                support::warn(format!("cp: directory {target} does not exist\n").as_bytes());
            }
            support::terminate(1);
        }
    };

    // cp.c:202, 205-206: the copying identity and the umask the retain branch
    // has to respect (read by setting it to zero and putting it back). A failed
    // `getuid` gets a value no file can own, so the branch simply does not fire.
    let myuid = getuid().map(|u| u.max(0) as u32).unwrap_or(u32::MAX);
    let myumask = match umask(0) {
        Ok(prev) => {
            let _ = umask(prev);
            prev
        }
        Err(_) => 0,
    };

    let mut any_failed = false;
    for src in &sources {
        let dest = join_target(&target, src, shape);

        let src_sb = match stat_of(src) {
            Some(sb) => sb,
            None => {
                // cp.c:341-348 (`FTS_NS`): warn, count it, keep going.
                support::warn(format!("cp: {src}\n").as_bytes());
                any_failed = true;
                continue;
            }
        };

        // cp.c:419-441: the destination decides whether this is a creation.
        let dest_sb = stat_of(&dest);
        if let Some(db) = &dest_sb {
            let db_ids = ids_of(db);
            let src_ids = ids_of(&src_sb);
            if is_identical(&src_ids, &db_ids) {
                support::warn(
                    format!("cp: {dest} and {src} are identical (not copied).\n").as_bytes(),
                );
                any_failed = true;
                continue;
            }
            if cp::overwrites_dir(src_sb.st_mode, db_ids.mode) {
                support::warn(
                    format!("cp: cannot overwrite directory {dest} with non-directory {src}\n")
                        .as_bytes(),
                );
                any_failed = true;
                continue;
            }
        }

        if is_unwanted_directory(src_sb.st_mode) {
            // cp.c:456-462: no -R, so a directory source is refused.
            support::warn(format!("cp: {src} is a directory (not copied).\n").as_bytes());
            any_failed = true;
            continue;
        }

        let failed = copy_file(
            src,
            &dest,
            &src_sb,
            &flags,
            dest_sb.is_none(),
            myuid,
            myumask,
        );
        any_failed |= failed;
        if flags.verbose && !failed {
            // cp.c:539-540.
            support::emit(format!("{src} -> {dest}\n").as_bytes());
        }
    }

    support::terminate(if any_failed { 1 } else { 0 });
}

fn usage() -> ! {
    support::warn(
        b"usage: cp [-R [-H | -L | -P]] [-f | -i] [-alNpv] src target\n\
                cp [-R [-H | -L | -P]] [-f | -i] [-alNpv] src1 ... srcN directory\n",
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
