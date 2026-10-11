//! Minix-RS `users` — the doing half over `minix_proctools::{utmp, users}`.
//!
//! Ground truth: `minix3/usr.bin/users/users.c`. It reads the utmp logins
//! and prints the distinct logged-in names, sorted, as one space-separated
//! line. The name-list sort/unique transform lives in
//! [`minix_proctools::users::sorted_unique`]; this program opens the utmp
//! database (`getutentries`' default path), collects the active logins, and
//! prints them. Operands are ignored (C parses `getopt(argc, argv, "")`, so a
//! leading `-x` is a usage error but extra operands are simply dropped).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::users::sorted_unique;
use minix_proctools::utmp::walk_database;
use minix_sys::{Fd, close, open, read};

#[path = "../bin_support.rs"]
mod support;

/// C `users` reads the default utmp database (`getutentries(NULL, …)`).
const DEFAULT_UTMP: &str = "/usr/adm/utmp";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // getopt(argc, argv, "") — the first non-option ends parsing; a leading
    // option like `-x` reaches getopt's `?`/default and prints usage (users.c:66-69).
    if let Some(first) = argv.get(1)
        && first.starts_with('-')
        && first.len() >= 2
        && first != "--"
    {
        support::warn(b"usage: users\n");
        support::terminate(1);
    }

    // Read the database. A missing file is not an error in C — getutentries
    // yields nothing and `users` exits 0 — so mirror that here.
    let fd: Fd = match open(DEFAULT_UTMP, 0, 0) {
        Ok(fd) => fd,
        Err(_) => support::terminate(0),
    };
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => {
                let _ = close(fd);
                support::terminate(0);
            }
        }
    }
    let _ = close(fd);

    // Collect the active logins' names (empty name == logged out).
    let mut names: Vec<&str> = Vec::new();
    let walk = walk_database(&image, |entry| {
        if entry.active() {
            names.push(entry.name);
        }
    });
    if walk.is_err() {
        support::warn(b"users: corrupt database\n");
        support::terminate(1);
    }

    let count = sorted_unique(names.as_mut_slice());
    if count == 0 {
        // users.c: ncnt == 0 -> return 0 (no output).
        support::terminate(0);
    }

    let mut out: Vec<u8> = Vec::new();
    for (i, name) in names[..count].iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(name.as_bytes());
    }
    out.push(b'\n');
    support::emit(&out);
    support::terminate(0);
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
