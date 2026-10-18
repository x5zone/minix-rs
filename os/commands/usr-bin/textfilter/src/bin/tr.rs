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
//!    取不到就报）。`str.c` 那侧的集合语法句形状相同，由 `parse_set` 回
//!    `SetError::Grammar` 后在这里逐句原样打出：`unknown class <名>`、
//!    `Sequences only valid in string2`、`Misplaced sequence asterisk`、
//!    `illegal sequence count`。`[=c=]` 等价类的两句仍是静默退 1。
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
    delete, parse_set, parse_tr_args, squeeze, translate, translate_and_squeeze, usage_block,
    Set2Tail, SetError, SetSite, TrArgs, TrError,
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

    // string1 用不到尾部判据（它决定的是「谁是定义域」），string2 才需要：式子以集合类
    // 收尾时耗尽之后补的是 OOBCH，不是末字节。
    let (set1, _) = expand(prog, operands[0], SetSite::String1, complement);
    let (set2, set2_tail) = if operands.len() == 2 {
        expand(prog, operands[1], SetSite::String2, false)
    } else {
        (Vec::new(), Set2Tail::LastByte)
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
        // C 的这一腿是**一趟**：映射之后的 `int` 值直接与 `string2[]`、`lastch` 比
        //（`tr.c:239-246`），所以拆成 translate + squeeze 两步会把 OOBCH 与真 NUL 混掉。
        emit(run_filter(&input, |buf, out| {
            translate_and_squeeze(buf, &set1, &set2, set2_tail, out)
        }));
    } else {
        emit(run_filter(&input, |buf, out| {
            translate(buf, &set1, &set2, set2_tail, out)
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
///
/// 语法句的形状是 C 的 `errx(1, ...)`：程序名、冒号、空格、原句，走标准错误，退 1，
/// **不跟** usage 块。式子出在串1 还是串2 决定它撞哪一句，所以串位是入参而不是巧合。
fn expand(prog: &str, text: &str, site: SetSite, complement: bool) -> (Vec<u8>, Set2Tail) {
    let mut expanded = [0u8; 256];
    let (used, tail) = match parse_set(text, site, &mut expanded) {
        Ok((used, tail)) => (used, tail),
        Err(SetError::Grammar(grammar)) => {
            support::warn(format!("{prog}: {}\n", grammar.sentence()).as_bytes());
            support::terminate(1);
        }
        Err(_) => {
            support::terminate(1);
        }
    };
    if !complement {
        return (expanded[..used].to_vec(), tail);
    }
    let mut member = [false; 256];
    for &byte in &expanded[..used] {
        member[byte as usize] = true;
    }
    (
        (0..=255u8).filter(|byte| !member[*byte as usize]).collect(),
        tail,
    )
}

/// 两个出错出口共用一块四行 usage，区别只在前面的 libc 行有无。
fn fail(prog: &str, error: &TrError) -> ! {
    match error {
        TrError::Usage { reports } => {
            for report in reports {
                // 字节形：libc 那一行的字母位是一个裸字节（`getopt.c:79` 的
                // `optopt = *place++` 一次进一个字节、`:103` 用 `%c` 原样写出）。参照件
                // `tmp/tr-322/tr` 现量 `tr -é a b` 打 `tr: unknown option -- ` + `c3` + 换行，
                // 按 `char` 渲染会多出第二个字节（U+00C3 的 UTF-8 是 `c3 83`）。
                support::warn(&report.line_bytes(prog));
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
