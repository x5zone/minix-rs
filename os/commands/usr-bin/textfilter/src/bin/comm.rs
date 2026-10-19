//! Minix-RS comm — three-column merge of two sorted streams.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 66-164: `-1`/`-2`/`-3` suppress a column, `-f` folds the comparison,
//! column two carries one tab and column three two, both streams drain
//! at end of input, and the status is 0 (usage and an unopenable file
//! leave 1). Exactly two operands are required; each `-` names standard
//! input. The merge itself is the library's tested `comm` module.
//!
//! 诊断面只有两处出口，但**行数不同**：集外字母是 libc 先印一行（`getopt.c:101-104`）
//! 再由 `comm.c:91-93` 的 `default: usage()` 接第二行；操作数不是两个（多、少都算）
//! 只有 `usage()` 那一行（`comm.c:101`）。optstring `"123f"` 里没有带值字母，所以
//! libc 的缺值那一句在本支根本到不了——可达出口由 optstring 定，不按同族规律补。
//! 打不开的档名是 `err(1, "%s", name)` 三段形且**致命**（`comm.c:181-184`）：与
//! `column`/`rev` 的 warn+继续不同，`comm` 开不了第一个档就直接退 1，第二个档名
//! 连试都不试。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::comm::{CommArgError, CommOptions, comm, parse_comm_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    // C 的选项环只有一条报错支：libc 先印一行，`usage()` 再印一行（退 1）。
    // 旧写法少了 libc 那一行，把两类失败都压成一行自撰的 usage。
    let (options, files): (CommOptions, [&str; 2]) = match parse_comm_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(CommArgError::UnknownOption(letter)) => {
            // 字母位走 `line_bytes`：取法已在 `src/comm.rs` 改成按字节，两边同形才成立。
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(prog));
            usage();
        }
        Err(CommArgError::NotTwoOperands) => usage(),
    };

    let lines1 = operand_lines(prog, files[0]);
    let lines2 = operand_lines(prog, files[1]);
    let a: Vec<&str> = lines1.iter().map(String::as_str).collect();
    let b: Vec<&str> = lines2.iter().map(String::as_str).collect();
    let merged = comm(&a, &b, &options);
    // 列前缀按 C 那样**只发给真正打出来的列**（`comm.c:105-110`：`p = tabs;` 然后
    // `if (flag1) col1 = *p++; if (flag2) col2 = *p++; if (flag3) col3 = *p;`）。
    // 所以压掉列一之后，原来的列二拿到的是 `tabs[0]`（无前缀）、原来的列三拿 `tabs[1]`，
    // 而不是按列号固定取 `tabs[column - 1]`——旧写法正是后者，`comm -1` 于是给
    // 公共行打了两个制表符。
    let tabs = ["", "\t", "\t\t"];
    let mut prefix = ["\t\t"; 4];
    let mut next = 0usize;
    for column in 1..=3u8 {
        let suppressed = match column {
            1 => options.suppress1,
            2 => options.suppress2,
            _ => options.suppress3,
        };
        if !suppressed {
            prefix[column as usize] = tabs[next];
            next += 1;
        }
    }
    for entry in &merged {
        let mut out = prefix[entry.column as usize].as_bytes().to_vec();
        out.extend_from_slice(entry.text.as_bytes());
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0)
}

/// Read one operand (`-` is standard input) and split it into newline-stripped
/// lines. An unopenable named file is fatal (C `file()` `err(1)`).
///
/// 末行没有换行符也照样是一条记录（`getnextln` 只在「EOF 且一个字符都没读到」时才
/// 返回 `NULL`，`comm.c:211-214`）——这一点与 `column` 相反，那边把残行当「长行」丢掉。
fn operand_lines(prog: &str, name: &str) -> Vec<String> {
    let input = if name == "-" {
        support::read_stdin()
    } else {
        match support::read_file(name) {
            Ok(data) => data,
            Err(code) => {
                // C 是 `err(1, "%s", name)`：程序名、档名、`strerror` 句子，一行，然后退 1。
                support::warn_line(prog, name, code);
                support::terminate(1);
            }
        }
    };
    split_lines(&input)
}

fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

/// C 的 `usage()`（`comm.c:195-199`）：一行、**程序名写死在串里**（不像 `column` 那样
/// 跟着 `getprogname()`）、退 1。
fn usage() -> ! {
    support::warn(b"usage: comm [-123f] file1 file2\n");
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
