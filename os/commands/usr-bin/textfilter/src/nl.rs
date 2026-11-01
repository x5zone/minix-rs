//! `nl` deciding half: add line numbers with a blank fill on unnumbered lines.
//!
//! Ground truth: `minix3/usr.bin/nl/nl.c` — `filter()` (lines 272-353). For
//! each input line, a line that is exactly the logical-page delimiter (the
//! default `\:`) switches the section and is *not* emitted (C's `goto
//! nextline` before any output); when `restart` is set it also resets the
//! counter to `startnum`. Otherwise the body numbering type decides: `all`
//! numbers every line, except a run of fewer than `nblank` (`-l`) consecutive
//! blank lines is left unnumbered, `nonempty` numbers only lines that are
//! not just a newline. A numbered line prints the counter rendered by the
//! format (`%-*d`/`%*d`/`%0*d`), an unnumbered line prints `width` blanks
//! instead; either way the separator then the original line follow.
//!
//! Wired: the three logical-page sections (a line of `\:`/`\:\:`/`\:\:\:`
//! switches to footer/body/header and is dropped, resetting the counter under
//! restart), each section's numbering type via `-b`/`-h`/`-f` (a/t/n), plus
//! `-v` (start), `-i` (step), `-w` (number field width), `-n` (ln/rn/rz),
//! `-p` (no restart), `-d` (the 2-byte logical-page delimiter, `\:` by
//! default), `-s` (the separator bytes between the number field and the text,
//! a tab by default) and `-l` (blank-run grouping for `-b a`, C `nblank`,
//! default 1). Declared residual: the `number_regex` form (`-b p…`) — rides the
//! next `nl` slice.

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
    /// First number of each page (`-v`; C's `static int startnum` at
    /// `nl.c:125`, assigned from `startnum = (int)val` at `:215`). C's counter
    /// is an `int`, so a negative start number is legal and counts downward.
    pub startnum: i32,
    /// Number increment (`-i`; C's `static int incr`, assigned from
    /// `incr = (int)val` at `nl.c:182`). There is no non-zero or sign check in
    /// C, so a negative increment counts backwards and zero repeats a number.
    pub incr: i32,
    /// Line-number field width (`-w`).
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
    /// Blank-run grouping for `number_all` sections (`-l`; C `nblank`, default
    /// 1). A run of fewer than this many consecutive blank lines stays
    /// unnumbered; the `n`-th blank numbers and resets the run (`nl.c:313`).
    pub nblank: usize,
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
            nblank: 1,
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
    let mut adjblank = 0usize;
    let mut section = Section::Body;
    for line in lines {
        if let Some(next) = delimiter_section(line, opts.delim) {
            section = next;
            adjblank = 0;
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
            // C `number_all` grouping (`nl.c:307-316`): a blank line increments
            // the adjacent-blank run and stays unnumbered until the run reaches
            // `nblank`; a non-blank line numbers and resets the run.
            NumberType::All => {
                if line.is_empty() {
                    adjblank += 1;
                    if adjblank < opts.nblank {
                        false
                    } else {
                        adjblank = 0;
                        true
                    }
                } else {
                    adjblank = 0;
                    true
                }
            }
            NumberType::Nonempty => !line.is_empty(),
            NumberType::None => false,
        };
        let mut row = Vec::new();
        if donumber {
            write_number(counter, opts, &mut row);
            counter = counter.wrapping_add(opts.incr);
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
/// Render one counter value the way C's
/// `snprintf(intbuffer, intbuffersize, format, width, line)` does
/// (`nl.c:331-332`): the digits of the magnitude, a leading `-` when the value
/// is negative (C prints `line`, an `int`, through `%*d` and its two siblings at
/// `nl.c:63-65`), and the field padded to `width` — spaces on the right under
/// `-n ln`, spaces on the left under `-n rn`, and **zeros after the sign** under
/// `-n rz` (`%0*d` turns `-7` at width 5 into `-0007`, as the reference
/// measures). `i32::MIN` has no positive counterpart, so the magnitude is taken
/// in `i64`.
fn write_number(n: i32, opts: &NlOptions, out: &mut Vec<u8>) {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    let mut v = i64::from(n).unsigned_abs();
    loop {
        digits[len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let negative = n < 0;
    // C pads to the *rendered* width, and the sign occupies one of those cells.
    let pad = opts.width.saturating_sub(len + usize::from(negative));
    match opts.format {
        NumFormat::Left => {
            if negative {
                out.push(b'-');
            }
            for d in digits[..len].iter().rev() {
                out.push(*d);
            }
            out.resize(out.len() + pad, b' ');
        }
        NumFormat::Right => {
            out.resize(out.len() + pad, b' ');
            if negative {
                out.push(b'-');
            }
            for d in digits[..len].iter().rev() {
                out.push(*d);
            }
        }
        NumFormat::RightZero => {
            if negative {
                out.push(b'-');
            }
            out.resize(out.len() + pad, b'0');
            for d in digits[..len].iter().rev() {
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

    /// C's counter is an `int` (`nl.c:125` and the two `strtol` arms at
    /// `:176-183`/`:208-216`), so a negative first number and a negative
    /// increment are both accepted and the numbering runs backwards; each
    /// format then puts the sign in its own place — `-n ln` left-justifies and
    /// pads right (`-7  `), `-n rn` pads on the left (`  -7`), and `-n rz` puts
    /// the zeros **after** the minus (`-0007`, because `%0*d` pads between the
    /// sign and the digits). Every expectation is the reference's own reading
    /// (`tmp/nl-417/nl_ref` on `a\nb\nc`).
    #[test]
    fn negative_counters_and_the_sign_position_of_each_format() {
        let o = NlOptions {
            startnum: 3,
            incr: -1,
            ..Default::default()
        };
        assert_eq!(nl("a\nb\nc", &o), "     3\ta\n     2\tb\n     1\tc");
        let o = NlOptions {
            startnum: -2,
            ..Default::default()
        };
        assert_eq!(nl("a\nb\nc", &o), "    -2\ta\n    -1\tb\n     0\tc");
        let o = NlOptions {
            startnum: -7,
            width: 4,
            format: NumFormat::Left,
            ..Default::default()
        };
        assert_eq!(nl("a\nb", &o), "-7  \ta\n-6  \tb");
        let o = NlOptions {
            startnum: -7,
            width: 5,
            format: NumFormat::RightZero,
            ..Default::default()
        };
        assert_eq!(nl("a\nb", &o), "-0007\ta\n-0006\tb");
    }

    /// Stepping past `i32::MAX` wraps to `i32::MIN` the way C's `int` does
    /// (`line += incr` at `nl.c:334` is UB on overflow and two's-complement
    /// wrap in the measured reference: `-v 2147483647 -i 1 -w 12` prints
    /// `2147483647`, `-2147483648`, `-2147483647`). `i32::MIN` is also the one
    /// value whose magnitude does not fit an `int`, which is why the digits are
    /// taken in `i64` — the width here (12) exceeds both renderings, so this
    /// gate does not touch the tail-truncation leg C gets from
    /// `intbuffer + max(0, consumed - width)`.
    #[test]
    fn the_counter_wraps_at_the_int_boundary_and_prints_int_min() {
        let o = NlOptions {
            startnum: i32::MAX,
            incr: 1,
            width: 12,
            ..Default::default()
        };
        assert_eq!(
            nl("a\nb\nc", &o),
            "  2147483647\ta\n -2147483648\tb\n -2147483647\tc"
        );
        let o = NlOptions {
            startnum: i32::MIN,
            incr: 0,
            width: 12,
            ..Default::default()
        };
        assert_eq!(nl("a\nb", &o), " -2147483648\ta\n -2147483648\tb");
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

    #[test]
    fn test_all_nblank_groups_blank_run() {
        // `-b a -l 3`: only every third blank line is numbered; a run of fewer
        // than 3 blanks stays unnumbered (nl.c:307-316).
        let o = NlOptions {
            body: NumberType::All,
            nblank: 3,
            width: 3,
            ..Default::default()
        };
        assert_eq!(nl("\n\n\n\n", &o), "   \t\n   \t\n  1\t\n   \t");
    }

    #[test]
    fn test_all_nblank_resets_run_on_numbered_and_text() {
        // nblank 2: the adjacent-blank run resets both when a blank is numbered
        // and when a non-blank line appears.
        let o = NlOptions {
            body: NumberType::All,
            nblank: 2,
            width: 3,
            ..Default::default()
        };
        assert_eq!(nl("\n\nx\n\n\n", &o), "   \t\n  1\t\n  2\tx\n   \t\n  3\t");
    }
}
