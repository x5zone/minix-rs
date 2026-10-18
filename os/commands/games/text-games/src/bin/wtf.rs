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

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::acronym::{is_skipped_word, lookup_acronym};

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

    // 开档读档走 `support::slurp_operand` 的双生面（真机腿 `minix_sys::{open, read, close}`、
    // 宿主腿 `std::fs`）。改前这里直接写死 `minix_sys::open`，**宿主上任何真实存在的库档都报
    // `cannot open` 退 1**——与批次三百六十一给 `fortune` 治的是同一枚缺陷，只是本壳当时没有
    // 端到端门，所以一直没被发现。
    //
    // 顺手去掉的是本壳自创的 `{db_path}: read error` 那一句：这条缝按 C 与脚本共同的形状，
    // 把「读断」当作「读到尽头」（脚本那侧是 `fgrep … 2>/dev/null`，库读断只会退化成
    // 「查无此词」而不是另一句报文，见 `minix3/games/wtf/wtf:76-77`）。句子本身的齐平另立
    // `WTF-SCRIPT-PARITY`，本笔不动口径。
    let image = match support::slurp_operand(db_path) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(_)) => {
            support::warn(format!("wtf: {db_path}: cannot open\n").as_bytes());
            support::terminate(1);
        }
    };

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
