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

use minix_doctools::whatis::{ManDb, SliceManDb};
use minix_sys::{open, read, Fd};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_path = "/usr/man/whatis";
    let mut names: Vec<&str> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        match args[at] {
            "-M" => {
                at += 1;
                db_path = args.get(at).copied().unwrap_or_else(|| {
                    eprintln!("whatis: -M needs a path");
                    std::process::exit(1);
                });
            }
            other => names.push(other),
        }
        at += 1;
    }
    if names.is_empty() {
        eprintln!("usage: whatis [-M db] name ...");
        std::process::exit(1);
    }

    let fd: Fd = match open(db_path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            eprintln!("whatis: {db_path}: cannot open");
            std::process::exit(1);
        }
    };
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => {
                eprintln!("whatis: {db_path}: read error");
                std::process::exit(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    let text = String::from_utf8_lossy(&image).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        // 空库：每个名字都查无（EmptyManDb 的诚实面）。
        eprintln!("whatis: {db_path}: nothing appropriate");
        std::process::exit(1);
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
                println!(
                    "{}({}) - {}",
                    entry.name_list().join(", "),
                    entry.section,
                    entry.description
                );
            }
            None => {
                eprintln!("whatis: {}: nothing appropriate", name);
                status = 1;
            }
        }
    }
    std::process::exit(status)
}
