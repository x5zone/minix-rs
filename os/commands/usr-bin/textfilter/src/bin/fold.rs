//! Minix-RS fold — wrap lines at a width.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The deciding
//! half (the column rules for `\b`/`\r`/`\t`, the width break, the
//! last-space preference without `-s`) is the library's `fold` module.
//! With no operand the program folds standard input; with operands it folds
//! each file in turn (the column/word state restarting per file), and a file
//! it cannot open is fatal (C `err(1)`). The obsolete `-N` width form parses
//! too (`-w` is the modern spelling).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::fold::{fold_input, parse_fold_args, FoldArgs, FoldError, FoldOptions};
use minix_textfilter::GetoptReport;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // 程序名那一腿的渲染口仍是 `&str`（`GetoptReport::line_bytes`、`warn_line` 都收 `&str`），
    // 库侧字节口在册未动：`ARGV-BYTE-PATH-IN-LIBS`。字母与操作数这两腿从这里开始是原字节。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let FoldArgs { options, operands } = match parse_fold_args(&words[1..]) {
        Ok(parsed) => parsed,
        // 两支 getopt 自己的错（集外字母、`-w` 缺值）：先 libc 那一行，再 `usage()`。
        // 旧写法只打 usage，把首行丢了；而 usage 的文本与退码当时也不合 C。
        //
        // 字母位走 `line_bytes`，两半都在字节上：参数缝交回原字节，`src/fold.rs` 的取法
        // 取一个字节（`bytes[pos] as char`），于是 `-é`（`c3 a9`）只打 `c3` 一枚——与参照件
        // `tmp/fold-405/fold_ref` 同形（libc 那一行的字母位本来就是一个字节：
        // `getopt.c:79` 的 `optopt = *place++`、`:103` 的 `%c`）。缺值那一支的字母来自选项串
        // `"0123456789bsw:"`，本来就是 ASCII 的 `w`，两个入口同串。
        Err(FoldError::UnknownOption(flag)) => {
            support::warn(&GetoptReport::UnknownOption(flag).line_bytes(&prog));
            usage(&prog);
        }
        Err(FoldError::MissingValue(flag)) => {
            support::warn(&GetoptReport::MissingValue(flag).line_bytes(&prog));
            usage(&prog);
        }
        Err(FoldError::IllegalWidth) => {
            // C 这里是 `errx(1, "illegal width value")`：**只有这一行**，不接 usage。
            support::warn(format!("{prog}: illegal width value\n").as_bytes());
            support::terminate(1);
        }
    };

    // C `if (!*argv) fold(stdin); else for each argv freopen() … err(1) on
    // failure`: no operand folds stdin once; otherwise each file is folded in
    // turn (a fresh `fold_input` call restarts the column/word state), and an
    // unopenable file is fatal.
    if operands.is_empty() {
        fold_bytes(&support::read_stdin(), &options);
        support::terminate(0);
    }
    for operand in operands {
        // 开档与报文回显这两条腿的口都收 `&str`（`os/libs/minix-sys/src/lib.rs:fn open`、
        // `os/libs/minix-types/src/types/errno.rs:fn err_line`），字节形操作数只能在这里
        // 折一次——差在 `ARGV-BYTE-PATH-IN-LIBS` 里在册。
        let path = String::from_utf8_lossy(operand).into_owned();
        // 读缝走 `bin_support` 的双生面（宿主 `std::fs`、真机 open+read）：本文件此前
        // 直接拿裸 `open`/`read` 的 fd 自己读，**宿主上读不到任何东西**（同批次三百零六
        // 给 `nl` 收的那处），所以新立的端到端门一跑就现形：输出是空的。
        let input = match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // C 是 `err(1, "%s", *argv)`（`fold.c:107-109`）：程序名、操作数、`strerror`
                // 句子三段齐备。旧写法只打前两段，把句子弄丢了。
                support::warn_line(&prog, &path, code);
                support::terminate(1);
            }
        };
        fold_bytes(&input, &options);
    }
    support::terminate(0)
}

/// Emit each folded line of one already-read input; the per-file restart of the
/// column/word state falls out of calling `fold_input` once per file.
fn fold_bytes(input: &[u8], options: &FoldOptions) {
    let text = String::from_utf8_lossy(input);
    for line in fold_input(&text, options) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
}

/// C 的 `usage()`（`fold.c:115-121`）：一行、程序名跟着 `getprogname()`、`exit(1)`。
/// 旧写法把行项自成了 `[-b] [-s] [-w width | -width]` 且丢了 `[file ...]`。
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [-bs] [-w width] [file ...]\n").as_bytes());
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
