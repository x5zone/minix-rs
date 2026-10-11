//! Stream merging for `paste`.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD). With `-s`
//! ("serial") each input's lines join onto one line separated by
//! delimiters; without it, lines from the inputs merge in parallel —
//! the first line of each input forms the first output line. The
//! delimiter list cycles per output gap (`-d`); the default delimiter
//! is a tab. File operands wait for the gated open-existing call, so
//! the doing half serves one stdin stream: without `-s` a single
//! stream passes through unchanged (nothing to merge), and `-s`
//! joins its lines — both shapes the C defines for one input.
//! Empty strings in the delimiter list mean "no delimiter for this
//! gap" (paste.c's delim handling).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Paste options: delimiter list and serial mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PasteOptions {
    /// `-d` delimiter list; each entry is one gap's delimiter, cycling;
    /// an empty entry means no delimiter for that gap. Empty list =
    /// tab for every gap.
    pub delimiters: Vec<String>,
    /// `-s`: join each input's lines serially instead of merging
    /// across inputs.
    pub serial: bool,
}

/// Parses `-d list` and `-s`, collecting the remaining words as file operands.
/// Unknown options are [`PasteError::BadOption`].
pub fn parse_args<'a>(args: &[&'a str]) -> Result<(PasteOptions, Vec<&'a str>), PasteError> {
    let mut options = PasteOptions::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg == "-d" {
            index += 1;
            match args.get(index) {
                Some(list) => {
                    options.delimiters = list.chars().map(String::from).collect();
                }
                None => return Err(PasteError::MissingValue),
            }
        } else if arg == "-s" {
            options.serial = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            return Err(PasteError::BadOption(arg.to_string()));
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    Ok((options, operands))
}

/// Options-only view of [`parse_args`] (any file operands are dropped).
pub fn parse(args: &[&str]) -> Result<PasteOptions, PasteError> {
    Ok(parse_args(args)?.0)
}

/// What went wrong parsing the arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    /// An unknown option (the C's usage, paste.c:229-232).
    BadOption(String),
    /// `-d` without its list.
    MissingValue,
}

/// Serial mode (`-s`): joins the lines with the cycling delimiters.
///
/// With an empty delimiter list the separator is a tab; an empty
/// delimiter entry joins with nothing. The delimiter index advances
/// per gap and wraps (paste.c's `delim` cycling).
pub fn join_serial(lines: &[&str], options: &PasteOptions) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push_str(delimiter_for(index - 1, options));
        }
        out.push_str(line);
    }
    out.push('\n');
    out
}

/// The delimiter for the gap before output line `gap_index` (0 based):
/// the list cycles; an empty entry means no delimiter; an empty list
/// means tab for every gap.
fn delimiter_for(gap_index: usize, options: &PasteOptions) -> &str {
    if options.delimiters.is_empty() {
        return "\t";
    }
    let index = gap_index % options.delimiters.len();
    &options.delimiters[index]
}

/// Parallel mode (the default): merge one line from each stream into an output
/// row. A row runs while any stream still has a line; a stream that has run out
/// contributes an empty cell. The delimiter before column `j` (gap `j - 1`
/// between the previous column and this one) cycles through the list exactly as
/// [`join_serial`] does, restarting each row (paste.c's per-line `dcount`).
/// Returns the whole output, each row newline-terminated.
pub fn join_parallel(streams: &[&[&str]], options: &PasteOptions) -> String {
    let rows = streams.iter().map(|stream| stream.len()).max().unwrap_or(0);
    let mut out = String::new();
    for row in 0..rows {
        for (column, stream) in streams.iter().enumerate() {
            if column > 0 {
                out.push_str(delimiter_for(column - 1, options));
            }
            if let Some(cell) = stream.get(row) {
                out.push_str(cell);
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_joins_with_tabs_by_default() {
        let options = parse(&["-s"]).unwrap();
        assert_eq!(join_serial(&["a", "b", "c"], &options), "a\tb\tc\n");
    }

    #[test]
    fn test_parallel_merges_lines_across_streams() {
        let options = parse(&[]).unwrap();
        let a = ["a1", "a2", "a3"];
        let b = ["b1", "b2"];
        // Shorter second stream contributes an empty cell on the last row.
        assert_eq!(
            join_parallel(&[&a[..], &b[..]], &options),
            "a1\tb1\na2\tb2\na3\t\n"
        );
    }

    #[test]
    fn test_parallel_delimiter_cycles_restart_each_row() {
        let options = parse(&["-d", "+:"]).unwrap();
        let a = ["1", "2"];
        let b = ["3", "4"];
        let c = ["5", "6"];
        // gaps use +,: alternating and reset at each row start.
        assert_eq!(
            join_parallel(&[&a[..], &b[..], &c[..]], &options),
            "1+3:5\n2+4:6\n"
        );
    }

    #[test]
    fn test_parse_args_collects_file_operands() {
        let (options, operands) = parse_args(&["-d", ",", "x.txt", "y.txt"]).unwrap();
        assert_eq!(options.delimiters, vec![String::from(",")]);
        assert_eq!(operands, vec!["x.txt", "y.txt"]);
        assert_eq!(
            parse_args(&["-Z"]),
            Err(PasteError::BadOption("-Z".to_string()))
        );
    }

    #[test]
    fn test_serial_delimiter_list_cycles_per_character() {
        // paste.c cycles the -d list per character: + : % + : ...
        let options = parse(&["-s", "-d", "+:%"]).unwrap();
        assert_eq!(join_serial(&["a", "b", "c", "d"], &options), "a+b:c%d\n");
    }

    #[test]
    fn test_serial_delimiter_list_character_semantics() {
        // "-d @@" gives two @ entries cycling: gaps use @, @, @...
        let options = parse(&["-s", "-d", "@@"]).unwrap();
        assert_eq!(join_serial(&["a", "b", "c"], &options), "a@b@c\n");
    }

    #[test]
    fn test_parse_rejects_unknown() {
        assert!(matches!(parse(&["-Z"]), Err(PasteError::BadOption(_))));
        assert_eq!(parse(&["-d"]), Err(PasteError::MissingValue));
    }
}
