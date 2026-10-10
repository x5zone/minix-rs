//! `nl` deciding half: add line numbers with a blank fill on unnumbered lines.
//!
//! Ground truth: `minix3/usr.bin/nl/nl.c` — `filter()` (lines 272-353). For
//! each input line, a line that is exactly the logical-page delimiter (the
//! default `\:`) switches the section and is *not* emitted (C's `goto
//! nextline` before any output); when `restart` is set it also resets the
//! counter to `startnum`. Otherwise the body numbering type decides: `all`
//! numbers every line (with the `nblank` blank-run grouping defaulting to
//! "group of 1", i.e. every line), `nonempty` numbers only lines that are
//! not just a newline. A numbered line prints the counter rendered by the
//! format (`%-*d`/`%*d`/`%0*d`), an unnumbered line prints `width` blanks
//! instead; either way the separator then the original line follow.
//!
//! Wired: body numbering with `-a` (number all), `-v` (start), `-i` (step),
//! `-l` (width), `-n` (ln/rn/rz), `-p` (no restart). Declared residual: the
//! separate header/footer/body numbering types (`-b`/`-h`/`-f` with their own
//! values, and `number_regex`), `-d` multi-char delimiters, `-s` separator
//! strings, `-w` blank-run counts, and `-b` field selection — those ride the
//! next `nl` slice; here every non-delimiter line is treated as body.

use alloc::vec::Vec;

/// Where the number sits within its field (C `FORMAT_LN`/`FORMAT_RN`/`FORMAT_RZ`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumFormat {
    /// Left justified, leading zeros suppressed.
    Left,
    /// Right justified, leading zeros suppressed (the C default).
    Right,
    /// Right justified, leading zeros kept.
    RightZero,
}

/// Line-numbering options (the C globals `nl` reads from its flags).
#[derive(Debug, Clone)]
pub struct NlOptions {
    /// First number of each page (`-v`).
    pub startnum: u64,
    /// Number increment (`-i`).
    pub incr: u64,
    /// Line-number field width (`-l`).
    pub width: usize,
    /// Number field format (`-n`).
    pub format: NumFormat,
    /// Separator byte between the number and the text (C `sep`, default tab).
    pub sep: u8,
    /// Number every line, not just non-empty ones (`-a`).
    pub number_all: bool,
    /// Restart numbering at each page delimiter (default on; `-p` clears it).
    pub restart: bool,
}

impl Default for NlOptions {
    fn default() -> Self {
        NlOptions {
            startnum: 1,
            incr: 1,
            width: 6,
            format: NumFormat::Right,
            sep: b'\t',
            number_all: false,
            restart: true,
        }
    }
}

/// A line is the logical-page delimiter when it is exactly the default `\:`
/// with no other content (the C "was this the whole line?" test).
fn is_delimiter(line: &[u8]) -> bool {
    line == b"\\:"
}

/// Number `lines` (newline-stripped byte slices) into output lines.
pub fn nl_lines(lines: &[&[u8]], opts: &NlOptions) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut counter = opts.startnum;
    for line in lines {
        if is_delimiter(line) {
            if opts.restart {
                counter = opts.startnum;
            }
            continue; // the delimiter line itself is not emitted
        }
        let nonempty = !line.is_empty();
        let donumber = opts.number_all || nonempty;
        let mut row = Vec::new();
        if donumber {
            write_number(counter, opts, &mut row);
            counter += opts.incr;
        } else {
            row.resize(row.len() + opts.width, b' ');
        }
        row.push(opts.sep);
        row.extend_from_slice(line);
        out.push(row);
    }
    out
}

/// Render `n` into the fixed-width number field per the chosen format.
fn write_number(n: u64, opts: &NlOptions, out: &mut Vec<u8>) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    let mut v = n;
    loop {
        digits[len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let digits = &digits[..len];
    let pad = opts.width.saturating_sub(len);
    match opts.format {
        NumFormat::Left => {
            for d in digits.iter().rev() {
                out.push(*d);
            }
            out.resize(out.len() + pad, b' ');
        }
        NumFormat::Right => {
            out.resize(out.len() + pad, b' ');
            for d in digits.iter().rev() {
                out.push(*d);
            }
        }
        NumFormat::RightZero => {
            out.resize(out.len() + pad, b'0');
            for d in digits.iter().rev() {
                out.push(*d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nl(src: &str, opts: &NlOptions) -> String {
        let lines: Vec<&[u8]> = {
            let raw: Vec<&[u8]> = src.as_bytes().split(|&b| b == b'\n').collect();
            // a trailing newline yields an empty last element we drop
            if raw.last() == Some(&&b""[..]) {
                &raw[..raw.len() - 1]
            } else {
                &raw[..]
            }
            .to_vec()
        };
        let out = nl_lines(&lines, opts);
        out.iter()
            .map(|l| core::str::from_utf8(l).unwrap().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_numbers_nonempty_by_default() {
        assert_eq!(nl("a\nb", &NlOptions::default()), "     1\ta\n     2\tb");
    }

    #[test]
    fn test_blank_line_gets_blank_field() {
        // The middle empty line is not numbered but keeps the width + sep.
        assert_eq!(
            nl("a\n\nb", &NlOptions::default()),
            "     1\ta\n      \t\n     2\tb"
        );
    }

    #[test]
    fn test_number_all_includes_blanks() {
        let o = NlOptions { number_all: true, ..Default::default() };
        assert_eq!(nl("a\n\nb", &o), "     1\ta\n     2\t\n     3\tb");
    }

    #[test]
    fn test_startnum_incr_width() {
        let o = NlOptions { startnum: 5, incr: 5, width: 3, ..Default::default() };
        assert_eq!(nl("a\nb\nc", &o), "  5\ta\n 10\tb\n 15\tc");
    }

    #[test]
    fn test_left_and_zero_formats() {
        let ln = NlOptions { format: NumFormat::Left, width: 3, ..Default::default() };
        assert_eq!(nl("a\nb", &ln), "1  \ta\n2  \tb");
        let rz = NlOptions { format: NumFormat::RightZero, width: 3, ..Default::default() };
        assert_eq!(nl("a\nb", &rz), "001\ta\n002\tb");
    }

    #[test]
    fn test_delimiter_resets_and_is_dropped() {
        // A `\:` line resets numbering (restart on) and is not emitted.
        assert_eq!(
            nl("a\n\\:\nb", &NlOptions::default()),
            "     1\ta\n     1\tb"
        );
        // With restart off, the delimiter is still dropped but numbering runs on.
        let nop = NlOptions { restart: false, ..Default::default() };
        assert_eq!(nl("a\n\\:\nb", &nop), "     1\ta\n     2\tb");
    }
}
