//! `join` deciding half: merge two field-structured inputs on a key field.
//!
//! Ground truth: `minix3/usr.bin/join/join.c` — inputs are read grouped by
//! their (equal) join field (`slurp`, lines 300-403), the two files are
//! merged key by key (`cmp`/`joinlines`, lines 405-450), and each matched pair
//! prints the join field then the remaining fields of file 1 then of file 2
//! (`outtwoline` default branch, lines 476-489); unmatched lines print all
//! their own fields when `-a` asks (`outoneline`, lines 447-466). Fields split
//! on runs of `tabchar` with empties skipped (`spans`, default on).
//!
//! Wired here: the default join (field 1 both sides), `-t` (delimiter +
//! output separator = that byte), `-1`/`-2` (per-file join field), and `-a`
//! (unmatched lines). Declared residual: the `-o` output-field list, `-j`,
//! `-v` (suppress the common field / report only unpairables), `-e` (empty
//! replacement) and the obsolete one-argument form — these ride the next
//! `join` slice; the caller supplies options it does not set to the defaults.
//!
//! Pure: takes the two inputs as byte-line slices and returns the output
//! lines, so the merge is unit-tested without files or the IPC layer.

use crate::TextError;
use alloc::vec::Vec;

/// Which side an output-field entry draws from (the `F` of `join -o F.N`),
/// or the join field itself (`0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The join field (always taken from file 1's key column).
    Join,
    /// A field of file 1.
    One,
    /// A field of file 2.
    Two,
}

/// One `join -o` output field: a `Which` and a 0-based field index (unused
/// for `Join`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OField {
    pub which: Which,
    pub idx: usize,
}

/// Join options, mirroring the C globals `join` carries per run.
#[derive(Debug, Clone)]
pub struct JoinOptions {
    /// Field index (0-based) that keys file 1 (`-1`, minus one).
    pub field1: usize,
    /// Field index that keys file 2 (`-2`, minus one).
    pub field2: usize,
    /// Delimiter byte: both the split character and the output separator
    /// (`-t`; the C default is `" \t"`, whose first byte — a space — also
    /// separates output). Modelled as one byte; the default is space with
    /// tabs treated as separators too via `collapse`.
    pub delim: u8,
    /// Emit unpaired lines from file 1 (`-a 1`).
    pub unmatched1: bool,
    /// Emit unpaired lines from file 2 (`-a 2`).
    pub unmatched2: bool,
    /// Explicit output field list (`-o`); empty selects the default layout
    /// (join field, then file 1's rest, then file 2's rest).
    pub output_fields: Vec<OField>,
    /// Replacement for a missing/unpaired field (`-e`); `None` leaves it blank.
    pub empty: Option<Vec<u8>>,
}

impl Default for JoinOptions {
    fn default() -> Self {
        JoinOptions {
            field1: 0,
            field2: 0,
            delim: b' ',
            unmatched1: false,
            unmatched2: false,
            output_fields: Vec::new(),
            empty: None,
        }
    }
}

/// Parse a `join -o` field list (`"1.2,2.3,0"`) into entries; tokens are
/// comma/space/tab separated (the C `strsep(", \t")`), `0` is the join field,
/// `F.N` (1-based) a field of file `F`; malformed or zero field numbers are
/// `EINVAL` (mirroring `fieldarg`'s "field numbers are 1 based").
pub fn parse_olist(spec: &str) -> Result<Vec<OField>, TextError> {
    let mut fields = Vec::new();
    for token in spec.split([',', ' ', '\t']) {
        if token.is_empty() {
            continue;
        }
        if token == "0" {
            fields.push(OField { which: Which::Join, idx: 0 });
            continue;
        }
        let mut parts = token.splitn(2, '.');
        let file = parts.next().unwrap_or("");
        let num = parts.next().ok_or(TextError::InvalidArgument)?;
        let which = match file {
            "1" => Which::One,
            "2" => Which::Two,
            _ => return Err(TextError::InvalidArgument),
        };
        let n: usize = num.parse().map_err(|_| TextError::InvalidArgument)?;
        if n == 0 {
            return Err(TextError::InvalidArgument);
        }
        fields.push(OField { which, idx: n - 1 });
    }
    Ok(fields)
}

/// Split a line into fields on runs of space/tab (the C default `tabchar`
/// `" \t"` with `spans`), or on the single `-t` byte; empty runs are skipped.
fn split_fields(line: &[u8], delim: u8) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = 0usize;
    for i in 0..=line.len() {
        let is_delim =
            i < line.len() && (line[i] == delim || (delim == b' ' && line[i] == b'\t'));
        if is_delim {
            if i > start {
                fields.push(&line[start..i]);
            }
            start = i + 1;
        } else if i == line.len() && i > start {
            fields.push(&line[start..i]);
        }
    }
    fields
}

/// The key of a field vector at index `f` (empty when the line is too short).
fn key<'l>(fields: &[&'l [u8]], f: usize) -> &'l [u8] {
    fields.get(f).copied().unwrap_or(b"")
}

/// Join `f1` and `f2` (byte-line slices, each already newline-stripped) into
/// output lines. Inputs are assumed sorted by their join field (as `join(1)`
/// requires); equal keys cross-multiply, the lesser key advances alone.
pub fn join_lines(f1: &[&[u8]], f2: &[&[u8]], opts: &JoinOptions) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < f1.len() && j < f2.len() {
        let g1 = group_end(f1, i, opts.field1, opts.delim);
        let g2 = group_end(f2, j, opts.field2, opts.delim);
        let k1 = key(&split_fields(f1[i], opts.delim), opts.field1);
        let k2 = key(&split_fields(f2[j], opts.delim), opts.field2);
        match k1.cmp(k2) {
            core::cmp::Ordering::Equal => {
                for la in f1[i..g1].iter().copied() {
                    let fa = split_fields(la, opts.delim);
                    for lb in f2[j..g2].iter().copied() {
                        let fb = split_fields(lb, opts.delim);
                        out.push(twoline(&fa, &fb, opts));
                    }
                }
                i = g1;
                j = g2;
            }
            core::cmp::Ordering::Less => {
                if opts.unmatched1 {
                    for la in f1[i..g1].iter().copied() {
                        out.push(onlyline(&split_fields(la, opts.delim), true, opts));
                    }
                }
                i = g1;
            }
            core::cmp::Ordering::Greater => {
                if opts.unmatched2 {
                    for lb in f2[j..g2].iter().copied() {
                        out.push(onlyline(&split_fields(lb, opts.delim), false, opts));
                    }
                }
                j = g2;
            }
        }
    }
    // A file may have trailing groups past the other's end.
    if opts.unmatched1 {
        while i < f1.len() {
            let g = group_end(f1, i, opts.field1, opts.delim);
            for la in f1[i..g].iter().copied() {
                out.push(onlyline(&split_fields(la, opts.delim), true, opts));
            }
            i = g;
        }
    }
    if opts.unmatched2 {
        while j < f2.len() {
            let g = group_end(f2, j, opts.field2, opts.delim);
            for lb in f2[j..g].iter().copied() {
                out.push(onlyline(&split_fields(lb, opts.delim), false, opts));
            }
            j = g;
        }
    }
    out
}

/// Index one past the run of lines from `start` sharing the key of `start`.
fn group_end(lines: &[&[u8]], start: usize, field: usize, delim: u8) -> usize {
    let base = key(&split_fields(lines[start], delim), field).to_vec();
    let mut end = start + 1;
    while end < lines.len()
        && key(&split_fields(lines[end], delim), field).to_vec() == base
    {
        end += 1;
    }
    end
}

/// Append a non-empty field to an in-progress output line, inserting the
/// separator before all but the first field.
fn push_tok(out: &mut Vec<u8>, sep: u8, first: &mut bool, tok: &[u8]) {
    if tok.is_empty() {
        return;
    }
    if !*first {
        out.push(sep);
    }
    out.extend_from_slice(tok);
    *first = false;
}

/// Pick the token for one `join -o` entry given the (possibly absent) field
/// vectors of each file; a missing field or the absent side yields the `-e`
/// replacement (empty string when none was given).
fn olist_token(entry: &OField, a: Option<&[&[u8]]>, b: Option<&[&[u8]]>, opts: &JoinOptions) -> Vec<u8> {
    let fallback = opts.empty.clone().unwrap_or_default();
    let pick = |side: Option<&[&[u8]]>, idx: usize| -> Vec<u8> {
        match side {
            Some(fields) => fields.get(idx).map(|f| f.to_vec()).unwrap_or_else(|| fallback.clone()),
            None => fallback.clone(),
        }
    };
    match entry.which {
        // The join field comes from file 1's key column when present, else
        // file 2's (the unmatched file-1-only case still has its own key).
        Which::Join => a
            .and_then(|f| f.get(opts.field1))
            .or_else(|| b.and_then(|f| f.get(opts.field2)))
            .map(|f| f.to_vec())
            .unwrap_or_default(),
        Which::One => pick(a, entry.idx),
        Which::Two => pick(b, entry.idx),
    }
}

/// The pair output line: the `-o` field list when set (each entry resolved
/// against both files), else the default join field + file 1 rest + file 2 rest.
fn twoline(a: &[&[u8]], b: &[&[u8]], opts: &JoinOptions) -> Vec<u8> {
    let mut line = Vec::new();
    let mut first = true;
    if !opts.output_fields.is_empty() {
        for entry in &opts.output_fields {
            push_tok(&mut line, opts.delim, &mut first, &olist_token(entry, Some(a), Some(b), opts));
        }
        return line;
    }
    if a.len() > opts.field1 {
        push_tok(&mut line, opts.delim, &mut first, a[opts.field1]);
    }
    for (idx, f) in a.iter().enumerate() {
        if idx != opts.field1 {
            push_tok(&mut line, opts.delim, &mut first, f);
        }
    }
    for (idx, f) in b.iter().enumerate() {
        if idx != opts.field2 {
            push_tok(&mut line, opts.delim, &mut first, f);
        }
    }
    line
}

/// An unpaired line: with `-o` the other file's fields yield the `-e`
/// replacement; otherwise all of the line's own fields, joined by `delim`.
fn onlyline(fields: &[&[u8]], is_file1: bool, opts: &JoinOptions) -> Vec<u8> {
    let mut line = Vec::new();
    let mut first = true;
    if !opts.output_fields.is_empty() {
        let a: Option<&[&[u8]]> = if is_file1 { Some(fields) } else { None };
        let b: Option<&[&[u8]]> = if is_file1 { None } else { Some(fields) };
        for entry in &opts.output_fields {
            push_tok(&mut line, opts.delim, &mut first, &olist_token(entry, a, b, opts));
        }
        return line;
    }
    for f in fields {
        if f.is_empty() {
            continue;
        }
        if !first {
            line.push(opts.delim);
        }
        line.extend_from_slice(f);
        first = false;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<Vec<u8>> {
        s.lines().map(|l| l.as_bytes().to_vec()).collect()
    }

    fn refs(v: &[Vec<u8>]) -> Vec<&[u8]> {
        v.iter().map(|l| l.as_slice()).collect()
    }

    fn join(f1: &str, f2: &str, opts: &JoinOptions) -> String {
        let (a, b) = (lines(f1), lines(f2));
        let out = join_lines(&refs(&a), &refs(&b), opts);
        out.iter()
            .map(|l| core::str::from_utf8(l).unwrap().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_default_inner_join() {
        let f1 = "1 a\n2 b\n3 c";
        let f2 = "1 x\n2 y\n4 z";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a x\n2 b y");
    }

    #[test]
    fn test_unmatched_a_flags() {
        let f1 = "1 a\n3 c";
        let f2 = "1 x\n4 z";
        let o = JoinOptions {
            unmatched1: true,
            ..Default::default()
        };
        // -a 1: file 1's unpaired key 3 survives; file 2's key 4 dropped.
        assert_eq!(join(f1, f2, &o), "1 a x\n3 c");
        let o2 = JoinOptions {
            unmatched1: true,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join(f1, f2, &o2), "1 a x\n3 c\n4 z");
    }

    #[test]
    fn test_equal_keys_cross_multiply() {
        // Multiple lines share a key on both sides -> full cross product.
        let f1 = "1 a\n1 b";
        let f2 = "1 x\n1 y";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a x\n1 a y\n1 b x\n1 b y");
    }

    #[test]
    fn test_custom_join_fields() {
        // -1 2 -2 1: join file1's field 2 against file2's field 1.
        let f1 = "x A\ny B";
        let f2 = "A 1\nB 2";
        let o = JoinOptions {
            field1: 1,
            field2: 0,
            ..Default::default()
        };
        // key is file1's field2; output = key then file1-rest then file2-rest.
        assert_eq!(join(f1, f2, &o), "A x 1\nB y 2");
    }

    #[test]
    fn test_dash_t_delimiter() {
        let f1 = "1:a\n2:b";
        let f2 = "1:x\n3:z";
        let o = JoinOptions {
            delim: b':',
            ..Default::default()
        };
        // split and join both use ':'; unmatched file lines dropped (no -a).
        assert_eq!(join(f1, f2, &o), "1:a:x");
    }

    #[test]
    fn test_multifield_lines() {
        let f1 = "1 a b\n2 c d";
        let f2 = "1 x\n2 y";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a b x\n2 c d y");
    }

    #[test]
    fn test_parse_olist_forms() {
        assert_eq!(
            parse_olist("1.2,2.1,0").unwrap(),
            vec![
                OField { which: Which::One, idx: 1 },
                OField { which: Which::Two, idx: 0 },
                OField { which: Which::Join, idx: 0 },
            ]
        );
        assert!(parse_olist("3.1").is_err()); // no file 3
        assert!(parse_olist("1.0").is_err()); // fields are 1-based
        assert!(parse_olist("5").is_err()); // no dot, not `0`
    }

    #[test]
    fn test_olist_selects_fields_and_join() {
        // -o 0,2.2,1.2 : join field, file2's field2, file1's field2.
        let o = JoinOptions {
            output_fields: parse_olist("0,2.2,1.2").unwrap(),
            ..Default::default()
        };
        assert_eq!(join("1 a b\n2 c d", "1 x\n2 y", &o), "1 x a\n2 y c");
    }

    #[test]
    fn test_olist_empty_replacement_on_unmatched() {
        // -a 1 -o 0,1.2,2.2 -e '-' : file-1-only line has no file-2 field.
        let mut o = JoinOptions {
            output_fields: parse_olist("0,1.2,2.2").unwrap(),
            empty: Some(b"-".to_vec()),
            unmatched1: true,
            ..Default::default()
        };
        o.unmatched2 = false;
        // key 3 pairs with nothing -> file-2 field replaced by '-'.
        assert_eq!(join("1 a\n3 c", "1 x", &o), "1 a x\n3 c -");
    }
}
