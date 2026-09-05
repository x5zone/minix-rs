//! Quote aware word splitting for shell input lines.
//!
//! Ground truth: the Almquist shell grammar (`minix3/bin/sh/parser.c`,
//! 1686 lines) reduces, at the lowest level, to the same question this
//! module answers: where does one word end and the next begin when quotes,
//! backslashes, and comments can hide blanks? The C parser builds syntax
//! nodes on top; this module stops at words, which is exactly the layer the
//! command tools (`env`, `xargs`, startup file readers) need.
//!
//! Rules implemented:
//!
//! - Blank characters (space, tab) separate words; leading and trailing
//!   blanks are ignored.
//! - `#` starts a comment running to the end of the line (only outside
//!   quotes and only at a word start, matching shell practice).
//! - `'...'` quotes verbatim: nothing inside is special, and the quotes
//!   vanish from the word.
//! - `"..."` quotes weakly: blanks stay literal, the quotes vanish, and
//!   the word is marked so expansion knows it was quoted.
//! - `\` outside quotes escapes the next byte (including newline, which
//!   vanishes entirely: line continuation); inside double quotes it escapes
//!   only `$`, `` ` ``, `"`, `\`, and newline.
//!
//! Words borrow from the input line (quotes excluded by re-slicing runs);
//! because removing quotes shortens the text, each word is reported as a
//! list of borrowed slices plus its quoting mark.

use crate::ShellError;

/// How strongly a word (or word part) was quoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quoting {
    /// No quotes involved: expansion applies fully.
    Bare,
    /// Came from double quotes: blanks literal, expansion still applies.
    Weak,
    /// Came from single quotes: everything literal, no expansion.
    Strong,
}

/// One shell word: up to 8 borrowed runs plus the strongest quoting seen.
///
/// A word like `a"b c"d` arrives as runs `a`, `b c`, `d` with quoting
/// `Weak` (the double quoted middle dominates). Callers needing one flat
/// string join the runs; callers expanding variables walk them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word<'a> {
    /// Borrowed text runs making up the word (quotes excluded).
    pub runs: [&'a str; 8],
    /// How many of `runs` are used.
    pub run_count: usize,
    /// Strongest quoting over the runs (`Strong` beats `Weak` beats `Bare`).
    pub quoting: Quoting,
}

impl<'a> Word<'a> {
    /// Total bytes across all runs.
    pub fn len(&self) -> usize {
        self.runs[..self.run_count].iter().map(|run| run.len()).sum()
    }

    /// True when the word holds no bytes.
    pub fn is_empty(&self) -> bool {
        self.run_count == 0 || self.len() == 0
    }
}

/// Maximum words per line; more is rejected, not truncated.
pub const MAX_WORDS: usize = 64;

/// Split `line` into words.
///
/// Returns the words plus how many are used. A trailing comment is
/// dropped. Unterminated quotes and a trailing lone backslash are syntax
/// errors, matching the C shell refusing the line.
pub fn split_words(line: &str) -> Result<([Word<'_>; MAX_WORDS], usize), ShellError> {
    let bytes = line.as_bytes();
    let mut words: [Word<'_>; MAX_WORDS] = [Word {
        runs: [" "; 8],
        run_count: 0,
        quoting: Quoting::Bare,
    }; MAX_WORDS];
    let mut word_count = 0;
    let mut pos = 0;
    while pos < bytes.len() {
        // Skip blanks between words.
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        // A comment ends the line when it starts a word.
        if bytes[pos] == b'#' {
            break;
        }
        if word_count >= MAX_WORDS {
            return Err(ShellError::TooLong);
        }
        let (word, next) = parse_word(line, pos)?;
        words[word_count] = word;
        word_count += 1;
        pos = next;
    }
    Ok((words, word_count))
}

/// Append one borrowed run to a word, tracking the strongest quoting.
fn push_run<'a>(word: &mut Word<'a>, run: &'a str, quoting: Quoting) -> Result<(), ShellError> {
    if word.run_count >= word.runs.len() {
        return Err(ShellError::TooLong);
    }
    word.runs[word.run_count] = run;
    word.run_count += 1;
    if quoting as u8 > word.quoting as u8 {
        word.quoting = quoting;
    }
    Ok(())
}

/// Parse one word starting at `pos`; returns the word plus the position
/// just past it (at a blank, `#` at word start, or end of line).
fn parse_word(line: &str, mut pos: usize) -> Result<(Word<'_>, usize), ShellError> {
    let bytes = line.as_bytes();
    let mut word = Word {
        runs: [" "; 8],
        run_count: 0,
        quoting: Quoting::Bare,
    };
    while pos < bytes.len() {
        match bytes[pos] {
            b' ' | b'\t' => break,
            b'\'' => {
                let start = pos + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'\'' {
                    end += 1;
                }
                if end >= bytes.len() {
                    return Err(ShellError::InvalidSyntax);
                }
                push_run(&mut word, &line[start..end], Quoting::Strong)?;
                pos = end + 1;
            }
            b'"' => {
                let start = pos + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'"' {
                    if bytes[end] == b'\\'
                        && end + 1 < bytes.len()
                        && matches!(bytes[end + 1], b'$' | b'`' | b'"' | b'\\' | b'\n')
                    {
                        end += 2;
                    } else {
                        end += 1;
                    }
                }
                if end >= bytes.len() {
                    return Err(ShellError::InvalidSyntax);
                }
                push_run(&mut word, &line[start..end], Quoting::Weak)?;
                pos = end + 1;
            }
            b'\\' => {
                if pos + 1 >= bytes.len() {
                    // A lone backslash-newline continues the line; a lone
                    // backslash at the very end is a syntax error.
                    return Err(ShellError::InvalidSyntax);
                }
                if bytes[pos + 1] == b'\n' {
                    pos += 2;
                    continue;
                }
                push_run(&mut word, &line[pos + 1..pos + 2], Quoting::Bare)?;
                pos += 2;
            }
            _ => {
                let start = pos;
                while pos < bytes.len()
                    && !matches!(bytes[pos], b' ' | b'\t' | b'\'' | b'"' | b'\\')
                {
                    pos += 1;
                }
                push_run(&mut word, &line[start..pos], Quoting::Bare)?;
            }
        }
    }
    Ok((word, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Join a word's runs into one flat string for assertions.
    fn flat(word: &Word<'_>, buf: &mut [u8]) -> usize {
        let mut len = 0;
        for run in word.runs[..word.run_count].iter() {
            for byte in run.as_bytes() {
                if len < buf.len() {
                    buf[len] = *byte;
                    len += 1;
                }
            }
        }
        len
    }

    #[test]
    fn test_blanks_separate_words() {
        let (words, count) = split_words("  ls   -l  /tmp ").unwrap();
        assert_eq!(count, 3);
        assert_eq!(words[0].runs[0], "ls");
        assert_eq!(words[1].runs[0], "-l");
        assert_eq!(words[2].runs[0], "/tmp");
    }

    #[test]
    fn test_comment_dropped() {
        let (_, count) = split_words("ls # list files").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_single_quotes_verbatim() {
        let (words, count) = split_words("echo 'a  b'").unwrap();
        assert_eq!(count, 2);
        assert_eq!(words[1].runs[0], "a  b");
        assert_eq!(words[1].quoting, Quoting::Strong);
    }

    #[test]
    fn test_double_quotes_keep_blanks() {
        let (words, _) = split_words("echo \"a  b\"").unwrap();
        assert_eq!(words[1].runs[0], "a  b");
        assert_eq!(words[1].quoting, Quoting::Weak);
    }

    #[test]
    fn test_mixed_word_runs() {
        let (words, count) = split_words("a\"b c\"d").unwrap();
        assert_eq!(count, 1);
        assert_eq!(words[0].run_count, 3);
        assert_eq!(words[0].quoting, Quoting::Weak);
    }

    #[test]
    fn test_backslash_escapes_blank() {
        let (words, count) = split_words("a\\ b").unwrap();
        // One word in three runs: `a`, the escaped blank, `b`.
        assert_eq!(count, 1);
        assert_eq!(words[0].run_count, 3);
        assert_eq!(words[0].runs[1], " ");
    }

    #[test]
    fn test_unterminated_single_quote_rejected() {
        assert_eq!(
            split_words("echo 'abc").map(|(_, count)| count),
            Err(ShellError::InvalidSyntax)
        );
    }

    #[test]
    fn test_unterminated_double_quote_rejected() {
        assert_eq!(
            split_words("echo \"abc").map(|(_, count)| count),
            Err(ShellError::InvalidSyntax)
        );
    }

    #[test]
    fn test_empty_line_gives_no_words() {
        let (_, count) = split_words("   ").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_mixed_word_flattens() {
        let (words, _) = split_words("a\"b c\"d").unwrap();
        let mut buf = [0u8; 32];
        let len = flat(&words[0], &mut buf);
        assert_eq!(&buf[..len], b"ab cd");
    }
}
