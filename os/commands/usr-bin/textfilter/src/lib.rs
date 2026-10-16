#![cfg_attr(not(test), no_std)]
// SD-19 静默吞错家族机械化门（第七家 crate，与 minix-sys/rt/fs/kernel/sef/fs-rt 同程序）：
// textfilter 里当前仅 `cmp.rs` 一处 `let _ = write!` 入 String（仅内存分配可失败，
// C regular.c 对应 printf 亦不分支）。新开无理由的 `let _ =` 会当场多报一条。
#![warn(clippy::let_underscore_must_use)]

//! Text filtering and data processing core for Minix-RS commands.
//!
//! Covers `rewrite-notes/18-stage-commands/07-text-filter.md`:
//! the line oriented tools in `minix3/usr.bin/` (`head`, `tail`, `sort`,
//! `uniq`, `wc`, `cut`, `tr`, `expand`, `fold`, `rev`, `seq`, `tee`, ...).
//! Every tool here reads lines and writes lines; the only thing that
//! differs is the per line decision. This crate owns those decisions as
//! pure functions over borrowed text:
//!
//! - [`window`]: head/tail line windows behind the [`window::LineWindow`]
//!   trait.
//! - [`count`]: the `wc` counters (lines, words, bytes, longest line).
//! - [`cut`]: field list parsing and column selection.
//! - [`tr`]: character set translation, deletion, and squeezing over the
//!   [`tr::CharClass`] trait.
//! - [`uniq`]: adjacent duplicate handling (`-c` counts, `-d`/`-u`
//!   selection).
//!
//! Sorting (`sort`), comparison (`cmp`, `diff`, `patch`), pagination, and
//! number formatting stay with later stages: `sort` needs ordering plus
//! buffering policy, the comparators need file pairing, and the formatters
//! need output width policy. The line algorithms here are the shared
//! foundation they will all reuse.
//!
//! Everything borrows from the input and uses fixed size buffers: no heap,
//! `no_std` throughout.

// The `sort` option parser materialises its key list; every other module
// stays allocation free.
extern crate alloc;

pub mod asa;
pub mod cksum;
pub mod cmp;
pub mod col;
pub mod colcrt;
pub mod colrm;
pub mod column;
pub mod comm;
pub mod count;
pub mod diff;
pub mod expand;
pub mod jot;
pub mod join;
pub mod lam;
pub mod look;
pub mod nl;
pub mod patch;
pub mod paste;
pub mod split;
pub mod floatfmt;
pub mod fmt;
pub mod fold;
pub mod hexdump;
pub mod unexpand;
pub mod cut;
pub mod pr;
pub mod rev;
pub mod seq;
pub mod sort;
pub mod tsort;
pub mod tr;
pub mod unifdef;
pub mod units;
pub mod uniq;
pub mod vis;
pub mod window;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): bad field lists, bad counts, bad
/// sets. All bounds are explicit fixed capacities; overflow is an error,
// never a silent truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    /// Malformed input.
    InvalidArgument,
    /// A fixed buffer proved too small for the result.
    TooLong,
}

impl TextError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            TextError::InvalidArgument => 22,
            TextError::TooLong => 12,
        }
    }
}

/// libc 的 `getopt` 在返回 `'?'` **之前**自己先印的那一行。
///
/// 这一行不属于任何一条命令：只要 optstring 不以 `:` 开头（本族没有一条以它开头），
/// `getopt` 就在两个地方各自印一句，然后才把 `'?'` 交给调用者的 `usage()`：
/// 集外字母走 `minix3/lib/libc/stdlib/getopt.c:101-104`，带值选项缺值走
/// `:132-136`。两句都把程序名放在最前（`getprogname()`），字母**不带引号**——
/// 本机 glibc 写的是 `invalid option -- 'x'`，拿宿主当基线会错，所以这一句的真值
/// 只能来自 C 源。两个命令各写一份同样的句子就是重复承载同一个事实（谁先被抹平
/// 都不会改变行为），所以把它收在这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetoptReport {
    /// `getopt.c:101-104`：集外字母（libc 版）。
    UnknownOption(char),
    /// `egetopt.c:144-151`：集外字母（`pr` 自带的 `egetopt` 版）。措辞与 libc 那一行**不同**，
    /// 所以不能共用一个变体：`pr` 不吃 libc 的 `getopt`，它链的是
    /// `minix3/usr.bin/pr/egetopt.c`，而那里写的是 `illegal option`。同一个字母在两个
    /// 解析器下得到的句子就是不一样，拿“同族规律”抹平会把措辞写错。
    IllegalOption(char),
    /// `getopt.c:132-136`（libc）与 `egetopt.c:196-205`（`pr`）——两处的句子逐字相同，
    /// 所以共一个变体。（`egetopt` 还有第三句 `illegal '+' delimiter with option`，
    /// 在 `:158-167`，只在 `+` 后面接不上数字时走，本壳的 `+` 腿自己报 `+page number ...`。）
    MissingValue(char),
}

impl GetoptReport {
    /// 那一行的完整文本（带换行）。调用方把它写到标准错误，随后接自己的 usage。
    ///
    /// 前缀都是程序名：libc 那两句递的是 `getprogname()`，而 `egetopt` 自己拿
    /// `strrchr(*nargv, '/')` 现算 `argv[0]` 的最后一段（`egetopt.c:145-147`、
    /// `:159-161`、`:197-199`），结果同一物。
    #[must_use]
    pub fn line(self, prog: &str) -> alloc::string::String {
        use alloc::format;
        match self {
            GetoptReport::UnknownOption(letter) => {
                format!("{prog}: unknown option -- {letter}\n")
            }
            GetoptReport::IllegalOption(letter) => {
                format!("{prog}: illegal option -- {letter}\n")
            }
            GetoptReport::MissingValue(letter) => {
                format!("{prog}: option requires an argument -- {letter}\n")
            }
        }
    }
}

/// 空 optstring 的选项环：交回 `argv += optind` 之后的操作数。
///
/// `rev.c:70` 与 `colrm.c:67` 写的都是 `getopt(argc, argv, "")`，两处形状逐字相同，
/// 所以这一支放在这里由两条命令共用（同 [`GetoptReport`] 的处置理由：一份事实只承载一次）。
/// 空 optstring 不等于「没有选项环」，它的后果有三处，全部由 libc `getopt` 定：
/// ① `strchr("", <字母>)` 永远是 `NULL`，所以任何 `-` 开头的 token 都落
/// `minix3/lib/libc/stdlib/getopt.c:95-104` 的集外字母支——libc 先印一行再返回 `'?'`；
/// ② `--` 在 `:81-85` 结束选项环且**先 `++optind`**，所以它自身不是操作数；
/// ③ 裸 `-` 在 `:87-92` 因 optstring 里没有 `-` 而结束选项环**却不推进** `optind`，
/// 于是它自己是操作数（两条命令怎么处理这个操作数各不相同，归各自的壳）。
///
/// 不重排：非选项 token 一到，`:74-78` 当场返回 -1，它和它后面的 token 整个交回当操作数。
/// 只看**第一个** token：调用者的 `while` 圈定不了第二次（拿到 `'?'` 就 `usage()` 退出，
/// 拿到 -1 就结束），所以这里写单次判定而不写循环。
///
/// 这一支不判「带值选项缺值」：空 optstring 里没有任何选项要值，`getopt.c:112-141`
/// 那整段到不了，所以只可能交回 [`GetoptReport::UnknownOption`]。
pub fn empty_optstring_operands<'a>(
    argv: &'a [&'a str],
) -> Result<alloc::vec::Vec<&'a str>, GetoptReport> {
    let Some(first) = argv.first() else {
        return Ok(alloc::vec::Vec::new());
    };
    let Some(flag) = first.strip_prefix('-') else {
        return Ok(argv.to_vec());
    };
    if flag == "-" {
        return Ok(argv[1..].to_vec());
    }
    let Some(letter) = flag.chars().next() else {
        return Ok(argv.to_vec());
    };
    Err(GetoptReport::UnknownOption(letter))
}

#[cfg(test)]
mod tests {
    use super::GetoptReport;

    /// 那两行的原文逐字钉在这里（单源），因为壳侧的门只能证明「渲染出来了」，
    /// 证明不了措辞。真值取 `minix3/lib/libc/stdlib/getopt.c:101-104` 与
    /// `:132-136`：`"%s: unknown option -- %c\n"`、
    /// `"%s: option requires an argument -- %c\n"`，字母两边都不带引号。
    #[test]
    fn getopt_report_lines_match_the_libc_format_strings() {
        assert_eq!(
            GetoptReport::UnknownOption('x').line("cut"),
            "cut: unknown option -- x\n"
        );
        assert_eq!(
            GetoptReport::MissingValue('f').line("tail"),
            "tail: option requires an argument -- f\n"
        );
        // 程序名来自 `getprogname()`（或 `egetopt` 自己算的那一段），不是写死的命令名。
        assert_eq!(
            GetoptReport::UnknownOption('3').line("head"),
            "head: unknown option -- 3\n"
        );
        // `pr` 自带的 `egetopt` 对集外字母写的是另一句（`egetopt.c:148`），与上面
        // 那一形一字之差；拿同族措辞替它会把句子写错。
        assert_eq!(
            GetoptReport::IllegalOption('x').line("pr"),
            "pr: illegal option -- x\n"
        );
        // 缺值那一句两边逐字相同（`egetopt.c:202` 与 `getopt.c:134`），所以两枝共用一个变体。
        assert_eq!(
            GetoptReport::MissingValue('w').line("pr"),
            "pr: option requires an argument -- w\n"
        );
    }
}
