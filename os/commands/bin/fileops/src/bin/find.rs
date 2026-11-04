//! Minix-RS `find` — the doing half over `minix_fileops::find`.
//!
//! Ground truth: `minix3/usr.bin/find/` (NetBSD lineage). The deciding half holds the
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
//! - The parse-side error lines are byte-for-byte C's `errx(1, …)` text (they used
//!   invented wording before; see the `errx` sites in
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
//! - Walk warnings print the path plus `strerror(errno)` exactly as C does
//!   (`find.c:230-231`): the probe hands the number over.
//!   One declared difference stays: C's `fts_build` loop treats a `readdir()`
//!   failure and end-of-directory the same (`fts.c:763`, `!= NULL`) and so says
//!   nothing when a directory cannot be read to the end, while this shell hands
//!   the number back and warns. Kept as-is (warning beats silence, and matching C
//!   here would need its own evidence round), registered in doc 07.
//! - A hard error while evaluating one entry (an unwired action, a malformed
//!   numeric argument) records the failure and moves on rather than calling
//!   `err(1)` and killing the whole walk. C exits the process (`find.c:269`,
//!   the `errx` sites in `function.c`); matching du's declared "a `getdents`
//!   error only counts the operand, does not terminate the run".
//! - `-L`/`-H` (logical traversal: follow symlinks) are parsed into the walk
//!   bits but this shell traverses physically regardless — resolving a symlink
//!   chain and its cycle detection needs full fts symlink handling, registered
//!   as a residual in `rewrite-notes/18-stage-commands/todo.md`.
//! - SIGINFO progress reporting (`find.c:156-183`) needs signal registration
//!   this tree does not have on the command side; omitted, as in `sleep`.
//!
//! Real-machine effect is not yet gated: `find` is read-only, but seeding it
//! into the image and the serial assertion belong to E5, and the root
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
use minix_proctools::passwd;
use minix_shell::exec_run::exec_search;
use minix_sys::{
    Errno, Fd, Stat, Strerror, close, fork, getdents, gettimeofday, lstat, open, read, rmdir,
    stat as stat_follows, unlink, waitpid,
};

/// The per-call refill buffer for directory reads (the same level as `du`):
/// the size is not observable; it only bounds how many records arrive per
/// `getdents` call.
const WALK_BUF: usize = 4096;

fn zeroed() -> Stat {
    // SAFETY: `Stat` is a byte struct of plain integer fields; all-zero is a
    // valid bit pattern, and a successful `stat`/`lstat` overwrites it
    // wholesale.
    unsafe { core::mem::zeroed() }
}

/// The three ground-truth legs of the walk and of `-newer`: `lstat` (type,
/// time, device), `getdents` (directory children), and `stat` (follow the
/// symlink chain for the reference file's times). All go through `minix_sys`;
/// the deciding half never touches system calls. The account/group seams
/// read C `_PATH_PASSWD`/`_PATH_GROUP` directly (no db machinery).
struct SysProbe;

/// C `_PATH_PASSWD` / the group database path.
const PASSWD_PATH: &str = "/etc/passwd";
const GROUP_PATH: &str = "/etc/group";
const DB_BUF: usize = 4096;

/// Read a whole database file into text; `None` = the file is unavailable
/// (the same face as an absent entry).
fn read_db(path: &str) -> Option<String> {
    let fd = open(path, 0, 0).ok()?;
    let mut text = String::new();
    let mut chunk = [0u8; DB_BUF];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => text.push_str(&String::from_utf8_lossy(&chunk[..n])),
            Err(_) => {
                let _ = close(fd);
                return None;
            }
        }
    }
    let _ = close(fd);
    Some(text)
}

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
    /// The reference file's times for `-newer`: C uses `stat` (following
    /// symlinks), and failing to obtain them is `err(1)` in `c_newer`. This
    /// layer returns `None`, and the evaluator reports the
    /// `<name>: illegal numeric value` arm from that (see the difference note).
    fn times_of(&self, path: &str) -> Option<(i64, i64, i64)> {
        let mut sb = zeroed();
        stat_follows(path, &mut sb)
            .ok()
            .map(|()| (sb.st_atim.tv_sec, sb.st_mtim.tv_sec, sb.st_ctim.tv_sec))
    }
    fn account_by_name(&self, name: &str) -> Option<u32> {
        read_db(PASSWD_PATH).and_then(|db| passwd::lookup_in(&db, name).map(|a| a.uid))
    }
    fn uid_known(&self, uid: u32) -> bool {
        read_db(PASSWD_PATH)
            .map(|db| passwd::lookup_uid_in(&db, uid).is_some())
            .unwrap_or(false)
    }
    fn group_by_name(&self, name: &str) -> Option<u32> {
        read_db(GROUP_PATH).and_then(|db| passwd::lookup_group_in(&db, name).map(|g| g.gid))
    }
    fn gid_known(&self, gid: u32) -> bool {
        read_db(GROUP_PATH)
            .map(|db| passwd::lookup_gid_in(&db, gid).is_some())
            .unwrap_or(false)
    }
    fn unlink_entry(&self, path: &str) -> Result<(), Errno> {
        unlink(path)
    }
    fn rmdir_entry(&self, path: &str) -> Result<(), Errno> {
        rmdir(path)
    }

    /// C `f_exec`'s run half (`function.c:550-565`): fork + `execvp` +
    /// `waitpid`. The child's working directory is already find's own (this
    /// walk never `fchdir`s, so C's `dotfd` restore has no counterpart);
    /// `execvp`'s PATH search goes through the shell crate's shared
    /// `exec_search`, with the child's inherited environment (C passes
    /// `environ` through). The exec-failure child prints C's
    /// `warn("%s", plan->e_argv[0])` line and exits 1
    /// (`function.c:559-561`); a signal death answers C's `rval` `-1`.
    fn run_utility(&self, argv: &[&str]) -> Result<i32, Errno> {
        match fork() {
            Ok(0) => {
                let envp: Vec<String> = support::envs()
                    .iter()
                    .map(|raw| String::from_utf8_lossy(raw).into_owned())
                    .collect();
                let refs: Vec<&str> = argv.to_vec();
                let env: Vec<&str> = envp.iter().map(|s| s.as_str()).collect();
                let path = env.iter().find(|e| e.starts_with("PATH=")).map(|e| &e[5..]);
                let errno = exec_search(argv[0], &refs, &env, path);
                // `warn("%s", plan->e_argv[0])` (`function.c:559-561`):
                // `vwarn` renders `find: <name>: <strerror sentence>`.
                support::warn(
                    format!("find: {}: {}\n", argv[0], Strerror(Errno::from_i32(errno))).as_bytes(),
                );
                support::terminate(1);
            }
            Ok(pid) => {
                let mut raw = 0;
                waitpid(pid, &mut raw, 0).map(|_| {
                    // C's `rval` shape (`function.c:598-601`): `WEXITSTATUS`
                    // when exited, `-1` when the child died by signal.
                    let signal = raw & 0x7F;
                    if signal == 0 { (raw >> 8) & 0xFF } else { -1 }
                })
            }
            Err(e) => Err(e),
        }
    }

    /// C `queryuser` (`misc.c:99-117`): `"argv …"? ` on standard error, the
    /// first answer byte from standard input, drain to end of line, and a
    /// newline of our own when the answer ended without one.
    fn query_ok(&self, argv: &[&str]) -> bool {
        let mut line = String::from("\"");
        if let Some(first) = argv.first() {
            line.push_str(first);
        }
        for arg in argv.iter().skip(1) {
            line.push(' ');
            line.push_str(arg);
        }
        line.push_str("\"? ");
        support::warn(line.as_bytes());
        let mut buf = [0u8; 1];
        let mut first = None;
        let mut nl = false;
        while let Ok(1) = read(support::STDIN, &mut buf) {
            let ch = buf[0];
            if first.is_none() {
                first = Some(ch);
            }
            if ch == b'\n' {
                nl = true;
                break;
            }
        }
        if !nl {
            support::warn(b"\n");
        }
        first == Some(b'y')
    }
}

impl WalkProbe for SysProbe {
    /// The body position of `FTSENT::fts_statp`: the link itself (not
    /// followed), for the physical shape and broken-link decisions. On failure
    /// it hands `lstat`'s number straight back to the walk leg (that is the
    /// number C stores into `fts_errno`).
    fn lstat_of(&self, path: &str) -> Result<StatBits, Errno> {
        let mut sb = zeroed();
        lstat(path, &mut sb).map(|()| Self::bits(&sb))
    }

    /// The logical shape (`-L`) and command-line following (`-H` for the
    /// starting paths) take the target's bits: `stat` follows symlinks
    /// naturally.
    fn stat_of(&self, path: &str) -> Result<StatBits, Errno> {
        let mut sb = zeroed();
        stat_follows(path, &mut sb).map(|()| Self::bits(&sb))
    }

    /// `readdir` reads every entry of one directory in one go (including
    /// `.`/`..`, dropping empty slots with `d_ino == 0`, `readdir.c:83-84`).
    /// An open failure is C's `FTS_DNR`: `fts.c:675-677` stores `errno` first
    /// and then sets `FTS_DNR`, so handing back `open`'s number is the second
    /// half of C's warn line.
    /// A read error (`getdents` failure) is a **declared difference**, already
    /// settled: C's loop `while ((dp = readdir(dirp)) != NULL)`
    /// (`fts.c:763`) **does not distinguish a read error from
    /// end-of-directory**, and in that case neither stops nor warns; this
    /// layer hands the number back to the walk leg and prints one warn line
    /// (better to over-report than to miss). Matching C's silence would need
    /// its own evidence round and does not ride this batch (registered in
    /// doc 07).
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
        // When the clock is unavailable, degrade to 0: only the time-based
        // predicates (`-atime` etc.) are affected, and compared with C's
        // `time(&now)` this is "counted from the epoch" -- an honest
        // degradation without a clock source.
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
        // C goes through `warnx`, whose family prints `getprogname() + ": "`
        // first in `vwarnx.c`, so walk warnings must carry the program name
        // too -- the same convention as the parse-error `errx` lines above
        // (which already carry it). The deciding half hands back
        // `<path>: <sentence>` (the two arguments C passes to `warnx`), and the
        // prefix is added here.
        support::warn(format!("{prog}: {warn}\n").as_bytes());
    }
    support::terminate(result.rval as i32)
}

/// Each `FindError` arm lands on one of C's message lines -- all in
/// `errx(1, …)` shape: program name, phrase, and **no** errno sentence (C does
/// not check `errno`). Each arm's source coordinates are given on its line.
fn report_parse_error(prog: &str, e: &FindError<'_>) {
    let msg = match e {
        // `main.c:159-163`: `fprintf(stderr, "Usage: %s ...", getprogname())`
        // -- capital first letter, program name embedded in the line.
        FindError::Usage => {
            format!("Usage: {prog} [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]")
        }
        // `option.c:131`: `errx(1, "%s: unknown option", *argv)` -- the name
        // comes first, no backticks.
        FindError::UnknownOption(name) => format!("{prog}: {name}: unknown option"),
        // `option.c:134`: `errx(1, "%s: requires additional arguments", *--argv)`.
        FindError::NeedsArguments(name) => {
            format!("{prog}: {name}: requires additional arguments")
        }
        // `function.c:649-651`: `errx(1, "%s: no terminating \";\" or \"+\"", ...)`.
        FindError::NoTerminator(name) => {
            format!("{prog}: {name}: no terminating \";\" or \"+\"")
        }
        // `function.c:1588-1591`: `errx(1, "regexp %s: %s", regexp, errbuf)`
        // — the sentence stands in for C's `regerror` text (this engine
        // answers two shapes; registered divergence).
        FindError::Regex(pattern, sentence) => {
            format!("{prog}: regexp {pattern}: {sentence}")
        }
        // `function.c:668`: `errx(1, "-ok: terminating \"+\" not permitted.")` --
        // note the period at the end.
        FindError::OkWithPlus => format!("{prog}: -ok: terminating \"+\" not permitted."),
        // `operator.c:96-98`: C uses `err` here rather than `errx`, so it
        // attaches the `errno` left by the previous call (the same judgement as
        // `mkfifo`'s `-m` leg) -- no number can be reproduced, so the line
        // stops at the phrase.
        FindError::UnclosedParen => format!("{prog}: (: missing closing ')'"),
        // `operator.c:106-108`: `errx(1, "(): empty inner expression")`.
        FindError::EmptyParens => format!("{prog}: (): empty inner expression"),
        // `operator.c:149`: `errx(1, "): no beginning '('")` -- C's line does
        // not carry the offending word, so this layer drops the name inside
        // `StrayCloseParen` instead of inventing a field C does not have.
        FindError::StrayCloseParen(_) => format!("{prog}: ): no beginning '('"),
        // `operator.c:197-199`: `errx(1, "!: no following expression")`.
        FindError::NotWithoutOperand => format!("{prog}: !: no following expression"),
        // `operator.c:200-201`: after consecutive `!` are folded away, yank
        // lands on `N_OR` -- C splits this shape from the previous one, and
        // this layer now tells them apart too (`parse_primary` refuses when it
        // sees `-o`/`-or` in the `!` position).
        FindError::NotBeforeOr => format!("{prog}: !: nothing between ! and -o"),
        // `operator.c:253-255` and `:257-259`: the left-empty and right-empty
        // `-o` are also two sentences, and C orders them.
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
