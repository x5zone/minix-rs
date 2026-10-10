//! `look` — find lines in a sorted list that begin with a prefix.
//!
//! Ground truth: `minix3/minix/commands/look/look.c`. The real content is
//! `compare()` (`look.c:24-58`), which walks the search string and a candidate
//! word a *significant character* at a time and returns 0 when the whole search
//! string is consumed — i.e. the word begins with it. Two flags reshape the
//! walk: `-d` (dictionary) ignores every character that is not alphanumeric or
//! a space, and collapses runs of spaces to a single one (`look.c:33-40,46-53`);
//! `-f` folds upper case to lower before comparing (`look.c:41,54`). A search
//! string that runs out first returns 0 (a match); a word that runs out first
//! returns 1 (shorter than the string); otherwise the byte difference decides
//! ordering.
//!
//! C locates the run of matching lines with a byte-offset binary search over
//! the file (`look.c:103-125`) and then prints every line whose `compare` is 0.
//! Because `look` is defined over a *sorted* wordlist, that search and the
//! linear filter `look_matches` here select exactly the same lines — the
//! ordering of the returned lines matches the file, and the print is the raw
//! word line (`look.c:124`).
//!
//! Declared residual: C aborts (`nonascii()`, `look.c:18-22`) on any byte
//! outside ASCII encountered while comparing; this ASCII-oriented model does
//! not, so high bytes are simply compared by value (out of scope for a plain
//! dictionary). The `MAXLEN` truncation error in `readword` (`look.c:76-79`) is
//! likewise not reproduced.

use alloc::vec::Vec;

/// Search options: the C `dflg` (dictionary) and `fflg` (fold case) flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LookOptions {
    /// `-d`: compare only alphanumerics and spaces, collapsing space runs.
    pub dictionary: bool,
    /// `-f`: fold upper case to lower before comparing.
    pub fold: bool,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn is_alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

/// The C `compare()` (`look.c:24-58`), returning 0 when `prefix` is fully
/// consumed against `word`, 1 when `word` runs out first, else the difference
/// of the first mismatching significant bytes.
fn compare(prefix: &[u8], word: &[u8], opts: &LookOptions) -> i32 {
    let (d, f) = (opts.dictionary, opts.fold);
    let (mut pi, mut wi) = (0usize, 0usize);
    loop {
        // Next significant prefix byte: skip non-space/non-alnum under -d.
        let mut cp;
        loop {
            if pi >= prefix.len() {
                return 0;
            }
            cp = prefix[pi];
            pi += 1;
            if !(d && !is_space(cp) && !is_alnum(cp)) {
                break;
            }
        }
        if d && is_space(cp) {
            while pi < prefix.len() && is_space(prefix[pi]) {
                pi += 1;
            }
            cp = b' ';
        }
        if f && cp.is_ascii_uppercase() {
            cp += 32;
        }
        // Next significant word byte, same treatment.
        let mut cw;
        loop {
            if wi >= word.len() {
                return 1;
            }
            cw = word[wi];
            wi += 1;
            if !(d && !is_space(cw) && !is_alnum(cw)) {
                break;
            }
        }
        if d && is_space(cw) {
            while wi < word.len() && is_space(word[wi]) {
                wi += 1;
            }
            cw = b' ';
        }
        if f && cw.is_ascii_uppercase() {
            cw += 32;
        }
        if cp != cw {
            return cp as i32 - cw as i32;
        }
    }
}

/// Return every line in `lines` whose `compare` against `prefix` is 0 — the
/// lines that begin with `prefix` under the search options.
pub fn look_matches<'a>(prefix: &[u8], lines: &[&'a [u8]], opts: &LookOptions) -> Vec<&'a [u8]> {
    lines
        .iter()
        .copied()
        .filter(|line| compare(prefix, line, opts) == 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(dictionary: bool, fold: bool) -> LookOptions {
        LookOptions { dictionary, fold }
    }

    #[test]
    fn exact_prefix_matches() {
        assert_eq!(compare(b"ap", b"apple", &opts(false, false)), 0);
        assert_eq!(compare(b"ap", b"ape", &opts(false, false)), 0);
    }

    #[test]
    fn shorter_word_is_positive() {
        // "a" runs out while "ap" still wants a byte → 1 (not a match).
        assert_eq!(compare(b"ap", b"a", &opts(false, false)), 1);
    }

    #[test]
    fn mismatch_returns_byte_difference() {
        assert!(compare(b"ap", b"banana", &opts(false, false)) < 0);
        assert!(compare(b"b", b"apple", &opts(false, false)) > 0);
    }

    #[test]
    fn fold_case_flag() {
        assert_eq!(compare(b"AP", b"apple", &opts(false, true)), 0);
        // Without -f the case differs → not a match.
        assert!(compare(b"AP", b"apple", &opts(false, false)) != 0);
    }

    #[test]
    fn dictionary_ignores_punctuation() {
        // "ab-cd" reduces to a,b,c,d; "abcdef" begins with those → match.
        assert_eq!(compare(b"ab-cd", b"abcdef", &opts(true, false)), 0);
        // Without -d the '-' is compared literally → mismatch.
        assert!(compare(b"ab-cd", b"abcdef", &opts(false, false)) != 0);
    }

    #[test]
    fn dictionary_collapses_space_runs() {
        assert_eq!(compare(b"a   b", b"a b", &opts(true, false)), 0);
    }

    #[test]
    fn look_selects_prefix_subset_in_order() {
        let lines: [&[u8]; 4] = [b"ape", b"apple", b"apply", b"banana"];
        let got = look_matches(b"ap", &lines, &opts(false, false));
        assert_eq!(got, vec![&b"ape"[..], &b"apple"[..], &b"apply"[..]]);
    }

    #[test]
    fn empty_prefix_matches_every_line() {
        // A consumed prefix returns 0 immediately, so all lines match.
        let lines: [&[u8]; 2] = [b"x", b"y"];
        assert_eq!(look_matches(b"", &lines, &opts(false, false)), vec![&b"x"[..], &b"y"[..]]);
    }
}
