//! Minix-RS wtf — the doing half over `minix_text_games::acronym`.
//!
//! Ground truth: `minix3/games/wtf/wtf`（`-f` 指库、`-o` 只认全字、
//! 命令行里的 `is` 词跳过；库行是 `term: expansion`）。决定半
//! （跳过词、行解析、大小写不敏感首中查询）在库内；本壳拼参数、读库、
//! 逐词打印 `term: expansion`，任一词查无即退出 1。
//!
//! 库默认 `/usr/share/misc/acronyms`（NetBSD wtf 的缺省路径）。


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::acronym::{is_skipped_word, lookup_acronym};
use minix_sys::{open, read, Fd};

const DEFAULT_DB: &str = "/usr/share/misc/acronyms";


fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_path = DEFAULT_DB;
    let mut terms: Vec<&str> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        match args[at] {
            "-f" => {
                at += 1;
                db_path = args.get(at).copied().unwrap_or_else(|| {
                    support::warn(b"wtf: -f needs a database path\n");
                    support::terminate(1);
                });
            }
            "-o" => {
                // 只认全字的旗标：决定半的查询本就是全字匹配，语义已含。
            }
            "is" if !terms.is_empty() || at == 2 => {
                // `wtf is LOL` 的 is 词跳过（用法约定，库面 `is_skipped_word`）。
            }
            other => terms.push(other),
        }
        at += 1;
    }
    if terms.is_empty() {
        support::warn(b"usage: wtf [-f db] [is] term ...\n");
        support::terminate(1);
    }

    let fd: Fd = match open(db_path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("wtf: {db_path}: cannot open\n").as_bytes());
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
                support::warn(format!("wtf: {db_path}: read error\n").as_bytes());
                support::terminate(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    let text = String::from_utf8_lossy(&image).into_owned();
    let lines: Vec<&str> = text.lines().collect();

    let mut status = 0;
    for term in &terms {
        let word = if is_skipped_word(term) && terms.len() > 1 {
            // 单个 is 词不是查询对象；查询词序列里混入的 is 也跳过。
            continue;
        } else {
            term
        };
        match lookup_acronym(&lines, word) {
            Ok(expansion) => support::emit(format!("{word}: {expansion}\n").as_bytes()),
            Err(_) => {
                support::warn(format!("wtf: {word}: nothing appropriate\n").as_bytes());
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
