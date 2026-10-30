//! Byte comparison for `cmp`.
//!
//! Ground truth: `minix3/usr.bin/cmp/` (NetBSD): the compare loop
//! (`regular.c:70-118`) walks both byte streams from the skip offsets
//! with byte and line counters starting at one; the first difference
//! prints "%s %s differ: char %lld, line %lld" and exits 1 (`diffmsg`,
//! misc.c:78-85) unless `-l` lists every difference as
//! "%6lld %3o %3o" (regular.c:98) with only the status leaving; `-s`
//! prints nothing at all. A shorter stream is reported by `eofmsg`
//! (misc.c:59-73): "EOF on %s", or with `-l` "EOF on %s: char %lld,
//! line %lld" — and the EOF names the *shorter* file
//! (`regular.c:111`: `len1 > len2 ? file2 : file1`). Line numbers count
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
    //[SD19-gate] String formatting can only fail on memory allocation; the
    // corresponding printf in C regular.c:98 does not branch on it either
    #[allow(clippy::let_underscore_must_use)]
    let _ = write!(out, "{:6} {:3o} {:3o}\n", byte, left, right);
}

/// The first-difference line `misc.c` `diffmsg` sends to standard output:
/// `"%s %s differ: char %lld, line %lld\n"` (with a trailing newline).
pub fn report_first_diff(file1: &[u8], file2: &[u8], byte: u64, line: u64) -> Vec<u8> {
    let tail = alloc::format!(" differ: char {byte}, line {line}\n");
    let mut line = Vec::with_capacity(file1.len() + file2.len() + tail.len() + 1);
    line.extend_from_slice(file1);
    line.push(b' ');
    line.extend_from_slice(file2);
    line.extend_from_slice(tail.as_bytes());
    line
}

/// The early-EOF message `misc.c` `eofmsg` sends to standard error (before the
/// `cmp: ` program prefix the caller adds): without `-l` just `"EOF on <file>"`;
/// with `-l` the byte and — when a line count is meaningful — the line too.
pub fn report_eof(file: &[u8], byte: u64, line: u64, list_all: bool) -> Vec<u8> {
    let tail = if !list_all {
        String::new()
    } else if line > 0 {
        alloc::format!(": char {byte}, line {line}")
    } else {
        alloc::format!(": char {byte}")
    };
    let mut out = Vec::with_capacity("EOF on ".len() + file.len() + tail.len());
    out.extend_from_slice(b"EOF on ");
    out.extend_from_slice(file);
    out.extend_from_slice(tail.as_bytes());
    out
}

/// What `cmp`'s option loop reports: the two flags plus **every token left
/// after the loop**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmpFlags<'a> {
    /// `-l`: list every difference byte by byte.
    pub list_all: bool,
    /// `-s`: print nothing and put the verdict in the exit status only.
    pub silent: bool,
    /// The token sequence the option loop hands back:
    /// `file1 file2 [skip1 [skip2]]`; the caller decides the count.
    pub operands: Vec<&'a [u8]>,
}

/// The failure shape of the option loop. `cmp.c:74`'s optstring is `"ls"` —
/// **neither letter takes a value**, so libc's `option requires an argument`
/// message is unreachable on this branch; only unknown option letters can come
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpArgError {
    /// `getopt.c:95-104`: unknown option letter (the caller renders libc's
    /// line through [`crate::GetoptReport`]).
    UnknownOption(char),
}

/// Split the tail of `cmp`'s argv into [`CmpFlags`] the way
/// `getopt(argc, argv, "ls")` (`cmp.c:74`) does.
///
/// Cluster letters: `-ls` and `-sl` both raise both flags (C then reports
/// them with `if (lflag && sflag) errx(...)` as **one line**, not via usage,
/// `cmp.c:87-88`), while `-lq` falls into the unknown-letter branch on the
/// second letter. The old front end compared whole tokens only
/// (`arg == "-l"`), so `-ls` fell into "unknown option" and printed usage —
/// two exits with different line counts and different sentences. Repeating the
/// same flag (`-l -l`) is legal (C just assigns 1 again).
///
/// **No permutation** (`getopt.c:74-78`): the first non-option token returns
/// -1 without advancing `optind`, and it plus everything after it comes back
/// as operands; so `cmp file1 -l file2` is **three operands** and that `-l` is
/// the second file name (measured against the reference: `cmp: -l: No such
/// file or directory`, exit 2). `--` ends the loop and is not itself an
/// operand; a bare `-` has length one and is an operand (`cmp.c:95-113` treats
/// it as standard input).
///
/// The operand count test (`< 2 || > 4`) and the `-l`/`-s` mutual exclusion
/// stay with the caller: C's order is "option loop → exclusion → count → open
/// files", and each sentence belongs to its own exit; changing the order
/// would mix the one-line `errx` branch with the two-line usage branch.
pub fn parse_cmp_flags<'a>(args: &'a [&'a [u8]]) -> Result<CmpFlags<'a>, CmpArgError> {
    let mut flags = CmpFlags {
        list_all: false,
        silent: false,
        operands: Vec::new(),
    };
    let mut index = 0usize;
    flags.operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        if arg == b"--" {
            // `getopt.c:81-85`: `++optind` happens before returning -1, so
            // `--` itself is not an operand.
            break args[index + 1..].to_vec();
        }
        if arg.len() < 2 || !arg.starts_with(b"-") {
            // This includes a bare `-`: `getopt.c:87-92` ends the loop
            // **without advancing** when the optstring has no `-`.
            break args[index..].to_vec();
        }
        // The loop works on **bytes**: C's `optopt` is a single `char`
        // (`getopt.c:79`'s `optopt = *place++` consumes one byte at a time),
        // so `-é` (`c3 a9`) reports `c3`, not the whole character. The
        // earlier `chars()` pass handed back both bytes of one character as a
        // single letter, and rendering that report as UTF-8 produced two
        // bytes.
        for &byte in &arg[1..] {
            match byte {
                b'l' => flags.list_all = true,
                b's' => flags.silent = true,
                other => return Err(CmpArgError::UnknownOption(char::from(other))),
            }
        }
        index += 1;
    };
    Ok(flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unknown option letter comes back as the **first byte** after the
    /// dash, not the first character: `-\u{e9}` (`c3 a9`) lands in the `char`
    /// domain as U+00C3. Measured against the reference binary, the letter
    /// position in that line is the single byte `c3`.
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_cmp_flags(&[b"-\xc3\xa9", b"a", b"b"]),
            Err(CmpArgError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // Same with clusters: the legal `l` is consumed first, and the first
        // byte after it is what gets reported.
        assert_eq!(
            parse_cmp_flags(&[b"-l\xc3\xa9", b"a", b"b"]),
            Err(CmpArgError::UnknownOption('\u{00c3}')),
            "-l\u{e9}"
        );
        // The ASCII form must not be affected by this change.
        assert_eq!(
            parse_cmp_flags(&[b"-q", b"a", b"b"]),
            Err(CmpArgError::UnknownOption('q')),
            "-q"
        );
    }

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
    fn test_report_first_diff_matches_netbsd_format() {
        assert_eq!(
            report_first_diff(b"a", b"b", 5, 2),
            b"a b differ: char 5, line 2\n"
        );
    }

    #[test]
    fn test_report_eof_branches() {
        // Without -l: just the file. With -l: char, and line when nonzero.
        assert_eq!(report_eof(b"b", 3, 0, false), b"EOF on b");
        assert_eq!(report_eof(b"b", 3, 0, true), b"EOF on b: char 3");
        assert_eq!(report_eof(b"b", 9, 2, true), b"EOF on b: char 9, line 2");
    }

    #[test]
    fn test_list_all_collects_every_difference() {
        let outcome = compare_bytes(b"aXcYe\n", b"aBcDe\n", true);
        assert!(!outcome.equal);
        assert_eq!(outcome.byte_list, vec![(2, b'X', b'B'), (4, b'Y', b'D')]);
    }

    #[test]
    fn test_eof_names_the_shorter_stream() {
        // regular.c:111 names the shorter stream: the first stream here.
        let outcome = compare_bytes(b"ab", b"abc", false);
        assert_eq!(outcome.eof_on, Some((1, 3, 1)));
        // Second stream shorter: the EOF names stream two
        // (regular.c:111's ternary picks file2 when len1 > len2).
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

    /// Clusters are read letter by letter: `-ls` and `-sl` raise both flags
    /// (the caller then takes the `errx` line), `-lq` reports an unknown
    /// letter on the second one, and repeating the same flag (`-l -l`) is
    /// legal.
    #[test]
    fn parse_flags_cluster_letter_by_letter() {
        let both = parse_cmp_flags(&[b"-ls", b"a", b"b"]).unwrap();
        assert!(both.list_all && both.silent);
        assert_eq!(both.operands, [b"a".as_slice(), b"b".as_slice()]);
        assert_eq!(
            parse_cmp_flags(&[b"-sl", b"a", b"b"]).unwrap(),
            parse_cmp_flags(&[b"-ls", b"a", b"b"]).unwrap()
        );
        assert_eq!(
            parse_cmp_flags(&[b"-lq", b"a", b"b"]).err(),
            Some(CmpArgError::UnknownOption('q'))
        );
        assert_eq!(
            parse_cmp_flags(&[b"--x"]).err(),
            Some(CmpArgError::UnknownOption('-'))
        );
        let twice = parse_cmp_flags(&[b"-l", b"-l", b"a", b"b"]).unwrap();
        assert!(twice.list_all && !twice.silent);
    }

    /// The loop does not permute, `--` ends it without being an operand, and
    /// a bare `-` is an operand.
    #[test]
    fn parse_ring_stops_at_the_first_operand() {
        // Reference behavior: `cmp f1 -l f2` has three operands, with `-l` as
        // the second file name.
        assert_eq!(
            parse_cmp_flags(&[b"f1", b"-l", b"f2"]).unwrap().operands,
            [b"f1".as_slice(), b"-l".as_slice(), b"f2".as_slice()]
        );
        assert_eq!(
            parse_cmp_flags(&[b"-l", b"f1", b"-l", b"f2"])
                .unwrap()
                .operands,
            [b"f1".as_slice(), b"-l".as_slice(), b"f2".as_slice()]
        );
        assert!(!parse_cmp_flags(&[b"--", b"-l", b"-s"]).unwrap().list_all);
        assert_eq!(
            parse_cmp_flags(&[b"--", b"-l", b"-s"]).unwrap().operands,
            [b"-l".as_slice(), b"-s".as_slice()]
        );
        assert_eq!(
            parse_cmp_flags(&[b"-", b"-"]).unwrap().operands,
            [b"-".as_slice(), b"-".as_slice()]
        );
    }
}
