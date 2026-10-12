//! Minix-RS ls — list directory contents.
//!
//! Ground truth: `minix3/bin/ls/ls.c` (NetBSD): operands name files or
//! directories; directories are listed entry by entry, non-directory
//! operands print themselves, and `.` is the default operand. The record
//! walk rides `minix_sys::getdents` — NetBSD `struct dirent` records
//! (`d_fileno` u64@0, `d_reclen` u16@8, `d_namlen` u16@10, `d_type` u8@12,
//! name@13; `minix3/sys/sys/dirent.h`, layout encoded by the FS side's
//! `minix-fs/src/dentry.rs`).
//!
//! Wired here: one name per line (the `-1` shape — pinned divergence from
//! C's down-column default, which is a terminal-width layout this layer
//! does not model; `-1` is accepted as the no-op it is), `-a` (dot entries
//! included), `-A` (dot entries except `.`/`..`), `-F` unwired, and `-l`
//! rejected with an explicit message: the long form needs the full stat
//! face plus the passwd/group databases (C `ls.c:580-712`), which stay
//! with later stages — the same explicit-reject shape `pathchk` uses for
//! its unmodellable default mode. Multiple directory operands print a
//! `path:` header per directory (C `ls.c`'s operand printing). Names sort
//! byte-wise (C `strcoll` degrades to `strcmp` outside locales this layer
//! has).
//!
//! The hosted-versus-target split rides the crate's shared seams
//! (`../bin_support.rs`); the entry contract is documented once in
//! `echo.rs`, this crate's template binary.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::dents;

/// The getdents refill buffer (C `ls` uses a 8 KiB DirentBlock; half of
/// that covers every real name here).
const LIST_BUF: usize = 4096;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding is the testable `parse_ls_args` below: BSD
    // `getopt("1AaBbCcdFfghikLlMmnOoPpqRrSsTtuWwXx")` (`ls.c:154`). This build
    // models `-1`/`-a`/`-A`, reports `-l` (long-stat face unwired), and rejects
    // every other letter. The old whole-token matcher rejected modeled-flag
    // clusters (`ls -1a`, `ls -aA`) and mishandled `--` (skipped the token but
    // kept option-parsing after it, so `ls -- -a` wrongly set `-a`).
    let parsed = match parse_ls_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e),
    };
    let show_dot = parsed.show_dot;
    let almost_all = parsed.almost_all;
    let operands: Vec<String> = parsed.operands.iter().map(|s| String::from(*s)).collect();

    let mut status = 0;
    if operands.is_empty() {
        status = list_one(".", show_dot, almost_all, false);
        support::terminate(status);
    }
    let headers = operands.len() > 1;
    for (index, operand) in operands.iter().enumerate() {
        if headers && index > 0 {
            support::emit(b"\n");
        }
        if headers {
            support::emit(format!("{operand}:\n").as_bytes());
        }
        let result = list_one(operand, show_dot, almost_all, headers);
        if result != 0 {
            status = result;
        }
    }
    support::terminate(status)
}

/// Lists one operand: non-directories print themselves; directories walk
/// the getdents stream. A per-operand open error warns and returns 1 (C
/// keeps processing later operands, `ls.c:main`).
fn list_one(operand: &str, show_dot: bool, almost_all: bool, _header: bool) -> i32 {
    if !is_directory(operand) {
        support::emit(format!("{operand}\n").as_bytes());
        return 0;
    }
    let fd = match minix_sys::open(operand, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("ls: {operand}: cannot open\n").as_bytes());
            return 1;
        }
    };
    let mut names: Vec<String> = Vec::new();
    let mut buf = [0u8; LIST_BUF];
    loop {
        let transferred = match minix_sys::getdents(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                //[SD19-gate] SD-19: error path cleanup close
                #[allow(clippy::let_underscore_must_use)]
                let _ = minix_sys::close(fd);
                support::warn(format!("ls: {operand}: read error\n").as_bytes());
                return 1;
            }
        };
        // The record walk itself is the crate's shared decoder
        // (`minix_fileops::dents`, C libc `readdir`'s cursor rule); a record
        // whose lengths contradict the buffer ends the directory the way C's
        // readdir returning NULL does, rather than reading off the end.
        let mut cursor = 0;
        while cursor < transferred {
            let entry = match dents::next_record(&buf, cursor) {
                Ok(Some((entry, next))) => {
                    cursor = next;
                    entry
                }
                Ok(None) => break,
                Err(_) => break,
            };
            let name = String::from_utf8_lossy(entry.name).into_owned();
            if !dents::keep_in_listing(&name, show_dot, almost_all) {
                continue;
            }
            if entry.ino == 0 && name.starts_with('.') {
                continue;
            }
            names.push(name);
        }
    }
    //[SD19-gate] SD-19: close 后 fd 不再使用
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::close(fd);
    names.sort();
    for name in names {
        support::emit(format!("{name}\n").as_bytes());
    }
    0
}

/// Probes whether the operand is a directory: `stat` the path and read the
/// mode's type bits (C `d_type` from the getdents record can't answer for
/// a non-listed operand; `stat` is the authority — `minix-sys` top-level
/// `stat`, which `-l` will later reuse).
fn is_directory(path: &str) -> bool {
    let mut st: minix_sys::Stat = unsafe { core::mem::zeroed() };
    if minix_sys::stat(path, &mut st).is_err() {
        // Unreadable operands fail at open below with the same message
        // shape; treat as non-directory so `ls missing` prints the name
        // and the open error carries the failure.
        return false;
    }
    const S_IFMT: u32 = 0o170_000;
    const S_IFDIR: u32 = 0o040_000;
    st.st_mode & S_IFMT == S_IFDIR
}

/// Parsed `ls` argv: the dot-entry selection flags and file operands (a bare
/// `-` names a file literally, since `ls` opens operands through `fopen`).
#[derive(Debug, PartialEq, Eq)]
struct LsArgs<'a> {
    show_dot: bool,
    almost_all: bool,
    operands: Vec<&'a str>,
}

/// `ls` argument failure. Each arm maps to a distinct C error path.
#[derive(Debug, PartialEq, Eq)]
enum LsError {
    /// `-l` requested — needs the long-stat face (stat + passwd/group db),
    /// unwired in this build.
    LongStatUnwired,
    /// An option letter `ls` does not model (`getopt` `?`/default → usage).
    Usage,
}

/// Fold `ls`'s argv tail faithfully to BSD
/// `getopt("1AaBbCcdFfghikLlMmnOoPpqRrSsTtuWwXx")` (`ls.c:154`): option letters
/// cluster (`-1a` ≡ `-1 -a`), `--` ends option scanning, a bare `-` is an
/// operand. Only `-1` (single-column shape, a no-op here), `-a` (show dot
/// entries) and `-A` (dot entries except `.`/`..`) are modelled; `-l` is
/// reported and any other letter is a usage error.
fn parse_ls_args<'a>(args: &'a [&'a str]) -> Result<LsArgs<'a>, LsError> {
    let mut show_dot = false;
    let mut almost_all = false;
    let mut operands: Vec<&'a str> = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            for &letter in &bytes[1..] {
                match letter {
                    b'1' => {} // single-column shape: the default here, a no-op
                    b'a' => show_dot = true,
                    b'A' => almost_all = true,
                    b'l' => return Err(LsError::LongStatUnwired),
                    _ => return Err(LsError::Usage),
                }
            }
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    Ok(LsArgs {
        show_dot,
        almost_all,
        operands,
    })
}

/// Map an [`LsError`] to the message and exit code the old inline matcher used.
fn arg_error(e: LsError) -> ! {
    match e {
        LsError::LongStatUnwired => {
            support::warn(b"ls: -l waits on the long-stat face (stat + passwd/group db)\n");
            support::terminate(1);
        }
        LsError::Usage => {
            support::warn(b"usage: ls [-1aA] [file ...]\n");
            support::terminate(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_flags() {
        assert_eq!(parse_ls_args(&["-a"]).unwrap().show_dot, true);
        assert_eq!(parse_ls_args(&["-A"]).unwrap().almost_all, true);
        assert!(
            parse_ls_args(&["-1"]).unwrap()
                == LsArgs {
                    show_dot: false,
                    almost_all: false,
                    operands: vec![]
                }
        );
    }

    #[test]
    fn clustered_modeled_flags_accepted() {
        // 关键回归：旧整 token 匹配把合法聚簇 `-1a`/`-aA` 误拒。`-1` 为 no-op。
        let a = parse_ls_args(&["-1a", "f"]).unwrap();
        assert!(a.show_dot);
        assert_eq!(a.operands, vec!["f"]);
        let b = parse_ls_args(&["-aA", "f"]).unwrap();
        assert!(b.show_dot && b.almost_all);
    }

    #[test]
    fn dash_dash_ends_option_scanning() {
        // 关键回归：`--` 之后的 `-a` 是文件、不是选项（旧用 continue 误置 show_dot）。
        let a = parse_ls_args(&["--", "-a", "f"]).unwrap();
        assert!(!a.show_dot);
        assert_eq!(a.operands, vec!["-a", "f"]);
    }

    #[test]
    fn reports_long_stat_and_unknown() {
        assert_eq!(parse_ls_args(&["-l"]).err(), Some(LsError::LongStatUnwired));
        assert_eq!(
            parse_ls_args(&["-la"]).err(),
            Some(LsError::LongStatUnwired)
        );
        assert_eq!(
            parse_ls_args(&["-al"]).err(),
            Some(LsError::LongStatUnwired)
        );
        assert_eq!(parse_ls_args(&["-x", "f"]).err(), Some(LsError::Usage));
        // 裸 `-` 是操数。
        assert_eq!(parse_ls_args(&["-"]).unwrap().operands, vec!["-"]);
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
