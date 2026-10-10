//! Minix-RS `test` / `[` — conditional expression evaluation.
//!
//! Ground truth: `minix3/bin/test/test.c` (717 lines). The whole decision
//! logic (grammar, operators, precedence, C's `test_access` mode-bit rule)
//! lives in the library's `testexpr` module, where it is unit-tested
//! against in-memory file tables. This binary is the doing half: it reads
//! the operands, hands them to [`evaluate`], and answers through the
//! C exit-status contract — `0` when the expression is true, `1` when it
//! is false (`test.c:233` computes `res = !oexpr(...)`), and `2` when the
//! expression is malformed (`error`/`verrx(2)`, `test.c:180-189`).
//!
//! When invoked as `[`, the last operand must be the closing `]`, which
//! `test.c:223-227` strips after checking; a missing `]` is the exit-2
//! `missing ]` path. Bare `test` with no operands is false (`test.c:229`).
//!
//! The real [`FileTester`] answers the file-status questions from
//! `stat`/`lstat`, following C's split (`test.c:540`): `-L` reads the
//! `lstat` result (does not follow the final symlink), every other query
//! reads `stat` (follows it). Read/write/execute resolve through
//! [`access_bits`], which is C `test_access` verbatim — mode bits, not
//! `access(2)` — for the reasons the C comment at lines 373-494 gives.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::testexpr::{
    Access, FileKind, FileTester, ModeFlags, access_bits, evaluate,
};
use minix_sys::Stat;

// File-type bits (`minix3/sys/sys/stat.h:138-158`); `test.c:553-565` reads
// them through the `S_IS*` macros, which all mask `st_mode` against `_S_IFMT`.
const S_IFMT: u32 = 0o170_000;
const S_IFIFO: u32 = 0o010_000;
const S_IFCHR: u32 = 0o020_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFBLK: u32 = 0o060_000;
const S_IFREG: u32 = 0o100_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFSOCK: u32 = 0o140_000;
// Permission ornaments (`stat.h:111-112,145`), as `test.c:566-571` reads.
const S_ISUID: u32 = 0o004_000;
const S_ISGID: u32 = 0o002_000;
const S_ISVTX: u32 = 0o001_000;

/// The file-system tester: answers each question with a real `stat`
/// (`lstat` for `-L`), comparing against the caller's effective ids.
struct FsTester {
    euid: u32,
    egid: u32,
}

impl FsTester {
    /// Reads the caller's effective uid/gid once (C: `geteuid`, `getegid`).
    fn fresh() -> Self {
        FsTester {
            euid: minix_sys::geteuid().unwrap_or(0) as u32,
            egid: minix_sys::getegid().unwrap_or(0) as u32,
        }
    }

    /// `stat` the path (follows symlinks), the C authority for every query
    /// except `-L` (`test.c:540`).
    fn stat(&self, path: &str) -> Option<Stat> {
        let mut st: Stat = unsafe { core::mem::zeroed() };
        minix_sys::stat(path, &mut st).ok().map(|_| st)
    }

    /// `lstat` the path (does not follow the final symlink).
    fn lstat(&self, path: &str) -> Option<Stat> {
        let mut st: Stat = unsafe { core::mem::zeroed() };
        minix_sys::lstat(path, &mut st).ok().map(|_| st)
    }
}

impl FileTester for FsTester {
    fn exists(&self, path: &str) -> bool {
        self.stat(path).is_some()
    }

    fn kind(&self, path: &str) -> FileKind {
        let st = self.stat(path);
        let ls = self.lstat(path);
        let mode = st.map(|s| s.st_mode);
        FileKind {
            regular: mode.is_some_and(|m| m & S_IFMT == S_IFREG),
            directory: mode.is_some_and(|m| m & S_IFMT == S_IFDIR),
            // `-L` is the one flag that reads the `lstat` result.
            link: ls.is_some_and(|s| s.st_mode & S_IFMT == S_IFLNK),
            pipe: mode.is_some_and(|m| m & S_IFMT == S_IFIFO),
            socket: mode.is_some_and(|m| m & S_IFMT == S_IFSOCK),
            block: mode.is_some_and(|m| m & S_IFMT == S_IFBLK),
            char_dev: mode.is_some_and(|m| m & S_IFMT == S_IFCHR),
        }
    }

    fn access(&self, path: &str) -> Access {
        match self.stat(path) {
            Some(st) => access_bits(st.st_mode, st.st_uid, st.st_gid, self.euid, self.egid, &[]),
            None => Access::default(),
        }
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.stat(path).map(|st| st.st_size as u64)
    }

    fn mtime(&self, path: &str) -> Option<u64> {
        self.stat(path).map(|st| st.st_mtim.tv_sec as u64)
    }

    fn flags(&self, path: &str) -> ModeFlags {
        self.stat(path).map_or(ModeFlags::default(), |st| ModeFlags {
            setuid: st.st_mode & S_ISUID != 0,
            setgid: st.st_mode & S_ISGID != 0,
            sticky: st.st_mode & S_ISVTX != 0,
        })
    }

    fn owned_by_me(&self, path: &str) -> bool {
        self.stat(path).is_some_and(|st| st.st_uid == self.euid)
    }

    fn grouped_by_me(&self, path: &str) -> bool {
        self.stat(path).is_some_and(|st| st.st_gid == self.egid)
    }

    fn same_file(&self, a: &str, b: &str) -> bool {
        match (self.stat(a), self.stat(b)) {
            (Some(x), Some(y)) => x.st_dev == y.st_dev && x.st_ino == y.st_ino,
            _ => false,
        }
    }

    fn modified_since_read(&self, path: &str) -> bool {
        self.stat(path)
            .is_some_and(|st| st.st_mtim.tv_sec > st.st_atim.tv_sec)
    }
}

/// The whole program body; both `main` forms call it and it never returns.
/// The two-form entry contract is documented once in `echo.rs`, this crate's
/// template binary.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let program = argv.first().map(String::as_str).unwrap_or("test");
    let is_bracket = program == "[" || program.ends_with("/[");

    // Operands are everything after the program name.
    let mut words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    if is_bracket {
        // `[` requires the trailing `]` (`test.c:223-227`); dropping it makes
        // `[ ]` an empty (false) expression, matching C's `argc < 2` → 1.
        if words.last() != Some(&"]") {
            support::warn(b"[: missing ]\n");
            support::terminate(2);
        }
        words.pop();
    }

    let tester = FsTester::fresh();
    match evaluate(&tester, &words) {
        Ok(true) => support::terminate(0),
        Ok(false) => support::terminate(1),
        Err(_) => {
            // C renders the offending word then exits 2 (`syntax`,
            // `test.c:241-249`); the shape here mirrors `expr`'s diagnose path.
            support::warn(b"test: syntax error in expression\n");
            support::terminate(2)
        }
    }
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
