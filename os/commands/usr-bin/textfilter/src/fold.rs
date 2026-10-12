//! Line folding for `fold`.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The display
//! column advances by `new_column_position` (fold.c:209-232): backspace
//! pulls one back, carriage return resets to zero, tab rounds up to the
//! next multiple of eight, and every other character adds one (byte
//! counts with `-b`); when the column passes the width the buffered
//! segment flushes — breaking after the last space unless `-s`
//! (split-words, fold.c:150-178) — the carried tail replays for its
//! column, and a newline flushes the buffer (fold.c:140-149). The
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
            if options.split_words {
                out.push(buffer.iter().collect());
                buffer.clear();
                column = advance_column(0, character, options.count_bytes);
            } else {
                // Break after the last space in the buffer; the carried
                // tail replays for its column.
                let last_space = buffer.iter().rposition(|&ch| ch == ' ');
                match last_space {
                    // C: print buf[0..last_space] — the blank itself
                    // moves to the head of the carried tail
                    // (fold.c:163-172).
                    Some(space) => {
                        out.push(buffer[..space].iter().collect());
                        let tail: Vec<char> = buffer[space..].to_vec();
                        buffer = tail;
                        column = 0;
                        for &carried in &buffer {
                            column = advance_column(column, carried, options.count_bytes);
                        }
                        column = advance_column(column, character, options.count_bytes);
                    }
                    None => {
                        out.push(buffer.iter().collect());
                        buffer.clear();
                        column = advance_column(0, character, options.count_bytes);
                    }
                }
            }
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
    pub operands: Vec<&'a str>,
}

/// `fold` argument-parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldError {
    /// An option letter outside `0-9`/`b`/`s`/`w`, or a `-w` with no value
    /// (C `getopt` default → usage, `fold.c:99`).
    Usage,
    /// A `-w`/legacy width that is not a positive integer (C `atoi <= 0` →
    /// `errx(1, "illegal width value")`, `fold.c:84-86`).
    IllegalWidth,
}

/// Split `fold`'s argv tail into [`FoldArgs`], faithful to C's
/// `getopt(argc, argv, "0123456789bsw:")` (`fold.c:76`): the valueless `-b` and
/// `-s` cluster in one token (`-bs`, `-bw10`), `-w` takes a value attached
/// (`-w80`) or as the following argument (`-w 80`), a leading all-digit token is
/// the obsolete `-N` width form (`fold.c:91-98`), `--` ends option scanning, a
/// lone `-` is an operand, and any other letter is [`FoldError::Usage`]. A width
/// that is not a positive integer is [`FoldError::IllegalWidth`] — the old
/// front door warned and *kept* the default instead of rejecting, diverging from
/// C's `errx`. The old bin matched only the whole tokens `-b`/`-s`/`-w`, so the
/// C-legal attached form `fold -w80 file` fell through to a usage error.
pub fn parse_fold_args<'a>(args: &'a [&'a str]) -> Result<FoldArgs<'a>, FoldError> {
    let mut options = FoldOptions::new(80);
    let mut operands: Vec<&'a str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
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
                                return Err(FoldError::Usage);
                            }
                            args[index]
                        };
                        options.width = parse_width(spec)?;
                        pos = bytes.len();
                    }
                    _ => return Err(FoldError::Usage),
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
fn parse_width(spec: &str) -> Result<usize, FoldError> {
    match spec.parse::<usize>() {
        Ok(width) if width > 0 => Ok(width),
        _ => Err(FoldError::IllegalWidth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_lines_pass_through() {
        let options = FoldOptions::new(80);
        assert_eq!(fold_input("hello world\n", &options), vec!["hello world"]);
    }

    #[test]
    fn test_break_after_the_last_space() {
        let options = FoldOptions::new(10);
        let folded = fold_input("one two three four\n", &options);
        // The carried " three four" replays past width 10 and folds
        // again at its own space — the C's buffer replay does the same.
        assert_eq!(folded, vec!["one two", " three", " four"]);
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
            parse_fold_args(&["f"]),
            Ok(FoldArgs {
                options: FoldOptions::new(80),
                operands: vec!["f"]
            })
        );
        // `-w 20` value as the next token.
        assert_eq!(
            parse_fold_args(&["-w", "20", "f"]).map(|a| a.options.width),
            Ok(20)
        );
        // The regression: attached `-w20` is accepted (was a usage error).
        assert_eq!(
            parse_fold_args(&["-w20", "f"]).map(|a| (a.options.width, a.operands)),
            Ok((20, vec!["f"]))
        );
        // Obsolete leading all-digit `-40` sets the width.
        assert_eq!(
            parse_fold_args(&["-40", "f"]).map(|a| a.options.width),
            Ok(40)
        );
    }

    #[test]
    fn test_parse_fold_cluster_and_errors() {
        // `-b` and `-s` cluster; `-bw30` clusters `-b` then attached `-w30`.
        assert_eq!(
            parse_fold_args(&["-bs"]).map(|a| (a.options.count_bytes, a.options.split_words)),
            Ok((true, true))
        );
        assert_eq!(
            parse_fold_args(&["-bw30"]).map(|a| (a.options.count_bytes, a.options.width)),
            Ok((true, 30))
        );
        // `--` ends options; a lone `-` is an operand.
        assert_eq!(
            parse_fold_args(&["--", "-w"]).map(|a| a.operands),
            Ok(vec!["-w"])
        );
        assert_eq!(parse_fold_args(&["-"]).map(|a| a.operands), Ok(vec!["-"]));
        // Unknown letter → usage; `-w` with no value → usage.
        assert_eq!(parse_fold_args(&["-z", "f"]), Err(FoldError::Usage));
        assert_eq!(parse_fold_args(&["-w"]), Err(FoldError::Usage));
        // A non-positive / non-integer width → IllegalWidth (C errx).
        assert_eq!(parse_fold_args(&["-w", "0"]), Err(FoldError::IllegalWidth));
        assert_eq!(
            parse_fold_args(&["-w", "abc"]),
            Err(FoldError::IllegalWidth)
        );
    }
}
