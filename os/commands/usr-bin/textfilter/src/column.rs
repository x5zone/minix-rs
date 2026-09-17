//! Columnation for `column`.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). Input
//! becomes entries: leading whitespace is skipped, blank lines are
//! dropped, and the entry is the rest of the line (column.c's `input`,
//! lines 173-200). Three layouts follow the same widths: `print` (one
//! entry per line) when the longest entry reaches the terminal width,
//! `r_columnate` (fill down the columns, column.c:181-199) by default,
//! and `c_columnate` (fill across rows, column.c:141-165) with `-x`.
//! Both columnate modes pad with tabs up to `TABROUND` boundaries
//! (`TABROUND(l) = (l + 8) & ~7`, column.c:59) and advance `endcol` by
//! the rounded maximum length per column.
//!
//! The terminal width comes from `-c` (default 80, column.c:68 — the C
//! queries the window size, which belongs to the terminal face). `-t`
//! selects the table form: entries split on the separator (default
//! `\t `, strtok run-collapsing semantics, `maketbl` column.c:211-238)
//! and each row prints its fields padded by `lens[col] - len[col] + 2`
//! spaces (column.c:252-256).

use alloc::string::{String, ToString};
use alloc::vec::{self, Vec};

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
    pub separator: String,
}

impl Default for ColumnOptions {
    fn default() -> Self {
        ColumnOptions {
            termwidth: 80,
            table: false,
            fill_rows_first: false,
            separator: "\t ".to_string(),
        }
    }
}

/// Splits one input line into table fields (strtok semantics: leading
/// and repeated separators collapse, empty fields dropped).
fn split_fields<'a>(line: &'a str, separator: &str) -> Vec<&'a str> {
    line.split(|ch: char| separator.contains(ch))
        .filter(|field| !field.is_empty())
        .collect()
}

/// Parses entries from input lines. Blank lines are skipped; leading
/// blanks are stripped (the C `input` loop, column.c:180-200).
pub fn parse_entries(lines: &[&str]) -> Vec<String> {
    let mut entries = Vec::new();
    for line in lines {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if !trimmed.is_empty() {
            entries.push(trimmed.to_string());
        }
    }
    entries
}

/// Lays the entries out and returns the output lines.
///
/// Table mode (`-t`) aligns fields into padded columns; plain mode
/// columnates the whole entries: down the columns by default, across
/// rows with `-x`.
pub fn columnate(entries: &[String], options: &ColumnOptions) -> Vec<String> {
    if options.table {
        table_mode(entries, options)
    } else if options.fill_rows_first {
        c_columnate(entries, options)
    } else {
        r_columnate(entries, options)
    }
}

/// Table mode (`-t`): split fields, size each column by its longest
/// member, and emit every row with a two-space gap after each field.
fn table_mode(entries: &[String], options: &ColumnOptions) -> Vec<String> {
    let rows: Vec<Vec<&str>> = entries
        .iter()
        .map(|line| split_fields(line, &options.separator))
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
        let mut line = String::new();
        for (index, field) in row.iter().enumerate() {
            line.push_str(field);
            let gap = widths[index] - field.len() + 2;
            for _ in 0..gap {
                line.push(' ');
            }
        }
        out.push(line.trim_end().to_string());
    }
    out
}

/// Fills entries across rows (`-x`): `numcols` per row with tabs up to
/// the `TABROUND` boundary of each column (`c_columnate`,
/// column.c:141-165).
fn c_columnate(entries: &[String], options: &ColumnOptions) -> Vec<String> {
    let maxlength = tab_round(maxlen(entries));
    let numcols = options.termwidth / maxlength;
    let mut out = Vec::new();
    let mut chcnt = 0usize;
    let mut col = 0usize;
    let mut endcol = maxlength;
    for entry in entries {
        out.push(entry.clone());
        chcnt += entry.len();
        col += 1;
        if col == numcols {
            chcnt = 0;
            col = 0;
            endcol = maxlength;
            out.push("\n".to_string());
        } else {
            while tab_round(chcnt) <= endcol {
                out.push('\t'.to_string());
                chcnt = tab_round(chcnt);
            }
            endcol += maxlength;
        }
    }
    if chcnt != 0 {
        out.push('\n'.to_string());
    }
    out
}

/// Fills entries down the columns: `numcols` across, rows first.
fn r_columnate(entries: &[String], options: &ColumnOptions) -> Vec<String> {
    let maxlength = tab_round(maxlen(entries));
    let numcols = options.termwidth / maxlength;
    let numrows = entries.len().div_ceil(numcols);
    let mut out = Vec::new();
    for row in 0..numrows {
        let mut endcol = maxlength;
        let mut chcnt = 0usize;
        for col in 0..numcols {
            let index = row + col * numrows;
            match entries.get(index) {
                Some(entry) => {
                    out.push(entry.clone());
                    chcnt += entry.len();
                }
                None => break,
            }
            while tab_round(chcnt) <= endcol {
                out.push('\t'.to_string());
                chcnt = tab_round(chcnt);
            }
            endcol += maxlength;
        }
        out.push("\n".to_string());
    }
    out
}

fn maxlen(entries: &[String]) -> usize {
    entries.iter().map(|e| e.len()).max().unwrap_or(0)
}

/// `TABROUND(l)` (column.c:59): round a length up to the next multiple
/// of eight.
fn tab_round(length: usize) -> usize {
    (length + 7) & !7
}
