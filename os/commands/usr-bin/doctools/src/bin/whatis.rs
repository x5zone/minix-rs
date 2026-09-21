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

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_doctools::whatis::{ManDb, SliceManDb};
use minix_sys::{open, read, Fd};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_path = "/usr/man/whatis";
    let mut names: Vec<&str> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        match args[at] {
            "-M" => {
                at += 1;
                db_path = args.get(at).copied().unwrap_or_else(|| {
                    support::warn(b"whatis: -M needs a path\n");
                    support::terminate(1);
                });
            }
            other => names.push(other),
        }
        at += 1;
    }
    if names.is_empty() {
        support::warn(b"usage: whatis [-M db] name ...\n");
        support::terminate(1);
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
        // 空库：每个名字都查无（EmptyManDb 的诚实面）。
        support::warn(format!("whatis: {db_path}: nothing appropriate\n").as_bytes());
        support::terminate(1);
    }
    // 生命周期：库行借 `text`，查询循环在同段借用内完成。
    query(SliceManDb { lines: &lines }, &names);
}

fn query(db: SliceManDb, names: &[&str]) -> ! {
    let mut status = 0;
    for name in names {
        match db.lookup_exact(name) {
            Some(entry) => {
                // 经典行式 `name, name(section) - description`——条目不存
                // 原文行，按解析域重组（空白已归一，语义同行）。
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


#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
