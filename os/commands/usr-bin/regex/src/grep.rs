//! Search option model and exit status computation for `grep`.
//!
//! Ground truth: `minix3/minix/usr.bin/grep/grep.c` — the usage text at
//! line 116, the flag variables at lines 67 to 86, the option handling at
//! lines 281 to 451, and the exit computation at line 505:
//!
//! ```c
//! exit(c ? (file_err ? (qflag ? 0 : 2) : 0) : (file_err ? 2 : 1));
//! ```
//!
//! where `c` counts matched lines. In words: a match exits 0 (even with
//! file errors when quiet); no match exits 1, or 2 when files failed.

use crate::RegexError;

/// The result of parsing a `grep` command line: options, up to 8 pattern
/// texts with their count, and the remaining file operands.
pub type GrepParse<'a> = (GrepOptions, [&'a str; 8], usize, &'a [&'a str]);

/// Which pattern spelling the search uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Default and `-G`: basic regular expressions.
    Basic,
    /// `-E`: extended regular expressions.
    Extended,
    /// `-F`: fixed strings, no metacharacters.
    Fixed,
}

/// The `grep` options this crate models: output shaping plus matching
/// behaviour. File traversal, recursion, and context lines stay with the
/// execution layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrepOptions {
    /// Pattern spelling selector.
    pub mode: SearchMode,
    /// `-i`: ignore case when matching.
    pub ignore_case: bool,
    /// `-v`: select lines that do not match.
    pub invert: bool,
    /// `-c`: print only the count of selected lines.
    pub count_only: bool,
    /// `-l`: print only names of files with a selected line.
    pub files_only: bool,
    /// `-n`: prefix each printed line with its line number.
    pub show_numbers: bool,
    /// `-q`: print nothing (exit status only).
    pub quiet: bool,
    /// `-x`: the pattern must match the whole line.
    pub whole_line: bool,
    /// `-w`: the match must sit on word boundaries.
    pub word: bool,
}

impl Default for GrepOptions {
    fn default() -> Self {
        GrepOptions {
            mode: SearchMode::Basic,
            ignore_case: false,
            invert: false,
            count_only: false,
            files_only: false,
            show_numbers: false,
            quiet: false,
            whole_line: false,
            word: false,
        }
    }
}

/// Parse the words after the program name into options plus patterns.
///
/// Recognised: `-E`, `-F`, `-G`, `-i`, `-v`, `-c`, `-l`, `-n`, `-q`, `-x`,
/// `-w`, and `-e pattern` (repeatable, up to 8 patterns; a line matching
/// any one of them is selected). The first non flag word is the pattern
/// when no `-e` was given; anything after the patterns is file operands
/// (left for the execution layer to open). A `--` word ends flag parsing.
/// Unknown flags and missing `-e` arguments are rejected.
pub fn parse_grep_args<'a>(argv: &'a [&'a str]) -> Result<GrepParse<'a>, RegexError> {
    let mut options = GrepOptions::default();
    let mut patterns: [&'a str; 8] = [""; 8];
    let mut pattern_count = 0;
    let mut index = 0;
    let mut end_of_flags = false;
    while index < argv.len() {
        let word = argv[index];
        if !end_of_flags && word == "--" {
            end_of_flags = true;
            index += 1;
            continue;
        }
        if !end_of_flags && word.len() > 1 && word.starts_with('-') {
            if word == "-e" {
                index += 1;
                let pattern = argv.get(index).ok_or(RegexError::InvalidPattern)?;
                if pattern_count >= patterns.len() {
                    return Err(RegexError::TooComplex);
                }
                patterns[pattern_count] = pattern;
                pattern_count += 1;
                index += 1;
                continue;
            }
            for flag in word.bytes().skip(1) {
                match flag {
                    b'E' => options.mode = SearchMode::Extended,
                    b'F' => options.mode = SearchMode::Fixed,
                    b'G' => options.mode = SearchMode::Basic,
                    b'i' => options.ignore_case = true,
                    b'v' => options.invert = true,
                    b'c' => options.count_only = true,
                    b'l' => options.files_only = true,
                    b'n' => options.show_numbers = true,
                    b'q' => options.quiet = true,
                    b'x' => options.whole_line = true,
                    b'w' => options.word = true,
                    _ => return Err(RegexError::InvalidPattern),
                }
            }
            index += 1;
        } else {
            break;
        }
    }
    let mut operands = &argv[index..];
    if pattern_count == 0 {
        let (first, rest) = operands.split_first().ok_or(RegexError::InvalidPattern)?;
        patterns[0] = first;
        pattern_count = 1;
        operands = rest;
    }
    Ok((options, patterns, pattern_count, operands))
}

/// Compute the process exit status from the outcome, mirroring
/// `grep.c:505` exactly: a match exits 0 unless files failed without quiet
/// (then 2); no match exits 1, or 2 when files failed.
pub fn exit_code(matched_any: bool, file_error: bool, quiet: bool) -> i32 {
    if matched_any {
        if file_error && !quiet {
            2
        } else {
            0
        }
    } else if file_error {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_operand_form() {
        let argv = ["hello", "file.txt"];
        let (options, patterns, count, files) = parse_grep_args(&argv).unwrap();
        assert_eq!(options.mode, SearchMode::Basic);
        assert_eq!(count, 1);
        assert_eq!(patterns[0], "hello");
        assert_eq!(files, &["file.txt"]);
    }

    #[test]
    fn test_bundled_flags_and_e_pattern() {
        let argv = ["-in", "-e", "foo", "-e", "bar"];
        let (options, patterns, count, files) = parse_grep_args(&argv).unwrap();
        assert!(options.ignore_case);
        assert!(options.show_numbers);
        assert_eq!(count, 2);
        assert_eq!(patterns[1], "bar");
        assert!(files.is_empty());
    }

    #[test]
    fn test_mode_flags() {
        let argv_e = ["-E", "a|b"];
        assert_eq!(
            parse_grep_args(&argv_e).unwrap().0.mode,
            SearchMode::Extended
        );
        let argv_f = ["-F", "a.c"];
        assert_eq!(
            parse_grep_args(&argv_f).unwrap().0.mode,
            SearchMode::Fixed
        );
    }

    #[test]
    fn test_unknown_flag_rejected() {
        assert_eq!(
            parse_grep_args(&["-Z", "x"]),
            Err(RegexError::InvalidPattern)
        );
    }

    #[test]
    fn test_missing_pattern_rejected() {
        assert_eq!(parse_grep_args(&[]), Err(RegexError::InvalidPattern));
        assert_eq!(parse_grep_args(&["-e"]), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_double_dash_ends_flags() {
        let argv = ["--", "-n"];
        let (_, patterns, count, _) = parse_grep_args(&argv).unwrap();
        assert_eq!(count, 1);
        assert_eq!(patterns[0], "-n");
    }

    #[test]
    fn test_exit_code_matches_c_truth_table() {
        // (matched, file_error, quiet) -> status, mirroring grep.c:505.
        assert_eq!(exit_code(true, false, false), 0);
        assert_eq!(exit_code(true, true, false), 2);
        assert_eq!(exit_code(true, true, true), 0);
        assert_eq!(exit_code(false, false, false), 1);
        assert_eq!(exit_code(false, true, false), 2);
        assert_eq!(exit_code(false, true, true), 2);
    }
}
