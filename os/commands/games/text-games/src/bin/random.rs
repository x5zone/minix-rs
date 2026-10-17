//! Minix-RS random — the doing half over `minix_text_games::lottery`.
//!
//! Ground truth: `minix3/games/random/random.c`. `random` 是一条过滤器：把标准输入
//! 的每一行独立掷一次，掷中的**整行原样写出去**（`selected` 的判据在 `:126`，
//! 逐字符 `putchar` 的循环在 `:127-137`）。分母来自操作数：零枚取 2、一枚取
//! `strtod(*argv, &ep)`、两枚以上 usage（`:89-104`）。播种混入时钟与进程号
//! （`mix_seed` 的决定半语义，`:106-107`）。
//!
//! 操作数那一步与过滤器都已按 C 成形。仍敞着的是选项环（`-e` 把一个 `[0, denom]`
//! 里的数当退码返回、`-r` 关掉 stdio 缓冲、集外字母那两行）与 `strtod` 那片取值域
//! （`1e3`、`1e-3`、`inf`、`nan`、`--` 之后才算操作数的起点），账在
//! `rewrite-notes/18-stage-commands/todo.md` 的 `RANDOM-DENOM-LEGS` 条目里（片2b 与片3）。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
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

    let (bytes, read_failure) = support::read_stdin_error();
    if let Some(code) = read_failure {
        // `ferror(stdin)` 那一路是 `err(2, "stdin")`（`random.c:135-136`）：三段形，位上
        // 是 `stdin` 这个词而不是档名（C 的 `ifd` 初值就是 0，标准输入与命名档走同一条
        // `read()` 腿），退码是 2。薄壳这里原来打的是一句自创的 `random: read error` 退 1
        support::warn_line(prog, "stdin", code);
        support::terminate(2);
    }
    // C 是一趟过流（`random.c:126-137`）：掷中就把这一行整行写出去，下一行另掷一次。
    // 一行都没掷中不是错误——环走完就 `exit(0)`，参照件那一形是空输出、退 0
    let text = String::from_utf8_lossy(&bytes).into_owned();
    for line in text.lines() {
        // LCG（Numerical Recipes 常数），取模前先掷高位。C 掷的是 libc 的 `random()`，
        // 位序列不复现（这条承载差登记在本支的账上），判据 `1/denom` 照 C
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let draw = ((state >> 33) as u32) % denominator;
        if matches!(is_selected(draw, denominator), Ok(true)) {
            support::emit(format!("{line}\n").as_bytes());
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
