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
    let long_format = parsed.long_format;
    let operands: Vec<String> = parsed.operands.iter().map(|s| String::from(*s)).collect();

    // The long-stat face's name service: C's `NAMES` fills `user`/`group`
    // through `user_from_uid`/`group_from_gid` (`ls.c:574-590`), each
    // answering the numeric string when the account is unknown. The two
    // databases are read once per run.
    let (users, groups) = if long_format {
        (read_db(PASSWD_PATH), read_db(GROUP_PATH))
    } else {
        (None, None)
    };
    // C `printtime`'s freshness window anchors at `time(NULL)` taken once
    // per printlong call (print.c:129).
    let now = match minix_sys::gettimeofday() {
        Ok((sec, _usec)) => sec,
        Err(_) => 0,
    };

    let mut status = 0;
    if operands.is_empty() {
        status = list_one(
            ".",
            show_dot,
            almost_all,
            long_format,
            users.as_deref(),
            groups.as_deref(),
            now,
        );
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
        let result = list_one(
            operand,
            show_dot,
            almost_all,
            long_format,
            users.as_deref(),
            groups.as_deref(),
            now,
        );
        if result != 0 {
            status = result;
        }
    }
    support::terminate(status)
}

/// C `_PATH_PASSWD` / the group database path.
const PASSWD_PATH: &str = "/etc/passwd";
const GROUP_PATH: &str = "/etc/group";

/// Read a whole database file into text; `None` = the file is unavailable
/// (the same face as an absent account — C's `user_from_uid` then answers
/// the numeric fallback).
fn read_db(path: &str) -> Option<String> {
    let fd = minix_sys::open(path, 0, 0).ok()?;
    let mut text = String::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => text.push_str(&String::from_utf8_lossy(&chunk[..n])),
            Err(_) => {
                //[SD19-gate] close on the error path
                #[allow(clippy::let_underscore_must_use)]
                let _ = minix_sys::close(fd);
                return None;
            }
        }
    }
    //[SD19-gate] the fd is no longer used after close
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::close(fd);
    Some(text)
}

/// Lists one operand: non-directories print themselves; directories walk
/// the getdents stream. A per-operand open error warns and returns 1 (C
/// keeps processing later operands, `ls.c:main`).
fn list_one(
    operand: &str,
    show_dot: bool,
    almost_all: bool,
    long_format: bool,
    users: Option<&str>,
    groups: Option<&str>,
    now: i64,
) -> i32 {
    if !is_directory(operand) {
        if long_format {
            // C `display`/`printlong` for a non-directory operand: one long
            // line for the file itself, no total (the entry sits at
            // FTS_ROOTLEVEL, so `printtotal` is skipped, print.c:131).
            let mut sb = zeroed_stat();
            match minix_sys::lstat(operand, &mut sb) {
                Ok(()) => {
                    let line = render_long_line(&sb, operand, users, groups, now);
                    support::emit(line.as_bytes());
                }
                Err(_) => {
                    support::warn(format!("ls: {operand}: cannot stat\n").as_bytes());
                    return 1;
                }
            }
            return 0;
        }
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
                //[SD19-gate] close on the error path
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
    //[SD19-gate] the fd is no longer used after close
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::close(fd);
    names.sort();
    if !long_format {
        for name in names {
            support::emit(format!("{name}\n").as_bytes());
        }
        return 0;
    }
    // The long-stat face: lstat every entry, compute the column widths over
    // the listing (C `display`'s sizing loop, ls.c:525-580), print the
    // `total` line, then one long line per entry.
    let mut rows: Vec<(String, minix_sys::Stat)> = Vec::new();
    for name in names {
        let path = join_path(operand, &name);
        let mut sb = zeroed_stat();
        match minix_sys::lstat(&path, &mut sb) {
            Ok(()) => rows.push((path, sb)),
            Err(_) => {
                // C's fts NS arm warns and skips the entry (ls.c:230-241).
                support::warn(format!("ls: {path}: cannot stat\n").as_bytes());
            }
        }
    }
    if rows.is_empty() {
        return 0;
    }
    // `btotal += sp->st_blocks` (ls.c:576); the default blocksize is 512,
    // the unit `st_blocks` already counts, so the total is the raw sum.
    let mut btotal: u64 = 0;
    for (_, sb) in &rows {
        btotal += sb.st_blocks.max(0) as u64;
    }
    // The NAMES are computed once; each row reuses them.
    let names_of: Vec<(String, String)> = rows
        .iter()
        .map(|(_, sb)| (user_name(users, sb.st_uid), group_name(groups, sb.st_gid)))
        .collect();
    // `printtotal` (`print.c:172-186`): the root start's own listing is the
    // one place without it — every directory listing prints it.
    support::emit(format!("total {btotal}\n").as_bytes());
    for ((path, sb), (user, group)) in rows.iter().zip(names_of.iter()) {
        let line = render_long_line_with(sb, path, user, group, now);
        support::emit(line.as_bytes());
    }
    0
}

/// Zeroed `Stat` (the same SAFETY shape as find's `zeroed`).
fn zeroed_stat() -> minix_sys::Stat {
    // SAFETY: `Stat` is a byte struct of plain integer fields; all-zero is a
    // valid bit pattern, and a successful `lstat` overwrites it wholesale.
    unsafe { core::mem::zeroed() }
}

/// C `major(x)` (`sys/sys/types.h:290`).
fn major(x: u64) -> u64 {
    (x & 0x000f_ff00) >> 8
}

/// C `minor(x)` (`sys/sys/types.h:291`).
fn minor(x: u64) -> u64 {
    ((x & 0xfff0_0000) >> 12) | (x & 0x0000_00ff)
}

/// `user_from_uid(uid, 0)` (`pwcache.c`): the account name, or the numeric
/// string — the noname=0 shape always answers something printable.
fn user_name(users: Option<&str>, uid: u32) -> String {
    users
        .and_then(|db| minix_proctools::passwd::lookup_uid_in(db, uid))
        .map(|a| String::from(a.name))
        .unwrap_or_else(|| {
            // C `snprintf(nuser, "%u", sp->st_uid)` (`ls.c:584-585`); the
            // alloc crate carries no Display for u32 in no_std, so the
            // digits go in by hand.
            let mut text = [0u8; 10];
            let mut used = 0;
            let mut v = uid;
            loop {
                text[used] = b'0' + (v % 10) as u8;
                v /= 10;
                used += 1;
                if v == 0 {
                    break;
                }
            }
            let mut out = String::new();
            for b in text[..used].iter().rev() {
                out.push(*b as char);
            }
            out
        })
}

/// `group_from_gid(gid, 0)`'s numeric-fallback shape over the group db.
fn group_name(groups: Option<&str>, gid: u32) -> String {
    groups
        .and_then(|db| minix_proctools::passwd::lookup_gid_in(db, gid))
        .map(|g| String::from(g.name))
        .unwrap_or_else(|| {
            let mut text = [0u8; 10];
            let mut used = 0;
            let mut v = gid;
            loop {
                text[used] = b'0' + (v % 10) as u8;
                v /= 10;
                used += 1;
                if v == 0 {
                    break;
                }
            }
            let mut out = String::new();
            for b in text[..used].iter().rev() {
                out.push(*b as char);
            }
            out
        })
}

/// `path/name` with the `.` root staying prefix-free (`./a`, not `..a`).
fn join_path(root: &str, name: &str) -> String {
    if root == "." {
        String::from(name)
    } else if root.ends_with('/') {
        format!("{root}{name}")
    } else {
        format!("{root}/{name}")
    }
}

/// C `printlong`'s default-flag line (`print.c:123-198`): `strmode`,
/// nlink, user, group, size (or `major, minor` for character/block
/// devices), the mtime window, and the path. `precomputed` carries the
/// already-named user/group when the caller sized the columns; `None`
/// names them on the fly (the single-operand face).
fn render_long_line(
    sb: &minix_sys::Stat,
    path: &str,
    users: Option<&str>,
    groups: Option<&str>,
    now: i64,
) -> String {
    let user = user_name(users, sb.st_uid);
    let group = group_name(groups, sb.st_gid);
    render_long_line_with(sb, path, &user, &group, now)
}

/// The row renderer over already-named accounts (the listing face keeps
/// C's default column widths; the sizing loop is a registered simplifier).
fn render_long_line_with(
    sb: &minix_sys::Stat,
    path: &str,
    user: &str,
    group: &str,
    now: i64,
) -> String {
    // `ModeString` is a plain `[u8; 11]` alias (`mode.rs:317`); printlong
    // prints the ten permission characters and keeps the trailing space C's
    // format string adds between the mode and the nlink column.
    let mode_text =
        String::from_utf8_lossy(&minix_fileops::mode::strmode(sb.st_mode)[..10]).into_owned();
    let is_device = sb.st_mode & 0o170000 == 0o020000 || sb.st_mode & 0o170000 == 0o060000;
    let size_text = if is_device {
        format!("{:>3}, {:>3}", major(sb.st_rdev), minor(sb.st_rdev))
    } else {
        format!("{:>8}", sb.st_size)
    };
    let time_text = render_time(sb.st_mtim.tv_sec, now);
    format!(
        "{} {:>3} {:<8}  {:<8}  {} {} {}\n",
        mode_text, sb.st_nlink, user, group, size_text, time_text, path
    )
}

/// C `printtime` (`print.c:327-352`): ctime's byte window — `mon dd ` from
/// bytes 4-10, then `HH:MM` (bytes 11-16) when the stamp sits within
/// ±SIXMONTHS of now, else ` yyyy` (a space plus bytes 20-24). UTC: this
/// build carries no time-zone database (the 12-process-tools declared
/// shape), so `dates::breakdown`'s fields stand in for ctime's local text.
fn render_time(ftime: i64, now: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const SIXMONTHS: i64 = (365 / 2) * 86_400;
    let broken = minix_proctools::dates::breakdown(ftime);
    let month = MONTHS
        .get((broken.month as usize).saturating_sub(1))
        .copied()
        .unwrap_or("???");
    let head = format!("{} {:>2} ", month, broken.day);
    let tail = if ftime + SIXMONTHS > now && ftime - SIXMONTHS < now {
        format!("{:02}:{:02}", broken.hour, broken.minute)
    } else {
        format!(" {:>4}", broken.year)
    };
    format!("{head}{tail} ")
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
    long_format: bool,
    operands: Vec<&'a str>,
}

/// `ls` argument failure. Each arm maps to a distinct C error path.
#[derive(Debug, PartialEq, Eq)]
enum LsError {
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
    let mut long_format = false;
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
                    b'l' => long_format = true,
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
        long_format,
        operands,
    })
}

/// Map an [`LsError`] to the message and exit code the old inline matcher used.
fn arg_error(e: LsError) -> ! {
    match e {
        LsError::Usage => {
            support::warn(b"usage: ls [-1aA] [file ...]\n");
            support::terminate(1);
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
                    long_format: false,
                    operands: vec![]
                }
        );
    }

    #[test]
    fn clustered_modeled_flags_accepted() {
        // Key regression: the old whole-token matcher wrongly rejected legal
        // clusters `-1a`/`-aA`. `-1` is a no-op.
        let a = parse_ls_args(&["-1a", "f"]).unwrap();
        assert!(a.show_dot);
        assert_eq!(a.operands, vec!["f"]);
        let b = parse_ls_args(&["-aA", "f"]).unwrap();
        assert!(b.show_dot && b.almost_all);
    }

    #[test]
    fn dash_dash_ends_option_scanning() {
        // Key regression: after `--`, `-a` is a file, not an option (the old
        // code used `continue` and wrongly set show_dot).
        let a = parse_ls_args(&["--", "-a", "f"]).unwrap();
        assert!(!a.show_dot);
        assert_eq!(a.operands, vec!["-a", "f"]);
    }

    #[test]
    fn parses_long_format_and_reports_unknown() {
        // `-l` now takes the long-stat face (this round's wiring).
        assert!(parse_ls_args(&["-l"]).unwrap().long_format);
        assert!(parse_ls_args(&["-la"]).unwrap().long_format);
        assert!(parse_ls_args(&["-al"]).unwrap().long_format);
        assert_eq!(parse_ls_args(&["-x", "f"]).err(), Some(LsError::Usage));
        // A bare `-` is an operand.
        assert_eq!(parse_ls_args(&["-"]).unwrap().operands, vec!["-"]);
    }
}

