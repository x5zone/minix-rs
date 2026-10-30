//! A port of `fnmatch(3)`: match one pattern against a file name or path.
//!
//! The source of truth is `minix3/lib/libc/gen/fnmatch.c` (NetBSD 1.26, 222
//! lines). `find`'s three predicates `-name`/`-iname`/`-path` use it directly
//! (`minix3/usr.bin/find/function.c:1282-1310`, `:1404-1410`), with only two
//! argument values, `0` and `FNM_CASEFOLD`; the whole set is ported here so
//! flag semantics are not reinvented in this tree.
//!
//! Three details that are easy to simplify are kept as C has them:
//! - A `/` inside brackets never matches: when `rangematch` meets a slash it
//!   returns the "malformed" sentinel (`fnmatch.c:94-95`), so the caller takes
//!   the `[` literally and looks again (`:186-200`).
//! - A leading `^` means the same as `!` (`fnmatch.c:82-88` says so to agree
//!   with regex syntax; POSIX leaves it undefined).
//! - `FNM_PERIOD` asks "is this the start of the string, or under
//!   `FNM_PATHNAME` is the previous character a `/`": the `?` branch is at
//!   `fnmatch.c:140-143`, the `*` branch at `:152-155`.
//! - When the recursion budget runs out C returns `FNM_NORES` (`fnmatch.c:126-127`
//!   tests it, `:219-221` supplies the 64 levels); this layer expresses the
//!   same thing as `Option`'s `None`, and the `find` side takes it as "non-zero
//!   is false" (`minix3/usr.bin/find/function.c:1282-1286`).

/// The flag bits of `minix3/include/fnmatch.h`.
pub mod flags {
    /// A path separator is not an ordinary character (`*` does not cross `/`).
    pub const PATHNAME: i32 = 0x01;
    /// A leading period must be matched by a literal period in the pattern.
    pub const PERIOD: i32 = 0x02;
    /// A backslash is not an escape character.
    pub const NOESCAPE: i32 = 0x04;
    /// Case-insensitive (`find`'s `-iname` uses this one).
    pub const CASEFOLD: i32 = 0x10;
    /// Allow matching a truncated path at a `/` (`fnmatch.c:131-134`).
    pub const LEADING_DIR: i32 = 0x20;
}

/// C's `FNM_NOMATCH`/`FNM_NORES` semantics are expressed here as `Option`:
/// `Some(true)` matches, `Some(false)` does not, and `None` is exhausted
/// recursion (`FNM_NORES`).
pub fn matches(pattern: &str, string: &str, flags: i32) -> Option<bool> {
    match_one(pattern.as_bytes(), string.as_bytes(), flags, 64)
}

/// `foldcase()`: folds only when `FNM_CASEFOLD` is set and the byte is
/// uppercase (`fnmatch.c:62-71`). It works on ASCII, matching the treatment of
/// `isupper`/`tolower` elsewhere in this tree.
fn fold(byte: u8, flags: i32) -> u8 {
    if flags & flags::CASEFOLD != 0 && byte.is_ascii_uppercase() {
        return byte + b'a' - b'A';
    }
    byte
}

/// `fnmatchx()` (`fnmatch.c:118-215`). The recursion depth limit is the 64
/// hardcoded at C's call site (`fnmatch.c:219-221`).
fn match_one(pattern: &[u8], string: &[u8], flags: i32, mut depth: usize) -> Option<bool> {
    let mut p = 0usize;
    let mut s = 0usize;
    loop {
        if depth == 0 {
            return None;
        }
        depth -= 1;
        let c = match pattern.get(p) {
            // End of pattern: with `FNM_LEADING_DIR`, a string that stops at
            // `/` also passes.
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
                // Consecutive `*` collapse into one (`fnmatch.c:147-150`).
                let mut next = pattern.get(p).copied().unwrap_or(b'\0');
                while next == b'*' {
                    p += 1;
                    next = pattern.get(p).copied().unwrap_or(b'\0');
                }
                if at_period(string, s, flags) {
                    return Some(false);
                }
                if next == b'\0' {
                    // A trailing `*`: without PATHNAME it passes outright;
                    // with PATHNAME the remainder must contain no `/`.
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
                // The general case: try the rest of the pattern at each
                // position (`fnmatch.c:171-181`).
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
                    // A `/` met in the bracket expression: retry with `[` as a
                    // literal (`fnmatch.c:94-95`, `:186-200`).
                    RangeMatch::BadSlash => {
                        if cur != b'[' {
                            return Some(false);
                        }
                    }
                    RangeMatch::Rest(next) => {
                        // `range_match` was given `pattern[p..]`, so the index
                        // it returns must be added.
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
                    // A backslash at the end of the pattern compares against
                    // a real NUL (`fnmatch.c:201-208`).
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

/// Whether the period at this position counts as "the period that opens the
/// string of this match" (when `FNM_PERIOD` is in force a wildcard cannot match
/// it).
/// C's test is `string == stringstart` (`fnmatch.c:140-143`, `:152-155`), that
/// is, "index 0 in the string of this call"; recursion passes substrings, so
/// testing the index means the same as testing the pointer.
fn at_period(string: &[u8], s: usize, flags: i32) -> bool {
    flags & flags::PERIOD != 0
        && string.get(s) == Some(&b'.')
        && (s == 0 || (flags & flags::PATHNAME != 0 && string[s - 1] == b'/'))
}

/// The three exits of `rangematch()`.
enum RangeMatch {
    /// No match (C returns `NULL`).
    NoMatch,
    /// A `/` appeared in the bracket expression, so the whole bracket form is
    /// malformed (C returns `(void *)-1`).
    BadSlash,
    /// A match, giving the index in the pattern after the bracket (C returns
    /// a pointer to the remaining string).
    Rest(usize),
}

/// `rangematch()` (`fnmatch.c:74-116`): consume the contents of one `[...]`
/// and decide whether `test` is inside.
fn range_match(pattern: &[u8], test: u8, flags: i32) -> RangeMatch {
    let mut at = 0usize;
    let negate = matches!(pattern.first(), Some(&b'!') | Some(&b'^'));
    if negate {
        at += 1;
    }
    let mut ok = false;
    // C's loop condition is `(c = *pattern++) != ']' || need`, i.e. the
    // **first** character is consumed unconditionally, even when it is `]` (in
    // `[]]` that position really is a literal `]`).
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
        // A range: `-` is followed by a character that is not the closing `]`.
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

/// Convenience: fold `Option<bool>` into a bool, treating exhausted recursion
/// as "no match" — the `find` side needs only one truth value, and C's `f_name`
/// likewise takes any non-zero as false (`function.c:1282-1286`).
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
        // `^` means the same as `!` (the choice at `fnmatch.c:82-88`).
        assert!(hit("[^abc]", "d"));
        assert!(hit("[a-c]", "b"));
        assert!(!hit("[a-c]", "d"));
        assert!(hit("[]]", "]"));
        assert!(hit("[a-]", "-"));
    }

    #[test]
    fn a_slash_inside_a_bracket_never_matches() {
        // When `rangematch` meets `/` it declares the form malformed, so only
        // a literal `[` can still match (`fnmatch.c:94-95` and `:186-200`).
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
        // As POSIX puts it: a leading `*` does not cover a leading period
        // either, so this shape is false too.
        assert!(!hitf("*.profile", ".profile", flags::PERIOD));
        // A period that is not at the start is not governed by `FNM_PERIOD`.
        assert!(hitf("*.profile", "x.profile", flags::PERIOD));
        // Only the start of a component counts: a period in the middle is not
        // governed.
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
        // The pattern ends with a backslash: it compares against a NUL, that
        // is, the end of the string.
        assert!(!hit(r"ab\", "ab"));
    }

    #[test]
    fn leading_dir_stops_at_the_first_slash() {
        assert!(hitf("a*", "a/b", flags::LEADING_DIR));
        // Without `FNM_PATHNAME`, `/` is just an ordinary character (`find`'s
        // `-name` compares a name, and `-path` compares the whole path but also
        // without that flag; see `function.c:1404-1410`).
        assert!(hit("a*", "a/b"));
        assert!(!hitf("a*", "a/b", flags::PATHNAME));
        // An empty pattern matches only an empty string; `LEADING_DIR` takes
        // effect only when the pattern is exhausted while the string stops at a
        // `/` (`fnmatch.c:131-134`), so a character must be left for the part
        // before the slash.
        assert!(!hitf("", "a/b", flags::LEADING_DIR));
        assert!(hitf("a", "a/b", flags::LEADING_DIR));
    }

    #[test]
    fn an_empty_pattern_only_matches_an_empty_name() {
        assert!(hit("", ""));
        assert!(!hit("", "x"));
    }
}
