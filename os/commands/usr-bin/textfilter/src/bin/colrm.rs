//! Minix-RS colrm — remove columns from each line.
//!
//! Ground truth: `minix3/usr.bin/colrm/colrm.c` (NetBSD), main at lines
//! 61-121: `colrm [start [stop]]`; every input byte advances a display
//! column (tab rounds up to the next multiple of eight, backspace pulls
//! one back, newline resets) and bytes whose new column falls inside
//! the removed range are dropped. The deciding half (the column walk and
//! the keep/drop rule, plus the operand face) is the library's `colrm`
//! module; this program runs the option ring, prints the three diagnostic
//! lines, and filters stdin. `colrm` takes no file operands at all —
//! its operands are column numbers.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::colrm::{ColrmOperand, filter_column, parse_columns};
use minix_textfilter::empty_optstring_operands_bytes;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    // `colrm.c:67` 的 optstring 是空串，与 `rev.c:70` 逐字同形：`-Z` 先由 libc 印一行
    // 集外字母（`getopt.c:101-104`），再由 `:68-72` 的 `default: usage()` 印 usage，
    // 而裸 `-` 与 `--` 之后的 token 都落回操作数（操作数是列号，不是文件名）。
    // 集外字母那一行走字节形 `line_bytes`：libc 的字母位是短横后的**一个裸字节**
    //（`getopt.c:79` 的 `optopt = *place++` 一次进一个字节，`:103` 用 `%c` 原样写出），
    // 参照件 `tmp/colrm-310/colrm_ref` 现量 `colrm -é` 打 `c3` 一个字节。
    let operands = match empty_optstring_operands_bytes(&words) {
        Ok(rest) => rest,
        Err(report) => {
            support::warn(&report.line_bytes(&prog));
            usage();
        }
    };
    let (start, stop) = match parse_columns(&operands) {
        Ok(range) => range,
        Err(ColrmOperand::TooManyOperands) => usage(),
        Err(ColrmOperand::IllegalColumn(word)) => {
            // C 的 `errx(1, "illegal column -- %s", argv[..])`（`colrm.c:80-86`）：
            // `errx` 只有程序名与句子两段，不缀 `strerror`；`%s` 打的是**操作数原文的
            // 字节**，所以那一串字节不经任何文本折返。
            support::warn(&illegal_column(&prog, word));
            support::terminate(1);
        }
        Err(ColrmOperand::StartAfterStop) => {
            // C 的 `err(1, "illegal start and stop columns")`（`colrm.c:94-95`）是 `err`
            // 而不是 `errx`，所以三段形里还缀一句 `strerror(errno)`；走到这里时 `errno`
            // 还是 0，句子内容由各平台的 strerror 表定（参照件在本机印 `Success`，
            // Minix 的表对 0 另有一句），所以本壳只保证形状。
            support::warn_line(
                &prog,
                "illegal start and stop columns",
                minix_sys::Errno::from_i32(0),
            );
            support::terminate(1);
        }
    };

    // 读缝走 `bin_support` 的双生面（宿主 `std::io::stdin`、真机 `read(STDIN)`）。
    // C 那边是一边读一边写（`getchar`/`putchar`），输出内容与先收全再过滤同一物。
    //
    // 两根腿都可能出错，C 都交给同一个 `check()`（`colrm.c:123-130`）：它先看 `feof`
    // 再看 `ferror`，报错时报的是**流名**——`err(1, "%s", stream == stdin ? "stdin" :
    // "stdout")`。写错发生在环内（`putchar` 返回 EOF 就当场查流并退出），所以它先于
    // 「读到头才发现的读错」；本壳把这两件事的先后照搬，各报一行、退码 1。
    let (input, read_failure) = support::read_stdin_error();
    let filtered = filter_column(&input, start, stop);
    match support::emit_error(&filtered) {
        Some(code) => {
            support::warn_line(&prog, "stdout", code);
            support::terminate(1);
        }
        None => match read_failure {
            Some(code) => {
                support::warn_line(&prog, "stdin", code);
                support::terminate(1);
            }
            // 读干净、写干净，就是 C 的 `check(stdin)` 走到 `feof` 那一支：`exit(0)`。
            None => support::terminate(0),
        },
    }
}

/// `errx(1, "illegal column -- %s", argv[..])` 那一行：程序名、句子、操作数的**原字节**。
fn illegal_column(prog: &str, word: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(prog.as_bytes());
    out.extend_from_slice(b": illegal column -- ");
    out.extend_from_slice(word);
    out.push(b'\n');
    out
}

/// C 的 `usage()`（`colrm.c:132-137`）：一行、退 1。这一行里的命令名是**写死**的
/// `"colrm"`（不像 `rev.c:110-115` 那样跟着 `getprogname()`），所以这里不拼程序名。
fn usage() -> ! {
    support::warn(b"usage: colrm [start [stop]]\n");
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
