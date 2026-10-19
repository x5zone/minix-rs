//! Minix-RS cut — select bytes or fields per line.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (NetBSD), `cut -b list |
//! -c list | -f list [-d delim] [-s] [file ...]`. The list grammar and the
//! byte/field selection are the library's (`parse_list`,
//! `select_bytes`, `select_fields`); `-b` and `-c` coincide here because
//! the selection is byte-grained (the C distinction matters only for
//! multibyte locales, which this build does not carry). With no operand lines
//! come from stdin; each `-` operand reads stdin and each file operand is
//! opened, and a file that cannot be opened warns and is skipped (the exit
//! status stays nonzero, C `cut.c:138-142`). A malformed list or a missing
//! selection exits 1; a delimiter must be one byte.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::cut::{CutError, parse_cut_args, parse_list, select_bytes, select_fields};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C 的每一句诊断都以 `getprogname()` 开头（err/errx 走 `vwarn`/`vwarnx`）。
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_cut_args`): BSD
    // `getopt("b:c:d:f:sn")` with attached-or-next list/delimiter values and
    // flag clustering. The old inline whole-token matcher mishandled a flag
    // clustered with a value option (`cut -sf1-3` silently dropped the field
    // list) and `-ns` (dropped the `-s` suppress).
    let parsed = match parse_cut_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e, prog),
    };
    let bytes_list = parsed.bytes_list;
    let fields_list = parsed.fields_list;
    let delimiter = parsed.delimiter;
    let suppress = parsed.suppress;
    let operands: Vec<String> = parsed.operands.iter().map(|s| String::from(*s)).collect();

    let selecting_bytes = bytes_list.is_some();
    // `parse_cut_args` 已经在选项环里把列表解了一遍（C 的 `get_list` 就坐在那个
    // case 里），所以这里只剩「取哪一份」。四道 post-check 也已在那一半跑完。
    let list_text = match (bytes_list, fields_list) {
        (Some(list), None) | (None, Some(list)) => list,
        _ => usage(),
    };
    let ranges = match parse_list(list_text) {
        Ok(parsed) => parsed,
        // 不可达：上面那个解析器已经跑过同一个串。真到了这里也按 C 的句子报，
        // 而不是静默退 1——旧写法就是静默退 1，一句都不说。
        Err(error) => arg_error(CutError::from(error), prog),
    };

    let process = |input: &[u8]| {
        for line in support::lines_of(input) {
            // `cut` 是 C 里**归一化补换行**的那一支：`cut.c` 每处理完一行都写一枚
            // `\n`，参照件 `tmp/cut-406/cut_ref` 对无尾换行的输入也交回三行带换行
            // （读数 `alp\nbet\ngam\n`，见 18 篇批次四百二十三）。所以这里取 `text()`
            // 而**不取** `eol()`——不是漏改，是这一支的 C 就这么做。
            let line = line.text();
            let mut out = vec![0u8; line.len() + 1];
            let selection = if selecting_bytes {
                // 字节腿没有「整行跳过」这一说：`-s` 只与 `-f` 并存（`cut.c:124`），
                // 所以那一边的返回一律是「要写的一行」。
                select_bytes(line, &ranges, &mut out).map(Some)
            } else {
                select_fields(line, &ranges, delimiter, suppress, &mut out)
            };
            match selection {
                Ok(Some(len)) => {
                    support::emit(&out[..len]);
                    support::emit(b"\n");
                }
                // `-s` 且这一行没有分隔符：C 连换行都不打——`cut.c:248` 那一判不成立，
                // `:253-254` 又跳过 `:279` 那句 `putchar('\n')`。
                Ok(None) => {}
                Err(_) => support::terminate(1),
            }
        }
    };

    // C `if (*argv) for each operand fcn(...) else fcn(stdin)`: `-` and the
    // empty operand list both read stdin; a file is opened per operand, and an
    // unopenable file warns and is skipped (exit status stays nonzero).
    let mut status = 0;
    if operands.is_empty() {
        process(&support::read_stdin());
    }
    for operand in &operands {
        if operand == "-" {
            process(&support::read_stdin());
            continue;
        }
        // `support::read_file` 而不是裸 `minix_sys::open`：后者在本 crate 的命令层
        // 没有宿主双生面，直接跑宿主二进制时任何操作数都打不开（tail 在同一条腿上
        // 过同一个坑，那个缺口当时被登为「逐字对账不适用于 tail」）。接回双生面之后，
        // 这一腿的报错腿才能被 `cut_face.rs` 钉住。
        let input = match support::read_file(operand) {
            Ok(input) => input,
            // C `cut.c:138-142` 的 `warn("%s", *argv)`：程序名、操作数、`strerror`
            // 句子三个成分缺一不可（旧写法把 `warn(3)` 当成 `warnx(3)` 用）。
            Err(code) => {
                support::warn_line(prog, operand, code);
                status = 1;
                continue;
            }
        };
        process(&input);
    }
    support::terminate(status)
}

/// Map a [`CutError`] to the exact C path (message and exit code), then terminate.
fn arg_error(e: CutError, prog: &str) -> ! {
    match e {
        CutError::Usage => usage(),
        // libc 的那一行排在前面，usage 的三行在后面。
        //
        // 字母位走 `line_bytes`：`src/cut.rs` 的取法本就按字节（`bytes[pos] as char`），而 libc
        // 那一行的字母位本来是一个字节（`getopt.c:79` 的 `optopt = *place++`、`:103` 的 `%c`）。
        // 参照件 `tmp/cut-406/cut_ref` 现量 `-é` 与 `-cé` 都是 `cut: unknown option -- ` +
        // 单个 `c3` + 换行，再跟那三行制表符对齐的 usage、退 1。
        CutError::Getopt(report) => {
            support::warn(&report.line_bytes(prog));
            usage()
        }
        // C 的两句 errx（`cut.c:196`、`:198`）：走 `vwarnx`，所以后面不缀 `errno`
        // 句子，而且两句都不把出错的那一段拼回去。
        CutError::IllegalList => {
            support::warn(format!("{prog}: [-bcf] list: illegal list value\n").as_bytes());
            support::terminate(1);
        }
        CutError::ZeroInList => {
            support::warn(format!("{prog}: [-bcf] list: values may not include zero\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// C `usage()`（`cut.c:290-296`）：**三行**，`usage:` 后面是一个制表符，后两行各以
/// 制表符开头，而且这句里没有程序名占位（C 写死了 `cut`）。全部写标准错误——
/// 旧写法走的是 `support::emit`，即把诊断发到标准输出，那是通道错。退 1。
fn usage() -> ! {
    support::warn(
        b"usage:\tcut -b list [-n] [file ...]\n\tcut -c list [file ...]\n\tcut -f list [-d string] [-s] [file ...]\n",
    );
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
