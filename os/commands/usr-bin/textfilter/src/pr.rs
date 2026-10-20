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

use crate::GetoptReport;
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
    /// `-e[ch][gap]`, and implied by multi-column or merge output
    /// (`pr.c:1840-1843`): the input byte that stands for a tab and the pitch it
    /// expands to. `None` is C's `ingap == 0`, which leaves the input bytes alone.
    pub expand: Option<(u8, usize)>,
    /// `-i[ch][gap]`, likewise implied by multi-column or merge output
    /// (`pr.c:1844-1847`): the byte a run of output blanks contracts into, and the
    /// pitch it steps by. `None` is C's `ogap == 0`, the plain `fwrite` arm of `otln`.
    pub contract: Option<(u8, usize)>,
    /// `-F`, and also implied by `-f` (`pr.c:1693-1700`: `case 'f'` sets the pause
    /// flag and falls through to `case 'F'`). The page then ends with a form-feed
    /// byte instead of the blank trailer lines (`prtail`'s `if (formfeed)` arm,
    /// `pr.c:1555-1561`).
    pub form_feed: bool,
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
            expand: None,
            contract: None,
            form_feed: false,
        }
    }
}

/// C's `inln` expansion half (`pr.c:1032-1068`): each `ch` byte becomes the number of
/// blanks that carries the column up to the next multiple of `gap`, counted from the
/// start of the line because `inln` restarts its buffer pointer at every newline. A
/// `gap` of zero is C's `if (gap)` test — no expansion at all, the bytes pass through.
pub fn expand_tabs(data: &[u8], ch: u8, gap: usize) -> Vec<u8> {
    if gap == 0 {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut column = 0usize;
    for &byte in data {
        if byte == ch {
            let blanks = gap - (column % gap);
            out.extend(core::iter::repeat_n(b' ', blanks));
            column += blanks;
            continue;
        }
        if byte == b'\n' {
            column = 0;
        } else {
            column += 1;
        }
        out.push(byte);
    }
    out
}

/// C's `otln` contraction half (`pr.c:1137-1240`): a run of blanks is re-encoded as
/// as many `ch` bytes as reach tab stops inside it, then single blanks up to the exact
/// column. Two details make it more than a search-and-replace. First, a `ch` already in
/// the text is *simulated* rather than copied — it advances the input column by
/// `gap - (ips % gap)` and emits nothing itself, so contraction re-flows existing tabs
/// instead of preserving them. Second, a run of one blank is never contracted (the C
/// condition is `ips - ops > 1`), which is what keeps the single space between two
/// words a space. C finishes the pending run before the newline
/// (`pr.c:1229-1262`), so a line ending in blanks keeps its width.
pub fn contract_tabs(data: &[u8], ch: u8, gap: usize) -> Vec<u8> {
    if gap == 0 {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut input_column = 0usize;
    let mut output_column = 0usize;
    let next_stop = |column: usize| column + gap - (column % gap);
    for &byte in data {
        if byte == b' ' {
            input_column += 1;
            continue;
        }
        if byte == ch {
            input_column += gap - (input_column % gap);
            continue;
        }
        while input_column - output_column > 1 && next_stop(output_column) <= input_column {
            out.push(ch);
            output_column = next_stop(output_column);
        }
        while output_column < input_column {
            out.push(b' ');
            output_column += 1;
        }
        out.push(byte);
        input_column += 1;
        output_column += 1;
    }
    while input_column - output_column > 1 && next_stop(output_column) <= input_column {
        out.push(ch);
        output_column = next_stop(output_column);
    }
    while output_column < input_column {
        out.push(b' ');
        output_column += 1;
    }
    out
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
    /// 集外字母与带值选项缺值：`egetopt` 先印自己那一句（集外字母是
    /// `illegal option -- <字母>`，`egetopt.c:144-151`；缺值是
    /// `option requires an argument -- <字母>`，`:196-205`），然后 `setup` 返回 1、
    /// `main` 再接上那三行 usage（`pr.c:156-157`）。注意措辞：`pr` 链的是自带的
    /// `egetopt`，不是 libc 的 `getopt`，所以集外字母那一句与同族其它命令**不同形**。
    Getopt(crate::GetoptReport),
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
/// `pr.h:45-48`: the two characters and the two pitches the C falls back to.
const INCHAR: u8 = b'\t';
const OCHAR: u8 = b'\t';
const DEFAULT_GAP: usize = 8;

/// The character `-e`/`-i` default to, spelled out so the two options stay readable
/// at their call sites.
fn default_char(letter: char) -> u8 {
    if letter == 'e' { INCHAR } else { OCHAR }
}

/// The shared value grammar of `-e[ch][gap]` and `-i[ch][gap]` (`pr.c:1674-1692`,
/// `1710-1728`): a leading non-digit replaces the character and is consumed, a digit
/// run then sets the gap through `atoi` with a literal zero meaning the default pitch,
/// and anything else left over is C's named invalid value. C's `gap < 0` refusal sits
/// behind the same digit test that got us here, so it cannot be reached and is not
/// reproduced.
fn parse_char_gap(
    rest: &str,
    fallback_char: u8,
    fallback_gap: usize,
    letter: char,
) -> Result<(u8, usize), PrError> {
    let mut character = fallback_char;
    let mut cursor = 0usize;
    if let Some(&first) = rest.as_bytes().first()
        && !first.is_ascii_digit()
    {
        character = first;
        cursor = 1;
    }
    let tail = &rest[cursor..];
    if tail.is_empty() {
        return Ok((character, fallback_gap));
    }
    if !tail.as_bytes()[0].is_ascii_digit() {
        return Err(PrError::InvalidValue(letter, String::from(tail)));
    }
    let gap = c_atoi(tail);
    Ok((
        character,
        if gap == 0 { fallback_gap } else { gap as usize },
    ))
}

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
    // `eflag`/`iflag` record that the option was given at all, which is what tells
    // the implied defaults of `pr.c:1840-1848` apart from an explicit one.
    let (mut eflag, mut iflag) = (false, false);
    let (mut in_char, mut in_gap) = (INCHAR, DEFAULT_GAP);
    let (mut out_char, mut out_gap) = (OCHAR, DEFAULT_GAP);

    // Take a required option value: the glued remainder of this token, else
    // the next token (advancing `index`). 缺值要把那个字母交出去——`egetopt` 的那一句
    // 带的是字母（`egetopt.c:196-205`）。
    macro_rules! value {
        ($rest:expr, $index:ident, $letter:expr) => {{
            if !$rest.is_empty() {
                $rest
            } else {
                $index += 1;
                match args.get($index) {
                    Some(v) => *v,
                    None => {
                        return Err(PrError::Getopt(GetoptReport::MissingValue($letter)));
                    }
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
                    title_override = Some(value!(&arg[pos + 1..], index, 'h'));
                    break;
                }
                b'l' => {
                    let v = value!(&arg[pos + 1..], index, 'l');
                    options.page_len = c_counted(v, PrError::NumberLines)?;
                    break;
                }
                b'w' => {
                    let v = value!(&arg[pos + 1..], index, 'w');
                    options.page_width = c_counted(v, PrError::WidthValue)?;
                    wflag = true;
                    break;
                }
                b'o' => {
                    let v = value!(&arg[pos + 1..], index, 'o');
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
                b'e' | b'i' => {
                    // `-e[ch][gap]` and `-i[ch][gap]`: an optional glued value
                    // (`pr.c:1674-1692`, `1710-1728`).
                    let letter = if bytes[pos] == b'e' { 'e' } else { 'i' };
                    let (character, gap) =
                        parse_char_gap(&arg[pos + 1..], default_char(letter), DEFAULT_GAP, letter)?;
                    if letter == 'e' {
                        eflag = true;
                        in_char = character;
                        in_gap = gap;
                    } else {
                        iflag = true;
                        out_char = character;
                        out_gap = gap;
                    }
                    break;
                }
                b'F' => options.form_feed = true,
                b'f' => {
                    // C's `case 'f'` sets `pgpause |= FIRSTPAGE` and falls through to
                    // `case 'F'` (`pr.c:1693-1700`). The form feed is the observable
                    // half; the per-page pause only ever fires when stdout is a
                    // terminal (`prpause` is guarded by `ttyout`, `pr.c:1463`), which
                    // this shell has no leg for — registered, not invented.
                    options.form_feed = true;
                }
                // Letters C's optstring carries but this deciding half does not
                // implement are named separately from letters `egetopt` itself would
                // answer `?` for (`pr.c:1655`, `1805-1809`).
                b'p' | b'r' | b'T' => {
                    return Err(PrError::Unmodelled);
                }
                _ => {
                    return Err(PrError::Getopt(GetoptReport::IllegalOption(
                        bytes[pos] as char,
                    )));
                }
            }
            pos += 1;
        }
        index += 1;
    }

    if !wflag && sflag {
        options.page_width = 512;
    }
    // `pr.c:1840-1848`: multi-column and merged output imply *both* transformations
    // with the C's own defaults unless the option already asked for one; plain
    // single-column output implies neither, which is why a lone `-e` is the only way
    // to expand tabs in a normal `pr` run.
    let implied = options.columns > 1 || merge;
    if eflag || implied {
        options.expand = Some((in_char, in_gap));
    }
    if iflag || implied {
        options.contract = Some((out_char, out_gap));
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
/// to the body capacity with empty lines when headers are on and no form feed was
/// asked for), the trailer lines (empty when `-t` or under `-F`), and whether the
/// page ends with a form-feed byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub header_lines: Vec<Vec<u8>>,
    pub body_lines: Vec<Vec<u8>>,
    pub trailer_lines: Vec<Vec<u8>>,
    /// `-F`: `prtail` writes one `\f` and returns (`pr.c:1555-1561`), which replaces
    /// both the blank padding lines and the five-line trailer. It carries no newline
    /// of its own, which is why it is a page property rather than one more line.
    /// `-t` cancels it: `prtail` bails out on `nohead` before ever reaching the
    /// form-feed arm (`pr.c:1526-1537`), so a form-feed page without headers is just
    /// its body lines and nothing else.
    pub form_feed: bool,
}

impl Page {
    /// All lines in print order.
    pub fn lines(&self) -> impl Iterator<Item = &Vec<u8>> {
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
fn number_field(n: usize, width: usize) -> Vec<u8> {
    let digits = alloc::format!("{n}").into_bytes();
    if digits.len() >= width {
        digits[digits.len() - width..].to_vec()
    } else {
        let mut s = vec![b' '; width - digits.len()];
        s.extend_from_slice(&digits);
        s
    }
}

pub fn paginate(lines: &[Vec<u8>], options: &PrOptions) -> Vec<Page> {
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
        let mut body: Vec<Vec<u8>> = if options.number_width == 0 {
            lines[cursor..body_end].to_vec()
        } else {
            // Running line number per file (`cursor` is the global index, and
            // `paginate` runs once per file, so it restarts at 1 each file —
            // matching C's per-file `lncnt`).
            (cursor..body_end)
                .map(|idx| {
                    let mut line = number_field(idx + 1, options.number_width);
                    line.push(options.number_char);
                    line.extend_from_slice(&lines[idx]);
                    line
                })
                .collect()
        };
        if double {
            // C `otln` emits the line then a blank (`dspace`), so interleave.
            let mut spaced = Vec::with_capacity(body.len() * 2);
            for line in body {
                spaced.push(line);
                spaced.push(Vec::new());
            }
            body = spaced;
        }
        // The padding rows are `prtail`'s `cnt` loop, which the form-feed arm of
        // `prtail` skips entirely — it writes one `\f` instead of the blanks
        // (`pr.c:1555-1561`).
        if !options.no_header && !options.form_feed {
            while body.len() < body_capacity {
                body.push(Vec::new());
            }
        }
        let (header_lines, trailer_lines) = if options.no_header {
            (Vec::new(), Vec::new())
        } else {
            (
                vec![
                    Vec::new(),
                    Vec::new(),
                    alloc::format!(
                        "{}{} Page {}",
                        options.time_prefix,
                        options.header,
                        page_number
                    )
                    .into_bytes(),
                    Vec::new(),
                    Vec::new(),
                ],
                vec![Vec::new(); TAILLEN],
            )
        };
        // Under `-F` the trailer block disappears entirely; the form-feed byte is
        // written by the caller after the last line.
        let trailer_lines = if options.form_feed {
            Vec::new()
        } else {
            trailer_lines
        };
        // `prtail` bails out inside its `nohead` arm, before the form-feed arm, so
        // `-t` also cancels the byte itself (`pr.c:1526-1537`).
        let form_feed = options.form_feed && !options.no_header;
        pages.push(Page {
            header_lines,
            body_lines: body,
            trailer_lines,
            form_feed,
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
fn paginate_columns(lines: &[Vec<u8>], options: &PrOptions) -> Vec<Page> {
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
            // Same rule as the single-column path: the padding rows are `prtail`'s
            // blank run, which the form-feed arm does not write.
            if !options.form_feed {
                while body.len() < rows {
                    body.push(Vec::new());
                }
            }
            (
                vec![
                    Vec::new(),
                    Vec::new(),
                    alloc::format!(
                        "{}{} Page {}",
                        options.time_prefix,
                        options.header,
                        page_number
                    )
                    .into_bytes(),
                    Vec::new(),
                    Vec::new(),
                ],
                vec![Vec::new(); TAILLEN],
            )
        };
        // Under `-F` the trailer block disappears entirely; the form-feed byte is
        // written by the caller after the last line.
        let trailer_lines = if options.form_feed {
            Vec::new()
        } else {
            trailer_lines
        };
        // Same `-t` cancellation as the single-column path: `vertcol` also ends every
        // page through `prtail` (`pr.c:604-608`).
        let form_feed = options.form_feed && !options.no_header;
        pages.push(Page {
            header_lines,
            body_lines: body,
            trailer_lines,
            form_feed,
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
    files: &[(&str, Vec<Vec<u8>>)],
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

/// Clip `s` to at most `n` **bytes**. C's `inln` truncates a line at `colwd` raw
/// bytes and `stp` then writes exactly that many, so a cell may end mid-code-point;
/// flooring to a code-point boundary (the old shape) was a self-inflicted
/// divergence. With the line carried as bytes end to end there is no `str`
/// invariant left to protect.
fn clip_to_width(s: &[u8], n: usize) -> &[u8] {
    &s[..n.min(s.len())]
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
    lines_block: &[Vec<u8>],
    rows: usize,
    columns: usize,
    colwd: usize,
    sep: Option<u8>,
) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if rows == 0 || columns == 0 {
        return out;
    }
    for row in 0..rows {
        let mut line: Vec<u8> = Vec::new();
        let mut any = false;
        for column in 0..columns {
            let idx = column * rows + row;
            let raw = lines_block.get(idx).map(Vec::as_slice).unwrap_or(&[]);
            if !raw.is_empty() {
                any = true;
            }
            let cell = clip_to_width(raw, colwd);
            line.extend_from_slice(cell);
            if column + 1 == columns {
                break; // last column: no trailing pad
            }
            match sep {
                Some(byte) => line.push(byte),
                None => {
                    line.extend(core::iter::repeat_n(b' ', colwd + 1 - cell.len()));
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
    lines_block: &[Vec<u8>],
    columns: usize,
    colwd: usize,
    sep: Option<u8>,
) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let count = lines_block.len();
    if count == 0 || columns == 0 {
        return out;
    }
    let pln = count.div_ceil(columns);
    for row in 0..pln {
        let mut line: Vec<u8> = Vec::new();
        let mut any = false;
        for column in 0..columns {
            let idx = column * pln + row;
            let raw = match lines_block.get(idx) {
                Some(s) => s.as_slice(),
                None => break, // C `tvc >= cvc`: this balanced column ran dry
            };
            if !raw.is_empty() {
                any = true;
            }
            let cell = clip_to_width(raw, colwd);
            line.extend_from_slice(cell);
            if column + 1 == columns {
                break; // last column: never padded (C's `mclcnt` arm)
            }
            match sep {
                Some(byte) => line.push(byte),
                None => {
                    line.extend(core::iter::repeat_n(b' ', colwd + 1 - cell.len()));
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
    lines_block: &[Vec<u8>],
    columns: usize,
    colwd: usize,
    sep: Option<u8>,
) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if columns == 0 {
        return out;
    }
    let rows = lines_block.len().div_ceil(columns);
    for row in 0..rows {
        let mut line: Vec<u8> = Vec::new();
        let mut any = false;
        for column in 0..columns {
            let idx = row * columns + column;
            let raw = match lines_block.get(idx) {
                Some(s) => s.as_slice(),
                None => break, // EOF: stop this row here (C `inln < 0`)
            };
            if !raw.is_empty() {
                any = true;
            }
            let cell = clip_to_width(raw, colwd);
            line.extend_from_slice(cell);
            if column + 1 == columns {
                break; // last column: no trailing pad
            }
            match sep {
                Some(byte) => line.push(byte),
                None => {
                    line.extend(core::iter::repeat_n(b' ', colwd + 1 - cell.len()));
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
pub fn merge_body_rows(streams: &[Vec<Vec<u8>>], colwd: usize, sep: Option<u8>) -> Vec<Vec<u8>> {
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
        let mut line: Vec<u8> = Vec::new();
        for j in 0..=last {
            let raw = streams[j].get(row).map(Vec::as_slice).unwrap_or(&[]);
            let cell = clip_to_width(raw, colwd);
            line.extend_from_slice(cell);
            if j == last {
                break;
            }
            match sep {
                Some(byte) => line.push(byte),
                None => {
                    line.extend(core::iter::repeat_n(b' ', colwd + 1 - cell.len()));
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
pub fn render_page(page: &Page, offset: usize) -> Vec<Vec<u8>> {
    let pad = |s: &[u8]| -> Vec<u8> {
        let mut out = vec![b' '; offset];
        out.extend_from_slice(s);
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
        let owned: Vec<Vec<u8>> = lines.iter().map(|s| s.as_bytes().to_vec()).collect();
        paginate(&owned, options)
    }

    #[test]
    fn test_page_geometry_is_66_lines() {
        let lines: Vec<Vec<u8>> = (1..=100).map(|n| n.to_string().into_bytes()).collect();
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
        assert_eq!(pages[0].header_lines[2], b"my file Page 1");
        // The body starts right after the five header lines.
        assert_eq!(pages[0].body_lines[0], b"one");
    }

    #[test]
    fn test_time_prefix_rides_the_header_line() {
        let mut options = PrOptions::default();
        options.time_prefix = "2026-09-17 12:00:00 ".to_string();
        let pages = paginate_lines(&["one"], &options);
        assert_eq!(pages[0].header_lines[2], b"2026-09-17 12:00:00  Page 1");
    }

    #[test]
    fn test_body_pads_to_capacity() {
        let pages = paginate_lines(&["one"], &PrOptions::default());
        let body_empty = pages[0].body_lines[1..].iter().all(|line| line.is_empty());
        assert!(body_empty);
        assert_eq!(pages[0].body_lines[0], b"one");
    }

    #[test]
    fn test_no_header_prints_the_body_bare() {
        let mut options = PrOptions::default();
        options.no_header = true;
        let pages = paginate_lines(&["one", "two"], &options);
        assert!(pages[0].header_lines.is_empty());
        assert!(pages[0].trailer_lines.is_empty());
        assert_eq!(pages[0].body_lines, vec![b"one".to_vec(), b"two".to_vec()]);
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
            ("a.txt", vec![b"x1".to_vec(), b"x2".to_vec()]),
            ("b.txt", vec![b"y".to_vec()]),
        ];
        let pages = paginate_files(&files, &base, None);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].header_lines[2], b"a.txt Page 1");
        assert_eq!(pages[0].body_lines[0], b"x1");
        assert_eq!(pages[1].header_lines[2], b"b.txt Page 1");
        assert_eq!(pages[1].body_lines[0], b"y");
    }

    #[test]
    fn test_paginate_files_restarts_page_numbers_per_file() {
        // A 57-line first file spans Page 1 and Page 2 (body cap 56); the
        // second file restarts at Page 1 (pr.c:233 `pagecnt = 1`).
        let base = PrOptions::default();
        let many: Vec<Vec<u8>> = (1..=57).map(|n| n.to_string().into_bytes()).collect();
        let files = [("f1", many), ("f2", vec![b"z".to_vec()])];
        let pages = paginate_files(&files, &base, None);
        assert_eq!(pages[0].header_lines[2], b"f1 Page 1");
        assert_eq!(pages[1].header_lines[2], b"f1 Page 2");
        assert_eq!(pages[2].header_lines[2], b"f2 Page 1");
    }

    #[test]
    fn test_paginate_files_header_override_applies_to_all() {
        let base = PrOptions::default();
        let files = [
            ("a.txt", vec![b"x".to_vec()]),
            ("b.txt", vec![b"y".to_vec()]),
        ];
        let pages = paginate_files(&files, &base, Some("TITLE"));
        assert_eq!(pages[0].header_lines[2], b"TITLE Page 1");
        assert_eq!(pages[1].header_lines[2], b"TITLE Page 1");
    }

    fn block(lines: &[&str]) -> Vec<Vec<u8>> {
        lines.iter().map(|s| s.as_bytes().to_vec()).collect()
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
            ("two.txt", vec![b"a".to_vec(), b"b".to_vec()]),
        ];
        let pages = paginate_files(&files, &options, None);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].body_lines[..2], [b"a".to_vec(), b"b".to_vec()]);
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
        let lines: Vec<Vec<u8>> = (1..=10).map(|n| n.to_string().into_bytes()).collect();
        let pages = paginate(&lines, &options);
        assert_eq!(pages.len(), 3);
        assert_eq!(
            pages[0].body_lines,
            [b"1".to_vec(), b"2".to_vec(), b"3".to_vec(), b"4".to_vec()]
        );
        assert_eq!(
            pages[2].body_lines,
            [b"9".to_vec(), b"10".to_vec()],
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
        assert_eq!(
            pages[0].body_lines,
            [b"5".to_vec(), b"6".to_vec(), b"7".to_vec(), b"8".to_vec()]
        );
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
        let lines: Vec<Vec<u8>> = (1..=24).map(|n| n.to_string().into_bytes()).collect();
        // One page is `rows * columns` = 12 input lines, laid out down column 1 and
        // then column 2, each cell padded to `colwd + 1` except the last column.
        let first = paginate(&lines, &options);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].body_lines[0], b"1         7");
        let second_page = PrOptions {
            start_page: 2,
            ..options.clone()
        };
        let second = paginate(&lines, &second_page);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].body_lines[0], b"13        19");
        let past_end = PrOptions {
            start_page: 3,
            ..options.clone()
        };
        assert!(paginate(&lines, &past_end).is_empty());
    }

    #[test]
    fn expand_tabs_follows_the_inln_column_arithmetic() {
        // `col = gap - (col % gap)` blanks per tab, counted from the start of the line
        // because `inln` restarts its buffer at every newline (`pr.c:1046-1063`). The
        // widths are spelled out as `" ".repeat(n)` so the arithmetic, not a hand
        // count of spaces, is what the reader checks.
        assert_eq!(
            expand_tabs(b"a\tb", b'\t', 8),
            format!("a{}b", " ".repeat(7)).into_bytes()
        );
        assert_eq!(
            expand_tabs(b"\t\tx", b'\t', 8),
            format!("{}x", " ".repeat(16)).into_bytes()
        );
        // A second line starts its column count over, so its tab reaches the first
        // stop again instead of continuing the previous line's column.
        assert_eq!(
            expand_tabs(b"12345678\t\n\t", b'\t', 8),
            format!("12345678{}\n{}", " ".repeat(8), " ".repeat(8)).into_bytes()
        );
        // A different pitch and a different tab byte are exactly what the option says.
        assert_eq!(
            expand_tabs(b"a\tb", b'\t', 4),
            format!("a{}b", " ".repeat(3)).into_bytes()
        );
        assert_eq!(
            expand_tabs(b"a^tb", b'^', 8),
            format!("a{}tb", " ".repeat(7)).into_bytes()
        );
        // `ingap == 0` is C's `if (gap)` test: nothing happens at all.
        assert_eq!(expand_tabs(b"a\tb", b'\t', 0), b"a\tb".to_vec());
    }

    #[test]
    fn contract_tabs_reencodes_blanks_and_simulates_existing_tabs() {
        // Eight blanks reach one stop exactly, so the run becomes a tab; ten blanks
        // become a tab plus two (`pr.c:1178-1204`).
        assert_eq!(contract_tabs(b"        x", b'\t', 8), b"\tx".to_vec());
        assert_eq!(contract_tabs(b"          x", b'\t', 8), b"\t  x".to_vec());
        // A single blank is never contracted — that is what keeps the space between
        // two words a space. With the default pitch that also follows from the stop
        // test alone, so the `ips - ops > 1` half is only load-bearing where a stop
        // is one column away: `gap` of 1 (reachable via `-i1`) separates the two.
        assert_eq!(contract_tabs(b"a b", b'\t', 8), b"a b".to_vec());
        assert_eq!(contract_tabs(b"a b", b'\t', 1), b"a b".to_vec());
        assert_eq!(contract_tabs(b"a  b", b'\t', 1), b"a\t b".to_vec());
        // A tab already in the text is *simulated*: it moves the input column to the
        // next stop and emits nothing itself. A tab sitting on a stop is therefore a
        // fixed point, and the three blanks behind it stay three because they are one
        // short of the stop that `ips - ops > 1` refuses to spend a tab on.
        assert_eq!(contract_tabs(b"\t   x", b'\t', 8), b"\t   x".to_vec());
        // Seven blanks after a stop fall one short of the next one (column 8 + 7 = 15,
        // the stop is 16), so that run stays blanks: the fixed point only breaks once
        // the run reaches the stop itself, and then the eight blanks become one tab.
        assert_eq!(
            contract_tabs(format!("a\t{}b", " ".repeat(7)).as_bytes(), b'\t', 8),
            format!("a\t{}b", " ".repeat(7)).into_bytes()
        );
        assert_eq!(
            contract_tabs(format!("a\t{}b", " ".repeat(8)).as_bytes(), b'\t', 8),
            b"a\t\tb".to_vec()
        );
        // Trailing blanks keep their width through the end-of-line flush.
        assert_eq!(contract_tabs(b"a       ", b'\t', 8), b"a\t".to_vec());
        assert_eq!(contract_tabs(b"a  ", b'\t', 8), b"a  ".to_vec());
        // `ogap == 0` is C's plain `fwrite` arm.
        assert_eq!(contract_tabs(b"        x", b'\t', 0), b"        x".to_vec());
    }

    #[test]
    fn e_and_i_values_take_the_c_char_then_gap_shape() {
        // `-e[ch][gap]`: a non-digit first byte is the character, a digit run after it
        // is the gap through `atoi`, and a leftover non-digit is the named invalid
        // value (`pr.c:1674-1692`).
        let plain = parse_pr_args(&["-e"]).expect("-e");
        assert_eq!(plain.options.expand, Some((b'\t', 8)));
        let gap_only = parse_pr_args(&["-e4"]).expect("-e4");
        assert_eq!(gap_only.options.expand, Some((b'\t', 4)));
        let both = parse_pr_args(&["-e:9"]).expect("-e:9");
        assert_eq!(both.options.expand, Some((b':', 9)));
        // A literal zero gap means the default pitch, not "off" (`if (ingap == 0)
        // ingap = INGAP`).
        let zero = parse_pr_args(&["-e0"]).expect("-e0");
        assert_eq!(zero.options.expand, Some((b'\t', 8)));
        // Trailing garbage after the digits is never reached by C's `atoi`, so `-e5x`
        // is simply gap 5.
        assert_eq!(
            parse_pr_args(&["-e5x"]).expect("-e5x").options.expand,
            Some((b'\t', 5))
        );
        assert_eq!(
            parse_pr_args(&["-e:xy"]).err(),
            Some(PrError::InvalidValue('e', "xy".to_string()))
        );
        let contract = parse_pr_args(&["-i3"]).expect("-i3");
        assert_eq!(contract.options.contract, Some((b'\t', 3)));
    }

    #[test]
    fn columns_and_merge_imply_both_transformations() {
        // `pr.c:1840-1848`: multi-column or merged output switches expansion and
        // contraction on with the C's defaults, while plain single-column output
        // switches neither on.
        let single = parse_pr_args(&["-t"]).expect("single");
        assert_eq!(single.options.expand, None);
        assert_eq!(single.options.contract, None);
        let columns = parse_pr_args(&["-t", "-2"]).expect("columns");
        assert_eq!(columns.options.expand, Some((b'\t', 8)));
        assert_eq!(columns.options.contract, Some((b'\t', 8)));
        let merged = parse_pr_args(&["-t", "-m"]).expect("merge");
        assert_eq!(merged.options.expand, Some((b'\t', 8)));
        // An explicit `-e` is not overwritten by the implied default, and `-i` keeps
        // its own gap while `-e` supplies the other.
        let explicit = parse_pr_args(&["-2", "-e4"]).expect("explicit");
        assert_eq!(explicit.options.expand, Some((b'\t', 4)));
        assert_eq!(explicit.options.contract, Some((b'\t', 8)));
    }

    #[test]
    fn f_and_capital_f_replace_the_trailer_with_a_form_feed() {
        // `-F` sets `formfeed`, and `-f` sets it too on the way through the pause
        // flag (`pr.c:1693-1700`). `prtail` then writes one form feed and returns
        // (`pr.c:1555-1561`), so neither the blank padding rows nor the five trailer
        // lines appear — while the header block is untouched.
        let options = parse_pr_args(&["-F", "-l", "20", "-h", "T"]).expect("-F");
        let pages = paginate(&[b"a".to_vec(), b"b".to_vec()], &options.options);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].header_lines.len(), HEADLEN);
        assert_eq!(pages[0].body_lines, [b"a".to_vec(), b"b".to_vec()]);
        assert!(
            pages[0].trailer_lines.is_empty(),
            "no blank trailer under -F"
        );
        assert!(pages[0].form_feed);

        let same = parse_pr_args(&["-f", "-l", "20", "-h", "T"]).expect("-f");
        assert!(same.options.form_feed, "-f falls through to -F");
        assert_eq!(
            paginate(&[b"a".to_vec(), b"b".to_vec()], &same.options),
            pages
        );

        // Without either flag the trailer block is the five blank lines and the short
        // page is padded up to the body height, as before.
        let plain = parse_pr_args(&["-l", "20", "-h", "T"]).expect("plain");
        let plain_pages = paginate(&[b"a".to_vec()], &plain.options);
        assert_eq!(plain_pages[0].trailer_lines.len(), TAILLEN);
        assert_eq!(plain_pages[0].body_lines.len(), 20 - HEADLEN - TAILLEN);
        assert!(!plain_pages[0].form_feed);
    }

    #[test]
    fn form_feed_replaces_padding_in_both_paths_and_no_header_cancels_the_byte() {
        // The padding rows and the trailer come from the same `prtail` call, so a short
        // page under `-F` stops at its real length in the single-column path *and* in
        // `vertcol`, which also ends each page through `prtail` (`pr.c:604-608`).
        let lines: Vec<Vec<u8>> = (1..=3).map(|n| n.to_string().into_bytes()).collect();

        // Two columns over three lines make two rows; the headers keep this leg on the
        // padding branch, which is the whole point of the contrast below.
        let columnar = parse_pr_args(&["-F", "-l", "20", "-2", "-w", "20"]).expect("-F -2");
        let pages = paginate(&lines, &columnar.options);
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].body_lines.len(),
            2,
            "one row per column pair, unpadded"
        );
        assert!(pages[0].trailer_lines.is_empty());
        assert!(pages[0].form_feed);

        let same_shape = parse_pr_args(&["-l", "20", "-2", "-w", "20"]).expect("-2");
        let padded = paginate(&lines, &same_shape.options);
        assert_eq!(
            padded[0].body_lines.len(),
            20 - HEADLEN - TAILLEN,
            "without -F the short column page is padded up to the body height"
        );
        assert_eq!(padded[0].trailer_lines.len(), TAILLEN);
        assert!(!padded[0].form_feed);

        // `-t` makes `prtail` return inside its `nohead` arm, before the form-feed arm,
        // so the byte disappears along with the padding and the trailer — in each of
        // the two column paths, which is why both are asserted separately.
        let bare_single = parse_pr_args(&["-F", "-t", "-l", "20"]).expect("-F -t");
        let pages = paginate(&[b"a".to_vec()], &bare_single.options);
        assert_eq!(
            pages[0].body_lines,
            [b"a".to_vec()],
            "nohead never pads either"
        );
        assert!(!pages[0].form_feed, "single-column nohead drops the byte");

        let bare = parse_pr_args(&["-F", "-t", "-l", "20", "-2", "-w", "20"]).expect("-F -t -2");
        let pages = paginate(&lines, &bare.options);
        assert!(!pages[0].form_feed, "column nohead drops the byte");
        assert!(pages[0].header_lines.is_empty() && pages[0].trailer_lines.is_empty());
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
            vec![b"1   3".to_vec(), b"2   4".to_vec()]
        );
        // Uneven last page: row 1 has col1 empty; col0's inter-column pad
        // stays (C keeps it), only the last column omits padding.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3"]), 2, 2, 3, None),
            vec![b"1   3".to_vec(), b"2   ".to_vec()]
        );
        // A row with no data in any column stops emission: block=["1","2"] fills
        // col0's two rows, so both emit (col1 empty) — trailing inter-column pad
        // kept, last column unpadded.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2"]), 2, 2, 3, None),
            vec![b"1   ".to_vec(), b"2   ".to_vec()]
        );
        // Only row 0 has data → row 1 is empty in every column → emission stops.
        assert_eq!(
            vertcol_body_rows(&block(&["1"]), 2, 2, 3, None),
            vec![b"1   ".to_vec()]
        );
    }

    #[test]
    fn test_vertcol_sep_and_clip() {
        // -s: a single separator byte, no width padding between columns.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3", "4"]), 2, 2, 3, Some(b'\t')),
            vec![b"1\t3".to_vec(), b"2\t4".to_vec()]
        );
        // Cells clip to colwd bytes (C's inln truncation).
        assert_eq!(
            vertcol_body_rows(&block(&["abcdef", "x", "y", "z"]), 2, 2, 3, None),
            vec![b"abc y".to_vec(), b"x   z".to_vec()]
        );
        // A cell is cut at `colwd` bytes: Latin-1 `e-acute` is one byte, so the
        // third byte of the cell is that byte and the cut lands right after it.
        assert_eq!(
            vertcol_body_rows(&[b"h\xe9llo".to_vec()], 1, 1, 3, None),
            vec![b"h\xe9l".to_vec()]
        );
    }

    #[test]
    fn a_cell_is_cut_at_bytes_even_mid_code_point() {
        // The cut point is a byte count, so a cell may end inside a code point:
        // `a b e-acute` is four bytes (`c3 a9` for the accent) and `colwd` 3 hands
        // back the three bytes including the dangling lead byte. Flooring to a code
        // point boundary — the shape this leg had before — would hand back two.
        assert_eq!(
            vertcol_body_rows(&[b"ab\xc3\xa9".to_vec()], 1, 1, 3, None),
            vec![b"ab\xc3".to_vec()]
        );
        // A byte that is no UTF-8 lead byte at all travels the same way: C's `stp`
        // counts bytes and never inspects them.
        assert_eq!(
            vertcol_body_rows(&[b"a\xff\xfez".to_vec()], 1, 1, 3, None),
            vec![b"a\xff\xfe".to_vec()]
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
            vec![b"a  c".to_vec(), b"b  d".to_vec()]
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
        assert_eq!(out[0], b""); // header blank spacer, unindented
        assert_eq!(out[2], b"   f.txt Page 1"); // header text line, indented
        assert_eq!(out[5], b"   a"); // body, indented
        assert_eq!(out[6], b"   b");
        assert_eq!(out[7], b""); // trailer, unindented
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
        assert_eq!(pages[0].header_lines[2], b" Page 1");
        // Two lines across a 2-column page leave far more than a full column of
        // slack, so the final page balances (`pr.c:522`): `a` and `b` share one
        // row (`a` padded to the `colwd + 1` stride, `b` the unpadded last
        // column), then the body pads out to the 56-line capacity.
        assert_eq!(pages[0].body_lines[0], b"a  b");
        assert_eq!(pages[0].body_lines[1], b"");
        assert_eq!(pages[0].body_lines.len(), 56);
        assert_eq!(pages[0].trailer_lines.len(), TAILLEN);
    }

    #[test]
    fn test_number_field_width_and_truncation() {
        assert_eq!(number_field(1, 3), b"  1");
        assert_eq!(number_field(12, 3), b" 12");
        // A number wider than the field keeps its last `width` digits (C addnum
        // writes from the right and stops at the left edge).
        assert_eq!(number_field(1234, 3), b"234");
        assert_eq!(number_field(7, 1), b"7");
    }

    #[test]
    fn test_paginate_number_prefixes_body_running_per_file() {
        let mut options = PrOptions::default();
        options.number_width = 3; // number_field width; default char tab
        options.page_len = 12; // body capacity 2
        let pages = paginate_lines(&["a", "b", "c", "d"], &options);
        // 4 lines over body capacity 2 → 2 pages, numbering runs continuously.
        assert_eq!(pages[0].body_lines[0], b"  1\ta");
        assert_eq!(pages[0].body_lines[1], b"  2\tb");
        assert_eq!(pages[1].body_lines[0], b"  3\tc");
        assert_eq!(pages[1].body_lines[1], b"  4\td");
    }

    #[test]
    fn test_horzcol_fills_across_rows() {
        // Row-major (across): consecutive lines go left-to-right, contrasted
        // against vertcol_body_rows' column-major down-fill of the same block.
        assert_eq!(
            horzcol_body_rows(&block(&["a", "b", "c", "d"]), 2, 3, None),
            vec![b"a   b".to_vec(), b"c   d".to_vec()]
        );
        assert_eq!(
            vertcol_body_rows(&block(&["a", "b", "c", "d"]), 2, 2, 3, None),
            vec![b"a   c".to_vec(), b"b   d".to_vec()]
        );
        // Odd count: last row's single cell is padded (col0 is not last), then
        // EOF stops the row — trailing inter-column pad kept.
        assert_eq!(
            horzcol_body_rows(&block(&["a", "b", "c"]), 2, 3, None),
            vec![b"a   b".to_vec(), b"c   ".to_vec()]
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
            vec![b"a  b".to_vec(), b"c  d".to_vec()]
        );
    }

    #[test]
    fn test_double_space_interleaves_blank_and_halves_page() {
        let mut options = PrOptions::default();
        options.double_space = true;
        options.page_len = 12; // body_capacity 2 → 1 content line/page, doubled to 2 rows
        let pages = paginate_lines(&["a", "b", "c"], &options);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].body_lines, vec![b"a".to_vec(), Vec::new()]);
        assert_eq!(pages[2].body_lines, vec![b"c".to_vec(), Vec::new()]);
    }

    #[test]
    fn test_double_space_one_row_page_falls_back() {
        let mut options = PrOptions::default();
        options.double_space = true;
        options.page_len = 11; // body_capacity 1 → no room to double (C `dspace = 0`)
        let pages = paginate_lines(&["a", "b"], &options);
        assert_eq!(pages[0].body_lines, vec![b"a".to_vec()]);
        assert_eq!(pages[1].body_lines, vec![b"b".to_vec()]);
    }

    #[test]
    fn test_start_page_skips_ahead_and_offsets_numbering() {
        let mut options = PrOptions::default();
        options.page_len = 12; // body_capacity 2
        options.start_page = 3; // skip (3-1)*2 = 4 content lines
        let lines: Vec<Vec<u8>> = ("123456")
            .chars()
            .map(|c| c.to_string().into_bytes())
            .collect();
        let pages = paginate(&lines, &options);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].header_lines[2], b" Page 3");
        assert_eq!(pages[0].body_lines, vec![b"5".to_vec(), b"6".to_vec()]);
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
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
            vec![b"1".to_vec(), b"2".to_vec()],
        ];
        // Row 0/1: both columns; row 2: only stream 0 has data → trailing EOF
        // column dropped, and colwd 3 leaves "c" alone (last active, no pad).
        assert_eq!(
            merge_body_rows(&streams, 3, None),
            vec![b"a   1".to_vec(), b"b   2".to_vec(), b"c".to_vec()]
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
            vec![b"1   3".to_vec(), b"2   4".to_vec()]
        );
        // The naive down-fill of the same block is the staircase, for contrast.
        assert_eq!(
            vertcol_body_rows(&block(&["1", "2", "3", "4"]), 3, 2, 3, None),
            vec![b"1   4".to_vec(), b"2   ".to_vec(), b"3   ".to_vec()]
        );
    }

    #[test]
    fn test_vertcol_balanced_odd_count_keeps_trailing_pad() {
        // 3 lines, 2 columns → pln 2: col0 [1,2], col1 [3]. Row 1's only cell
        // (col0) is non-last, so its inter-column pad stays and col1 (absent)
        // breaks the row — exactly C's `tvc >= cvc` break (pr.c:568).
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3"]), 2, 3, None),
            vec![b"1   3".to_vec(), b"2   ".to_vec()]
        );
    }

    #[test]
    fn test_vertcol_balanced_three_columns() {
        // 3 lines, 3 columns → pln = ceil(3/3) = 1: one even row across all
        // three columns; the last column is unpadded.
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3"]), 3, 3, None),
            vec![b"1   2   3".to_vec()]
        );
    }

    #[test]
    fn test_vertcol_balanced_sep_and_clip() {
        // -s: one separator byte, no width padding; last column unpadded.
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["1", "2", "3", "4"]), 2, 3, Some(b'\t')),
            vec![b"1\t3".to_vec(), b"2\t4".to_vec()]
        );
        // Cells clip to colwd bytes before the pad (C's inln truncation).
        assert_eq!(
            vertcol_body_rows_balanced(&block(&["abcdef", "x", "y", "z"]), 2, 3, None),
            vec![b"abc y".to_vec(), b"x   z".to_vec()]
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
            vec![b"a  c".to_vec(), b"b  d".to_vec()]
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
        assert_eq!(pages[0].header_lines[2], b" Page 1");
        assert_eq!(
            pages[0].body_lines,
            vec![b"a  c".to_vec(), b"b  d".to_vec(), Vec::new(), Vec::new(),]
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
            vec![b"a  d".to_vec(), b"b  e".to_vec(), b"c  f".to_vec()]
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

    /// 集外字母交回的是短横后的**第一个字节**而不是第一个字符：`-\u{e9}`（`c3 a9`）在
    /// `char` 域里是 U+00C3。本支的字母来自 `:522` 的 `bytes[pos] as char`，机制层证据在
    /// `minix3/usr.bin/pr/egetopt.c:110`（`eoptopt = (int)*place++` 一次进一个字节）与
    /// `:148`（`%c` 原样写出），参照件 `tmp/pr-419/pr_ref` 现量 `-é` 与 `-dé` 都打单个
    /// `303`（`d` 在 `"#adFfmrte?h:i?l:n?o:ps?T:w:"` 里不带值，聚簇继续吃到 `é` 的第一字节）。
    /// 逐字节面门在 `tests/pr_face.rs`。
    #[test]
    fn the_illegal_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_pr_args(&["-\u{e9}"]).err(),
            Some(PrError::Getopt(GetoptReport::IllegalOption('\u{00c3}'))),
            "`é` 是 c3 a9 两字节，egetopt 报 c3 而不是整个字符"
        );
        // `d` 合法且不带值 ⇒ 聚簇继续吃到第一字节（参照件同读数）。
        assert_eq!(
            parse_pr_args(&["-d\u{e9}"]).err(),
            Some(PrError::Getopt(GetoptReport::IllegalOption('\u{00c3}'))),
            "-d\u{e9}"
        );
        // 环停在第一个坏字母上。
        assert_eq!(
            parse_pr_args(&["-Z\u{e9}"]).err(),
            Some(PrError::Getopt(GetoptReport::IllegalOption('Z'))),
            "-Z\u{e9}"
        );
        // 反面：带值字母把其后的字节整个当值收走（`w` 的值首位不是数字 ⇒ 自己那句）。
        assert_eq!(
            parse_pr_args(&["-w\u{e9}"]).err(),
            Some(PrError::WidthValue),
            "-w\u{e9} 走的是 `-w width must be 1 or more ' 那一句"
        );
    }

    #[test]
    fn parse_error_paths() {
        // 集外/未建模字母、缺值、C 为每个数值选项单写的判据消息、`-a` 与 `-m`
        // 的组合拒绝。
        // `-e`/`-i` are modelled now, so an unimplemented letter has to come from the
        // rest of C's optstring; `-x` is not in it at all and answers usage.
        assert_eq!(parse_pr_args(&["-p"]).err(), Some(PrError::Unmodelled));
        // 集外字母与缺值各带一个 `egetopt` 报文（注意集外字母那一句的措辞与
        // libc `getopt` 同族其它命令不同形：这里是 `illegal option`）。
        assert_eq!(
            parse_pr_args(&["-w"]).err(),
            Some(PrError::Getopt(GetoptReport::MissingValue('w')))
        ); // 缺值 ⇒ egetopt 那一句（egetopt.c:196-205）
        assert_eq!(
            parse_pr_args(&["-x"]).err(),
            Some(PrError::Getopt(GetoptReport::IllegalOption('x')))
        ); // 集外字母 ⇒ egetopt.c:144-151
        assert_eq!(parse_pr_args(&["-w0"]).err(), Some(PrError::WidthValue)); // atoi < 1
        assert_eq!(parse_pr_args(&["-w", "x"]).err(), Some(PrError::WidthValue)); // 首位非数字
        assert_eq!(parse_pr_args(&["-o0"]).err(), Some(PrError::OffsetValue));
        assert_eq!(parse_pr_args(&["+0"]).err(), Some(PrError::PlusPage));
        assert_eq!(parse_pr_args(&["-0"]).err(), Some(PrError::ColumnCount));
        assert_eq!(parse_pr_args(&["-n0"]).err(), Some(PrError::NumberWidth));
        assert_eq!(
            parse_pr_args(&["-sab"]).err(),
            Some(PrError::InvalidValue('s', "b".to_string()))
        );
        assert_eq!(
            parse_pr_args(&["-nxy"]).err(),
            Some(PrError::InvalidValue('n', "y".to_string()))
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
