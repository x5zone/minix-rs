//! Columnation for `column`.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). Input
//! becomes entries: a line that is entirely blank is dropped, a last line
//! without its newline is dropped with a warning, and every other entry is
//! the whole line — leading blanks included (the C `input`,
//! `column.c:261-297`). Three layouts follow the same widths: `print` (one
//! entry per line) when the longest entry reaches the terminal width,
//! `r_columnate` (fill down the columns, `column.c:170-190`) by default,
//! and `c_columnate` (fill across rows, `column.c:141-167`) with `-x`.
//! Both columnate modes pad with tabs up to `TABROUND` boundaries
//! (`TABROUND(l) = (l + 8) & ~7`, column.c:59) and advance `endcol` by
//! the rounded maximum length per column.
//!
//! Entries, fields and widths are all **bytes**: C reads one byte at a time with
//! `fgetln`, measures length with `strlen`, and emits verbatim with
//! `printf("%s")`; the whole chain never decodes once. So a non-UTF-8 byte
//! neither makes its record disappear nor makes the padding width count one
//! unit too many.
//!
//! The terminal width comes from `-c` (default 80, column.c:68 — the C
//! queries the window size, which belongs to the terminal face). `-t`
//! selects the table form: entries split on the separator (default
//! `\t `, strtok run-collapsing semantics, `maketbl` column.c:223-247)
//! and each row prints its fields padded by `lens[col] - len[col] + 2`
//! spaces (column.c:249-251).

use alloc::vec::Vec;

use crate::GetoptReport;

/// Column options for one `column` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnOptions {
    /// Terminal width for layout decisions (`-c`; default 80).
    pub termwidth: usize,
    /// `-t`: table mode — split entries into fields on the separator.
    pub table: bool,
    /// `-x`: fill rows before columns.
    pub fill_rows_first: bool,
    /// `-s`: field separator for table mode (default space and tab).
    pub separator: Vec<u8>,
}

impl Default for ColumnOptions {
    fn default() -> Self {
        ColumnOptions {
            termwidth: 80,
            table: false,
            fill_rows_first: false,
            separator: b"\t ".to_vec(),
        }
    }
}

/// The result of parsing `column`'s argv: options plus file operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnArgs<'a> {
    pub options: ColumnOptions,
    pub operands: Vec<&'a [u8]>,
}

/// Splits the tail of `column`'s argv into [`ColumnArgs`] per BSD
/// `getopt("c:s:tx")` (`column.c:93`). `t`/`x` are valueless flags (clustered
/// verbatim, e.g. `-tx`/`-xt`); `c`/`s` take a value, either attached to the
/// letter (`-c100`/`-s,`) or taken from the next item (`-c 100`). Failures can
/// only be libc `getopt`'s two shapes, so the error payload is the crate-root
/// [`GetoptReport`] directly (one sentence source, rendered by the shell):
/// 1. an out-of-set letter [`GetoptReport::UnknownOption`] (`getopt.c:95-104`;
///    the letter of `--x` is `-`);
/// 2. a missing value after a value-taking option
///    [`GetoptReport::MissingValue`] (`getopt.c:132-136`, i.e. the line libc
///    itself prints before `column.c:107-109`'s `default: usage()`).
///
/// `--` ends the option loop without being an operand (`getopt.c:81-85` does
/// `++optind` first); a bare `-` or any non-option token ends the option loop
/// on arrival **and is not reordered** (`getopt.c:74-78`, `:87-92`), so it and
/// the tokens after it all become operands — `column` will `fopen` them as file
/// names (the old form kept scanning past a non-option token, so the `-t` in
/// `foo -t` was silently eaten as a flag while C opens the file named `-t`).
///
/// The `-c` width goes through [`crate::atoi`], verbatim the same shape as
/// `column.c:96`'s `atoi(optarg)`: non-digits, the empty string and negative
/// numbers are not errors. For a non-positive width, C's `maxlength >=
/// termwidth` test (`:131`) is **always true** when the entry pool is non-empty
/// (the longest entry is at least one byte, and rounding it up to the tab stop
/// gives 8), so the layout degrades to one entry per line; folding a negative
/// width to 0 here keeps `termwidth` a `usize` — the two are observationally
/// equivalent under the test above, and `numcols`' division is only reached
/// when `maxlength < termwidth`, where the width must be positive. The terminal
/// width itself (`ioctl(TIOCGWINSZ)` and `COLUMNS`) belongs to the terminal
/// face, not this layer.
pub fn parse_column_args<'a>(args: &'a [&'a [u8]]) -> Result<ColumnArgs<'a>, GetoptReport> {
    let mut options = ColumnOptions::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        if arg == b"--" {
            // `getopt.c:81-85`: -1 is returned only after `++optind`, so `--`
            // itself is not an operand.
            break args[index + 1..].to_vec();
        }
        if arg.len() < 2 || arg[0] != b'-' {
            // `getopt.c:74-78`: -1 is returned on the first non-option token
            // **without advancing** `optind`, so it and the tokens after it are
            // all operands.
            break args[index..].to_vec();
        }
        let mut pos = 1usize;
        while pos < arg.len() {
            match arg[pos] {
                b't' => options.table = true,
                b'x' => options.fill_rows_first = true,
                ch @ (b'c' | b's') => {
                    // The value is "the rest of this string after the `-` and the
                    // letter"; with nothing left, take the next token
                    // (`getopt.c:118-141`). Both forms consume this string to its
                    // end.
                    let value = if pos + 1 < arg.len() {
                        &arg[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(GetoptReport::MissingValue(ch as char));
                            }
                        }
                    };
                    if ch == b'c' {
                        options.termwidth = crate::atoi(value).max(0) as usize;
                    } else {
                        options.separator = value.to_vec();
                    }
                    pos = arg.len();
                }
                _ => {
                    // What is reported is **the first byte at this position**: C's
                    // `optopt` is a `char`, and `%c` prints exactly that one byte.
                    // It is taken by byte rather than by slicing the string
                    // because the cluster cursor `pos` is a **byte** index; for a
                    // non-ASCII letter it lands inside a multi-byte character,
                    // where `&arg[pos..]` is not a character boundary and slicing
                    // panics on the spot (the sibling `col` carries the same item,
                    // already registered).
                    return Err(GetoptReport::UnknownOption(char::from(arg[pos])));
                }
            }
            pos += 1;
        }
        index += 1;
    };
    Ok(ColumnArgs { options, operands })
}

/// Splits one input line into table fields (strtok semantics: leading
/// and repeated separators collapse, empty fields dropped).
///
/// The separator set is a set of **bytes**: what C hands `strtok` is a `char *`,
/// and `strspn` compares one byte at a time (`maketbl`'s
/// `strtok(p, separator)`, `column.c:224-225`). Testing by character would let a
/// lone continuation byte through — entries now enter the pool as bytes, so such
/// a byte really can occur.
fn split_fields<'a>(line: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    line.split(|byte| separator.contains(byte))
        .filter(|field| !field.is_empty())
        .collect()
}

/// What one input hands back: the entries that entered the pool, plus the number
/// of unterminated tail lines that C drops **and warns about**.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EntryPool {
    pub entries: Vec<Vec<u8>>,
    /// The number of tail lines without a newline (only 0 or 1 can occur). C's
    /// `input` first skips leading whitespace to test for a blank line, then
    /// fails `strchr(p, '\n')`, at which point it calls `warnx("line too long")`,
    /// sets `eval = 1` and **drops that whole line** (`column.c:277-283`) — so "the
    /// last line has no carriage return" is not a harmless shape: it loses one
    /// output line and changes the exit code.
    pub dropped_tail: usize,
}

/// Collects one source's lines into the entry pool: blank lines dropped, an
/// unterminated tail line dropped with a warning, and every kept entry the
/// **whole line** as read (the C `input` loop, `column.c:272-296`).
///
/// Skipping leading whitespace serves only two tests — "the whole line is
/// whitespace, hence blank" and "look for the newline starting here" — and what
/// enters the pool is `buf` itself, so an entry **keeps its leading whitespace**
/// (`column.c:287`'s `list[entries++] = buf`, while `:285`'s `len = p - buf` is
/// also measured from the start of the line, so `maxlength` counts the leading
/// whitespace too). The reference confirms this: the input `  lead`, in a layout
/// other than `-t`, prints those two spaces back verbatim.
///
/// The parameter is the entries themselves (bytes, not text — C's `input` reads
/// one byte at a time into `buf`, `column.c:272-296`). Read them as the output of
/// `split(b'\n')`: **the last element is the tail line without a newline**; when
/// the input ends with `\n` it is the empty string, and then there is no
/// unterminated line (`column.c:272`'s `fgetln` only hands out records that carry
/// a newline; the last segment is the residual truncated by EOF).
#[must_use]
pub fn scan_entries(lines: &[&[u8]]) -> EntryPool {
    let mut pool = EntryPool::default();
    let Some((tail, records)) = lines.split_last() else {
        return pool;
    };
    let unterminated = !tail.is_empty();
    let mut pending = records.to_vec();
    if unterminated {
        pending.push(tail);
    }
    for (index, line) in pending.iter().enumerate() {
        // Skip leading whitespace one run at a time (this toolchain's `&[u8]` has
        // no `trim_start_matches`, so walk it by hand). Which bytes count as
        // whitespace follows the C locale's `isspace` table: space, tab, newline,
        // vertical tab, form feed, carriage return — recognising only the first
        // two would take a vertical tab as part of the entry.
        let mut blank = 0usize;
        while matches!(
            line.get(blank),
            Some(&byte) if matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
        ) {
            blank += 1;
        }
        let head = &line[blank..];
        // The blank test comes before the newline test (C runs
        // `for (...; isspace) ;if (!*p) continue` first and calls `strchr`
        // after): a tail line that is entirely whitespace counts as blank, not as
        // a long line, and draws no warning at all.
        if head.is_empty() {
            continue;
        }
        if unterminated && index + 1 == pending.len() {
            pool.dropped_tail += 1;
            continue;
        }
        pool.entries.push((*line).to_vec());
    }
    pool
}

/// Lays the entries out and returns the output lines.
///
/// Table mode (`-t`) aligns fields into padded columns; plain mode
/// columnates the whole entries: down the columns by default, across
/// rows with `-x`.
pub fn columnate(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    // main() rounds the longest entry up to the tab stop first, then
    // dispatches (column.c:128-137); a length that reaches the terminal
    // width degrades to one entry per line.
    let maxlength = tab_round(maxlen(entries));
    if options.table {
        return table_mode(entries, options);
    }
    if maxlength >= options.termwidth {
        return entries.to_vec();
    }
    if options.fill_rows_first {
        c_columnate(entries, options)
    } else {
        r_columnate(entries, options)
    }
}

/// Table mode (`-t`): split fields, size each column by its longest
/// member, and emit every row with a two-space gap after each field.
fn table_mode(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    let separator = &options.separator;
    let rows: Vec<Vec<&[u8]>> = entries
        .iter()
        .map(|line| split_fields(line, separator))
        .collect();
    let numcols = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    let mut widths: Vec<usize> = alloc::vec![0; numcols];
    for row in &rows {
        for (index, field) in row.iter().enumerate() {
            if field.len() > widths[index] {
                widths[index] = field.len();
            }
        }
    }
    let mut out = Vec::new();
    for row in &rows {
        let mut line: Vec<u8> = Vec::new();
        let last = row.len().saturating_sub(1);
        for (index, field) in row.iter().enumerate() {
            line.extend_from_slice(field);
            // C's last column goes through `printf("%s\n", t->list[coloff])`
            // (`column.c:252`): no padding and no "trim the line end" step. So
            // trailing whitespace carried by the field itself — characters such as
            // carriage return that are not in the separator set — stays at the end
            // of the line; the old form padded and then called `trim_end`, eating
            // that carriage return too.
            if index < last {
                line.extend(core::iter::repeat_n(b' ', widths[index] - field.len() + 2));
            }
        }
        out.push(line);
    }
    out
}

/// Fills entries across rows (`-x`): `numcols` per row with tabs up to
/// the `TABROUND` boundary of each column (`c_columnate`,
/// column.c:141-167).
fn c_columnate(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    // The C returns early when there are no entries (column.c:126-128);
    // without that guard the width divisions below would be zero.
    if entries.is_empty() {
        return Vec::new();
    }
    let maxlength = tab_round(maxlen(entries));
    let numcols = options.termwidth / maxlength;
    let mut out = Vec::new();
    let mut line: Vec<u8> = Vec::new();
    let mut chcnt = 0usize;
    let mut col = 0usize;
    let mut endcol = maxlength;
    for (index, entry) in entries.iter().enumerate() {
        line.extend_from_slice(entry);
        chcnt += entry.len();
        // C's `if (!--entries) break;` comes before the padding: after the last
        // entry there is **no tab padding**, and it does not decide whether the
        // line closes (closing is decided by the `if (chcnt)` below).
        if index + 1 == entries.len() {
            break;
        }
        col += 1;
        if col == numcols {
            chcnt = 0;
            col = 0;
            endcol = maxlength;
            out.push(line);
            line = Vec::new();
        } else {
            while tab_round(chcnt) <= endcol {
                line.push(b'\t');
                chcnt = tab_round(chcnt);
            }
            endcol += maxlength;
        }
    }
    // `if (chcnt) putchar('\n')`: when the line was just taken in the previous
    // step, `chcnt` has already been cleared to 0, so no extra blank line is
    // handed out here.
    if chcnt != 0 {
        out.push(line);
    }
    out
}

/// Fills entries down the columns: `numcols` across, rows first.
fn r_columnate(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    // Same empty-input guard as `c_columnate` (column.c:126-128).
    if entries.is_empty() {
        return Vec::new();
    }
    let maxlength = tab_round(maxlen(entries));
    let numcols = options.termwidth / maxlength;
    let numrows = entries.len().div_ceil(numcols);
    let mut out = Vec::new();
    for row in 0..numrows {
        let mut line: Vec<u8> = Vec::new();
        let mut endcol = maxlength;
        let mut chcnt = 0usize;
        for col in 0..numcols {
            let index = row + col * numrows;
            let Some(entry) = entries.get(index) else {
                break;
            };
            line.extend_from_slice(entry);
            chcnt += entry.len();
            // Close the line when the next cell is out of range: C's
            // `if ((base += numrows) >= entries) break;` also comes before the
            // padding, so the line end carries no tab.
            if index + numrows >= entries.len() {
                break;
            }
            while tab_round(chcnt) <= endcol {
                line.push(b'\t');
                chcnt = tab_round(chcnt);
            }
            endcol += maxlength;
        }
        out.push(line);
    }
    out
}

fn maxlen(entries: &[Vec<u8>]) -> usize {
    entries.iter().map(|e| e.len()).max().unwrap_or(0)
}

/// `TABROUND(l)` (column.c:59): `(l + TAB) & ~(TAB - 1)` — round up to
/// the next multiple of eight; an already-aligned length advances a
/// full stop (which is what terminates the columnate tab loops).
fn tab_round(length: usize) -> usize {
    (length + 8) & !7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_mode_splits_and_pads_by_max_column_width() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let entries = vec![b"a\tbb\tccc".to_vec(), b"dddd\te\tff".to_vec()];
        let out = columnate(&entries, &options);
        // Each row: fields padded to the per-column max (4) plus a
        // two-space gap; the trailing gap is trimmed.
        // Per-column widths [4, 2, 3]; each field is followed by
        // (width - len + 2) spaces, the trailing run trimmed.
        assert_eq!(out[0], *b"a     bb  ccc");
        assert_eq!(out[1], *b"dddd  e   ff");
    }

    #[test]
    fn a_blank_line_is_any_run_of_isspace_bytes_not_just_space_and_tab() {
        // C's `input` first scans leading whitespace with `isspace`; reaching the
        // end of the string makes the line blank and silently dropped, so the
        // "tail line has no newline" test is not even reached (the order of
        // `column.c:274-278` is reproduced here). The fixture writes literal
        // bytes: vertical tab `0b`, form feed `0c` and carriage return `0d`, one
        // each, must all be dropped.
        let pool = scan_entries(&[b"\x0b\x0c", b"\r", b""]);
        assert!(pool.entries.is_empty(), "got {:?}", pool.entries);
        assert_eq!(pool.dropped_tail, 0, "整条空白不是残行，不该计数");
        // The other side as before: whitespace before real content is skipped as
        // a whole, and the entry itself is not changed by a single byte.
        let pool = scan_entries(&[b"\x0bx", b"y\r", b""]);
        assert_eq!(pool.entries, [b"\x0bx".to_vec(), b"y\r".to_vec()]);
    }

    #[test]
    fn test_table_mode_ignores_blank_lines() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        // The last element is the empty string: the segment `split('\n')` leaves
        // when the input ends with a newline, not a residual line.
        let pool = scan_entries(&[b"", b"x\ty", b""]);
        let out = columnate(&pool.entries, &options);
        // Blank lines are dropped (C `input`'s leading-whitespace test), so only
        // one padded line remains.
        assert_eq!(pool.dropped_tail, 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], *b"x  y");
    }

    /// An entry is a byte string: a non-UTF-8 byte neither leaves it out of the
    /// pool nor changes the layout width (the width follows `strlen`,
    /// `column.c:243`). The old form decoded UTF-8 once in the shell, and one bad
    /// byte made the whole file hand out **not a single entry**.
    #[test]
    fn a_byte_that_is_not_utf8_is_still_an_entry_and_its_layout_is_bytes() {
        let options = ColumnOptions::default();
        let pool = scan_entries(&[b"a\xffb", b"z", b""]);
        assert_eq!(pool.entries, vec![b"a\xffb".to_vec(), b"z".to_vec()]);
        assert_eq!(pool.dropped_tail, 0);
        assert_eq!(
            columnate(&pool.entries, &options),
            vec![b"a\xffb\tz".to_vec()]
        );
        // Table mode also pads by bytes: `a\xffb` is 3 bytes, so against `dddd`
        // it is one byte short and gets exactly one space.
        let table = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let pool = scan_entries(&[b"a\xffb\tq", b"z\tww", b""]);
        assert_eq!(
            columnate(&pool.entries, &table),
            vec![b"a\xffb  q".to_vec(), b"z    ww".to_vec()]
        );
    }

    /// The separator set is a set of **bytes** (what C hands `strtok` is that byte
    /// table): the value of `-s é` is `c3 a9`, so a lone `a9` inside an entry is a
    /// separator. Testing by character would let it through as an ordinary byte
    /// and the whole line would become one column.
    #[test]
    fn the_separator_is_a_set_of_bytes_not_a_set_of_characters() {
        let options = ColumnOptions {
            table: true,
            separator: "\u{e9}".as_bytes().to_vec(),
            ..ColumnOptions::default()
        };
        let entries = vec![b"a\xa9b".to_vec()];
        assert_eq!(columnate(&entries, &options), vec![b"a  b".to_vec()]);
    }

    /// The last column of table mode is emitted verbatim (`column.c:252`'s
    /// `printf("%s\n", …)`): a trailing carriage return therefore stays at the end
    /// of the line. The old form padded and then called `trim_end`, cutting it
    /// away too.
    #[test]
    fn a_trailing_carriage_return_survives_because_the_last_column_is_printed_raw() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let entries = vec![b"a\tb\r".to_vec(), b"zz\tq".to_vec()];
        assert_eq!(
            columnate(&entries, &options),
            vec![b"a   b\r".to_vec(), b"zz  q".to_vec()]
        );
    }

    #[test]
    fn test_table_mode_custom_separator() {
        let mut options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        options.separator = b":".to_vec();
        let entries = vec![b"1:b:22".to_vec()];
        let out = columnate(&entries, &options);
        assert_eq!(out.len(), 1);
        assert!(out[0].starts_with(b"1  b  22"));
    }

    /// What enters the pool is the whole line: leading whitespace stays in the
    /// entry (C stores `buf`, not the whitespace-skipped `p`), and only the
    /// "entirely whitespace = blank line" test uses the skipped segment.
    #[test]
    fn test_entry_collection_keeps_the_line_and_drops_blank_lines() {
        let pool = scan_entries(&[b"   one", b"", b"  two", b""]);
        assert_eq!(pool.entries, vec![b"   one".to_vec(), b"  two".to_vec()]);
        assert_eq!(pool.dropped_tail, 0);
    }

    /// A tail line without a newline: C calls it a "long line", drops the entry and
    /// records a warning (the reference behaves the same, `column.c:277-283`). A
    /// tail line that is entirely whitespace is still treated as blank, with no
    /// warning at all.
    #[test]
    fn test_unterminated_tail_line_is_dropped_and_counted() {
        let pool = scan_entries(&[b"one", b"two"]);
        assert_eq!(pool.entries, vec![b"one".to_vec()]);
        assert_eq!(pool.dropped_tail, 1);
        let blank_tail = scan_entries(&[b"one", b"   "]);
        assert_eq!(blank_tail.entries, vec![b"one".to_vec()]);
        assert_eq!(blank_tail.dropped_tail, 0);
        let empty = scan_entries(&[b""]);
        assert!(empty.entries.is_empty());
        assert_eq!(empty.dropped_tail, 0);
    }

    #[test]
    fn parse_flags_cluster_and_attached_values() {
        // Key regression: clusters and attached values are not mistaken for file
        // operands by the front door.
        let argv: &[&[u8]] = &[b"-tx", b"f"];
        let a = parse_column_args(argv).unwrap();
        assert!(a.options.table && a.options.fill_rows_first);
        assert_eq!(a.operands, vec![&b"f"[..]]);
        let argv: &[&[u8]] = &[b"-c100", b"-s,", b"f1", b"f2"];
        let b = parse_column_args(argv).unwrap();
        assert_eq!(b.options.termwidth, 100);
        assert_eq!(b.options.separator, b",".to_vec());
        assert_eq!(b.operands, vec![&b"f1"[..], &b"f2"[..]]);
        // A detached value is still taken from the next item as usual.
        let argv: &[&[u8]] = &[b"-c", b"120", b"-t", b"f"];
        let c = parse_column_args(argv).unwrap();
        assert_eq!(c.options.termwidth, 120);
        assert!(c.options.table);
    }

    /// The value of `-s` is the separator set itself, so it must come back **byte
    /// for byte**: C hands `optarg` to `strtok` verbatim (`column.c:224-225`), and
    /// that table is compared one byte at a time. Once the text seam folds,
    /// `from_utf8_lossy` turns a bare `ff` into the three bytes `ef bf bd`, which
    /// breaks two things at once — the real `ff` no longer splits, while the three
    /// U+FFFD bytes in the input are split as if they were separators. Measured
    /// against the reference `tmp/column-314/column`: under `-s<ff>`, `a<ff>b`
    /// splits into two columns and `a<ef bf bd>b` stays one line
    /// (`tmp/column-459/ab-measure.log`).
    #[test]
    fn the_separator_value_reaches_the_byte_set_one_byte_per_separator() {
        let argv: &[&[u8]] = &[b"-t", b"-s\xff"];
        let parsed = parse_column_args(argv).unwrap();
        assert_eq!(
            parsed.options.separator,
            vec![0xff],
            "分隔符集里就该是那一枚字节"
        );
        let entries = vec![b"a\xffb".to_vec()];
        assert_eq!(
            columnate(&entries, &parsed.options),
            vec![b"a  b".to_vec()],
            "裸字节切得开"
        );
        let lookalike = vec![b"a\xef\xbf\xbdb".to_vec()];
        assert_eq!(
            columnate(&lookalike, &parsed.options),
            lookalike,
            "U+FFFD 那三字节不是分隔符，不该被误切"
        );
    }

    #[test]
    fn parse_operand_discrimination_without_reordering() {
        // A bare `-` and non-dash tokens are operands; everything after `--` is an
        // operand (`--` itself is not).
        let argv: &[&[u8]] = &[b"-", b"f2"];
        let a = parse_column_args(argv).unwrap();
        assert_eq!(a.operands, vec![&b"-"[..], &b"f2"[..]]);
        let argv: &[&[u8]] = &[b"-t", b"--", b"-c", b"f"];
        let b = parse_column_args(argv).unwrap();
        assert!(b.options.table);
        assert_eq!(b.operands, vec![&b"-c"[..], &b"f"[..]]); // `-c` after `--` is a file name
        // No reordering: on the first non-option token the option loop ends on the
        // spot, and the tokens after it are all operands.
        let argv: &[&[u8]] = &[b"f", b"-t"];
        let c = parse_column_args(argv).unwrap();
        assert_eq!(c.operands, vec![&b"f"[..], &b"-t"[..]]);
        assert!(!c.options.table);
    }

    /// The value of `-c` goes through C's `atoi`: non-digits, 0 and negative
    /// numbers are not errors, they just collapse the width to "one entry per
    /// line".
    #[test]
    fn parse_width_is_atoi_not_a_strict_number() {
        let cases: [(&[&[u8]], usize); 5] = [
            (&[&b"-c"[..], &b"0"[..]], 0usize),
            (&[&b"-c"[..], &b"abc"[..]], 0),
            (&[&b"-c"[..], &b"-5"[..]], 0),
            (&[&b"-c"[..], &b"20x"[..]], 20),
            (&[&b"-c"[..], &b" 30"[..]], 30),
        ];
        for (argv, width) in cases {
            assert_eq!(parse_column_args(argv).unwrap().options.termwidth, width);
        }
    }

    #[test]
    fn parse_error_faces_carry_the_libc_letters() {
        assert_eq!(
            parse_column_args(&[b"-z", b"f"]).err(),
            Some(GetoptReport::UnknownOption('z'))
        ); // out of set
        assert_eq!(
            parse_column_args(&[b"-tZ"]).err(),
            Some(GetoptReport::UnknownOption('Z'))
        ); // out-of-set letter inside a cluster
        assert_eq!(
            parse_column_args(&[b"--x"]).err(),
            Some(GetoptReport::UnknownOption('-'))
        ); // the letter of `--x` is `-` (getopt.c:95-104)
        assert_eq!(
            parse_column_args(&[b"-c"]).err(),
            Some(GetoptReport::MissingValue('c'))
        ); // missing value
        assert_eq!(
            parse_column_args(&[b"-t", b"-s"]).err(),
            Some(GetoptReport::MissingValue('s'))
        );
    }

    /// The cluster cursor is a **byte** index, so for a non-ASCII letter it lands
    /// inside a multi-byte character: the reported letter is that one byte, while
    /// slicing the string would panic on the spot (`&arg[pos..]` is not on a
    /// character boundary).
    #[test]
    fn a_non_ascii_cluster_letter_reports_a_byte_without_panicking() {
        // `-té`: `t` is a valid flag, the cursor reaches the first byte of `é`
        // (0xC3), and that byte is what gets reported.
        assert_eq!(
            parse_column_args(&[b"-t\xc3\xa9"]).err(),
            Some(GetoptReport::UnknownOption('\u{c3}'))
        );
        // Landing on a continuation byte does not panic either.
        assert_eq!(
            parse_column_args(&[b"-tx\xc3\xa9"]).err(),
            Some(GetoptReport::UnknownOption('\u{c3}'))
        );
        // A multi-byte value after a value-taking letter is a valid optarg (the
        // value is sliced from a character boundary).
        assert_eq!(
            parse_column_args(&[b"-s\xc3\xa9", b"f"])
                .unwrap()
                .options
                .separator,
            "é".as_bytes().to_vec()
        );
    }
}
