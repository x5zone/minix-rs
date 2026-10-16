//! Minix-RS col — filter reverse line feeds from input.
//!
//! Ground truth: `minix3/usr.bin/col/col.c` (NetBSD), usage
//! `col [-bfpx] [-l nline]` with no operands. The deciding half (the
//! line-assembly state machine and flush rules) is the library's
//! `col` module — including its option ring (`parse_col_flags`);
//! this program renders the four exits and runs the filter over stdin.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::col::{ColFlagError, filter, parse_col_flags};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let options = match parse_col_flags(&refs[1..]) {
        Ok(options) => options,
        Err(error) => flag_error(prog, error),
    };

    // 读缝走 `bin_support` 的双生面（宿主 `std::io::stdin`、真机 `read(STDIN)`）：
    // 本文件以前直接拿裸 `minix_sys::read`，**宿主上读不到任何输入**（同族第七例：
    // `nl`/`fold`/`rev`/`colrm` 的读腿、`split` 的写腿、`unexpand` 的读腿），
    // 所以 `col` 的过滤输出从来没有被进程级测过。
    let out = filter(&support::read_stdin(), &options);
    if out.warned {
        support::warn(b"col: warning: can't back up -- line already flushed\n");
    }
    support::emit(&out.bytes);
    support::terminate(0)
}

/// 四类出口各自的句子与退码，逐支照 C。
fn flag_error(prog: &str, error: ColFlagError<'_>) -> ! {
    match error {
        ColFlagError::OperandsPresent => usage(),
        // 集外字母与 `-l` 缺值：libc 那一行由 `getprogname()` 打头（真机是 `col`），
        // 随后才是 `col.c:154-156` 的 `usage()` —— 两行。
        ColFlagError::UnknownOption(letter) => {
            support::warn(GetoptReport::UnknownOption(letter).line(prog).as_bytes());
            usage()
        }
        ColFlagError::MissingValue(letter) => {
            support::warn(GetoptReport::MissingValue(letter).line(prog).as_bytes());
            usage()
        }
        // `col.c:145-150` 是裸 `fprintf(stderr, "col: bad -l argument %s.\n", optarg)`
        // 加 `exit`：程序名写死、带操作数原文、有个句点，而且**不接 usage**。
        // 旧写法少了操作数原文与句点，还在 `-l` 后面没有那一段时错走了这一支。
        ColFlagError::BadLineCount(value) => {
            support::warn(format!("col: bad -l argument {value}.\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// C 的 `usage()`（`col.c:520-526`）：程序名是**写死**的 `col`（不像 `rev` 那样跟着
/// `getprogname()`），一行、退 1。
fn usage() -> ! {
    support::warn(b"usage: col [-bfpx] [-l nline]\n");
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
