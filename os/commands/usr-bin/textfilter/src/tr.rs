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

use alloc::string::{String, ToString};
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
/// C 的 `str2set()` 带一个 `s->which`（`minix3/usr.bin/tr/str.c`），它决定同一串字符在
/// 两串里各自合法到什么程度：重复式 `[c*n]` 只在第二串合法。壳侧同样把它做成入参，
/// 而不是让两串共用一条无差别的解析。
/// 正在解析的是哪一串。C 的 `STRING1`/`STRING2`。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetSite {
    String1,
    String2,
}

/// `str.c` 里由语法自身触发的 `errx(1, ...)`。句子逐字照 C，因为 `errx` 是两段形
/// （程序名加句子，不缀 `strerror`），而 C 每处都是当场退出。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SetGrammar {
    /// `errx(1, "unknown class %.*s", (int)len, class)`（`str.c:253`）。
    UnknownClass(String),
    /// `genseq()` 的第一条判断：`if (s->which == STRING1) errx(1, "Sequences only valid in string2")`
    ///（`str.c:353-356`）——它在吃任何字符之前，所以第二枚字符是什么都无所谓。
    SequencesOnlyInString2,
    /// 取完成员字符后 `if (*s->str != '*') errx(1, "Misplaced sequence asterisk")`
    ///（`str.c:362-366`）。故 `[ab*2]` 报这句：`a` 之后的不是 `*` 而是 `b`。
    MisplacedSequenceAsterisk,
    /// 计数段既不是 `]`、也不是「数字紧跟 `]`」时 `errx(1, "illegal sequence count")`
    ///（`str.c:368-386`）。
    IllegalSequenceCount,
    /// `genequiv()` 取完成员字符后 `if (*s->str != '=') errx(1, "Misplaced equivalence
    /// equals sign")`（`str.c:307-320`）。成员只吃**一枚**字符（或一枚反斜杠转义），
    /// 所以 `[=ab=]`、`[==]`、`[=a b=]` 都撞在这一句上——式尾有没有 `=]` 由 `bracket()`
    /// 事先认过（`str.c:193-199`），它认得形式，`genequiv()` 才负责核形式内部。
    MisplacedEquivalenceEquals,
    /// 等号之后 `if (*s->str != ']') errx(1, "Misplaced equivalence right bracket")`
    ///（`str.c:321-323`）。`[=a=b=]` 走这一句：`bracket()` 找到的第一个 `=]` 在式尾，
    /// 而 `genequiv()` 逐枚走完 `a`、`=` 之后撞上的是 `b`。
    MisplacedEquivalenceRightBracket,
}

impl SetGrammar {
    /// C 打出去的那一句原文（不含程序名与冒号，那两段由 `errx` 的前缀负责）。
    pub fn sentence(&self) -> String {
        match self {
            SetGrammar::UnknownClass(name) => alloc::format!("unknown class {name}"),
            SetGrammar::SequencesOnlyInString2 => "Sequences only valid in string2".to_string(),
            SetGrammar::MisplacedSequenceAsterisk => "Misplaced sequence asterisk".to_string(),
            SetGrammar::IllegalSequenceCount => "illegal sequence count".to_string(),
            SetGrammar::MisplacedEquivalenceEquals => {
                "Misplaced equivalence equals sign".to_string()
            }
            SetGrammar::MisplacedEquivalenceRightBracket => {
                "Misplaced equivalence right bracket".to_string()
            }
        }
    }
}

/// 解析一个集合式的全部失败出口：语法句，以及尚未定形的两类（式子没闭合、成员表已满）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SetError {
    Grammar(SetGrammar),
    /// `[...)` 没有闭括号、或 `[:name:]` 没有收尾——C 对这两形走的是
    /// 「按字面量继续吃」还是另一句尚未逐行核对，故仍按既有的静默退 1 处理
    /// （登记见 18 篇台账）。
    Malformed,
    /// 成员超过 256 个，即 `TextError::TooLong`。
    TooLong,
}

impl From<TextError> for SetError {
    fn from(error: TextError) -> Self {
        match error {
            TextError::TooLong => SetError::TooLong,
            TextError::InvalidArgument => SetError::Malformed,
        }
    }
}

/// C 的名字表（`str.c` 的 `classes[]`）里认得的类名；表外的名字一律是
/// `unknown class <名>`，不是「当字面量凑合」。
pub fn parse_set(
    text: &str,
    site: SetSite,
    expanded: &mut [u8; 256],
) -> Result<(usize, Set2Tail), SetError> {
    let bytes = text.as_bytes();
    // 式子里最后一项的种类，决定 string2 耗尽之后补什么（见 `Set2Tail`）。
    let mut tail = Set2Tail::LastByte;
    let mut pos = 0;
    let mut used = 0;
    while pos < bytes.len() {
        // `next()` 一撞见 `[` 就先问 `bracket()`，它返回假就当场 FALLTHROUGH 成一枚
        // **左方括号字面量**（`str.c:124-132`）。所以「像特殊形而没成形」的 `[x]`、
        // `[:alpha`、`[x`、`[x]y*` 在 C 里都是普通字符、退 0——这三条形判据必须逐条
        // 照 `bracket()` 的 switch 抄（`str.c:176-210`），少抄一条就多一类假报错。
        let class_end = if marker_at(bytes, pos, b':') {
            find_pair(bytes, pos + 2, b':', b']')
        } else {
            None
        };
        let equiv_end = if marker_at(bytes, pos, b'=') {
            find_pair(bytes, pos + 2, b'=', b']')
        } else {
            None
        };
        let sequence_form = bytes[pos] == b'['
            && pos + 1 < bytes.len()
            && !matches!(bytes[pos + 1], b':' | b'=')
            && sequence_ahead(bytes, pos + 2);
        if let Some(end) = class_end {
            let name = &text[pos + 2..end];
            let class = match name {
                "alpha" => NamedClass::Alpha,
                "alnum" => NamedClass::Alnum,
                "digit" => NamedClass::Digit,
                "blank" => NamedClass::Blank,
                "space" => NamedClass::Space,
                "upper" => NamedClass::Upper,
                "lower" => NamedClass::Lower,
                _ => {
                    return Err(SetError::Grammar(SetGrammar::UnknownClass(
                        name.to_string(),
                    )))
                }
            };
            tail = Set2Tail::Oobch;
            for byte in 0..=255u16 {
                let byte = byte as u8;
                if CharClass::Named(class).contains(byte) {
                    push_byte(expanded, &mut used, byte)?;
                }
            }
            pos = end + 2;
        } else if let Some(end) = equiv_end {
            // `[=c=]`：`bracket()`（`str.c:193-199`）只认「式子里有没有一处 `=]`」，
            // 认过了就交给 `genequiv()`（`str.c:305-330`）逐枚核内部——成员吃**一枚**字符
            // （或一枚反斜杠转义），然后必须是 `=`，再然后必须是 `]`。C 的注释写明
            // 「English doesn't have any equivalence classes, so for now we just syntax
            // check and grab the character」，所以这个类永远只有一枚成员，而式内多塞的
            // 任何字符都是那两句「放错了等号 / 放错了右括号」之一。
            let mut cursor = pos + 2;
            let member = if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                let escaped = bytes[cursor + 1];
                cursor += 2;
                escape_literal(escaped)
            } else {
                let ch = bytes[cursor];
                cursor += 1;
                ch
            };
            if bytes.get(cursor) != Some(&b'=') {
                return Err(SetError::Grammar(SetGrammar::MisplacedEquivalenceEquals));
            }
            cursor += 1;
            if bytes.get(cursor) != Some(&b']') {
                return Err(SetError::Grammar(
                    SetGrammar::MisplacedEquivalenceRightBracket,
                ));
            }
            tail = Set2Tail::Oobch;
            push_byte(expanded, &mut used, member)?;
            // 两道核对都过了，`bracket()` 认下的那处 `=]` 必然就是刚核完的这两个字节，
            // 所以收口仍按 `end + 2`（与 C 的 `s->str = p + 2`，`str.c:200` 同形）。
            pos = end + 2;
        } else if sequence_form {
            // 三句的**先后**本身就是行为，照 `genseq()` 抄：先问「这是哪一串」（吃任何
            // 字符之前，`str.c:353-356`），再取成员字符，然后才核星号（`:362-366`），
            // 最后才是个数（`:368-386`）。于是第一串里的 `[c*n]` 永远轮不到谈内容，
            // 而第二串里的 `[ab*2]` 报的是星号那句而不是「个数非法」。
            if site == SetSite::String1 {
                return Err(SetError::Grammar(SetGrammar::SequencesOnlyInString2));
            }
            let mut cursor = pos + 1;
            let member = if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                let escaped = bytes[cursor + 1];
                cursor += 2;
                escape_literal(escaped)
            } else {
                let ch = bytes[cursor];
                cursor += 1;
                ch
            };
            if bytes.get(cursor) != Some(&b'*') {
                return Err(SetError::Grammar(SetGrammar::MisplacedSequenceAsterisk));
            }
            cursor += 1;
            let repeat = match bytes.get(cursor) {
                // `case ']'` 与「数字但值为 0」都汇到 `s->state = s->cnt ? SEQUENCE :
                // INFINITE` 的 INFINITE 一侧：`[x*]` 与 `[x*0]` 是**无限**重复，不是空集。
                // 本层把无限落成一枚成员——`translate()` 对短的 set2 一律重复其末字节
                // （`tr.c:205-211`），那正是 INFINITE 在翻译路径上的效果。
                Some(b']') => {
                    cursor += 1;
                    1
                }
                _ => {
                    let start = cursor;
                    while matches!(bytes.get(cursor), Some(ch) if ch.is_ascii_digit()) {
                        cursor += 1;
                    }
                    if cursor == start || bytes.get(cursor) != Some(&b']') {
                        return Err(SetError::Grammar(SetGrammar::IllegalSequenceCount));
                    }
                    cursor += 1;
                    let mut count = 0usize;
                    for digit in &bytes[start..cursor - 1] {
                        count = count
                            .checked_mul(10)
                            .and_then(|v| v.checked_add((digit - b'0') as usize))
                            .ok_or(SetError::Malformed)?;
                    }
                    if count == 0 {
                        1
                    } else {
                        count
                    }
                }
            };
            tail = Set2Tail::LastByte;
            for _ in 0..repeat {
                push_byte(expanded, &mut used, member)?;
            }
            pos = cursor;
        } else if pos + 2 < bytes.len() && bytes[pos + 1] == b'-' {
            let (low, high) = (bytes[pos], bytes[pos + 2]);
            if high < low {
                // `genrange()` 的 `if (stopval < s->lastch) { s->str = savestart; return 0; }`
                // 把整个区间退回给主循环，左端点于是按字面量入集，下一轮再从 `-` 起：
                // `tr z-a y` 的 set1 是 z、-、a 三枚成员并退 0，不是「非法区间」。
                tail = Set2Tail::LastByte;
                push_byte(expanded, &mut used, low)?;
                pos += 1;
                continue;
            }
            tail = Set2Tail::LastByte;
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
            let byte = escape_literal(bytes[pos + 1]);
            tail = Set2Tail::LastByte;
            push_byte(expanded, &mut used, byte)?;
            pos += 2;
        } else {
            tail = Set2Tail::LastByte;
            push_byte(expanded, &mut used, bytes[pos])?;
            pos += 1;
        }
    }
    Ok((used, tail))
}

/// 这枚 `[` 后面紧跟的是不是那个特殊标记（`bracket()` 的 `switch (s->str[1])`）。
#[must_use]
fn marker_at(bytes: &[u8], pos: usize, marker: u8) -> bool {
    bytes.get(pos) == Some(&b'[') && bytes.get(pos + 1) == Some(&marker)
}

/// 从 `from` 起找成对的收尾 `ab`（`bracket()` 的 `strstr(s->str + 2, ":]")` 与
/// `strstr(s->str + 2, "=]")`），返回 `a` 的下标；找不到就说明这枚 `[` 不是特殊形。
#[must_use]
fn find_pair(bytes: &[u8], from: usize, a: u8, b: u8) -> Option<usize> {
    (from..bytes.len().saturating_sub(1)).find(|&i| bytes[i] == a && bytes[i + 1] == b)
}

/// 序列形的判据是 `strpbrk(s->str + 2, "*]")` **先撞上 `*`**，且那枚 `*` 之后还有一个
/// `]`（`str.c:200-206`）。任缺其一，C 就把这枚 `[` 当字面量：`[x]` 与 `[x]y*` 都因
/// 为「先撞上 `]`」而落回普通字符，`[x` 则因为「往后什么都没有」而落回普通字符。
#[must_use]
fn sequence_ahead(bytes: &[u8], from: usize) -> bool {
    let hit = (from..bytes.len())
        .position(|i| bytes[i] == b'*' || bytes[i] == b']')
        .map(|i| i + from);
    match hit {
        Some(i) => bytes[i] == b'*' && bytes[i..].contains(&b']'),
        None => false,
    }
}

/// `\` 后面那个字符代表谁——`backslash()` 的转义表里本支只接得起这三个命名形，
/// 八进制那一段（`str.c:398-415` 的循环）登记在 `TR-BACKSLASH-OCTAL` 而未接。
#[must_use]
fn escape_literal(escaped: u8) -> u8 {
    match escaped {
        b'n' => b'\n',
        b't' => b'\t',
        b'r' => b'\r',
        other => other,
    }
}

/// C 的 `extern.h:39` 的 `#define OOBCH (UCHAR_MAX + 1)`：一个**装不进气表**的值当哨兵。
/// 本层把它留在 `u16` 域里（`translate_squeeze` 的挤压判据要比的就是这个 256），
/// 只在写出的那一刻截成 `unsigned char`，也就是 `0`。
const OOBCH: u16 = 256;

/// 式子耗尽之后 `next()` 还会不会给出新东西——`tr.c:205-211` 那句注释说的「重复末字节」
/// 只对**普通**式子成立，而以一枚集合类收尾的式子交回的是 `OOBCH`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set2Tail {
    /// 最后一项是普通字符、区间或 `[x*n]`：`lastch` 停在那枚成员上，所以耗尽之后重复末字节。
    LastByte,
    /// 最后一项是 `[:class:]` 或 `[=c=]`：`str.c:158-166` 的 `SET` 分支先做
    /// `s->lastch = s->set[s->cnt++]`，走到哨兵时把 `lastch` 顶成 `OOBCH`，随后
    /// `case '\0'` 那一支交回的就是这个 256（`:110-122`）。`tr.c:217` 存进映射表、
    /// `putchar` 截成 `0`；而 `-s` 那条腿在 `int` 域里比较，所以 256 与普通 0 是两回事。
    Oobch,
}

/// Append one expanded member byte; the 256 table bounds every set.
fn push_byte(expanded: &mut [u8; 256], used: &mut usize, byte: u8) -> Result<(), TextError> {
    if *used >= expanded.len() {
        return Err(TextError::TooLong);
    }
    expanded[*used] = byte;
    *used += 1;
    Ok(())
}

/// C 那张映射表：`int string1[NCHARS]`（`tr.c:55`）先填恒等（`:178-181`），再按 string1
/// 的每一位写入 string2 交回的值（`:217`）。**值域是 `int` 而不是字节**，因为一枚集合类
/// 耗尽之后交回的是 `OOBCH`（256）；`translate` 与 `translate_and_squeeze` 共用这张表，
/// 差别只在下半场要不要按「交回过的值」挤压。
fn build_map(set1: &[u8], set2: &[u8], tail: Set2Tail) -> [u16; 256] {
    let mut map = [0u16; 256];
    for (index, slot) in map.iter_mut().enumerate() {
        *slot = index as u16;
    }
    for (index, byte) in set1.iter().enumerate() {
        map[*byte as usize] = if index < set2.len() {
            u16::from(set2[index])
        } else {
            // string2 比 string1 短：C 不再取新值，而是把 `next()` 当时那个 `lastch` 一路重复
            //（`tr.c:205-211` 的注释说的就是这件事）。普通式子停在末字节，集合类停在 OOBCH。
            match tail {
                Set2Tail::Oobch => OOBCH,
                Set2Tail::LastByte => u16::from(*set2.last().unwrap_or(&0)),
            }
        };
    }
    map
}

/// Translate `input` through two expanded sets into `out`.
///
/// `set1`/`set2` are the expanded member lists; a short second set repeats its
/// last byte **unless it ends with a character class**, in which case C repeats
/// `OOBCH` and `putchar` writes that as a NUL byte. Returns the used byte count.
///
/// **一个空的 `set1` 是合法输入**，走的是恒等映射：C 在非取补路径上先把映射表填成
/// `string1[ch] = ch`（`tr.c:178-181`），随后取 string1 首元素的 `next(s1)` 在空集合上
/// 当场失败而跳出映射循环（`:214-215`），于是一张谁也没被改过的恒等表被原样用于过滤——
/// 参照件 `tr '' x` 因此是「退 0 且逐字节照抄标准输入」。空的 `set2` 不在这条例外里：
/// C 在循环之前就用 `next(s2)` 把它判死成 `empty string2`（`:187-188`），本层那条判据在壳里。
pub fn translate(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    tail: Set2Tail,
    out: &mut [u8],
) -> Result<usize, TextError> {
    if set2.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    let map = build_map(set1, set2, tail);
    if input.len() > out.len() {
        return Err(TextError::TooLong);
    }
    for (index, byte) in input.iter().enumerate() {
        // `putchar(string1[ch])`：C 的实参是 `int`，落到 `putchar` 时被截成 `unsigned char`，
        // 所以 OOBCH(256) 打出去就是 `\0`（参照件 `tr abc '[=c=]'` ← `abc` 交回 `c\0\0`）。
        out[index] = map[*byte as usize] as u8;
    }
    Ok(input.len())
}

/// `tr [-c] -s string1 string2` 那一腿（`tr.c:239-246`）：**映射与挤压在同一趟里做**，
/// 而且挤压判据比的是映射**之后**的 `int` 值——`lastch` 初值就是 `OOBCH`，被记进
/// `string2[]` 的也恰恰是那一路交回过的值（`tr.c:222-224` 记在映射那一趟里）。
///
/// 这两句合起来解释了一格看着矛盾的现象：`tr -s abc '[=c=]'` ← `abc` 交回 `c\0`，
/// 即那两枚由 OOBCH 变来的 NUL **互相挤压成一枚**，而输入里本来就有的 NUL 不受影响。
/// 将本腿拆成「先 translate 再 squeeze」两步就会丢掉这个区别——两步之间 256 已经被截成 0，
/// 于是 0 与真 NUL 混为一谈，挤压判据也换到了字节域上。
pub fn translate_and_squeeze(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    tail: Set2Tail,
    out: &mut [u8],
) -> Result<usize, TextError> {
    if set2.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    let map = build_map(set1, set2, tail);
    // C 的 `string2[ch2] = 1` 坐在映射循环里（`tr.c:222-224`），记的是**交回过的值**；
    // 值域含那枚哨兵，所以表比映射表宽一格。
    let mut yielded = [false; 257];
    for byte in set1 {
        yielded[map[*byte as usize] as usize] = true;
    }
    if input.len() > out.len() {
        return Err(TextError::TooLong);
    }
    let mut written = 0;
    let mut lastch = OOBCH;
    for byte in input {
        let ch = map[*byte as usize];
        if !yielded[ch as usize] || lastch != ch {
            lastch = ch;
            out[written] = ch as u8;
            written += 1;
        }
    }
    Ok(written)
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
        let (used, _) = parse_set(text, SetSite::String2, &mut expanded).unwrap();
        (expanded, used)
    }

    /// 只取尾部判据：式子以什么收尾，决定 string2 耗尽之后补什么。
    fn tail_of(text: &str) -> Set2Tail {
        let mut expanded = [0u8; 256];
        parse_set(text, SetSite::String2, &mut expanded).unwrap().1
    }

    /// string2 耗尽之后补什么，判据是**式子的最后一项**而不是整条式子里有没有集合类：
    /// `[:digit:]x` 停在普通字符上（重复 `x`），`x[:digit:]` 停在集合类上（重复 OOBCH）。
    /// 参照件八形逐一核过（`tmp/tr-343/ref-padding.log`）。
    #[test]
    fn only_a_class_at_the_end_changes_what_exhaustion_yields() {
        assert_eq!(tail_of("[=c=]"), Set2Tail::Oobch, "等价类收尾");
        assert_eq!(tail_of("[:digit:]"), Set2Tail::Oobch, "名字类收尾");
        assert_eq!(tail_of("x[:digit:]"), Set2Tail::Oobch, "集合类是最后一项");
        assert_eq!(tail_of("[=c=][=d=]"), Set2Tail::Oobch, "两枚等价类");
        assert_eq!(
            tail_of("[:digit:]x"),
            Set2Tail::LastByte,
            "集合类之后还有普通字符"
        );
        assert_eq!(tail_of("xy"), Set2Tail::LastByte, "普通两枚");
        assert_eq!(tail_of("a-z"), Set2Tail::LastByte, "区间收尾");
        assert_eq!(tail_of("[x*2]"), Set2Tail::LastByte, "重复式收尾");
    }

    /// 集合类把 string2 用尽之后，映射表里存的是 OOBCH、打出去是 NUL；
    /// 普通 string2 短了则一路重复末字节。两形各一枚对照，差的就是这一格。
    #[test]
    fn the_class_tail_maps_to_nul_bytes_while_a_plain_tail_repeats_its_last_byte() {
        let mut out = [0u8; 32];
        let (set1, len1) = expanded_set("abcdefghijklmnop");
        let (digits, len2) = expanded_set("[:digit:]");
        let len = translate(
            b"abcdefghijklmnop",
            &set1[..len1],
            &digits[..len2],
            tail_of("[:digit:]"),
            &mut out,
        )
        .unwrap();
        assert_eq!(
            &out[..len],
            b"0123456789\0\0\0\0\0\0",
            "十枚数字之后是六枚 NUL（OOBCH 被 putchar 截成 0）"
        );

        let (plain, plain_len) = expanded_set("xy");
        let (set3, len3) = expanded_set("abc");
        let len = translate(
            b"abc",
            &set3[..len3],
            &plain[..plain_len],
            tail_of("xy"),
            &mut out,
        )
        .unwrap();
        assert_eq!(&out[..len], b"xyy", "普通式子仍按末字节重复");
    }

    /// `-s` 那一腿在 `int` 域里挤压：相邻两枚由 OOBCH 变来的 NUL **挤成一枚**，
    /// 而输入里本来就有的 NUL 不参与挤压（它映射到自身，从没被记进「交回过」那张表）。
    /// 把这条腿拆成 translate + squeeze 两步就会把两者混为一谈——那两步之间 256 已经没了。
    /// 四形期望值全部取自参照件（`tmp/tr-390/ab.log`，26 形不一致 0）：本枚门第一次跑
    /// 就把自己的期望写错了（写成 `c\0\0`），是参照件的 `c\0\0\0` 纠正的。
    #[test]
    fn the_squeeze_leg_collapses_padded_nuls_but_leaves_a_real_nul_alone() {
        let mut out = [0u8; 32];
        let (set1, len1) = expanded_set("abc");
        let (class, len2) = expanded_set("[=c=]");
        for (input, expected) in [
            (&b"abc"[..], &b"c\0"[..]),
            (&b"abbc"[..], &b"c\0"[..]),
            (&b"ab\0c"[..], &b"c\0\0\0"[..]),
            (&b"a\0b\0c"[..], &b"c\0\0\0\0"[..]),
            // `tr.c:240` 的 `lastch` 初值就是 `OOBCH`，所以**开头**那枚 OOBCH 也算「与前一枚相同」
            // 而被挤掉——把初值写成任何真字节都会让它多打一枚 `\0`（参照件 `tr -s ab [=c=]` ← `b`
            // 交回**空串**，本枚门第一版没测这一格，负控 P4 因此空咬）。
            (&b"b"[..], &b""[..]),
        ] {
            let len = translate_and_squeeze(
                input,
                &set1[..len1],
                &class[..len2],
                tail_of("[=c=]"),
                &mut out,
            )
            .unwrap();
            assert_eq!(&out[..len], expected, "{input:?}");
        }
    }

    /// 本层原先给集合表留了八格，并让**每一枚**成员都占一格，于是九枚字面量
    /// （`tr 'abcdefghi' 'x'`）就是一条静默的退 1——C 侧没有这个上限：字面量、区间、
    /// 转义都只是 `lastch` 的一次赋值，只有 `[:class:]`/`[=c=]` 才用到 SET 状态。
    /// 参照件同形交回 `xxx` 退 0。
    #[test]
    fn a_literal_set_is_bounded_by_256_members_not_by_an_eight_slot_class_table() {
        for text in [
            "abcdefghi",
            "abcdefghijklmnopqrstuvwxyz",
            "1234567890 abcdefgh",
        ] {
            let (expanded, used) = expanded_set(text);
            assert_eq!(used, text.len(), "{text:?} 该按字面量整串展开");
            assert_eq!(
                &expanded[..used],
                text.as_bytes(),
                "{text:?} 成员顺序照式子"
            );
        }
        let mut out = [0u8; 8];
        let (set1, len1) = expanded_set("abcdefghi");
        let (set2, len2) = expanded_set("x");
        let len = translate(b"abc", &set1[..len1], &set2[..len2], tail_of("x"), &mut out).unwrap();
        assert_eq!(&out[..len], b"xxx", "九枚成员不是错误，是一枚都不少地映射");
    }

    /// 只取失败出口：本模块的语法句断言都走这里，第二串位（重复式合法的那一侧）。
    fn set_error(text: &str) -> SetError {
        let mut expanded = [0u8; 256];
        parse_set(text, SetSite::String2, &mut expanded)
            .err()
            .unwrap_or_else(|| panic!("{text:?} 本应报错"))
    }

    fn grammar(text: &str, site: SetSite) -> SetGrammar {
        let mut expanded = [0u8; 256];
        match parse_set(text, site, &mut expanded) {
            Err(SetError::Grammar(g)) => g,
            other => panic!("{text:?} 在 {site:?} 应为语法句，实得 {other:?}"),
        }
    }

    #[test]
    fn test_translate_basic() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("XYZ");
        let mut out = [0u8; 16];
        let len = translate(
            b"axbxc",
            &set1[..len1],
            &set2[..len2],
            Set2Tail::LastByte,
            &mut out,
        )
        .unwrap();
        // Only set members translate; `x` passes through twice.
        assert_eq!(&out[..len], b"XxYxZ");
    }

    #[test]
    fn test_empty_first_set_is_the_identity_map() {
        // `tr.c:178-181` 先把映射表填成恒等，`:214-215` 的空 string1 让循环一次也不进入，
        // 于是没有任何位置被改写过——参照件 `tr '' x` 因此逐字节照抄标准输入且退 0
        let (set2, len2) = expanded_set("XYZ");
        let mut out = [0u8; 16];
        let len = translate(b"aXb", &[], &set2[..len2], Set2Tail::LastByte, &mut out).unwrap();
        assert_eq!(&out[..len], b"aXb");
    }

    #[test]
    fn test_short_second_set_repeats_last() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("X");
        let mut out = [0u8; 16];
        let len = translate(
            b"abc",
            &set1[..len1],
            &set2[..len2],
            Set2Tail::LastByte,
            &mut out,
        )
        .unwrap();
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
    fn a_reversed_range_is_handed_back_as_literals() {
        // genrange() 的 `stopval < lastch` 把指针退回 `savestart` 并返回 0，主循环于是
        // 按字面量吃：`tr z-a y` 的集合是 z、-、a 三枚成员且退 0，不是「非法区间」
        let (set, len) = expanded_set("z-a");
        assert_eq!(len, 3);
        assert_eq!(&set[..len], [b'z', b'-', b'a']);
    }

    #[test]
    fn an_unknown_class_says_which_name_it_rejected() {
        assert_eq!(
            set_error("[:nope:]"),
            SetError::Grammar(SetGrammar::UnknownClass("nope".to_string()))
        );
        assert_eq!(
            SetGrammar::UnknownClass("nope".to_string()).sentence(),
            "unknown class nope"
        );
    }

    #[test]
    fn a_repeat_in_string1_is_refused_before_its_own_shape_is_read() {
        // genseq() 的第一句判据在吃任何字符之前，所以星号、个数长什么样都轮不到发言
        for text in ["[a*2]", "[ab*2]", "[x*]", "[x*1a]"] {
            assert_eq!(
                grammar(text, SetSite::String1),
                SetGrammar::SequencesOnlyInString2,
                "{text:?}"
            );
        }
        assert_eq!(
            SetGrammar::SequencesOnlyInString2.sentence(),
            "Sequences only valid in string2"
        );
        // 同一枚式子在第二串位上合法，证明分流按串位而不是按式子本身
        let (set, len) = expanded_set("[a*2]");
        assert_eq!(len, 2);
        assert_eq!(&set[..len], [b'a', b'a']);
    }

    #[test]
    fn the_asterisk_is_checked_before_the_count() {
        // 星号那一句在前：`[ab*2]` 里 `a` 之后跟着 `b`，谈不上「个数非法」
        for text in ["[ab*2]", "[abc*2]", "[a b*2]"] {
            assert_eq!(
                grammar(text, SetSite::String2),
                SetGrammar::MisplacedSequenceAsterisk,
                "{text:?}"
            );
        }
        assert_eq!(
            SetGrammar::MisplacedSequenceAsterisk.sentence(),
            "Misplaced sequence asterisk"
        );
    }

    #[test]
    fn an_unparsable_count_says_illegal_sequence_count() {
        for text in ["[x*1a]", "[x**2]", "[x*-1]", "[x*y]"] {
            assert_eq!(
                grammar(text, SetSite::String2),
                SetGrammar::IllegalSequenceCount,
                "{text:?}"
            );
        }
        assert_eq!(
            SetGrammar::IllegalSequenceCount.sentence(),
            "illegal sequence count"
        );
    }

    #[test]
    fn an_open_repeat_means_repeat_forever_not_empty() {
        // `case ']'` 与「数字但值为 0」都汇到 `s->cnt ? SEQUENCE : INFINITE` 的 INFINITE 侧
        for text in ["[x*]", "[x*0]"] {
            let (set, len) = expanded_set(text);
            assert_eq!(len, 1);
            assert_eq!(&set[..len], [b'x'], "{text:?}");
        }
        let (set1, len1) = expanded_set("ab");
        let (set2, len2) = expanded_set("[x*]");
        let mut out = [0u8; 8];
        let len = translate(
            b"aabb",
            &set1[..len1],
            &set2[..len2],
            Set2Tail::LastByte,
            &mut out,
        )
        .unwrap();
        assert_eq!(&out[..len], b"xxxx");
    }

    #[test]
    fn an_equivalence_with_a_second_member_blames_the_equals_sign() {
        // `genequiv()` 的成员腿只吃**一枚**字符，紧接着那枚必须是 `=`
        // （`str.c:307-320`）。式尾有没有 `=]` 是 `bracket()` 事先认过的
        // （`str.c:193-199`），所以 `[=ab=]` 这类「像成形而内部多塞」的式子走到这里，
        // 报的是这一句。参照件四形现量同一句、同退 1（`tmp/tr-341/measure341.log`）。
        for text in ["[=ab=]", "[=abc=]", "[=a b=]", "[==]"] {
            for site in [SetSite::String1, SetSite::String2] {
                assert_eq!(
                    grammar(text, site),
                    SetGrammar::MisplacedEquivalenceEquals,
                    "{text:?}"
                );
            }
        }
        assert_eq!(
            SetGrammar::MisplacedEquivalenceEquals.sentence(),
            "Misplaced equivalence equals sign"
        );
    }

    #[test]
    fn an_equivalence_bracketed_by_more_than_the_equals_blames_the_right_bracket() {
        // `[=a=b=]` 撞的是**下一句**（`str.c:321-323`）：成员 `a` 之后确实是 `=`，
        // 但 `=` 之后是 `b` 而不是 `]`。`bracket()` 找的是式尾那处 `=]`，
        // 于是「认下过」与「核得过」不是一回事——两串位同判。
        for site in [SetSite::String1, SetSite::String2] {
            assert_eq!(
                grammar("[=a=b=]", site),
                SetGrammar::MisplacedEquivalenceRightBracket
            );
        }
        assert_eq!(
            SetGrammar::MisplacedEquivalenceRightBracket.sentence(),
            "Misplaced equivalence right bracket"
        );
    }

    #[test]
    fn the_equivalence_member_leg_consumes_two_bytes_for_an_escape() {
        // 成员腿写作 `if (*s->str == '\\') s->equiv[0] = backslash(s);`（`str.c:309-312`），
        // `backslash()` 一次走过两枚字节。于是 `[=\=]` 在 `\=` 之后剩下的是 `]` 而不是 `=`，
        // 报等号那一句（参照件同形同句）；`[=\n=]` 则正常收成**换行符**这一枚成员
        // （参照件现量 `tr '[=\n=]' x` 把输入里的换行换成了 `x`）。
        assert_eq!(
            grammar(r"[=\=]", SetSite::String1),
            SetGrammar::MisplacedEquivalenceEquals
        );
        let (set, len) = expanded_set(r"[=\n=]");
        assert_eq!((len, &set[..len]), (1, &[b'\n'][..]));
        let (set, len) = expanded_set(r"[=\t=]");
        assert_eq!((len, &set[..len]), (1, &[b'\t'][..]));
    }

    #[test]
    fn an_unshaped_bracket_is_a_literal_bracket() {
        // `bracket()` 返回假 ⇒ `next()` FALLTHROUGH 成一枚字面 `[`：这四形在参照件里都
        // 退 0，少抄一条判据就会在这里多报一句假话，所以逐形钉成字面量成员
        for (text, want) in [
            ("[x]", "[x]".as_bytes()),
            ("[x]y*", "[x]y*".as_bytes()),
            ("[x", "[x".as_bytes()),
            ("[:alpha", "[:alpha".as_bytes()),
        ] {
            let (set, len) = expanded_set(text);
            assert_eq!(&set[..len], want, "{text:?}");
        }
    }

    #[test]
    fn a_wellformed_equivalence_is_its_own_single_member() {
        // 参照件 `tr_ref abc [=c=]` 退 0；它那两句「放错了等号 / 右括号」要的是式内
        // 再夹字符的形，本支未接，登记为 TR-EQUIV-SENTENCES
        let (set, len) = expanded_set("[=c=]");
        assert_eq!(len, 1);
        assert_eq!(&set[..len], [b'c']);
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
