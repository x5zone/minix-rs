//! Field list parsing and column selection behind `cut`.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (option string `b:c:d:f:sn` at
//! line 88). Three selection modes share one list grammar: numbers and
//! `low-high` ranges, 1 based, separated by any of comma, space and tab
//! (`strtok(list, ", \t")`, `cut.c:175`), with empty items collapsed and no
//! cap on the item count:
//!
//! - `-b`: byte positions.
//! - `-c`: character positions (identical to bytes here; multibyte
//!   awareness is a documented follow-up, matching the engine's byte
//!   orientation).
//! - `-f`: delimiter separated fields with a custom delimiter (`-d`),
//!   suppressing lines without the delimiter unless `-s` is off (default
//!   prints them whole).

use crate::{GetoptReport, TextError};
use alloc::vec::Vec;

/// One selected interval, 1 based and inclusive on both ends.
/// `high == u32::MAX` means "to the end of the line" (`3-`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// First selected position, 1 based.
    pub low: u32,
    /// Last selected position, inclusive.
    pub high: u32,
}

/// The two errors a list can still raise beyond its two endpoints, matching the
/// two `errx` calls of C `get_list()`.
///
/// C has only these two names (`cut.c:196`, `:198`), and neither sentence takes
/// a value: there is no `%s` in it, so the caller cannot splice the offending
/// span back into the sentence. The only difference is: leftover characters get
/// the first sentence (`if (*p) errx(...)`, `cut.c:195-196`), a parsed zero gets
/// the second (`if (!stop || !start)`, `:197-198`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListError {
    /// This item cannot be read to the end (leftover characters, not a number).
    IllegalValue,
    /// A zero appeared in this item (a lone `0`, `0-5`, `5-0`).
    IncludesZero,
}

/// Parse a `cut` list (`1,3-5,8-`) into ranges, sorted by `low` with overlaps
/// kept (overlapping output is deduplicated at selection time, matching the C
/// tool printing each byte once).
///
/// The four shapes below were re-derived from C, and the old spelling rejected
/// all of them: the **delimiter set is exactly `,`, space and tab**, and empty
/// items collapse (`cut.c:175` uses `strtok(list, ", \t")`), so `1,,2` and
/// `1 2` are both two items; **an empty list is not an error** (`strtok` hands
/// back NULL, the loop body never runs, so no position is selected); `5-2` does
/// not check the ordering (`:209-210` just sets positions while
/// `start++ <= stop`, so a reversed range sets nothing); and the item count has
/// no cap (`:163-164` first allocation, `:199-206` grows by
/// `ALLOC_CHUNK = _POSIX2_LINE_MAX`, `:153`). The autostart/autostop of forms
/// like `-3-5` are global quantities spanning items and lines (`:183-184`,
/// `:191-192`, `:213-219`); this tree models only as far as single-item parsing
/// and single-range selection, and the rest is left for a later item.
pub fn parse_list(text: &[u8]) -> Result<Vec<Range>, ListError> {
    let mut ranges: Vec<Range> = Vec::new();
    for token in text.split(|byte| matches!(byte, b' ' | b'\t' | b',')) {
        if token.is_empty() {
            // strtok's empty-item collapse: neither `1,,2` nor ` 1 ` is an error.
            continue;
        }
        ranges.push(parse_token(token)?);
    }
    // Insertion-sort semantics unchanged (C's `positions` bitmap is naturally
    // ordered); the list is small anyway.
    ranges.sort_by_key(|range| range.low);
    Ok(ranges)
}

/// One item, matching the three stages of `cut.c:176-198` literally: a leading
/// `-` is C's setautostart form, and `N-` reaches the end of the line through
/// autostop. C has **two** tests on `*p == '-'` (`:186`, `:189`), and the second
/// examines the same position — the pointer advances only when `isdigit(p[1])`
/// holds and `strtol` actually ran (`:187-188`); otherwise the pointer stays
/// put, so the second test hits the same `-` again and does `++p` (`:190`).
/// Flattening this two-level structure into "skip the `-` and read a number"
/// reverses the outcomes: `5--` is leftover characters in C (`illegal list
/// value`), while the flattened form reads it as the zero sentence; `1-2-` is
/// **accepted** in C (the autostop arm prints everything to the end of the
/// line), while the flattened form reports a false leftover. The order of the
/// leftover and zero sentences may not swap either: leftover first (`:195-196`),
/// zero second (`:197-198`), so `0-` takes the zero sentence and `x` takes the
/// leftover one.
fn parse_token(token: &[u8]) -> Result<Range, ListError> {
    let bytes = token;
    let mut cursor = 0usize;
    let auto_start = bytes.first() == Some(&b'-');
    if auto_start {
        cursor = 1;
    }
    // `if (isdigit((unsigned char)*p)) start = stop = strtol(p, &p, 10)`
    // (`:181-182`): with no digit at all both endpoints stay 0 and the pointer
    // does not move.
    let (start, next) = scan_number(token, cursor);
    cursor = next;
    let mut stop = start;
    let mut open_ended = false;
    if bytes.get(cursor) == Some(&b'-') {
        let after = cursor + 1;
        // Inner first test `if (isdigit(p[1])) stop = strtol(p + 1, &p, 10)`
        // (`:187-188`). Note it **overwrites** stop unconditionally, including
        // writing it to 0 (`5-0` therefore lands in the zero sentence).
        let (value, scanned) = scan_number(token, after);
        if scanned > after {
            stop = value;
            cursor = scanned;
        }
        // Inner second test `if (*p == '-') { ++p; … }` (`:189-193`): when
        // `strtol` did not run, this hits the same `-` again. Reaching the end of
        // the line is C's autostop; this tree represents the effect ("print
        // everything after maxval as is", `x_cut.c:80-82`) by letting high go to
        // the top.
        if bytes.get(cursor) == Some(&b'-') {
            cursor += 1;
            if cursor == bytes.len() {
                open_ended = true;
            }
        }
    }
    if cursor != bytes.len() {
        return Err(ListError::IllegalValue);
    }
    if start == 0 || stop == 0 {
        return Err(ListError::IncludesZero);
    }
    Ok(Range {
        low: if auto_start { 1 } else { start },
        high: if open_ended { u32::MAX } else { stop },
    })
}

/// C's `strtol(p, &p, 10)` (`:182`, `:188`): when no digit converts, the value
/// is 0 and the pointer does not move (matching C's `if (isdigit(…))` being
/// false). When digits convert but overflow `u32`, this tree saturates to the
/// maximum, while C gets `LONG_MAX` and reallocates `positions` to that size
/// (`:199-206`) until allocation fails — that shape is not modelled; saturation
/// and C agree on "no sentence reported".
fn scan_number(token: &[u8], from: usize) -> (u32, usize) {
    let bytes = token;
    let mut cursor = from;
    let mut value: u32 = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(bytes[cursor] - b'0'));
        cursor += 1;
    }
    (value, cursor)
}

/// Select byte ranges from `line` (1 based positions), writing into `out`.
///
/// Returns the used byte count. Overlapping ranges print each byte once.
pub fn select_bytes(line: &[u8], ranges: &[Range], out: &mut [u8]) -> Result<usize, TextError> {
    let bytes = line;
    let mut written = 0;
    let mut next = 1u32;
    for range in ranges {
        let low = range.low.max(next);
        let high = range.high.min(bytes.len() as u32);
        if low <= high {
            for index in (low - 1)..high {
                if written >= out.len() {
                    return Err(TextError::TooLong);
                }
                out[written] = bytes[index as usize];
                written += 1;
            }
            next = high + 1;
        }
    }
    Ok(written)
}

/// Select delimiter separated fields from `line`.
///
/// Fields are 1 based; `ranges` reuse the same grammar. The delimiter is one
/// byte. The return says **how much of this line exists at all**: `Some(len)`
/// is the selected bytes, and the caller terminates them as it terminates any
/// output line; `None` means this line has no delimiter and `-s` is set, so C
/// skips it whole — `cut.c:248` only hands the line to `fwrite` when `!sflag`,
/// and `:253-254`'s `if (!isdelim) continue;` then jumps past the
/// `putchar('\n')` at `:279`, so the line contributes **zero bytes**, not an
/// empty line. A line
/// that does carry the delimiter but selects nothing is the opposite case and
/// stays `Some(0)` (a reference run of `-f 9 -s` on `a\tb\tc` returns one
/// newline).
pub fn select_fields(
    line: &[u8],
    ranges: &[Range],
    delimiter: u8,
    suppress: bool,
    out: &mut [u8],
) -> Result<Option<usize>, TextError> {
    // When the delimiter happens to be a newline, every line "carries the
    // delimiter" from C's point of view: the string handed to the scanner
    // **always ends with a newline** (`minix3/usr.bin/cut/cut.c:235-241` — a last
    // line without one gets `malloc(len + 1)` and a patched byte), so the
    // scanner's `if (ch == sep)` always hits at that cell (`cut.c:242-251`),
    // `isdelim` holds, `-s` cannot erase the line, and the "copy the whole line
    // as is" leg is not taken either. That the line body here lacks that byte is
    // the result of **line splitting**, not what C sees.
    let carries_delimiter = delimiter == b'\n' || line.contains(&delimiter);
    if !carries_delimiter {
        if suppress {
            return Ok(None);
        }
        if line.len() > out.len() {
            return Err(TextError::TooLong);
        }
        out[..line.len()].copy_from_slice(line);
        return Ok(Some(line.len()));
    }
    let bytes = line;
    let mut written = 0;
    let mut field: u32 = 1;
    let mut start = 0;
    let mut first = true;
    let mut pos = 0;
    while pos <= bytes.len() {
        let end = pos == bytes.len() || bytes[pos] == delimiter;
        if end {
            if in_ranges(ranges, field) {
                if !first {
                    if written >= out.len() {
                        return Err(TextError::TooLong);
                    }
                    out[written] = delimiter;
                    written += 1;
                }
                first = false;
                for byte in &bytes[start..pos] {
                    if written >= out.len() {
                        return Err(TextError::TooLong);
                    }
                    out[written] = *byte;
                    written += 1;
                }
            }
            field += 1;
            start = pos + 1;
        }
        pos += 1;
    }
    Ok(Some(written))
}

fn in_ranges(ranges: &[Range], field: u32) -> bool {
    ranges
        .iter()
        .any(|range| range.low <= field && field <= range.high)
}

/// Parsed `cut` command line: the byte (`-b`/`-c`) and field (`-f`) list
/// selectors, the `-d` delimiter, the `-s` suppress flag, and file operands
/// (a bare `-` names standard input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutArgs<'a> {
    pub bytes_list: Option<&'a [u8]>,
    pub fields_list: Option<&'a [u8]>,
    pub delimiter: u8,
    /// Whether `-d` was actually given. C records it separately in `dflag`, and
    /// the post-check at `cut.c:124` asks about the flag rather than the value:
    /// a byte list together with `-d` is a usage error.
    pub delimiter_given: bool,
    pub suppress: bool,
    /// Whether `-s` was actually given (`sflag`, the other half of the same
    /// post-check).
    pub suppress_given: bool,
    pub operands: Vec<&'a [u8]>,
}

/// `cut` argument errors. Each arm corresponds to one real exit of C (the caller
/// renders it), instead of "everything is a usage error":
/// - [`CutError::Usage`]: C's four post-checks (`cut.c:120-128`); all go
///   straight to `usage()`, **with no getopt line in front**.
/// - [`CutError::Getopt`]: a letter outside the set or a missing value; getopt
///   prints one line first and usage follows.
/// - [`CutError::IllegalList`] / [`CutError::ZeroInList`]: the two `errx` calls
///   of C `get_list()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutError {
    Usage,
    Getopt(GetoptReport),
    IllegalList,
    ZeroInList,
}

/// Attribute a list-parse error to one of C's two sentences. The shell also
/// takes this path when fetching ranges (unreachable there any more, but the
/// sentences live only here and in `arg_error`, so they are not written twice).
impl From<ListError> for CutError {
    fn from(error: ListError) -> Self {
        match error {
            ListError::IllegalValue => CutError::IllegalList,
            ListError::IncludesZero => CutError::ZeroInList,
        }
    }
}

/// Fold `cut`'s argv tail faithfully to BSD `getopt("b:c:d:f:sn")` (`cut.c:88`):
/// `-b`/`-c`/`-f`/`-d` take a value attached (`-f1-3`, `-d:`) or as the next
/// word; `-s`/`-n` are flags (so `-sf1-3` is `-s` then `-f 1-3`, and `-sn`/`-ns`
/// both set suppress); `--` ends option scanning; any other dash letter is the
/// libc report, never a leaked operand. `-b`/`-c` set the byte list (they
/// coincide here — the selection is byte-grained).
///
/// The two moments cannot be merged: a list's **validity** is checked on the
/// spot inside the option ring (C's `get_list(optarg)` sits in the
/// `case 'b'`/`case 'c'`/`case 'f'` arms, `cut.c:90-108`), so an invalid list
/// reports before a later unknown letter; while the **mode conflict** is only
/// checked outside the ring (`:120-128`), so `cut -b 1 -f 2 -x` reports `-x`
/// rather than the conflict.
pub fn parse_cut_args<'a>(args: &'a [&'a [u8]]) -> Result<CutArgs<'a>, CutError> {
    let mut bytes_list: Option<&'a [u8]> = None;
    let mut fields_list: Option<&'a [u8]> = None;
    let mut delimiter: u8 = b'\t';
    let mut delimiter_given = false;
    let mut suppress = false;
    let mut suppress_given = false;
    let mut operands: Vec<&'a [u8]> = Vec::new();

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg;
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'b' | b'c' | b'f' => {
                        let letter = bytes[pos];
                        let value = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            match args.get(index) {
                                Some(v) => *v,
                                // C: the optstring does not start with `:`, so on a
                                // missing value getopt prints one line first and then
                                // returns `'?'` (`getopt.c:132-136`).
                                None => {
                                    return Err(CutError::Getopt(GetoptReport::MissingValue(
                                        char::from(letter),
                                    )));
                                }
                            }
                        };
                        // C's `get_list(optarg)` runs inside this case
                        // (`cut.c:91/96/105`), so an error may be reported before
                        // the two post-checks below.
                        parse_list(value).map_err(CutError::from)?;
                        if letter == b'f' {
                            fields_list = Some(value);
                        } else {
                            bytes_list = Some(value);
                        }
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    b'd' => {
                        let value = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            match args.get(index) {
                                Some(v) => *v,
                                None => {
                                    return Err(CutError::Getopt(GetoptReport::MissingValue('d')));
                                }
                            }
                        };
                        // C `cut.c:101-102` is a single `dchar = *optarg`: take
                        // the first byte; extra bytes are ignored without an
                        // error, and an empty string yields that `\0`.
                        delimiter = *value.first().unwrap_or(&0);
                        delimiter_given = true;
                        pos = bytes.len();
                    }
                    b's' => {
                        suppress = true;
                        suppress_given = true;
                        pos += 1;
                    }
                    // `-n`: accepted and ignored for compatibility
                    // (`cut.c:112-113` `case 'n': break`), never a leaked operand.
                    b'n' => pos += 1,
                    _ => {
                        return Err(CutError::Getopt(GetoptReport::UnknownOption(char::from(
                            bytes[pos],
                        ))));
                    }
                }
            }
            index += 1;
            continue;
        }
        operands.push(arg);
        index += 1;
    }
    // C `cut.c:120-128`'s four post-checks; all four arms go through the same
    // `usage()` (no getopt line in front):
    //   if (fflag) { if (cflag || bflag) usage(); }
    //   else if ((!cflag && !bflag) || dflag || sflag) usage();
    //   else if (bflag && cflag) usage();
    // In this implementation `-b` and `-c` are synonyms (selection is
    // byte-grained), so the third check is unreachable: the "bflag and cflag both
    // set" state it needs cannot be expressed in this tree, and the difference is
    // recorded in the cut row of
    // `rewrite-notes/18-stage-commands/07-text-filter.md`.
    if fields_list.is_some() {
        if bytes_list.is_some() {
            return Err(CutError::Usage);
        }
    } else if bytes_list.is_none() || delimiter_given || suppress_given {
        return Err(CutError::Usage);
    }
    Ok(CutArgs {
        bytes_list,
        fields_list,
        delimiter,
        delimiter_given,
        suppress,
        suppress_given,
        operands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_of(line: &str, list: &[u8]) -> String {
        let ranges = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_bytes(line.as_bytes(), &ranges, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    /// The byte leg counts bytes: C compiles `-b` into its own `b_cut`, whose
    /// get/put macros are `getc`/`putchar` (`minix3/usr.bin/cut/x_cut.c:45-50`;
    /// only the `c_cut` leg of the same file uses `getwc`/`putwchar`, `:52-57`).
    /// So a byte that does not decode as UTF-8 occupies one cell like any other
    /// byte and must be handed back unchanged.
    #[test]
    fn a_byte_that_is_not_utf8_occupies_one_position() {
        let ranges = [Range { low: 2, high: 2 }];
        let mut out = [0u8; 8];
        let len = select_bytes(b"a\xffb", &ranges, &mut out).unwrap();
        assert_eq!(&out[..len], b"\xff");
        // The line has no delimiter and `-s` is off: the whole line is copied,
        // and that byte travels along unchanged.
        let mut copied = [0u8; 8];
        let whole = select_fields(b"a\xffb", &ranges, b'\t', false, &mut copied)
            .unwrap()
            .expect("整行搬运，不是跳过这一行");
        assert_eq!(&copied[..whole], b"a\xffb");
    }

    /// The delimiter is exactly a newline: every line counts as "carrying the
    /// delimiter", so the field leg runs and the second cell is an empty one,
    /// while `-s` has no line to erase. A reference run of `-f 2` and `-f 2 -s`
    /// on `a\nb c\n` returns `\n\n` for both (one newline per line, empty
    /// field).
    #[test]
    fn a_newline_delimiter_makes_every_line_a_fielded_line() {
        let second = [Range { low: 2, high: 2 }];
        let mut out = [0u8; 8];
        assert_eq!(
            select_fields(b"a", &second, b'\n', false, &mut out).unwrap(),
            Some(0),
            "第二格存在但是空的：这一行走的是字段腿，不是整行搬运"
        );
        assert_eq!(
            select_fields(b"a", &second, b'\n', true, &mut out).unwrap(),
            Some(0),
            "这里没有「整行不带分隔符」可供 `-s` 抹掉"
        );
        // Only with a different delimiter, which the line body does not contain,
        // does the shape return to "the whole line has no delimiter".
        assert_eq!(
            select_fields(b"a", &second, b';', true, &mut out).unwrap(),
            None
        );
    }

    /// `None` means "this line was not written at all", which is not the same
    /// thing as an empty string (written, but no field could be selected).
    fn fields_of(line: &str, list: &[u8], delimiter: u8, suppress: bool) -> Option<String> {
        let ranges = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len =
            select_fields(line.as_bytes(), &ranges, delimiter, suppress, &mut out).unwrap()?;
        Some(String::from_utf8_lossy(&out[..len]).into_owned())
    }

    #[test]
    fn test_byte_selection() {
        assert_eq!(bytes_of("hello", b"2-4"), "ell");
        assert_eq!(bytes_of("hello", b"1,5"), "ho");
        assert_eq!(bytes_of("hello", b"3-"), "llo");
        assert_eq!(bytes_of("hello", b"-2"), "he");
    }

    #[test]
    fn test_overlaps_print_once() {
        assert_eq!(bytes_of("hello", b"1-3,2-4"), "hell");
    }

    #[test]
    fn test_field_selection() {
        assert_eq!(fields_of("a:b:c", b"2", b':', false).as_deref(), Some("b"));
        assert_eq!(
            fields_of("a:b:c", b"1,3", b':', false).as_deref(),
            Some("a:c")
        );
        assert_eq!(
            fields_of("a:b:c", b"2-", b':', false).as_deref(),
            Some("b:c")
        );
    }

    #[test]
    fn test_missing_delimiter_policy() {
        assert_eq!(fields_of("abc", b"1", b':', false).as_deref(), Some("abc"));
        // `-s` with no delimiter in this line: skip the whole line, not one byte
        // is emitted (`cut.c:248`/`:253-254`).
        assert_eq!(fields_of("abc", b"1", b':', true), None);
    }

    /// A reference reading: `-s` swallows "the line without the delimiter",
    /// while the line that "has the delimiter but selects no field" still returns
    /// one newline — the two shapes must be two distinct values in the library.
    #[test]
    fn a_suppressed_line_is_absent_rather_than_empty() {
        let ranges = parse_list(b"9").unwrap();
        let mut out = [0u8; 16];
        assert_eq!(
            select_fields(b"a\tb\tc", &ranges, b'\t', true, &mut out).unwrap(),
            Some(0),
            "第 9 档选空：这一行写了，只是写不出字段"
        );
        assert_eq!(
            select_fields(b"alone", &ranges, b'\t', true, &mut out).unwrap(),
            None,
            "没有分隔符：这一行整行不存在"
        );
        assert_eq!(
            select_fields(b"alone", &ranges, b'\t', false, &mut out).unwrap(),
            Some(5),
            "不带 `-s` 时原行照抄"
        );
    }

    #[test]
    fn test_bad_lists_rejected() {
        // Only two shapes raise an error, each corresponding to C's own sentence:
        // zero (`:197-198`) and leftover (`:195-196`).
        assert_eq!(parse_list(b"0").err(), Some(ListError::IncludesZero));
        assert_eq!(parse_list(b"a").err(), Some(ListError::IllegalValue));
        assert_eq!(parse_list(b"1-x").err(), Some(ListError::IllegalValue));
        assert_eq!(parse_list(b"1-2-3").err(), Some(ListError::IllegalValue));
        // A lone minus: both start and stop are zero (C's setautostart produces
        // no number), so the zero sentence applies.
        assert_eq!(parse_list(b"-").err(), Some(ListError::IncludesZero));
    }

    /// The four shapes of the list grammar are derived from C's `get_list`: the
    /// old spelling rejected all four, while C accepts them.
    #[test]
    fn list_grammar_matches_the_strtok_rule() {
        // (1) An empty list is not an error: `strtok` hands back NULL
        // (`cut.c:175`), so no position is selected.
        assert_eq!(parse_list(b"").unwrap().len(), 0);
        assert_eq!(bytes_of("abc", b""), "");
        // (2) Empty items collapse and blanks separate: `cut.c:175`'s delimiter
        // set is `,`, space and tab.
        assert_eq!(bytes_of("abcde", b"1,,2"), "ab");
        assert_eq!(bytes_of("abcde", b"1 2"), "ab");
        assert_eq!(bytes_of("abcde", b"1\t3"), "ac");
        // (3) A reversed range does not check the ordering; it simply selects
        // nothing (the `start++ <= stop` loop at `:209-210` never runs).
        assert_eq!(bytes_of("abcde", b"5-2"), "");
        // (4) No cap on the item count (C grows by `ALLOC_CHUNK`): 20 items are
        // accepted.
        let many = (1..=20)
            .map(|n| n.to_string())
            .collect::<Vec<String>>()
            .join(",")
            .into_bytes();
        assert_eq!(parse_list(&many).unwrap().len(), 20);
    }

    /// C's two tests on `*p == '-'` (`:186`, `:189`) must not be flattened into
    /// one thing: when `strtol` did not run the pointer stays put and the second
    /// test hits the same `-`. Under the flattened spelling the two error
    /// sentences come out exactly swapped.
    #[test]
    fn the_two_dash_tests_are_not_merged() {
        // `5--`: the first test's `isdigit(p[1])` fails (`:187`), so the second
        // test steps over the `-` at index 1 and stops at the `-` at index 2 →
        // the leftover sentence (`:195-196`).
        assert_eq!(parse_list(b"5--").err(), Some(ListError::IllegalValue));
        // `1-2-` is accepted by C: stepping over the trailing `-` lands exactly
        // at the end of the line (`:189-193`), and the autostop arm prints what
        // follows maxval as is (`x_cut.c:80-82`), equivalent to printing from 1
        // to the end of the line.
        assert_eq!(bytes_of("abcde", b"1-2-"), "abcde");
        // `0-` is an accepted shape too, but takes the zero sentence: the
        // leftover test comes first, the zero test after (`:195-198`).
        assert_eq!(parse_list(b"0-").err(), Some(ListError::IncludesZero));
    }

    #[test]
    fn test_parse_cut_args_faithful_getopt() {
        // Defaults: a field list with attached and next-word values.
        let a = parse_cut_args(&[b"-f", b"1-3", b"file"]).unwrap();
        assert_eq!(a.fields_list, Some(&b"1-3"[..]));
        assert_eq!(a.operands, vec![&b"file"[..]]);
        assert_eq!(a.delimiter, b'\t');
        // Attached list value (regression vs whole-token leak).
        assert_eq!(
            parse_cut_args(&[b"-f1-3", b"file"]).unwrap().fields_list,
            Some(&b"1-3"[..])
        );
        // KEY regression: a flag clustered with a value option (`-sf1-3`).
        let s = parse_cut_args(&[b"-sf1-3", b"file"]).unwrap();
        assert!(s.suppress);
        assert_eq!(s.fields_list, Some(&b"1-3"[..]));
        // `-sn` and `-ns` both set suppress (`-n` is the compat no-op).
        assert!(parse_cut_args(&[b"-sn", b"-f1"]).unwrap().suppress);
        assert!(parse_cut_args(&[b"-ns", b"-f1"]).unwrap().suppress);
        // `-d` attached and separate; `-b`/`-c` set the byte list.
        assert_eq!(parse_cut_args(&[b"-d:", b"-f1"]).unwrap().delimiter, b':');
        assert_eq!(
            parse_cut_args(&[b"-d", b",", b"-f1"]).unwrap().delimiter,
            b','
        );
        assert_eq!(
            parse_cut_args(&[b"-c1-2", b"file"]).unwrap().bytes_list,
            Some(&b"1-2"[..])
        );
        // `--` ends scanning; bare `-` is an operand.
        assert_eq!(
            parse_cut_args(&[b"-f1", b"--", b"-x"]).unwrap().operands,
            vec![&b"-x"[..]]
        );
        assert_eq!(
            parse_cut_args(&[b"-f1", b"-"]).unwrap().operands,
            vec![&b"-"[..]]
        );
        // An unknown letter or a missing value carries libc's line (the sentence
        // has a single home in `GetoptReport`).
        assert_eq!(
            parse_cut_args(&[b"-x", b"file"]).err(),
            Some(CutError::Getopt(GetoptReport::UnknownOption('x')))
        );
        assert_eq!(
            parse_cut_args(&[b"-f"]).err(),
            Some(CutError::Getopt(GetoptReport::MissingValue('f')))
        );
        assert_eq!(
            parse_cut_args(&[b"-d"]).err(),
            Some(CutError::Getopt(GetoptReport::MissingValue('d')))
        );
        // Two list kinds at once is an **outside-the-ring** post-check
        // (`cut.c:120-122`) and reports bare usage: no libc line in front. The
        // old spelling treated it as an in-ring `Conflict` reported first, so
        // `cut -b 1 -f 2 -x` reported the conflict instead of `-x`.
        assert_eq!(
            parse_cut_args(&[b"-b1", b"-f1"]).err(),
            Some(CutError::Usage)
        );
        assert_eq!(
            parse_cut_args(&[b"-f1", b"-b1"]).err(),
            Some(CutError::Usage)
        );
        // `-d`/`-s` only pair with `-f` (`dflag || sflag` at `cut.c:124`):
        // giving them alongside a byte list is a usage error too. The old
        // spelling accepted both silently.
        assert_eq!(
            parse_cut_args(&[b"-c1", b"-d,"]).err(),
            Some(CutError::Usage)
        );
        assert_eq!(
            parse_cut_args(&[b"-c1", b"-s"]).err(),
            Some(CutError::Usage)
        );
        // No list at all → the `(!cflag && !bflag)` arm.
        assert_eq!(parse_cut_args(&[b"-s"]).err(), Some(CutError::Usage));
        // C's `dchar = *optarg` takes only the first byte; extra bytes are
        // ignored without an error (`cut.c:101-102`), whereas the old spelling
        // treated the multi-byte case as a dedicated error.
        assert_eq!(parse_cut_args(&[b"-dab", b"-f1"]).unwrap().delimiter, b'a');
        assert_eq!(parse_cut_args(&[b"-d", b"", b"-f1"]).unwrap().delimiter, 0);
        // List validity is checked on the spot inside the option ring (C's
        // `get_list` sits in that case), so an invalid list reports before a
        // later unknown letter.
        assert_eq!(
            parse_cut_args(&[b"-f", b"0", b"-x"]).err(),
            Some(CutError::ZeroInList)
        );
        assert_eq!(
            parse_cut_args(&[b"-f", b"a", b"-x"]).err(),
            Some(CutError::IllegalList)
        );
    }

    /// The argv byte string must reach the library unfolded: `cut`'s `-d` and
    /// `column`'s `-s` are the same family — the value is not an echo slot but is
    /// used to split the **output content** (C's one-liner `dchar = *optarg`,
    /// `minix3/usr.bin/cut/cut.c:101-102`, takes the first byte). Under the text
    /// seam `-d<ff>` folds to the three bytes `ef bf bd` first, making the
    /// delimiter `ef`: on the same input "what should split does not, and what
    /// should not gets split". A reference build measured four shapes:
    /// `-d<ff> -f1` on `a<ff>b<TAB>z` returns `a`, `-f2` returns `b<TAB>z`;
    /// `-d<ff>` returns a line containing only the three U+FFFD bytes unchanged;
    /// the attached-value form `-d<ff>` matches the first. The operand leg here
    /// only reaches the `&str` mouths of opening the file and `warn` (registered
    /// as `ARGV-BYTE-PATH-IN-LIBS`), so what is pinned here is **whether the
    /// library can express a raw-byte file name** — the old `&[&str]` signature
    /// made that shape impossible to write.
    #[test]
    fn the_delimiter_and_the_operand_reach_the_library_as_raw_bytes() {
        let argv: &[&[u8]] = &[b"-d", b"\xff", b"-f", b"1", b"a\xffb"];
        let parsed = parse_cut_args(argv).unwrap();
        assert_eq!(parsed.delimiter, 0xff, "分隔符是 argv 里那一枚字节本身");
        assert_eq!(
            parsed.operands,
            vec![&b"a\xffb"[..]],
            "档名就是 argv 里那串字节，一字节都不许多"
        );

        let ranges = parse_list(b"1").unwrap();
        let mut out = [0u8; 16];
        let len = select_fields(b"a\xffb\tz", &ranges, parsed.delimiter, false, &mut out)
            .unwrap()
            .unwrap();
        assert_eq!(&out[..len], b"a", "真 ff 切得开");

        let whole = b"a\xef\xbf\xbdb";
        let len = select_fields(whole, &ranges, parsed.delimiter, false, &mut out)
            .unwrap()
            .unwrap();
        assert_eq!(
            &out[..len],
            &whole[..],
            "U+FFFD 的三枚字节不是分隔符，整行照搬"
        );

        let attached: &[&[u8]] = &[b"-d\xff", b"-f", b"1"];
        assert_eq!(
            parse_cut_args(attached).unwrap().delimiter,
            0xff,
            "值附在字母后也是同一枚字节"
        );
    }

    /// The letter slot must be taken **bytewise** in the library:
    /// `getopt.c:79`'s `optopt = *place++` takes one byte at a time and `:103`
    /// writes it back with `%c`, so `-é` (`c3 a9`) yields `char::from(0xc3)`
    /// rather than the whole character. This test and the one in `cut_face` pin
    /// one half each — **changing only one of them must still go red**: if the
    /// take reverts to whole characters per cluster only this one goes red first,
    /// and if message rendering reverts to per-`char` only the face side does
    /// (the same two-half pinning as the `comm`/`cmp` pairs). A reference build
    /// measured both `-é` and `-sé` as returning a single `c3` (`s` is one of the
    /// value-less letters in `"b:c:d:f:sn"`, so the cluster keeps eating into the
    /// first byte of `é`).
    #[test]
    fn the_unknown_letter_is_taken_bytewise_in_the_ring() {
        assert_eq!(
            parse_cut_args(&[b"-\xc3\xa9", b"a", b"b"]).err(),
            Some(CutError::Getopt(GetoptReport::UnknownOption('\u{00c3}'))),
            "`é` 是 c3 a9 两字节，libc 报第一字节"
        );
        // The legal letter inside the cluster is consumed first, and the report
        // is still the first byte after it.
        assert_eq!(
            parse_cut_args(&[b"-s\xc3\xa9", b"a", b"b"]).err(),
            Some(CutError::Getopt(GetoptReport::UnknownOption('\u{00c3}'))),
            "-s 后紧跟多字节字母"
        );
        // The ASCII half must not be affected.
        assert_eq!(
            parse_cut_args(&[b"-Z", b"a", b"b"]).err(),
            Some(CutError::Getopt(GetoptReport::UnknownOption('Z'))),
            "-Z"
        );
    }
}
