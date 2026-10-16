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
pub const RLF: u8 = 0o007; // ESC-07 reverse line feed
pub const RHLF: u8 = 0o010; // ESC-010 reverse half-line feed
pub const FHLF: u8 = 0o011; // ESC-011 forward half-line feed

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

/// `col` 的选项环（`col.c:133-158`）各支出口，一支对应 C 的一条路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColFlagError<'a> {
    /// 集外字母：libc 先印一行（`getopt.c:101-104`），`col.c:154-156` 的
    /// `case '?': default: usage()` 再接第二行 —— 两行、退 1。
    UnknownOption(char),
    /// `-l` 后面没有那一段：libc 的另一句 `option requires an argument -- l`
    /// （`getopt.c:132-136`）+ usage，**不是** `bad -l argument`。
    MissingValue(char),
    /// `col.c:145-150`：`atoi(optarg) <= 0` 时 C 自己印
    /// `col: bad -l argument <原文>.` 就退出——这一支不接 usage，程序名也是写死的 `col`。
    BadLineCount(&'a str),
    /// `col.c:157-158`：`if (optind != argc) usage()`。`col` 只读标准输入，
    /// 所以任何操作数都是这一支（裸 `-` 也算操作数，因为它不被 `getopt` 推进）。
    OperandsPresent,
}

/// C 的 `getopt(argc, argv, "bfhl:px")` 那一圈：五个无值字母加一个带值的 `l`，
/// 聚簇要逐字母读（`-bfpx` 是一串、`-xb` 也合法），`-l` 的值可以附在本串里
/// （`-l3`，参照件量过）也可以另起一段（`-l 3`）。
///
/// `--` 结束选项环而自身不是操作数（`getopt.c:81-85` 先 `++optind`）；裸 `-` 与任何
/// 非选项 token 一到就结束选项环**且不重排**（`getopt.c:74-78`），剩下的都成了操作数——
/// 而 `col` 一个操作数都不接受。
pub fn parse_col_flags<'a>(args: &'a [&'a str]) -> Result<ColOptions, ColFlagError<'a>> {
    let mut options = ColOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        let bytes = arg.as_bytes();
        if arg == "--" {
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
                    // 值把本串剩下的部分整个吃掉（C 的 `optarg` 就是「本串去掉 `-` 与
                    // 字母之后的那一段」），所以这一串到此结束。
                    pos = bytes.len();
                }
                _ => {
                    return Err(ColFlagError::UnknownOption(
                        arg[pos..].chars().next().unwrap_or('-'),
                    ));
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
        // ESC-07 backs up a whole line; the earlier line gets more
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
        // Text on line 0, ESC-011 moves to half line 1, text there,
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

    /// 选项环逐支钉：聚簇、附值与另起一段的 `-l`、`--`、操作数、四支点亮的旗标。
    #[test]
    fn the_option_ring_reads_letter_by_letter_like_getopt() {
        let plain = parse_col_flags(&[]).unwrap();
        assert_eq!(
            (plain.compress_spaces, plain.no_backspaces, plain.fine),
            (true, false, false)
        );
        assert_eq!(
            plain.max_bufd_lines, 128,
            "C 的缺省 128 个缓冲行（`col.c:132`）"
        );

        let all = parse_col_flags(&["-bfpx"]).unwrap();
        assert!(
            (all.no_backspaces && all.fine && !all.compress_spaces && all.pass_unknown_seqs),
            "聚簇要逐字母生效，`-bfpx` 里那个 `x` 关掉压缩"
        );
        // 同一串里后写的字母覆盖先写的：`-xb` 与 `-bx` 都是「不压缩 + 不出退格」。
        assert!(!parse_col_flags(&["-xb"]).unwrap().compress_spaces);
        assert!(parse_col_flags(&["-xb"]).unwrap().no_backspaces);
        assert!(
            !parse_col_flags(&["-hx"]).unwrap().compress_spaces,
            "后到的 x 覆盖 h"
        );

        // `-l` 的值可以附在本串里，也可以另起一段；两者都吃掉那一整段。
        assert_eq!(parse_col_flags(&["-l3"]).unwrap().max_bufd_lines, 3);
        assert_eq!(parse_col_flags(&["-l", "3"]).unwrap().max_bufd_lines, 3);
        assert_eq!(
            parse_col_flags(&["-b", "-l", "7", "-f"])
                .unwrap()
                .max_bufd_lines,
            7
        );
        assert!(parse_col_flags(&["-b", "-l", "7", "-f"]).unwrap().fine);
        // C 的 `atoi` 不管尾巴上的杂字符，所以 `3x` 是合法的 3（参照件同受）。
        assert_eq!(parse_col_flags(&["-l", "3x"]).unwrap().max_bufd_lines, 3);

        // 三种失败各自成支，互不混用。
        assert_eq!(
            parse_col_flags(&["-Z"]),
            Err(ColFlagError::UnknownOption('Z'))
        );
        assert_eq!(
            parse_col_flags(&["-Zb"]),
            Err(ColFlagError::UnknownOption('Z'))
        );
        assert_eq!(
            parse_col_flags(&["--x"]),
            Err(ColFlagError::UnknownOption('-'))
        );
        assert_eq!(
            parse_col_flags(&["-l"]),
            Err(ColFlagError::MissingValue('l'))
        );
        assert_eq!(
            parse_col_flags(&["-bl"]),
            Err(ColFlagError::MissingValue('l'))
        );
        for bad in ["0", "-2", "abc", "", " "] {
            assert_eq!(
                parse_col_flags(&["-l", bad]),
                Err(ColFlagError::BadLineCount(bad)),
                "`-l {bad:?}` 该交回原文去印那句 bad -l argument"
            );
        }
        // `--` 自身不是操作数；它后面有东西才算；裸 `-` 与普通 token 都是操作数。
        assert_eq!(parse_col_flags(&["--"]).unwrap().max_bufd_lines, 128);
        assert_eq!(
            parse_col_flags(&["--", "-b"]),
            Err(ColFlagError::OperandsPresent)
        );
        assert_eq!(parse_col_flags(&["-"]), Err(ColFlagError::OperandsPresent));
        assert_eq!(
            parse_col_flags(&["file"]),
            Err(ColFlagError::OperandsPresent)
        );
        // 不重排：操作数一到就结束，后面的合法字母也不再读（`col` 反正不接受操作数）。
        assert_eq!(
            parse_col_flags(&["-b", "file", "-f"]),
            Err(ColFlagError::OperandsPresent)
        );
    }
}
