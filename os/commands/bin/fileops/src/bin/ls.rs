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

/// The getdents refill buffer (C `ls` uses a 8 KiB DirentBlock; half of
/// that covers every real name here).
const LIST_BUF: usize = 4096;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut show_dot = false;
    let mut almost_all = false;
    let mut operands: Vec<String> = Vec::new();
    for arg in argv.iter().skip(1) {
        match arg.as_str() {
            "-1" => {}
            "-a" => show_dot = true,
            "-A" => almost_all = true,
            "--" => continue,
            _ if arg.starts_with('-') && arg.len() > 1 && arg != "-" => {
                if arg.contains('l') {
                    support::warn(b"ls: -l waits on the long-stat face (stat + passwd/group db)\n");
                    support::terminate(1);
                }
                support::warn(b"usage: ls [-1aA] [file ...]\n");
                support::terminate(1);
            }
            _ => operands.push(arg.clone()),
        }
    }

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
                let _ = minix_sys::close(fd);
                support::warn(format!("ls: {operand}: read error\n").as_bytes());
                return 1;
            }
        };
        let mut cursor = 0;
        while cursor < transferred {
            let record = &buf[cursor..transferred];
            if record.len() < 13 {
                break;
            }
            let reclen = u16::from_ne_bytes([record[8], record[9]]) as usize;
            if reclen == 0 || cursor + reclen > transferred {
                break;
            }
            let ino = u64::from_ne_bytes(record[0..8].try_into().expect("8 bytes"));
            let namlen = u16::from_ne_bytes([record[10], record[11]]) as usize;
            let name = String::from_utf8_lossy(&record[13..13 + namlen]).into_owned();
            let keep = if name.starts_with('.') {
                show_dot || (almost_all && name != "." && name != "..")
            } else {
                true
            };
            if keep && !(ino == 0 && name.starts_with('.')) {
                names.push(name);
            }
            cursor += reclen;
        }
    }
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

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
