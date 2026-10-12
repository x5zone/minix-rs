//! Minix-RS `find` — the doing half over `minix_fileops::find`.
//!
//! Ground truth: `minix3/usr.bin/find/` (NetBSD 派). The deciding half holds the
//! option face, the expression parser, the predicate family, the whole-plan
//! evaluator, and the fts-shaped traversal (`find.c:201-273`'s `find_execute`).
//! This program supplies only the legs the deciding half cannot hold: `lstat`
//! for each entry's type, `getdents` for a directory's children, and `stat` for
//! the `-newer` reference file — the same three legs `du` wires (`bin/du.rs`).
//!
//! Wired: the physical traversal (`FTS_PHYSICAL | FTS_NOSTAT`, `main.c:88`) with
//! `-d` post-order, `-prune`, `-x` (do not cross devices), `-s` (sort), `-X`
//! (reject xargs-unsafe paths), and the implicit `-print` when the expression
//! has no output operator. Predicates and actions the deciding half already
//! judges are printed through it; `-exec`/`-delete`/`-user`/… still refuse by
//! name (their legs are absent), which is the evaluator's decision, not this
//! shell's.
//!
//! Declared differences, all from legs or C behaviours this tree does not carry:
//! - Warnings print the path plus a fixed phrase, not `strerror(errno)` (no
//!   strerror table here — the same boundary `stat`, `du`, and the `what`/`asa`
//!   batches document). `FTS_NS` prints `<path>: stat failed`, `FTS_DNR` prints
//!   `<path>: directory read failed`.
//! - A hard error while evaluating one entry (an unwired action, a malformed
//!   numeric argument) records the failure and moves on rather than calling
//!   `err(1)` and killing the whole walk. C exits the process (`find.c:269`,
//!   the `errx` sites in `function.c`); matching du's declared "a `getdents`
//!   error only counts the operand, does not terminate the run".
//! - `-L`/`-H` (logical traversal: follow symlinks) are parsed into the walk
//!   bits but this shell traverses physically regardless — resolving a symlink
//!   chain and its cycle detection needs full fts symlink handling, registered
//!   as a residual in `rewrite-notes/18-stage-commands/todo.md` (批次 next).
//! - SIGINFO progress reporting (`find.c:156-183`) needs signal registration
//!   this tree does not have on the command side; omitted, as in `sleep`.
//!
//! Real-machine effect is not yet gated: `find` is read-only, but seeding it
//! into the image and the serial assertion belong to `C-57`/E5, and the root
//! disk's directory-read leg is broken there (`ls: /bin: read error`), so a full
//! traversal is not observable on the minimal boot image yet.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::cp::O_RDONLY;
use minix_fileops::dents;
use minix_fileops::find::{
    FindError, ReferenceStat, StatBits, WalkProbe, World, parse, walk_paths,
};
use minix_sys::{Fd, Stat, close, getdents, gettimeofday, lstat, open, stat as stat_follows};

/// 读目录的逐次填充缓冲（与 `du` 同级）：尺寸不可观察，只限每趟 `getdents` 到达的记录数。
const WALK_BUF: usize = 4096;

fn zeroed() -> Stat {
    // SAFETY: `Stat` 是纯整数字段的字节结构，全零是合法位形，随后由
    // `stat`/`lstat` 在成功时整体覆写。
    unsafe { core::mem::zeroed() }
}

/// 走表腿与 `-newer` 的三条真源：`lstat`（型/时间/设备）、`getdents`（目次子项）、
/// `stat`（跟随符号链取参照文件时间）。全部走 `minix_sys`，决定半不碰系统调用。
struct SysProbe;

impl SysProbe {
    fn bits(st: &Stat) -> StatBits {
        StatBits {
            mode: st.st_mode,
            ino: st.st_ino,
            nlink: st.st_nlink,
            size: st.st_size,
            uid: st.st_uid,
            gid: st.st_gid,
            dev: st.st_dev,
            atime: st.st_atim.tv_sec,
            mtime: st.st_mtim.tv_sec,
            ctime: st.st_ctim.tv_sec,
        }
    }
}

impl ReferenceStat for SysProbe {
    /// `-newer` 的参照文件时间：C 走 `stat`（跟随符号链），取不到 `c_newer` 里 `err(1)`。
    /// 本层交回 `None`，求值半据此报 `<name>: illegal numeric value` 一支（见差异注）。
    fn times_of(&self, path: &str) -> Option<(i64, i64, i64)> {
        let mut sb = zeroed();
        stat_follows(path, &mut sb)
            .ok()
            .map(|()| (sb.st_atim.tv_sec, sb.st_mtim.tv_sec, sb.st_ctim.tv_sec))
    }
}

impl WalkProbe for SysProbe {
    /// `FTSENT::fts_statp` 的来源：物理遍历下不跟随符号链，故用 `lstat`（`main.c:88`）。
    fn entry_stat(&self, path: &str) -> Option<StatBits> {
        let mut sb = zeroed();
        lstat(path, &mut sb).ok().map(|()| Self::bits(&sb))
    }

    /// `readdir` 一次读完一个目次的全部条目（含 `.`/`..`，剔除 `d_ino == 0` 的空槽，
    /// `readdir.c:83-84`）。打不开 = C 的 `FTS_DNR`，交回 `None` 让走表腿报 warn。
    fn dir_children(&self, dir: &str) -> Option<Vec<String>> {
        let fd: Fd = match open(dir, O_RDONLY, 0) {
            Ok(fd) => fd,
            Err(_) => return None,
        };
        let mut names: Vec<String> = Vec::new();
        let mut buf = [0u8; WALK_BUF];
        let mut opened = true;
        loop {
            let transferred = match getdents(fd, &mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                // 读错：C 会 `err(1, "fts_read")` 终止全局；本层视同为该目次不可读，
                // 交回 `None`（走表腿报 `FTS_DNR`），差异登记在头注。
                Err(_) => {
                    opened = false;
                    break;
                }
            };
            let mut cursor = 0;
            while cursor < transferred {
                match dents::next_record(&buf, cursor) {
                    Ok(Some((entry, next))) => {
                        cursor = next;
                        if entry.ino == 0 {
                            continue;
                        }
                        if let Ok(name) = core::str::from_utf8(entry.name) {
                            names.push(String::from(name));
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }
        let _ = close(fd);
        if opened { Some(names) } else { None }
    }
}

fn run() -> ! {
    let argv = support::args();
    let refs: Vec<&str> = argv.iter().skip(1).map(|s| s.as_str()).collect();
    let parsed = match parse(&refs) {
        Ok(p) => p,
        Err(e) => {
            report_parse_error(&e);
            support::terminate(1);
        }
    };
    let now = match gettimeofday() {
        Ok((sec, _usec)) => sec,
        // 取不到时钟时退化为 0：只有时间类判据会受影响（`-atime` 等），与 C 的
        // `time(&now)` 相比是「纪元起算」，属无时钟源的诚实退化。
        Err(_) => 0,
    };
    let world = World {
        now,
        euid: 0,
        egid: 0,
        root_dev: 0,
    };
    let result = walk_paths(&world, &SysProbe, &parsed);
    for line in &result.emitted {
        support::emit(line.as_bytes());
    }
    for warn in &result.warnings {
        support::warn(format!("{warn}\n").as_bytes());
    }
    support::terminate(result.rval as i32)
}

/// `FindError` 逐支落到 C 的各条报错文案（`usage`/`unknown option`/`requires
/// additional arguments` 等）。无 strerror 表，故按名报，差异见头注。
fn report_parse_error(e: &FindError<'_>) {
    let msg = match e {
        FindError::Usage => String::from(
            "Usage: find [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]",
        ),
        FindError::UnknownOption(name) => format!("find: unknown option `{name}'"),
        FindError::NeedsArguments(name) => {
            format!("find: {name}: requires additional arguments")
        }
        FindError::NoTerminator(name) => {
            format!("find: {name}: missing argument to `;' command")
        }
        FindError::OkWithPlus => String::from("find: -ok ... + is not supported"),
        FindError::UnclosedParen => String::from("find: (` unmatched"),
        FindError::EmptyParens => String::from("find: expression requires non-empty parens"),
        FindError::StrayCloseParen(name) => format!("find: unexpected `)' near `{name}'"),
        FindError::NotWithoutOperand => String::from("find: `!' requires an expression"),
    };
    support::warn(format!("{msg}\n").as_bytes());
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
