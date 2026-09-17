//! Byte comparison for `cmp`.
//!
//! Ground truth: `minix3/usr.bin/cmp/` (NetBSD): the compare loop
//! (`regular.c:70-120`) walks both byte streams from the skip offsets
//! with byte and line counters starting at one; the first difference
//! prints "%s %s differ: char %lld, line %lld" and exits 1 (`diffmsg`,
//! misc.c:78-85) unless `-l` lists every difference as
//! "%6lld %3o %3o" (regular.c:98) with only the status leaving; `-s`
//! prints nothing at all. A shorter stream is reported by `eofmsg`
//! (misc.c:59-73): "EOF on %s", or with `-l` "EOF on %s: char %lld,
//! line %lld" — and the EOF names the *shorter* file
//! (`regular.c:119`: `len1 > len2 ? file2 : file1`). Line numbers count
//! the first stream's newlines.

use alloc::string::String;
use alloc::vec::Vec;

/// Everything the doing half needs to render one `cmp` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmpOutcome {
    /// True when the compared ranges are byte identical.
    pub equal: bool,
    /// The first difference as (byte offset from one, line from one).
    pub first_difference: Option<(u64, u64)>,
    /// Every difference as (byte, first-stream byte, second-stream
    /// byte) — populated only when `list_all` was set.
    pub byte_list: Vec<(u64, u8, u8)>,
    /// A stream ended early: (which stream, 1 or 2, plus the byte and
    /// line where the longer one continued).
    pub eof_on: Option<(u8, u64, u64)>,
}

/// Compares two byte streams after their skip offsets have been applied
/// by the caller. `list_all` collects every difference (the `-l`
/// report); without it only the first difference is recorded.
pub fn compare_bytes(a: &[u8], b: &[u8], list_all: bool) -> CmpOutcome {
    let mut outcome = CmpOutcome {
        equal: true,
        first_difference: None,
        byte_list: Vec::new(),
        eof_on: None,
    };
    let mut byte: u64 = 1;
    let mut line: u64 = 1;
    let length = a.len().min(b.len());
    for index in 0..length {
        let (ca, cb) = (a[index], b[index]);
        if ca != cb {
            outcome.equal = false;
            if outcome.first_difference.is_none() {
                outcome.first_difference = Some((byte, line));
            }
            if list_all {
                outcome.byte_list.push((byte, ca, cb));
            } else {
                return outcome;
            }
        }
        if ca == b'\n' {
            line += 1;
        }
        byte += 1;
    }
    if a.len() != b.len() {
        outcome.equal = false;
        let shorter = if a.len() > b.len() { 2 } else { 1 };
        outcome.eof_on = Some((shorter, byte, line));
    }
    outcome
}

/// The `-l` line for one difference: `%6lld %3o %3o` (regular.c:98).
pub fn format_list_entry(entry: (u64, u8, u8), out: &mut String) {
    use core::fmt::Write;
    let (byte, left, right) = entry;
    let _ = write!(out, "{:6} {:3o} {:3o}\n", byte, left, right);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_streams() {
        let outcome = compare_bytes(b"same\n", b"same\n", false);
        assert!(outcome.equal);
        assert!(outcome.first_difference.is_none());
        assert!(outcome.eof_on.is_none());
    }

    #[test]
    fn test_first_difference_reports_byte_and_line() {
        let outcome = compare_bytes(b"one\ntwo\n", b"one\nTWO\n", false);
        assert!(!outcome.equal);
        // 't' of two versus 'T' of TWO: byte 5, on line 2.
        assert_eq!(outcome.first_difference, Some((5, 2)));
        assert!(outcome.byte_list.is_empty());
    }

    #[test]
    fn test_list_all_collects_every_difference() {
        let outcome = compare_bytes(b"aXcYe\n", b"aBcDe\n", true);
        assert!(!outcome.equal);
        assert_eq!(
            outcome.byte_list,
            vec![(2, b'X', b'B'), (4, b'Y', b'D')]
        );
    }

    #[test]
    fn test_eof_names_the_shorter_stream() {
        // regular.c:119 names the shorter stream: the first stream here.
        let outcome = compare_bytes(b"ab", b"abc", false);
        assert_eq!(outcome.eof_on, Some((1, 3, 1)));
        // Second stream shorter: the EOF names stream two
        // (regular.c:119's ternary picks file2 when len1 > len2).
        let outcome = compare_bytes(b"abc\n", b"ab", false);
        assert_eq!(outcome.eof_on, Some((2, 3, 1)));
    }

    #[test]
    fn test_line_counter_follows_the_first_stream() {
        let outcome = compare_bytes(b"a\nb\nc", b"a\nb\nc\nd\n", true);
        // Five bytes compared (byte counter now 6); the two newlines of
        // stream one put the counter on line 3.
        assert_eq!(outcome.eof_on, Some((1, 6, 3)));
    }
}
