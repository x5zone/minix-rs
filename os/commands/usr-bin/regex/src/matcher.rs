//! The matcher trait: one interface, one implementation per strategy.
//!
//! The C `grep` switches strategies by flag: fixed strings (`-F`) skip the
//! regex compiler entirely, while `-E`/`-G` compile a pattern. Both answer
//! the same question ("where does it match?"), so both implement
//! [`Matcher`]. The program layer depends on the trait and stays unchanged
//! while strategies come and go — the same dependency inversion the stage's
//! sibling crates apply between user programs and their data sources.

use crate::RegexError;
use crate::pattern::{Captures, Pattern, compile_basic, compile_extended};

/// One matching strategy: find the leftmost match span in a haystack.
pub trait Matcher {
    /// Leftmost `(start, end)` byte offsets, or `None` for no match.
    fn find_in(&self, haystack: &str) -> Option<(usize, usize)>;
    /// Leftmost match with captures (strategies without captures report
    /// only the whole span in slot 0).
    fn find_captures(&self, haystack: &str) -> Option<(usize, usize, Captures)>;
}

/// Fixed string search: the `-F` strategy.
///
/// Ground truth: `minix3/minix/usr.bin/grep/grep.c:67` (`-F` reads the
/// pattern as a list of fixed strings) with matching in `util.c`. No
/// pattern metacharacter is special; the needle must appear verbatim.
pub struct SubstringMatcher<'a> {
    /// The literal needle.
    pub needle: &'a str,
}

impl Matcher for SubstringMatcher<'_> {
    fn find_in(&self, haystack: &str) -> Option<(usize, usize)> {
        haystack
            .find(self.needle)
            .map(|start| (start, start + self.needle.len()))
    }

    fn find_captures(&self, haystack: &str) -> Option<(usize, usize, Captures)> {
        self.find_in(haystack).map(|(start, end)| {
            let mut caps = Captures::empty();
            caps.spans[0] = Some((start as u32, end as u32));
            (start, end, caps)
        })
    }
}

/// Regex search: the default, `-E`, and `-G` strategies over the engine in
/// [`crate::pattern`].
pub struct RegexMatcher {
    /// The compiled pattern.
    pub pattern: Pattern,
}

impl RegexMatcher {
    /// Compile with the basic spelling.
    pub fn basic(text: &str) -> Result<Self, RegexError> {
        Ok(RegexMatcher {
            pattern: compile_basic(text)?,
        })
    }

    /// Compile with the extended spelling.
    pub fn extended(text: &str) -> Result<Self, RegexError> {
        Ok(RegexMatcher {
            pattern: compile_extended(text)?,
        })
    }
}

impl Matcher for RegexMatcher {
    fn find_in(&self, haystack: &str) -> Option<(usize, usize)> {
        self.pattern
            .find(haystack)
            .map(|(start, end, _)| (start, end))
    }

    fn find_captures(&self, haystack: &str) -> Option<(usize, usize, Captures)> {
        self.pattern.find(haystack)
    }
}

/// True when `byte` can sit inside a word for `-w` purposes.
pub fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Leftmost match whose neighbours are not word bytes: the `-w` strategy
/// (`minix3/minix/usr.bin/grep/grep.c:85`, implemented at line 186).
pub fn find_word<M: Matcher>(matcher: &M, haystack: &str) -> Option<(usize, usize)> {
    let bytes = haystack.as_bytes();
    let mut search_from = 0;
    loop {
        let (start, end) = matcher.find_in(&haystack[search_from..])?;
        let (start, end) = (start + search_from, end + search_from);
        let left_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
        if left_ok && right_ok {
            return Some((start, end));
        }
        search_from = start + char_advance(bytes, start);
    }
}

/// Advance one character from `pos` (multibyte aware, never zero).
fn char_advance(bytes: &[u8], pos: usize) -> usize {
    if pos >= bytes.len() {
        return 1;
    }
    let lead = bytes[pos];
    if lead < 0x80 || (0x80..0xC0).contains(&lead) {
        1
    } else if lead >> 5 == 0b110 {
        2.min(bytes.len() - pos)
    } else if lead >> 4 == 0b1110 {
        3.min(bytes.len() - pos)
    } else if lead >> 3 == 0b11110 {
        4.min(bytes.len() - pos)
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substring_finds_verbatim() {
        let matcher = SubstringMatcher { needle: "a.c" };
        assert_eq!(matcher.find_in("xa.cy"), Some((1, 4)));
        // The dot is literal here, unlike in a pattern.
        assert_eq!(matcher.find_in("xabcy"), None);
    }

    #[test]
    fn test_regex_matcher_delegates() {
        let matcher = RegexMatcher::basic("a.c").unwrap();
        assert_eq!(matcher.find_in("xabcy"), Some((1, 4)));
        let extended = RegexMatcher::extended("(a|b)+").unwrap();
        assert!(extended.find_in("ccabcaa").is_some());
    }

    #[test]
    fn test_word_match_rejects_partial() {
        let matcher = SubstringMatcher { needle: "cat" };
        // `concatenate` holds `cat` but not as a word.
        assert_eq!(find_word(&matcher, "a cat sat"), Some((2, 5)));
        assert_eq!(find_word(&matcher, "concatenate"), None);
    }

    #[test]
    fn test_matchers_share_the_trait() {
        let fixed = SubstringMatcher { needle: "hi" };
        let regex = RegexMatcher::basic("h.").unwrap();
        let matchers: [&dyn Matcher; 2] = [&fixed, &regex];
        for matcher in matchers {
            assert_eq!(matcher.find_in("say hi"), Some((4, 6)));
        }
    }
}
