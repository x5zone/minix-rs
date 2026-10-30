//! Minix-RS whatis — the doing half over `minix_doctools::whatis`.
//!
//! Ground truth: the query face of `minix3/usr.bin/whatis/whatis.c` — a
//! name is looked up exactly in the `whatis` database and the matching
//! rows print. The NetBSD binary reads the mandoc-built database; this
//! shell reads the classic text database the deciding half models
//! (`SliceManDb`), one operand per query, exit 0 when every name matched
//! and 1 when any missed. The database default is the classic
//! `/usr/man/whatis`; the builder (`makewhatis`) is the declared gap of
//! 10-doc-man-tools.md §3.4.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_doctools::whatis::{parse_whatis_args, ManDb, SliceManDb, WhatisArgError, WhatisArgs};
use minix_sys::{open, read, Fd};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let WhatisArgs { db_path, names } = match parse_whatis_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(WhatisArgError::Usage) => usage(),
    };
    let db_path = db_path.unwrap_or("/usr/man/whatis");
    if names.is_empty() {
        usage();
    }

    let fd: Fd = match open(db_path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("whatis: {db_path}: cannot open\n").as_bytes());
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
                support::warn(format!("whatis: {db_path}: read error\n").as_bytes());
                support::terminate(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    let text = String::from_utf8_lossy(&image).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        // Empty database: every name misses (the honest face of
        // EmptyManDb).
        support::warn(format!("whatis: {db_path}: nothing appropriate\n").as_bytes());
        support::terminate(1);
    }
    // Lifetime: the database rows borrow `text`; the query loop finishes
    // inside that same borrow.
    query(SliceManDb { lines: &lines }, &names);
}

fn query(db: SliceManDb, names: &[&str]) -> ! {
    let mut status = 0;
    for name in names {
        match db.lookup_exact(name) {
            Some(entry) => {
                // Classic line form `name, name(section) - description` —
                // an entry does not keep its source line, so the line is
                // rebuilt from the parsed fields (whitespace already
                // normalized, same semantics as the line).
                support::emit(
                    format!(
                        "{}({}) - {}\n",
                        entry.name_list().join(", "),
                        entry.section,
                        entry.description
                    )
                    .as_bytes(),
                );
            }
            None => {
                support::warn(format!("whatis: {}: nothing appropriate\n", name).as_bytes());
                status = 1;
            }
        }
    }
    support::terminate(status)
}

fn usage() -> ! {
    support::warn(b"usage: whatis [-C file] [-M path] [-m path] command ...\n");
    support::terminate(1);
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
