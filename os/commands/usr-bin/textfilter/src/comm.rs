//! The three-way line merge behind `comm`.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 64-160: both operand files must already be sorted; the merge walks
//! them in lockstep — equal lines print in column three, the smaller
//! line prints in its own column and only that file advances — and at
//! either end of input the remaining lines drain into the surviving
//! column. Columns carry tab offsets: one tab for column two, two for
//! column three (comm.c:56, 105-110). `-1`/`-2`/`-3` suppress a column;
//! `-f` compares case-insensitively (`strcasecmp`, a **byte-wise** ctype table). Both files always
//! reach end of input (the loop reads on). [`parse_comm_args`] mirrors
//! C's `getopt` option scanning (`comm.c:80`): clustered letters, the `--`
//! terminator, a lone `-` operand, and an `UnknownOption` for any letter
//! outside `123f` — so an unrecognized flag is never mistaken for a file.

/// Column suppression flags (`-1`/`-2`/`-3`) and the `-f` fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommOptions {
    /// Suppress column one (lines only in the first file).
    pub suppress1: bool,
    /// Suppress column two (lines only in the second file).
    pub suppress2: bool,
    /// Suppress column three (common lines).
    pub suppress3: bool,
    /// `-f`: compare case-insensitively (C: `strcasecmp`).
    pub fold: bool,
}

/// One output line: which column it belongs to and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommLine<'a> {
    /// 1, 2, or 3 — the column the line prints in.
    pub column: u8,
    /// The line itself, **as bytes**: C's line leg `getnextln` fills the buffer byte
    /// by byte with `getc` (`comm.c:200-205`) and then does
    /// `printf("%s%s\n", col, line)` (`:140`), never decoding.
    pub text: &'a [u8],
}

use alloc::vec::Vec;

/// Merges two sorted line lists. Lines must be newline-stripped; both
/// lists are consumed to the end regardless of order violations (the C
/// assumes sorted input and simply advances the smaller side).
pub fn comm<'a>(a: &'a [&'a [u8]], b: &'a [&'a [u8]], options: &CommOptions) -> Vec<CommLine<'a>> {
    let mut out = Vec::new();
    let mut ia = 0usize;
    let mut ib = 0usize;
    while ia < a.len() || ib < b.len() {
        if ia >= a.len() {
            push(&mut out, 2, options, b[ib]);
            ib += 1;
            continue;
        }
        if ib >= b.len() {
            push(&mut out, 1, options, a[ia]);
            ia += 1;
            continue;
        }
        let ordering = line_cmp(a[ia], b[ib], options.fold);
        match ordering {
            core::cmp::Ordering::Equal => {
                push(&mut out, 3, options, a[ia]);
                ia += 1;
                ib += 1;
            }
            core::cmp::Ordering::Less => {
                push(&mut out, 1, options, a[ia]);
                ia += 1;
            }
            core::cmp::Ordering::Greater => {
                push(&mut out, 2, options, b[ib]);
                ib += 1;
            }
        }
    }
    out
}

fn push<'a>(out: &mut Vec<CommLine<'a>>, column: u8, options: &CommOptions, text: &'a [u8]) {
    let allowed = match column {
        1 => !options.suppress1,
        2 => !options.suppress2,
        _ => !options.suppress3,
    };
    if allowed {
        out.push(CommLine { column, text });
    }
}

/// The comparison: case-folded bytes with `-f`, plain bytes otherwise
/// (the C's `strcoll` is byte order in the C locale).
///
/// `-f` goes through `strcasecmp`: each side takes one byte and compares through the
/// `tolower` table looked up by `unsigned char`
/// (`minix3/common/lib/libc/string/strcasecmp.c:58-67`); in the C locale that table
/// only maps `A`–`Z` to `a`–`z` and returns `0x80`–`0xFF` unchanged. The character
/// domain's `to_lowercase` is the **Unicode** case table: `c3 89` (É) and `c3 a9` (é)
/// are the same character pair to it, while in C's table they differ in the second
/// byte.
fn line_cmp(a: &[u8], b: &[u8], fold: bool) -> core::cmp::Ordering {
    if fold {
        a.iter()
            .map(u8::to_ascii_lowercase)
            .cmp(b.iter().map(u8::to_ascii_lowercase))
    } else {
        a.cmp(b)
    }
}

/// Argument-splitting failure for `comm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommArgError {
    /// An option letter outside the `123f` set (C `getopt` returns `?`).
    UnknownOption(char),
    /// Not exactly the two file operands `comm` requires: C shifts
    /// `argc -= optind` (`comm.c:98`) then `if (argc != 2) usage()`
    /// (`comm.c:101`).
    NotTwoOperands,
}

/// Split `comm`'s arguments (the argv tail, without the program name) into
/// `(options, [file1, file2])`, faithful to C's `getopt(argc, argv, "123f")`
/// (`comm.c:80`): option letters cluster after one dash (`-12`, `-f1`), `--`
/// ends option scanning so everything after it is a file operand, a lone `-`
/// is itself a file operand (standard input), and any other option letter is
/// [`CommArgError::UnknownOption`] rather than a leaked file name. Fewer or more
/// than two operands is [`CommArgError::NotTwoOperands`]. The old front door
/// matched only whole `-1`/`-2`/`-3`/`-f` tokens and pushed everything else
/// (including an unknown `-z`) as an operand, so `comm -z f` opened a file
/// literally named `-z`; this parser routes that to `UnknownOption`.
///
/// The option ring does **not reorder** (`getopt.c:74-78`): the ring ends as soon as
/// a non-option token arrives, and any `-1`/`-f` after it is a file name, so
/// `comm a -1 b` has three operands rather than two operands plus a flag. This
/// command's optstring has no letter that takes a value, so libc's
/// `option requires an argument` line is unreachable (just as `rev`/`colrm` with an
/// empty optstring only have the outside-the-set letter shape: the reachable exits
/// are decided by the optstring, not by a "family rule").
///
/// Both the arguments and the returned file names are **bytes**: C hands `argv`
/// unchanged to `getopt` and to `file()`, never asking about encoding (`comm.c:80`
/// and `:177-187`). Bytes in, bytes out; the lossy fold happens only on the two legs
/// that genuinely need a `&str` (opening the file, and the `strerror` sentence of
/// `err(1, "%s", name)`).
pub fn parse_comm_args<'a>(
    args: &'a [&'a [u8]],
) -> Result<(CommOptions, [&'a [u8]; 2]), CommArgError> {
    let mut options = CommOptions::default();
    let mut index = 0;
    let operands: Vec<&'a [u8]> = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        if arg == b"--" {
            // `getopt.c:81-85`: `++optind` runs before -1 is returned, so `--` itself
            // is not an operand.
            break args[index + 1..].to_vec();
        }
        if arg.len() < 2 || arg[0] != b'-' {
            // `getopt.c:74-78`: a non-option token makes the ring return -1 and
            // **not advance** `optind`, so it and every token after it are operands
            // (in the reference build `comm a -1 b` measures three operands, hence
            // usage).
            break args[index..].to_vec();
        }
        // The ring walks **bytes**: C's `optopt` is a `char` (`optopt = *place++` at
        // `getopt.c:79` takes one byte at a time), so `-é` (`c3 a9`) reports `c3` and
        // not the whole character.
        for &byte in &arg[1..] {
            match byte {
                b'1' => options.suppress1 = true,
                b'2' => options.suppress2 = true,
                b'3' => options.suppress3 = true,
                b'f' => options.fold = true,
                other => return Err(CommArgError::UnknownOption(char::from(other))),
            }
        }
        index += 1;
    };
    if operands.len() != 2 {
        return Err(CommArgError::NotTwoOperands);
    }
    Ok((options, [operands[0], operands[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(result: &[CommLine<'_>]) -> Vec<(u8, Vec<u8>)> {
        result
            .iter()
            .map(|line| (line.column, line.text.to_vec()))
            .collect()
    }

    #[test]
    fn test_three_way_merge() {
        let a: [&[u8]; 3] = [b"apple", b"banana", b"cherry"];
        let b: [&[u8]; 3] = [b"banana", b"date", b"fig"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, b"apple".to_vec()),
                (3, b"banana".to_vec()),
                (1, b"cherry".to_vec()),
                (2, b"date".to_vec()),
                (2, b"fig".to_vec()),
            ]
        );
    }

    #[test]
    fn test_suppression_flags() {
        let a: [&[u8]; 2] = [b"common", b"only1"];
        let b: [&[u8]; 2] = [b"common", b"only2"];
        let options = CommOptions {
            suppress1: true,
            suppress3: true,
            ..CommOptions::default()
        };
        assert_eq!(texts(&comm(&a, &b, &options)), vec![(2, b"only2".to_vec())]);
    }

    #[test]
    fn test_fold_compares_case_insensitively() {
        let a: [&[u8]; 1] = [b"Apple"];
        let b: [&[u8]; 2] = [b"apple", b"zebra"];
        let options = CommOptions {
            fold: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![(3, b"Apple".to_vec()), (2, b"zebra".to_vec())]
        );
    }

    #[test]
    fn test_drains_surviving_file_at_either_end() {
        let a: [&[u8]; 1] = [b"early"];
        let b: [&[u8]; 3] = [b"mid1", b"mid2", b"mid3"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, b"early".to_vec()),
                (2, b"mid1".to_vec()),
                (2, b"mid2".to_vec()),
                (2, b"mid3".to_vec()),
            ]
        );
        let a: [&[u8]; 2] = [b"late1", b"late2"];
        let b: [&[u8]; 1] = [b"early"];
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (2, b"early".to_vec()),
                (1, b"late1".to_vec()),
                (1, b"late2".to_vec()),
            ]
        );
    }

    /// A line is a byte string: letting a non-UTF-8 byte make **the whole file's**
    /// lines disappear is the shell's failure shape (`from_utf8(input).unwrap_or("")`),
    /// while in C that line is still a record (`getnextln` at `comm.c:204-215` reads
    /// byte by byte with `getc` and never decodes).
    #[test]
    fn a_byte_that_is_not_utf8_is_still_a_comparable_line() {
        let left: [&[u8]; 2] = [b"a\xffb", b"common"];
        let right: [&[u8]; 1] = [b"a\xffb"];
        assert_eq!(
            texts(&comm(&left, &right, &CommOptions::default())),
            vec![(3, b"a\xffb".to_vec()), (1, b"common".to_vec())]
        );
    }

    /// The `-f` fold is a **per-byte ctype lookup**, not a Unicode lowercase:
    /// `strcasecmp` takes `tolower` by `unsigned char` on both sides
    /// (`minix3/common/lib/libc/string/strcasecmp.c:58-67`), so `c3 89` (É) and
    /// `c3 a9` (é) separate at the second byte — the reference build measures these
    /// two lines as **not** equal (each lands in its own column), whereas a Unicode
    /// fold would call them a common line in column three.
    #[test]
    fn the_fold_is_a_per_byte_ctype_lookup_not_a_unicode_lowercase() {
        let left: [&[u8]; 1] = [b"\xc3\x89"];
        let right: [&[u8]; 1] = [b"\xc3\xa9"];
        let fold = CommOptions {
            fold: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&left, &right, &fold)),
            vec![(1, b"\xc3\x89".to_vec()), (2, b"\xc3\xa9".to_vec())]
        );
        // The ASCII half must not be affected: `Apple` and `apple` still fold to the same line.
        let left: [&[u8]; 1] = [b"Apple"];
        let right: [&[u8]; 1] = [b"apple"];
        assert_eq!(
            texts(&comm(&left, &right, &fold)),
            vec![(3, b"Apple".to_vec())]
        );
    }

    #[test]
    fn test_parse_two_plain_operands() {
        assert_eq!(
            parse_comm_args(&[b"a", b"b"]),
            Ok((CommOptions::default(), [&b"a"[..], &b"b"[..]]))
        );
    }

    #[test]
    fn test_parse_separate_and_clustered_flags() {
        assert_eq!(
            parse_comm_args(&[b"-1", b"-2", b"a", b"b"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    suppress2: true,
                    ..CommOptions::default()
                },
                [&b"a"[..], &b"b"[..]]
            ))
        );
        // getopt clusters valueless letters: `-12f` == `-1 -2 -f`.
        assert_eq!(
            parse_comm_args(&[b"-12f", b"a", b"b"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    suppress2: true,
                    fold: true,
                    ..CommOptions::default()
                },
                [&b"a"[..], &b"b"[..]]
            ))
        );
    }

    #[test]
    fn test_parse_dash_and_double_dash_are_operands_or_terminator() {
        // A lone `-` is a file operand (standard input).
        assert_eq!(
            parse_comm_args(&[b"-", b"b"]),
            Ok((CommOptions::default(), [b"-", &b"b"[..]]))
        );
        // `--` ends option scanning; the dashes after it are file names.
        assert_eq!(
            parse_comm_args(&[b"--", b"-1", b"-2"]),
            Ok((CommOptions::default(), [&b"-1"[..], &b"-2"[..]]))
        );
    }

    /// The outside-the-set letter is the **first byte** after the dash, not the first
    /// character: `-\u{e9}` (`c3 a9`) in the `char` domain is U+00C3. The reference
    /// build measures a single `c3` at that line's option-letter position.
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_comm_args(&[b"-\xc3\xa9", b"a", b"b"]),
            Err(CommArgError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // Same for a cluster: the legal `1` is consumed first, then the next byte is reported.
        assert_eq!(
            parse_comm_args(&[b"-1\xc3\xa9", b"a", b"b"]),
            Err(CommArgError::UnknownOption('\u{00c3}')),
            "-1é"
        );
        // ASCII must not be affected by the byte-wise handling.
        assert_eq!(
            parse_comm_args(&[b"-Z", b"a", b"b"]),
            Err(CommArgError::UnknownOption('Z')),
            "-Z"
        );
    }

    /// An operand comes back as the **raw bytes** from argv: a `&str` signature could
    /// not express this shape — a bare `ff` would be folded into `ef bf bd` before it
    /// reached the ring, and the file name would no longer equal the argv bytes. C
    /// hands `argv[1]`/`argv[2]` unchanged to `file()` (`comm.c:177-187`), never
    /// decoding.
    #[test]
    fn an_operand_that_is_not_utf8_is_returned_as_the_argv_bytes() {
        let argv: &[&[u8]] = &[b"a\xffb", b"c\xd6"];
        let parsed = parse_comm_args(argv).unwrap();
        assert_eq!(
            parsed.1,
            [b"a\xffb".as_slice(), b"c\xd6".as_slice()],
            "档名就是 argv 里那串字节，一字节都不许多"
        );
    }

    #[test]
    fn test_parse_unknown_option_is_not_a_leaked_operand() {
        // The regression: `comm -z file` must reject `-z` (C `getopt` returns
        // `?` → usage), NOT open a file literally named `-z`.
        assert_eq!(
            parse_comm_args(&[b"-z", b"file"]),
            Err(CommArgError::UnknownOption('z'))
        );
        // Even with a full operand count the bad option still errors first.
        assert_eq!(
            parse_comm_args(&[b"-z", b"a", b"b"]),
            Err(CommArgError::UnknownOption('z'))
        );
    }

    #[test]
    fn test_parse_requires_exactly_two_operands() {
        assert_eq!(
            parse_comm_args(&[] as &[&[u8]]),
            Err(CommArgError::NotTwoOperands)
        );
        assert_eq!(parse_comm_args(&[b"a"]), Err(CommArgError::NotTwoOperands));
        assert_eq!(
            parse_comm_args(&[b"a", b"b", b"c"]),
            Err(CommArgError::NotTwoOperands)
        );
    }

    /// No reordering: the option ring ends as soon as the first non-option token
    /// arrives, so a following `-1` is a **file name** and all three tokens are
    /// operands → `NotTwoOperands` (the reference build's `comm a -1 b` prints one
    /// usage line and exits 1; the `-1` is neither a flag nor a bad option).
    #[test]
    fn test_parse_stops_at_the_first_operand_without_reordering() {
        assert_eq!(
            parse_comm_args(&[b"a", b"-1", b"b"]),
            Err(CommArgError::NotTwoOperands)
        );
        // The same with two operands first and a flag after: all three tokens become operands.
        assert_eq!(
            parse_comm_args(&[b"-", b"b", b"-f"]),
            Err(CommArgError::NotTwoOperands)
        );
        // Clusters inside the ring still take effect letter by letter, and everything after `--` is a file name.
        assert_eq!(
            parse_comm_args(&[b"-1f", b"--", b"-2", b"-3"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    fold: true,
                    ..CommOptions::default()
                },
                [&b"-2"[..], &b"-3"[..]]
            ))
        );
    }
}
