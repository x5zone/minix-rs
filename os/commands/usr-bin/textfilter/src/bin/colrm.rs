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

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::colrm::{ColrmOperand, filter_column, parse_columns};
use minix_textfilter::empty_optstring_operands;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    // `colrm.c:67` 的 optstring 是空串，与 `rev.c:70` 逐字同形：`-Z` 先由 libc 印一行
    // 集外字母（`getopt.c:101-104`），再由 `:68-72` 的 `default: usage()` 印 usage，
    // 而裸 `-` 与 `--` 之后的 token 都落回操作数（操作数是列号，不是文件名）。
    // 旧写法没有这一环，`-Z` 直接被当列号拒收，少了 libc 那一行；`--` 也被当坏选项。
    let operands = match empty_optstring_operands(&refs[1..]) {
        Ok(rest) => rest,
        Err(report) => {
            support::warn(report.line(prog).as_bytes());
            usage();
        }
    };
    let (start, stop) = match parse_columns(&operands) {
        Ok(range) => range,
        Err(ColrmOperand::TooManyOperands) => usage(),
        Err(ColrmOperand::IllegalColumn(word)) => {
            // C 的 `errx(1, "illegal column -- %s", argv[..])`（`colrm.c:80-86`）：
            // `errx` 只有程序名与句子两段，不缀 `strerror`。
            support::warn(format!("{prog}: illegal column -- {word}\n").as_bytes());
            support::terminate(1);
        }
        Err(ColrmOperand::StartAfterStop) => {
            // C 的 `err(1, "illegal start and stop columns")`（`colrm.c:94-95`）是 `err`
            // 而不是 `errx`，所以三段形里还缀一句 `strerror(errno)`；走到这里时 `errno`
            // 还是 0，句子内容由各平台的 strerror 表定（参照件在本机印 `Success`，
            // Minix 的表对 0 另有一句），所以本壳只保证形状。
            support::warn_line(
                prog,
                "illegal start and stop columns",
                minix_sys::Errno::from_i32(0),
            );
            support::terminate(1);
        }
    };

    // 读缝走 `bin_support` 的双生面（宿主 `std::io::stdin`、真机 `read(STDIN)`）：
    // 本文件以前直接拿裸 `minix_sys::read`，**宿主上读不到任何输入**，所以它从来没有
    // 被进程级测过（同批次三百零六的 `nl`、三百零八的 `fold`、三百零九的 `rev`）。
    // C 那边是一边读一边写（`getchar`/`putchar`），输出内容与先收全再过滤同一物。
    let input = support::read_stdin();
    support::emit(&filter_column(&input, start, stop));
    support::terminate(0);
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
