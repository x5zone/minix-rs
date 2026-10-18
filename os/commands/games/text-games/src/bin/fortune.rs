//! Minix-RS fortune — the doing half over `minix_text_games::fortune`.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c`——格言库按行
//! 切分，抽取下标对库长取模（`pick_quip` 的决定半语义）。本壳读数据
//! 文件（缺省 `/usr/share/games/fortune`，操作数可指），抽取值取时钟
//! 秒（真机再混进程号），命中行打印。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::fortune::{pick_quip, SliceQuips};

const DEFAULT_DB: &str = "/usr/share/games/fortune";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let path = args.get(1).copied().unwrap_or(DEFAULT_DB);

    // 开档与读档分两条臂各回各的失手，走的是 `support::slurp_operand` 的双生面：
    // 真机腿落在 `minix_sys::{open, read, close}`，宿主腿落在 `std::fs`。句子沿用本壳
    // 已有的两句（对 C 的 `err(1, "Cannot open `%s'", ...)` 形状尚未齐平，那笔账在
    // `rewrite-notes/18-stage-commands/todo.md`），本轮只把两条臂接可达、不改口径
    let image = match support::slurp_operand(path) {
        Ok(image) => image,
        Err(support::OperandFailure::Open) => {
            support::warn(format!("fortune: {path}: cannot open\n").as_bytes());
            support::terminate(1);
        }
        Err(support::OperandFailure::Read) => {
            support::warn(format!("fortune: {path}: read error\n").as_bytes());
            support::terminate(1);
        }
    };

    let text = String::from_utf8_lossy(&image).into_owned();
    let quips: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let draw = (support::epoch_micros().unwrap_or(0) / 1_000_000) as u32;
    let db = SliceQuips::new(&quips);
    match pick_quip(&db, draw) {
        Ok(quip) => {
            // C 的输出腿不查失手：`fputs(line, stdout)`（`minix3/games/fortune/fortune/fortune.c:272`）
            // 之后紧跟 `(void) fflush(stdout)`（`:274`），返回值被 `(void)` 丢掉，全文件没有
            // `ferror(stdout)` 这一判据（同目录 `strfile/strfile.c:255` 那句查的是它自己写出的
            // 数据档，属于另一枚工具）。⇒ 写不进去既不多一句话、也不改退码：号拿在手里不接，
            // 退码仍由下面的 `terminate(0)` 定，对应 C 的 `exit(0)`（`fortune.c:377`）
            let _ = support::emit_error(format!("{quip}\n").as_bytes());
        }
        Err(_) => {
            support::warn(format!("fortune: {path}: empty database\n").as_bytes());
            support::terminate(1);
        }
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
