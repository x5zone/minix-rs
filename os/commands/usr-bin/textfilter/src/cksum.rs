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
//! twice (`sum2.c:48-65`). The MD5/SHA family of the C (`-a` algorithms)
//! needs crypto primitives and is declared unsupported.
//!
//! The table builds as a `const` from the polynomial — 256 entries with
//! no runtime cost, no transcription risk.

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

/// One classified line of a `cksum -c` check file. C's `do_check`
/// (`cksum.c:283-460`) reads a line, strips the newline, and branches on
/// whether it carries the crypto-hash grammar (`… (file) = digest`) or the
/// sum/cksum grammar (`<checksum> [length] name`).
#[derive(Debug, PartialEq, Eq)]
pub enum CheckEntry<'a> {
    /// sum/cksum line: C takes the checksum from the first token (`buf` up to
    /// the first space) and the file name from after the **last** space
    /// (`strrchr(buf, ' ') + 1`); the printed length column is not compared —
    /// C checks only `val == strtoul(cksum)` (`cksum.c:443`). Names containing
    /// spaces therefore resolve to their trailing word, matching C.
    Checksum { crc: u32, name: &'a str },
    /// The line contains `(` — the `name (file) = digest` hash grammar. It
    /// needs a crypto primitive we do not carry, so it cannot be verified.
    Hash,
    /// No space to split on — C's "bogus format: … Skipping…" case.
    Malformed,
}

/// Parse the leading base-10 digits of `token`, mirroring C `strtoul(…, 10)`:
/// digits up to the first non-digit, `0` when it starts with a non-digit.
fn leading_u32(token: &str) -> u32 {
    let digits: &str = {
        let end = token
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(token.len());
        &token[..end]
    };
    digits.parse::<u32>().unwrap_or(0)
}

/// Classify one check-file `line` (the trailing newline already removed).
pub fn parse_check_line(line: &str) -> CheckEntry<'_> {
    if line.contains('(') {
        return CheckEntry::Hash;
    }
    let Some(first_space) = line.find(' ') else {
        return CheckEntry::Malformed;
    };
    let crc = leading_u32(&line[..first_space]);
    let name = &line[line.rfind(' ').unwrap() + 1..];
    CheckEntry::Checksum { crc, name }
}

/// Everything `cksum` was asked to do, folded out of argv by the C `main`
/// getopt loop (`cksum.c:193-279`, optstring `"a:cno:pqs:twx"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CksumRequest<'a> {
    /// Which sum to compute (`-a`/`-o` fold this; default `Crc`).
    pub kind: SumKind,
    /// C's `const struct hash *hash` (`cksum.c:162`): the `hashname` of the crypto
    /// algorithm `-a` selected, or `None` while the CRC/old-sum path is in force.
    /// Two of C's diagnostics are driven by this state alone — `-a` a second time
    /// (`cksum.c:195-199`) and `-o` after a selection (`:228-234`) — and both
    /// `usage()` before any digest is computed, so they are reachable without the
    /// primitives themselves.
    pub hash: Option<&'static str>,
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
    /// The file operands (or `-`/stdin marker), in argv order.
    pub operands: Vec<&'a str>,
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

/// Fold argv into a [`CksumRequest`]，按 libc `getopt` 的形状扫（`cksum.c:193` 的
/// `"a:cno:pqs:twx"`）：一个 token 里可以聚簇多个字母（`-nq`），带值字母的值可以是
/// 本 token 的剩余部分（`-ao`、`-sabc`）也可以是下一词（`-a md5`），`--` 结束扫描，
/// 裸 `-` 是操作数。旧写法把整个 token 当一个字母，`-nq` 会被当成集外字母 `nq`。
pub fn parse_cksum_args<'a>(args: &[&'a str]) -> Result<CksumRequest<'a>, CksumError> {
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
        if !scanning || arg.len() <= 1 || !arg.starts_with('-') {
            request.operands.push(arg);
            index += 1;
            continue;
        }
        if arg == "--" {
            scanning = false;
            index += 1;
            continue;
        }
        let bytes = arg.as_bytes();
        let mut pos = 1usize;
        while pos < bytes.len() {
            let flag = bytes[pos] as char;
            // 带值字母的取值：本 token 剩余部分优先，否则取下一词；两处都没有就是
            // libc 的缺值那一行（旧写法把它当成「值为空」再报一出自创句）。
            // 带值字母的取值：本 token 剩余部分优先（`-amd5`），否则取下一词（`-a md5`）；
            // 两处都没有就是 libc 的缺值那一行（旧写法把它当成「值为空」再报一出自创句）。
            // 只有真取了**另一个** token 时才推进下标——当前这个 token 由循环尾统一推进。
            // 两处都推就会多吞一个词：`-a crc file` 于是把 `file` 跳过去，转去读标准输入。
            let take_value = |parser_index: &mut usize| -> Result<&'a str, CksumError> {
                if pos + 1 < bytes.len() {
                    return Ok(&arg[pos + 1..]);
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
                    if let Some(name) = request.hash {
                        return Err(CksumError::MutuallyExclusiveWithSum(name));
                    }
                    request.kind = match value {
                        "1" => SumKind::Old1,
                        "2" => SumKind::Old2,
                        _ => return Err(CksumError::IllegalOptionValue('o')),
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
                    if let Some(name) = crypto_hashname(value) {
                        request.hash = Some(name);
                    } else if value.eq_ignore_ascii_case("crc") {
                        request.kind = SumKind::Crc;
                    } else if value.eq_ignore_ascii_case("old1") {
                        request.kind = SumKind::Old1;
                    } else if value.eq_ignore_ascii_case("old2") {
                        request.kind = SumKind::Old2;
                    } else {
                        return Err(CksumError::IllegalOptionValue('a'));
                    }
                    break;
                }
                's' => {
                    // `-s` 先过 `requirehash`（`cksum.c:521-525`）：没选中 hash 才报那句
                    // 并要求 `-a algorithm`；选中了则继续走，值本身要等真摘要时用。
                    let _ = take_value(&mut index)?;
                    if request.hash.is_none() {
                        return Err(CksumError::RequiresHash('s'));
                    }
                    // 放行之后 C 立刻置 `nohashstdin`（`cksum.c:258`）。它不是「这条腿做完
                    // 了」的记号，而是「末尾那一次对标准输入的摘要别再做了」的记号——
                    // `-s` 已经把这一串摘要印过了。
                    request.nohash_stdin = true;
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
                    // 这两个也过 `requirehash`，放行后各自置 `nohashstdin`（`cksum.c:264`、
                    // `:273`）：定时试验与测试套件自己就把话印完了。
                    if request.hash.is_none() {
                        return Err(CksumError::RequiresHash(flag));
                    }
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

/// The `hashname` C would carry in `hash` once `-a <value>` matches its table, or
/// `None` when the name is not in it. The table is `minix3/usr.bin/cksum/cksum.c:120-144`
/// — eight entries, matched with `strcasecmp` against the lower-case `progname`
/// (`cksum.c:201-207`) while the diagnostics quote the **upper-case** `hashname`
/// (`hash->hashname`, `cksum.c:231-234`). Names outside these eight are not aliases:
/// C has no `ripemd160` row, so that spelling falls through to
/// `illegal argument to -a option`. 名字与摘要各归两道缝：这里只管「选中了哪一个」，
/// 摘要本体在 `src/digest.rs`（八枚里已接六枚，`md2` 与 `rmd160` 在 `CKSUM-HASH-DIGEST`
/// 的第三片；`-s`/`-t`/`-x` 三条腿的句子归 `CKSUM-HASH-STATE`）。
fn crypto_hashname(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "md2" => Some("MD2"),
        "md4" => Some("MD4"),
        "md5" => Some("MD5"),
        "rmd160" => Some("RMD160"),
        "sha1" => Some("SHA1"),
        "sha256" => Some("SHA256"),
        "sha384" => Some("SHA384"),
        "sha512" => Some("SHA512"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_check_line_sum_and_cksum_format() {
        // "<crc> <length> <name>" — length is present but not compared by C.
        assert_eq!(
            parse_check_line("1219131554 3 abc"),
            CheckEntry::Checksum {
                crc: 1219131554,
                name: "abc"
            }
        );
        // Two tokens: first is the crc, the rest-after-last-space is the name.
        assert_eq!(
            parse_check_line("4294967295 a.txt"),
            CheckEntry::Checksum {
                crc: 4294967295,
                name: "a.txt"
            }
        );
    }

    #[test]
    fn test_parse_check_line_name_is_after_the_last_space() {
        // C uses strrchr(' ')+1, so a name with spaces resolves to its trailing
        // word — a documented C limitation, not a bug we diverge on.
        assert_eq!(
            parse_check_line("123 456 my file.txt"),
            CheckEntry::Checksum {
                crc: 123,
                name: "file.txt"
            }
        );
    }

    #[test]
    fn test_parse_check_line_classifies_hash_and_malformed() {
        // The crypto hash grammar carries a '(' — unverifiable without primitives.
        assert_eq!(parse_check_line("md5 (f) = d41d8cd98f00"), CheckEntry::Hash);
        // A single token with no space is "bogus format".
        assert_eq!(parse_check_line("nospace"), CheckEntry::Malformed);
    }

    #[test]
    fn test_parse_check_line_leading_digits_only() {
        // strtoul stops at the first non-digit; a non-numeric start yields 0.
        assert_eq!(
            parse_check_line("1234abc 9 f"),
            CheckEntry::Checksum {
                crc: 1234,
                name: "f"
            }
        );
        assert_eq!(
            parse_check_line("abc 9 f"),
            CheckEntry::Checksum { crc: 0, name: "f" }
        );
    }

    #[test]
    fn test_parse_cksum_args_keeps_options_out_of_operands() {
        // `-n`/`-q` are C print modes (no-op on the crc path) and must be
        // recognised, not pushed as file operands — the previous parser leaked
        // them into the file list.
        let req = parse_cksum_args(&["-n", "-q", "f"]).unwrap();
        assert!(req.print_normal);
        assert!(req.print_quiet);
        assert_eq!(req.operands, ["f"]);
    }

    #[test]
    fn test_parse_cksum_args_requires_hash_flags() {
        // `-p/-s/-t/-x` call requirehash; they error instead of leaking as files.
        assert_eq!(
            parse_cksum_args(&["-p"]),
            Err(CksumError::RequiresHash('p'))
        );
        assert_eq!(
            parse_cksum_args(&["-x", "y"]),
            Err(CksumError::RequiresHash('x'))
        );
    }

    #[test]
    fn test_parse_cksum_args_a_and_o_algorithms() {
        assert_eq!(parse_cksum_args(&["-a", "crc"]).unwrap().kind, SumKind::Crc);
        assert_eq!(
            parse_cksum_args(&["-a", "OLD1"]).unwrap().kind,
            SumKind::Old1
        );
        assert_eq!(
            parse_cksum_args(&["-a", "old2"]).unwrap().kind,
            SumKind::Old2
        );
        assert_eq!(
            parse_cksum_args(&["-a", "md5"]).unwrap().hash,
            Some("MD5"),
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
                parse_cksum_args(&["-a", word]).unwrap().hash,
                Some(name),
                "`-a {word}` 该选中 {name}"
            );
        }
        // 表里没有的拼法不是别名：C 没有 `ripemd160` 这一行，所以它落到
        // `illegal argument to -a option`（`cksum.c:201-224` 的两级查找之后）。
        assert_eq!(
            parse_cksum_args(&["-a", "ripemd160"]),
            Err(CksumError::IllegalOptionValue('a'))
        );
        // 选中之后再 `-a`：C 的 `case 'a'` 第一行就是这条（`cksum.c:195-199`），
        // 第二枚的名字好不好都在所不问——`crc` 也一样被这条挡住。
        assert_eq!(
            parse_cksum_args(&["-a", "md5", "-a", "sha1"]),
            Err(CksumError::IllegalUseOfA)
        );
        assert_eq!(
            parse_cksum_args(&["-a", "md5", "-a", "crc"]),
            Err(CksumError::IllegalUseOfA)
        );
        // 选中之后再 `-o`：句子带的是大写名字（`cksum.c:228-234`）。
        assert_eq!(
            parse_cksum_args(&["-a", "md5", "-o", "1"]),
            Err(CksumError::MutuallyExclusiveWithSum("MD5"))
        );
        // 反过来先 `-o` 再 `-a` 就不冲突：`hash` 这时还是空的。
        assert_eq!(
            parse_cksum_args(&["-o", "1", "-a", "md5"]).unwrap().hash,
            Some("MD5"),
        );
        assert_eq!(
            parse_cksum_args(&["-o", "1", "-a", "md5"]).unwrap().kind,
            SumKind::Old1,
        );
        // 选中之后 `-p`/`-s` 不再报 `requires \`-a algorithm'`（C 的 requirehash 放行）；
        // 没选中时才报，这一形是批次三百零七钉下的。
        assert!(parse_cksum_args(&["-a", "sha1", "-p"]).is_ok());
        assert!(parse_cksum_args(&["-a", "sha1", "-s", "abc"]).is_ok());
        assert_eq!(
            parse_cksum_args(&["-p"]),
            Err(CksumError::RequiresHash('p'))
        );
        // 缺值仍由 libc 先报，即使此时已有 hash 选中（getopt 在进入 switch 之前就要值）。
        assert_eq!(
            parse_cksum_args(&["-a", "md5", "-o"]),
            Err(CksumError::MissingValue('o'))
        );
        // 带值选项**后面**的操作数不能被吞。旧的下标算术在取值处和循环尾各推一次，
        // 于是 `-a crc file` 会跳过 `file`，让本应算文件的 cksum 转去读标准输入——
        // 附值形 `-acrc file` 同病。这一组断言钉住「一个 token 只由循环尾推进一次」。
        assert_eq!(
            parse_cksum_args(&["-a", "crc", "file"]).unwrap().operands,
            vec!["file"]
        );
        assert_eq!(
            parse_cksum_args(&["-acrc", "file"]).unwrap().operands,
            vec!["file"]
        );
        assert_eq!(
            parse_cksum_args(&["-o", "1", "-a", "sha1", "f1", "f2"])
                .unwrap()
                .operands,
            vec!["f1", "f2"]
        );
        assert_eq!(
            parse_cksum_args(&["-a", "crc", "-n", "-"])
                .unwrap()
                .operands,
            vec!["-"],
            "裸 `-` 仍是操作数，且不被前一个值吞掉"
        );
        assert_eq!(parse_cksum_args(&["-o", "2"]).unwrap().kind, SumKind::Old2);
        // `-o 9` 是「值不合法」（C 的 `illegal argument to -o option`），
        // 而 `-o` 到量尽头没有值是另一回事（libc 的缺值那一行）。
        assert_eq!(
            parse_cksum_args(&["-o", "9"]),
            Err(CksumError::IllegalOptionValue('o'))
        );
        assert_eq!(
            parse_cksum_args(&["-o"]),
            Err(CksumError::MissingValue('o'))
        );
        assert_eq!(
            parse_cksum_args(&["-a", "nosuch"]),
            Err(CksumError::IllegalOptionValue('a'))
        );
    }

    /// getopt 的形状：聚簇、附值、`--` 后的全是操作数。旧写法把整个 token 当一个字母，
    /// `-nq`、`-o2`、`-a crc -- -f` 这几形都会错。
    #[test]
    fn test_parse_cksum_args_follows_the_getopt_shape() {
        let req = parse_cksum_args(&["-nq", "f"]).unwrap();
        assert!(req.print_normal && req.print_quiet);
        assert_eq!(req.operands, vec!["f"]);
        // 附值形：`-o2` 与 `-o 2` 同效（`optarg` 就是本 token 的剩余部分）。
        assert_eq!(
            parse_cksum_args(&["-o2"]).unwrap().kind,
            parse_cksum_args(&["-o", "2"]).unwrap().kind
        );
        assert_eq!(parse_cksum_args(&["-acrc"]).unwrap().kind, SumKind::Crc);
        // `--` 之后的负向词是操作数，不被当旗标。
        let req = parse_cksum_args(&["-c", "--", "-o"]).unwrap();
        assert!(req.do_check);
        assert_eq!(req.operands, vec!["-o"]);
        // `-s` 先吃 getopt 的值（本 token 剩余或下一词），再落 `requirehash`；
        // 没有值时先报缺值而不是 requirehash。
        assert_eq!(
            parse_cksum_args(&["-s", "x"]),
            Err(CksumError::RequiresHash('s'))
        );
        assert_eq!(
            parse_cksum_args(&["-s"]),
            Err(CksumError::MissingValue('s'))
        );
        // 聚簇里的集外字母报的是那个字母本身。
        assert_eq!(
            parse_cksum_args(&["-ncZ"]),
            Err(CksumError::UnknownOption('Z'))
        );
    }

    #[test]
    fn test_parse_cksum_args_check_warn_and_edge_forms() {
        let req = parse_cksum_args(&["-w", "-c", "ck.txt"]).unwrap();
        assert!(req.do_check);
        assert!(req.check_warn);
        assert_eq!(req.operands, ["ck.txt"]);
        assert_eq!(
            parse_cksum_args(&["-Z"]),
            Err(CksumError::UnknownOption('Z'))
        );
        // A bare `-` is a file/stdin operand, not an option letter.
        assert_eq!(parse_cksum_args(&["-"]).unwrap().operands, ["-"]);
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
