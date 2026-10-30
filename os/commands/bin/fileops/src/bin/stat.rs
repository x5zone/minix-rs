//! Minix-RS `stat` — the doing half over `minix_fileops::stat`.
//!
//! Ground truth: `minix3/usr.bin/stat/stat.c` (NetBSD 1.38). The template
//! engine — option face, predefined formats, `%`-spec parsing, and the per-datum
//! rendering — lives in `minix_fileops::stat`; this program supplies only the
//! three legs the deciding half cannot hold: `lstat`/`stat`/`fstat` for the
//! status itself, `readlink` for `%Y`, and the byte sink.
//!
//! Wired: `stat [-FlLnqrsx] [-f format] [-t timefmt] [file ...]`, plus the same
//! code under the name `readlink` (`stat.c:217-232`), where `-f` means `%R`
//! and the exit code comes from the failed-link flag instead of the error flag.
//!
//! Two declared differences, all from legs this tree does not have:
//! - `%Su`/`%Sg` always take C's "no such password entry" branch and print
//!   `(123)`, because there is no password database wrapper in this tree —
//!   the same missing leg that blocks `chown`.
//! - `%d`/`%r` in the string form print `???`, which is what C prints when
//!   `devname()` finds nothing (`stat.c:609-613`); no `/dev` scan exists here.
//!
//! The report legs are wired to the shared renderer (`err_line` in the
//! `minix-types` crate, shape from `minix3/lib/libc/gen/vwarnc.c`), and all three of
//! C's lines here are the `warn(3)` family, so each carries the number's sentence:
//! `warn("%s: %s", name, usestat ? "stat" : "lstat")` (`stat.c:340-344`) becomes
//! `stat: <name>: stat: <sentence>`, `warn("readlink `%s'", file)` (`stat.c:849`)
//! becomes `stat: readlink \`<name>': <sentence>`, and `warn("realpath `%s'", file)` (`:828`) the
//! same shape with the number `getcwd.c` set — four of that folder's failure kinds
//! assign the number themselves (`ENOENT`/`ENAMETOOLONG`/`ELOOP`/`ENOTDIR`) and are
//! reproduced, while the three that pass a syscall's number through cannot be, since
//! the folding probe answers only yes-or-no; those three print the phrase alone
//! rather than an invented number, and the probe interface is registered as a
//! follow-up batch (the same item `find`'s walk probe needs).
//!
//! Time: C breaks the seconds down with `localtime`; this layer has no time-zone
//! database, so the breakdown is UTC — the same declared boundary as `date` and
//! `who` (`minix_proctools::dates`).
//!
//! Real-machine effect for read-only output is not gated by the writable-root
//! note (E5): `stat` only reads. It is not yet seeded into the image, so the
//! serial gate does not cover it; see the note in
//! `rewrite-notes/18-stage-commands/todo.md`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::realpath::{PathProbe, RealPathError, real_path};
use minix_fileops::stat::{
    self, FaceError, Plan, PlanError, Report, StatData, TimeField, parse_args, parse_format, render,
};
use minix_sys::{Errno, Stat, fstat, getcwd, lstat, readlink, stat as stat_follows};

/// Width of `readlink` and of the path buffer, the same capacity as C's `path`
/// array (`stat.c:578`), and equal to `realpath`'s upper bound `PATH_MAX`
/// (`minix-sys`'s line-length limit is this value).
const LINK_BUF: usize = 1024;

/// `ELOOP` from `minix3/sys/sys/errno.h:114` (value 62, "too many symbolic
/// links"). `Errno` carries no associated constant by that name, so, following
/// `rm.rs`'s treatment of `ETXTBSY`, the shell defines the value locally and
/// converts.
const ELOOP: i32 = 62;

/// The `realpath(3)` leg `%R` needs. All three actions go through `minix_sys`;
/// the folding algorithm lives in `minix_fileops::realpath` (a port of
/// `minix3/lib/libc/gen/getcwd.c:64-215`).
struct SysProbe;

impl PathProbe for SysProbe {
    fn lstat_mode(&self, path: &str) -> Option<u32> {
        let mut st = zeroed();
        lstat(path, &mut st).ok().map(|()| st.st_mode)
    }

    fn read_link(&self, path: &str) -> Option<String> {
        let mut buf = [0u8; LINK_BUF];
        let len = readlink(path, &mut buf).ok()?;
        if len == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    fn working_directory(&self) -> Option<String> {
        let mut buf = [0u8; LINK_BUF];
        let bytes = getcwd(&mut buf).ok()?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Fold `%R` once. C also "speaks once on failure, sets `linkfail`, treats it
/// as an empty string" here, and that line is
/// `warn("realpath `%s'", file)` (`stat.c:824-833`) -- it attaches the sentence
/// for the number `getcwd.c` set.
fn resolve_for(prog: &str, name: &str, quiet: bool) -> Option<String> {
    match real_path(&SysProbe, name) {
        Ok(resolved) => Some(resolved),
        Err(e) => {
            if !quiet {
                match real_path_errno(e) {
                    Some(code) => support::warn_line(prog, &format!("realpath `{name}'"), code),
                    // The pass-through arms (`lstat`/`readlink`/`getcwd` failing
                    // on their own) lose the number at the probe interface, so
                    // the phrase alone beats inventing a fake number (registered
                    // in the header note).
                    None => support::warn(format!("{prog}: realpath `{name}'\n").as_bytes()),
                }
            }
            None
        }
    }
}

/// The four causes in `RealPathError` whose number `getcwd.c` assigns itself
/// (the other three are syscall pass-throughs this layer cannot get). The
/// coordinates are given per arm on the right.
fn real_path_errno(e: RealPathError) -> Option<Errno> {
    match e {
        // `getcwd.c:104-107` (empty path) and `:185-191` (zero-length link
        // target) both set `ENOENT`.
        RealPathError::NoEntry => Some(Errno::ENOENT),
        // `getcwd.c:159-165`.
        RealPathError::NameTooLong => Some(Errno::ENAMETOOLONG),
        // `getcwd.c:180-183` (link depth exceeds `MAXSYMLINKS`).
        RealPathError::Loop => Some(Errno::from_i32(ELOOP)),
        // `getcwd.c:207-209` (a middle component is not a directory and
        // content follows).
        RealPathError::NotDir => Some(Errno::ENOTDIR),
        RealPathError::StatFailed
        | RealPathError::ReadLinkFailed
        | RealPathError::NoWorkingDirectory => None,
    }
}

/// Whether the template mentions `%R` (including forms like `%SR` with a shape
/// letter). Fold only when it does, because this leg costs one `lstat` per
/// piece.
fn needs_realpath(pieces: &[minix_fileops::stat::Piece<'_>]) -> bool {
    pieces.iter().any(|piece| {
        matches!(
            piece,
            minix_fileops::stat::Piece::Spec(minix_fileops::stat::Spec {
                datum: minix_fileops::stat::Datum::Realpath,
                ..
            })
        )
    })
}

fn zeroed() -> Stat {
    // SAFETY: `Stat` is a byte struct of plain integer fields; all-zero is a
    // valid bit pattern, and a successful `stat`/`lstat`/`fstat` overwrites it
    // wholesale.
    unsafe { core::mem::zeroed() }
}

/// Fold the kernel-returned `Stat` into the deciding half's plain-field shape
/// (the deciding half never touches system-call types).
fn to_data(st: &Stat) -> StatData {
    StatData {
        dev: st.st_dev,
        mode: st.st_mode,
        ino: st.st_ino,
        nlink: st.st_nlink,
        uid: st.st_uid,
        gid: st.st_gid,
        rdev: st.st_rdev,
        atime: TimeField::new(st.st_atim.tv_sec, st.st_atim.tv_nsec),
        mtime: TimeField::new(st.st_mtim.tv_sec, st.st_mtim.tv_nsec),
        ctime: TimeField::new(st.st_ctim.tv_sec, st.st_ctim.tv_nsec),
        btime: TimeField::new(st.st_birthtim.tv_sec, st.st_birthtim.tv_nsec),
        size: st.st_size,
        blocks: st.st_blocks,
        blksize: st.st_blksize,
        flags: st.st_flags,
        generation: st.st_gen,
    }
}

fn usage(prog: &str, synopsis: &str) -> ! {
    // `stat.c:357-365`: `fprintf(stderr, "usage: %s %s\n", getprogname(), synopsis)`
    // -- the name is inside the line, and the synopsis changes with the
    // personality. Exit status 1.
    support::warn(format!("usage: {prog} {synopsis}\n").as_bytes());
    support::terminate(1);
}

fn bad_format(prog: &str, source: &str) -> ! {
    // C's `errx` prints `<prog>: <sub-format string>: bad format` (`stat.c:555-556`).
    support::warn(format!("{prog}: {source}: bad format\n").as_bytes());
    support::terminate(1);
}

/// Read the symlink target. If unreadable, return `None` and let the deciding
/// half set `linkfail` (`stat.c:846-853`'s failure arm speaks only once; this
/// place likewise reports only one line on failure). C's line is
/// `warn("readlink `%s'", file)` (`:849`) -- it attaches the number.
fn link_target(prog: &str, name: &str, quiet: bool) -> Option<String> {
    let mut buf = [0u8; LINK_BUF];
    match readlink(name, &mut buf) {
        Ok(len) => Some(String::from_utf8_lossy(&buf[..len]).into_owned()),
        Err(e) => {
            if !quiet {
                support::warn_line(prog, &format!("readlink `{name}'"), e);
            }
            None
        }
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C first does `setprogname(argv[0])` (`stat.c:217`) and then compares
    // `getprogname()` against the personality (`:220`), so this string is the
    // segment after stripping the directory, and the comparison is exact.
    let prog = support::prog_name(&argv);
    if argv.is_empty() {
        usage(prog, "[-FlLnqrsx] [-f format] [-t timefmt] [file ...]");
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let rest = &args[1..];

    let plan: Plan<'_> = match parse_args(rest, prog) {
        Ok(p) => p,
        Err(PlanError::Usage) => usage(
            prog,
            if prog == "readlink" {
                "[-fnqsv] [file ...]"
            } else {
                "[-FlLnqrsx] [-f format] [-t timefmt] [file ...]"
            },
        ),
        Err(PlanError::FormatConflict { first, second }) => {
            // `stat.c:261-263`.
            support::warn(
                format!("{prog}: can't use format '{first}' with '{second}'\n").as_bytes(),
            );
            support::terminate(1);
        }
        Err(PlanError::NotWithDashF { fmtchar }) => {
            // `stat.c:289-291`.
            support::warn(format!("{prog}: can't use format '{fmtchar}' with -F\n").as_bytes());
            support::terminate(1);
        }
    };

    let pieces = match parse_format(plan.format) {
        Ok(p) => p,
        Err(e) => bad_format(prog, &e.span),
    };
    let operands: &[&str] = &rest[plan.operands..];

    let mut errs = false;
    let mut linkfail = false;
    // C's `do/while` runs once even with no operands: that round is standard
    // input (`stat.c:324`).
    let rounds = if operands.is_empty() {
        1
    } else {
        operands.len()
    };
    let mut number = 1usize;

    for index in 0..rounds {
        let name = operands.get(index).copied();
        let mut st = zeroed();
        let got = match name {
            None => fstat(support::STDIN, &mut st),
            // `stat.c:327-334`: with `-L`, call `stat()` first and fall back to
            // `lstat()` only on `ENOENT`; when both fail, blame `ENOENT`.
            Some(path) => match (plan.use_stat, stat_follows(path, &mut st)) {
                (true, Err(Errno::ENOENT)) => lstat(path, &mut st),
                (_, outcome) => outcome,
            },
        };
        if let Err(e) = got {
            errs = true;
            linkfail = true;
            if !plan.quiet {
                // C's `warn("%s: %s", name, usestat ? "stat" : "lstat")`
                // (`stat.c:340-344`): those two words are the two `%s` in the
                // caller's format string, and the sentence comes from `errno`
                // -- all four segments from that one authority.
                let leg = if plan.use_stat { "stat" } else { "lstat" };
                support::warn_line(prog, &format!("{}: {leg}", name.unwrap_or("(stdin)")), e);
            }
            number += 1;
            continue;
        }

        let data = to_data(&st);
        let link = if (data.mode & minix_fileops::cp::S_IFMT) == minix_fileops::cp::S_IFLNK {
            name.and_then(|path| link_target(prog, path, plan.quiet))
        } else {
            None
        };
        let resolved = if needs_realpath(&pieces) {
            name.and_then(|path| resolve_for(prog, path, plan.quiet))
        } else {
            None
        };
        let ctx = stat::Render {
            data: &data,
            name,
            link: link.as_deref(),
            resolved: resolved.as_deref(),
            timefmt: plan.timefmt,
            file_number: number,
        };
        let mut out = String::new();
        let report: Report = match render(&pieces, &ctx, &mut out) {
            Ok(r) => r,
            Err(FaceError::BadFormat(source)) => bad_format(prog, &source),
        };
        support::emit(out.as_bytes());
        if !report.ends_with_newline && !plan.no_nl {
            support::emit(b"\n");
        }
        linkfail |= report.linkfail;
        number += 1;
    }

    // `stat.c:354`: the readlink personality returns `linkfail`, the stat
    // personality returns `errs`.
    support::terminate(if prog.ends_with("readlink") {
        linkfail as i32
    } else {
        errs as i32
    })
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run();
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run();
}

#[cfg(test)]
mod tests {
    // The thin shell itself only provides legs and the loop; judgement lives in
    // the deciding half's host tests (`stat.rs`'s `tests` and `face_tests`).
    // What is pinned here are the two shapes the thin shell folds itself:
    // `to_data`'s field mapping, and the mask constants used for the symlink
    // test.
    use super::*;

    #[test]
    fn to_data_maps_every_status_field_by_name() {
        let mut st = zeroed();
        st.st_dev = 1;
        st.st_mode = 0o100644;
        st.st_ino = 9;
        st.st_nlink = 2;
        st.st_uid = 3;
        st.st_gid = 4;
        st.st_rdev = 5;
        st.st_atim.tv_sec = 6;
        st.st_atim.tv_nsec = 7;
        st.st_size = -8;
        st.st_blocks = 8;
        st.st_blksize = 4096;
        st.st_flags = 0x21;
        st.st_gen = 11;
        let data = to_data(&st);
        assert_eq!(
            (
                data.dev, data.mode, data.ino, data.nlink, data.uid, data.gid, data.rdev
            ),
            (1, 0o100644, 9, 2, 3, 4, 5)
        );
        assert_eq!(data.atime, TimeField::new(6, 7));
        assert_eq!((data.size, data.blocks, data.blksize), (-8, 8, 4096));
        assert_eq!((data.flags, data.generation), (0x21, 11));
    }

    #[test]
    fn the_link_test_uses_the_same_mask_and_value_as_the_deciding_half() {
        // The thin shell's `0o170000`/`0o120000` must equal
        // `cp::S_IFMT`/`S_IFLNK`, or `%Y` would fail to read the target on a
        // real symlink.
        assert_eq!(0o170000, minix_fileops::cp::S_IFMT);
        assert_eq!(0o120000, minix_fileops::cp::S_IFLNK);
    }
}
