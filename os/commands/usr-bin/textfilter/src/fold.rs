//! Line folding for `fold`.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The display
//! column advances by `new_column_position` (fold.c:209-232): backspace
//! pulls one back, carriage return resets to zero, tab rounds up to the
//! next multiple of eight, and every other character adds one (byte
//! counts with `-b`); when the column passes the width the buffered
//! segment flushes, and only `-s` (split-words) first looks back for the
//! last space in the buffer and drops that space from the carried tail
//! (fold.c:152-176) — without it the break is a hard one wherever the
//! column lands, so a space stays at the end of the line it filled.
//! A newline flushes the buffer (fold.c:140-149). The
//! default width is 80 (`DEFLINEWIDTH`, fold.c:56); `-w` sets it.
//! Multibyte characters each occupy one column in this slice (the C
//! counts wide characters the same way without `-b`).

use alloc::string::String;
use alloc::vec::Vec;

/// Fold options: the width and the two modifier flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldOptions {
    /// `-w` width (the break happens when the column passes it).
    pub width: usize,
    /// `-b`: every byte advances the column by one (no `\b`/`\r`/`\t`
    /// column rules).
    pub count_bytes: bool,
    /// `-s`: break without looking for the last space.
    pub split_words: bool,
}

impl FoldOptions {
    /// Default width 80, no modifiers.
    pub fn new(width: usize) -> Self {
        FoldOptions {
            width,
            ..FoldOptions::default()
        }
    }
}

/// The display column after one character (C: `new_column_position`,
/// fold.c:209-232).
fn advance_column(column: usize, character: char, count_bytes: bool) -> usize {
    if count_bytes {
        return column + 1;
    }
    match character {
        '\u{8}' => column.saturating_sub(1),
        '\r' => 0,
        '\t' => (column + 8) & !7,
        _ => column + 1,
    }
}

/// Folds one line into output lines no wider than `options.width`.
///
/// Faithful to `fold`'s per-character walk (fold.c:119-200): the buffer
/// accumulates characters while the column stays within the width; when
/// it passes, the buffer flushes — breaking after the last space unless
/// [`FoldOptions::split_words`] — and the carried tail replays for its
/// new column before the current character joins it. A newline flushes
/// the buffer and resets.
pub fn fold_line(line: &str, options: &FoldOptions) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buffer: Vec<char> = Vec::new();
    let mut column = 0usize;
    for character in line.chars() {
        if character == '\n' {
            out.push(buffer.iter().collect());
            buffer.clear();
            column = 0;
            continue;
        }
        column = advance_column(column, character, options.count_bytes);
        if column > options.width {
            // `fold.c:152-176`: looking back for the last space in the buffer is
            // the `-s`-only leg; without `-s`, the whole buffer is flushed as
            // soon as the column passes the width, and the space stays on the
            // line it landed in. The arm that finds a space drops that space
            // itself (the `wmemmove` happens only after `last_space++`), and the
            // continuation line's column is recomputed from the remaining
            // characters.
            let last_space = if options.split_words {
                buffer.iter().rposition(|&ch| ch == ' ')
            } else {
                None
            };
            match last_space {
                Some(space) => {
                    out.push(buffer[..space].iter().collect());
                    buffer.drain(..=space);
                    column = 0;
                    for &carried in &buffer {
                        column = advance_column(column, carried, options.count_bytes);
                    }
                }
                None => {
                    out.push(buffer.iter().collect());
                    buffer.clear();
                    column = 0;
                }
            }
            column = advance_column(column, character, options.count_bytes);
        }
        buffer.push(character);
    }
    if !buffer.is_empty() {
        out.push(buffer.iter().collect());
    }
    out
}

/// Folds the whole input (line by line; the C folds the stream, whose
/// newlines flush the same buffer).
pub fn fold_input(input: &str, options: &FoldOptions) -> Vec<String> {
    let mut out = Vec::new();
    for line in input.split('\n') {
        out.extend(fold_line(line, options));
    }
    if out.last().is_some_and(|last| last.is_empty()) {
        // An input ending in a newline flushes an empty buffer: the
        // trailing empty fold line is the newline itself, not padding.
        out.pop();
    }
    out
}

/// `fold`'s parsed argv tail: the fold options and the file operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldArgs<'a> {
    /// `-w` width (default 80), `-b`, `-s`.
    pub options: FoldOptions,
    /// File operands in order.
    pub operands: Vec<&'a [u8]>,
}

/// `fold` argument-parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldError {
    /// The option letter is not in `0-9`/`b`/`s`/`w`: libc `getopt` prints one
    /// line first (`minix3/lib/libc/stdlib/getopt.c:101-104`), and then
    /// `default: usage()` adds the usage line (`fold.c:99-101`, `:115-121`) — so
    /// the old form printed only usage and lost the first line.
    UnknownOption(char),
    /// Nothing follows `-w` (no remainder in this token and no next word): the
    /// other sentence of the same family (`getopt.c:132-136`), which also adds
    /// usage. The old form reported it as the "unknown letter" sentence.
    MissingValue(char),
    /// A `-w`/legacy width that is not a positive integer (C `atoi <= 0` →
    /// `errx(1, "illegal width value")`, `fold.c:84-86`).
    IllegalWidth,
}

/// Split `fold`'s argv tail into [`FoldArgs`], faithful to C's
/// `getopt(argc, argv, "0123456789bsw:")` (`fold.c:76`): the valueless `-b` and
/// `-s` cluster in one token (`-bs`, `-bw10`), `-w` takes a value attached
/// (`-w80`) or as the following argument (`-w 80`), a leading all-digit token is
/// the obsolete `-N` width form (`fold.c:91-98`), `--` ends option scanning, a
/// lone `-` is an operand, and any other letter is [`FoldError::UnknownOption`]
/// (a value-taking option with no value goes to [`FoldError::MissingValue`]). A
/// width that is not a positive integer is [`FoldError::IllegalWidth`] — the old
/// front door warned and *kept* the default instead of rejecting, diverging from
/// C's `errx`. The old bin matched only the whole tokens `-b`/`-s`/`-w`, so the
/// C-legal attached form `fold -w80 file` fell through to a usage error.
pub fn parse_fold_args<'a>(args: &[&'a [u8]]) -> Result<FoldArgs<'a>, FoldError> {
    let mut options = FoldOptions::new(80);
    let mut operands: Vec<&'a [u8]> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg;
        if bytes.len() > 1 && bytes[0] == b'-' {
            if bytes[1].is_ascii_digit() {
                // Obsolete `-N`: the whole token is the width (`fold.c:91-98`).
                options.width = parse_width(&arg[1..])?;
                index += 1;
                continue;
            }
            let mut pos = 1;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'b' => {
                        options.count_bytes = true;
                        pos += 1;
                    }
                    b's' => {
                        options.split_words = true;
                        pos += 1;
                    }
                    b'w' => {
                        let spec = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            if index >= args.len() {
                                return Err(FoldError::MissingValue('w'));
                            }
                            args[index]
                        };
                        options.width = parse_width(spec)?;
                        pos = bytes.len();
                    }
                    // A digit inside a cluster: C's optstring lists `0-9` as
                    // legal letters (the legacy `-N` width form), so it does
                    // **not** report an unknown option; but C's case takes
                    // `argv[optind - 1]` there (`fold.c:91-98`), and the result
                    // depends on optind's internal position, which cannot be
                    // pinned without the reference binary. This shell reads it
                    // with the same semantics as `-N`: the digit run from that
                    // position is the width. This shape is registered as a
                    // **declared difference** in the stage ledger.
                    digit if digit.is_ascii_digit() => {
                        options.width = parse_width(&arg[pos..])?;
                        pos = bytes.len();
                    }
                    _ => return Err(FoldError::UnknownOption(bytes[pos] as char)),
                }
            }
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    Ok(FoldArgs { options, operands })
}

/// A width is a positive integer; `0`, a non-integer, or overflow is rejected
/// exactly as C's `atoi(optarg) <= 0` guard (`fold.c:84-86`).
fn parse_width(spec: &[u8]) -> Result<usize, FoldError> {
    // C's `atoi`: a run of decimal digits; the empty string and non-digits all
    // count as 0, after which `<= 0` reports `illegal width value`.
    if spec.is_empty() || !spec.iter().all(u8::is_ascii_digit) {
        return Err(FoldError::IllegalWidth);
    }
    let mut width: usize = 0;
    for digit in spec {
        width = width
            .saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'));
    }
    if width == 0 {
        return Err(FoldError::IllegalWidth);
    }
    Ok(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GetoptReport;

    /// Test adapter for the byte-form entry point: fixtures stay textual and
    /// fold once before entering the parser.
    fn argv_of<'w>(list: &[&'w str]) -> Vec<&'w [u8]> {
        list.iter().map(|word| word.as_bytes()).collect()
    }

    #[test]
    fn test_short_lines_pass_through() {
        let options = FoldOptions::new(80);
        assert_eq!(fold_input("hello world\n", &options), vec!["hello world"]);
    }

    #[test]
    fn test_break_after_the_last_space() {
        // Measured against the reference binary
        // `tmp/fold-405/fold_ref -s -w 10`: `one two` / `three four`. The space
        // is dropped (C increments `last_space` before moving the buffer back),
        // so the continuation line carries no leading space.
        let options = FoldOptions {
            width: 10,
            split_words: true,
            ..FoldOptions::default()
        };
        assert_eq!(
            fold_input("one two three four\n", &options),
            vec!["one two", "three four"]
        );
    }

    #[test]
    fn without_split_words_the_break_is_a_hard_one_and_spaces_stay() {
        // The same input without `-s`: `fold.c:152-176` does not look back for
        // a space, and the whole buffer is flushed as soon as the column passes
        // the width, so the space stays at the end of the line it filled — the
        // reference binary measures `one two th` / `ree four`.
        let options = FoldOptions::new(10);
        assert_eq!(
            fold_input("one two three four\n", &options),
            vec!["one two th", "ree four"]
        );
    }

    #[test]
    fn split_words_drops_the_space_and_carries_the_rest_as_the_next_line() {
        // `-s -w 4` on `abcdefghij klmnop` (the reference binary measures the
        // same six lines): the space is dropped and the remaining characters
        // stay in the buffer as the start of the next line, with the column
        // recomputed from them — `ij` is followed by `klmn`, not by ` klmn` with
        // a leading space, and not by an extra empty line.
        let options = FoldOptions {
            width: 4,
            split_words: true,
            ..FoldOptions::default()
        };
        assert_eq!(
            fold_input("abcdefghij klmnop\n", &options),
            vec!["abcd", "efgh", "ij", "klmn", "op"]
        );
    }

    #[test]
    fn test_no_space_breaks_hard() {
        let options = FoldOptions::new(4);
        assert_eq!(
            fold_input("abcdefghij\n", &options),
            vec!["abcd", "efgh", "ij"]
        );
    }

    #[test]
    fn test_split_words_breaks_anywhere() {
        let options = FoldOptions {
            width: 5,
            split_words: true,
            ..FoldOptions::default()
        };
        assert_eq!(fold_input("onetwo\n", &options), vec!["onetw", "o"]);
    }

    #[test]
    fn test_tab_counts_to_the_next_stop() {
        // One tab from column 0 lands at 8; the x at 9 stays within 10.
        let options = FoldOptions::new(10);
        assert_eq!(fold_input("\tx\n", &options), vec!["\tx"]);
        // Past width 10 the line folds after the tab.
        let options = FoldOptions::new(4);
        // The tab itself overflows: the empty buffer flushes as an empty
        // line (the C prints buf[0..0] plus the newline), then the tab
        // and the x land on the next line.
        assert_eq!(fold_input("\tx\n", &options), vec!["", "\t", "x"]);
    }

    #[test]
    fn test_count_bytes_counts_the_tab_as_one() {
        let options = FoldOptions {
            width: 4,
            count_bytes: true,
            ..FoldOptions::default()
        };
        assert_eq!(fold_input("\txyz\n", &options), vec!["\txyz"]);
    }

    #[test]
    fn test_empty_input_yields_nothing() {
        assert!(fold_input("", &FoldOptions::new(80)).is_empty());
    }

    #[test]
    fn test_parse_fold_default_and_separate_width() {
        use alloc::vec;
        // No options → width 80 default, one operand.
        assert_eq!(
            parse_fold_args(&argv_of(&["f"])),
            Ok(FoldArgs {
                options: FoldOptions::new(80),
                operands: vec![b"f".as_slice()]
            })
        );
        // `-w 20` value as the next token.
        assert_eq!(
            parse_fold_args(&argv_of(&["-w", "20", "f"])).map(|a| a.options.width),
            Ok(20)
        );
        // The regression: attached `-w20` is accepted (was a usage error).
        assert_eq!(
            parse_fold_args(&argv_of(&["-w20", "f"])).map(|a| (a.options.width, a.operands)),
            Ok((20, vec![b"f".as_slice()]))
        );
        // Obsolete leading all-digit `-40` sets the width.
        assert_eq!(
            parse_fold_args(&argv_of(&["-40", "f"])).map(|a| a.options.width),
            Ok(40)
        );
    }

    #[test]
    fn test_parse_fold_cluster_and_errors() {
        // `-b` and `-s` cluster; `-bw30` clusters `-b` then attached `-w30`.
        assert_eq!(
            parse_fold_args(&argv_of(&["-bs"]))
                .map(|a| (a.options.count_bytes, a.options.split_words)),
            Ok((true, true))
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-bw30"])).map(|a| (a.options.count_bytes, a.options.width)),
            Ok((true, 30))
        );
        // `--` ends options; a lone `-` is an operand.
        assert_eq!(
            parse_fold_args(&argv_of(&["--", "-w"])).map(|a| a.operands),
            Ok(vec![b"-w".as_slice()])
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-"])).map(|a| a.operands),
            Ok(vec![b"-".as_slice()])
        );
        // Unknown letter → usage; `-w` with no value → usage.
        assert_eq!(
            parse_fold_args(&argv_of(&["-z", "f"])),
            Err(FoldError::UnknownOption('z'))
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-w"])),
            Err(FoldError::MissingValue('w'))
        );
        // A digit inside a cluster is not treated as an unknown letter (C's
        // optstring includes `0-9`).
        assert_eq!(
            parse_fold_args(&argv_of(&["-b9"])).map(|a| (a.options.count_bytes, a.options.width)),
            Ok((true, 9))
        );
        // A non-positive / non-integer width → IllegalWidth (C errx).
        assert_eq!(
            parse_fold_args(&argv_of(&["-w", "0"])),
            Err(FoldError::IllegalWidth)
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-w", "abc"])),
            Err(FoldError::IllegalWidth)
        );
    }
    #[test]
    fn parse_fold_args_keeps_a_raw_ff_operand_as_its_own_bytes() {
        assert_eq!(
            parse_fold_args(&[&b"a\xffb"[..]]),
            Ok(FoldArgs {
                options: FoldOptions::new(80),
                operands: vec![b"a\xffb".as_slice()]
            }),
            "操作数带的是原字节，不是替换字符的三字节"
        );
    }

    #[test]
    fn parse_fold_args_names_a_raw_ff_option_letter_as_one_byte() {
        // Extraction takes one byte (`bytes[pos] as char`) and the rendering
        // door `line_bytes` writes it back verbatim: only the pair together
        // leaves a single `ff` in the letter slot.
        assert_eq!(
            parse_fold_args(&[&b"-\xff"[..]]),
            Err(FoldError::UnknownOption('\u{ff}'))
        );
        assert_eq!(
            GetoptReport::UnknownOption('\u{ff}').line_bytes("fold"),
            b"fold: unknown option -- \xff\n".as_slice()
        );
    }
}
