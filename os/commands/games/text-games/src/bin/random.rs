//! Minix-RS random — the doing half over `minix_text_games::lottery`.
//!
//! Ground truth: `minix3/games/random/random.c`. `random` 是一条过滤器：把标准输入
//! 的每一行独立掷一次，掷中的**整行原样写出去**（`selected` 的判据在 `:126`，
//! 逐字符 `putchar` 的循环在 `:127-137`）。分母来自操作数：零枚取 2、一枚取
//! `strtod(*argv, &ep)`、两枚以上 usage（`:89-104`）。播种混入时钟与进程号
//! （`mix_seed` 的决定半语义，`:106-107`）。
//!
//! 本壳当前只把「操作数那一步」接到 C 的形状上；过滤体的多行输出、`-e`/`-r` 两个
//! 选项、以及 `strtod` 那一片取值域（`1e3`、`1e-3`、`inf`、`nan`）各归各的下一轮，
//! 登记在 `rewrite-notes/18-stage-commands/todo.md` 的 `RANDOM-DENOM-LEGS` 条目里。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::lottery::{
    is_selected, judge_operands, mix_seed, OperandLeg, NOT_VALID_SENTENCE, NO_OPERAND_DENOMINATOR,
    USAGE,
};
use minix_text_games::TextGameError;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    // C 的 `argc -= optind; switch (argc)`（`random.c:86-104`）。本支的选项环还没接上，
    // 所以操作数就是 `argv[1..]`；判完个数才轮到读输入。
    let operands: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let denominator = match judge_operands(&operands) {
        OperandLeg::NoOperandDefault => NO_OPERAND_DENOMINATOR,
        OperandLeg::Accepted(value) => value,
        OperandLeg::TooMany => {
            // `usage()` 那一行不带程序名前缀，`random` 是写死在串里的（`random.c:143-147`）
            support::warn(USAGE.as_bytes());
            support::terminate(1);
        }
        OperandLeg::NotValid => {
            // `errx(1, "denominator is not valid.")`（`:98-99`）：前缀跟着 `argv[0]`，
            // 但操作数不在句子里——`errx` 没有 `%s` 位
            support::warn(format!("{prog}: {NOT_VALID_SENTENCE}\n").as_bytes());
            support::terminate(1);
        }
        OperandLeg::OutOfRange => {
            // `err(1, "%s", *argv)`（`:96-97`）：三段形，位上是操作数，尾巴是号对应的句子
            support::warn_line(
                prog,
                operands[0],
                minix_sys::Errno::from_i32(TextGameError::OutOfRange.as_errno()),
            );
            support::terminate(1);
        }
    };

    let micros = support::epoch_micros().unwrap_or(0);
    let pid = minix_sys::getpid().unwrap_or(0) as u32;
    let mut state = mix_seed(micros / 1_000_000, (micros % 1_000_000) as u32, pid);

    // 逐行读标准输入：每行掷一次 [0, N)，掷中 0 即替换当前选中行。
    let mut chosen: Option<String> = None;
    let mut buf = Vec::new();
    let (bytes, read_failure) = support::read_stdin_error();
    if read_failure.is_some() {
        // 句子与退码还不是 C 的形状（C 是 `err(2, "stdin")`，`:135-136`）：那一腿归过滤体
        // 那一片，本笔只把读缝换成带号的双生面，让宿主也能跑到这条路径
        support::warn(b"random: read error\n");
        support::terminate(1);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    for line in text.lines() {
        // LCG（Numerical Recipes 常数），取模前先掷高位。
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let draw = ((state >> 33) as u32) % denominator;
        buf.clear();
        buf.extend_from_slice(line.as_bytes());
        match is_selected(draw, denominator) {
            Ok(true) => chosen = Some(line.to_string()),
            Ok(false) => {}
            Err(_) => {}
        }
    }
    match chosen {
        Some(line) => support::emit(format!("{line}\n").as_bytes()),
        None => {
            support::warn(b"random: nothing selected\n");
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
