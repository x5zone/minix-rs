//! `from` — print the "From " mailbox header records.
//!
//! Ground truth: `minix3/usr.bin/from/from.c`. The program reads a mailbox
//! and prints every line that opens a message — a line beginning with the
//! literal `From ` that sits at a record start (the first line, or right
//! after a blank line). With `-s sender` it prints only those records whose
//! sender name appears in the header line.
//!
//! The two halves that carry real logic are pure and host-testable here;
//! resolving which mailbox file to read (from `-f`, an operand, `MAIL`,
//! `LOGNAME`/`USER`) and the byte I/O stay with the caller (the thin bin),
//! exactly as `from.c:94-116` splits them out of the scan loop.
//!
//! [`match_sender`] mirrors the C matcher (`from.c:131-153`): a BSD quirk in
//! which the search aborts the moment its scan pointer lands on a whitespace
//! byte, `first` is bound once and never re-taken, and a failed attempt
//! resumes one byte past the character that matched `first` (overlapping
//! candidates are not re-examined). The caller lowercases `sender` up front,
//! as `from.c:78-79` does; the bytes scanned from the record are lowercased
//! on the fly to compare.

/// C `match` (`from.c:131-153`): does the already-lowercased `sender` occur
/// in `line` (the record bytes after `From `) at a whole-word-ish position?
///
/// The search is a left-to-right scan. At each position it aborts with
/// `false` if the byte is whitespace (the C `isspace` guard returns 0, which
/// ends the *whole* match, not just this position); otherwise it lowercases
/// the byte and, if it equals `sender[0]`, tries to match the remaining
/// `sender` bytes right there. On a mismatch it resumes at the byte *after*
/// the one that matched `sender[0]` (the pointer already advanced past it),
/// so it does not backtrack into an overlapping candidate — faithful to the
/// C loop that binds `first` once in its initializer.
///
/// An empty `sender` is undefined behaviour in C (it reads one byte past the
/// terminator); this port treats it as "never matches". A single-byte
/// `sender` matches as soon as its byte is seen at a non-whitespace
/// position, matching C's inner loop returning 1 on the terminator.
pub fn match_sender(line: &[u8], sender: &[u8]) -> bool {
    let (&first, rest) = match sender.split_first() {
        Some(pair) => pair,
        None => return false, // empty sender: UB in C, defined as no match here
    };
    let mut i = 0;
    while i < line.len() {
        let ch = line[i];
        if ch.is_ascii_whitespace() {
            return false; // C `isspace(ch)` → return(0): the whole search stops
        }
        i += 1; // C `++line` happens before the comparison
        if ch.to_ascii_lowercase() != first {
            continue; // keep scanning for `first` (never re-taken)
        }
        // Candidate start matched `first`; compare the rest byte-for-byte.
        // C inner loop: `p = sender` (past `first`), `t = line` (past the
        // byte that matched `first`) — so `rest` lines up with `line[i..]`.
        if line.len() - i >= rest.len()
            && line[i..]
                .iter()
                .zip(rest)
                .all(|(&c, &s)| c.to_ascii_lowercase() == s)
        {
            return true;
        }
        // Mismatch (or `line` too short for `rest`): the outer loop resumes at
        // `i` (already past `first`), still comparing against the same `first`.
    }
    false // `line` exhausted: C reads past the NUL (UB); safe no-match here
}

/// Lazy scan of a mailbox byte stream, yielding the record lines to print.
///
/// Mirrors the C main loop (`from.c:118-127`): `newline` starts true, a bare
/// blank line (`"\n"`) resets it without being a record, and a line at a
/// record start whose first five bytes are `From ` (and, when `sender` is
/// given, whose remainder matches) is emitted. Each yielded slice keeps its
/// trailing newline, as C's `fgets`-buffered `printf("%s", buf)` does.
///
/// C splits input on the `fgets` buffer boundary (`BUFSIZ`); a single header
/// line longer than that is chopped into chunks here only by real newlines.
/// Realistic mailbox headers are far shorter than `BUFSIZ`, so this differs
/// only for pathological over-long records (declared unwired).
pub struct FromRecords<'a> {
    rest: &'a [u8],
    newline: bool,
    sender: Option<&'a [u8]>,
}

impl<'a> FromRecords<'a> {
    /// New scan over `content`, filtering by `sender` when present (the
    /// caller passes the lowercased form, as `from.c` lowercases `sender`
    /// once before the loop).
    pub fn new(content: &'a [u8], sender: Option<&'a [u8]>) -> Self {
        FromRecords {
            rest: content,
            newline: true,
            sender,
        }
    }
}

impl<'a> Iterator for FromRecords<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        loop {
            if self.rest.is_empty() {
                return None;
            }
            // A line runs up to and including its trailing newline; the final
            // line may have none.
            let end = self
                .rest
                .iter()
                .position(|&c| c == b'\n')
                .map_or(self.rest.len(), |p| p + 1);
            let line = &self.rest[..end];
            self.rest = &self.rest[end..];

            // A blank line marks the next line as a record start, but is
            // never itself a record (`from.c:119-122`).
            if line[0] == b'\n' {
                self.newline = true;
                continue;
            }

            let matched = self.newline
                && line.starts_with(b"From ")
                && self.sender.is_none_or(|s| match_sender(&line[5..], s));
            self.newline = false;
            if matched {
                return Some(line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mailbox: three records. Only the "From " lines that sit at a record
    /// start (right after a blank line or at the very top) count; the stray
    /// `From ` mid-body and the lowercase `from ` must not match.
    const MAIL: &[u8] = b"From alice Tue Jan 1\nbody one\n\nFrom bob Wed Jan 2\nbody two\nFrom not-a-record-start\n\nfrom carol Thu\n";

    #[test]
    fn no_sender_prints_record_start_from_lines() {
        let recs: Vec<&[u8]> = FromRecords::new(MAIL, None).collect();
        assert_eq!(
            recs,
            vec![&b"From alice Tue Jan 1\n"[..], &b"From bob Wed Jan 2\n"[..],],
            "the mid-body `From ...` line is not at a record start; the \
             lowercase `from carol` fails the literal `From `"
        );
    }

    #[test]
    fn sender_selects_matching_records() {
        let recs: Vec<&[u8]> = FromRecords::new(MAIL, Some(b"bob")).collect();
        assert_eq!(recs, vec![b"From bob Wed Jan 2\n"]);
    }

    #[test]
    fn sender_is_case_insensitive() {
        // `from.c` lowercases the sender and lowercases the line byte before
        // comparing, so `BOB` selects the same record as `bob`.
        let recs: Vec<&[u8]> = FromRecords::new(MAIL, Some(b"bob")).collect();
        assert_eq!(recs.len(), 1);
        let recs_upper: Vec<&[u8]> = FromRecords::new(b"From BOB Mon\n", Some(b"bob")).collect();
        assert_eq!(recs_upper, vec![b"From BOB Mon\n"]);
    }

    #[test]
    fn record_without_trailing_newline_is_still_emitted() {
        let recs: Vec<&[u8]> = FromRecords::new(b"From dave Fri", None).collect();
        assert_eq!(recs, vec![b"From dave Fri"]);
    }

    #[test]
    fn first_line_counts_as_record_start() {
        // `newline` is initialized to 1, so a leading record is emitted.
        let recs: Vec<&[u8]> = FromRecords::new(b"From top Mon\nx\n", None).collect();
        assert_eq!(recs, vec![b"From top Mon\n"]);
    }

    #[test]
    fn match_sender_requires_non_space_start() {
        // A double space after `From ` puts a whitespace byte at the scan
        // start → the C matcher aborts with 0 (never a match).
        assert!(!match_sender(b" bob", b"bob"));
        assert!(match_sender(b"bob", b"bob"));
    }

    #[test]
    fn match_sender_requires_leading_word() {
        // The scan aborts at the first whitespace byte, so the sender must
        // begin at position 0 of the remainder (the first token after
        // `From `); a sender sitting after another word is never found.
        assert!(match_sender(b"bob@host", b"bob"));
        assert!(!match_sender(b"uid bob", b"bob")); // aborts at the space
        assert!(!match_sender(b"uid=1000 alice", b"alice"));
    }

    #[test]
    fn match_sender_resumes_after_rest_mismatch() {
        // `first` matches but the remaining sender bytes do not (the C inner
        // loop `break`s); the outer loop resumes one byte past the matched
        // `first` and keeps searching. `bx` has no `bb` (a `b` followed by
        // another `b`), while `acbb` does (the `b` pair at the tail).
        assert!(!match_sender(b"bx", b"bb"));
        assert!(match_sender(b"acbb", b"bb"));
    }

    #[test]
    fn match_sender_single_char_matches_on_first_seen() {
        assert!(match_sender(b"xyz", b"x"));
        assert!(match_sender(b"abcx", b"x"));
        assert!(!match_sender(b"abc", b"x"));
    }

    #[test]
    fn match_sender_empty_never_matches() {
        assert!(!match_sender(b"anything", b""));
    }

    #[test]
    fn match_sender_longer_than_line_no_match() {
        assert!(!match_sender(b"ab", b"abcdef"));
    }

    #[test]
    fn match_sender_abort_at_space_ends_whole_search() {
        // Once the scan reaches a space (before finding `first`), it stops —
        // the sender cannot be located past a word boundary.
        assert!(!match_sender(b"no match here", b"match"));
    }

    #[test]
    fn empty_mailbox_yields_nothing() {
        let recs: Vec<&[u8]> = FromRecords::new(b"", None).collect();
        assert!(recs.is_empty());
    }
}
