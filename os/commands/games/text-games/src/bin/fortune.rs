//! Minix-RS fortune — the doing half over `minix_text_games::fortune`.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c`——格言库按行
//! 切分，抽取下标对库长取模（`pick_quip` 的决定半语义）。本壳读数据
//! 文件（缺省 `/usr/share/games/fortune`，操作数可指），抽取值取时钟
//! 秒（真机再混进程号），命中行打印。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::fortune::{SliceQuips, pick_quip};

const DEFAULT_DB: &str = "/usr/share/games/fortune";

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let path: &[u8] = args.get(1).copied().unwrap_or(DEFAULT_DB.as_bytes());

    // 开档失手单独报（C 的 `err(1, "Cannot open `%s'", ...)`），读档失手不报：
    // C 装载用的是 `fgets(...) != NULL`（`fortune.c:268`、`:293`），从不查 `ferror`，
    // 所以读断了和读到尽头是同一件事。缝见 `support::slurp_operand` 的双生面：
    // 真机腿落在 `minix_sys::{open, read, close}`，宿主腿落在 `std::fs`。句子沿用本壳
    // 已有的两句（对 C 的 `err(1, "Cannot open `%s'", ...)` 形状尚未齐平，那笔账在
    // `rewrite-notes/18-stage-commands/todo.md`），本轮只把两条臂接可达、不改口径
    // 开档口要 `&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`），折只发生在这一刻；
    // 报文里念出去的是原字节。
    let shown = String::from_utf8_lossy(path).into_owned();
    let image = match support::slurp_operand(&shown) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(code)) => {
            // C 的这一路是 `err(1, "Cannot open `%s'", fp->path)`（`fortune.c:1056`，另有
            // `:1067` 与 `:1116` 两条同一句式的 `.dat` 腿）：三段形，中间那位是「句子 + 带反引号
            // 与单引号包住的档名（原字节）」，尾巴是号对应的句子，退码 1。本壳的档名访问失败
            // 正是这一路（目录腿在 C 走 `warn`（`:525`、`:705`）不退场，本壳现在还不走目录，
            // 不在这一形里）。`err_line` 的标签口收 `&str`，这里自己拼。
            let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
            let mut line: Vec<u8> = prog.as_bytes().to_vec();
            line.extend_from_slice(b": Cannot open `");
            line.extend_from_slice(path);
            line.extend_from_slice(b"': ");
            line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
            line.push(b'\n');
            support::warn(&line);
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
            let mut line: Vec<u8> = b"fortune: ".to_vec();
            line.extend_from_slice(path);
            line.extend_from_slice(b": empty database\n");
            support::warn(&line);
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
