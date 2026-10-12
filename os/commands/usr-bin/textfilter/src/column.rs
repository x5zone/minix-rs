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

/// `column` argv 解析结果：选项 + 文件操数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnArgs<'a> {
    pub options: ColumnOptions,
    pub operands: Vec<&'a str>,
}

/// `column` 参数解析失败（C `getopt` 的 `?`/default → `usage()`，`column.c:107-109`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnError {
    /// 表外选项字母、选项缺值、或 `-c` 宽度非正。
    Usage,
}

/// 把 `column` 的 argv 尾部按 BSD `getopt("c:s:tx")`（`column.c:93`）拆成 [`ColumnArgs`]。
/// `t`/`x` 是无值旗标（逐字聚簇，如 `-tx`/`-xt`）；`c`/`s` 带值，值附于字母后
/// （`-c100`/`-s,`）或取下一项（`-c 100`）。`--` 结束选项扫描（BSD getopt），裸 `-`
/// 与非连字符项都是文件操数；集外字母/缺值→[`ColumnError::Usage`]。旧薄壳只比整个 token
/// （`arg == "-t"`），故 C 合法的聚簇 `-tx` 与附值 `-c100`/`-s,` 会静默落进「文件操数」
/// 分支去 `open("-c100")`——本件把选项与操数正确区分。
/// 与 C 的一处差异：`-c` 的值 C 走 `atoi`（无守卫，`:96`）；本层要求解析为 `>0` 的
/// `usize`（否则 usage），避开宽度 0 在列布局里的已知陷阱（防御性，非语义发明）。
pub fn parse_column_args<'a>(args: &'a [&'a str]) -> Result<ColumnArgs<'a>, ColumnError> {
    let mut options = ColumnOptions::default();
    let mut operands: Vec<&'a str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg == "--" {
            operands.extend_from_slice(&args[i + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        // 裸 `-`（长度 1）与非连字符项都是文件操数；长度 >1 且以 `-` 开头才是选项串。
        if bytes.len() < 2 || bytes[0] != b'-' {
            operands.push(arg);
            i += 1;
            continue;
        }
        // 逐字符扫描一个选项串（BSD getopt 的聚簇）；取到需值的选项时，值可能是本 token
        // 的剩余部分（附值），否则是下一个 token。
        let mut pos = 1usize;
        while pos < arg.len() {
            match arg.as_bytes()[pos] {
                b't' => options.table = true,
                b'x' => options.fill_rows_first = true,
                ch @ (b'c' | b's') => {
                    let rest = &arg[pos + 1..];
                    let value = if !rest.is_empty() {
                        rest
                    } else {
                        i += 1;
                        args.get(i).ok_or(ColumnError::Usage)?
                    };
                    if ch == b'c' {
                        let width: usize = value.parse().map_err(|_| ColumnError::Usage)?;
                        if width == 0 {
                            return Err(ColumnError::Usage);
                        }
                        options.termwidth = width;
                    } else {
                        options.separator = value.to_string();
                    }
                    pos = arg.len(); // 值已消费（附值或下一项），本选项串结束
                    break;
                }
                _ => return Err(ColumnError::Usage),
            }
            pos += 1;
        }
        i += 1;
    }
    Ok(ColumnArgs { options, operands })
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
    // The C returns early when there are no entries (column.c:126-128);
    // without that guard the width divisions below would be zero.
    if entries.is_empty() {
        return Vec::new();
    }
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
    // Same empty-input guard as `c_columnate` (column.c:126-128).
    if entries.is_empty() {
        return Vec::new();
    }
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
        let entries = vec!["a\tbb\tccc".to_string(), "dddd\te\tff".to_string()];
        let out = columnate(&entries, &options);
        // Each row: fields padded to the per-column max (4) plus a
        // two-space gap; the trailing gap is trimmed.
        // Per-column widths [4, 2, 3]; each field is followed by
        // (width - len + 2) spaces, the trailing run trimmed.
        assert_eq!(out[0], "a     bb  ccc");
        assert_eq!(out[1], "dddd  e   ff");
    }

    #[test]
    fn test_table_mode_ignores_blank_lines() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let lines = ["", "x\ty"];
        let entries = parse_entries(&lines);
        let out = columnate(&entries, &options);
        // parse_entries skips the blank line (mirroring the C input
        // face), so one padded line comes out.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "x  y");
    }

    #[test]
    fn test_table_mode_custom_separator() {
        let mut options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        options.separator = ":".to_string();
        let entries = vec!["1:b:22".to_string()];
        let out = columnate(&entries, &options);
        assert_eq!(out.len(), 1);
        assert!(out[0].starts_with("1  b  22"));
    }

    #[test]
    fn test_entry_collection_strips_leading_blanks_and_skips_blank_lines() {
        let entries = parse_entries(&["   one", "", "  two"]);
        assert_eq!(entries, vec!["one", "two"]);
    }

    #[test]
    fn parse_flags_cluster_and_attached_values() {
        // 关键回归：聚簇与附值不被旧前门当文件操数。
        let a = parse_column_args(&["-tx", "f"]).unwrap();
        assert!(a.options.table && a.options.fill_rows_first);
        assert_eq!(a.operands, vec!["f"]);
        let b = parse_column_args(&["-c100", "-s,", "f1", "f2"]).unwrap();
        assert_eq!(b.options.termwidth, 100);
        assert_eq!(b.options.separator, ",");
        assert_eq!(b.operands, vec!["f1", "f2"]);
        // 分离值仍正确。
        let c = parse_column_args(&["-c", "120", "-t", "f"]).unwrap();
        assert_eq!(c.options.termwidth, 120);
        assert!(c.options.table);
    }

    #[test]
    fn parse_operand_discrimination_and_usage() {
        // 裸 `-` 与非 dash 是操数；`--` 后全为操数；集外字母/缺值/非正宽度 → Usage。
        let a = parse_column_args(&["-", "f2"]).unwrap();
        assert_eq!(a.operands, vec!["-", "f2"]);
        let b = parse_column_args(&["-t", "--", "-c", "f"]).unwrap();
        assert!(b.options.table);
        assert_eq!(b.operands, vec!["-c", "f"]); // `--` 后的 `-c` 是文件名字串
        assert_eq!(
            parse_column_args(&["-z", "f"]).err(),
            Some(ColumnError::Usage)
        );
        assert_eq!(parse_column_args(&["-c"]).err(), Some(ColumnError::Usage)); // 缺值
        assert_eq!(
            parse_column_args(&["-c", "0", "f"]).err(),
            Some(ColumnError::Usage)
        ); // 宽度 0
    }
}
