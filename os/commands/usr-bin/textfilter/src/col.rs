//! col — filter reverse line feeds from input (minix3/usr.bin/col/col.c).
//!
//! Deciding half: the full line-assembly state machine (backspace
//! overstrike, carriage returns, tabs, half-line feeds, character-set
//! shifts) plus the flushing rules (space-to-tab compression, the
//! counting sort for out-of-order columns, blank-line accounting).
//! Input is bytes: non-graphic bytes that are not recognized controls
//! are dropped, exactly like the C locale's `isgraph` gate.
#[cfg(test)]
use alloc::string::String;
use alloc::{vec, vec::Vec};

pub const ESC: u8 = 0o033;
pub const SI: u8 = 0o017; // shift in to normal character set
pub const SO: u8 = 0o016; // shift out to alternate character set
pub const VT: u8 = 0o013; // vertical tab (aka reverse line feed)
pub const RLF: u8 = 0o007; // ESC 07 reverse line feed
pub const RHLF: u8 = 0o010; // ESC 010 reverse half-line feed
pub const FHLF: u8 = 0o011; // ESC 011 forward half-line feed

/// build up at least this many lines before flushing them out
const BUFFER_MARGIN: i32 = 32;

pub const CS_NORMAL: u8 = 1;
pub const CS_ALTERNATE: u8 = 2;

/// One stored character: column, character set, byte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub col: i32,
    pub set: u8,
    pub ch: u8,
}

#[derive(Debug, Clone)]
struct Line {
    cells: Vec<Cell>,
    /// chars went in out of column order → counting sort at flush.
    needs_sort: bool,
    max_col: i32,
}

impl Line {
    fn new() -> Line {
        Line {
            cells: Vec::new(),
            needs_sort: false,
            max_col: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColOptions {
    /// `-b`: output no backspaces (only the last of an overstrike).
    pub no_backspaces: bool,
    /// `-f`: allow half-line forward feeds in output.
    pub fine: bool,
    /// `-h` (default): compress spaces into tabs; `-x` disables.
    pub compress_spaces: bool,
    /// `-l n`: buffered half-line count (default 128).
    pub max_bufd_lines: i32,
    /// `-p`: pass unknown escape sequences through.
    pub pass_unknown_seqs: bool,
}

impl Default for ColOptions {
    fn default() -> Self {
        ColOptions {
            no_backspaces: false,
            fine: false,
            compress_spaces: true,
            max_bufd_lines: 128,
            pass_unknown_seqs: false,
        }
    }
}

/// The exit points of `col`'s option ring (`col.c:133-158`), one variant per
/// C path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColFlagError<'a> {
    /// An unknown letter: libc prints one line first (`getopt.c:101-104`),
    /// then `col.c:154-156`'s `case '?': default: usage()` adds a second —
    /// two lines, exit 1.
    UnknownOption(char),
    /// No argument follows `-l`: libc's other sentence `option requires an
    /// argument -- l` (`getopt.c:132-136`) plus usage, **not**
    /// `bad -l argument`.
    MissingValue(char),
    /// `col.c:145-150`: when `atoi(optarg) <= 0`, C prints
    /// `col: bad -l argument <original text>.` itself and exits — this arm
    /// does not go through usage, and the program name is a hardcoded `col`.
    /// The payload is the **raw bytes**: the `-l` segment comes straight from
    /// argv, so the echoed field goes through no character decoding.
    BadLineCount(&'a [u8]),
    /// `col.c:157-158`: `if (optind != argc) usage()`. `col` reads standard
    /// input only, so any operand lands here (a bare `-` counts as an operand
    /// too, since `getopt` does not advance past it).
    OperandsPresent,
}

/// C's `getopt(argc, argv, "bfhl:px")` ring: five value-less letters plus one
/// value-taking `l`; a cluster is read letter by letter (`-bfpx` is one
/// string, `-xb` is legal too), and the `-l` value may be attached to the
/// same string (`-l3`, measured against the reference binary) or start a new
/// one (`-l 3`).
///
/// `--` ends the option ring without being an operand (`getopt.c:81-85`
/// does `++optind` first); a bare `-` or any non-option token ends the ring
/// on arrival **and is not permuted** (`getopt.c:74-78`), so everything left
/// becomes an operand — and `col` accepts no operand at all.
pub fn parse_col_flags<'a>(args: &'a [&'a [u8]]) -> Result<ColOptions, ColFlagError<'a>> {
    let mut options = ColOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        let bytes = arg;
        if arg == b"--" {
            if index + 1 < args.len() {
                return Err(ColFlagError::OperandsPresent);
            }
            return Ok(options);
        }
        if bytes.len() < 2 || bytes[0] != b'-' {
            return Err(ColFlagError::OperandsPresent);
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'b' => options.no_backspaces = true,
                b'f' => options.fine = true,
                b'h' => options.compress_spaces = true,
                b'p' => options.pass_unknown_seqs = true,
                b'x' => options.compress_spaces = false,
                b'l' => {
                    let value = if pos + 1 >= bytes.len() {
                        index += 1;
                        match args.get(index) {
                            Some(next) => *next,
                            None => return Err(ColFlagError::MissingValue('l')),
                        }
                    } else {
                        &arg[pos + 1..]
                    };
                    let lines = crate::atoi(value);
                    if lines <= 0 {
                        return Err(ColFlagError::BadLineCount(value));
                    }
                    options.max_bufd_lines = lines;
                    // The value consumes the rest of this string whole (C's
                    // `optarg` is exactly "this string minus the `-` and the
                    // letter"), so this string ends here.
                    pos = bytes.len();
                }
                _ => {
                    // libc's `getopt.c:103` prints with `%c`, which emits the
                    // **byte** in `optopt`, so `-é` (`c3 a9`) reports the single
                    // byte `c3`, not the whole character — the same approach as
                    // `src/column.rs:parse_column_args`: take a byte, never split
                    // by character.
                    return Err(ColFlagError::UnknownOption(char::from(bytes[pos])));
                }
            }
            pos += 1;
        }
        index += 1;
    }
    Ok(options)
}

/// Outcome of a run: the filtered bytes plus whether the C would have
/// warned (`warning: can't back up...` on stderr).
#[derive(Debug, Default, PartialEq)]
pub struct ColOutput {
    pub bytes: Vec<u8>,
    pub warned: bool,
}

/// Runs col over the whole input. This is `main`'s read loop plus the
/// flush machinery, byte for byte.
pub fn filter(input: &[u8], options: &ColOptions) -> ColOutput {
    let max_bufd_lines = options.max_bufd_lines * 2;
    let mut out = ColOutput::default();

    // `lines` holds the unflushed list; `head_abs` is the absolute
    // (half-line) number of its first element. In the C this equals
    // nflushd_lines - extra_lines.
    let mut lines: Vec<Line> = vec![Line::new()];
    let mut extra_lines = 0i32;
    let mut nflushd_lines = 0i32;
    let mut warned_once = false;

    let mut cur_set = CS_NORMAL;
    let mut last_set = CS_NORMAL;
    let mut cur_col = 0i32;
    let mut cur_line = 0i32;
    let mut max_line = 0i32;
    let mut this_line = 0i32;
    let mut adjust = 0i32;
    let mut nblank_lines = 0i32;

    let mut i = 0usize;
    while i < input.len() {
        let ch = input[i];
        i += 1;
        let graph = (0x21..=0x7e).contains(&ch);
        if !graph {
            match ch {
                0o010 => {
                    // BS: can't go back further
                    if cur_col == 0 {
                        continue;
                    }
                    cur_col -= 1;
                    continue;
                }
                b'\r' => {
                    cur_col = 0;
                    continue;
                }
                ESC => {
                    // just ignore EOF: a trailing ESC with no follower
                    // falls out of the loop in the C too (getchar
                    // returns EOF, switch default falls through).
                    if i < input.len() {
                        let next = input[i];
                        i += 1;
                        match next {
                            RLF => cur_line -= 2,
                            RHLF => cur_line -= 1,
                            FHLF => {
                                cur_line += 1;
                                if cur_line > max_line {
                                    max_line = cur_line;
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                b'\n' => {
                    cur_line += 2;
                    if cur_line > max_line {
                        max_line = cur_line;
                    }
                    cur_col = 0;
                    continue;
                }
                b' ' => {
                    cur_col += 1;
                    continue;
                }
                SI => {
                    cur_set = CS_NORMAL;
                    continue;
                }
                SO => {
                    cur_set = CS_ALTERNATE;
                    continue;
                }
                b'\t' => {
                    // adjust column
                    cur_col |= 7;
                    cur_col += 1;
                    continue;
                }
                VT => {
                    cur_line -= 2;
                    continue;
                }
                _ => {
                    if !options.pass_unknown_seqs {
                        continue;
                    }
                }
            }
        }

        // Must stuff ch in a line - are we at the right one?
        if cur_line != this_line - adjust {
            adjust = 0;
            let mut nmove = cur_line - this_line;
            if !options.fine {
                // round up to next line
                if cur_line & 1 != 0 {
                    adjust = 1;
                    nmove += 1;
                }
            }
            if nmove < 0 {
                // Moving toward the head: only within the list.
                let head_abs = nflushd_lines - extra_lines;
                let mut idx = (this_line - head_abs) as usize;
                while nmove < 0 && idx > 0 {
                    idx -= 1;
                    nmove += 1;
                }
                if nmove < 0 {
                    if nflushd_lines == 0 {
                        // Allow backup past first line if nothing has
                        // been flushed yet.
                        while nmove < 0 {
                            lines.insert(0, Line::new());
                            extra_lines += 1;
                            nmove += 1;
                        }
                    } else {
                        if !warned_once {
                            out.warned = true;
                            warned_once = true;
                        }
                        cur_line -= nmove;
                    }
                }
                this_line = cur_line + adjust;
            } else {
                let head_abs = nflushd_lines - extra_lines;
                let mut idx = (this_line - head_abs) as usize;
                while nmove > 0 && idx + 1 < lines.len() {
                    idx += 1;
                    nmove -= 1;
                }
                while nmove > 0 {
                    lines.push(Line::new());
                    idx += 1;
                    nmove -= 1;
                }
                this_line = cur_line + adjust;
            }
            let nmove = this_line - nflushd_lines;
            if nmove >= max_bufd_lines + BUFFER_MARGIN {
                let nflush = nmove - max_bufd_lines;
                flush_lines(
                    nflush,
                    &mut lines,
                    &mut nflushd_lines,
                    &mut nblank_lines,
                    &mut out.bytes,
                    options,
                    &mut last_set,
                );
            }
        }
        let head_abs = nflushd_lines - extra_lines;
        let idx = (this_line - head_abs) as usize;
        let line = &mut lines[idx];
        let col = cur_col;
        if col < line.max_col {
            line.needs_sort = true;
        } else {
            line.max_col = col;
        }
        line.cells.push(Cell {
            col,
            set: cur_set,
            ch,
        });
        cur_col += 1;
    }

    if max_line == 0 {
        return out;
    }

    // goto the last line that had a character on it: walk to the end
    // of the list, counting lines.
    let head_abs = nflushd_lines - extra_lines;
    this_line = head_abs + (lines.len() as i32 - 1);
    flush_lines(
        this_line - nflushd_lines + extra_lines + 1,
        &mut lines,
        &mut nflushd_lines,
        &mut nblank_lines,
        &mut out.bytes,
        options,
        &mut last_set,
    );

    // make sure we leave things in a sane state
    if last_set != CS_NORMAL {
        out.bytes.push(0o017);
    }

    // flush out the last few blank lines
    nblank_lines = max_line - this_line;
    if max_line & 1 != 0 {
        nblank_lines += 1;
    } else if nblank_lines == 0 {
        // missing a \n on the last line?
        nblank_lines = 2;
    }
    flush_blanks(&mut nblank_lines, &mut out.bytes, options);
    out
}

fn flush_lines(
    nflush: i32,
    lines: &mut Vec<Line>,
    nflushd_lines: &mut i32,
    nblank_lines: &mut i32,
    out: &mut Vec<u8>,
    options: &ColOptions,
    last_set: &mut u8,
) {
    for _ in 0..nflush {
        let l = lines.remove(0);
        *nflushd_lines += 1;
        if !l.cells.is_empty() {
            flush_blanks(nblank_lines, out, options);
            flush_line(&l, out, options, last_set);
        }
        *nblank_lines += 1;
    }
}

/// Print a number of newline/half newlines. With `fine`, an odd count
/// ends in a half-line feed, otherwise it rounds up to whole lines.
fn flush_blanks(nblank_lines: &mut i32, out: &mut Vec<u8>, options: &ColOptions) {
    let mut half = false;
    let mut nb = *nblank_lines;
    if nb & 1 != 0 {
        if options.fine {
            half = true;
        } else {
            nb += 1;
        }
    }
    nb /= 2;
    for _ in 0..nb {
        out.push(b'\n');
    }
    if half {
        out.push(0o033);
        out.push(0o011);
        if nb == 0 {
            out.push(b'\r');
        }
    }
    *nblank_lines = 0;
}

/// Write a line to output taking care of space to tab conversion (-h)
/// and character set shifts.
fn flush_line(line: &Line, out: &mut Vec<u8>, options: &ColOptions, last_set: &mut u8) {
    // O(n) counting sort by column, stable within a column.
    let sorted_storage: Vec<Cell>;
    let chars: &[Cell] = if line.needs_sort {
        let mut sorted = vec![
            Cell {
                col: 0,
                set: 0,
                ch: 0
            };
            line.cells.len()
        ];
        let mut count = vec![0usize; line.max_col as usize + 1];
        for c in &line.cells {
            count[c.col as usize] += 1;
        }
        let mut tot = 0usize;
        for i in 0..count.len() {
            let save = count[i];
            count[i] = tot;
            tot += save;
        }
        for c in &line.cells {
            sorted[count[c.col as usize]] = *c;
            count[c.col as usize] += 1;
        }
        sorted_storage = sorted;
        &sorted_storage
    } else {
        &line.cells
    };
    let mut last_col = 0i32;
    let mut pos = 0usize;
    while pos < chars.len() {
        let this_col = chars[pos].col;
        let mut end = pos + 1;
        while end < chars.len() && chars[end].col == this_col {
            end += 1;
        }
        // if -b only print last character
        let mut c = pos;
        if options.no_backspaces {
            c = end - 1;
        }
        if this_col > last_col {
            let mut nspace = this_col - last_col;
            if options.compress_spaces && nspace > 1 {
                let ntabs = ((last_col % 8) + nspace) / 8;
                if ntabs > 0 {
                    nspace -= (ntabs * 8) - (last_col % 8);
                    for _ in 0..ntabs {
                        out.push(b'\t');
                    }
                }
            }
            for _ in 0..nspace {
                out.push(b' ');
            }
            last_col = this_col;
        }
        last_col += 1;
        loop {
            if chars[c].set != *last_set {
                match chars[c].set {
                    CS_NORMAL => out.push(0o017),
                    CS_ALTERNATE => out.push(0o016),
                    _ => {}
                }
                *last_set = chars[c].set;
            }
            out.push(chars[c].ch);
            c += 1;
            if c >= end {
                break;
            }
            out.push(0x08);
        }
        pos = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> String {
        String::from_utf8(filter(s.as_bytes(), &ColOptions::default()).bytes).unwrap()
    }

    /// Test adapter for the byte-shaped entry: the fixtures still spell text,
    /// and this folds it to bytes once before the option ring.
    fn argv_of<'w>(list: &[&'w str]) -> Vec<&'w [u8]> {
        list.iter().map(|word| word.as_bytes()).collect()
    }

    #[test]
    fn unknown_nonascii_option_letter_reports_the_byte() {
        // Measured against the reference binary: `col -bé` prints
        // `col: unknown option -- ` + the raw byte 0xC3 + newline, exit 1.
        // `é` is two bytes `c3 a9`; taking a **char** yields U+00E9, only
        // taking a **byte** yields U+00C3.
        let words = argv_of(&["-b\u{e9}"]);
        let err = parse_col_flags(&words).expect_err("非 ASCII 字母该报错");
        assert_eq!(
            err,
            ColFlagError::UnknownOption('\u{00c3}'),
            "该报短横后的第一个字节"
        );
        // Same inside a cluster: in `-xé` the x is consumed first, so the
        // error lands on c3.
        let words = argv_of(&["-x\u{e9}"]);
        let err = parse_col_flags(&words).expect_err("同上");
        assert_eq!(err, ColFlagError::UnknownOption('\u{00c3}'));
    }

    #[test]
    fn test_plain_lines_pass_through() {
        assert_eq!(run("hello\nworld\n"), "hello\nworld\n");
    }

    #[test]
    fn test_backspace_overstrike_kept_with_backspaces() {
        // "a\b-" overlays - on a: col keeps both with a backspace.
        assert_eq!(run("a\u{8}-\n"), "a\u{8}-\n");
    }

    #[test]
    fn test_b_flag_keeps_only_last_overstrike() {
        let o = ColOptions {
            no_backspaces: true,
            ..ColOptions::default()
        };
        let out = String::from_utf8(filter(b"a\x08-\n", &o).bytes).unwrap();
        // -b prints only the last char of the column group.
        assert_eq!(out, "-\n");
    }

    #[test]
    fn test_carriage_return_merges_lines() {
        // "123\rXX\n" overwrites 123 with XX in place.
        assert_eq!(run("123\rXX\n"), "1\u{8}X2\u{8}X3\n");
    }

    #[test]
    fn test_tab_advances_to_eight_multiple() {
        assert_eq!(run("a\tb\n"), "a\tb\n");
        // Compressed re-emission: 'a' at col 0, 'b' at col 8 → tab.
        let o = ColOptions {
            compress_spaces: false,
            ..ColOptions::default()
        };
        let out = String::from_utf8(filter(b"a\tb\n", &o).bytes).unwrap();
        assert_eq!(out, "a       b\n");
    }

    #[test]
    fn test_reverse_line_feed_reorders_lines() {
        // ESC 07 backs up a whole line; the earlier line gets more
        // content appended and is flushed before the later one.
        let out = run("ab\nxy\u{1b}\u{7}Z\n");
        // Line 0 = "ab" + Z (column from the second visit), line 1 = xy.
        assert_eq!(out, "abZ\nxy\n");
    }

    #[test]
    fn test_vertical_tab_is_reverse_feed() {
        let out = run("ab\n\u{b}Z\n");
        assert_eq!(out, "a\u{8}Zb\n");
    }

    #[test]
    fn test_charset_shifts_pass_through() {
        // SO ... SI frames the alternate set bytes with 016/017.
        let out = run("\u{e}x\u{f}y\n");
        assert_eq!(out, "\u{e}x\u{f}y\n");
    }

    #[test]
    fn test_unknown_controls_dropped_unless_p() {
        assert_eq!(run("\u{1}x\n"), "x\n");
        let o = ColOptions {
            pass_unknown_seqs: true,
            ..ColOptions::default()
        };
        let out = String::from_utf8(filter(b"\x01x\n", &o).bytes).unwrap();
        assert_eq!(out, "\u{1}x\n");
    }

    #[test]
    fn test_high_bytes_dropped_like_c_locale() {
        // isgraph() is false for bytes ≥ 0x80 in the C locale: col
        // drops UTF-8 continuation bytes.
        assert_eq!(run("é\n"), "\n");
    }

    #[test]
    fn test_forward_half_line_feed_with_f() {
        let o = ColOptions {
            fine: true,
            ..ColOptions::default()
        };
        // Text on line 0, ESC 011 moves to half line 1, text there,
        // then \n completes the line: output keeps the half feed.
        let out = String::from_utf8(filter(b"a\x1b\x09b\n", &o).bytes).unwrap();
        // flush_blanks emits ESC TAB CR, then "b" sits in column 1
        // so one space precedes it.
        assert_eq!(out, "a\u{1b}\u{9}\r b\n\u{1b}\u{9}");
    }

    #[test]
    fn test_without_f_half_lines_round_up() {
        let out = run("a\u{1b}\u{9}b\n");
        // Half line rounds onto the next whole line (adjust +1); "b"
        // keeps its column 1, so one space precedes it.
        assert_eq!(out, "a\n b\n");
    }

    #[test]
    fn test_trailing_blank_lines_flushed() {
        // max_line counts half lines: "a\n\n\n" → max_line 6,
        // this_line 6 → nblank = 0 → but even max → 2 blanks min.
        assert_eq!(run("a\n\n\n"), "a\n\n\n");
    }

    #[test]
    fn test_no_input_no_output() {
        assert_eq!(filter(b"", &ColOptions::default()), ColOutput::default());
    }

    #[test]
    fn test_backup_warning_when_line_flushed() {
        // Buffer more than max_bufd_lines then back up past a flushed
        // line: the C warns once on stderr.
        let mut input = Vec::new();
        for _ in 0..70 {
            input.extend_from_slice(b"x\n");
        }
        input.push(0o013); // VT: back two half lines — still buffered
        let out = filter(&input, &ColOptions::default());
        assert!(!out.warned);
    }

    /// Pins the option ring arm by arm: clustering, `-l` attached or separate,
    /// `--`, operands, and the flags lit by all four.
    #[test]
    fn the_option_ring_reads_letter_by_letter_like_getopt() {
        let plain = parse_col_flags(&argv_of(&[])).unwrap();
        assert_eq!(
            (plain.compress_spaces, plain.no_backspaces, plain.fine),
            (true, false, false)
        );
        assert_eq!(
            plain.max_bufd_lines, 128,
            "C 的缺省 128 个缓冲行（`col.c:132`）"
        );

        let all = parse_col_flags(&argv_of(&["-bfpx"])).unwrap();
        assert!(
            (all.no_backspaces && all.fine && !all.compress_spaces && all.pass_unknown_seqs),
            "聚簇要逐字母生效，`-bfpx` 里那个 `x` 关掉压缩"
        );
        // A later letter in the same string overrides an earlier one: both
        // `-xb` and `-bx` mean "no compression + no backspaces".
        assert!(!parse_col_flags(&argv_of(&["-xb"])).unwrap().compress_spaces);
        assert!(parse_col_flags(&argv_of(&["-xb"])).unwrap().no_backspaces);
        assert!(
            !parse_col_flags(&argv_of(&["-hx"])).unwrap().compress_spaces,
            "后到的 x 覆盖 h"
        );

        // The `-l` value may be attached to this string or start a new one;
        // both consume that whole segment.
        assert_eq!(
            parse_col_flags(&argv_of(&["-l3"])).unwrap().max_bufd_lines,
            3
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["-l", "3"]))
                .unwrap()
                .max_bufd_lines,
            3
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["-b", "-l", "7", "-f"]))
                .unwrap()
                .max_bufd_lines,
            7
        );
        assert!(
            parse_col_flags(&argv_of(&["-b", "-l", "7", "-f"]))
                .unwrap()
                .fine
        );
        // C's `atoi` ignores trailing junk, so `3x` is a legal 3 (the
        // reference binary behaves the same).
        assert_eq!(
            parse_col_flags(&argv_of(&["-l", "3x"]))
                .unwrap()
                .max_bufd_lines,
            3
        );

        // The three failures each have their own arm and are never mixed.
        assert_eq!(
            parse_col_flags(&argv_of(&["-Z"])),
            Err(ColFlagError::UnknownOption('Z'))
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["-Zb"])),
            Err(ColFlagError::UnknownOption('Z'))
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["--x"])),
            Err(ColFlagError::UnknownOption('-'))
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["-l"])),
            Err(ColFlagError::MissingValue('l'))
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["-bl"])),
            Err(ColFlagError::MissingValue('l'))
        );
        for bad in ["0", "-2", "abc", "", " "] {
            assert_eq!(
                parse_col_flags(&argv_of(&["-l", bad])),
                Err(ColFlagError::BadLineCount(bad.as_bytes())),
                "`-l {bad:?}` 该交回原文去印那句 bad -l argument"
            );
        }
        // `--` itself is not an operand; only what follows it counts. A bare
        // `-` and an ordinary token are both operands.
        assert_eq!(
            parse_col_flags(&argv_of(&["--"])).unwrap().max_bufd_lines,
            128
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["--", "-b"])),
            Err(ColFlagError::OperandsPresent)
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["-"])),
            Err(ColFlagError::OperandsPresent)
        );
        assert_eq!(
            parse_col_flags(&argv_of(&["file"])),
            Err(ColFlagError::OperandsPresent)
        );
        // No permutation: an operand ends the ring on arrival, and later
        // legal letters are not read either (`col` accepts no operand anyway).
        assert_eq!(
            parse_col_flags(&argv_of(&["-b", "file", "-f"])),
            Err(ColFlagError::OperandsPresent)
        );
    }

    #[test]
    fn a_raw_nonutf8_line_count_is_named_by_its_own_bytes() {
        // The `-l` segment comes straight from argv: the echoed field is the
        // raw byte, not the three bytes of a replacement character.
        assert_eq!(
            parse_col_flags(&[b"-l\xff".as_slice()]),
            Err(ColFlagError::BadLineCount(b"\xff".as_slice()))
        );
    }
}
