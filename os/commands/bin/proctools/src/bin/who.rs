//! Minix-RS who — the doing half over `minix_proctools::{stamp, utmp}`.
//!
//! Ground truth: `minix3/usr.bin/who/who.c` — an operand names the
//! database (`who utmp_file`, line 189); each active entry prints name,
//! line and the 12-column login stamp (`eprint` → `print`, lines
//! 310-318: `%.12s` of `ctime` from byte 4). The idle/state columns need
//! `stat` on the terminal lines and are declared unwired; stamps are UTC
//! (no time-zone database in this layer, see 12-process-tools.md §5).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_proctools::stamp::format_login_time;
use minix_proctools::utmp::walk_database;
use minix_sys::{open, read, Fd};

/// Minix3's classic database location; an operand overrides it
/// (`who.c:189`).
const DEFAULT_UTMP: &str = "/usr/adm/utmp";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    if args.len() > 2 {
        support::warn(b"usage: who [utmp_file]\n");
        support::terminate(1);
    }
    let path = args.get(1).copied().unwrap_or(DEFAULT_UTMP);

    let fd: Fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("who: {path}: cannot open\n").as_bytes());
            support::terminate(1);
        }
    };
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => {
                support::warn(format!("who: {path}: read error\n").as_bytes());
                support::terminate(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    // Printing inside the visit keeps the database bytes borrowed; logout
    // records (empty name) stay invisible, matching `who`'s active view.
    let walk = walk_database(&image, |entry| {
        if !entry.active() {
            return;
        }
        let mut stamp_buf = [0u8; 16];
        let stamp = match format_login_time(entry.time, &mut stamp_buf) {
            Ok(used) => String::from_utf8_lossy(&stamp_buf[..used]).into_owned(),
            Err(_) => "?".to_string(),
        };
        support::emit(format!("{:<8} {:<8} {}\n", entry.name, entry.line, stamp).as_bytes());
    });
    if walk.is_err() {
        support::warn(format!("who: {path}: corrupt database\n").as_bytes());
        support::terminate(1);
    }
    support::terminate(0)
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
