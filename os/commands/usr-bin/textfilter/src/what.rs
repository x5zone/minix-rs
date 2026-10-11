//! `what` — extract SCCS `@(#)` identification strings from file contents.
//!
//! Ground truth: `minix3/usr.bin/what/what.c`, whose `search()` (lines 97-120)
//! scans a byte stream for the four-byte marker `@(#)`. On a match it emits a
//! tab, then every following byte *up to* (but not including) the first
//! terminator among NUL, `"`, `>`, newline and backslash, then a newline; so an
//! embedded C string constant `static char id[] = "@(#)thing 1.2";` surfaces as
//! `\tthing 1.2`. A probe that fails mid-way (`@` then a non-`(`) re-tests that
//! offending byte as a fresh candidate rather than discarding it, so `@@(#)`
//! still finds the marker — reproduced by not advancing on a probe failure.
//!
//! The `what_matches` here returns only the per-match content bytes; the
//! `<filename>` header line, the tab/newline framing and the `-s`
//! stop-after-first rule stay with the caller, mirroring how C keeps `matches`
//! across files and decides the exit status.

use alloc::vec::Vec;

/// Is this byte a match-content terminator (the set `search()` stops on,
/// `what.c:112`)? None of them is `@`, so scanning resumes right after it.
fn is_terminator(c: u8) -> bool {
    matches!(c, b'\0' | b'"' | b'>' | b'\n' | b'\\')
}

/// Return the content of every `@(#)`-introduced run in `input`, in order.
///
/// A run starts just after a `@(#)`, and holds bytes up to the next terminator
/// (or end of input). Failing probes do not consume their offending byte as a
/// non-candidate — that byte is re-tested for starting a fresh `@(#)`, matching
/// C's `goto loop`.
pub fn what_matches(input: &[u8]) -> Vec<Vec<u8>> {
    let mut res = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i] != b'@' {
            i += 1;
            continue;
        }
        // Candidate `@`; read `(`, `#`, `)` in turn. A miss re-tests that byte
        // as a new candidate (do not advance).
        i += 1;
        if i >= input.len() {
            break;
        }
        if input[i] != b'(' {
            continue;
        }
        i += 1;
        if i >= input.len() {
            break;
        }
        if input[i] != b'#' {
            continue;
        }
        i += 1;
        if i >= input.len() {
            break;
        }
        if input[i] != b')' {
            continue;
        }
        // Matched the marker; collect content up to a terminator.
        i += 1;
        let mut run = Vec::new();
        while i < input.len() && !is_terminator(input[i]) {
            run.push(input[i]);
            i += 1;
        }
        res.push(run);
        // C's inner loop consumed the terminator into `c`; the outer loop's
        // next `getchar()` therefore starts *after* it.
        if i < input.len() {
            i += 1;
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(input: &[u8]) -> Vec<String> {
        what_matches(input)
            .iter()
            .map(|r| String::from_utf8(r.clone()).unwrap())
            .collect()
    }

    #[test]
    fn single_marker() {
        assert_eq!(contents(b"@(#)thing 1.2"), ["thing 1.2"]);
    }

    #[test]
    fn no_marker_is_empty() {
        assert!(what_matches(b"nothing here").is_empty());
    }

    #[test]
    fn terminators_end_a_run_without_consuming_it_as_content() {
        // `"` closes the C string; the run stops before it.
        assert_eq!(contents(b"@(#)id; /* not */"), ["id; /* not */"]);
        // A quote is a terminator: split into one run.
        assert_eq!(contents(b"@(#)abc\"def"), ["abc"]);
    }

    #[test]
    fn newline_splits_two_matches() {
        // A `@` is not a terminator, so without a newline the second marker
        // would be swallowed as content; `\n` separates them.
        assert_eq!(contents(b"@(#)foo\n@(#)bar\n"), ["foo", "bar"]);
    }

    #[test]
    fn overlapping_at_still_finds_marker() {
        // "@@(#)x": the first `@` fails its `(` probe and the second `@`
        // becomes the real candidate.
        assert_eq!(contents(b"@@(#)x"), ["x"]);
    }

    #[test]
    fn empty_run_allowed() {
        // "@(#)" followed straight by a terminator yields an empty content run.
        let runs = what_matches(b"@(#)\n");
        assert_eq!(runs.len(), 1);
        assert!(runs[0].is_empty());
    }

    #[test]
    fn eof_terminates_final_run() {
        assert_eq!(contents(b"junk @(#)tail-without-newline"), ["tail-without-newline"]);
    }
}
