//! The three-way line merge behind `comm`.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 64-160: both operand files must already be sorted; the merge walks
//! them in lockstep — equal lines print in column three, the smaller
//! line prints in its own column and only that file advances — and at
//! either end of input the remaining lines drain into the surviving
//! column. Columns carry tab offsets: one tab for column two, two for
//! column three (comm.c:56, 105-110). `-1`/`-2`/`-3` suppress a column;
//! `-f` compares case-insensitively (`strcasecmp`). Both files always
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
    /// The line itself.
    pub text: &'a str,
}

use alloc::vec::Vec;

/// Merges two sorted line lists. Lines must be newline-stripped; both
/// lists are consumed to the end regardless of order violations (the C
/// assumes sorted input and simply advances the smaller side).
pub fn comm<'a>(
    a: &'a [&'a str],
    b: &'a [&'a str],
    options: &CommOptions,
) -> Vec<CommLine<'a>> {
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

fn push<'a>(out: &mut Vec<CommLine<'a>>, column: u8, options: &CommOptions, text: &'a str) {
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
fn line_cmp(a: &str, b: &str, fold: bool) -> core::cmp::Ordering {
    if fold {
        a.to_lowercase().cmp(&b.to_lowercase())
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
pub fn parse_comm_args<'a>(
    args: &'a [&'a str],
) -> Result<(CommOptions, [&'a str; 2]), CommArgError> {
    let mut options = CommOptions::default();
    let mut operands: Vec<&'a str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        if arg.len() > 1 && arg.starts_with('-') {
            for ch in arg[1..].chars() {
                match ch {
                    '1' => options.suppress1 = true,
                    '2' => options.suppress2 = true,
                    '3' => options.suppress3 = true,
                    'f' => options.fold = true,
                    other => return Err(CommArgError::UnknownOption(other)),
                }
            }
        } else {
            // A file operand; a lone `-` (len 1) and any non-dash token both
            // name a file (standard input for `-`).
            operands.push(arg);
        }
        index += 1;
    }
    if operands.len() != 2 {
        return Err(CommArgError::NotTwoOperands);
    }
    Ok((options, [operands[0], operands[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(result: &[CommLine<'_>]) -> Vec<(u8, String)> {
        result
            .iter()
            .map(|line| (line.column, line.text.to_string()))
            .collect()
    }

    #[test]
    fn test_three_way_merge() {
        let a = ["apple", "banana", "cherry"];
        let b = ["banana", "date", "fig"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, "apple".to_string()),
                (3, "banana".to_string()),
                (1, "cherry".to_string()),
                (2, "date".to_string()),
                (2, "fig".to_string()),
            ]
        );
    }

    #[test]
    fn test_suppression_flags() {
        let a = ["common", "only1"];
        let b = ["common", "only2"];
        let options = CommOptions {
            suppress1: true,
            suppress3: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![(2, "only2".to_string())]
        );
    }

    #[test]
    fn test_fold_compares_case_insensitively() {
        let a = ["Apple"];
        let b = ["apple", "zebra"];
        let options = CommOptions {
            fold: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (3, "Apple".to_string()),
                (2, "zebra".to_string()),
            ]
        );
    }

    #[test]
    fn test_drains_surviving_file_at_either_end() {
        let a = ["early"];
        let b = ["mid1", "mid2", "mid3"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, "early".to_string()),
                (2, "mid1".to_string()),
                (2, "mid2".to_string()),
                (2, "mid3".to_string()),
            ]
        );
        let a = ["late1", "late2"];
        let b = ["early"];
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (2, "early".to_string()),
                (1, "late1".to_string()),
                (1, "late2".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_two_plain_operands() {
        assert_eq!(
            parse_comm_args(&["a", "b"]),
            Ok((CommOptions::default(), ["a", "b"]))
        );
    }

    #[test]
    fn test_parse_separate_and_clustered_flags() {
        assert_eq!(
            parse_comm_args(&["-1", "-2", "a", "b"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    suppress2: true,
                    ..CommOptions::default()
                },
                ["a", "b"]
            ))
        );
        // getopt clusters valueless letters: `-12f` == `-1 -2 -f`.
        assert_eq!(
            parse_comm_args(&["-12f", "a", "b"]),
            Ok((
                CommOptions {
                    suppress1: true,
                    suppress2: true,
                    fold: true,
                    ..CommOptions::default()
                },
                ["a", "b"]
            ))
        );
    }

    #[test]
    fn test_parse_dash_and_double_dash_are_operands_or_terminator() {
        // A lone `-` is a file operand (standard input).
        assert_eq!(
            parse_comm_args(&["-", "b"]),
            Ok((CommOptions::default(), ["-", "b"]))
        );
        // `--` ends option scanning; the dashes after it are file names.
        assert_eq!(
            parse_comm_args(&["--", "-1", "-2"]),
            Ok((CommOptions::default(), ["-1", "-2"]))
        );
    }

    #[test]
    fn test_parse_unknown_option_is_not_a_leaked_operand() {
        // The regression: `comm -z file` must reject `-z` (C `getopt` returns
        // `?` → usage), NOT open a file literally named `-z`.
        assert_eq!(
            parse_comm_args(&["-z", "file"]),
            Err(CommArgError::UnknownOption('z'))
        );
        // Even with a full operand count the bad option still errors first.
        assert_eq!(
            parse_comm_args(&["-z", "a", "b"]),
            Err(CommArgError::UnknownOption('z'))
        );
    }

    #[test]
    fn test_parse_requires_exactly_two_operands() {
        assert_eq!(parse_comm_args(&[]), Err(CommArgError::NotTwoOperands));
        assert_eq!(parse_comm_args(&["a"]), Err(CommArgError::NotTwoOperands));
        assert_eq!(
            parse_comm_args(&["a", "b", "c"]),
            Err(CommArgError::NotTwoOperands)
        );
    }
}
