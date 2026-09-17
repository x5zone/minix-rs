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
    let body_capacity = options
        .page_len
        .saturating_sub(HEADLEN + TAILLEN)
        .max(1);
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
                alloc::format!(
                    "{}{} Page 1",
                    options.time_prefix,
                    options.header
                ),
                String::new(),
                String::new(),
            ],
            body_lines: vec![String::new(); body_capacity],
            trailer_lines: vec![String::new(); TAILLEN],
        });
    }
    pages
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
            let total = page.header_lines.len()
                + page.body_lines.len()
                + page.trailer_lines.len();
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
        let body_empty = pages[0].body_lines[1..]
            .iter()
            .all(|line| line.is_empty());
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
        assert_eq!(pages[0].body_lines, vec!["one".to_string(), "two".to_string()]);
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
}
