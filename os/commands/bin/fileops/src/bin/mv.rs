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
//! and is reported rather than faked. Two mechanical deviations
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
//! command, this one included, is the writable-root gate (`E5`).
//!
//! The diagnostic legs go to the shared renderer (`err_line` in the `minix-types`
//! crate; shape from `minix3/lib/libc/gen/vwarnc.c`), and C's labels come in three
//! shapes that must not be levelled out: a leading one (`warn("rename %s", from)`,
//! `mv.c:178`, `:184`), a trailing one (`warn("%s: remove", to)`, `:292`, `:346`;
//! `"%s: set times"`/`"set owner/group"`/`"set mode"` at `:327-:334`), and none at
//! all (`warn("%s", from)`/`("%s", to)` at `:247`, `:268`, `:273`, `:286`, `:290`,
//! `:341`). The two bare `fprintf` questions (`overwrite`, `override`) keep C's
//! missing program name, and `warn(NULL)` at `:278` has no counterpart here: that
//! one reports a `malloc` failure, and this tree's chunk is a `Vec` allocation that
//! never answers with a number anyone could print.

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
fn do_move(prog: &str, source: &str, target: &str, flags: &MvFlags) -> bool {
    // mv.c:172-203, through the pure `ask_about` decision. The three probes C
    // makes are kept separate because they answer different questions:
    // `access(to, F_OK)` gates the block, `access(to, W_OK)` drives the
    // override branch, and `stat(to)` supplies the mode and the owner names.
    let mut tsb = zeroed();
    // C's ladder consults `access(from, F_OK)` inside both asking branches and,
    // when that fails, reports the number `access(2)` left behind — so the probe
    // is kept whole rather than flattened to a bool.
    let source_probe = access(source, F_OK);
    let ask = ask_about(
        flags.force,
        flags.interactive,
        access(target, F_OK).is_ok(),
        isatty(support::STDIN),
        access(target, W_OK).is_ok(),
        stat(target, &mut tsb).is_ok(),
        source_probe.is_ok(),
    );
    match ask {
        Ask::None => {}
        Ask::SourceUnreachable => {
            // mv.c:177-180: warn("rename %s", from) and count it — the label is
            // in front, the operand follows it, and that number's sentence ends
            // the line. The branch is only reached when the probe failed, so the
            // number is always there; the fallback is for the type, not for a
            // second C path.
            let e = source_probe.err().unwrap_or(Errno::ENOENT);
            support::warn_line(prog, &format!("rename {source}"), e);
            return true;
        }
        Ask::Overwrite => {
            // mv.c:181: a bare fprintf, so the question carries no program name
            // and no quotes around the target.
            support::warn(format!("overwrite {target}? ").as_bytes());
            if !support::answer_yes() {
                // mv.c:200-201: a "no" answer skips this operand, not an error.
                return false;
            }
        }
        Ask::Override => {
            // `ask_about` only answers Override when the target could be stat'd,
            // which is also when tsb holds the mode and owner mv.c:188 prints.
            override_prompt(target, &tsb);
            if !support::answer_yes() {
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
        Err(e) if e == Errno::from_i32(EXDEV) => cross_device_move(prog, source, target, flags),
        Err(e) => {
            // `mv.c:225`: `warn("rename %s to %s", from, to)`.
            support::warn_line(prog, &format!("rename {source} to {target}"), e);
            true
        }
    }
}

/// The `EXDEV` leg of C's `do_move` (`mv.c:229-253`): clear whatever the
/// destination still is, look at the source, then either copy it in place or
/// hand the job to the subprocess form the C program uses.
fn cross_device_move(prog: &str, source: &str, target: &str, flags: &MvFlags) -> bool {
    // mv.c:235-240: `lstat` the target; if it exists, remove it first, and a
    // failed removal is the whole error (the source is left alone).
    let mut tsb = zeroed();
    if lstat(target, &mut tsb).is_ok() {
        let failed = match existing_removal(tsb.st_mode) {
            RemovalCall::Rmdir => rmdir(target).err(),
            RemovalCall::Unlink => unlink(target).err(),
        };
        if let Some(e) = failed {
            // `mv.c:237`: `warn("can't remove %s", to)`.
            support::warn_line(prog, &format!("can't remove {target}"), e);
            return true;
        }
    }

    // mv.c:246-249: the source type decides which copy routine runs.
    let mut sb = zeroed();
    if let Err(e) = lstat(source, &mut sb) {
        // `mv.c:247`: `warn("%s", from)` — no label, just the name and the number.
        support::warn_line(prog, source, e);
        return true;
    }
    match cross_device_plan(sb.st_mode) {
        CrossDevice::InlineCopy => fastcopy(prog, source, target, &sb, flags),
        CrossDevice::NeedsSubprocess => {
            // mv.c:362-398: the C program execs /bin/cp -PRp and then
            // /bin/rm -rf. No exec delivery chain here yet, so the
            // move is reported as not done. C's own lines here would name
            // `_PATH_CP`/`_PATH_RM` and the child's exit status, neither of
            // which exists in this tree, so this phrase is ours.
            support::warn(
                format!("{prog}: cross-device move of a non-regular file needs /bin/cp plus /bin/rm (exec delivery chain, NS5)\n").as_bytes(),
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
    /// The number the failing `read`/`write` answered with; C reads it out of
    /// `errno` at the report site (`mv.c:286`, `:290`).
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
        // mv.c:285: `write(to_fd, bp, nread) != nread` is the failure test.
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

/// `mv.c:256-354` `fastcopy`: move a plain file across devices by copying it,
/// stamping the source's times, owner and mode onto the copy, and only then
/// removing the source. Returns true when the move did not happen.
fn fastcopy(prog: &str, from: &str, to: &str, sb: &Stat, flags: &MvFlags) -> bool {
    let from_fd = match open(from, cp::O_RDONLY, 0) {
        Ok(fd) => fd,
        Err(e) => {
            // `mv.c:267-270`: `warn("%s", from)`.
            support::warn_line(prog, from, e);
            return true;
        }
    };
    let to_fd = match open(
        to,
        cp::O_CREAT | cp::O_TRUNC | cp::O_WRONLY,
        move_create_mode(sb.st_mode),
    ) {
        Ok(fd) => fd,
        Err(e) => {
            // `mv.c:272-276`: `warn("%s", to)` and the source descriptor closes.
            support::warn_line(prog, to, e);
            let _ = close(from_fd);
            return true;
        }
    };

    let mut copy_failed = false;
    let mut scratch = alloc::vec![0u8; CHUNK];
    let mut io = Files {
        from: from_fd,
        to: to_fd,
        last_error: None,
    };
    if let Err(which) = cp::pump(&mut io, &mut scratch) {
        // `mv.c:285-291` reports the destination for a failed `write` and
        // `:290` reports the source for a failed `read`, each with its own number.
        let path = match which {
            CopyError::Write => to,
            CopyError::Read => from,
        };
        if let Some(e) = io.last_error {
            support::warn_line(prog, path, e);
        }
        copy_failed = true;
    }
    if copy_failed {
        // mv.c:291-295: the half-written destination is removed, and even a
        // failed removal is reported — a partial file left behind must not
        // outlive the source. C's label is a suffix: `warn("%s: remove", to)`.
        if let Err(e) = unlink(to) {
            support::warn_line(prog, &format!("{to}: remove"), e);
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
    if let Err(e) = futimens(to_fd, Some(times)) {
        support::warn_line(prog, &format!("{to}: set times"), e);
    }

    // mv.c:328-334: owner, then mode. A failed chown always costs the set-id
    // bits; only a failure other than EPERM is reported (mv.c:329-331).
    let mut mode = sb.st_mode;
    if let Err(e) = chown(to, sb.st_uid, sb.st_gid) {
        if e != Errno::EPERM {
            support::warn_line(prog, &format!("{to}: set owner/group"), e);
        }
        mode &= !(cp::S_ISUID | cp::S_ISGID);
    }
    if let Err(e) = chmod(to, mode) {
        support::warn_line(prog, &format!("{to}: set mode"), e);
    }

    if let Err(e) = close(to_fd) {
        // `mv.c:341`: a bare `warn("%s", to)`.
        support::warn_line(prog, to, e);
        return true;
    }
    // mv.c:345-348: the source goes last, so a failure here leaves both names.
    if let Err(e) = unlink(from) {
        support::warn_line(prog, &format!("{from}: remove"), e);
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

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // `mv.c:85`: `setprogname(argv[0])` once, and every `warn`/`errx` line plus
    // both usage lines read that one string.
    let prog = support::prog_name(&argv);

    let (flags, operands) = match MvFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => usage(prog),
    };

    let eval = match plan_operands(&operands) {
        MvPlan::Usage => usage(prog),
        MvPlan::Pair { source, target } => {
            // A directory target means "move into it" under the basename.
            if is_dir(target) {
                let dest = format!("{target}/{}", dest_name(source));
                do_move(prog, source, &dest, &flags)
            } else {
                do_move(prog, source, target, &flags)
            }
        }
        MvPlan::ToDirectory { sources, dir } => {
            if !is_dir(dir) {
                usage(prog); // C: >2 operands with a non-directory target -> usage
            }
            let mut eval = false;
            for src in &sources {
                let dest = format!("{dir}/{}", dest_name(src));
                eval |= do_move(prog, src, &dest, &flags);
            }
            eval
        }
    };

    support::terminate(if eval { 1 } else { 0 });
}

fn usage(prog: &str) -> ! {
    // `mv.c:401-409`: one `fprintf` with two `getprogname()` slots and no spaces
    // inside the option group (`[-fiv]`); the second line is indented seven spaces.
    support::warn(format!("usage: {prog} [-fiv] source target\n").as_bytes());
    support::warn(format!("       {prog} [-fiv] source ... directory\n").as_bytes());
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
