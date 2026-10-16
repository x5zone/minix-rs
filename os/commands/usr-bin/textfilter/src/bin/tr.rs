//! Minix-RS tr — translate, delete or squeeze characters.
//!
//! Ground truth: `minix3/usr.bin/tr/tr.c` (NetBSD) + `str.c`，命令行是
//! `tr [-cs] string1 string2` / `tr [-c] -d string1` / `tr [-c] -s string1`
//! / `tr [-c] -ds string1 string2`。三种操作（替换、删除、挤压）与 `-c` 取补
//! 是库侧的 `translate`/`delete`/`squeeze`/`parse_set`；本程序只管前门与数据搬运。
//!
//! 前门按 C 分回三家句子、两个退码：
//! ① 环（`tr.c:68-82`，`getopt("cds")`）**只可能有一个集外字母报文**——
//!    `case '?': default: usage();`（`:79-81`）在环里直接退出，libc 那一行之后紧跟
//!    **整块四行** usage，退 1。缺值那一支不可达（三个字母都不带值）。
//! ② usage 那四行（`tr.c:276-282`）里程序名是**写死在串里**的，且走**标准错误**；
//!    libc 那一行的前缀才跟着 `getprogname()`。
//! ③ 命令自己的 `errx(1, ...)` 句子退 1、各一行且不带 usage：本支有的是
//!    `tr: empty string2`（`tr.c:187-188`，翻译路径先取 string2 的第一个字符，
//!    取不到就报）。集合语法那两句（`tr: Sequences only valid in string2`、
//!    `tr: unknown class <名>`）在 `str.c`，本层的 `parse_set` 目前只回一个
//!    `TextError`，故那两句登记为 `TR-SET-GRAMMAR-SENTENCES` 而未接。
//!
//! 个数判与组合判全走 usage：0 个操作数、1 个操作数却没给 `-d`/`-s`、3 个及以上、
//! `-ds` 少给一个、`-d` 多给一个——参照件逐形量出（`tmp/tr-322/ab322.log` 的 A 类）。
//! 环不重排：`tr a -d` 的 `-d` 是第二个集合名，`tr -` 里的 `-` 是集合名。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::tr::{
    TrArgs, TrError, delete, parse_set, parse_tr_args, squeeze, translate, usage_block,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let TrArgs {
        complement,
        delete: dflag,
        squeeze: sflag,
        operands,
    } = match parse_tr_args(&words) {
        Ok(parsed) => parsed,
        Err(error) => fail(prog, &error),
    };

    let set1 = expand(operands[0], complement);
    let set2 = if operands.len() == 2 {
        expand(operands[1], false)
    } else {
        Vec::new()
    };
    // `tr.c:187-188`：只有翻译路径（含 `-s` 翻译）会先取 string2 的首字符，取不到就报错；
    // `-ds` 与 `-d` 路径不碰这条判据。
    if !dflag && operands.len() == 2 && set2.is_empty() {
        support::warn(format!("{prog}: empty string2\n").as_bytes());
        support::terminate(1);
    }

    let input = support::read_stdin();
    if dflag && sflag {
        // `tr -ds string1 string2`：删掉 string1 里的，再挤压 string2 里的相邻重复
        let stage = run_filter(&input, |buf, out| delete(buf, &set1, out));
        emit(run_filter(&stage, |buf, out| squeeze(buf, &set2, out)));
    } else if dflag {
        emit(run_filter(&input, |buf, out| delete(buf, &set1, out)));
    } else if sflag && operands.len() == 1 {
        emit(run_filter(&input, |buf, out| squeeze(buf, &set1, out)));
    } else if sflag {
        let stage = run_filter(&input, |buf, out| translate(buf, &set1, &set2, out));
        emit(run_filter(&stage, |buf, out| squeeze(buf, &set2, out)));
    } else {
        emit(run_filter(&input, |buf, out| {
            translate(buf, &set1, &set2, out)
        }));
    }
    support::terminate(0)
}

/// 跑一步过滤：输出缓冲按输入长度 + 1 备（每步都只删不改长，挤压同理），
/// 失败即退 1——与 C 的 `setup` 在坏集合面前直接 `exit` 同形。
fn run_filter(
    input: &[u8],
    once: impl Fn(&[u8], &mut [u8]) -> Result<usize, minix_textfilter::TextError>,
) -> Vec<u8> {
    let mut out = vec![0u8; input.len() + 1];
    match once(input, &mut out) {
        Ok(len) => out[..len].to_vec(),
        Err(_) => {
            support::terminate(1);
        }
    }
}

fn emit(bytes: Vec<u8>) {
    if !bytes.is_empty() {
        support::emit(&bytes);
    }
}

/// 把一个集合式展开成它实际含的字节；`-c` 时列出式子**没**含的一切字节。
fn expand(text: &str, complement: bool) -> Vec<u8> {
    let mut expanded = [0u8; 256];
    let used = match parse_set(text, &mut expanded) {
        Ok((used, _, _)) => used,
        Err(_) => {
            support::terminate(1);
        }
    };
    if !complement {
        return expanded[..used].to_vec();
    }
    let mut member = [false; 256];
    for &byte in &expanded[..used] {
        member[byte as usize] = true;
    }
    (0..=255u8).filter(|byte| !member[*byte as usize]).collect()
}

/// 两个出错出口共用一块四行 usage，区别只在前面的 libc 行有无。
fn fail(prog: &str, error: &TrError) -> ! {
    match error {
        TrError::Usage { reports } => {
            for report in reports {
                support::warn(report.line(prog).as_bytes());
            }
            support::warn(usage_block());
            support::terminate(1);
        }
    }
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
