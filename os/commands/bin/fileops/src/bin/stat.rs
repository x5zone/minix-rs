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
//! Three declared differences, all from legs this tree does not have:
//! - `%R` (realpath) is refused rather than printed empty: resolving a path
//!   component by component has no wrapper here yet. The refusal happens per
//!   operand, the same place C would have warned (`stat.c:824-833`).
//! - `%Su`/`%Sg` always take C's "no such password entry" branch and print
//!   `(123)`, because there is no password database wrapper in this tree —
//!   the same missing leg that blocks `chown`.
//! - `%d`/`%r` in the string form print `???`, which is what C prints when
//!   `devname()` finds nothing (`stat.c:609-613`); no `/dev` scan exists here.
//!
//! Time: C breaks the seconds down with `localtime`; this layer has no time-zone
//! database, so the breakdown is UTC — the same declared boundary as `date` and
//! `who` (`minix_proctools::dates`).
//!
//! Real-machine effect for read-only output is not gated by the writable-root
//! note (`C-57`/E5): `stat` only reads. It is not yet seeded into the image, so
//! the serial gate does not cover it; see the batch note in
//! `rewrite-notes/18-stage-commands/todo.md`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::stat::{
    self, FaceError, Plan, PlanError, Report, StatData, TimeField, parse_args, parse_format, render,
};
use minix_sys::{Errno, Stat, fstat, lstat, readlink, stat as stat_follows};

/// `readlink` 缓冲的宽度，取 C 那份 `path` 数组的同级容量（`stat.c:578`）。
const LINK_BUF: usize = 1024;

fn zeroed() -> Stat {
    // SAFETY: `Stat` 是纯整数字段的字节结构，全零是合法位形，随后由
    // `stat`/`lstat`/`fstat` 在成功时整体覆写。
    unsafe { core::mem::zeroed() }
}

/// 把内核交回的 `Stat` 折成决定半的纯字段形（决定半不碰系统调用类型）。
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

fn usage(synopsis: &str) -> ! {
    // `stat.c:357-365`：`usage: <prog> <synopsis>`，退出码 1。
    support::warn(format!("usage: {synopsis}\n").as_bytes());
    support::terminate(1);
}

fn bad_format(prog: &str, source: &str) -> ! {
    // C 的 `errx` 打的是 `<prog>: <子格式串>: bad format`（`stat.c:555-556`）。
    support::warn(format!("{prog}: {source}: bad format\n").as_bytes());
    support::terminate(1);
}

/// 读符号链目标。读不到就交回 `None`，由决定半把 `linkfail` 置起
/// （`stat.c:846-853` 的失败支只出声一次，这里同样只在失败时报一行）。
fn link_target(name: &str, quiet: bool) -> Option<String> {
    let mut buf = [0u8; LINK_BUF];
    match readlink(name, &mut buf) {
        Ok(len) => Some(String::from_utf8_lossy(&buf[..len]).into_owned()),
        Err(e) => {
            if !quiet {
                support::warn(format!("stat: readlink `{name}'\n").as_bytes());
            }
            let _ = e;
            None
        }
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.is_empty() {
        usage("stat [-FlLnqrsx] [-f format] [-t timefmt] [file ...]");
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let prog = args[0];
    let rest = &args[1..];

    let plan: Plan<'_> = match parse_args(rest, prog) {
        Ok(p) => p,
        Err(PlanError::Usage) => usage(if prog.ends_with("readlink") {
            "[-fnqsv] [file ...]"
        } else {
            "[-FlLnqrsx] [-f format] [-t timefmt] [file ...]"
        }),
        Err(PlanError::FormatConflict { first, second }) => {
            // `stat.c:261-263`。
            support::warn(
                format!("{prog}: can't use format '{first}' with '{second}'\n").as_bytes(),
            );
            support::terminate(1);
        }
        Err(PlanError::NotWithDashF { fmtchar }) => {
            // `stat.c:289-291`。
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
    // C 的 `do/while` 即使无操作数也走一遍：那一遍是标准输入（`stat.c:324`）。
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
            // `stat.c:327-334`：`-L` 先 `stat()`，只有 `ENOENT` 才退回 `lstat()`，
            // 两趟都失败时把错归到 `ENOENT`。
            Some(path) => match (plan.use_stat, stat_follows(path, &mut st)) {
                (true, Err(Errno::ENOENT)) => lstat(path, &mut st),
                (_, outcome) => outcome,
            },
        };
        if got.is_err() {
            errs = true;
            linkfail = true;
            if !plan.quiet {
                // C 的 `warn` 会缀上 strerror；本树无 strerror，行止于腿名。
                let leg = if plan.use_stat { "stat" } else { "lstat" };
                support::warn(format!("{prog}: {}: {leg}\n", name.unwrap_or("(stdin)")).as_bytes());
            }
            number += 1;
            continue;
        }

        let data = to_data(&st);
        let link = if (data.mode & 0o170000) == 0o120000 {
            name.and_then(|path| link_target(path, plan.quiet))
        } else {
            None
        };
        let ctx = stat::Render {
            data: &data,
            name,
            link: link.as_deref(),
            timefmt: plan.timefmt,
            file_number: number,
        };
        let mut out = String::new();
        let report: Report = match render(&pieces, &ctx, &mut out) {
            Ok(r) => r,
            Err(FaceError::BadFormat(source)) => bad_format(prog, &source),
            Err(FaceError::RealpathUnported) => {
                // 本层没有 realpath 那条腿；照 C 的失败支出声并把这一列当空，
                // 但把退出码按「坏格式」定下来，免得把未接线当成渲染成功。
                if !plan.quiet {
                    support::warn(format!("{prog}: realpath leg not ported\n").as_bytes());
                }
                support::terminate(1);
            }
        };
        support::emit(out.as_bytes());
        if !report.ends_with_newline && !plan.no_nl {
            support::emit(b"\n");
        }
        linkfail |= report.linkfail;
        number += 1;
    }

    // `stat.c:354`：readlink 人格交回 `linkfail`，stat 人格交回 `errs`。
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
    // 薄壳本身只做腿与循环，判定全在决定半的宿主测里（`stat.rs` 的 `tests`
    // 与 `face_tests`）。此处只钉住两处薄壳自己的折形：`to_data` 的字段对应，
    // 以及符号链判定用的掩码常量。
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
        // 薄壳里的 `0o170000`/`0o120000` 必须与 `cp::S_IFMT`/`S_IFLNK` 同值，
        // 否则 `%Y` 会在真正的符链上不去读目标。
        assert_eq!(0o170000, minix_fileops::cp::S_IFMT);
        assert_eq!(0o120000, minix_fileops::cp::S_IFLNK);
    }
}
