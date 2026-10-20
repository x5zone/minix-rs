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
//!
//! 命令行按**字节**走到这里（[`bin_support::args_bytes`]）：`-s`/`-t` 的值就是打出去的
//! 内容，`-f` 的值与操作数就是报文里的内容，所以这三处只要经过文本缝就会改到输出与
//! 回显的字节（现量 `tmp/argv-443/measure.log`：参照件 `seq -s<ff> 1 3` 打
//! `1\xff2\xff3\xff`，折叠形打的是 `ef bf bd` 三位；同一枚 `-\xff` 在折叠形里是三个字母
//! 各报一行，在字节形里是一个字母一行）。唯一仍折一次的是 `OutOfRange` 那一行的操作数
//! 位——那一句要借库侧的 `strerror`，而 `strerror` 的口收 `&str`，账在
//! `ARGV-BYTE-PATH-IN-LIBS`。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{Errno, ERANGE};
use minix_textfilter::seq::{generate, parse, render_output, SeqError};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let options = match parse(&words) {
        Ok(options) => options,
        Err(error) => fail(&prog, &error),
    };
    let values = match generate(&options) {
        Ok(values) => values,
        Err(error) => fail(&prog, &error),
    };
    support::emit(&render_output(&values, &options));
    support::terminate(0);
}

/// 三家句子各走各的出口与退码；libc 那几行交回 [`minix_textfilter::GetoptReport`] 单源。
fn fail(prog: &str, error: &SeqError) -> ! {
    match error {
        SeqError::Usage { reports } => {
            for report in reports {
                // 字母位走 `line_bytes`：`getopt.c:79` 的 `optopt = *place++` 一次进一个
                // 字节，而 `seq` 的环是**累加形**（`errflg` 累加、不在第一个坏字母处停，
                // `seq.c:101-113`），所以命令行上几枚字节就交回几行、每行一个裸字节。
                support::warn(&report.line_bytes(prog));
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
            support::warn(&sentence(
                prog,
                b"invalid floating point argument: ",
                word,
                b"",
            ));
            support::terminate(2);
        }
        SeqError::OutOfRange(word) => {
            // `err(2, "%s", num)` 的三段形带 `strerror`，那一行只能借库侧的 `&str` 口
            let word = String::from_utf8_lossy(word);
            support::warn_line(prog, &word, Errno::from_i32(ERANGE));
            support::terminate(2);
        }
        SeqError::ZeroIncrement => give_up(prog, "zero increment"),
        SeqError::ZeroDecrement => give_up(prog, "zero decrement"),
        SeqError::NeedsPositiveIncrement => give_up(prog, "needs positive increment"),
        SeqError::NeedsNegativeDecrement => give_up(prog, "needs negative decrement"),
        SeqError::BadFormat(fmt) => {
            support::warn(&sentence(prog, b"invalid format string: `", fmt, b"'"));
            support::terminate(1);
        }
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

/// 「程序名 + 句子 + **原字节** + 后随字符 + 换行」那一族：句子与后随字符都是 ASCII，
/// 唯一不是的是被 `%s` 交回的那个值，所以它按字节拼而不能经 `format!` 的 `{}`。
fn sentence(prog: &str, lead: &[u8], word: &[u8], trail: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(prog.as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(lead);
    out.extend_from_slice(word);
    out.extend_from_slice(trail);
    out.push(b'\n');
    out
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
