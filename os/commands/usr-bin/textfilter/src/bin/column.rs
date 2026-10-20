//! Minix-RS column — columnate lines into a table.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). The
//! deciding half (entry collection, the two columnate layouts, and the
//! `-t` table alignment) is the library's `column` module; this program
//! reads stdin or file operands, lays the entries out, and prints. With
//! file operands each is opened in turn and fed through one shared entry
//! pool (`column.c:114-123`); an unopenable operand warns and only sets
//! the exit status, and the run continues to the next file. A bare `-` is
//! a literal file name (`fopen`), not a stdin alias. The terminal width
//! comes from `-c` (the C queries the window size, which belongs to the
//! terminal face).
//!
//! 条目池里走的是**字节**：`column.c` 的 `input` 把 `fgetln` 那段原样存进池子（`:270-297`），
//! 布局宽度按 `strlen` 量（`:243`），所以一枚非 UTF-8 字节既进池也占一格；表模式的最后一列
//! 走 `printf("%s\n", …)`（`:252`）原样发出，既不补齐也没有裁尾那一步。
//!
//! 诊断面有四处，句式各有主人：libc `getopt` 的两行（集外字母、带值选项缺值）由
//! [`minix_textfilter::GetoptReport`] 单源，后面必接一行 usage；`warn(3)` 的
//! `Cannot open \`…'` 是「程序名: 标签: strerror 句子」三段形；`warnx(3)` 的
//! `line too long` 只有两段（`vwarnx` 不缀 strerror），且它每丢一条残行打一句。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::column::{columnate, parse_column_args, scan_entries};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // C 的选项环把 `'?'` 与集外一切交回 `default: usage()`（`column.c:107-109`），
    // 而 libc 在返回 `'?'` 之前自己先印一行（`getopt.c:101-104`、`:132-136`）。
    // 那一行走字节形 `line_bytes`：libc 的字母位是一个裸字节（`getopt.c:79` 的
    // `optopt = *place++` 一次进一个字节、`:103` 用 `%c` 原样写出），参照件
    // `tmp/column-314/column` 现量 `column -é` 打 `c3` 一个字节，按 `char` 渲染会多一个。
    let (options, operands) = match parse_column_args(&args[1..]) {
        Ok(a) => (a.options, a.operands),
        Err(report) => {
            support::warn(&report.line_bytes(prog));
            usage(prog);
        }
    };

    // `column.c:114-123`：无操数读标准输入；否则逐个开档、共用同一个条目池，
    // 开不了的档只 `warn(3)` 并把退出码置 1（`eval = 1`），本次运行继续走下一个档。
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let mut rval = 0;
    if operands.is_empty() {
        collect(&mut entries, &mut rval, &support::read_stdin(), prog);
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(bytes) => collect(&mut entries, &mut rval, &bytes, prog),
                Err(code) => {
                    support::warn_line(prog, &format!("Cannot open `{operand}'"), code);
                    rval = 1;
                }
            }
        }
    }
    // `column.c:125-126`：条目池空则按 `eval` 直接返回，不进布局——布局里的
    // `maxlength` 是 0 条时算出来的那个取齐值，与 C 的「根本没进这几个函数」不同形。
    if entries.is_empty() {
        support::terminate(rval);
    }
    for line in columnate(&entries, &options) {
        support::emit(&line);
        support::emit(b"\n");
    }
    support::terminate(rval);
}

/// Collects one source's bytes into the entry pool: split on newlines, then
/// keep the non-blank entries (the C `input`/`add` behaviour). A tail
/// line without its newline is dropped by the C with one
/// `warnx("line too long")` per occurrence, and each such line also sets the
/// exit status (`column.c:277-283`).
///
/// 行是字节不是文本：C 的 `input` 把 `fgetln` 交回的整段原样存进池子，从不解码。
/// 旧写法 `from_utf8(bytes).unwrap_or("")` 让一枚坏字节抹掉**整个档**的条目
/// （现量见 `tmp/column-435/ab.log`：参照件打 `a\xffb\tz`，本壳交回空）。
fn collect(entries: &mut Vec<Vec<u8>>, rval: &mut i32, bytes: &[u8], prog: &str) {
    let lines: Vec<&[u8]> = bytes.split(|&byte| byte == b'\n').collect();
    let pool = scan_entries(&lines);
    entries.extend(pool.entries);
    for _ in 0..pool.dropped_tail {
        support::warn(format!("{prog}: line too long\n").as_bytes());
        *rval = 1;
    }
}

/// C 的 `usage()`（`column.c:302-307`）：一行大写 `Usage`、程序名跟着
/// `getprogname()`、`exit(1)`。
fn usage(prog: &str) -> ! {
    support::warn(format!("Usage: {prog} [-tx] [-c columns] [-s sep] [file ...]\n").as_bytes());
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
