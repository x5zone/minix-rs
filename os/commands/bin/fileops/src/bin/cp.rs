//! Minix-RS `cp` — the doing half over `minix_fileops::cp`.
//!
//! Ground truth: `minix3/bin/cp/cp.c` (option scan, target classification,
//! the per-source loop in `copy()`) and `minix3/bin/cp/utils.c`
//! (`copy_file`, `setfile`, `set_utimes`). The decisions — which shape, which
//! path, which mode — live in `minix_fileops::cp`; this program supplies the
//! `stat`, `open`, `read`, `write`, `chown`, `chmod`, `utimens` legs.
//!
//! Wired: `cp [-f|-i] [-p] [-v] src target`, `cp [-f|-i] [-p] [-v] src ...
//! directory`, and `-R` over either — the hierarchy rooted at a source is
//! duplicated, directories created then stamped, plain files pumped, links
//! recreated from their own text, fifos and device nodes recreated. `-i` only
//! asks when standard input is a terminal (`cp.c:148`), `-f` overwrites without
//! asking and retries through an unlink (`utils.c:150-156`), `-p` reproduces
//! ownership, permissions and both timestamps (`utils.c:364-404`), `-v` prints
//! `src -> dst` for each visit that succeeded.
//!
//! Declared residual (refused up front, like `chmod`'s `-R`): `-r`, the
//! deprecated logical recursion (`cp.c:175-188`); `-a`, `-P`, `-H`, `-L`, the
//! symlink-traversal modes, of which `-P` additionally needs the link-preserving
//! `lchown`/`lchmod` (`utils.c:381-389`) this tree does not wrap; `-l`, hard-link
//! instead of copy (`utils.c:167-176`); `-N`, keep a dangling link dangling
//! (`utils.c:395`). Three mechanical deviations inside the wired path:
//! the C code keeps `fchmod` for the setuid-restore branch (`utils.c:288`) and
//! `fchown`/`fchmod` in `setfile` (`utils.c:381-389`), while this tree has no
//! `f*` wrappers, so those bits are set by path with `chmod`/`chown` before the
//! descriptor is closed; `-p` on a duplicated symlink therefore stamps only its
//! timestamps (through `utimensat` with `AT_SYMLINK_NOFOLLOW`), because the
//! by-path `chown`/`chmod` would travel through the new link onto its target;
//! and fts(3)'s cycle report (`cp.c:349-352`) has no counterpart, a physical
//! walk never rejoins a tree through a link. The real machine effect of every
//! write-side command, this one included, is the writable-root gate
//! (`E5`).
//!
//! The diagnostic legs are wired to the shared renderer (`err_line` in the
//! `minix-types` crate, shape from `minix3/lib/libc/gen/vwarnc.c`), and they are
//! two families that must not be merged:
//! - `warn("%s", path)` / `warn("<step>: %s", path)` (`utils.c:103`, `:137`, `:159`,
//!   `:226`, `:237`, `:256`, `:285`, `:289`, `:294`, `:311`, `:316`, `:320`, `:330`,
//!   `:334`, `:344`, `:348`, `:384`, `:390`, and `:71`'s `lutimens` label) print
//!   `cp: <label and path>: <strerror sentence>` and carry on;
//! - `warnx`/`errx` lines (`cp.c:277`, `:280`, `:426`, `:435`, `:458`) keep the
//!   program name but have **no** sentence, because C never consulted `errno`.
//!
//! `err(EXIT_FAILURE, …)` (`cp.c:242`, `:264`, `:484`, `:489`, `:543`) prints that
//! same line and then leaves the run, which this program does too: the target's
//! `stat`, the first source's `lstat`, a failed `mkdir`, a destination that is not
//! a directory, and a failed directory read are fatal, not per-operand warnings.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp::{
    self, CopyError, CopyIo, CpFlags, CpOption, CpShapeError, EntryAction, FileIds, TargetState,
    classify, create_mode, entry_action, has_trailing_slash, is_identical, is_link,
    is_unwanted_directory, join_target, post_create_dir_mode, preserved_mode, pump, pumps_data,
    regains_set_bits, retained_mode, strip_trailing_slashes, writes_through,
};
use minix_fileops::dents;
use minix_sys::{
    Errno, Fd, Stat, chmod, chown, close, fstat, getdents, getuid, isatty, lstat, mkdir, mkfifo,
    mknod, open, read, readlink, stat, symlink, umask, unlink, utimens, utimensat, vfs, write,
};

/// The C pump's block size is `MAXBSIZE` (`utils.c:96`, one static buffer).
/// The buffer is not part of the observable behavior — the loop writes what it
/// read, block by block, at any chunk size — so the copy uses a small heap
/// chunk instead of a megabyte of `.bss` in every command image.
const CHUNK: usize = 8192;

/// The refill buffer for the `-R` walk (C's fts keeps its own; the size is not
/// observable, it only bounds how many records arrive per call).
const WALK_BUF: usize = 4096;

/// `utils.c:308` reads a link's text into `char target[MAXPATHLEN]`; the buffer
/// bounds how long a duplicated link text can be, and a longer one is reported
/// rather than truncated behind the caller's back.
const LINK_BUF: usize = 1024;

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid bit
    // pattern that `stat` overwrites on success.
    unsafe { core::mem::zeroed() }
}

/// Read the status of a path, keeping the number for the legs that print it
/// (`cp.c:237-244`'s fatal target look, `cp.c:419`'s destination look, and
/// `utils.c:134-135`'s look at a destination that may itself be a link).
fn stat_code(path: &str) -> Result<Stat, Errno> {
    let mut sb = zeroed();
    stat(path, &mut sb)?;
    Ok(sb)
}

/// `lstat` for the physical `-R` scan (`cp.c:174, 190-196`), number included
/// (`cp.c:258-266`, `utils.c:135`).
fn lstat_code(path: &str) -> Result<Stat, Errno> {
    let mut sb = zeroed();
    lstat(path, &mut sb)?;
    Ok(sb)
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
///
/// `last_error` records the number the failing `read`/`write` answered with: C
/// reads it out of `errno` at the report site (`utils.c:226`, `:237`, `:256`), and
/// the pump in the deciding half only says which leg broke. Keeping the number on
/// the io object rather than in `CopyError` leaves that pure type untouched.
struct Files {
    from: Fd,
    to: Fd,
    last_error: Option<Errno>,
}

impl CopyIo for Files {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, CopyError> {
        match read(self.from, buf) {
            Ok(n) => Ok(n),
            Err(e) => {
                self.last_error = Some(e);
                Err(CopyError::Read)
            }
        }
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), CopyError> {
        // A short write is a failure exactly as C's `rcount != wcount` test is
        // (`utils.c:247`); no retry loop, because the C program has none. C has
        // no number to print for the short-write case either (`errno` is whatever
        // the write left), so this reports the number the transport answered and
        // nothing when a write merely came back short.
        match write(self.to, buf) {
            Ok(n) if n == buf.len() => Ok(()),
            Ok(_) => {
                self.last_error = None;
                Err(CopyError::Write)
            }
            Err(e) => {
                self.last_error = Some(e);
                Err(CopyError::Write)
            }
        }
    }
}

/// `utils.c:93-296` `copy_file`. Returns true when the file was not copied.
#[allow(clippy::too_many_arguments)]
fn copy_file(
    prog: &str,
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
        Err(e) => {
            // `utils.c:102-105`: `warn("%s", entp->fts_path)` — the source name and
            // that number's sentence.
            support::warn_line(prog, from, e);
            return true;
        }
    };

    // The destination: an existing file is truncated (`utils.c:143`), a missing
    // one is created with the source mode minus the set-id bits
    // (`utils.c:145-146`).
    //
    // `utils.c:109` computes `tolnk = (Rflag && !(Lflag || Hflag)) || Pflag`,
    // which with `-L`/`-H`/`-P` refused is exactly the recursive scan. A
    // destination that is itself a symbolic link is then never written through:
    // the `lstat` says so (`utils.c:134-135`), the open is skipped
    // (`utils.c:142-143`) and the unlink-and-recreate branch replaces the link
    // with a fresh regular file. C's second look at the target is what makes
    // `cp -f a b` refuse rather than create `b` when `b` disappears between the
    // two looks, and it is reproduced either way.
    let tolnk = flags.recursive;
    // C keeps one `to_fd` int with a `-1` sentinel; this records the descriptor
    // and, separately, the number the failed open answered with, because
    // `utils.c:159` prints that number and not a made-up one.
    let mut to_fd: Option<Fd> = None;
    let mut open_error: Option<Errno> = None;
    if !dne {
        if flags.interactive && !overwrite_ok(to) {
            // utils.c:123-131: declining is not a failure, just a skip.
            let _ = close(from_fd);
            return false;
        }
        let dest = if tolnk { lstat_code(to) } else { stat_code(to) };
        let dest_sb = match dest {
            Ok(sb) => sb,
            // `utils.c:137`: `warn("stat: %s", to.p_path)` — the label is part of
            // C's format string, so the line reads `cp: stat: <to>: <sentence>`.
            Err(e) => {
                support::warn_line(prog, &format!("stat: {to}"), e);
                let _ = close(from_fd);
                return true;
            }
        };
        if writes_through(tolnk, is_link(dest_sb.st_mode)) {
            match open(to, cp::O_WRONLY | cp::O_TRUNC, 0) {
                Ok(fd) => to_fd = Some(fd),
                Err(e) => open_error = Some(e), // utils.c:142-143: `to_fd` stays at -1
            }
        }
    } else {
        match open(
            to,
            cp::O_WRONLY | cp::O_TRUNC | cp::O_CREAT,
            create_mode(fs.st_mode),
        ) {
            Ok(fd) => to_fd = Some(fd),
            Err(e) => open_error = Some(e),
        }
    }

    if to_fd.is_none() && (flags.force || tolnk) {
        // utils.c:148-156: remove the existing name and create a fresh file.
        let _ = unlink(to);
        open_error = None;
        match open(
            to,
            cp::O_WRONLY | cp::O_TRUNC | cp::O_CREAT,
            create_mode(fs.st_mode),
        ) {
            Ok(fd) => to_fd = Some(fd),
            Err(e) => open_error = Some(e),
        };
    }

    let Some(to_fd) = to_fd else {
        // `utils.c:159-162`: no descriptor means the create/open failed, and the
        // destination name goes out with that number. A destination that is
        // itself a link (`tolnk && S_ISLNK`) leaves `to_fd` at `-1` without an
        // attempted open, but that shape always falls into the unlink-and-create
        // retry above (`utils.c:148`), so a missing number here would be a bug
        // rather than a C case: stay silent instead of inventing one.
        if let Some(e) = open_error {
            support::warn_line(prog, to, e);
        }
        let _ = close(from_fd);
        return true;
    };

    let mut failed = false;

    // utils.c:183: a zero-size source is opened and closed without one read.
    if pumps_data(fs.st_size) {
        let mut scratch = alloc::vec![0u8; CHUNK];
        let mut io = Files {
            from: from_fd,
            to: to_fd,
            last_error: None,
        };
        if let Err(which) = pump(&mut io, &mut scratch) {
            // `utils.c:226`/`:237` report the destination for a failed `write`
            // and `:256` reports the source for a failed `read`; the number is
            // the one that leg answered with.
            let path = match which {
                CopyError::Write => to,
                CopyError::Read => from,
            };
            if let Some(e) = io.last_error {
                support::warn_line(prog, path, e);
            }
            failed = true;
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
    if flags.preserve && setfile(prog, to, fs) {
        failed = true;
    }

    // utils.c:282-290: without -p, a freshly created copy of a set-id file the
    // caller owns gets its bits back if the group came out the same.
    if regains_set_bits(dne, flags.preserve, fs.st_mode, fs.st_uid, myuid)
        && restore_set_bits(prog, to, to_fd, fs, myumask)
    {
        failed = true;
    }

    if let Err(e) = close(to_fd) {
        // `utils.c:294-297`: a `close` that fails is a `warn` on the destination.
        support::warn_line(prog, to, e);
        failed = true;
    }

    // utils.c:294-297: the timestamps are stamped after the close, because the
    // data written below them would otherwise bump the modification time again.
    if flags.preserve && set_utimes(prog, to, fs) {
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
fn restore_set_bits(prog: &str, to: &str, to_fd: Fd, fs: &Stat, myumask: u32) -> bool {
    let mut after = zeroed();
    if let Err(e) = fstat(to_fd, &mut after) {
        // `utils.c:284-286`: `warn("%s", to.p_path)` on the `fstat` failure.
        support::warn_line(prog, to, e);
        return true;
    }
    if fs.st_gid == after.st_gid
        && let Err(e) = chmod(to, retained_mode(fs.st_mode, myumask))
    {
        // `utils.c:288-290`: same line for the `fchmod` failure (this tree runs
        // it by path, the name and the number are unchanged).
        support::warn_line(prog, to, e);
        return true;
    }
    false
}

/// `utils.c:364-404` `setfile` in its path form: owner first (a failed chown
/// loses the set-id bits), then the mode minus the sticky bit. Returns true
/// when the mode could not be applied (`utils.c:380-382`).
fn setfile(prog: &str, path: &str, fs: &Stat) -> bool {
    let mut mode = preserved_mode(fs.st_mode);
    if let Err(e) = chown(path, fs.st_uid, fs.st_gid) {
        // utils.c:376-387: EPERM is expected (we are not root) and silent, any
        // other failure is `warn("chown: %s", …)` — label, name, sentence. Either
        // way the set-id bits are lost.
        if e != Errno::EPERM {
            support::warn_line(prog, &format!("chown: {path}"), e);
        }
        mode &= !(cp::S_ISUID | cp::S_ISGID);
    }
    if let Err(e) = chmod(path, mode) {
        support::warn_line(prog, &format!("chmod: {path}"), e);
        return true;
    }
    false
}

/// `utils.c:64-76` `set_utimes`: both timestamps, access then modification.
/// C's label is the call it makes — `lutimens` (`utils.c:71`) — and this tree's
/// wrapper answers the same request, so the line keeps C's word rather than
/// naming the Rust function.
fn set_utimes(prog: &str, path: &str, fs: &Stat) -> bool {
    let times = [
        (fs.st_atim.tv_sec, fs.st_atim.tv_nsec),
        (fs.st_mtim.tv_sec, fs.st_mtim.tv_nsec),
    ];
    if let Err(e) = utimens(path, Some(times)) {
        support::warn_line(prog, &format!("lutimens: {path}"), e);
        return true;
    }
    false
}

/// `utils.c:364-419` with no descriptor: ownership, mode, then both timestamps,
/// each failure counted and none of them skipping the others (`setfile(fs, 0)`
/// calls `set_utimes` itself at `utils.c:417`).
fn setfile_and_times(prog: &str, path: &str, fs: &Stat) -> bool {
    let mut failed = setfile(prog, path, fs);
    if set_utimes(prog, path, fs) {
        failed = true;
    }
    failed
}

/// The `-p` stamping of a freshly duplicated symbolic link. C reaches it through
/// `lchown`/`lchmod` (`utils.c:381-389`), which this tree does not wrap; the
/// by-path `chown`/`chmod` would travel through the new link onto its target,
/// so only the timestamps are reproduced, link-preservingly.
fn set_link_times(prog: &str, path: &str, fs: &Stat) -> bool {
    let times = [
        (fs.st_atim.tv_sec, fs.st_atim.tv_nsec),
        (fs.st_mtim.tv_sec, fs.st_mtim.tv_nsec),
    ];
    if let Err(e) = utimensat(vfs::AT_FDCWD, path, Some(times), vfs::AT_SYMLINK_NOFOLLOW) {
        support::warn_line(prog, &format!("lutimens: {path}"), e);
        return true;
    }
    false
}

/// `cp.c:339-541` for one walk position: read the source the way the scan reads
/// it (`lstat` for a physical scan, `stat` when the default scan follows the
/// operand), read the destination once (`cp.c:419`), run the identical and
/// directory-overwrite refusals before any copying, then let the type bits pick
/// the routine. Returns true when this position was not copied.
fn copy_one(
    prog: &str,
    src: &str,
    dst: &str,
    flags: &CpFlags,
    myuid: u32,
    myumask: u32,
    physical: bool,
) -> bool {
    let src_read = if physical {
        lstat_code(src)
    } else {
        stat_code(src)
    };
    let src_sb = match src_read {
        Ok(sb) => sb,
        // cp.c:341-348 (`FTS_NS`): `warnx("%s: %s", fts_path, strerror(errno))` —
        // program name, the path, that number's sentence; count it, keep going.
        Err(e) => {
            support::warn_line(prog, src, e);
            return true;
        }
    };

    // cp.c:419-441: the destination decides whether this is a creation, and two
    // refusals come before the type dispatch. Neither consults `errno`, so both
    // are `warnx`-shaped: program name plus the phrase, no sentence.
    let dst_sb = stat_code(dst).ok();
    if let Some(db) = &dst_sb {
        if is_identical(&ids_of(&src_sb), &ids_of(db)) {
            support::warn(
                format!("{prog}: {dst} and {src} are identical (not copied).\n").as_bytes(),
            );
            // cp.c:429-430 adds `fts_set(FTS_SKIP)`: a directory that is its own
            // destination is not walked at all, which is why this returns
            // instead of falling through.
            return true;
        }
        if cp::overwrites_dir(src_sb.st_mode, db.st_mode) {
            support::warn(
                format!("{prog}: cannot overwrite directory {dst} with non-directory {src}\n")
                    .as_bytes(),
            );
            return true;
        }
    }

    let action = entry_action(src_sb.st_mode);
    let failed = match action {
        EntryAction::Directory => copy_dir(prog, src, dst, &src_sb, dst_sb, flags, myuid, myumask),
        // cp.c:444-453: the link arm is only taken when the scan did not follow
        // the link — the `-R` walk and a `-P`/`-a` operand (the default scan's
        // `stat` cannot report `S_IFLNK`).
        EntryAction::Symlink => copy_link(prog, src, dst, &src_sb, flags, dst_sb.is_some()),
        // cp.c:517-533: the three special-file arms recreate the entry only
        // with `-R`; without it they go through `copy_file` like everything
        // else (for a fifo that means the plain `open` of `utils.c:102`).
        EntryAction::Fifo if flags.recursive => {
            copy_fifo(prog, dst, &src_sb, flags, dst_sb.is_some())
        }
        EntryAction::BlockSpecial | EntryAction::CharacterSpecial if flags.recursive => {
            copy_special(prog, dst, &src_sb, flags, dst_sb.is_some())
        }
        // cp.c:534-537: everything with no branch of its own — a plain file, a
        // socket, an unrecognised type — goes through `copy_file`, where a
        // socket fails at the `open` exactly as it does in C.
        _ => copy_file(
            prog,
            src,
            dst,
            &src_sb,
            flags,
            dst_sb.is_none(),
            myuid,
            myumask,
        ),
    };
    // cp.c:539-540 prints for every visit but the two a directory makes for
    // itself, which `copy_dir` echoes around its own descent.
    if flags.verbose && !failed && !matches!(action, EntryAction::Directory) {
        support::emit(format!("{src} -> {dst}\n").as_bytes());
    }
    failed
}

/// `cp.c:455-516` for one directory. C sees a directory twice — `FTS_D` going
/// in, `FTS_DP` coming out — and keeps a stack purely to carry the "did I have
/// to create this?" answer between the two visits (`cp.c:303-321`); here the
/// two visits are the two halves of one frame, so the answer is a local.
#[allow(clippy::too_many_arguments)]
fn copy_dir(
    prog: &str,
    src: &str,
    dst: &str,
    sb: &Stat,
    dst_sb: Option<Stat>,
    flags: &CpFlags,
    myuid: u32,
    myumask: u32,
) -> bool {
    if is_unwanted_directory(sb.st_mode, flags.recursive) {
        // cp.c:456-462: without `-R` a directory source is refused and skipped.
        // C's `warnx("%s is a directory (not copied).", src)` has no sentence.
        support::warn(format!("{prog}: {src} is a directory (not copied).\n").as_bytes());
        return true;
    }
    let dne = dst_sb.is_none();

    // First pass (cp.c:470-493). Both failures are `err(EXIT_FAILURE)` in C, so
    // the whole run stops here rather than this operand alone.
    if dne {
        // cp.c:482-483 forces the owner bits on so that a 555 source directory
        // can be written into while it is being filled; the umask still applies.
        if let Err(e) = mkdir(dst, sb.st_mode | cp::S_IRWXU) {
            support::warn_line(prog, dst, e);
            support::terminate(1);
        }
    } else if let Some(db) = &dst_sb
        && !cp::is_dir(db.st_mode)
    {
        // cp.c:487-491: `errno = ENOTDIR; err(EXIT_FAILURE, "%s", to.p_path)` —
        // the number is the one C assigns itself, so the sentence is part of the
        // leg rather than something the filesystem answered.
        support::warn_line(prog, dst, Errno::ENOTDIR);
        support::terminate(1);
    }
    if flags.verbose {
        support::emit(format!("{src} -> {dst}\n").as_bytes());
    }

    let mut failed = descend_copy(prog, src, dst, flags, myuid, myumask);

    // Second pass (cp.c:494-507): `-p` stamps owner, mode and times, and a
    // directory that had to be created is chmod'ed back to the source mode
    // whether or not `-p` ran — the umask is undone here, "arguably wrong, but
    // it's been that way forever".
    let stamped_failed = flags.preserve && setfile_and_times(prog, dst, sb);
    if stamped_failed {
        failed = true;
    } else if dne {
        // cp.c:505-506 hands `setfile`'s already-masked mode to `chmod` when
        // `-p` ran (the sticky bit is gone by then) and the raw source mode
        // otherwise; `(void)` means the result is not even looked at.
        let _ = chmod(dst, post_create_dir_mode(sb.st_mode, flags.preserve));
    }
    if flags.verbose && !stamped_failed {
        support::emit(format!("{src} -> {dst}\n").as_bytes());
    }
    failed
}

/// Walk one source directory and copy each of its entries onto the matching
/// name under `dst_dir`. The fts(3) queue is replaced by this crate's own
/// directory decoder (`minix_fileops::dents`), the same substitution `rm -r`
/// made: freed slots and the two dot names are passed over, and every entry is
/// handed to `copy_one` as its own visit.
fn descend_copy(
    prog: &str,
    src_dir: &str,
    dst_dir: &str,
    flags: &CpFlags,
    myuid: u32,
    myumask: u32,
) -> bool {
    let fd = match open(src_dir, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(e) => {
            // cp.c:341-348 (`FTS_DNR`): the unreadable directory is reported with
            // its number and the run goes on; the caller still stamps it.
            support::warn_line(prog, src_dir, e);
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
                // `cp.c:542-544`: a failed `fts_read` with a number behind it is
                // `err(EXIT_FAILURE, "fts_read")` — the C name of the step, that
                // number's sentence, and the run leaves at once.
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
            // cp.c:387-416 keeps the path relative to the source root and
            // replays it under the destination root; joining the same name onto
            // both directories says the same thing.
            let child_src = dents::join_path(src_dir, name);
            let child_dst = dents::join_path(dst_dir, name);
            failed |= copy_one(prog, &child_src, &child_dst, flags, myuid, myumask, true);
        }
    }
    let _ = close(fd);
    failed
}

/// `utils.c:304-324` `copy_link`: duplicate the link text, never the target.
fn copy_link(prog: &str, src: &str, dst: &str, sb: &Stat, flags: &CpFlags, exists: bool) -> bool {
    let mut buf = [0u8; LINK_BUF];
    let len = match readlink(src, &mut buf) {
        Ok(len) => len,
        Err(e) => {
            // `utils.c:311`: `warn("readlink: %s", p->fts_path)`.
            support::warn_line(prog, &format!("readlink: {src}"), e);
            return true;
        }
    };
    // C hands the raw bytes to `symlink()`; this tree's syscall face takes
    // `&str`, so a link text that is not UTF-8 has no C counterpart to copy:
    // there is no number behind it either, so the line is the `warnx` shape with
    // a phrase that says what was refused.
    let text = match core::str::from_utf8(&buf[..len]) {
        Ok(text) => text,
        Err(_) => {
            support::warn(format!("{prog}: {src}: link text is not valid UTF-8\n").as_bytes());
            return true;
        }
    };
    if exists && let Err(e) = unlink(dst) {
        // `utils.c:316`: `warn("unlink: %s", to.p_path)`.
        support::warn_line(prog, &format!("unlink: {dst}"), e);
        return true;
    }
    if let Err(e) = symlink(text, dst) {
        // `utils.c:320`: `warn("symlink: %s", target)` — the label carries the
        // link **text**, not the destination name.
        support::warn_line(prog, &format!("symlink: {text}"), e);
        return true;
    }
    // utils.c:323: `pflag ? setfile(p->fts_statp, 0) : 0`.
    flags.preserve && set_link_times(prog, dst, sb)
}

/// `utils.c:326-338` `copy_fifo`: recreate the fifo with the source's mode.
fn copy_fifo(prog: &str, dst: &str, sb: &Stat, flags: &CpFlags, exists: bool) -> bool {
    if exists && let Err(e) = unlink(dst) {
        // `utils.c:330`: `warn("unlink: %s", to.p_path)`.
        support::warn_line(prog, &format!("unlink: {dst}"), e);
        return true;
    }
    if let Err(e) = mkfifo(dst, sb.st_mode) {
        // `utils.c:334`: `warn("mkfifo: %s", to.p_path)`.
        support::warn_line(prog, &format!("mkfifo: {dst}"), e);
        return true;
    }
    flags.preserve && setfile_and_times(prog, dst, sb)
}

/// `utils.c:340-352` `copy_special`: recreate a block or character node with the
/// source's mode and device number. Recreating a node needs the privilege the
/// file system checks for, so the usual answer outside root is `EPERM` here.
fn copy_special(prog: &str, dst: &str, sb: &Stat, flags: &CpFlags, exists: bool) -> bool {
    if exists && let Err(e) = unlink(dst) {
        support::warn_line(prog, &format!("unlink: {dst}"), e);
        return true;
    }
    if let Err(e) = mknod(dst, sb.st_mode, sb.st_rdev) {
        // `utils.c:348`: `warn("mknod: %s", to.p_path)`.
        support::warn_line(prog, &format!("mknod: {dst}"), e);
        return true;
    }
    flags.preserve && setfile_and_times(prog, dst, sb)
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C calls `setprogname(argv[0])` (`cp.c:113`) and every `warn`/`warnx`/`err`
    // line, plus the two usage lines, start from that one string.
    let prog = support::prog_name(&argv);
    if argv.is_empty() {
        usage(prog);
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match CpFlags::parse(&args[1..], isatty(support::STDIN)) {
        Ok(v) => v,
        Err(CpOption::NotWired(c)) => {
            support::warn(
                format!("{prog}: -{c} not wired (the -r/-H/-L traversal ladder, hard-link copy)\n")
                    .as_bytes(),
            );
            support::terminate(1);
        }
        Err(CpOption::Unknown(_)) => usage(prog),
    };

    if operands.len() < 2 {
        usage(prog); // cp.c:171
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

    // cp.c:237-244: anything but ENOENT on the target is fatal — `err(EXIT_FAILURE,
    // "%s", to.p_path)` takes the run down with that number's sentence behind it.
    let mut target_sb = zeroed();
    let target_state = match stat(&target, &mut target_sb) {
        Ok(()) => TargetState::Exists {
            is_dir: cp::is_dir(target_sb.st_mode),
        },
        Err(e) if e == Errno::ENOENT => TargetState::Missing,
        Err(e) => {
            support::warn_line(prog, &target, e);
            support::terminate(1);
        }
    };

    // cp.c:245-268: inside case (1) — the target is not a directory, which a
    // missing target always is — C takes the first operand's status through
    // `lstat` (`:262`; the `-r` branch that would use `stat` at `:260` is refused
    // here) and **exits** if that fails (`err(EXIT_FAILURE, "%s", *argv)`, `:264`).
    // The answer only changes the shape when `-R`/`-r` are set (`:267-268`), but
    // the status is taken either way: `cp missing-src new-dst` reports the source
    // and leaves before copying anything, even without `-R`.
    let source_is_dir = target_state == TargetState::Missing
        && match lstat_code(&sources[0]) {
            Ok(sb) => flags.recursive && cp::is_dir(sb.st_mode),
            Err(e) => {
                support::warn_line(prog, &sources[0], e);
                support::terminate(1);
            }
        };

    let shape = match classify(
        sources.len(),
        target_state,
        source_is_dir,
        flags.recursive,
        have_trailing_slash,
    ) {
        Ok(s) => s,
        Err(CpShapeError::TooManySources) => usage(prog),
        Err(CpShapeError::SlashOnNonDir { target_exists }) => {
            // `cp.c:275-281`: two `errx` lines — program name plus the phrase,
            // and no sentence, because C never looked at `errno` here.
            if target_exists {
                support::warn(format!("{prog}: {target} is not a directory\n").as_bytes());
            } else {
                support::warn(format!("{prog}: directory {target} does not exist\n").as_bytes());
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

    // cp.c:293-299: every operand goes into `copy()`, whose fts queue this
    // tree's `copy_one` answers one visit at a time — the operand itself for a
    // plain file, the whole hierarchy for a directory under `-R`.
    let mut any_failed = false;
    for src in &sources {
        let dest = join_target(&target, src, shape);
        // `cp.c:174-200`: only the no-flag default resolves the operand's
        // symlink, so `-R`, `-P` and `-a` all ask for the raw entry.
        let physical = cp::scan_is_physical(flags.recursive, flags.nofollow);
        any_failed |= copy_one(prog, src, &dest, &flags, myuid, myumask, physical);
    }

    support::terminate(if any_failed { 1 } else { 0 });
}

fn usage(prog: &str) -> ! {
    // `utils.c:422-430`: one `fprintf` with two lines, each carrying
    // `getprogname()`; the second line starts with seven spaces so it lines up
    // under the first one's argument.
    support::warn(
        format!("usage: {prog} [-R [-H | -L | -P]] [-f | -i] [-alNpv] src target\n").as_bytes(),
    );
    support::warn(
        format!("       {prog} [-R [-H | -L | -P]] [-f | -i] [-alNpv] src1 ... srcN directory\n")
            .as_bytes(),
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
