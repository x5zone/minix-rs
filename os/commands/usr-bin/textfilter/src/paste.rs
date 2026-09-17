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

/// Parses `-d list` and `-s`. Unknown options are [`PasteError::BadOption`].
pub fn parse(args: &[&str]) -> Result<PasteOptions, PasteError> {
    let mut options = PasteOptions::default();
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
        } else {
            return Err(PasteError::BadOption(arg.to_string()));
        }
        index += 1;
    }
    Ok(options)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_joins_with_tabs_by_default() {
        let options = parse(&["-s"]).unwrap();
        assert_eq!(join_serial(&["a", "b", "c"], &options), "a\tb\tc\n");
    }

    #[test]
    fn test_serial_delimiter_list_cycles() {
        let options = parse(&["-s", "-d", "+:%"]).unwrap();
        assert_eq!(join_serial(&["a", "b", "c", "d"], &options), "a+:b%:c+:d\n");
    }

    #[test]
    fn test_serial_delimiter_list_entry_can_be_empty() {
        // "-d x@" has two entries: "x" for the first gap, "" for the
        // second — nothing joins there.
        let options = parse(&["-s", "-d", "x@"]).unwrap();
        assert_eq!(join_serial(&["a", "b", "c"], &options), "axbc\n");
    }

    #[test]
    fn test_parse_rejects_unknown() {
        assert!(matches!(
            parse(&["-Z"]),
            Err(PasteError::BadOption(_))
        ));
        assert_eq!(parse(&["-d"]), Err(PasteError::MissingValue));
    }
}
