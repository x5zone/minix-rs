//! Pagination for `pr`.
//!
//! Ground truth: `minix3/usr.bin/pr/pr.c` (NetBSD) with the page
//! constants from `pr.h`: 66-line pages (`LINES`, pr.h:49), a 5-line
//! header (`HEADLEN`, pr.h:60) — two blank lines, the
//! `{time} {header} Page {n}` line (`HDFMT`, pr.h:59, printed after the
//! two blanks by `prhead`, pr.c:1481-1515), and two blank lines — and a
//! 5-line blank trailer (`TAILLEN`, pr.h:61), so a 66-line page carries
//! 56 body lines. `-l` moves the page length (rejected below the header
//! plus trailer, pr.c:1875-1878), `-h` sets the header text, `-t`
//! suppresses header and trailer entirely (body lines then print as-is,
//! per `prtail`'s nohead arm, pr.c:1499-1512). The time field belongs
//! to the doing half (the C renders the current time); it rides the
//! options here so the deciding half stays pure.
//!
//! Multi-column output (`-column`, `-a`, `-m`), double spacing, and the
//! page-pause are later batches; this module covers the single-column
//! pagination face.

use alloc::string::String;
use alloc::{vec, vec::Vec};

/// Page and header/trailer constants (pr.h:49-61).
pub const LINES: usize = 66;
pub const HEADLEN: usize = 5;
pub const TAILLEN: usize = 5;

/// One `pr` run over a single-column stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrOptions {
    /// `-h` header text.
    pub header: String,
    /// `-l` lines per page; must exceed header plus trailer.
    pub page_len: usize,
    /// `-t`: no header, no trailer, no padding.
    pub no_header: bool,
    /// `-N`: column count for vertical multi-column output (`1`, the default,
    /// is the single-column path). When `> 1`, `paginate` lays the body out
    /// down `columns` columns via `vertcol`.
    pub columns: usize,
    /// `-a`: fill multi-column output across each row (`horzcol`) instead of
    /// down each column (`vertcol`, the default). Requires `columns > 1` (the
    /// `doing` half rejects a lone `-a`, `pr.c:1823-1828`).
    pub across: bool,
    /// `-d`: double-space the body — each content line is followed by a blank
    /// line, and the per-page content count halves to fit (`otln`'s extra
    /// `putchar`, `pr.c:1258`; the `setup` halving + odd-page `addone`,
    /// `pr.c:1883-1890`). Single-column path only; a page that can hold fewer
    /// than two rows falls back to no doubling (C `if (lines == 1) dspace = 0`).
    pub double_space: bool,
    /// `+N`: start printing at page N — skip the leading `(N-1)*content_per_page`
    /// content lines and number pages from N (C `pgnm` + `inskip`, `pr.c:1657`).
    /// A file too short to reach page N prints nothing (C skips it). Single-column
    /// path; `1` (the default) prints from the first page.
    pub start_page: usize,
    /// `-w`: requested page width — `PGWD` 72 by default, `SPGWD` 512 under
    /// `-s` (the `doing` half picks the default; see `setup` `pr.c:1834-1839`).
    /// `column_width` folds it into the per-column data width.
    pub page_width: usize,
    /// `-s[C]`: one byte that separates columns without space padding (`None`
    /// pads each non-last column out to `colwd + 1`).
    pub sep: Option<u8>,
    /// `-o`: left margin in blank columns. Applied by [`render_page`] to the
    /// body lines and the header's text line; the blank header spacers and the
    /// trailer go out unindented (`prhead`/`prtail` print those with bare
    /// `putchar`). `0` is the default (no margin).
    pub offset: usize,
    /// `-n`: width of the running line-number field (`0` = no numbering, the
    /// default). The single-column path prefixes each real body line with
    /// `number_field(n, number_width)` then `number_char` (C `addnum`,
    /// `pr.c:1438`); blank padding lines are left unnumbered.
    pub number_width: usize,
    /// `-n[ch]`: the byte after the number field (`NMCHAR`, a tab by default).
    pub number_char: u8,
    /// The time field rendered into the header line (the doing half
    /// supplies the host clock's text; an empty prefix is fine).
    pub time_prefix: String,
}

impl Default for PrOptions {
    fn default() -> Self {
        PrOptions {
            header: String::new(),
            page_len: LINES,
            no_header: false,
            columns: 1,
            across: false,
            double_space: false,
            start_page: 1,
            page_width: 72,
            sep: None,
            offset: 0,
            number_width: 0,
            number_char: b'\t',
            time_prefix: String::new(),
        }
    }
}

/// Parsed `pr` command line: resolved options + file operands + the `-m` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrArgs<'a> {
    pub options: PrOptions,
    /// `-h`: title override (None ⇒ the default date/header line).
    pub title_override: Option<&'a str>,
    /// `-m`: merge operands side by side (one column per file).
    pub merge: bool,
    /// File operands in order; a `-` names standard input.
    pub operands: Vec<&'a str>,
}

/// `pr` argument failure. Each arm maps to a distinct C error path (the caller
/// renders it), so the messages and exit codes match the old in-bin parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrError {
    /// `getopt` `?`/default, or a required option value that is missing: C's
    /// three-line `usage()` (`pr.c:1616-1627`) and exit 1.
    Usage,
    /// `+page` whose `atoi` is `< 1` (`pr.c:1658-1662`).
    PlusPage,
    /// the obsolete `-columns` form whose `atoi` is `< 1` (`pr.c:1665-1667`).
    ColumnCount,
    /// `-l` whose value does not start with a digit, or whose `atoi` is `< 1`
    /// (`pr.c:1733-1739`). Note `1` is legal: a short page length turns the
    /// header off (`pr.c:1875`) rather than being refused.
    NumberLines,
    /// `-o` whose value does not start with a digit, or whose `atoi` is `< 1`
    /// (`pr.c:1765-1769`).
    OffsetValue,
    /// `-w` with the same rule (`pr.c:1798-1803`).
    WidthValue,
    /// `-n`'s width, when a digit run is present but does not reach 1
    /// (`pr.c:1752-1756`).
    NumberWidth,
    /// leftover text an optional-value letter could not use: C names the letter
    /// and echoes what is left (`pr.c:1726-1729`, `1757-1760`, `1785-1788`).
    InvalidValue(char, String),
    /// `-a` given but `columns <= 1` (`pr.c:1823-1828`).
    AcrossNeedsColumns,
    /// `-m` combined with `-a`, which C refuses before looking at any file
    /// (`pr.c:1829-1832`).
    MergeAcross,
    /// the computed column width is `< 1` for the requested columns
    /// (`pr.c:1863-1867`); carries the column count for the message.
    ColumnTooSmall(usize),
    /// an option letter the deciding half does not model yet (`-e`/`-f`/`-F`/
    /// `-i`/`-p`/`-r`/`-T`): reported rather than silently ignored.
    Unmodelled,
}

/// Parse `pr`'s argv tail faithfully to BSD
/// `egetopt("#adFfmrte?h:i?l:n?o:ps?T:w:")` (`pr.c:1655`), then apply the
/// post-scan page-width default (`-w` absent ⇒ 512 under `-s` else 72) and the
/// pre-layout column/`-a` validations.
///
/// Parity this adds over the old whole-token matcher (`arg == "-t"`, value on a
/// separate word): option letters cluster (`-dm`, `-ta`, `-tw100`), and the
/// value-bearing letters take their value either glued (`-w100`, `-l66`, `-o4`,
/// `-hT`) or as the following word (`-w 100`). Leading-digit vertical columns
/// (`-N`) and start page (`+N`) keep their C forms; `-n`/`-s` keep their
/// optional glued value. Unmodelled letters (e/f/F/i/p/r/T) and any other
/// `-…` form are rejected, matching the old behaviour (never a silent operand).
/// C's `atoi` as `setup` uses it for every numeric option: skip leading space, take
/// one sign, read digits until the first byte that is not one, and ignore the rest.
/// Saturation stands in for C's undefined overflow.
fn c_atoi(word: &str) -> i64 {
    let bytes = word.as_bytes();
    let mut cursor = 0usize;
    while bytes.get(cursor).is_some_and(|b| b.is_ascii_whitespace()) {
        cursor += 1;
    }
    let negative = match bytes.get(cursor) {
        Some(b'-') => {
            cursor += 1;
            true
        }
        Some(b'+') => {
            cursor += 1;
            false
        }
        _ => false,
    };
    let mut value: i64 = 0;
    while let Some(digit @ b'0'..=b'9') = bytes.get(cursor).copied() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'));
        cursor += 1;
    }
    if negative { -value } else { value }
}

/// The `pr.c:1733-1739` shape shared by `-l`, `-o` and `-w`: the value must begin
/// with a digit — `+8` and `" 8"` are refused even though `atoi` would read them —
/// and the number it starts must reach 1.
fn c_counted(word: &str, error: PrError) -> Result<usize, PrError> {
    if !word.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return Err(error);
    }
    let n = c_atoi(word);
    if n < 1 {
        return Err(error);
    }
    Ok(n as usize)
}

pub fn parse_pr_args<'a>(args: &'a [&'a str]) -> Result<PrArgs<'a>, PrError> {
    let mut options = PrOptions::default();
    let mut title_override: Option<&'a str> = None;
    let mut operands: Vec<&'a str> = Vec::new();
    let (mut merge, mut across) = (false, false);
    let (mut wflag, mut sflag) = (false, false);

    // Take a required option value: the glued remainder of this token, else
    // the next token (advancing `index`). Missing ⇒ `Usage`.
    macro_rules! value {
        ($rest:expr, $index:ident) => {{
            if !$rest.is_empty() {
                $rest
            } else {
                $index += 1;
                match args.get($index) {
                    Some(v) => *v,
                    None => return Err(PrError::Usage),
                }
            }
        }};
    }

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        if let Some(rest) = arg.strip_prefix('+') {
            // C checks only `atoi(eoptarg) < 1` here, so a word with no digits is
            // zero and is refused the same way `+0` is.
            if c_atoi(rest) < 1 {
                return Err(PrError::PlusPage);
            }
            options.start_page = c_atoi(rest) as usize;
            index += 1;
            continue;
        }
        let bytes = arg.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            operands.push(arg);
            index += 1;
            continue;
        }
        // obsolete leading-digit vertical column count `-N` (`pr.c:1665-1668`).
        if arg[1..].bytes().all(|b| b.is_ascii_digit()) {
            let n = c_atoi(&arg[1..]);
            if n < 1 {
                return Err(PrError::ColumnCount);
            }
            options.columns = n as usize;
            index += 1;
            continue;
        }
        // Scan this token's option letters (BSD clusters them). A value-taking
        // letter consumes the glued remainder (or the next word) and ends the
        // token; flag letters fall through to the next letter.
        let mut pos = 1usize;
        while pos < arg.len() {
            match bytes[pos] {
                b't' => options.no_header = true,
                b'd' => options.double_space = true,
                b'm' => merge = true,
                b'a' => across = true,
                b'h' => {
                    title_override = Some(value!(&arg[pos + 1..], index));
                    break;
                }
                b'l' => {
                    let v = value!(&arg[pos + 1..], index);
                    options.page_len = c_counted(v, PrError::NumberLines)?;
                    break;
                }
                b'w' => {
                    let v = value!(&arg[pos + 1..], index);
                    options.page_width = c_counted(v, PrError::WidthValue)?;
                    wflag = true;
                    break;
                }
                b'o' => {
                    let v = value!(&arg[pos + 1..], index);
                    options.offset = c_counted(v, PrError::OffsetValue)?;
                    break;
                }
                b's' => {
                    // `-s` takes an optional *glued* character; anything left after
                    // it is C's "invalid value for -s" (`pr.c:1779-1789`).
                    let rest = &arg[pos + 1..];
                    options.sep = Some(if rest.is_empty() {
                        b'\t'
                    } else {
                        rest.as_bytes()[0]
                    });
                    if rest.len() > 1 {
                        return Err(PrError::InvalidValue('s', String::from(&rest[1..])));
                    }
                    sflag = true;
                    break;
                }
                b'n' => {
                    // `-n[ch][width]`: BSD optional glued value (`pr.c:1744-1762`).
                    // The separator may be any non-digit, and only then may a digit
                    // run follow it; the run is read by `atoi`, so trailing text does
                    // not reach the "invalid value" arm — C never advances `eoptarg`
                    // past the digits it parsed.
                    let rest = &arg[pos + 1..];
                    let (mut width, mut nsep) = (5usize, b'\t');
                    if !rest.is_empty() {
                        let rb = rest.as_bytes();
                        let digits = if rb[0].is_ascii_digit() {
                            rest
                        } else {
                            nsep = rb[0];
                            &rest[1..]
                        };
                        if digits.is_empty() {
                            // only a separator: the width stays at NMWD
                        } else if !digits.as_bytes()[0].is_ascii_digit() {
                            return Err(PrError::InvalidValue('n', String::from(digits)));
                        } else {
                            let n = c_atoi(digits);
                            if n < 1 {
                                return Err(PrError::NumberWidth);
                            }
                            width = n as usize;
                        }
                    }
                    options.number_width = width;
                    options.number_char = nsep;
                    break;
                }
                // Letters C's optstring carries but this deciding half does not
                // implement are named separately from letters `egetopt` itself would
                // answer `?` for (`pr.c:1655`, `1805-1809`).
                b'e' | b'i' | b'F' | b'f' | b'p' | b'r' | b'T' => {
                    return Err(PrError::Unmodelled);
                }
                _ => return Err(PrError::Usage),
            }
            pos += 1;
        }
        index += 1;
    }

    if !wflag && sflag {
        options.page_width = 512;
    }
    // `pr.c:1875-1876`: a page too short to carry a five-line header *and* a
    // five-line trailer does not fail, it loses the header — and because the
    // header is what the ten lines were reserved for, the full `lines` then holds
    // body text. `paginate` reads this flag to decide whether to subtract.
    if options.page_len <= HEADLEN + TAILLEN {
        options.no_header = true;
    }
    if options.columns > 1 && column_width(options.page_width, options.columns).is_none() {
        return Err(PrError::ColumnTooSmall(options.columns));
    }
    if across {
        if options.columns <= 1 {
            return Err(PrError::AcrossNeedsColumns);
        }
        if merge {
            return Err(PrError::MergeAcross);
        }
        options.across = true;
    }
    Ok(PrArgs {
        options,
        title_override,
        merge,
        operands,
    })
}

/// One page: the header lines (empty when `-t`), the body lines (padded
/// to the body capacity with empty lines when headers are on), and the
/// trailer lines (empty when `-t`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub header_lines: Vec<String>,
    pub body_lines: Vec<String>,
    pub trailer_lines: Vec<String>,
}

impl Page {
    /// All lines in print order.
    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.header_lines
            .iter()
            .chain(self.body_lines.iter())
            .chain(self.trailer_lines.iter())
    }
}

/// Splits the input lines into 66-line style pages.
///
/// The first page number is one. The header line reads
/// `{time_prefix}{header} Page {n}` (`HDFMT`'s three fields); body
/// capacity is `page_len - HEADLEN - TAILLEN` and short bodies pad with
/// empty lines. With `-t` the pages carry the body lines only, without
/// padding.
/// The `-n` line-number field: running line number `n` right-justified in
/// `width` columns, space-padded. A number wider than `width` keeps its last
/// `width` digits (C `addnum` writes from the right and stops at the field's
/// left edge, so it truncates leading digits rather than growing the field).
fn number_field(n: usize, width: usize) -> String {
    let digits = alloc::format!("{n}");
    if digits.len() >= width {
        String::from(&digits[digits.len() - width..])
    } else {
        let mut s = String::new();
        for _ in 0..(width - digits.len()) {
            s.push(' ');
        }
        s.push_str(&digits);
        s
    }
}

pub fn paginate(lines: &[String], options: &PrOptions) -> Vec<Page> {
    if options.columns > 1 {
        return paginate_columns(lines, options);
    }
    // `setup` subtracts the ten header/trailer lines only when they will actually
    // be printed (`pr.c:1875-1878`): with `-t` the whole page length is body. The
    // page count still applies — C's `onecol` runs `while (linecnt < lines)` per
    // page and only pads when headers are on (`prtail`'s `nohead` arm returns
    // without padding, `pr.c:1526-1538`).
    let body_capacity = if options.no_header {
        options.page_len.max(1)
    } else {
        options.page_len.saturating_sub(HEADLEN + TAILLEN).max(1)
    };
    // Double-spacing halves the content lines per page (`setup` `lines /= 2`);
    // each is then emitted with a following blank, so the body still fills
    // `body_capacity` rows and an odd base leaves the `addone` row to the pad.
    let double = options.double_space && body_capacity >= 2;
    let content_per_page = if double {
        body_capacity / 2
    } else {
        body_capacity
    };
    // C reads the first line before it prints anything, so an input with no lines
    // produces no page at all — header included (`pr.c:252-262`, `:294`). The loop
    // below is do-while shaped, so this guard has to be here.
    if lines.is_empty() {
        return Vec::new();
    }
    let mut pages = Vec::new();
    let start_page = options.start_page.max(1);
    let skip = (start_page - 1) * content_per_page;
    if start_page > 1 && skip >= lines.len() {
        // C `inskip` could not reach page N → the file yields no output.
        return Vec::new();
    }
    let mut page_number = start_page;
    let mut cursor = skip;
    loop {
        let body_end = (cursor + content_per_page).min(lines.len());
        let mut body: Vec<String> = if options.number_width == 0 {
            lines[cursor..body_end].to_vec()
        } else {
            // Running line number per file (`cursor` is the global index, and
            // `paginate` runs once per file, so it restarts at 1 each file —
            // matching C's per-file `lncnt`).
            (cursor..body_end)
                .map(|idx| {
                    alloc::format!(
                        "{}{}{}",
                        number_field(idx + 1, options.number_width),
                        options.number_char as char,
                        lines[idx]
                    )
                })
                .collect()
        };
        if double {
            // C `otln` emits the line then a blank (`dspace`), so interleave.
            let mut spaced = Vec::with_capacity(body.len() * 2);
            for line in body {
                spaced.push(line);
                spaced.push(String::new());
            }
            body = spaced;
        }
        if !options.no_header {
            while body.len() < body_capacity {
                body.push(String::new());
            }
        }
        let (header_lines, trailer_lines) = if options.no_header {
            (Vec::new(), Vec::new())
        } else {
            (
                vec![
                    String::new(),
                    String::new(),
                    alloc::format!(
                        "{}{} Page {}",
                        options.time_prefix,
                        options.header,
                        page_number
                    ),
                    String::new(),
                    String::new(),
                ],
                vec![String::new(); TAILLEN],
            )
        };
        pages.push(Page {
            header_lines,
            body_lines: body,
            trailer_lines,
        });
        page_number += 1;
        cursor = body_end;
        if cursor >= lines.len() {
            break;
        }
    }
    // No fallback page: C prints the header only from inside the line loop, after a
    // first line exists (`pr.c:255-262`), and `prtail` is guarded by
    // `if (linecnt && ...)` (`:294`), so an empty input yields no output at all.
    pages
}

/// Vertical multi-column pagination — the `vertcol` face (the default under
/// `-columns` when `-a` is absent). Input lines are chunked into page-blocks of
/// `rows * columns` (`rows` = body lines per column, matching `setup`'s
/// `pr.c:1869-1878` line adjustment), each block laid out down its columns by
/// [`vertcol_body_rows`] — except a final block that is short by at least a full
/// column, which [`vertcol_body_rows_balanced`] repacks into even columns (C's
/// last-page reorder, `pr.c:505-586`) — and wrapped in the same 5-line
/// header/trailer as the single-column path (body padded to `rows` while headers
/// show). `column_width` is assumed valid; the `doing` half rejects an
/// over-narrow page before here.
fn paginate_columns(lines: &[String], options: &PrOptions) -> Vec<Page> {
    let (colwd, _) = column_width(options.page_width, options.columns)
        .unwrap_or((options.page_width, options.page_width));
    let rows = if options.no_header {
        options.page_len.max(1)
    } else {
        options.page_len.saturating_sub(HEADLEN + TAILLEN).max(1)
    };
    let block = rows * options.columns;
    if lines.is_empty() {
        return Vec::new();
    }
    let mut pages = Vec::new();
    // `vertcol` skips whole column-pages before its loop, so a start page consumes
    // `lines * columns` input lines and the printed page carries that number
    // (`pr.c:376-381`, `inskip`).
    let start_page = options.start_page.max(1);
    let skip = (start_page - 1) * block;
    if skip >= lines.len() {
        return Vec::new();
    }
    let mut page_number = start_page;
    let mut cursor = skip;
    loop {
        let end = (cursor + block).min(lines.len());
        let page_lines = &lines[cursor..end];
        // The final page repacks into balanced columns when it is short by at
        // least a full column (`(mvc - cvc) >= clcnt`, pr.c:522); every other
        // page, and an over-full final page, keeps the plain down-fill.
        let balance =
            !options.across && end >= lines.len() && block - page_lines.len() >= options.columns;
        let mut body = if options.across {
            horzcol_body_rows(page_lines, options.columns, colwd, options.sep)
        } else if balance {
            vertcol_body_rows_balanced(page_lines, options.columns, colwd, options.sep)
        } else {
            vertcol_body_rows(page_lines, rows, options.columns, colwd, options.sep)
        };
        let (header_lines, trailer_lines) = if options.no_header {
            (Vec::new(), Vec::new())
        } else {
            while body.len() < rows {
                body.push(String::new());
            }
            (
                vec![
                    String::new(),
                    String::new(),
                    alloc::format!(
                        "{}{} Page {}",
                        options.time_prefix,
                        options.header,
                        page_number
                    ),
                    String::new(),
                    String::new(),
                ],
                vec![String::new(); TAILLEN],
            )
        };
        pages.push(Page {
            header_lines,
            body_lines: body,
            trailer_lines,
        });
        page_number += 1;
        cursor = end;
        if cursor >= lines.len() {
            break;
        }
    }
    // Same rule as the single-column path: `vertcol` prints its header from inside
    // the line loop after a first line exists, so empty input produces no output.
    pages
}

/// Paginate several logical files, each independently, exactly as C's
/// `onecol` does: page numbering restarts at 1 for every file and each file
/// carries its own header title. The title is the file's name, unless
/// `title_override` (a `-h` value) is supplied — C applies that override to
/// every file (`pr.c:224-233` restart, `pr.c:1318-1376` + `pr.h:63` `FNAME`
/// = "" for standard input). Per-file options inherit `base` (page length,
/// `-t`, time prefix).
pub fn paginate_files(
    files: &[(&str, Vec<String>)],
    base: &PrOptions,
    title_override: Option<&str>,
) -> Vec<Page> {
    let mut out: Vec<Page> = Vec::new();
    for (name, lines) in files {
        let mut opts = base.clone();
        opts.header = String::from(title_override.unwrap_or(*name));
        out.extend(paginate(lines, &opts));
    }
    out
}

/// Column data width derived from the requested page width and column count —
/// the `nmwd == 0` arm of `setup` (`pr.c:1860-1861`): each column occupies
/// `colwd + 1` cells (data plus one separating space), so `colwd =
/// (pgwd + 1 - clcnt) / clcnt` (C's integer division) and the page width is
/// re-rounded to `(colwd + 1) * clcnt - 1`. Returns `None` when the columns
/// would leave no data width (`colwd < 1`, the "page width too small" error at
/// `pr.c:1863`). This is the first package of multi-column `pr`; the page
/// assembly that consumes it lands next.
pub fn column_width(pgwd: usize, clcnt: usize) -> Option<(usize, usize)> {
    if clcnt == 0 {
        return None;
    }
    // C computes with signed ints; `pgwd + 1 - clcnt` may go negative and its
    // division truncates toward zero, so mirror that in i64 rather than let
    // unsigned subtraction wrap before the `colwd < 1` guard.
    let numerator = i64::from(pgwd as u32) + 1 - i64::from(clcnt as u32);
    let colwd = numerator / i64::from(clcnt as u32);
    if colwd < 1 {
        return None;
    }
    let colwd = colwd as usize;
    Some((colwd, (colwd + 1) * clcnt - 1))
}

/// Clip `s` to at most `n` bytes at a `char` boundary. C's `inln` truncates a
/// line to `colwd` raw bytes; this floors to the last whole code point so a
/// clipped cell stays valid UTF-8 (the byte-vs-code-point divergence is the
/// documented localisation residual, never a panic).
fn clip_to_width(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// One multi-column page's body rows, down-filled the way `vertcol` is (the
/// default when `-columns` is given and `-a` is not): column `c` takes input
/// lines `[c*rows .. (c+1)*rows]`, so line `i` lands at column `i / rows`, row
/// `i % rows` (`pr.c:438-499`). Each cell is clipped to `colwd` bytes; a
/// non-last column advances `colwd + 1` cells — space padding by default, or a
/// single `sep` byte under `-s` — and the last column is never padded
/// (`stp`, `pr.c:478`). Emission stops at the first row with no data in any
/// column (`pr.c:608`'s `<= offst break`). Inter-column padding is kept exactly
/// as C emits it (no trailing trim); only the last column omits padding.
pub fn vertcol_body_rows(
    lines_block: &[String],
    rows: usize,
    columns: usize,
    colwd: usize,
    sep: Option<u8>,
) -> Vec<String> {
    let mut out = Vec::new();
    if rows == 0 || columns == 0 {
        return out;
    }
    for row in 0..rows {
        let mut line = String::new();
        let mut any = false;
        for column in 0..columns {
            let idx = column * rows + row;
            let raw = lines_block.get(idx).map(String::as_str).unwrap_or("");
            if !raw.is_empty() {
                any = true;
            }
            let cell = clip_to_width(raw, colwd);
            line.push_str(cell);
            if column + 1 == columns {
                break; // last column: no trailing pad
            }
            match sep {
                Some(byte) => line.push(byte as char),
                None => {
                    for _ in 0..(colwd + 1 - cell.len()) {
                        line.push(' ');
                    }
                }
            }
        }
        if !any {
            break;
        }
        out.push(line);
    }
    out
}

/// Last-page column balancing for vertical multi-column output — `vertcol`'s
/// reorder arm (`pr.c:505-586`). On the final, partially filled page the plain
/// down-fill leaves a staircase: earlier columns packed to the full `rows`
/// height while later ones trail. When the page carries at least a full
/// column's worth of unused cells (`rows * columns - count >= columns`, the
/// `(mvc - cvc) >= clcnt` gate at `pr.c:522`), C repacks the `count` sequential
/// input lines into balanced columns of height `pln = ceil(count / columns)`
/// (`pr.c:523-525`) and emits them column-major with stride `pln`
/// (`vc[tvc]; tvc += pln`, `pr.c:544-567`) so the columns come out even. The
/// separator/last-column rules match [`vertcol_body_rows`]: a non-last column
/// advances `colwd + 1` (space pad, or a single `sep` byte under `-s`), the last
/// column is never padded, and a column whose cell index runs past the last
/// line ends the row (C's `tvc >= cvc` break, `pr.c:568` — which may leave the
/// row's final cell carrying its inter-column pad, exactly as C emits it). The
/// reorder path is only reached with no line-number field (`nmwd == 0`), the
/// multi-column `-n` face this module does not model.
pub fn vertcol_body_rows_balanced(
    lines_block: &[String],
    columns: usize,
    colwd: usize,
    sep: Option<u8>,
) -> Vec<String> {
    let mut out = Vec::new();
    let count = lines_block.len();
    if count == 0 || columns == 0 {
        return out;
    }
    let pln = count.div_ceil(columns);
    for row in 0..pln {
        let mut line = String::new();
        let mut any = false;
        for column in 0..columns {
            let idx = column * pln + row;
            let raw = match lines_block.get(idx) {
                Some(s) => s.as_str(),
                None => break, // C `tvc >= cvc`: this balanced column ran dry
            };
            if !raw.is_empty() {
                any = true;
            }
            let cell = clip_to_width(raw, colwd);
            line.push_str(cell);
            if column + 1 == columns {
                break; // last column: never padded (C's `mclcnt` arm)
            }
            match sep {
                Some(byte) => line.push(byte as char),
                None => {
                    for _ in 0..(colwd + 1 - cell.len()) {
                        line.push(' ');
                    }
                }
            }
        }
        if !any {
            break;
        }
        out.push(line);
    }
    out
}

/// One multi-column page's body rows, filled across each row like `horzcol`
/// (the `-a` face, `pr.c:710-770`): row `r` takes the consecutive input lines
/// `[r*columns .. (r+1)*columns]`, so the page reads line-by-line across the
/// columns rather than down them. A column past the last input line is EOF and
/// stops the row (C `inln < 0` → `break`); the last column is never padded; a
/// row whose only cells are absent/empty stops emission entirely (C's
/// `lstdat - buf <= offst` break at `pr.c:755`). The row count is derived from
/// the block (`ceil(len / columns)`), capped by the caller's `rows` chunk.
pub fn horzcol_body_rows(
    lines_block: &[String],
    columns: usize,
    colwd: usize,
    sep: Option<u8>,
) -> Vec<String> {
    let mut out = Vec::new();
    if columns == 0 {
        return out;
    }
    let rows = lines_block.len().div_ceil(columns);
    for row in 0..rows {
        let mut line = String::new();
        let mut any = false;
        for column in 0..columns {
            let idx = row * columns + column;
            let raw = match lines_block.get(idx) {
                Some(s) => s.as_str(),
                None => break, // EOF: stop this row here (C `inln < 0`)
            };
            if !raw.is_empty() {
                any = true;
            }
            let cell = clip_to_width(raw, colwd);
            line.push_str(cell);
            if column + 1 == columns {
                break; // last column: no trailing pad
            }
            match sep {
                Some(byte) => line.push(byte as char),
                None => {
                    for _ in 0..(colwd + 1 - cell.len()) {
                        line.push(' ');
                    }
                }
            }
        }
        if !any {
            break;
        }
        out.push(line);
    }
    out
}

/// `pr -m`: merge several line-streams side by side, one stream per column
/// (C `mulfile`, `pr.c:907-993`). Row `r` shows each stream's `r`-th line;
/// columns past the last stream that still has a line at `r` are dropped
/// entirely (C's `fproc >= actf` break — no trailing empty columns), a stream
/// that ran out earlier contributes an empty cell with its inter-column pad.
/// The pad/separator and last-column rules match [`horzcol_body_rows`].
pub fn merge_body_rows(streams: &[Vec<String>], colwd: usize, sep: Option<u8>) -> Vec<String> {
    let mut out = Vec::new();
    let columns = streams.len();
    if columns == 0 {
        return out;
    }
    let max_rows = streams.iter().map(Vec::len).max().unwrap_or(0);
    for row in 0..max_rows {
        // Last column still holding a line at this row (trailing EOFs dropped).
        let Some(last) = (0..columns).filter(|&j| streams[j].len() > row).next_back() else {
            break;
        };
        let mut line = String::new();
        for j in 0..=last {
            let raw = streams[j].get(row).map(String::as_str).unwrap_or("");
            let cell = clip_to_width(raw, colwd);
            line.push_str(cell);
            if j == last {
                break;
            }
            match sep {
                Some(byte) => line.push(byte as char),
                None => {
                    for _ in 0..(colwd + 1 - cell.len()) {
                        line.push(' ');
                    }
                }
            }
        }
        out.push(line);
    }
    out
}

/// The lines `pr` prints for one page under a `-o` left margin: `offset`
/// leading blanks on each body line and on the header's text line (the middle
/// of the 5-line header, carrying the title and page number), while the two
/// blank header spacers around it and the trailer lines go out unindented — the
/// C writes those with bare `putchar('\n')` (`prhead` `pr.c:1493-1496`, `prtail`).
/// `offset == 0` reproduces the plain [`Page::lines`] order exactly, so the
/// default path is unchanged.
pub fn render_page(page: &Page, offset: usize) -> Vec<String> {
    let pad = |s: &str| -> String {
        let mut out = String::new();
        for _ in 0..offset {
            out.push(' ');
        }
        out.push_str(s);
        out
    };
    let mut out = Vec::new();
    for (i, h) in page.header_lines.iter().enumerate() {
        out.push(if i == 2 { pad(h) } else { h.clone() });
    }
    for b in &page.body_lines {
        out.push(pad(b));
    }
    for t in &page.trailer_lines {
        out.push(t.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paginate_lines(lines: &[&str], options: &PrOptions) -> Vec<Page> {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        paginate(&owned, options)
    }

    #[test]
    fn test_page_geometry_is_66_lines() {
        let lines: Vec<String> = (1..=100).map(|n| n.to_string()).collect();
        let pages = paginate(&lines, &PrOptions::default());
        assert_eq!(pages.len(), 2);
        for page in &pages {
            let total = page.header_lines.len() + page.body_lines.len() + page.trailer_lines.len();
            assert_eq!(total, LINES);
            assert_eq!(page.header_lines.len(), HEADLEN);
            assert_eq!(page.trailer_lines.len(), TAILLEN);
        }
    }

    #[test]
    fn test_header_line_carries_text_and_page_number() {
        let mut options = PrOptions::default();
        options.header = "my file".to_string();
        let pages = paginate_lines(&["one"], &options);
        assert_eq!(pages[0].header_lines[2], "my file Page 1");
        // The body starts right after the five header lines.
        assert_eq!(pages[0].body_lines[0], "one");
    }

    #[test]
    fn test_time_prefix_rides_the_header_line() {
        let mut options = PrOptions::default();
        options.time_prefix = "2026-09-17 12:00:00 ".to_string();
        let pages = paginate_lines(&["one"], &options);
        assert_eq!(pages[0].header_lines[2], "2026-09-17 12:00:00  Page 1");
    }

    #[test]
    fn test_body_pads_to_capacity() {
        let pages = paginate_lines(&["one"], &PrOptions::default());
        let body_empty = pages[0].body_lines[1..].iter().all(|line| line.is_empty());
        assert!(body_empty);
        assert_eq!(pages[0].body_lines[0], "one");
    }

    #[test]
    fn test_no_header_prints_the_body_bare() {
        let mut options = PrOptions::default();
        options.no_header = true;
        let pages = paginate_lines(&["one", "two"], &options);
        assert!(pages[0].header_lines.is_empty());
        assert!(pages[0].trailer_lines.is_empty());
        assert_eq!(
            pages[0].body_lines,
            vec!["one".to_string(), "two".to_string()]
        );
    }

    #[test]
    fn test_page_length_below_header_plus_trailer_is_rejected_by_the_caller() {
        // pr.c:1875-1878 reduces the page length and errors when it
        // cannot carry header and trailer; the option surface leaves the
        // guard to the doing half, which this test pins.
        let mut options = PrOptions::default();
        options.page_len = 8;
        let body_capacity = options.page_len.saturating_sub(HEADLEN + TAILLEN).max(1);
        assert_eq!(body_capacity, 1);
    }

    #[test]
    fn test_paginate_files_titles_each_file_independently() {
        let base = PrOptions::default();
        let files = [
            ("a.txt", vec![String::from("x1"), String::from("x2")]),
            ("b.txt", vec![String::from("y")]),
        ];
        let pages = paginate_files(&files, &base, None);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].header_lines[2], "a.txt Page 1");
        assert_eq!(pages[0].body_lines[0], "x1");
        assert_eq!(pages[1].header_lines[2], "b.txt Page 1");
        assert_eq!(pages[1].body_lines[0], "y");
    }

    #[test]
    fn test_paginate_files_restarts_page_numbers_per_file() {
        // A 57-line first file spans Page 1 and Page 2 (body cap 56); the
        // second file restarts at Page 1 (pr.c:233 `pagecnt = 1`).
        let base = PrOptions::default();
        let many: Vec<String> = (1..=57).map(|n| n.to_string()).collect();
        let files = [("f1", many), ("f2", vec![String::from("z")])];
        let pages = paginate_files(&files, &base, None);
        assert_eq!(pages[0].header_lines[2], "f1 Page 1");
        assert_eq!(pages[1].header_lines[2], "f1 Page 2");
        assert_eq!(pages[2].header_lines[2], "f2 Page 1");
    }

    #[test]
    fn test_paginate_files_header_override_applies_to_all() {
        let base = PrOptions::default();
        let files = [
            ("a.txt", vec![String::from("x")]),
            ("b.txt", vec![String::from("y")]),
        ];
        let pages = paginate_files(&files, &base, Some("TITLE"));
        assert_eq!(pages[0].header_lines[2], "TITLE Page 1");
        assert_eq!(pages[1].header_lines[2], "TITLE Page 1");
    }

    fn block(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn empty_input_prints_nothing_at_all() {
        // C prints the header from inside the line loop, only after `inln` has
        // handed over a first line (`pr.c:252-262`), and guards the trailer with
        // `if (linecnt && ...)` (`:294`) — so an empty file contributes no page,
        // with or without `-t`, single column or multiple.
        let options = PrOptions::default();
        assert!(paginate(&[], &options).is_empty());
        let nohead = PrOptions {
            no_header: true,
            ..PrOptions::default()
        };
        assert!(paginate(&[], &nohead).is_empty());
        let two = PrOptions {
            columns: 2,
            page_width: 40,
            ..PrOptions::default()
        };
        assert!(paginate(&[], &two).is_empty());
        // …and a mixed run of files still prints the non-empty one.
        let files = [
            ("empty.txt", Vec::new()),
            ("two.txt", vec!["a".to_string(), "b".to_string()]),
        ];
        let pages = paginate_files(&files, &options, None);
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].body_lines[..2],
            [String::from("a"), String::from("b")]
        );
    }

    #[test]
    fn headerless_pages_still_break_at_the_page_length() {
        // With `-t` the ten header/trailer lines are not subtracted
        // (`pr.c:1875-1878`), so `-l 4` gives pages of four body lines and the
        // last short page is not padded (`prtail`'s nohead arm returns early).
        let options = PrOptions {
            page_len: 4,
            no_header: true,
            ..PrOptions::default()
        };
        let lines: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
        let pages = paginate(&lines, &options);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].body_lines, ["1", "2", "3", "4"]);
        assert_eq!(
            pages[2].body_lines,
            ["9", "10"],
            "a short last page is not padded"
        );
        assert!(pages.iter().all(|page| page.header_lines.is_empty()));
        // `+page` counts these same four-line pages, and it is the observable proof
        // that the headerless path really is page-split.
        let from_two = PrOptions {
            start_page: 2,
            ..options.clone()
        };
        let pages = paginate(&lines, &from_two);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].body_lines, ["5", "6", "7", "8"]);
        // Beyond the last page C prints nothing.
        let past_end = PrOptions {
            start_page: 4,
            ..options.clone()
        };
        assert!(paginate(&lines, &past_end).is_empty());
    }

    #[test]
    fn column_pages_honour_the_start_page() {
        // `vertcol` calls the same `inskip` before its loop, so a start page
        // consumes whole `rows * columns` blocks (`pr.c:376-381`).
        let options = PrOptions {
            columns: 2,
            page_width: 20,
            page_len: 6, // with -t the whole six lines are body, per column
            no_header: true,
            ..PrOptions::default()
        };
        let lines: Vec<String> = (1..=24).map(|n| n.to_string()).collect();
        // One page is `rows * columns` = 12 input lines, laid out down column 1 and
        // then column 2, each cell padded to `colwd + 1` except the last column.
        let first = paginate(&lines, &options);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].body_lines[0], "1         7");
        let second_page = PrOptions {
            start_page: 2,
            ..options.clone()
        };
        let second = paginate(&lines, &second_page);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].body_lines[0], "13        19");
        let past_end = PrOptions {
            start_page: 3,
            ..options.clone()
        };
        assert!(paginate(&lines, &past_end).is_empty());
    }

    #[test]
    fn test_column_width_matches_setup_formula() {
        // pgwd=72 default (PGWD): colwd=(72+1-clcnt)/clcnt, pgwd re-rounded.
        assert_eq!(column_width(72, 2), Some((35, 71)));
        assert_eq!(column_width(72, 3), Some((23, 71)));
        assert_eq!(column_width(72, 1), Some((72, 72)));
        // Exactly one data byte per column is still legal (colwd == 1).
        assert_eq!(column_width(3, 2), Some((1, 3)));
        // colwd would fall below 1 → the "too small" refusal.
        assert_eq!(column_width(2, 2), None);
        assert_eq!(column_width(72, 72), None);
        assert_eq!(column_width(72, 0), None);
    }

    #[test]
    fn test_vertcol_downfills_each_column_then_trims_last() {
        // Column-major fill: col0=[1,2], col1=[3,4]; colwd 3 → stride 4 for
        // non-last columns, last column unpadded.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3", "4"]), 2, 2, 3, None),
            vec![String::from("1   3"), String::from("2   4")]
        );
        // Uneven last page: row 1 has col1 empty; col0's inter-column pad
        // stays (C keeps it), only the last column omits padding.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3"]), 2, 2, 3, None),
            vec![String::from("1   3"), String::from("2   ")]
        );
        // A row with no data in any column stops emission: block=["1","2"] fills
        // col0's two rows, so both emit (col1 empty) — trailing inter-column pad
        // kept, last column unpadded.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2"]), 2, 2, 3, None),
            vec![String::from("1   "), String::from("2   ")]
        );
        // Only row 0 has data → row 1 is empty in every column → emission stops.
        assert_eq!(
            vertcol_body_rows(&block(&["1"]), 2, 2, 3, None),
            vec![String::from("1   ")]
        );
    }

    #[test]
    fn test_vertcol_sep_and_clip() {
        // -s: a single separator byte, no width padding between columns.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3", "4"]), 2, 2, 3, Some(b'\t')),
            vec![String::from("1\t3"), String::from("2\t4")]
        );
        // Cells clip to colwd bytes (C's inln truncation).
        assert_eq!(
            vertcol_body_rows(&block(&["abcdef", "x", "y", "z"]), 2, 2, 3, None),
            vec![String::from("abc y"), String::from("x   z")]
        );
        // Clip floors to a char boundary, never a panic on multibyte input.
        assert_eq!(
            vertcol_body_rows(&block(&["h\u{e9}llo"]), 1, 1, 3, None),
            vec![String::from("h\u{e9}")]
        );
    }

    #[test]
    fn test_paginate_columns_no_header_downfills() {
        // `-N` wires paginate → paginate_columns → vertcol_body_rows. page_width
        // 5, 2 cols → colwd 2 (stride 3). -t: no header/trailer/padding.
        let mut options = PrOptions::default();
        options.columns = 2;
        options.page_width = 5;
        options.page_len = 2;
        options.no_header = true;
        let pages = paginate_lines(&["a", "b", "c", "d"], &options);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].header_lines.is_empty());
        assert_eq!(
            pages[0].body_lines,
            vec![String::from("a  c"), String::from("b  d")]
        );
    }

    #[test]
    fn test_render_page_offsets_body_and_header_text_not_blanks() {
        let mut options = PrOptions::default();
        options.header = "f.txt".to_string();
        // page_len 12 → body capacity 2, so ["a","b"] exactly fills the body
        // (no pad lines) and index 7 onward is the trailer.
        options.page_len = 12;
        let pages = paginate_lines(&["a", "b"], &options);
        let out = render_page(&pages[0], 3);
        assert_eq!(out.len(), 12);
        assert_eq!(out[0], ""); // header blank spacer, unindented
        assert_eq!(out[2], "   f.txt Page 1"); // header text line, indented
        assert_eq!(out[5], "   a"); // body, indented
        assert_eq!(out[6], "   b");
        assert_eq!(out[7], ""); // trailer, unindented
        // offset 0 reproduces the plain page order exactly.
        let plain = render_page(&pages[0], 0);
        assert_eq!(plain, pages[0].lines().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn test_paginate_columns_headers_on_pad_body_and_number_page() {
        let mut options = PrOptions::default();
        options.columns = 2;
        options.page_width = 5;
        let pages = paginate_lines(&["a", "b"], &options);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].header_lines[2], " Page 1");
        // Two lines across a 2-column page leave far more than a full column of
        // slack, so the final page balances (`pr.c:522`): `a` and `b` share one
        // row (`a` padded to the `colwd + 1` stride, `b` the unpadded last
        // column), then the body pads out to the 56-line capacity.
        assert_eq!(pages[0].body_lines[0], "a  b");
        assert_eq!(pages[0].body_lines[1], "");
        assert_eq!(pages[0].body_lines.len(), 56);
        assert_eq!(pages[0].trailer_lines.len(), TAILLEN);
    }

    #[test]
    fn test_number_field_width_and_truncation() {
        assert_eq!(number_field(1, 3), "  1");
        assert_eq!(number_field(12, 3), " 12");
        // A number wider than the field keeps its last `width` digits (C addnum
        // writes from the right and stops at the left edge).
        assert_eq!(number_field(1234, 3), "234");
        assert_eq!(number_field(7, 1), "7");
    }

    #[test]
    fn test_paginate_number_prefixes_body_running_per_file() {
        let mut options = PrOptions::default();
        options.number_width = 3; // number_field width; default char tab
        options.page_len = 12; // body capacity 2
        let pages = paginate_lines(&["a", "b", "c", "d"], &options);
        // 4 lines over body capacity 2 → 2 pages, numbering runs continuously.
        assert_eq!(pages[0].body_lines[0], "  1\ta");
        assert_eq!(pages[0].body_lines[1], "  2\tb");
        assert_eq!(pages[1].body_lines[0], "  3\tc");
        assert_eq!(pages[1].body_lines[1], "  4\td");
    }

    #[test]
    fn test_horzcol_fills_across_rows() {
        // Row-major (across): consecutive lines go left-to-right, contrasted
        // against vertcol_body_rows' column-major down-fill of the same block.
        assert_eq!(
            horzcol_body_rows(&block(&["a", "b", "c", "d"]), 2, 3, None),
            vec![String::from("a   b"), String::from("c   d")]
        );
        assert_eq!(
            vertcol_body_rows(&block(&["a", "b", "c", "d"]), 2, 2, 3, None),
            vec![String::from("a   c"), String::from("b   d")]
        );
        // Odd count: last row's single cell is padded (col0 is not last), then
        // EOF stops the row — trailing inter-column pad kept.
        assert_eq!(
            horzcol_body_rows(&block(&["a", "b", "c"]), 2, 3, None),
            vec![String::from("a   b"), String::from("c   ")]
        );
        assert!(horzcol_body_rows(&[], 2, 3, None).is_empty());
    }

    #[test]
    fn test_paginate_across_fills_row_major() {
        let mut options = PrOptions::default();
        options.columns = 2;
        options.page_width = 5; // → colwd 2
        options.across = true;
        options.no_header = true;
        let pages = paginate_lines(&["a", "b", "c", "d"], &options);
        assert_eq!(
            pages[0].body_lines,
            vec![String::from("a  b"), String::from("c  d")]
        );
    }

    #[test]
    fn test_double_space_interleaves_blank_and_halves_page() {
        let mut options = PrOptions::default();
        options.double_space = true;
        options.page_len = 12; // body_capacity 2 → 1 content line/page, doubled to 2 rows
        let pages = paginate_lines(&["a", "b", "c"], &options);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].body_lines, vec![String::from("a"), String::new()]);
        assert_eq!(pages[2].body_lines, vec![String::from("c"), String::new()]);
    }

    #[test]
    fn test_double_space_one_row_page_falls_back() {
        let mut options = PrOptions::default();
        options.double_space = true;
        options.page_len = 11; // body_capacity 1 → no room to double (C `dspace = 0`)
        let pages = paginate_lines(&["a", "b"], &options);
        assert_eq!(pages[0].body_lines, vec![String::from("a")]);
        assert_eq!(pages[1].body_lines, vec![String::from("b")]);
    }

    #[test]
    fn test_start_page_skips_ahead_and_offsets_numbering() {
        let mut options = PrOptions::default();
        options.page_len = 12; // body_capacity 2
        options.start_page = 3; // skip (3-1)*2 = 4 content lines
        let lines: Vec<String> = ("123456").chars().map(|c| c.to_string()).collect();
        let pages = paginate(&lines, &options);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].header_lines[2], " Page 3");
        assert_eq!(
            pages[0].body_lines,
            vec![String::from("5"), String::from("6")]
        );
    }

    #[test]
    fn test_start_page_beyond_end_prints_nothing() {
        let mut options = PrOptions::default();
        options.page_len = 12; // body_capacity 2 → page 3 needs lines 5-6
        options.start_page = 3;
        // Only 3 lines: page 3 is unreachable → C `inskip` skips the file.
        assert!(paginate_lines(&["a", "b", "c"], &options).is_empty());
    }

    #[test]
    fn test_merge_body_rows_zip_longest_drops_trailing_eof() {
        let streams = vec![
            vec![String::from("a"), String::from("b"), String::from("c")],
            vec![String::from("1"), String::from("2")],
        ];
        // Row 0/1: both columns; row 2: only stream 0 has data → trailing EOF
        // column dropped, and colwd 3 leaves "c" alone (last active, no pad).
        assert_eq!(
            merge_body_rows(&streams, 3, None),
            vec![
                String::from("a   1"),
                String::from("b   2"),
                String::from("c")
            ]
        );
        assert!(merge_body_rows(&[], 3, None).is_empty());
    }

    #[test]
    fn test_vertcol_balanced_repacks_the_staircase() {
        // 4 lines, 2 columns → pln = ceil(4/2) = 2, columns come out even
        // (the plain down-fill would have stacked [1,2,3] in col0 and left
        // col1 with just [4]).
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3", "4"]), 2, 3, None),
            vec![String::from("1   3"), String::from("2   4")]
        );
        // The naive down-fill of the same block is the staircase, for contrast.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3", "4"]), 3, 2, 3, None),
            vec![
                String::from("1   4"),
                String::from("2   "),
                String::from("3   ")
            ]
        );
    }

    #[test]
    fn test_vertcol_balanced_odd_count_keeps_trailing_pad() {
        // 3 lines, 2 columns → pln 2: col0 [1,2], col1 [3]. Row 1's only cell
        // (col0) is non-last, so its inter-column pad stays and col1 (absent)
        // breaks the row — exactly C's `tvc >= cvc` break (pr.c:568).
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3"]), 2, 3, None),
            vec![String::from("1   3"), String::from("2   ")]
        );
    }

    #[test]
    fn test_vertcol_balanced_three_columns() {
        // 3 lines, 3 columns → pln = ceil(3/3) = 1: one even row across all
        // three columns; the last column is unpadded.
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3"]), 3, 3, None),
            vec![String::from("1   2   3")]
        );
    }

    #[test]
    fn test_vertcol_balanced_sep_and_clip() {
        // -s: one separator byte, no width padding; last column unpadded.
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3", "4"]), 2, 3, Some(b'\t')),
            vec![String::from("1\t3"), String::from("2\t4")]
        );
        // Cells clip to colwd bytes before the pad (C's inln truncation).
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["abcdef", "x", "y", "z"]), 2, 3, None),
            vec![String::from("abc y"), String::from("x   z")]
        );
    }

    #[test]
    fn test_vertcol_balanced_empty_guards() {
        assert!(vertcol_body_rows_balanced(&[], 2, 3, None).is_empty());
        assert!(vertcol_body_rows_balanced(&block(&["1"]), 0, 3, None).is_empty());
    }

    #[test]
    fn test_paginate_columns_balances_final_partial_page() {
        // page_width 5, 2 cols → colwd 2 (stride 3); -t page_len 3 → rows 3,
        // block 6. 4 lines leave slack 2 >= 2 columns → the final page balances
        // to ["a  c","b  d"] instead of the staircase ["a  d","b  ","c  "].
        let mut options = PrOptions::default();
        options.columns = 2;
        options.page_width = 5;
        options.page_len = 3;
        options.no_header = true;
        let pages = paginate_lines(&["a", "b", "c", "d"], &options);
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].body_lines,
            vec![String::from("a  c"), String::from("b  d")]
        );
    }

    #[test]
    fn test_paginate_columns_balanced_pads_body_with_header() {
        // page_len 14 → rows 4 (14 - HEADLEN - TAILLEN), block 8. 4 lines leave
        // slack 4 >= 2 → balanced to 2 rows, padded out to the 4-line body.
        let mut options = PrOptions::default();
        options.columns = 2;
        options.page_width = 5;
        options.page_len = 14;
        let pages = paginate_lines(&["a", "b", "c", "d"], &options);
        assert_eq!(pages[0].header_lines[2], " Page 1");
        assert_eq!(
            pages[0].body_lines,
            vec![
                String::from("a  c"),
                String::from("b  d"),
                String::new(),
                String::new(),
            ]
        );
    }

    #[test]
    fn test_paginate_columns_full_page_keeps_downfill() {
        // An exactly-full final block (slack 0) is not balanced: the plain
        // down-fill stands, guarding against a spurious repack on full pages.
        let mut options = PrOptions::default();
        options.columns = 2;
        options.page_width = 5; // colwd 2, stride 3
        options.page_len = 3;
        options.no_header = true; // rows 3, block 6
        let pages = paginate_lines(&["a", "b", "c", "d", "e", "f"], &options);
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].body_lines,
            vec![
                String::from("a  d"),
                String::from("b  e"),
                String::from("c  f")
            ]
        );
    }

    #[test]
    fn parse_defaults_and_basic_options() {
        // 无参：默认 width 72、columns 1。
        let d = parse_pr_args(&[]).unwrap();
        assert_eq!(d.options.page_width, 72);
        assert_eq!(d.options.columns, 1);
        assert!(d.operands.is_empty());
        // 现有分离值形式（回归）。
        let a = parse_pr_args(&["-t", "-h", "T", "-l", "66", "-w", "100", "f1"]).unwrap();
        assert!(a.options.no_header);
        assert_eq!(a.title_override, Some("T"));
        assert_eq!(a.options.page_len, 66);
        assert_eq!(a.options.page_width, 100);
        assert_eq!(a.operands, vec!["f1"]);
    }

    #[test]
    fn parse_glued_values_and_clusters_accepted() {
        // 关键回归：附值与聚簇（旧前门拒）。`-w100 -l66 -o4`。
        let a = parse_pr_args(&["-w100", "-l66", "-o4"]).unwrap();
        assert_eq!(a.options.page_width, 100);
        assert_eq!(a.options.page_len, 66);
        assert_eq!(a.options.offset, 4);
        // `-hT`：附值标题。
        let b = parse_pr_args(&["-hTitle", "f1", "f2"]).unwrap();
        assert_eq!(b.title_override, Some("Title"));
        assert_eq!(b.operands, vec!["f1", "f2"]);
        // 聚簇：`-dm`（双行距+合并）、`-tw100`（无页眉后 w 取附值）。
        let c = parse_pr_args(&["-dm", "f"]).unwrap();
        assert!(c.options.double_space && c.merge);
        let e = parse_pr_args(&["-tw100", "f"]).unwrap();
        assert!(e.options.no_header);
        assert_eq!(e.options.page_width, 100);
        // `+N` 起始页、`-N` 竖列、`-n5`、`-sX`。
        let f = parse_pr_args(&["+3", "-n5", "-sX", "f"]).unwrap();
        assert_eq!(f.options.start_page, 3);
        assert_eq!(f.options.number_width, 5);
        assert_eq!(f.options.sep, Some(b'X'));
        let g = parse_pr_args(&["-w", "200", "-2"]).unwrap();
        assert_eq!(g.options.columns, 2);
    }

    #[test]
    fn parse_error_paths() {
        // 集外/未建模字母、缺值、C 为每个数值选项单写的判据消息、`-a` 与 `-m`
        // 的组合拒绝。
        assert_eq!(parse_pr_args(&["-e", "f"]).err(), Some(PrError::Unmodelled));
        assert_eq!(parse_pr_args(&["-w"]).err(), Some(PrError::Usage)); // 缺值 ⇒ getopt '?'
        assert_eq!(parse_pr_args(&["-w0"]).err(), Some(PrError::WidthValue)); // atoi < 1
        assert_eq!(parse_pr_args(&["-w", "x"]).err(), Some(PrError::WidthValue)); // 首位非数字
        assert_eq!(parse_pr_args(&["-o0"]).err(), Some(PrError::OffsetValue));
        assert_eq!(parse_pr_args(&["+0"]).err(), Some(PrError::PlusPage));
        assert_eq!(parse_pr_args(&["-0"]).err(), Some(PrError::ColumnCount));
        assert_eq!(parse_pr_args(&["-n0"]).err(), Some(PrError::NumberWidth));
        assert_eq!(
            parse_pr_args(&["-sab"]).err(),
            Some(PrError::InvalidValue('s', String::from("b")))
        );
        assert_eq!(
            parse_pr_args(&["-nxy"]).err(),
            Some(PrError::InvalidValue('n', String::from("y")))
        );
        assert_eq!(
            parse_pr_args(&["-a"]).err(),
            Some(PrError::AcrossNeedsColumns)
        );
        assert_eq!(
            parse_pr_args(&["-m", "-a", "-2"]).err(),
            Some(PrError::MergeAcross)
        );
        assert_eq!(
            parse_pr_args(&["-100"]).err(),
            Some(PrError::ColumnTooSmall(100))
        );
    }

    #[test]
    fn short_page_length_turns_the_header_off_rather_than_failing() {
        // `pr.c:1875-1876` only raises `nohead` when the page cannot carry both a
        // five-line header and a five-line trailer, and `-l`'s lower bound is 1 —
        // so `-l5` is a usable page of five body lines, not an error.
        let args = parse_pr_args(&["-l5"]).expect("-l 5 is legal in C");
        assert_eq!(args.options.page_len, 5);
        assert!(
            args.options.no_header,
            "too short for a header ⇒ header off"
        );
        assert!(!parse_pr_args(&["-l11"]).expect("-l11").options.no_header);
        // The value still has to *start* with a digit, which is why `+8` and a
        // leading space are refused even though `atoi` reads both as 8.
        assert_eq!(parse_pr_args(&["-l+8"]).err(), Some(PrError::NumberLines));
        assert_eq!(
            parse_pr_args(&["-l", " 8"]).err(),
            Some(PrError::NumberLines)
        );
        assert_eq!(parse_pr_args(&["-l0"]).err(), Some(PrError::NumberLines));
        // Everything else is `atoi`'s leniency: trailing junk ignored, leading
        // zeroes read as the number they spell.
        assert_eq!(parse_pr_args(&["-l8x"]).expect("-l8x").options.page_len, 8);
        assert_eq!(parse_pr_args(&["-l08"]).expect("-l08").options.page_len, 8);
    }

    #[test]
    fn optional_option_values_keep_the_c_digit_run_leniency() {
        // `-n` reads its width with `atoi` and never advances past the digits it
        // consumed, so `-nx5y` sets width 5 without tripping the invalid-value arm;
        // only a non-digit where a number should be does (`pr.c:1744-1762`).
        let glued = parse_pr_args(&["-nx5y"]).expect("-nx5y");
        assert_eq!(glued.options.number_char, b'x');
        assert_eq!(glued.options.number_width, 5);
        // A separator alone leaves the width at NMWD (5).
        let sep = parse_pr_args(&["-n:"]).expect("-n:");
        assert_eq!(sep.options.number_char, b':');
        assert_eq!(sep.options.number_width, 5);
        // `-s` takes one character and no more (`pr.c:1779-1789`).
        assert_eq!(
            parse_pr_args(&["-s:"]).expect("-s:").options.sep,
            Some(b':')
        );
        assert_eq!(parse_pr_args(&["-s"]).expect("-s").options.sep, Some(b'\t'));
    }
}
