//! `fnmatch(3)` 的移植：拿一条模式去比一个文件名或路径。
//!
//! 真源是 `minix3/lib/libc/gen/fnmatch.c`（NetBSD 1.26，222 行）。`find` 的
//! `-name`/`-iname`/`-path` 三判据直接用它（`minix3/usr.bin/find/function.c:1282-1310`、
//! `:1404-1410`），参数只有 `0` 与 `FNM_CASEFOLD` 两种；这里把整套都搬过来，
//! 好让旗标语义不在本树里被重新发明。
//!
//! 三处容易被简化的细节，都按 C 保留：
//! - 括号里的 `/` 永远不匹配：`rangematch` 撞上斜杠就返回「写法不合法」的哨兵
//!   （`fnmatch.c:94-95`），调用方遂把 `[` 当字面量再看一眼（`:186-200`）。
//! - 开头的 `^` 与 `!` 同义（`fnmatch.c:82-88` 的注释明说为与正则语法一致，POSIX 未定义）。
//! - `FNM_PERIOD` 看「是否串首，或在 `FNM_PATHNAME` 下上一个字符是 `/`」：
//!   `?` 一支在 `fnmatch.c:140-143`，`*` 一支在 `:152-155`。
//! - 递归层数用尽时 C 返回 `FNM_NORES`（`fnmatch.c:126-127` 判、`:219-221` 给 64 层）；
//!   本层用 `Option` 的 `None` 表达同一件事，`find` 那侧按「非 0 即假」收
//!   （`minix3/usr.bin/find/function.c:1282-1286`）。

/// `minix3/include/fnmatch.h` 的旗标位。
pub mod flags {
    /// 路径分隔符不当普通字符处理（`*` 不跨 `/`）。
    pub const PATHNAME: i32 = 0x01;
    /// 句点开头的名字必须被模式里的字面句点匹配。
    pub const PERIOD: i32 = 0x02;
    /// 反斜杠不作转义符。
    pub const NOESCAPE: i32 = 0x04;
    /// 大小写不敏感（`find` 的 `-iname` 用这一条）。
    pub const CASEFOLD: i32 = 0x10;
    /// 允许在 `/` 处截尾匹配（`fnmatch.c:131-134`）。
    pub const LEADING_DIR: i32 = 0x20;
}

/// C 的 `FNM_NOMATCH`/`FNM_NORES` 语义在这里用 `Option` 表达：
/// `Some(true)` 匹配、`Some(false)` 不匹配、`None` 是递归用尽（`FNM_NORES`）。
pub fn matches(pattern: &str, string: &str, flags: i32) -> Option<bool> {
    match_one(pattern.as_bytes(), string.as_bytes(), flags, 64)
}

/// `foldcase()`：只在 `FNM_CASEFOLD` 且是大写时下调（`fnmatch.c:62-71`）。
/// 按 ASCII 处理，与本树其余处对 `isupper`/`tolower` 的口径一致。
fn fold(byte: u8, flags: i32) -> u8 {
    if flags & flags::CASEFOLD != 0 && byte.is_ascii_uppercase() {
        return byte + b'a' - b'A';
    }
    byte
}

/// `fnmatchx()`（`fnmatch.c:118-215`）。递归深度上限即 C 调用侧写死的 64
/// （`fnmatch.c:219-221`）。
fn match_one(pattern: &[u8], string: &[u8], flags: i32, mut depth: usize) -> Option<bool> {
    let mut p = 0usize;
    let mut s = 0usize;
    loop {
        if depth == 0 {
            return None;
        }
        depth -= 1;
        let c = match pattern.get(p) {
            // 模式的 EOS：`FNM_LEADING_DIR` 且串停在 `/` 也算过。
            None => {
                if flags & flags::LEADING_DIR != 0 && string.get(s) == Some(&b'/') {
                    return Some(true);
                }
                return Some(s == string.len());
            }
            Some(&b) => fold(b, flags),
        };
        p += 1;
        match c {
            b'?' => {
                let cur = *string.get(s)?;
                if cur == b'/' && flags & flags::PATHNAME != 0 {
                    return Some(false);
                }
                if at_period(string, s, flags) {
                    return Some(false);
                }
                s += 1;
            }
            b'*' => {
                // 连打多个 `*` 收成一个（`fnmatch.c:147-150`）。
                let mut next = pattern.get(p).copied().unwrap_or(b'\0');
                while next == b'*' {
                    p += 1;
                    next = pattern.get(p).copied().unwrap_or(b'\0');
                }
                if at_period(string, s, flags) {
                    return Some(false);
                }
                if next == b'\0' {
                    // `*` 收尾：不看 PATHNAME 直接过；看了则要求余下没有 `/`。
                    if flags & flags::PATHNAME != 0 {
                        let tail = &string[s..];
                        let ok = flags & flags::LEADING_DIR != 0 || !tail.contains(&b'/');
                        return Some(ok);
                    }
                    return Some(true);
                }
                if next == b'/' && flags & flags::PATHNAME != 0 {
                    match string[s..].iter().position(|&b| b == b'/') {
                        Some(at) => s += at,
                        None => return Some(false),
                    }
                    continue;
                }
                // 一般情形：逐个位置试剩下的模式（`fnmatch.c:171-181`）。
                loop {
                    let test = match string.get(s) {
                        None => return Some(false),
                        Some(&b) => fold(b, flags),
                    };
                    let inner =
                        match_one(&pattern[p..], &string[s..], flags & !flags::PERIOD, depth);
                    match inner {
                        Some(false) => {}
                        other => return other,
                    }
                    if test == b'/' && flags & flags::PATHNAME != 0 {
                        return Some(false);
                    }
                    s += 1;
                }
            }
            b'[' => {
                if string.get(s).is_none() {
                    return Some(false);
                }
                let cur = string[s];
                if cur == b'/' && flags & flags::PATHNAME != 0 {
                    return Some(false);
                }
                match range_match(&pattern[p..], fold(cur, flags), flags) {
                    RangeMatch::NoMatch => return Some(false),
                    // 区间写法里撞见 `/`：把 `[` 当字面量再比一次（`fnmatch.c:94-95`、`:186-200`）。
                    RangeMatch::BadSlash => {
                        if cur != b'[' {
                            return Some(false);
                        }
                    }
                    RangeMatch::Rest(next) => {
                        // `range_match` 拿的是 `pattern[p..]`，故它交回的下标要累加。
                        p += next;
                    }
                }
                s += 1;
            }
            b'\\' if flags & flags::NOESCAPE == 0 => {
                let c = match pattern.get(p) {
                    Some(&b) => {
                        p += 1;
                        fold(b, flags)
                    }
                    // 串尾的反斜杠：比一个真正的 NUL（`fnmatch.c:201-208`）。
                    None => {
                        p -= 1;
                        0
                    }
                };
                if Some(c) != string.get(s).map(|&b| fold(b, flags)) {
                    return Some(false);
                }
                s += 1;
            }
            other => {
                if Some(other) != string.get(s).map(|&b| fold(b, flags)) {
                    return Some(false);
                }
                s += 1;
            }
        }
    }
}

/// 该位置的句点是否算「本次匹配串开头的那个句点」（`FNM_PERIOD` 生效时通配盖不掉它）。
/// C 的判据是 `string == stringstart`（`fnmatch.c:140-143`、`:152-155`），也就是
/// 「在本层调用的串里处于下标 0」；递归换的是子串，故按下标判与按指针判同义。
fn at_period(string: &[u8], s: usize, flags: i32) -> bool {
    flags & flags::PERIOD != 0
        && string.get(s) == Some(&b'.')
        && (s == 0 || (flags & flags::PATHNAME != 0 && string[s - 1] == b'/'))
}

/// `rangematch()` 的三种出口。
enum RangeMatch {
    /// 不匹配（C 返回 `NULL`）。
    NoMatch,
    /// 区间里出现 `/`，整个括号写法判不合法（C 返回 `(void *)-1`）。
    BadSlash,
    /// 匹配，并给出括号之后在模式里的下标（C 返回剩余串的指针）。
    Rest(usize),
}

/// `rangematch()`（`fnmatch.c:74-116`）：吃一个 `[...]` 的内容，判断 `test` 在不在里面。
fn range_match(pattern: &[u8], test: u8, flags: i32) -> RangeMatch {
    let mut at = 0usize;
    let negate = matches!(pattern.first(), Some(&b'!') | Some(&b'^'));
    if negate {
        at += 1;
    }
    let mut ok = false;
    // C 的循环条件是 `(c = *pattern++) != ']' || need`，即**第一个**字符无条件要吃，
    // 哪怕它就是 `]`（`[]]` 里那一位确实是字面 `]`）。
    let mut need = true;
    loop {
        let mut c = match pattern.get(at) {
            Some(&b) => {
                at += 1;
                fold(b, flags)
            }
            None => return RangeMatch::NoMatch,
        };
        if c == b']' && !need {
            break;
        }
        need = false;
        if c == b'/' {
            return RangeMatch::BadSlash;
        }
        if c == b'\\' && flags & flags::NOESCAPE == 0 {
            c = match pattern.get(at) {
                Some(&b) => {
                    at += 1;
                    fold(b, flags)
                }
                None => return RangeMatch::NoMatch,
            };
        }
        // 区间：`-` 后面还有字符且那字符不是收尾的 `]`。
        let range_ahead = pattern.get(at) == Some(&b'-');
        let endpoint = pattern.get(at + 1).map(|&b| fold(b, flags));
        if range_ahead && endpoint.is_some_and(|b| b != b']') {
            let mut c2 = endpoint.unwrap_or(b'\0');
            at += 2;
            if c2 == b'\\' && flags & flags::NOESCAPE == 0 {
                c2 = match pattern.get(at) {
                    Some(&b) => {
                        at += 1;
                        fold(b, flags)
                    }
                    None => return RangeMatch::NoMatch,
                };
            }
            if c <= test && test <= c2 {
                ok = true;
            }
        } else if c == test {
            ok = true;
        }
    }
    if ok == negate {
        RangeMatch::NoMatch
    } else {
        RangeMatch::Rest(at)
    }
}

/// 便捷式：把 `Option<bool>` 折成布尔，递归用尽按「不匹配」处理——`find` 那侧
/// 只需要一个真值，而 C 的 `f_name` 也是把非 0 一律当假（`function.c:1282-1286`）。
pub fn matches_bool(pattern: &str, string: &str, flags: i32) -> bool {
    matches(pattern, string, flags) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(pattern: &str, name: &str) -> bool {
        matches_bool(pattern, name, 0)
    }

    fn hitf(pattern: &str, name: &str, flags: i32) -> bool {
        matches_bool(pattern, name, flags)
    }

    #[test]
    fn literals_and_stars() {
        assert!(hit("foo", "foo"));
        assert!(!hit("foo", "food"));
        assert!(hit("foo*", "food"));
        assert!(hit("*", ""));
        assert!(hit("a*b*c", "abbbc"));
        assert!(hit("**", "anything"));
    }

    #[test]
    fn question_mark_takes_exactly_one_character() {
        assert!(hit("fo?", "foo"));
        assert!(!hit("fo?", "fo"));
        assert!(!hit("fo?", "fooo"));
    }

    #[test]
    fn brackets_cover_lists_negation_and_ranges() {
        assert!(hit("[abc]", "b"));
        assert!(!hit("[abc]", "d"));
        assert!(hit("[!abc]", "d"));
        // `^` 与 `!` 同义（`fnmatch.c:82-88` 的取舍）。
        assert!(hit("[^abc]", "d"));
        assert!(hit("[a-c]", "b"));
        assert!(!hit("[a-c]", "d"));
        assert!(hit("[]]", "]"));
        assert!(hit("[a-]", "-"));
    }

    #[test]
    fn a_slash_inside_a_bracket_never_matches() {
        // `rangematch` 撞上 `/` 就判写法不合法，于是只有字面 `[` 才有救
        // （`fnmatch.c:94-95` 与 `:186-200`）。
        assert!(!hitf("[a/b]", "/", flags::PATHNAME));
        assert!(!hit("[a/b]", "/"));
    }

    #[test]
    fn pathname_flags_keep_stars_from_crossing_slashes() {
        assert!(!hitf("*", "a/b", flags::PATHNAME));
        assert!(hitf("a*", "a/b", flags::PATHNAME | flags::LEADING_DIR));
        assert!(hitf("a/*", "a/b", flags::PATHNAME));
        assert!(!hitf("a/*", "a/b/c", flags::PATHNAME));
        assert!(hitf("*/b", "a/b", flags::PATHNAME));
    }

    #[test]
    fn period_at_the_start_needs_a_literal_dot() {
        assert!(!hitf("*", ".profile", flags::PERIOD));
        assert!(hitf(".*", ".profile", flags::PERIOD));
        // POSIX 的说法：开头的 `*` 也盖不住开头的句点，故这一形同样是假。
        assert!(!hitf("*.profile", ".profile", flags::PERIOD));
        // 句点不在开头就不受 `FNM_PERIOD` 管。
        assert!(hitf("*.profile", "x.profile", flags::PERIOD));
        // 段首才算：中间的句点不受管。
        assert!(hitf("a*b", "a.b", flags::PERIOD));
    }

    #[test]
    fn casefold_makes_upper_and_lower_the_same() {
        assert!(!hit("FOO", "foo"));
        assert!(hitf("FOO", "foo", flags::CASEFOLD));
        assert!(hitf("[A-Z]x", "ax", flags::CASEFOLD));
    }

    #[test]
    fn backslash_escapes_unless_noescape_is_set() {
        assert!(hit(r"a*b", r"a*b"));
        assert!(hit(r"a\*b", "a*b"));
        assert!(hitf(r"a\*b", r"a\*b", flags::NOESCAPE));
        // 模式以反斜杠收尾：它比的是一个 NUL，也就是串的结束。
        assert!(!hit(r"ab\", "ab"));
    }

    #[test]
    fn leading_dir_stops_at_the_first_slash() {
        assert!(hitf("a*", "a/b", flags::LEADING_DIR));
        // 不带 `FNM_PATHNAME` 时 `/` 就是普通字符（`find` 的 `-name` 比的是名字，
        // `-path` 比整条路径但同样不带该旗标，见 `function.c:1404-1410`）。
        assert!(hit("a*", "a/b"));
        assert!(!hitf("a*", "a/b", flags::PATHNAME));
        // 空模式只匹空串；`LEADING_DIR` 是在「模式吃完而串停在 `/`」时才生效
        // （`fnmatch.c:131-134`），故这里要留一个字符给斜杠之前的部分。
        assert!(!hitf("", "a/b", flags::LEADING_DIR));
        assert!(hitf("a", "a/b", flags::LEADING_DIR));
    }

    #[test]
    fn an_empty_pattern_only_matches_an_empty_name() {
        assert!(hit("", ""));
        assert!(!hit("", "x"));
    }
}
