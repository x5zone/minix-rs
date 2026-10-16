//! Character set translation, deletion, and squeezing behind `tr`.
//!
//! Ground truth: `minix3/usr.bin/tr/tr.c` (283 lines; usage collected
//! around lines 58 to 125). Three operations over two character sets:
//!
//! - Translation: each byte in set 1 becomes the corresponding byte of set
//!   2 (a short set 2 repeats its last byte, matching the C tool).
//! - Deletion (`-d`): bytes in set 1 vanish.
//! - Squeezing (`-s`): runs of bytes in the squeeze set collapse to one.
//!
//! Sets are built from literals, ranges (`a-z`), repeats (`[a*5]`), and
//! classes (`[:alpha:]` and friends — recognised by name, ASCII only).
//! Membership tests go through [`CharClass`] with one implementation per
//! set shape, so callers never special case ranges versus lists.

use alloc::vec;
use alloc::vec::Vec;

use crate::TextError;

/// Maximum members in an explicit byte list set.
pub const MAX_MEMBERS: usize = 64;

/// One character set flavour: how bytes join the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    /// Explicit member bytes (up to 64).
    List(CharList),
    /// Inclusive byte range.
    Range(u8, u8),
    /// Named class (`alpha`, `digit`, `space`, `upper`, `lower`, `alnum`).
    Named(NamedClass),
}

/// Explicit member list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharList {
    /// Member bytes.
    pub members: [u8; MAX_MEMBERS],
    /// How many of `members` are used.
    pub count: u8,
}

/// Named character classes (ASCII only, C locale behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedClass {
    /// Letters.
    Alpha,
    /// Letters and digits.
    Alnum,
    /// Digits.
    Digit,
    /// Blanks (space and tab).
    Blank,
    /// Whitespace (space, tab, newline, carriage return, vertical tab,
    /// form feed).
    Space,
    /// Uppercase letters.
    Upper,
    /// Lowercase letters.
    Lower,
}

impl CharClass {
    /// Decide whether `byte` belongs to the set.
    pub fn contains(self, byte: u8) -> bool {
        match self {
            CharClass::List(list) => list.members[..list.count as usize].contains(&byte),
            CharClass::Range(low, high) => low <= byte && byte <= high,
            CharClass::Named(class) => match class {
                NamedClass::Alpha => byte.is_ascii_alphabetic(),
                NamedClass::Alnum => byte.is_ascii_alphanumeric(),
                NamedClass::Digit => byte.is_ascii_digit(),
                NamedClass::Blank => matches!(byte, b' ' | b'\t'),
                NamedClass::Space => matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C),
                NamedClass::Upper => byte.is_ascii_uppercase(),
                NamedClass::Lower => byte.is_ascii_lowercase(),
            },
        }
    }
}

/// Parse one set expression (`abc`, `a-z`, `[a*5]`, `[:alpha:]`, escapes
/// like `\n`) into up to 8 classes, expanding to at most 256 member bytes
/// in `expanded` (returns the used count).
///
/// Repeat counts that would overflow the 256 table are an error, matching
/// the C tool refusing absurd sets rather than wrapping.
pub fn parse_set(
    text: &str,
    expanded: &mut [u8; 256],
) -> Result<(usize, [CharClass; 8], usize), TextError> {
    let bytes = text.as_bytes();
    let mut classes: [CharClass; 8] = [CharClass::Range(0, 0); 8];
    let mut class_count = 0;
    let mut pos = 0;
    let mut used = 0;
    while pos < bytes.len() {
        if bytes[pos] == b'[' && pos + 1 < bytes.len() && bytes[pos + 1] == b':' {
            let start = pos + 2;
            let mut end = start;
            while end + 1 < bytes.len() && !(bytes[end] == b':' && bytes[end + 1] == b']') {
                end += 1;
            }
            if end + 1 >= bytes.len() {
                return Err(TextError::InvalidArgument);
            }
            let name = &text[start..end];
            let class = match name {
                "alpha" => NamedClass::Alpha,
                "alnum" => NamedClass::Alnum,
                "digit" => NamedClass::Digit,
                "blank" => NamedClass::Blank,
                "space" => NamedClass::Space,
                "upper" => NamedClass::Upper,
                "lower" => NamedClass::Lower,
                _ => return Err(TextError::InvalidArgument),
            };
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Named(class);
            class_count += 1;
            for byte in 0..=255u16 {
                let byte = byte as u8;
                if CharClass::Named(class).contains(byte) {
                    push_byte(expanded, &mut used, byte)?;
                }
            }
            pos = end + 2;
        } else if bytes[pos] == b'[' && pos + 2 < bytes.len() {
            // Repeat `[c*n]`: find the closing bracket first.
            let mut end = pos + 1;
            while end < bytes.len() && bytes[end] != b']' {
                end += 1;
            }
            if end >= bytes.len() {
                return Err(TextError::InvalidArgument);
            }
            let (member, count) = parse_repeat(&text[pos..=end])?;
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(member, member);
            class_count += 1;
            for _ in 0..count {
                push_byte(expanded, &mut used, member)?;
            }
            pos = end + 1;
        } else if pos + 2 < bytes.len() && bytes[pos + 1] == b'-' {
            let (low, high) = (bytes[pos], bytes[pos + 2]);
            if high < low {
                return Err(TextError::InvalidArgument);
            }
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(low, high);
            class_count += 1;
            let mut byte = low;
            loop {
                push_byte(expanded, &mut used, byte)?;
                if byte == high {
                    break;
                }
                byte += 1;
            }
            pos += 3;
        } else if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
            let byte = match bytes[pos + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'\\' => b'\\',
                other => other,
            };
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(byte, byte);
            class_count += 1;
            push_byte(expanded, &mut used, byte)?;
            pos += 2;
        } else {
            if class_count >= classes.len() {
                return Err(TextError::InvalidArgument);
            }
            classes[class_count] = CharClass::Range(bytes[pos], bytes[pos]);
            class_count += 1;
            push_byte(expanded, &mut used, bytes[pos])?;
            pos += 1;
        }
    }
    Ok((used, classes, class_count))
}

/// Append one expanded member byte; the 256 table bounds every set.
fn push_byte(expanded: &mut [u8; 256], used: &mut usize, byte: u8) -> Result<(), TextError> {
    if *used >= expanded.len() {
        return Err(TextError::InvalidArgument);
    }
    expanded[*used] = byte;
    *used += 1;
    Ok(())
}

/// Parse a `[c*n]` repeat expression (brackets included).
fn parse_repeat(text: &str) -> Result<(u8, usize), TextError> {
    let inner = text
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(TextError::InvalidArgument)?;
    let (member_text, count_text) = inner.split_once('*').ok_or(TextError::InvalidArgument)?;
    let member_bytes = member_text.as_bytes();
    if member_bytes.len() != 1 {
        return Err(TextError::InvalidArgument);
    }
    let mut count: usize = 0;
    if count_text.is_empty() || !count_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TextError::InvalidArgument);
    }
    for byte in count_text.bytes() {
        count = count
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as usize))
            .ok_or(TextError::InvalidArgument)?;
    }
    Ok((member_bytes[0], count))
}

/// Translate `input` through two expanded sets into `out`.
///
/// `set1`/`set2` are the expanded member lists with their lengths; a short
/// second set repeats its last byte. Returns the used byte count.
pub fn translate(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    out: &mut [u8],
) -> Result<usize, TextError> {
    if set1.is_empty() || set2.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    // Build the 256 byte map once: identity, then set1 positions point at
    // the corresponding (or repeated last) set2 byte.
    let mut map = [0u8; 256];
    for (index, slot) in map.iter_mut().enumerate() {
        *slot = index as u8;
    }
    for (index, byte) in set1.iter().enumerate() {
        let replacement = if index < set2.len() {
            set2[index]
        } else {
            set2[set2.len() - 1]
        };
        map[*byte as usize] = replacement;
    }
    if input.len() > out.len() {
        return Err(TextError::TooLong);
    }
    for (index, byte) in input.iter().enumerate() {
        out[index] = map[*byte as usize];
    }
    Ok(input.len())
}

/// Delete every byte of `set` from `input` into `out`.
pub fn delete(input: &[u8], set: &[u8], out: &mut [u8]) -> Result<usize, TextError> {
    let mut table = [false; 256];
    for byte in set {
        table[*byte as usize] = true;
    }
    let mut written = 0;
    for byte in input {
        if !table[*byte as usize] {
            if written >= out.len() {
                return Err(TextError::TooLong);
            }
            out[written] = *byte;
            written += 1;
        }
    }
    Ok(written)
}

/// Squeeze runs of `set` bytes in `input` down to one each.
pub fn squeeze(input: &[u8], set: &[u8], out: &mut [u8]) -> Result<usize, TextError> {
    let mut table = [false; 256];
    for byte in set {
        table[*byte as usize] = true;
    }
    let mut written = 0;
    let mut previous: Option<u8> = None;
    for byte in input {
        if table[*byte as usize] && previous == Some(*byte) {
            continue;
        }
        if written >= out.len() {
            return Err(TextError::TooLong);
        }
        out[written] = *byte;
        written += 1;
        previous = Some(*byte);
    }
    Ok(written)
}

/// `tr.c:68` 的选项环：`while ((ch = getopt(argc, argv, "cds")) != -1)`——`c`/`d`/`s`
/// **三个都是旗标**，所以 `getopt.c:108-140` 的「带值 / 缺值」那一支在本支**不可达**，
/// 可达的只有「集外字母」一种 libc 报文（`getopt.c:98-106`）。更要紧的是
/// `tr.c:79-81` 的 `case '?': default: usage();` **坐在环里直接退出**，于是本支的 libc 报文
/// **至多一条**（参照件 `-ZQ` 只报 `Z`、`-sZ a` 只报 `Z`），与 `seq` 的累加形相反。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrArgs<'a> {
    /// `-c`：把 set1 取补。
    pub complement: bool,
    /// `-d`：删除 set1 里的字节。
    pub delete: bool,
    /// `-s`：挤压。
    pub squeeze: bool,
    /// 环不重排：第一个不是选项的 token 及其后一切**都是操作数**
    /// （`tr.c:83-84` 的 `argc -= optind; argv += optind;`）。所以 `tr a -d` 的 `-d`
    /// 是第二个集合名而不是旗标。
    pub operands: Vec<&'a str>,
}

/// 前门的错：本支所有出错出口打的都是**同一块四行 usage** 并退 1（`tr.c:276-282`），
/// 区别只在于前面有没有一行 libc 报文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrError {
    /// libc 那一行（至多一条）加上整块 usage，由壳发到标准错误。
    Usage { reports: Vec<crate::GetoptReport> },
}

/// 环 + 个数判 + 组合判，按 `tr.c` 的**先后次序**执行：环里的集外字母 ⇒ 带 libc 行的 usage；
/// `tr.c:86-97` 的 `switch (argc)` 判 0 个与 3 个以上；`-ds` 要两个操作数（`:104-106`）；
/// `-d` 给了两个就报错（`:123-125`）；既没 `-d` 也没 `-s` 时必须恰好两个（`:156-157`）。
pub fn parse_tr_args<'a>(args: &'a [&'a str]) -> Result<TrArgs<'a>, TrError> {
    const OPTSTRING: &[u8] = b"cds";
    let mut parsed = TrArgs {
        complement: false,
        delete: false,
        squeeze: false,
        operands: Vec::new(),
    };
    let mut i = 0usize;
    while i < args.len() {
        let token = args[i].as_bytes();
        // `getopt.c:74-78` 首字符不是 `-` ⇒ 收环且**不推进**游标；`getopt.c:86-93` 里
        // 一个光杆 `-` 因为 optstring 不含 `-` 也走同一收法，于是它落进操作数。
        if token.len() < 2 || token[0] != b'-' {
            break;
        }
        // `getopt.c:80-85`：**恰好**两个短横才认作结束标记，并且吃掉自身。
        if token == b"--" {
            i += 1;
            break;
        }
        let mut k = 1usize;
        while k < token.len() {
            let letter = token[k];
            k += 1;
            if !OPTSTRING.contains(&letter) {
                // `getopt.c:98-106` 先印一行，`tr.c:79-81` 的 `default: usage()` 紧接着退出
                return Err(TrError::Usage {
                    reports: vec![crate::GetoptReport::UnknownOption(char::from(letter))],
                });
            }
            match letter {
                b'c' => parsed.complement = true,
                b'd' => parsed.delete = true,
                _ => parsed.squeeze = true,
            }
            // 三个字母都不带值，所以只有「这个 token 已到末尾」才推进到下一个
            if k >= token.len() {
                i += 1;
            }
        }
    }
    parsed.operands = args[i..].to_vec();
    let count = parsed.operands.len();
    // `tr.c:86-97`：0 个与 3 个以上都只落进 `default: usage();`
    if count == 0 || count > 2 {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    let two_sets = count == 2;
    // 下面三条一一对应 C 里三段代码的**原次序**，每条就是 C 的那一句 `if`：
    // `-ds` 少给 string2（`:104-106`）、`-d` 多给 string2（`:123-125`）、
    // 翻译路径没有 string2（`:156-157`）。`-s` 单带一个操作数在 `:139-140` 自成一局，
    // 所以不进任何一条。
    if parsed.delete && parsed.squeeze && !two_sets {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    if parsed.delete && !parsed.squeeze && two_sets {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    if !parsed.delete && !parsed.squeeze && !two_sets {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    Ok(parsed)
}

/// `tr.c:276-282` 那**四行**：第一行以 `usage: ` 起，后三行是七个空格的对齐续行；
/// 整个块没有程序名占位（C 把 `tr` 写死在串里）且走**标准错误**。
/// 用 `concat!` 拼是因为 Rust 的字符串续行会吃掉下一行的行首空白，而那七个空格就是对齐本身。
#[must_use]
pub const fn usage_block() -> &'static [u8] {
    concat!(
        "usage: tr [-cs] string1 string2\n",
        "       tr [-c] -d string1\n",
        "       tr [-c] -s string1\n",
        "       tr [-c] -ds string1 string2\n"
    )
    .as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded_set(text: &str) -> ([u8; 256], usize) {
        let mut expanded = [0u8; 256];
        let (used, _, _) = parse_set(text, &mut expanded).unwrap();
        (expanded, used)
    }

    #[test]
    fn test_translate_basic() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("XYZ");
        let mut out = [0u8; 16];
        let len = translate(b"axbxc", &set1[..len1], &set2[..len2], &mut out).unwrap();
        // Only set members translate; `x` passes through twice.
        assert_eq!(&out[..len], b"XxYxZ");
    }

    #[test]
    fn test_short_second_set_repeats_last() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("X");
        let mut out = [0u8; 16];
        let len = translate(b"abc", &set1[..len1], &set2[..len2], &mut out).unwrap();
        assert_eq!(&out[..len], b"XXX");
    }

    #[test]
    fn test_range_and_repeat_sets() {
        let (set1, len1) = expanded_set("a-c");
        assert_eq!(len1, 3);
        let (set2, len2) = expanded_set("[x*2]");
        assert_eq!((len2, set2[0], set2[1]), (2, b'x', b'x'));
        let _ = set1;
    }

    #[test]
    fn test_named_class() {
        let (set, len) = expanded_set("[:digit:]");
        assert_eq!(len, 10);
        assert!(set[..len].contains(&b'5'));
    }

    #[test]
    fn test_delete_and_squeeze() {
        let (vowels, vowels_len) = expanded_set("aeiou");
        let mut out = [0u8; 16];
        let len = delete(b"hello", &vowels[..vowels_len], &mut out).unwrap();
        assert_eq!(&out[..len], b"hll");
        let (spaces, spaces_len) = expanded_set(" ");
        let len = squeeze(b"a  b   c", &spaces[..spaces_len], &mut out).unwrap();
        assert_eq!(&out[..len], b"a b c");
    }

    #[test]
    fn test_class_strategies_agree() {
        let list = CharClass::List(CharList {
            members: {
                let mut members = [0u8; MAX_MEMBERS];
                members[0] = b'a';
                members[1] = b'b';
                members
            },
            count: 2,
        });
        let range = CharClass::Range(b'a', b'b');
        for byte in 0..=255u16 {
            assert_eq!(list.contains(byte as u8), range.contains(byte as u8));
        }
    }

    #[test]
    fn test_bad_sets_rejected() {
        let mut expanded = [0u8; 256];
        assert_eq!(
            parse_set("z-a", &mut expanded).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        assert_eq!(
            parse_set("[:nope:]", &mut expanded).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        assert_eq!(
            parse_set("[ab*2]", &mut expanded).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
    }
    #[test]
    fn test_ring_stops_at_the_first_off_letter() {
        // 环里的 usage 直接退出 ⇒ 报文至多一条，且报的是**第一个**坏字母
        assert_eq!(
            parse_tr_args(&["-ZQ"]),
            Err(TrError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
        assert_eq!(
            parse_tr_args(&["-sZ", "a"]),
            Err(TrError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
        // 三个字母都是旗标 ⇒ 缺值那一支到不了，`-d` 的 `d` 不是「带值字母」
        assert_eq!(
            parse_tr_args(&["-d", "a"]).map(|p| (p.delete, p.operands)),
            Ok((true, vec!["a"]))
        );
    }

    #[test]
    fn test_operand_count_and_combination_all_say_usage() {
        for args in [
            &[][..],               // 零个操作数
            &["a"][..],            // 一个操作数却没给 -d/-s
            &["a", "b", "c"][..],  // 三个
            &["-d"][..],           // 只有旗标
            &["-d", "a", "b"][..], // -d 多给一个
            &["-ds", "a"][..],     // -ds 少给一个
            &["-c"][..],
        ] {
            assert_eq!(
                parse_tr_args(args).map(|_| ()),
                Err(TrError::Usage {
                    reports: Vec::new()
                }),
                "{args:?}"
            );
        }
        // 合法的形：`-s` 配一个、`-ds` 配两个、`-c` 配两个、裸 `-` 是集合名
        assert_eq!(
            parse_tr_args(&["-s", "a"]).map(|p| (p.squeeze, p.operands.len())),
            Ok((true, 1))
        );
        assert_eq!(
            parse_tr_args(&["-ds", "a", "b"]).map(|p| (p.delete, p.squeeze, p.operands.len())),
            Ok((true, true, 2))
        );
        assert_eq!(
            parse_tr_args(&["-", "a"]).map(|p| p.operands),
            Ok(vec!["-", "a"])
        );
        // `--` 被吃掉且自身不是操作数；只给 `--` 时操作数为零 ⇒ usage（参照件同形）
        assert_eq!(
            parse_tr_args(&["--", "a", "b"]).map(|p| p.operands),
            Ok(vec!["a", "b"])
        );
        assert_eq!(
            parse_tr_args(&["--"]).map(|p| p.operands),
            Err(TrError::Usage {
                reports: Vec::new()
            })
        );
        // 环不重排：`a -d` 的两个 token 都是集合名
        assert_eq!(
            parse_tr_args(&["a", "-d"]).map(|p| (p.delete, p.operands)),
            Ok((false, vec!["a", "-d"]))
        );
    }

    #[test]
    fn test_usage_block_is_four_aligned_lines() {
        let block = core::str::from_utf8(usage_block()).expect("ascii");
        let lines: Vec<&str> = block.split('\n').collect();
        assert_eq!(lines.len(), 5, "四行 + 结尾换行后的空段");
        assert_eq!(lines[0], "usage: tr [-cs] string1 string2");
        for line in &lines[1..4] {
            assert!(
                line.starts_with("       tr "),
                "续行是七个空格对齐：{line:?}"
            );
        }
    }
}
