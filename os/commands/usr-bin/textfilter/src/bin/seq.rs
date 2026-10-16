//! Minix-RS seq — number sequences.
//!
//! Ground truth: `minix3/usr.bin/seq/seq.c` (NetBSD). The deciding half
//! (operand shapes, the direction-following default increment, the
//! wrong-direction errors) is the library's `seq` module; this program
//! generates the numbers and prints them with the separator after each
//! one and the terminator after the last (seq.c:174-186). The floating
//! generation and `-f` formats of the C are declared unsupported by the
//! engine (the same float-rendering adjudication as printf's).
//!
//! 前门有**三家句子、两个退码**，都按 C 逐句搬回：
//! ① `errflg` 一族退 1，形如「libc 的零到多行 + **一行** usage」——`-h`、操作数
//!    不是 1..3 个都只有 usage 那一行（`h` 在 optstring 里，libc 无话可说；C 的
//!    `case 'h'` 后面没有 `break`，直接落 `default: errflg++`，`seq.c:120-122`），
//!    而集外字母与带值字母缺值会先各印一行 libc（环**不在第一个坏字母处停**，
//!    `seq -+- 3` 因此有两行 libc）。usage 那行跟着 `getprogname()`（`seq.c:133-136`）。
//! ② 命令自己的 `errx(1, ...)` 一族退 1，各**一行**且不接 usage：
//!    `zero increment`/`zero decrement`（名字随 `first < last` 变）、
//!    `needs positive increment`、`needs negative decrement`、
//!    `invalid format string: \`%q'`（带原文，反引号加单引号是 C 的原样措辞）。
//! ③ 操作数一族走 `e_atof()`，退 **2**：尾巴有杂字符是
//!    `invalid floating point argument: 3x`（`errx`），溢出/下溢是
//!    `1e310: Numerical result out of range`（`err` 的三段形，带 `strerror`）。
//!    C 解析操作数的次序是 `last`→`first`→`incr`，所以 `seq 3 -w` 报的是 `-w`。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{ERANGE, Errno};
use minix_textfilter::seq::{SeqError, generate, parse, render_output};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let options = match parse(&words) {
        Ok(options) => options,
        Err(error) => fail(prog, &error),
    };
    let values = match generate(&options) {
        Ok(values) => values,
        Err(error) => fail(prog, &error),
    };
    support::emit(render_output(&values, &options).as_bytes());
    support::terminate(0);
}

/// 三家句子各走各的出口与退码；libc 那几行交回 [`minix_textfilter::GetoptReport`] 单源。
fn fail(prog: &str, error: &SeqError) -> ! {
    match error {
        SeqError::Usage { reports } => {
            for report in reports {
                support::warn(report.line(prog).as_bytes());
            }
            support::warn(
                format!(
                    "usage: {prog} [-w] [-f format] [-s string] [-t string] [first [incr]] last\n"
                )
                .as_bytes(),
            );
            support::terminate(1);
        }
        SeqError::InvalidNumber(word) => {
            support::warn(format!("{prog}: invalid floating point argument: {word}\n").as_bytes());
            support::terminate(2);
        }
        SeqError::OutOfRange(word) => {
            support::warn_line(prog, word, Errno::from_i32(ERANGE));
            support::terminate(2);
        }
        SeqError::ZeroIncrement => give_up(prog, "zero increment"),
        SeqError::ZeroDecrement => give_up(prog, "zero decrement"),
        SeqError::NeedsPositiveIncrement => give_up(prog, "needs positive increment"),
        SeqError::NeedsNegativeDecrement => give_up(prog, "needs negative decrement"),
        SeqError::BadFormat(fmt) => give_up(prog, &format!("invalid format string: `{fmt}'")),
        SeqError::BadFormatPlain => give_up(prog, "invalid format string"),
        // 与 printf 家同一处裁决：说明符本身合法之后按 `%g`/`%.2f` 渲染浮点还没接上。
        // C 在这一形照常打数，本壳交回一行自认的说明并退 1，登记为 SEQ-FLOAT-RENDER。
        SeqError::FloatNotModelled => {
            support::warn(format!("{prog}: -f formats wait for the float renderer\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// `errx(1, ...)`：一行、带程序名、不接 usage、退 1。
fn give_up(prog: &str, message: &str) -> ! {
    support::warn(format!("{prog}: {message}\n").as_bytes());
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
