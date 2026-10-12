//! Minix-RS apropos — the keyword face of the manual database.
//!
//! Ground truth: `minix3/usr.bin/whatis/whatis.c` 的 apropos 半（按关键
//! 词扫库，命中的行原样打印，一行未中即退出 1）。本壳走决定半的
//! `for_each_matching`（大小写折叠的行内匹配），库面与缺库行为同
//! `whatis`——经典文本库，`makewhatis` 构建面是 10 篇 §3.4 的声明留白。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_doctools::whatis::{parse_apropos_args, AproposArgs, SliceManDb, WhatisArgError};
use minix_sys::{open, read, Fd};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let AproposArgs { db_path, keywords } = match parse_apropos_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(WhatisArgError::Usage) => usage(),
    };
    let db_path = db_path.unwrap_or("/usr/man/whatis");
    if keywords.is_empty() {
        usage();
    }

    let fd: Fd = match open(db_path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("apropos: {db_path}: cannot open\n").as_bytes());
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
                support::warn(format!("apropos: {db_path}: read error\n").as_bytes());
                support::terminate(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    let text = String::from_utf8_lossy(&image).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        support::terminate(1);
    }
    let db = SliceManDb { lines: &lines };
    let mut hits = 0;
    for keyword in keywords {
        hits += db.for_each_matching(keyword, |line| support::emit(format!("{line}\n").as_bytes()));
    }
    support::terminate(if hits > 0 { 0 } else { 1 });
}

fn usage() -> ! {
    support::warn(
        b"usage: apropos [-C file] [-M path] [-m path] [-S arch] [-s section] keyword ...\n",
    );
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
