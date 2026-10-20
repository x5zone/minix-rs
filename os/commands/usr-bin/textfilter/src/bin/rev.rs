//! Minix-RS rev — reverse each line's characters.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 59-109:
//! file operands or stdin, every line's characters print in reverse with
//! a newline, and an unopenable file warns and is skipped (exit status stays
//! nonzero, non-fatal). `rev` takes no options (C `getopt("")` at `:70-75`)——
//! 空 optstring 不等于「没有选项环」：`-Z` 仍由 libc 报一行集外字母、后接 `usage()`，
//! 而裸 `-` 与 `--` 之后的 token 都落回操作数。The
//! character-reversal walk (multibyte kept intact) is the library's tested
//! `reversed_lines`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::rev::{parse_rev_args, reversed_lines};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // 程序名那一腿的渲染口仍是 `&str`（`GetoptReport::line_bytes`、`warn_line` 都收 `&str`），
    // 库侧字节口在册未动：`ARGV-BYTE-PATH-IN-LIBS`。字母与操作数这两腿从这里开始是原字节。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C 的选项环只有一条出口：拿到 `'?'` 就 `usage()`（`rev.c:72-74`），而那一行
    // `unknown option -- <字母>` 是 libc 在返回之前自己印的（`getopt.c:101-104`）。
    // 旧写法没有选项环，`-Z` 被当成文件操作数，句子也少了 `strerror` 那一段。
    // 走字节形 `line_bytes`：libc 那一行的字母是一个裸字节（`optopt = *place++`），
    // 参照件现量 `rev -é` 打 `c3` 一个字节，按 `char` 渲染会多出一个字节。
    let operands = match parse_rev_args(&refs[1..]) {
        Ok(rest) => rest,
        Err(report) => {
            support::warn(&report.line_bytes(&prog));
            usage(&prog);
        }
    };

    // C `do { if (*argv) fopen or warn+continue; while(fgetwln reverse) } while(*argv)`
    // （`rev.c:83-106`）：打不开的档是 `warn(3)` 而**不是致命错**，跳过它继续下一个，
    // 退出码置 1。
    let mut status = 0;
    if operands.is_empty() {
        // 读缝走 `bin_support` 的双生面（宿主 `std::io::stdin`、真机 `read(STDIN)`）：
        // 本文件以前直接拿裸 `minix_sys::read`，**宿主上读不到任何输入**，所以新立的
        // 端到端门一跑就现形（同批次三百零六的 `nl`、三百零八的 `fold`）。
        //
        // 读失败不等于文件末尾：C 在行环之后试 `ferror` 并报**流名**，标准输入那一形
        // 报的是初值 `"stdin"`（`rev.c:80-81`、`:101-104`）。已经读到的部分先照做
        // （C 在读到时就把它打印掉了），然后一行 `warn`、退码置 1。
        let (input, failure) = support::read_stdin_error();
        emit_reversed(&input);
        if let Some(code) = failure {
            support::warn_line(&prog, "stdin", code);
            status = 1;
        }
        support::terminate(status);
    }
    for operand in operands {
        // 开档与报文回显这两条腿的口都收 `&str`（`os/libs/minix-sys/src/lib.rs:fn open`、
        // `os/libs/minix-types/src/types/errno.rs:fn err_line`），字节形操作数只能在
        // 这里折一次——差在 `ARGV-BYTE-PATH-IN-LIBS` 里在册。
        let path = String::from_utf8_lossy(operand).into_owned();
        let input = match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // C 的 `warn("%s", *argv)`（`rev.c:86`）是 err(3) 三段形：程序名、操作数、
                // `strerror` 句子。
                support::warn_line(&prog, &path, code);
                status = 1;
                continue;
            }
        };
        emit_reversed(&input);
    }
    support::terminate(status)
}

/// Emit each reversed line with its newline.
fn emit_reversed(input: &[u8]) {
    for line in reversed_lines(input) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
}

/// C 的 `usage()`（`rev.c:110-115`）：一行、程序名跟着 `getprogname()`、
/// `exit(EXIT_FAILURE)` 即退 1。
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [file ...]\n").as_bytes());
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
