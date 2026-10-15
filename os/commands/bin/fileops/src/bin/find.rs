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
//! - The parse-side error lines are byte-for-byte C's `errx(1, …)` text (they went
//!   through their own invented wording until this batch; see the `errx` sites in
//!   `option.c:131`/`:134`, `function.c:649`/`:668`, `operator.c:98`/`:108`/`:149`/`:199`),
//!   and each starts with `getprogname()` like C's `verrc.c`/`vwarnx.c` do. The one
//!   exception is `operator.c:98`, where C calls `err(1, …)` rather than `errx`:
//!   the errno there is whatever the last call left, so no number can be reproduced
//!   honestly and the line stops at the phrase — the same judgement `mkfifo`'s `-m`
//!   leg documents.
//! - `! -a` (and `! -and`) is *accepted* by C: `yanknode` hands back an `N_AND` node
//!   (`option.c:63` — `{ "-a", N_AND, c_null, 0 }`), which is neither `NULL` nor `N_OR`,
//!   so neither of `c_not`'s two refusals fires. This layer cannot produce a node for
//!   `-a` at all (C's `c_null` builds nothing that `!` could invert), so it refuses the
//!   shape at parse time with `!: no following expression`. Declared difference; matching
//!   C would mean giving `-a` a node form that can participate in `!` — registered in 06.
//! - Walk warnings now print the path plus `strerror(errno)` exactly as C does
//!   (`find.c:230-231`): the probe hands the number over since batch 274.
//!   One declared difference stays: C's `fts_build` loop treats a `readdir()`
//!   failure and end-of-directory the same (`fts.c:763`, `!= NULL`) and so says
//!   nothing when a directory cannot be read to the end, while this shell hands
//!   the number back and warns. Kept as-is (warning beats silence, and matching C
//!   here would need its own evidence round), registered in 07 篇.
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
use minix_sys::{
    Errno, Fd, Stat, close, getdents, gettimeofday, lstat, open, stat as stat_follows,
};

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
    /// `FTSENT::fts_statp` 的本体位：链本身（不跟随），供物理形与断链判定。
    /// 失败时直接把 `lstat` 的号交回走表腿（它就是 C 存进 `fts_errno` 的那个数）。
    fn lstat_of(&self, path: &str) -> Result<StatBits, Errno> {
        let mut sb = zeroed();
        lstat(path, &mut sb).map(|()| Self::bits(&sb))
    }

    /// 逻辑形（`-L`）与命令行跟随（`-H` 对起始路径）取目标位：`stat` 自然跟随符号链。
    fn stat_of(&self, path: &str) -> Result<StatBits, Errno> {
        let mut sb = zeroed();
        stat_follows(path, &mut sb).map(|()| Self::bits(&sb))
    }

    /// `readdir` 一次读完一个目次的全部条目（含 `.`/`..`，剔除 `d_ino == 0` 的空槽，
    /// `readdir.c:83-84`）。打不开 = C 的 `FTS_DNR`：`fts.c:675-677` 先存 `errno` 再置
    /// `FTS_DNR`，所以交回 `open` 的号就是 C 那一行 warn 的后半。
    /// 读错（`getdents` 失败）则是一处**声明差**，已改准：C 的循环 `while ((dp = readdir(dirp)) != NULL)`
    /// （`fts.c:763`）**不区分读错与读到尾**，那种情况下既不终止也不告警；本层把号交回走表腿
    /// 并报一行 warn（宁可多报不漏报）。把它改成 C 的静默需要自己的证据轮，不混进本批（登记在 07 篇）。
    fn dir_children(&self, dir: &str) -> Result<Vec<String>, Errno> {
        let fd: Fd = open(dir, O_RDONLY, 0)?;
        let mut names: Vec<String> = Vec::new();
        let mut buf = [0u8; WALK_BUF];
        let mut outcome: Result<(), Errno> = Ok(());
        loop {
            match getdents(fd, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut cursor = 0;
                    while cursor < n {
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
                Err(err) => {
                    outcome = Err(err);
                    break;
                }
            }
        }
        let _ = close(fd);
        outcome.map(|()| names)
    }
}

fn run() -> ! {
    let argv = support::args();
    // C never calls `setprogname` here either; `getprogname()` answers `argv[0]`'s
    // last segment because `crt0-common.c:156-165` seeds it from there, and every
    // `errx`/`err`/`usage` line below starts with it.
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().skip(1).map(|s| s.as_str()).collect();
    let parsed = match parse(&refs) {
        Ok(p) => p,
        Err(e) => {
            report_parse_error(prog, &e);
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
        // C 走的是 `warnx`，那一族在 `vwarnx.c` 里先打 `getprogname() + ": "`，所以
        // 走表告警也得缀程名——与上面解析错那一批 `errx` 行同口径（它们已经缀了）。
        // 决定半交回的是 `<路径>: <句子>`（C 交给 `warnx` 的那两个参数），前缀在此补上。
        support::warn(format!("{prog}: {warn}\n").as_bytes());
    }
    support::terminate(result.rval as i32)
}

/// `FindError` 逐支落到 C 的那几行报文——全是 `errx(1, …)` 形：程序名、短语、**不带**
/// 句子（C 没查 `errno`）。每支的原文坐标写在行上。
fn report_parse_error(prog: &str, e: &FindError<'_>) {
    let msg = match e {
        // `main.c:159-163`：`fprintf(stderr, "Usage: %s ...", getprogname())`——首字母
        // 大写，程序名嵌在行里。
        FindError::Usage => {
            format!("Usage: {prog} [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]")
        }
        // `option.c:131`：`errx(1, "%s: unknown option", *argv)`——名字在前，无反引号。
        FindError::UnknownOption(name) => format!("{prog}: {name}: unknown option"),
        // `option.c:134`：`errx(1, "%s: requires additional arguments", *--argv)`。
        FindError::NeedsArguments(name) => {
            format!("{prog}: {name}: requires additional arguments")
        }
        // `function.c:649-651`：`errx(1, "%s: no terminating \";\" or \"+\"", ...)`。
        FindError::NoTerminator(name) => {
            format!("{prog}: {name}: no terminating \";\" or \"+\"")
        }
        // `function.c:668`：`errx(1, "-ok: terminating \"+\" not permitted.")`——句末有句点。
        FindError::OkWithPlus => format!("{prog}: -ok: terminating \"+\" not permitted."),
        // `operator.c:96-98`：C 在这里用的是 `err` 而不是 `errx`，缀的是上一个调用留下的
        // `errno`（与 `mkfifo` 的 `-m` 腿同判）——无号可复刻，故止于短语。
        FindError::UnclosedParen => format!("{prog}: (: missing closing ')'"),
        // `operator.c:106-108`：`errx(1, "(): empty inner expression")`。
        FindError::EmptyParens => format!("{prog}: (): empty inner expression"),
        // `operator.c:149`：`errx(1, "): no beginning '('")`——C 的那行不带出错的那个词，
        // 所以本层把 `StrayCloseParen` 里的名字放下，不另造 C 没有的字段。
        FindError::StrayCloseParen(_) => format!("{prog}: ): no beginning '('"),
        // `operator.c:197-199`：`errx(1, "!: no following expression")`。
        FindError::NotWithoutOperand => format!("{prog}: !: no following expression"),
        // `operator.c:200-201`：连续 `!` 消合并后 yank 到的是 `N_OR` —— C 把这一形与上一形
        // 分作两句，本层现在也分得出（`parse_primary` 在 `!` 位上看到 `-o`/`-or` 即拒）。
        FindError::NotBeforeOr => format!("{prog}: !: nothing between ! and -o"),
        // `operator.c:253-255` 与 `:257-259`：`-o` 的左空与右空也是两句，且 C 有先后。
        FindError::OrWithoutLeft => format!("{prog}: -o: no expression before -o"),
        FindError::OrWithoutRight => format!("{prog}: -o: no expression after -o"),
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
