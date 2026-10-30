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
pub mod cut;
pub mod diff;
pub mod digest;
pub mod expand;
pub mod floatfmt;
pub mod fmt;
pub mod fold;
pub mod hexdump;
pub mod join;
pub mod jot;
pub mod lam;
pub mod look;
pub mod nl;
pub mod paste;
pub mod patch;
pub mod pr;
pub mod rev;
pub mod seq;
pub mod sort;
pub mod split;
pub mod tr;
pub mod tsort;
pub mod unexpand;
pub mod unifdef;
pub mod uniq;
pub mod units;
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
    /// 那一行的措辞与它要报的字母——两个渲染入口共用这一份，措辞只承载一次。
    fn wording(self) -> (&'static str, char) {
        match self {
            GetoptReport::UnknownOption(letter) => ("unknown option -- ", letter),
            GetoptReport::IllegalOption(letter) => ("illegal option -- ", letter),
            GetoptReport::MissingValue(letter) => ("option requires an argument -- ", letter),
        }
    }

    /// 那一行的完整文本（带换行）。调用方把它写到标准错误，随后接自己的 usage。
    ///
    /// 前缀都是程序名：libc 那两句递的是 `getprogname()`，而 `egetopt` 自己拿
    /// `strrchr(*nargv, '/')` 现算 `argv[0]` 的最后一段（`egetopt.c:145-147`、
    /// `:159-161`、`:197-199`），结果同一物。
    ///
    /// 字母按 `char` 的 `Display` 渲染，于是 `0x80..=0xFF` 这一段会得到**两个**字节
    /// （UTF-8 编码），而 C 打的是一个裸字节——差在渲染，见 [`GetoptReport::line_bytes`]。
    #[must_use]
    pub fn line(self, prog: &str) -> alloc::string::String {
        let (wording, letter) = self.wording();
        alloc::format!("{prog}: {wording}{letter}\n")
    }

    /// 同一行的**字节**形：把字母当**一个裸字节**写出去。
    ///
    /// C 那一行递的是 `optopt`，而 `optopt = *place++`（`getopt.c:79`、`:95`）取的是 argv 里的
    /// **一个字节**，`fprintf` 的 `%c` 再把这个字节原样写出（`getopt.c:101-104`）。参照件
    /// `tmp/rev-309/rev_ref` 现量：`rev -é`（`é` 是 `c3 a9` 两字节）打的是
    /// `rev: unknown option -- ` + 裸字节 `c3` + 换行，只此一行（`rev.c:72-74` 拿到 `'?'`
    /// 就 `usage()` 退出，不会再回到 `getopt` 读第二个字节）。
    ///
    /// 码位落在 `0x00..=0xFF` 就当**那一个字节**写——C 的字母位本来就是一个字节，
    /// 这一段与本方法的用途完全对上。码位**高于** `0xFF` 时 C 无字节可打：那种码位只可能
    /// 来自「解析那半还按字符取」的壳，这里退回该字符的 UTF-8 编码，与
    /// [`GetoptReport::line`] 一字不差。这条退路是为**分轮迁移**留的：`line` 的调用方散在
    /// 二十多支壳里，一轮只能换一支，换渲染的那一轮不该顺带改动还没换取法的壳的报文。
    #[must_use]
    pub fn line_bytes(self, prog: &str) -> alloc::vec::Vec<u8> {
        let (wording, letter) = self.wording();
        let mut out = alloc::format!("{prog}: {wording}").into_bytes();
        match u8::try_from(u32::from(letter)) {
            Ok(byte) => out.push(byte),
            Err(_) => out.extend_from_slice(letter.encode_utf8(&mut [0u8; 4]).as_bytes()),
        }
        out.push(b'\n');
        out
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
///
/// 报的是短横后的**第一个字节**，不是第一个字符：`getopt.c:79` 的 `optopt = *place++`
/// 步进在 `char *` 上，一次一个字节。参照件现量 `rev -é` 报裸字节 `c3`（`é` 的后半 `a9`
/// 根本不进报文），按字符取会报 U+00E9。同法先例见 `src/col.rs:parse_col_flags`。
/// 空 optstring 的操作数环（A2 参数缝半的字节形，`&str` 共件已随族终笔删除）：同一张三形判据，交回的
/// 是原字节。字母仍是短横后的**第一个字节**——`char::from(byte)` 收进报告，那正是 C 里
/// `unsigned char` 提升成 `int` 的形状；[`GetoptReport::line_bytes`] 再把它打回那一个字节。
pub fn empty_optstring_operands_bytes<'a>(
    argv: &'a [&'a [u8]],
) -> Result<alloc::vec::Vec<&'a [u8]>, GetoptReport> {
    let Some(first) = argv.first() else {
        return Ok(alloc::vec::Vec::new());
    };
    let Some(flag) = first.strip_prefix(b"-") else {
        return Ok(argv.to_vec());
    };
    if flag == &b"-"[..] {
        return Ok(argv[1..].to_vec());
    }
    let Some(&byte) = flag.first() else {
        return Ok(argv.to_vec());
    };
    Err(GetoptReport::UnknownOption(char::from(byte)))
}

/// C 的 `atoi`：跳过前导 `isspace`、可带符号、取一串十进制数字，**尾巴上有杂字符不管**，
/// 一位数字都没有就是 0。
///
/// 住在 crate 根由几条命令共用（同 [`GetoptReport`] 的处置理由：一份事实只承载一次）——
/// `col` 的 `-l`（`col.c` 的 `atoi(optarg)`）与 `column` 的 `-c`（`column.c:96`）
/// 走的是同一个 libc 函数，判据一模一样。
///
/// 所以 `3x` 是合法的 3（参照件同受），而 `abc`、`0`、`-2` 分别得到 0、0、-2；
/// 溢出按 C 的 `strtol` 截到 `i32` 两端（`saturating_*`），因为 `atoi` 的返回值
/// 本来就是 `int`。
#[must_use]
pub fn atoi(text: &[u8]) -> i32 {
    let bytes = text;
    let mut cursor = 0usize;
    while matches!(
        bytes.get(cursor),
        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(0x0b) | Some(0x0c) | Some(b'\r')
    ) {
        cursor += 1;
    }
    let negative = match bytes.get(cursor) {
        Some(b'+') => {
            cursor += 1;
            false
        }
        Some(b'-') => {
            cursor += 1;
            true
        }
        _ => false,
    };
    let mut value: i32 = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i32::from(bytes[cursor] - b'0'));
        cursor += 1;
    }
    if negative { -value } else { value }
}

#[cfg(test)]
mod tests {
    use super::{GetoptReport, empty_optstring_operands_bytes};

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

    /// 字节形那一支：字母是**一个裸字节**，不是它的 UTF-8。
    ///
    /// 真值是参照件现量（`tmp/rev-309/rev_ref` 打 `rev -é` 得 `unknown option -- ` + `c3`），
    /// 机制在 `getopt.c:79` 的 `optopt = *place++` 与 `:103` 的 `%c`。
    #[test]
    fn getopt_report_bytes_write_the_letter_as_one_byte() {
        assert_eq!(
            GetoptReport::UnknownOption('\u{00c3}').line_bytes("rev"),
            b"rev: unknown option -- \xc3\n".to_vec(),
            "0xC3 只能是一个字节；按 char 渲染会多出第二个字节 0x83"
        );
        assert_eq!(
            GetoptReport::IllegalOption('\u{0080}').line_bytes("pr"),
            b"pr: illegal option -- \x80\n".to_vec()
        );
        assert_eq!(
            GetoptReport::MissingValue('f').line_bytes("tail"),
            b"tail: option requires an argument -- f\n".to_vec(),
            "另外两句的措辞与 line 同一份来源"
        );
        // ASCII 那一形两个入口一字不差：迁移不许动它。
        assert_eq!(
            GetoptReport::UnknownOption('Z').line_bytes("rev"),
            GetoptReport::UnknownOption('Z').line("rev").into_bytes()
        );
        // 落在 `0x00..=0xFF` 的码位一律当**那一个字节**写：U+00E9 打 `e9`，比 `line` 的
        // UTF-8 形（`c3 a9`）少一个字节。
        assert_eq!(
            GetoptReport::UnknownOption('\u{00e9}').line_bytes("rev"),
            b"rev: unknown option -- \xe9\n".to_vec()
        );
        // 分叉的另一侧也钉住：`line` 对同一个码位打两个字节，这就是登记的那处偏差。
        assert_eq!(
            GetoptReport::UnknownOption('\u{00c3}')
                .line("rev")
                .into_bytes(),
            b"rev: unknown option -- \xc3\x83\n".to_vec(),
            "`line` 的 char 渲染比 C 多一个字节——A2 的射程"
        );
        // 高于 0xFF 的码位（只有解析那半还按字符取时才拿得到）退回 UTF-8，与 `line` 同串——
        // 所以「换渲染」这一轮不会改动还没「换取法」的壳的报文。
        assert_eq!(
            GetoptReport::UnknownOption('\u{fffd}').line_bytes("rev"),
            GetoptReport::UnknownOption('\u{fffd}')
                .line("rev")
                .into_bytes()
        );
    }

    /// 空 optstring 的字节形认得**非法 UTF-8 的单字节**：那一枚 `ff` 就是字母，
    /// 不走有损折叠那一条路（`args()` 会先把它折成 U+FFFD，字母就成了 `ef`）。
    #[test]
    fn empty_optstring_bytes_name_an_invalid_utf8_option_byte() {
        assert_eq!(
            empty_optstring_operands_bytes(&[&b"-\xff"[..]]),
            Err(GetoptReport::UnknownOption('\u{00ff}')),
            "裸字节 ff 应当在字母位上，而不是折成 U+FFFD"
        );
        assert_eq!(
            GetoptReport::UnknownOption('\u{00ff}').line_bytes("rev"),
            b"rev: unknown option -- \xff\n".to_vec()
        );
        // 文本缝的折叠对照形已随 `args()` 的删除而失去载体：那条腿不存在了，
        // A/B 里「字母位差一位」的残差自此无源。
    }

    /// 空 optstring 那一支报的是短横后的第一个**字节**（`&str` 形共件已随族终笔删除，
    /// 这张三形判据的钉子全部迁到字节形上）。
    #[test]
    fn empty_optstring_names_the_first_byte_of_the_flag() {
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-\xc3\xa9"]),
            Err(GetoptReport::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // 聚簇仍报第一个字节：ASCII 的 x 排在前面就落在 x 上。
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-x\xc3\xa9"]),
            Err(GetoptReport::UnknownOption('x'))
        );
        // ASCII 与三形（`--` 吃掉自身、裸 `-` 与 `--x`）不受这条改法影响。
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-Z"]),
            Err(GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            empty_optstring_operands_bytes(&[b"--x"]),
            Err(GetoptReport::UnknownOption('-'))
        );
        assert_eq!(
            empty_optstring_operands_bytes(&[b"--", b"f"]),
            Ok(vec![b"f".as_slice()])
        );
        assert_eq!(
            empty_optstring_operands_bytes(&[b"-"]),
            Ok(vec![b"-".as_slice()])
        );
    }
}
