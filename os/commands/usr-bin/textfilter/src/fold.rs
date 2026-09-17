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
        assert_eq!(fold_input("abcdefghij\n", &options), vec!["abcd", "efgh", "ij"]);
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
}
