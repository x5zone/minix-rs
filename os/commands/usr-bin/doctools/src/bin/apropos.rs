//! Minix-RS apropos — the keyword face of the manual database.
//!
//! Ground truth: `minix3/usr.bin/whatis/whatis.c` 的 apropos 半（按关键
//! 词扫库，命中的行原样打印，一行未中即退出 1）。本壳走决定半的
//! `for_each_matching`（大小写折叠的行内匹配），库面与缺库行为同
//! `whatis`——经典文本库，`makewhatis` 构建面是 10 篇 §3.4 的声明留白。

use minix_doctools::whatis::SliceManDb;
use minix_sys::{open, read, Fd};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_path = "/usr/man/whatis";
    let mut keywords: Vec<&str> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        match args[at] {
            "-M" => {
                at += 1;
                db_path = args.get(at).copied().unwrap_or_else(|| {
                    eprintln!("apropos: -M needs a path");
                    std::process::exit(1);
                });
            }
            other => keywords.push(other),
        }
        at += 1;
    }
    if keywords.is_empty() {
        eprintln!("usage: apropos [-M db] keyword ...");
        std::process::exit(1);
    }

    let fd: Fd = match open(db_path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            eprintln!("apropos: {db_path}: cannot open");
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
                eprintln!("apropos: {db_path}: read error");
                std::process::exit(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    let text = String::from_utf8_lossy(&image).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        std::process::exit(1);
    }
    let db = SliceManDb { lines: &lines };
    let mut hits = 0;
    for keyword in keywords {
        hits += db.for_each_matching(keyword, |line| println!("{line}"));
    }
    std::process::exit(if hits > 0 { 0 } else { 1 });
}
