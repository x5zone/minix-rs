//! The three-way line merge behind `comm`.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 64-160: both operand files must already be sorted; the merge walks
//! them in lockstep — equal lines print in column three, the smaller
//! line prints in its own column and only that file advances — and at
//! either end of input the remaining lines drain into the surviving
//! column. Columns carry tab offsets: one tab for column two, two for
//! column three (comm.c:56, 105-110). `-1`/`-2`/`-3` suppress a column;
//! `-f` compares case-insensitively (`strcasecmp`). Both files always
//! reach end of input (the loop reads on).

/// Column suppression flags (`-1`/`-2`/`-3`) and the `-f` fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommOptions {
    /// Suppress column one (lines only in the first file).
    pub suppress1: bool,
    /// Suppress column two (lines only in the second file).
    pub suppress2: bool,
    /// Suppress column three (common lines).
    pub suppress3: bool,
    /// `-f`: compare case-insensitively (C: `strcasecmp`).
    pub fold: bool,
}

/// One output line: which column it belongs to and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommLine<'a> {
    /// 1, 2, or 3 — the column the line prints in.
    pub column: u8,
    /// The line itself.
    pub text: &'a str,
}

use alloc::vec::Vec;

/// Merges two sorted line lists. Lines must be newline-stripped; both
/// lists are consumed to the end regardless of order violations (the C
/// assumes sorted input and simply advances the smaller side).
pub fn comm<'a>(
    a: &'a [&'a str],
    b: &'a [&'a str],
    options: &CommOptions,
) -> Vec<CommLine<'a>> {
    let mut out = Vec::new();
    let mut ia = 0usize;
    let mut ib = 0usize;
    while ia < a.len() || ib < b.len() {
        if ia >= a.len() {
            push(&mut out, 2, options, b[ib]);
            ib += 1;
            continue;
        }
        if ib >= b.len() {
            push(&mut out, 1, options, a[ia]);
            ia += 1;
            continue;
        }
        let ordering = line_cmp(a[ia], b[ib], options.fold);
        match ordering {
            core::cmp::Ordering::Equal => {
                push(&mut out, 3, options, a[ia]);
                ia += 1;
                ib += 1;
            }
            core::cmp::Ordering::Less => {
                push(&mut out, 1, options, a[ia]);
                ia += 1;
            }
            core::cmp::Ordering::Greater => {
                push(&mut out, 2, options, b[ib]);
                ib += 1;
            }
        }
    }
    out
}

fn push<'a>(out: &mut Vec<CommLine<'a>>, column: u8, options: &CommOptions, text: &'a str) {
    let allowed = match column {
        1 => !options.suppress1,
        2 => !options.suppress2,
        _ => !options.suppress3,
    };
    if allowed {
        out.push(CommLine { column, text });
    }
}

/// The comparison: case-folded bytes with `-f`, plain bytes otherwise
/// (the C's `strcoll` is byte order in the C locale).
fn line_cmp(a: &str, b: &str, fold: bool) -> core::cmp::Ordering {
    if fold {
        a.to_lowercase().cmp(&b.to_lowercase())
    } else {
        a.cmp(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(result: &[CommLine<'_>]) -> Vec<(u8, String)> {
        result
            .iter()
            .map(|line| (line.column, line.text.to_string()))
            .collect()
    }

    #[test]
    fn test_three_way_merge() {
        let a = ["apple", "banana", "cherry"];
        let b = ["banana", "date", "fig"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, "apple".to_string()),
                (3, "banana".to_string()),
                (1, "cherry".to_string()),
                (2, "date".to_string()),
                (2, "fig".to_string()),
            ]
        );
    }

    #[test]
    fn test_suppression_flags() {
        let a = ["common", "only1"];
        let b = ["common", "only2"];
        let options = CommOptions {
            suppress1: true,
            suppress3: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![(2, "only2".to_string())]
        );
    }

    #[test]
    fn test_fold_compares_case_insensitively() {
        let a = ["Apple"];
        let b = ["apple", "zebra"];
        let options = CommOptions {
            fold: true,
            ..CommOptions::default()
        };
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (3, "Apple".to_string()),
                (2, "zebra".to_string()),
            ]
        );
    }

    #[test]
    fn test_drains_surviving_file_at_either_end() {
        let a = ["early"];
        let b = ["mid1", "mid2", "mid3"];
        let options = CommOptions::default();
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (1, "early".to_string()),
                (2, "mid1".to_string()),
                (2, "mid2".to_string()),
                (2, "mid3".to_string()),
            ]
        );
        let a = ["late1", "late2"];
        let b = ["early"];
        assert_eq!(
            texts(&comm(&a, &b, &options)),
            vec![
                (2, "early".to_string()),
                (1, "late1".to_string()),
                (1, "late2".to_string()),
            ]
        );
    }
}
