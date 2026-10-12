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
//! Wired: the three logical-page sections (a line of `\:`/`\:\:`/`\:\:\:`
//! switches to footer/body/header and is dropped, resetting the counter under
//! restart), each section's numbering type via `-b`/`-h`/`-f` (a/t/n), plus
//! `-v` (start), `-i` (step), `-l` (width), `-n` (ln/rn/rz), `-p` (no restart),
//! `-d` (the 2-byte logical-page delimiter, `\:` by default) and `-s` (the
//! separator bytes between the number field and the text, a tab by default).
//! Declared residual: the `number_regex` form (`-b p…`) and `-w` blank-run
//! grouping — those ride the next `nl` slice.

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

/// How a section's lines are numbered (C `numbering_type`; the `p` regex form
/// is a declared residual and not representable here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberType {
    /// Number every line.
    All,
    /// Number non-empty lines (the body default).
    Nonempty,
    /// Number no lines (the header/footer default).
    None,
}

/// The logical page section a line belongs to (C `numbering_properties`
/// order: footer, body, header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Footer,
    Body,
    Header,
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
    /// Separator bytes between the number field and the text (`-s`; C `sep`
    /// string, a tab by default).
    pub sep: Vec<u8>,
    /// Numbering type for body lines (`-b`).
    pub body: NumberType,
    /// Numbering type for header lines (`-h`).
    pub header: NumberType,
    /// Numbering type for footer lines (`-f`).
    pub footer: NumberType,
    /// Restart numbering at each page delimiter (default on; `-p` clears it).
    pub restart: bool,
    /// The 2-byte logical-page delimiter (`-d`; C `delim`, default `\\:`). A
    /// line that is exactly one/two/three repeats of it switches to
    /// footer/body/header (`nl.c:157-169`).
    pub delim: [u8; 2],
}

impl Default for NlOptions {
    fn default() -> Self {
        NlOptions {
            startnum: 1,
            incr: 1,
            width: 6,
            format: NumFormat::Right,
            sep: [b'\t'].to_vec(),
            body: NumberType::Nonempty,
            header: NumberType::None,
            footer: NumberType::None,
            restart: true,
            delim: [b'\\', b':'],
        }
    }
}

/// Classify a delimiter line: how many copies of the 2-byte section delimiter
/// `delim` fill the line exactly (C `filter`'s FOOTER/BODY/HEADER probe, one to
/// three). Returns the section it switches to, or `None` for a non-delimiter
/// line.
fn delimiter_section(line: &[u8], delim: [u8; 2]) -> Option<Section> {
    if line.is_empty() || !line.len().is_multiple_of(delim.len()) {
        return None;
    }
    if !line.chunks(delim.len()).all(|c| c == delim) {
        return None;
    }
    match line.len() / delim.len() {
        1 => Some(Section::Footer),
        2 => Some(Section::Body),
        3 => Some(Section::Header),
        _ => None,
    }
}

/// Number `lines` (newline-stripped byte slices) into output lines, tracking
/// the current logical page section. A delimiter line switches the section,
/// resets the counter under `restart`, and is dropped from the output (C's
/// `goto nextline`); every other line is numbered per its section's type, an
/// unnumbered line still carrying the `width`-blank field and separator.
pub fn nl_lines(lines: &[&[u8]], opts: &NlOptions) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut counter = opts.startnum;
    let mut section = Section::Body;
    for line in lines {
        if let Some(next) = delimiter_section(line, opts.delim) {
            section = next;
            if opts.restart {
                counter = opts.startnum;
            }
            continue;
        }
        let kind = match section {
            Section::Footer => opts.footer,
            Section::Body => opts.body,
            Section::Header => opts.header,
        };
        let donumber = match kind {
            NumberType::All => true,
            NumberType::Nonempty => !line.is_empty(),
            NumberType::None => false,
        };
        let mut row = Vec::new();
        if donumber {
            write_number(counter, opts, &mut row);
            counter += opts.incr;
        } else {
            row.resize(row.len() + opts.width, b' ');
        }
        row.extend_from_slice(&opts.sep);
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
        let o = NlOptions { body: NumberType::All, ..Default::default() };
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
    fn test_delimiters_switch_sections_and_are_dropped() {
        // body `a`=1; `\:` switches to footer (unnumbered, dropped, reset);
        // `b` is a footer line, so its field is blank but kept.
        assert_eq!(nl("a\n\\:\nb", &NlOptions::default()), "     1\ta\n      \tb");
        // `\:\:` switches to body and (restart) resets numbering to startnum.
        assert_eq!(nl("a\n\\:\\:\nb", &NlOptions::default()), "     1\ta\n     1\tb");
        // With restart off, the body section still renumbers from the running count.
        let nop = NlOptions { restart: false, ..Default::default() };
        assert_eq!(nl("a\n\\:\\:\nb", &nop), "     1\ta\n     2\tb");
    }

    #[test]
    fn test_delim_one_char_option() {
        // `-d x` -> delimiter [x, ':']; `x:` switches to footer and is dropped
        // (nl.c:157-161 fills only delim[0], leaving ':' the default).
        let o = NlOptions {
            delim: [b'x', b':'],
            ..Default::default()
        };
        assert_eq!(nl("a\nx:\nb", &o), "     1\ta\n      \tb");
    }

    #[test]
    fn test_delim_two_byte_option() {
        // `-d '#$'` -> delimiter ['#', '$']; a line of two repeats switches to
        // body and, under restart, renumbers from startnum.
        let o = NlOptions {
            delim: [b'#', b'$'],
            ..Default::default()
        };
        assert_eq!(nl("a\n#$#$\nb", &o), "     1\ta\n     1\tb");
    }

    #[test]
    fn test_custom_delim_retires_the_default_backslash_colon() {
        // With a custom delimiter the default `\:` is an ordinary line again,
        // proving the classifier honors `-d` rather than the built-in byte.
        let o = NlOptions {
            delim: [b'x', b':'],
            ..Default::default()
        };
        assert_eq!(nl("a\n\\:\nb", &o), "     1\ta\n     2\t\\:\n     3\tb");
    }

    #[test]
    fn test_section_types_independent() {
        // -h a: header numbers every line; after `\:\:\:` (3 delims) restart
        // reset the counter to startnum, so the blank header line gets number 1.
        let o = NlOptions { header: NumberType::All, ..Default::default() };
        assert_eq!(nl("a\n\\:\\:\\:\n\n", &o), "     1\ta\n     1\t");
    }

    #[test]
    fn test_separator_string_option() {
        // `-s '::'` -> separator bytes `::` between the number field and text.
        let o = NlOptions {
            sep: b"::".to_vec(),
            ..Default::default()
        };
        assert_eq!(nl("a\nb", &o), "     1::a\n     2::b");
    }

    #[test]
    fn test_empty_separator_option() {
        // `-s ''` -> no separator; the number field abuts the text directly.
        let o = NlOptions {
            sep: Vec::new(),
            ..Default::default()
        };
        assert_eq!(nl("a\nb", &o), "     1a\n     2b");
    }
}
