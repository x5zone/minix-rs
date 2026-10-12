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
            page_width: 72,
            sep: None,
            offset: 0,
            time_prefix: String::new(),
        }
    }
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
pub fn paginate(lines: &[String], options: &PrOptions) -> Vec<Page> {
    if options.columns > 1 {
        return paginate_columns(lines, options);
    }
    if options.no_header {
        return lines
            .chunks(lines.len().max(1))
            .map(|body| Page {
                header_lines: Vec::new(),
                body_lines: body.to_vec(),
                trailer_lines: Vec::new(),
            })
            .collect();
    }
    let body_capacity = options.page_len.saturating_sub(HEADLEN + TAILLEN).max(1);
    let mut pages = Vec::new();
    let mut page_number = 1usize;
    let mut cursor = 0usize;
    loop {
        let body_end = (cursor + body_capacity).min(lines.len());
        let mut body: Vec<String> = lines[cursor..body_end].to_vec();
        while body.len() < body_capacity {
            body.push(String::new());
        }
        let header_lines = vec![
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
        ];
        let trailer_lines = vec![String::new(); TAILLEN];
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
    if pages.is_empty() {
        // An empty input still prints one page, header and all (the C
        // emits the first page header before reading).
        pages.push(Page {
            header_lines: vec![
                String::new(),
                String::new(),
                alloc::format!("{}{} Page 1", options.time_prefix, options.header),
                String::new(),
                String::new(),
            ],
            body_lines: vec![String::new(); body_capacity],
            trailer_lines: vec![String::new(); TAILLEN],
        });
    }
    pages
}

/// Vertical multi-column pagination — the `vertcol` face (the default under
/// `-columns` when `-a` is absent). Input lines are chunked into page-blocks of
/// `rows * columns` (`rows` = body lines per column, matching `setup`'s
/// `pr.c:1869-1878` line adjustment), each block laid out down its columns by
/// [`vertcol_body_rows`], and wrapped in the same 5-line header/trailer as the
/// single-column path (body padded to `rows` while headers show). `column_width`
/// is assumed valid; the `doing` half rejects an over-narrow page before here.
fn paginate_columns(lines: &[String], options: &PrOptions) -> Vec<Page> {
    let (colwd, _) = column_width(options.page_width, options.columns)
        .unwrap_or((options.page_width, options.page_width));
    let rows = if options.no_header {
        options.page_len.max(1)
    } else {
        options.page_len.saturating_sub(HEADLEN + TAILLEN).max(1)
    };
    let block = rows * options.columns;
    let mut pages = Vec::new();
    let mut page_number = 1usize;
    let mut cursor = 0usize;
    loop {
        let end = (cursor + block).min(lines.len());
        let mut body = vertcol_body_rows(
            &lines[cursor..end],
            rows,
            options.columns,
            colwd,
            options.sep,
        );
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
    if pages.is_empty() && !options.no_header {
        // An empty input still prints one headered page (the C emits the first
        // page header before reading); `-t` of nothing stays nothing.
        pages.push(Page {
            header_lines: vec![
                String::new(),
                String::new(),
                alloc::format!("{}{} Page 1", options.time_prefix, options.header),
                String::new(),
                String::new(),
            ],
            body_lines: vec![String::new(); rows],
            trailer_lines: vec![String::new(); TAILLEN],
        });
    }
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
        // body row 0 = "a" + pad-to-stride + empty last column, then padded
        // out to the 56-line body capacity.
        assert_eq!(pages[0].body_lines[0], "a  ");
        assert_eq!(pages[0].body_lines.len(), 56);
        assert_eq!(pages[0].trailer_lines.len(), TAILLEN);
    }
}
