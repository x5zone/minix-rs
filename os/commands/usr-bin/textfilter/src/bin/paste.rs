//! Minix-RS paste — merge lines of files.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD), `paste [-s]
//! [-d list] file ...`. The ring is `paste.c:70`'s `getopt(argc, argv,
//! "d:s")`; the merge itself is the library's `join_parallel` /
//! `join_serial`, and every operand names an already-open stream (each
//! file opens once, all `-` operands share the single standard-input
//! stream, `paste.c:106-112`).
//!
//! 前门有**三家句子、两个退码**，都按 C 逐句搬回：
//! ① 集外字母与 `-d` 缺值各走「libc 一行 + **一行** usage」且退 **1**
//!    （`paste.c:80-82` 的 `case '?': default: usage();` 在环里直接退出，所以
//!    本支的环**只可能有一行** libc 报文——与 `seq` 的累加形正相反，`jot` 与本支同形）；
//!    usage 那一行的程序名是**写死在串里**的（`paste.c:231`），libc 那行跟着
//!    `getprogname()`，同一支命令里两行来历不同。
//! ② `-d ''` 走 `errx(1, "no delimiters specified.")`：一行、带程序名、**句子自带句号**、
//!    不带 `strerror`，退 1（`paste.c:221-223`）。
//! ③ 开不了的档名在两种模式下走两条不同的出口（这是本支最值钱的一处形状）：
//!    并行模式是 `err(1, "%s", argv[cnt])` 三段形**致命**（`paste.c:107-108`，
//!    第一个开不了的档名就退出，后面的档名连试都不试）；顺序模式是
//!    `warn("%s", p)` 三段形**非致命**（`paste.c:166-169`，跳过这个档名继续下一个，
//!    整体仍退 0）。两支的句子形状相同，退码与是否继续相反。
//!
//! 操作数**没有个数判**：`paste` 不带操作数既不读标准输入也不报错，退 0 打空
//! （`paste.c:88-92` 把 `argc` 直接交给 `parallel`，零条流时那个行循环一步不走）。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::paste::{PasteError, join_parallel, join_serial, parse_paste_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let parsed = match parse_paste_args(&words) {
        Ok(parsed) => parsed,
        Err(error) => fail(prog, &error),
    };
    let options = parsed.options;
    let operands = parsed.operands;
    if operands.is_empty() {
        support::terminate(0);
    }

    // 一列 = 一个**已打开的流**：档名各开一次（所以 `paste a.txt a.txt` 是两个游标），
    // 而所有 `-` 共用标准输入这一个流（参照件 `paste - -` 对三行打的是两列各取一行）。
    let mut sources: Vec<Vec<String>> = Vec::new();
    let mut columns: Vec<usize> = Vec::new();
    let mut stdin_source: Option<usize> = None;
    for operand in &operands {
        if *operand == "-" {
            let index = match stdin_source {
                Some(index) => index,
                None => {
                    sources.push(split_lines(&support::read_stdin()));
                    stdin_source = Some(sources.len() - 1);
                    sources.len() - 1
                }
            };
            columns.push(index);
            continue;
        }
        match support::read_file(operand) {
            Ok(data) => {
                sources.push(split_lines(&data));
                columns.push(sources.len() - 1);
            }
            Err(errno) => {
                support::warn_line(prog, operand, errno);
                if options.serial {
                    // `sequential` 的那一支是 `warn` + `continue`（`paste.c:166-169`）
                    continue;
                }
                support::terminate(1);
            }
        }
    }

    if options.serial {
        // 同一个流被列在多处时，第二处已经没有行可取（游标是流的，不是列的）
        let mut taken: Vec<usize> = vec![0; sources.len()];
        for column in &columns {
            let source = *column;
            let rest = &sources[source][taken[source]..];
            taken[source] = sources[source].len();
            let lines: Vec<&str> = rest.iter().map(String::as_str).collect();
            support::emit(join_serial(&lines, &options).as_bytes());
        }
    } else {
        let lines: Vec<Vec<&str>> = sources
            .iter()
            .map(|source| source.iter().map(String::as_str).collect())
            .collect();
        let slices: Vec<&[&str]> = lines.iter().map(Vec::as_slice).collect();
        support::emit(join_parallel(&columns, &slices, &options).as_bytes());
    }
    support::terminate(0)
}

/// 三家句子各走各的出口与退码；libc 那一行交回 [`minix_textfilter::GetoptReport`] 单源。
///
/// 字母位走 `line_bytes` 而不是 `line`：`src/paste.rs` 的取法本就按字节（`token[k]` 是一枚
/// `u8`，再 `char::from`），非 ASCII 字母经 `line` 会被 UTF-8 编码成两字节。参照件
/// `tmp/paste-321/paste_ref` 现量 `paste -é` 与 `paste -sé` 都是 `paste: unknown option -- ` +
/// 单个 `c3` + 换行，机制在 `getopt.c:79` 的 `optopt = *place++` 与 `:103` 的 `%c`。
fn fail(prog: &str, error: &PasteError) -> ! {
    match error {
        PasteError::Usage { reports } => {
            for report in reports {
                support::warn(&report.line_bytes(prog));
            }
            support::warn(b"paste: [-s] [-d delimiters] file ...\n");
            support::terminate(1);
        }
        PasteError::NoDelimiters => {
            support::warn(format!("{prog}: no delimiters specified.\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// Split bytes into newline-stripped lines, dropping the empty piece after a
/// trailing newline (interior blank lines stay as empty cells)——C 用 `fgetln`
/// 拿到含换行的整行再把换行减掉（`paste.c:134-136`），末行没有换行照样是一行。
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

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
