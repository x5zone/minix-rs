//! Line reversal for `rev`.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 57-95:
//! each line's characters print in reverse order with a newline, files
//! (operands) fall back to stdin, and a read or open failure sets status
//! 1. The C reverses wide characters; this slice reverses Rust `char`s —
//! the same character-level guarantee, byte-safe through UTF-8.

use crate::GetoptReport;
use alloc::string::String;
use alloc::vec::Vec;

/// Reverses every line of the collected input (lines split on `\n`, the
/// final newline dropped), returning one reversed line per input line.
/// The walk's outcome. C streams line by line and stops at the first line
/// whose bytes do not decode, so the outcome carries either every line or
/// only the ones before the stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevWalk {
    /// Every line decoded and reversed.
    Done(Vec<String>),
    /// The reversed lines before the first undecodable one. The offending
    /// line and everything after it never appear (C's `fgetwln` returns NULL
    /// there and the loop ends, `rev.c:92-98`).
    Stopped(Vec<String>),
}

/// Reverse the input's lines under this tree's decode contract: **UTF-8,
/// checked line by line** — the environment contract the whole command face
/// runs under, stated as such rather than claimed as a reproduction of C's
/// locale behavior (the cross-board adjudication on `REV-MULTIBYTE` requires
/// exactly that wording). An undecodable byte sequence is where C's
/// `fgetwln` raises `EILSEQ` and the stream stops; the lines before it are
/// still reversed, and the caller reports the error.
pub fn reversed_lines(input: &[u8]) -> RevWalk {
    if input.is_empty() {
        return RevWalk::Done(Vec::new());
    }
    let mut acc: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    while index <= input.len() {
        if index == input.len() || input[index] == b'\n' {
            let line = &input[start..index];
            match core::str::from_utf8(line) {
                Ok(text) => acc.push(text.chars().rev().collect()),
                // The undecodable line is dropped whole: C's `fgetwln`
                // returns NULL for it and the loop never prints it.
                Err(_) => return RevWalk::Stopped(acc),
            }
            if index == input.len() {
                break;
            }
            start = index + 1;
        }
        index += 1;
    }
    // A trailing newline does not produce a trailing empty line (the same
    // rule the old splitter kept).
    if input.ends_with(b"\n") {
        acc.pop();
    }
    RevWalk::Done(acc)
}

/// Reverses already-split lines.
pub fn reversed_lines_of(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.chars().rev().collect())
        .collect()
}

/// `rev`'s option loop, returning the remaining operands (C's `argv += optind`,
/// `rev.c:77-78`).
///
/// C's optstring is the **empty string** (`rev.c:70`), so this branch is the three
/// shapes described by the shared [`crate::empty_optstring_operands_bytes`]: any
/// token starting with `-` counts as an off-charset letter and gets one libc line
/// first, `--` itself is not an operand, and a bare `-` is an operand. After
/// receiving the operands, `rev` `fopen`s them one by one as files
/// (`rev.c:83-106`); that part is `rev`'s own business and belongs to the shell.
///
/// The reported letter is the **first byte** after the dash (`getopt.c:79`'s
/// `optopt = *place++` steps over bytes); the shell-side line must use
/// [`crate::GetoptReport::line_bytes`] to print C's raw byte.
pub fn parse_rev_args<'a>(argv: &'a [&'a [u8]]) -> Result<Vec<&'a [u8]>, GetoptReport> {
    crate::empty_optstring_operands_bytes(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_reverse_character_order() {
        assert_eq!(
            reversed_lines(b"abc\ndef\n"),
            RevWalk::Done(vec!["cba".into(), "fed".into()])
        );
    }

    #[test]
    fn test_unterminated_final_line_still_reverses() {
        assert_eq!(
            reversed_lines(b"abc\ndef"),
            RevWalk::Done(vec!["cba".into(), "fed".into()])
        );
    }

    #[test]
    fn test_multibyte_characters_stay_intact() {
        // Character-level reversal: a two-byte character does not split.
        assert_eq!(
            reversed_lines("éab\n".as_bytes()),
            RevWalk::Done(vec!["baé".into()])
        );
    }

    #[test]
    fn test_empty_input_yields_no_lines() {
        assert!(matches!(reversed_lines(b""), RevWalk::Done(lines) if lines.is_empty()));
    }

    #[test]
    fn a_lone_high_byte_stops_the_walk_before_itself() {
        // The adjudicated error leg: a bare `0xFF` is undecodable, so the
        // walk stops there — nothing is emitted for the offending line.
        assert!(matches!(reversed_lines(b"\xff"), RevWalk::Stopped(lines) if lines.is_empty()));
    }

    #[test]
    fn lines_before_the_invalid_sequence_still_reverse() {
        // C prints as it reads (`rev.c:94-97`), so the already-decoded lines
        // are out; the first undecodable line cuts the walk.
        let got = reversed_lines(b"abc\ndef\n\xc3\nzzz\n");
        assert!(
            matches!(got, RevWalk::Stopped(ref lines) if lines.len() == 2
            && lines[0] == "cba" && lines[1] == "fed")
        );
    }

    #[test]
    fn a_truncated_multibyte_sequence_is_undecodable_too() {
        // A three-byte lead with its continuation bytes missing is as
        // invalid as a lone high byte — the check is whole-line UTF-8, not
        // a byte whitelist.
        assert!(matches!(reversed_lines(b"\xe4\xb8\n"), RevWalk::Stopped(_)));
    }

    /// Pin the option-loop shapes one by one: any letter is off-charset (the first
    /// character is reported), `--` itself is not an operand, a bare `-` is an
    /// operand, and content after a non-option token is not scanned (no
    /// rearrangement).
    #[test]
    fn the_option_ring_takes_no_letters_and_stops_at_the_first_operand() {
        // Empty optstring ⇒ every letter is reported, and what is reported is the
        // first character after `-`.
        assert_eq!(
            parse_rev_args(&["-Z".as_bytes()]),
            Err(GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            parse_rev_args(&["-bZ".as_bytes()]),
            Err(GetoptReport::UnknownOption('b')),
            "聚簇只报第一个字符，因为调用者拿到 '?' 就 usage() 退出"
        );
        assert_eq!(
            parse_rev_args(&["--x".as_bytes()]),
            Err(GetoptReport::UnknownOption('-'))
        );
        // `--` ends the option loop and is itself consumed; a bare `-` stays as an
        // operand.
        assert_eq!(
            parse_rev_args(&["--".as_bytes(), "f".as_bytes()]),
            Ok(vec!["f".as_bytes()])
        );
        assert_eq!(parse_rev_args(&["-".as_bytes()]), Ok(vec!["-".as_bytes()]));
        assert_eq!(parse_rev_args(&["--".as_bytes()]), Ok(vec![]));
        // Stop at the first non-option token: a following `-Z` is no longer treated
        // as an option.
        assert_eq!(
            parse_rev_args(&["f".as_bytes(), "-Z".as_bytes()]),
            Ok(vec!["f".as_bytes(), "-Z".as_bytes()])
        );
        assert_eq!(
            parse_rev_args(&["f".as_bytes(), "g".as_bytes()]),
            Ok(vec!["f".as_bytes(), "g".as_bytes()])
        );
        let nothing: [&[u8]; 0] = [];
        assert_eq!(parse_rev_args(&nothing), Ok(vec![]));
    }

    /// An off-charset letter lands on **bytes**: `é` (`c3 a9`) reports U+00C3, not
    /// U+00E9. The reference `tmp/rev-309/rev_ref` measures `rev -é` →
    /// `unknown option -- ` + raw byte `c3`, one line, then one `usage` line and
    /// exit 1 (`getopt.c:79`'s `optopt = *place++` advances one byte at a time).
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_rev_args(&["-\u{e9}".as_bytes()]),
            Err(GetoptReport::UnknownOption('\u{00c3}')),
            "两字节的字符不能整个当字母报"
        );
        // When a non-ASCII byte is second in a cluster, the report still lands on the
        // first byte (the caller exits as soon as it gets '?').
        assert_eq!(
            parse_rev_args(&["-x\u{e9}".as_bytes()]),
            Err(GetoptReport::UnknownOption('x'))
        );
    }

    /// After the argument seam returns raw bytes, an option letter of a raw `ff` is
    /// no longer folded into U+FFFD: the letter in the report is C's `optopt`, and
    /// `line_bytes` prints it back as that one byte.
    #[test]
    fn a_raw_ff_option_letter_is_named_by_that_byte_not_the_replacement_char() {
        assert_eq!(
            parse_rev_args(&[&b"-\xff"[..]]),
            Err(GetoptReport::UnknownOption('\u{00ff}')),
            "字母该是裸字节 ff，不是折叠后的 U+FFFD"
        );
        // The ASCII shape is unaffected: the letter already is that one byte.
        assert_eq!(
            parse_rev_args(&[b"-Z".as_slice()]),
            Err(GetoptReport::UnknownOption('Z'))
        );
    }
}
