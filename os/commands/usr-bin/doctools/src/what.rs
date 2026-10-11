//! `what` — extract SCCS `@(#)` identification strings, the pure layer.
//!
//! Ground truth: `minix3/usr.bin/what/what.c` (NetBSD 1.11, BSD what). The
//! program reads files and prints each `@(#)description` identifier found;
//! this module owns the deciding half — the option scan and the byte scanner
//! that turns file contents into the list of identifier strings — mirroring
//! `what.c`'s `main` `getopt(argc, argv, "s")` (lines 68-77) and `search()`
//! (lines 97-120). Opening the operands and printing live in the thin bin
//! `src/bin/what.rs`.
//!
//! [`extract_ids`] reproduces `search()` exactly, including its `goto loop`
//! backtracking: the character that fails the `(`/`#`/`)` test is re-tested as
//! a new `@` candidate rather than discarded, so `@@(#)…` and `@(x@(#)…` match
//! the way the C loop does. After the `@(#)` header, the identifier runs up to
//! (but not including) the first terminator in `{'\0', '"', '>', '\n', '\\'}`
//! or end of input — the terminator itself is consumed, exactly as C's inner
//! `while` consumes it without printing it. `-s` stops after the first
//! identifier in the input.

use alloc::vec::Vec;

/// The resolved option set of `what`, from `what.c:68-77`. `-s` prints only
/// the first identifier found per file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WhatFlags {
    /// `-s`: stop searching a file after the first identifier.
    pub stop_first: bool,
}

impl WhatFlags {
    /// Parse the option string `"s"`. Returns the flags and the operand slice
    /// (the file names), or `None` on an unknown option so the bin prints usage
    /// and exits 1. Classic BSD getopt: no permutation — the first non-option
    /// ends the scan, and `--` ends it too.
    pub fn parse<'a>(args: &[&'a str]) -> Option<(WhatFlags, Vec<&'a str>)> {
        let mut f = WhatFlags::default();
        let mut i = 0;
        let mut done = false;
        while i < args.len() && !done {
            let a = args[i];
            if a == "--" {
                done = true;
                i += 1;
                continue;
            }
            if a.len() >= 2 && a.starts_with('-') {
                for &byte in a.as_bytes()[1..].iter() {
                    match byte {
                        b's' => f.stop_first = true,
                        _ => return None,
                    }
                }
                i += 1;
                continue;
            }
            break; // first non-option ends the option scan
        }
        Some((f, args[i..].to_vec()))
    }
}

/// `what.c:112-113` terminator set: an identifier ends at `'\0'`, `'"'`,
/// `'>'`, `'\n'`, or `'\'` (all consumed but not printed), or at end of input.
fn is_terminator(b: u8) -> bool {
    matches!(b, 0 | b'"' | b'>' | b'\n' | b'\\')
}

/// C `getchar()`: return the byte at `*pos` and advance, or `None` at end of
/// input (C's `EOF`). Kept as a free function so the scanner can inspect `pos`
/// between reads without a lingering closure borrow.
fn getchar(input: &[u8], pos: &mut usize) -> Option<u8> {
    if *pos < input.len() {
        let b = input[*pos];
        *pos += 1;
        Some(b)
    } else {
        None
    }
}

/// Scan `input` for `@(#)identifier` strings, returning each identifier's
/// bytes (borrowed from `input`). `stop_first` (`-s`) halts after the first
/// identifier. Reproduces `search()` (`what.c:97-120`) byte-for-byte,
/// including the `goto loop` re-test so an overlapping `@` still matches.
pub fn extract_ids(input: &[u8], stop_first: bool) -> Vec<&[u8]> {
    let mut out: Vec<&[u8]> = Vec::new();
    let mut pos = 0usize;

    // while ((c = getchar()) != EOF)
    let mut c = getchar(input, &mut pos);
    while c.is_some() {
        // loop: — `c` is the current candidate character.
        loop {
            if c != Some(b'@') {
                // continue → outer while: fetch the next character.
                c = getchar(input, &mut pos);
                break;
            }
            c = getchar(input, &mut pos);
            if c != Some(b'(') {
                continue; // goto loop: re-test this character as `@`.
            }
            c = getchar(input, &mut pos);
            if c != Some(b'#') {
                continue;
            }
            c = getchar(input, &mut pos);
            if c != Some(b')') {
                continue;
            }
            // Header `@(#)` matched; the identifier runs to the terminator.
            let start = pos;
            let mut by_terminator = false;
            loop {
                match getchar(input, &mut pos) {
                    None => break,
                    Some(b) if is_terminator(b) => {
                        by_terminator = true;
                        break;
                    }
                    Some(_) => {}
                }
            }
            // A consumed terminator sits one byte before `pos`; EOF does not.
            let end = if by_terminator { pos - 1 } else { pos };
            out.push(&input[start..end]);
            if stop_first {
                return out;
            }
            c = getchar(input, &mut pos);
            break; // resume the outer while with the next character.
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: Vec<&[u8]>) -> Vec<String> {
        v.iter()
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect()
    }

    #[test]
    fn flags_stop_first() {
        let (f, rest) = WhatFlags::parse(&["-s", "a.out"]).unwrap();
        assert!(f.stop_first);
        assert_eq!(rest, ["a.out"]);
    }

    #[test]
    fn flags_unknown_is_error_and_double_dash() {
        assert!(WhatFlags::parse(&["-z"]).is_none());
        let (f, rest) = WhatFlags::parse(&["--", "-s"]).unwrap();
        assert!(!f.stop_first, "-s after -- is an operand");
        assert_eq!(rest, ["-s"]);
    }

    #[test]
    fn empty_and_no_id() {
        assert!(extract_ids(b"", false).is_empty());
        assert!(extract_ids(b"nothing here", false).is_empty());
    }

    #[test]
    fn single_id_newline_terminated() {
        let r = extract_ids(b"@(#)Version 1.0\nrest", false);
        assert_eq!(strs(r), ["Version 1.0"]);
    }

    #[test]
    fn multiple_ids() {
        let r = extract_ids(b"@(#)a\"@(#)b>", false);
        assert_eq!(strs(r), ["a", "b"]);
    }

    #[test]
    fn double_at_backtracking() {
        // search(): first '@' consumed looking for '(' finds second '@',
        // goto loop re-tests it, so @@(#)... matches.
        let r = extract_ids(b"@@(#)x\n", false);
        assert_eq!(strs(r), ["x"]);
    }

    #[test]
    fn partial_header_then_match() {
        // "@(x" fails the '#' test; the loop resyncs and matches "@(#)y".
        let r = extract_ids(b"@(x@(#)y\n", false);
        assert_eq!(strs(r), ["y"]);
    }

    #[test]
    fn incomplete_header_eof() {
        assert!(extract_ids(b"@(#", false).is_empty());
        assert!(extract_ids(b"@", false).is_empty());
    }

    #[test]
    fn id_at_eof_without_terminator() {
        let r = extract_ids(b"@(#)abc", false);
        assert_eq!(strs(r), ["abc"]);
    }

    #[test]
    fn empty_id_immediate_terminator() {
        // "@" then "(" "#" ")" then a NUL terminator -> zero-length identifier.
        let r = extract_ids(b"@(#)\0more", false);
        assert_eq!(strs(r), [""]);
    }

    #[test]
    fn backslash_terminates() {
        let r = extract_ids(b"@(#)a\\b", false);
        assert_eq!(strs(r), ["a"]); // "b" scanned next, no '@'
    }

    #[test]
    fn stop_first_returns_only_first() {
        let all = extract_ids(b"@(#)a\n@(#)b\n", false);
        assert_eq!(strs(all), ["a", "b"]);
        let one = extract_ids(b"@(#)a\n@(#)b\n", true);
        assert_eq!(strs(one), ["a"]);
    }
}
