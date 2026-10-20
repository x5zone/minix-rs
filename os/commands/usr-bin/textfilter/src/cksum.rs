//! Checksums for `cksum`: the POSIX CRC and the two historic sums.
//!
//! Ground truth: `minix3/usr.bin/cksum/`. The default algorithm is the
//! CRC of crc.c: a 256-entry MSB-first table over the polynomial
//! `0x04c11db7` (`crctab`, crc.c:53-108; entries verified reproducible
//! from the polynomial), fed byte-wise MSB-first
//! (`COMPUTE`, crc.c:123: `crc = crc << 8 ^ crctab[crc >> 24 ^ byte]`),
//! then the length little-endian low byte first
//! (crc.c:131-133), and the final value is the bitwise complement
//! (crc.c:135). Cross-checked byte-for-byte against the system
//! `cksum`: empty → `4294967295 0`, `"a"` → `1220704766 1`, `"abc"`
//! → `1219131554 3`, `"hello world"` → `1135714720 11`.
//!
//! `-o 1` selects the historic 16-bit rotating sum (`sum1.c:44-56`:
//! rotate right, add, mask to 16 bits); `-o 2` the additive sum folded
//! twice (`sum2.c:48-65`). C 的 `-a` 那八枚摘要算法（`cksum.c:120-144` 的 `hashes[]`）全在
//! `src/digest.rs` 里有本体，所以本模块只负责**选出哪一枚**与**排版**；C 里剩下的
//! `-s`/`-t`/`-x` 三条腿与 hash 在里的 `-c` 校验腿还没接，那两条腿的话由薄壳说。
//!
//! The table builds as a `const` from the polynomial — 256 entries with
//! no runtime cost, no transcription risk.

use crate::digest::Algo;
use alloc::vec::Vec;

/// One entry per byte value, MSB-first CRC-32 over the polynomial
/// 0x04c11db7 (the C `crctab`).
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut entry = (index as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            entry = if entry & 0x8000_0000 != 0 {
                (entry << 1) ^ 0x04c11db7
            } else {
                entry << 1
            };
            bit += 1;
        }
        table[index] = entry;
        index += 1;
    }
    table
};

/// The default `cksum` algorithm over the whole input: returns
/// `(crc, length)` for the `%u %lld` output line.
pub fn cksum_crc(data: &[u8]) -> (u32, u64) {
    let mut crc: u32 = 0;
    let mut length: u64 = 0;
    for &byte in data {
        length += 1;
        crc = (crc << 8) ^ CRC_TABLE[(crc >> 24) as usize ^ (byte as usize)];
    }
    // Include the length of the input, low byte first (crc.c:131-133).
    let mut remaining = length;
    while remaining != 0 {
        crc = (crc << 8) ^ CRC_TABLE[(crc >> 24) as usize ^ (remaining & 0xFF) as usize];
        remaining >>= 8;
    }
    (!crc, length)
}

/// `-o 1`: the historic 16-bit rotating sum (`csum1`, sum1.c:44-56).
pub fn csum1(data: &[u8]) -> (u32, u64) {
    let mut crc: u32 = 0;
    let mut length: u64 = 0;
    for &byte in data {
        length += 1;
        if crc & 1 != 0 {
            crc |= 0x10000;
        }
        crc = ((crc >> 1) + byte as u32) & 0xFFFF;
    }
    (crc, length)
}

/// `-o 2`: the additive sum folded twice into 16 bits
/// (`csum2`, sum2.c:48-66).
pub fn csum2(data: &[u8]) -> (u32, u64) {
    let mut crc: u32 = 0;
    let mut length: u64 = 0;
    for &byte in data {
        length += 1;
        crc = crc.wrapping_add(byte as u32);
    }
    crc = (crc & 0xFFFF) + (crc >> 16);
    crc = (crc & 0xFFFF) + (crc >> 16);
    (crc, length)
}

/// Which length the print step shows. The three algorithms share the value-and-
/// length output line but disagree on the length's unit (`print.c`: `pcrc`
/// prints the raw byte count, `psum1` rounds up to 1024-byte blocks, `psum2`
/// to 512-byte blocks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SumKind {
    /// `cksum` (default CRC): print the byte count verbatim.
    Crc,
    /// `-o 1` / `sum`: print 1024-byte blocks, rounded up.
    Old1,
    /// `-o 2`: print 512-byte blocks, rounded up.
    Old2,
}

/// The second column `print.c` emits for `kind` given the input `bytes`.
pub fn sum_display_length(kind: SumKind, bytes: u64) -> u64 {
    match kind {
        SumKind::Crc => bytes,
        // C `(len + 1023) / 1024` and `(len + 511) / 512`, i.e. round up.
        SumKind::Old1 => bytes.div_ceil(1024),
        SumKind::Old2 => bytes.div_ceil(512),
    }
}

/// C 的 `do_check`（`cksum.c:283-465`）对校验档里一行的分类结果。
///
/// 分类只用两条判据——这一行有没有 `(`，以及**当前的 `hash` 状态**——后一条会让同一形的两行
/// 走不同的路：`hash` 一旦被某一行认定（`hash = nhash`，`:363`），后面那些没有 `(` 的普通行
/// 也走摘要那一支（`:373-389`），拿第一个词整串去和重算的摘要比。参照件 h-1/h-3/h-4 三形
/// 实测都是这样判失败并带 `(MD5) ` 前缀，其中 h-4 **没有给 `-a`**：档里第一行的行首前缀
/// 自己就把算法定了。
#[derive(Debug, PartialEq, Eq)]
pub enum CheckLine<'a> {
    /// 摘要形 `NAME (档) = 摘要`。档名取「第一个 `(` 之后」（`strchr`，`:314`）到「最后一个
    /// `)` 之前」（`strrchr`，`:322`），所以 `MD5 (a (f1) = …` 的档名是 `a (f1`（参照件 g-7 形）、
    /// `MD5 (f1)) = …` 的档名是 `f1)`（g-6 形）。摘要从最后一个 `)` **之后第四个字符**起
    ///（`:331` 的 `p_cksum += 4`，跳的就是 `) = ` 那四个字符），因此 `) =x` 这种少一个空格的
    /// 写法取到空串、`)` 后面不足四个字符时也取到空串（g-3/g-4 两形实测都是 `FAILED`）。
    /// `algo` 是行首前缀在 `hashes[]` 里认出来的那一枚（大小写敏感，见 `Algo::by_line_prefix`）；
    /// `None` 就是 C 那句 `unknown hash: <整行>`（`:358`，参照件 g-1/g-5?/g-10/h-12 四形）。
    Hash {
        algo: Option<Algo>,
        name: &'a str,
        digest: &'a str,
    },
    /// 两列形 `<校验和> <档名>`。`token` 恒为第一个空格之前那一段（C 的两支都是，`:404` 与
    /// `:389`）；`name` 的取法**跟着 `hash` 状态分叉**：未选中时取**最后一个**空格之后
    ///（`strrchr`，`:406`），所以 `123 f1 x` 的档名是 `x`（参照件 h-6 形）；已选中时取开头那段
    /// 空白之后（`while (isspace((int)*++p_filename))`，`:386-387`），所以带空格的档名整串留下。
    Token { token: &'a str, name: &'a str },
    /// C 的 `bogus format: %s. Skipping...`：有 `(` 却没有 `)`（`:322-328`），
    /// 或者没有 `(` 也没有空格而 `hash` 又没选中（`:395-401`）。两见同一句。
    Malformed,
    /// C 的另一句 `no filename in %s? Skipping...`（`:377-383`）：**只有 `hash` 已选中那条路
    /// 才到得了**——同一形在 `hash` 未选中时报的是上一条（`:406-412` 那一支只在有空格但档名为空
    /// 时才走到，实测见参照件 h-7 对 h-8/h-9）。两句不同，`-w` 下的读数也就不同。
    NoFilename,
}

/// Parse the leading base-10 digits of `token`, mirroring C `strtoul(…, 10)`:
/// digits up to the first non-digit, `0` when it starts with a non-digit（`cksum.c:442`）。
/// 校验腿里只有 `hash` 未选中那一支用它；选中那一支比的是整串（`:424-426` 的 `strcmp`）。
pub fn leading_u32(token: &str) -> u32 {
    let digits: &str = {
        let end = token
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(token.len());
        &token[..end]
    };
    digits.parse::<u32>().unwrap_or(0)
}

/// 按 C 的行分类把校验档里的一行分开（换行已由调用方剥掉）。`hash` 是 C 那个**跨行留存**的
/// `hash` 指针，所以本函数必须带着当前状态调用；返回的 `Hash { algo: Some(..) }` 就是调用方
/// 接下来要采用的新状态（C 在同一处既改状态又用它判失败行的前缀，`:363` 与 `:454`）。
pub fn classify_check_line(line: &str, hash: Option<Algo>) -> CheckLine<'_> {
    if let Some(open) = line.find('(') {
        let rest = &line[open + 1..];
        let Some(close) = rest.rfind(')') else {
            return CheckLine::Malformed;
        };
        let after = &rest[close + 1..];
        CheckLine::Hash {
            algo: Algo::by_line_prefix(line),
            name: &rest[..close],
            // C 那句 `p_cksum += 4` 把指针从右括号本身推到摘要的第一个字符：`)`、` `、`=`、` `
            // 四步（`cksum.c:331`），而 `after` 已经跳过了 `)` 本身，所以这里剥掉的是三个字符。
            // 不足三个（`) ` 后面没有 ` = `）时 C 的指针走到串尾，取到的就是空串（参照件
            // g-3/g-4 两形实测判 `FAILED`），本壳同一口径。
            digest: if after.len() >= 3 { &after[3..] } else { "" },
        }
    } else if let Some(first_space) = line.find(' ') {
        let token = &line[..first_space];
        if hash.is_some() {
            // C 先把指针推过一个空格、再吃掉整段空白（`:386-387`），第一个非空白起就是档名。
            let bytes = line.as_bytes();
            let mut index = first_space;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            CheckLine::Token {
                token,
                name: &line[index..],
            }
        } else {
            // 未选中那一支用的是 `strrchr`（`:406`）：最后一个空格之后。
            let last_space = line.rfind(' ').unwrap_or(first_space);
            CheckLine::Token {
                token,
                name: &line[last_space + 1..],
            }
        }
    } else if hash.is_some() {
        CheckLine::NoFilename
    } else {
        CheckLine::Malformed
    }
}

/// Everything `cksum` was asked to do, folded out of argv by the C `main`
/// getopt loop (`cksum.c:193-279`, optstring `"a:cno:pqs:twx"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CksumRequest<'a> {
    /// Which sum to compute (`-a`/`-o` fold this; default `Crc`).
    pub kind: SumKind,
    /// C's `const struct hash *hash` (`cksum.c:162`): the algorithm `-a` selected, or
    /// `None` while the CRC/old-sum path is in force. 名字表全仓只有一份，在 `src/digest.rs` 的
    /// `Algo::from_hashname`；要那句大写的名字时用 `Algo::hashname()`。
    /// Two of C's diagnostics are driven by this state alone — `-a` a second time
    /// (`cksum.c:195-199`) and `-o` after a selection (`:228-234`) — and both
    /// `usage()` before any digest is computed, so they are reachable without the
    /// primitives themselves.
    pub hash: Option<Algo>,
    /// `-c`: verify a check file rather than print sums.
    pub do_check: bool,
    /// `-w`: warn on each bogus/unreadable check line.
    pub check_warn: bool,
    /// `-n`: `PRINT_NORMAL`. C folds it into `print_flags`, which only shapes
    /// the crypto-hash output; it is accepted here so the flag is not mistaken
    /// for a file operand, and has no effect on the CRC/old-sum path.
    pub print_normal: bool,
    /// `-q`: `PRINT_QUIET`, same hash-output-only role as `-n`.
    pub print_quiet: bool,
    /// C 的 `pflag`（`cksum.c:247-251`，`:250` 置真）：选中算法之后它让标准输入那一腿
    /// 把读到的字节**原样转抄**到标准输出，再在末尾印摘要（`md5.c` 的 `MD5Filter` 里
    /// `if (pipe && (len != fwrite(buffer, 1, len, stdout))) err(1, "stdout")`）。
    /// 只在算法已选中时才可能被读到这里，所以它不进诊断分支。
    pub pipe: bool,
    /// C 的 `nohashstdin`（`cksum.c:258`、`:264`、`:273`）：`-s`/`-t`/`-x` 任一出现即置真。
    /// 它同时是两件事——「这一条腿自己已经把话印完了」与「末尾那一次对标准输入的摘要
    /// 不要再做」（`cksum.c:489` 的 `else if (hash && !nohashstdin)`）。
    pub nohash_stdin: bool,
    /// The file operands (or `-`/stdin marker), in argv order. C hands `argv`
    /// to `filefunc`/`hash_digest_file` as raw bytes, so these are the bytes the
    /// shell received, not a decoded view of them.
    pub operands: Vec<&'a [u8]>,
}

/// Why argv could not be folded into a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CksumError {
    /// 带值选项（`a`/`o`/`s`）后面什么都没有：libc `getopt` 的
    /// `option requires an argument -- <字母>`（`getopt.c:132-136`），随后接 usage。
    MissingValue(char),
    /// `-a`/`-o` 的值不是可认的名字：C 是 `warnx("illegal argument to -a|-o option")` + `usage()`
    ///（`cksum.c:219`、`:243`），**不是**一句自创的「invalid algorithm」，也不退 2。
    IllegalOptionValue(char),
    /// 选中之后再次 `-a`：C 的
    /// `warnx("illegal use of -a option")` 再接 `usage()`（`cksum.c:195-199`）。
    IllegalUseOfA,
    /// `-o` 在一个 hash 已被选中之后出现：C 的
    /// `warnx("%s mutually exclusive with sum", hash->hashname)` 再接 `usage()`
    ///（`cksum.c:228-234`），句子里是大写的那个名字。
    MutuallyExclusiveWithSum(&'static str),
    /// `-p`/`-s`/`-t`/`-x` call `requirehash` (`cksum.c:149`), which the
    /// CRC/old-sum path never satisfies. C 的那句是 `warnx("%s flag requires `-a algorithm'", flg)`
    /// 再接 usage（`cksum.c:521-525`）。
    RequiresHash(char),
    /// A flag outside C's optstring; C routes it to `usage()`.
    UnknownOption(char),
}

/// C 的 `-s`/`-t`/`-x` 三条**自己在 getopt 环里就把话印完**的腿（`cksum.c:255-274` 分别
/// 调 `stringfunc`/`timetrialfunc`/`testsuitefunc`，三者的本体都在 `minix3/usr.bin/cksum/md5.c`，
/// 八枚算法靠 `#include "md5.c"` 共用）。带上算法是因为 C 用的就是当时那个 `hash`：
/// `-p`/`-s`/`-t`/`-x` 都要先过 `requirehash`，所以走到这里时算法必定已选中。
///
/// 次序跟着 argv 走，而且**后面选项报错也追不回来**：参照件 e-1 形
/// （`-a md5 -s abc -Z`）的 stdout 有那一行串摘要，stderr 是 libc 的 `unknown option`，退码 1。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CksumAction<'a> {
    /// `-s <串>`：C 的 `MD5String`（`md5.c:60-67`），一行 `NAME ("<串>") = <摘要>`。
    /// 串是 argv 的原字节——C 把它直接交给 `printf("…\"%s\"…")`，不问编码。
    String(Algo, &'a [u8]),
    /// `-t`：C 的 `MD5TimeTrial`（`md5.c:73-112`），四行；后两行随墙钟变。
    TimeTrial(Algo),
    /// `-x`：C 的 `MD5TestSuite`（`md5.c:118-133`），`NAME test suite:` 加八行固定串。
    TestSuite(Algo),
}

/// Fold argv into a [`CksumRequest`]，按 libc `getopt` 的形状扫（`cksum.c:193` 的
/// `"a:cno:pqs:twx"`）：一个 token 里可以聚簇多个字母（`-nq`），带值字母的值可以是
/// 本 token 的剩余部分（`-ao`、`-sabc`）也可以是下一词（`-a md5`），`--` 结束扫描，
/// 裸 `-` 是操作数。旧写法把整个 token 当一个字母，`-nq` 会被当成集外字母 `nq`。
///
/// `-s`/`-t`/`-x` 这三条腿在 C 里是**边扫边印**的（`cksum.c:255-274` 直接调
/// `stringfunc`/`timetrialfunc`/`testsuitefunc`），于是它们的先后跟着 argv，而且后面
/// 某个选项报错也拦不住已经印出去的那几行。采集面因此放在这里交给调用方，而不是
/// 塞进 [`CksumRequest`]：报错那一条路上没有 request 可挂。
fn name_matches_ignore_case(value: &[u8], want: &[u8]) -> bool {
    value.len() == want.len()
        && value
            .iter()
            .zip(want)
            .all(|(got, expected)| got.eq_ignore_ascii_case(expected))
}

pub fn parse_cksum_args_into<'a>(
    args: &[&'a [u8]],
    actions: &mut Vec<CksumAction<'a>>,
) -> Result<CksumRequest<'a>, CksumError> {
    let mut request = CksumRequest {
        kind: SumKind::Crc,
        hash: None,
        do_check: false,
        check_warn: false,
        print_normal: false,
        print_quiet: false,
        pipe: false,
        nohash_stdin: false,
        operands: Vec::new(),
    };
    let mut index = 0;
    let mut scanning = true;
    while index < args.len() {
        let arg = args[index];
        // A lone `-`, an empty string, or anything not starting with `-` is a
        // file operand (C getopt leaves a bare `-` to `argv[optind..]`).
        if !scanning || arg.len() <= 1 || arg[0] != b'-' {
            request.operands.push(arg);
            index += 1;
            continue;
        }
        if arg == b"--" {
            scanning = false;
            index += 1;
            continue;
        }
        let bytes = arg;
        let mut pos = 1usize;
        while pos < bytes.len() {
            let flag = bytes[pos] as char;
            // 带值字母的取值：本 token 剩余部分优先，否则取下一词；两处都没有就是
            // libc 的缺值那一行（旧写法把它当成「值为空」再报一出自创句）。
            // 带值字母的取值：本 token 剩余部分优先（`-amd5`），否则取下一词（`-a md5`）；
            // 两处都没有就是 libc 的缺值那一行（旧写法把它当成「值为空」再报一出自创句）。
            // 只有真取了**另一个** token 时才推进下标——当前这个 token 由循环尾统一推进。
            // 两处都推就会多吞一个词：`-a crc file` 于是把 `file` 跳过去，转去读标准输入。
            let take_value = |parser_index: &mut usize| -> Result<&'a [u8], CksumError> {
                if pos + 1 < bytes.len() {
                    return Ok(&bytes[pos + 1..]);
                }
                match args.get(*parser_index + 1) {
                    Some(word) => {
                        *parser_index += 1;
                        Ok(word)
                    }
                    None => Err(CksumError::MissingValue(flag)),
                }
            };
            match flag {
                'o' => {
                    let value = take_value(&mut index)?;
                    // C 在 `case 'o'` 的第一行就查 `hash`（`cksum.c:228-234`），值本身
                    // 已被 getopt 吃掉，所以缺值那一行仍然先由 libc 印。
                    if let Some(algo) = request.hash {
                        return Err(CksumError::MutuallyExclusiveWithSum(algo.hashname()));
                    }
                    request.kind = if value == b"1".as_slice() {
                        SumKind::Old1
                    } else if value == b"2".as_slice() {
                        SumKind::Old2
                    } else {
                        return Err(CksumError::IllegalOptionValue('o'));
                    };
                    break;
                }
                'a' => {
                    let value = take_value(&mut index)?;
                    // C 的 `case 'a'` 先看 `hash` 是否已被上一次选中（`cksum.c:195-199`），
                    // 选中过就是 `illegal use of -a option`——哪怕这一枚的名字本身合法。
                    if request.hash.is_some() {
                        return Err(CksumError::IllegalUseOfA);
                    }
                    // 然后才按 `strcasecmp` 查名字表（`:200-207`），查不到才试
                    // `old1`/`old2`/`crc`（`:209-224`），再不行是 `illegal argument to -a option`。
                    if let Some(algo) = Algo::from_hashname_bytes(value) {
                        request.hash = Some(algo);
                    } else if name_matches_ignore_case(value, b"crc") {
                        request.kind = SumKind::Crc;
                    } else if name_matches_ignore_case(value, b"old1") {
                        request.kind = SumKind::Old1;
                    } else if name_matches_ignore_case(value, b"old2") {
                        request.kind = SumKind::Old2;
                    } else {
                        return Err(CksumError::IllegalOptionValue('a'));
                    }
                    break;
                }
                's' => {
                    // `-s` 先过 `requirehash`（`cksum.c:255-259` + `:521-525`）：没选中才报那句
                    // 并要求 `-a algorithm`。值本身已由 getopt 取走，所以缺值那一行仍先由 libc 印。
                    let value = take_value(&mut index)?;
                    let Some(algo) = request.hash else {
                        return Err(CksumError::RequiresHash('s'));
                    };
                    // 放行之后 C **当场**调 `stringfunc(optarg)`（`cksum.c:258-259`）并把
                    // `nohashstdin` 置真（`:258`）。`nohashstdin` 不是「这条腿做完了」的记号，
                    // 而是「末尾那一次对标准输入的摘要别再做了」的记号（`:489`）。
                    actions.push(CksumAction::String(algo, value));
                    request.nohash_stdin = true;
                    // 本 token 的剩余部分一旦当了值，这个 token 就扫完了（libc 把本 token 的剩余部分当值交回之后
                    // 就把待扫指针清空（`getopt.c:116-117` 与 `:140` 的 `place = EMSG`）。`-a`/`-o` 两臂本来就 break，
                    // 这一臂以前不 break：`-sabc` 于是把 `a`、`b`、`c` 又当旗标扫一遍，
                    // 撞上 `case 'a'` 去取「bc」当算法名——参照件 s-5 形是 `MD5 ("abc") = …`，
                    // 本壳旧读数是一条 `illegal argument to -a option`。本轮新门把它咬出来了。
                    break;
                }
                'c' => request.do_check = true,
                'w' => request.check_warn = true,
                'n' => request.print_normal = true,
                'q' => request.print_quiet = true,
                'p' => {
                    // `-p` 同样过 `requirehash`，但它**不**动 `nohashstdin`（`cksum.c:247-251`
                    // 里只有 `pflag = 1`）：转抄标准输入那一腿照旧要跑。
                    if request.hash.is_none() {
                        return Err(CksumError::RequiresHash('p'));
                    }
                    request.pipe = true;
                }
                't' | 'x' => {
                    // 这两个也过 `requirehash`，放行后 C 当场调 `timetrialfunc`/`testsuitefunc`
                    //（`cksum.c:261-266`、`:270-275`）并各自置 `nohashstdin`：定时试验与测试
                    // 套件自己就把话印完了，末尾那一次标准输入摘要不再做。
                    let Some(algo) = request.hash else {
                        return Err(CksumError::RequiresHash(flag));
                    };
                    actions.push(if flag == 't' {
                        CksumAction::TimeTrial(algo)
                    } else {
                        CksumAction::TestSuite(algo)
                    });
                    request.nohash_stdin = true;
                }
                _ => return Err(CksumError::UnknownOption(flag)),
            }
            pos += 1;
        }
        index += 1;
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这些门判的是**状态折叠**（哪个字母把哪个字段改成什么），不关心三条自印腿的打印
    /// 次序；采集面自己的门直接叫 `parse_cksum_args_into`。
    fn parse_cksum_args<'a>(args: &[&'a [u8]]) -> Result<CksumRequest<'a>, CksumError> {
        parse_cksum_args_into(args, &mut Vec::new())
    }

    /// 三条自印腿按 **argv 次序**进采集面，且 `-x -s abc` 这种同现不合并、不重排
    ///（参照件 s-9 形：先 `MD5 test suite:` 那九行，再 `MD5 ("abc") = …`）。
    #[test]
    fn the_self_printing_legs_are_collected_in_argv_order() {
        let mut actions = Vec::new();
        assert!(
            parse_cksum_args_into(
                &[
                    "-a".as_bytes(),
                    "md5".as_bytes(),
                    "-x".as_bytes(),
                    "-s".as_bytes(),
                    "abc".as_bytes()
                ],
                &mut actions
            )
            .is_ok()
        );
        assert_eq!(
            actions,
            [
                CksumAction::TestSuite(Algo::Md5),
                CksumAction::String(Algo::Md5, "abc".as_bytes())
            ],
            "`-x` 在 argv 里在前，采集就必须在先"
        );

        // 重复出现就重复采集：C 的 getopt 环每命中一次调一次（参照件 x-5 形印了两套十六行）。
        let mut twice = Vec::new();
        assert!(
            parse_cksum_args_into(
                &[
                    "-a".as_bytes(),
                    "sha1".as_bytes(),
                    "-x".as_bytes(),
                    "-x".as_bytes()
                ],
                &mut twice
            )
            .is_ok()
        );
        assert_eq!(
            twice,
            [
                CksumAction::TestSuite(Algo::Sha1),
                CksumAction::TestSuite(Algo::Sha1)
            ]
        );

        // 附值与聚簇两形取到的值都是那一串：`-sabc` 与 `-sa` 都印 `("abc")`/`("a")`。
        let mut inline = Vec::new();
        assert!(
            parse_cksum_args_into(
                &["-a".as_bytes(), "md5".as_bytes(), "-sabc".as_bytes()],
                &mut inline
            )
            .is_ok()
        );
        assert_eq!(inline, [CksumAction::String(Algo::Md5, "abc".as_bytes())]);

        // 三条腿都动 `nohashstdin`，而 `-p` 不动（`cksum.c:247-251` 里只有 `pflag = 1`）。
        assert!(
            parse_cksum_args_into(
                &["-a".as_bytes(), "md5".as_bytes(), "-t".as_bytes()],
                &mut Vec::new()
            )
            .is_ok()
        );
        assert!(
            !parse_cksum_args(&["-a".as_bytes(), "md5".as_bytes(), "-p".as_bytes()])
                .unwrap()
                .nohash_stdin
        );
    }

    /// 报错那一形也要把已经印出去的行留下：C 在 getopt 环里就调了 `stringfunc`，
    /// 之后的 `usage()` 追不回来（参照件 e-1/e-2/e-3 三形的 stdout 都有 `MD5 ("abc") = …`
    /// 那一行，退码仍是 1）。采集面写在返回之前，正是为了这条路径。
    #[test]
    fn actions_survive_a_later_option_error() {
        for tail in [
            vec!["-Z".as_bytes()],
            vec!["-o".as_bytes(), "1".as_bytes()],
            vec!["-a".as_bytes(), "sha1".as_bytes()],
        ] {
            let mut actions = Vec::new();
            let mut argv = vec![
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-s".as_bytes(),
                "abc".as_bytes(),
            ];
            argv.extend(tail.iter());
            assert!(
                parse_cksum_args_into(&argv, &mut actions).is_err(),
                "{argv:?} 该报错"
            );
            assert_eq!(
                actions,
                [CksumAction::String(Algo::Md5, "abc".as_bytes())],
                "{argv:?} 报错之前那一行已经印出去了"
            );
        }
    }

    #[test]
    fn test_crc_table_matches_the_polynomial() {
        // Spot checks against the C crctab (crc.c:53-108): entries 0, 1,
        // 2, 255.
        assert_eq!(CRC_TABLE[0], 0x0);
        assert_eq!(CRC_TABLE[1], 0x04c11db7);
        assert_eq!(CRC_TABLE[2], 0x09823b6e);
        assert_eq!(CRC_TABLE[255], 0xb1f740b4);
    }

    #[test]
    fn test_cksum_known_values_from_the_system() {
        // Cross-checked against the system cksum on this host.
        assert_eq!(cksum_crc(b""), (4294967295, 0));
        assert_eq!(cksum_crc(b"a"), (1220704766, 1));
        assert_eq!(cksum_crc(b"abc"), (1219131554, 3));
        assert_eq!(cksum_crc(b"hello world"), (1135714720, 11));
    }

    #[test]
    fn test_sum_display_length_block_rounding() {
        // CRC shows bytes; old1 rounds to 1024-blocks, old2 to 512-blocks.
        assert_eq!(sum_display_length(SumKind::Crc, 11), 11);
        assert_eq!(sum_display_length(SumKind::Old1, 0), 0);
        assert_eq!(sum_display_length(SumKind::Old1, 1), 1); // (1+1023)/1024
        assert_eq!(sum_display_length(SumKind::Old1, 1024), 1);
        assert_eq!(sum_display_length(SumKind::Old1, 1025), 2);
        assert_eq!(sum_display_length(SumKind::Old2, 511), 1);
        assert_eq!(sum_display_length(SumKind::Old2, 512), 1);
        assert_eq!(sum_display_length(SumKind::Old2, 513), 2);
    }

    #[test]
    fn test_classify_check_line_sum_and_cksum_format() {
        // "<crc> <length> <name>" — length is present but not compared by C.
        assert_eq!(
            classify_check_line("1219131554 3 abc", None),
            CheckLine::Token {
                token: "1219131554",
                name: "abc"
            }
        );
        // Two tokens: first is the crc, the rest-after-last-space is the name.
        assert_eq!(
            classify_check_line("4294967295 a.txt", None),
            CheckLine::Token {
                token: "4294967295",
                name: "a.txt"
            }
        );
    }

    /// `hash` 未选中那一支用 `strrchr`（`cksum.c:406`）取档名，所以带空格的名字被截成
    /// 最后一个词；而 `hash` 已选中那一支用「跳过开头那段空白」（`:386-387`），整串留着。
    /// 同一行两判，是参照件 h-5/h-6 两形的分别。
    #[test]
    fn the_filename_rule_follows_the_hash_state() {
        assert_eq!(
            classify_check_line("123 456 my file.txt", None),
            CheckLine::Token {
                token: "123",
                name: "file.txt"
            },
            "未选中取最后一个空格之后"
        );
        assert_eq!(
            classify_check_line("123 456 my file.txt", Some(Algo::Md5)),
            CheckLine::Token {
                token: "123",
                name: "456 my file.txt"
            },
            "选中之后取开头那段空白之后"
        );
        // 多个空格只影响 C 的 `nspaces`，`token` 仍是第一个空格之前那一段（`:389`）。
        assert_eq!(
            classify_check_line("12345    f1", None),
            CheckLine::Token {
                token: "12345",
                name: "f1"
            }
        );
        assert_eq!(
            classify_check_line("12345    f1", Some(Algo::Sha1)),
            CheckLine::Token {
                token: "12345",
                name: "f1"
            }
        );
    }

    /// 摘要形 `NAME (档) = 摘要`：C 的档名是「第一个 `(` 之后」到「最后一个 `)` 之前」，
    /// 摘要是最后一个 `)` 之后第四个字符起（`cksum.c:314`/`:322`/`:331`）。三处畸形都实测过：
    /// 少一个空格、`=` 后空、行首前缀认不出。
    #[test]
    fn the_hash_grammar_splits_at_the_first_open_and_last_close() {
        assert_eq!(
            classify_check_line("MD5 (f1) = d41d8cd98f00b204e9800998ecf8427e", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1",
                digest: "d41d8cd98f00b204e9800998ecf8427e"
            }
        );
        // 档名里带括号：前取到最后一个 `)` 之前（参照件 g-6/g-7 两形）。
        assert_eq!(
            classify_check_line("MD5 (a (f1) = xyz", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "a (f1",
                digest: "xyz"
            }
        );
        assert_eq!(
            classify_check_line("MD5 (f1)) = xyz", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1)",
                digest: "xyz"
            }
        );
        // `) =x` 这种少一个空格的写法：C 那句 `+= 4` 是不看空格的硬位移，四个字符数过去
        // 已经越过 `x` 本身，取到的仍是空串（参照件 g-3 形实测判 `FAILED`）。
        assert_eq!(
            classify_check_line("MD5 (f1) =x", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1",
                digest: ""
            }
        );
        // `)` 后面不足 ` = `：C 的指针走到串尾，取到空串（g-4 形实测 `FAILED`）。
        assert_eq!(
            classify_check_line("MD5 (f1)", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1",
                digest: ""
            }
        );
        // 行首前缀是**大小写敏感**的（`strncmp`，`:343-352`）：小写 `md5` 认不出，
        // 与 `-a` 那一腿的 `strcasecmp` 正相反（参照件 g-10 形）。
        assert_eq!(
            classify_check_line("md5 (f1) = d41d8cd98f00b204e9800998ecf8427e", None),
            CheckLine::Hash {
                algo: None,
                name: "f1",
                digest: "d41d8cd98f00b204e9800998ecf8427e"
            }
        );
        // 有 `(` 没有 `)` 走 `bogus format`（`:322-328`）。
        assert_eq!(
            classify_check_line("MD5 (f1 = abc", None),
            CheckLine::Malformed
        );
    }

    /// 「没有空格」这一形在两条路上报的是**两句不同的话**：`hash` 已选中时
    /// `no filename in %s?`（`:377-383`），未选中时 `bogus format: %s.`（`:395-401`）。
    /// 参照件 h-7 对 h-8/h-9 的分别正在这里。
    #[test]
    fn a_spaceless_line_reports_two_different_sentences_depending_on_state() {
        assert_eq!(classify_check_line("12345", None), CheckLine::Malformed);
        assert_eq!(
            classify_check_line("12345", Some(Algo::Md5)),
            CheckLine::NoFilename
        );
        assert_eq!(classify_check_line("", None), CheckLine::Malformed);
        assert_eq!(
            classify_check_line("", Some(Algo::Md5)),
            CheckLine::NoFilename
        );
    }

    #[test]
    fn leading_u32_follows_strtoul() {
        // strtoul stops at the first non-digit; a non-numeric start yields 0.
        assert_eq!(leading_u32("1234abc"), 1234);
        assert_eq!(leading_u32("abc"), 0);
        assert_eq!(leading_u32(""), 0);
        assert_eq!(leading_u32("4294967295"), 4294967295);
    }

    #[test]
    fn test_parse_cksum_args_keeps_options_out_of_operands() {
        // `-n`/`-q` are C print modes (no-op on the crc path) and must be
        // recognised, not pushed as file operands — the previous parser leaked
        // them into the file list.
        let req = parse_cksum_args(&["-n".as_bytes(), "-q".as_bytes(), "f".as_bytes()]).unwrap();
        assert!(req.print_normal);
        assert!(req.print_quiet);
        assert_eq!(req.operands, ["f".as_bytes()]);
    }

    #[test]
    fn test_parse_cksum_args_requires_hash_flags() {
        // `-p/-s/-t/-x` call requirehash; they error instead of leaking as files.
        assert_eq!(
            parse_cksum_args(&["-p".as_bytes()]),
            Err(CksumError::RequiresHash('p'))
        );
        assert_eq!(
            parse_cksum_args(&["-x".as_bytes(), "y".as_bytes()]),
            Err(CksumError::RequiresHash('x'))
        );
    }

    #[test]
    fn test_parse_cksum_args_a_and_o_algorithms() {
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "crc".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Crc
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "OLD1".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Old1
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "old2".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Old2
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "md5".as_bytes()])
                .unwrap()
                .hash,
            Some(Algo::Md5),
            "选中记的是 C 表里的 hashname（大写那一份，`cksum.c:126`）"
        );
        // 表里的八项逐个都要认得（`cksum.c:120-144`），且按 `strcasecmp` 匹配：
        // 名字的大小写不影响选中，但报文里用的是大写那一份。
        for (word, name) in [
            ("md2", "MD2"),
            ("MD4", "MD4"),
            ("md5", "MD5"),
            ("rmd160", "RMD160"),
            ("sha1", "SHA1"),
            ("SHA256", "SHA256"),
            ("sha384", "SHA384"),
            ("sha512", "SHA512"),
        ] {
            assert_eq!(
                parse_cksum_args(&["-a".as_bytes(), word.as_bytes()])
                    .unwrap()
                    .hash
                    .map(Algo::hashname),
                Some(name),
                "`-a {word}` 该选中诊断与报文里那一个大写的 {name}"
            );
        }
        // 表里没有的拼法不是别名：C 没有 `ripemd160` 这一行，所以它落到
        // `illegal argument to -a option`（`cksum.c:201-224` 的两级查找之后）。
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "ripemd160".as_bytes()]),
            Err(CksumError::IllegalOptionValue('a'))
        );
        // 选中之后再 `-a`：C 的 `case 'a'` 第一行就是这条（`cksum.c:195-199`），
        // 第二枚的名字好不好都在所不问——`crc` 也一样被这条挡住。
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-a".as_bytes(),
                "sha1".as_bytes()
            ]),
            Err(CksumError::IllegalUseOfA)
        );
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-a".as_bytes(),
                "crc".as_bytes()
            ]),
            Err(CksumError::IllegalUseOfA)
        );
        // 选中之后再 `-o`：句子带的是大写名字（`cksum.c:228-234`）。
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-o".as_bytes(),
                "1".as_bytes()
            ]),
            Err(CksumError::MutuallyExclusiveWithSum("MD5"))
        );
        // 反过来先 `-o` 再 `-a` 就不冲突：`hash` 这时还是空的。
        assert_eq!(
            parse_cksum_args(&[
                "-o".as_bytes(),
                "1".as_bytes(),
                "-a".as_bytes(),
                "md5".as_bytes()
            ])
            .unwrap()
            .hash,
            Some(Algo::Md5),
        );
        assert_eq!(
            parse_cksum_args(&[
                "-o".as_bytes(),
                "1".as_bytes(),
                "-a".as_bytes(),
                "md5".as_bytes()
            ])
            .unwrap()
            .kind,
            SumKind::Old1,
        );
        // 选中之后 `-p`/`-s` 不再报 `requires \`-a algorithm'`（C 的 requirehash 放行）；
        // 没选中时才报，这一形是批次三百零七钉下的。
        assert!(parse_cksum_args(&["-a".as_bytes(), "sha1".as_bytes(), "-p".as_bytes()]).is_ok());
        assert!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "sha1".as_bytes(),
                "-s".as_bytes(),
                "abc".as_bytes()
            ])
            .is_ok()
        );
        assert_eq!(
            parse_cksum_args(&["-p".as_bytes()]),
            Err(CksumError::RequiresHash('p'))
        );
        // 缺值仍由 libc 先报，即使此时已有 hash 选中（getopt 在进入 switch 之前就要值）。
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "md5".as_bytes(), "-o".as_bytes()]),
            Err(CksumError::MissingValue('o'))
        );
        // 带值选项**后面**的操作数不能被吞。旧的下标算术在取值处和循环尾各推一次，
        // 于是 `-a crc file` 会跳过 `file`，让本应算文件的 cksum 转去读标准输入——
        // 附值形 `-acrc file` 同病。这一组断言钉住「一个 token 只由循环尾推进一次」。
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "crc".as_bytes(), "file".as_bytes()])
                .unwrap()
                .operands,
            vec!["file".as_bytes()]
        );
        assert_eq!(
            parse_cksum_args(&["-acrc".as_bytes(), "file".as_bytes()])
                .unwrap()
                .operands,
            vec!["file".as_bytes()]
        );
        assert_eq!(
            parse_cksum_args(&[
                "-o".as_bytes(),
                "1".as_bytes(),
                "-a".as_bytes(),
                "sha1".as_bytes(),
                "f1".as_bytes(),
                "f2".as_bytes()
            ])
            .unwrap()
            .operands,
            vec!["f1".as_bytes(), "f2".as_bytes()]
        );
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "crc".as_bytes(),
                "-n".as_bytes(),
                "-".as_bytes()
            ])
            .unwrap()
            .operands,
            vec!["-".as_bytes()],
            "裸 `-` 仍是操作数，且不被前一个值吞掉"
        );
        assert_eq!(
            parse_cksum_args(&["-o".as_bytes(), "2".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Old2
        );
        // `-o 9` 是「值不合法」（C 的 `illegal argument to -o option`），
        // 而 `-o` 到量尽头没有值是另一回事（libc 的缺值那一行）。
        assert_eq!(
            parse_cksum_args(&["-o".as_bytes(), "9".as_bytes()]),
            Err(CksumError::IllegalOptionValue('o'))
        );
        assert_eq!(
            parse_cksum_args(&["-o".as_bytes()]),
            Err(CksumError::MissingValue('o'))
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "nosuch".as_bytes()]),
            Err(CksumError::IllegalOptionValue('a'))
        );
    }

    /// 集外字母交回的是短横后的**第一个字节**而不是第一个字符：`-\u{e9}`（`c3 a9`）在
    /// `char` 域里是 U+00C3。环那一侧本来就是 `bytes[pos] as char`（`getopt.c:79` 的
    /// `optopt = *place++` 一次进一个字节），所以这一枚钉的是**已经是那样**的取法，
    /// 免得将来有人把它改回按字符取；参照件 `tmp/cksum-hash-state/cksum_ref` 现量
    /// `-é` 报单个 `303`，逐字节门在 `tests/cksum_face.rs`。
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_cksum_args(&["-\u{e9}".as_bytes()]),
            Err(CksumError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // 聚簇：`n` 合法先吃掉，报的是其后第一字节。
        assert_eq!(
            parse_cksum_args(&["-n\u{e9}".as_bytes()]),
            Err(CksumError::UnknownOption('\u{00c3}')),
            "-n\u{e9}"
        );
        // 环停在第一个坏字母上。
        assert_eq!(
            parse_cksum_args(&["-Z\u{e9}".as_bytes()]),
            Err(CksumError::UnknownOption('Z')),
            "-Z\u{e9}"
        );
        // ASCII 那一形不许被波及。取 `z`：`cksum.c:193` 的选项串是 `"a:cno:pqs:twx"`，
        // 里面没有 `z`（本支的 `q` 反倒是合法字母，它自己那句被这枚门的第一版当过反例）。
        assert_eq!(
            parse_cksum_args(&["-z".as_bytes()]),
            Err(CksumError::UnknownOption('z'))
        );
        // 反面：带值字母把其后的字节整个当值收走，轮不到字母位（参照件同句
        // `cksum: illegal argument to -a option` 再接 usage）。
        assert_eq!(
            parse_cksum_args(&["-a\u{e9}".as_bytes()]),
            Err(CksumError::IllegalOptionValue('a'))
        );
    }

    /// getopt 的形状：聚簇、附值、`--` 后的全是操作数。旧写法把整个 token 当一个字母，
    /// `-nq`、`-o2`、`-a crc -- -f` 这几形都会错。
    #[test]
    fn test_parse_cksum_args_follows_the_getopt_shape() {
        let req = parse_cksum_args(&["-nq".as_bytes(), "f".as_bytes()]).unwrap();
        assert!(req.print_normal && req.print_quiet);
        assert_eq!(req.operands, vec!["f".as_bytes()]);
        // 附值形：`-o2` 与 `-o 2` 同效（`optarg` 就是本 token 的剩余部分）。
        assert_eq!(
            parse_cksum_args(&["-o2".as_bytes()]).unwrap().kind,
            parse_cksum_args(&["-o".as_bytes(), "2".as_bytes()])
                .unwrap()
                .kind
        );
        assert_eq!(
            parse_cksum_args(&["-acrc".as_bytes()]).unwrap().kind,
            SumKind::Crc
        );
        // `--` 之后的负向词是操作数，不被当旗标。
        let req = parse_cksum_args(&["-c".as_bytes(), "--".as_bytes(), "-o".as_bytes()]).unwrap();
        assert!(req.do_check);
        assert_eq!(req.operands, vec!["-o".as_bytes()]);
        // `-s` 先吃 getopt 的值（本 token 剩余或下一词），再落 `requirehash`；
        // 没有值时先报缺值而不是 requirehash。
        assert_eq!(
            parse_cksum_args(&["-s".as_bytes(), "x".as_bytes()]),
            Err(CksumError::RequiresHash('s'))
        );
        assert_eq!(
            parse_cksum_args(&["-s".as_bytes()]),
            Err(CksumError::MissingValue('s'))
        );
        // 聚簇里的集外字母报的是那个字母本身。
        assert_eq!(
            parse_cksum_args(&["-ncZ".as_bytes()]),
            Err(CksumError::UnknownOption('Z'))
        );
    }

    #[test]
    fn test_parse_cksum_args_check_warn_and_edge_forms() {
        let req =
            parse_cksum_args(&["-w".as_bytes(), "-c".as_bytes(), "ck.txt".as_bytes()]).unwrap();
        assert!(req.do_check);
        assert!(req.check_warn);
        assert_eq!(req.operands, ["ck.txt".as_bytes()]);
        assert_eq!(
            parse_cksum_args(&["-Z".as_bytes()]),
            Err(CksumError::UnknownOption('Z'))
        );
        // A bare `-` is a file/stdin operand, not an option letter.
        assert_eq!(
            parse_cksum_args(&["-".as_bytes()]).unwrap().operands,
            ["-".as_bytes()]
        );
    }

    #[test]
    fn test_cksum_is_the_bitwise_complement_of_the_running_crc() {
        // The C complements at the end (crc.c:135); an empty input's
        // running CRC is zero, so the result is all ones.
        assert_eq!(cksum_crc(b"").0, u32::MAX);
    }

    #[test]
    fn test_csum1_rotating_16_bit() {
        assert_eq!(csum1(b""), (0, 0));
        assert_eq!(csum1(b"a"), (97, 1));
        assert_eq!(csum1(b"abc\n"), (8288, 4));
        assert_eq!(csum1(b"hello world\n"), (3762, 12));
    }

    #[test]
    fn test_csum2_additive_folded() {
        assert_eq!(csum2(b""), (0, 0));
        assert_eq!(csum2(b"a"), (97, 1));
        assert_eq!(csum2(b"abc\n"), (304, 4));
        assert_eq!(csum2(b"hello world\n"), (1126, 12));
    }
}
