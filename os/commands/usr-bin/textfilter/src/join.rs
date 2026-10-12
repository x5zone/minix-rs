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
//! output separator = that byte), `-1`/`-2` (per-file join field), `-j` (both
//! join fields at once — the CLI alias for `-1 N -2 N`), `-a` (unmatched
//! lines), `-v` (report only unpairables from one file, suppressing matched
//! lines via the C `joinout` global), `-o` (output-field list) and `-e`
//! (empty replacement). Declared residual: the obsolete one-argument form (a
//! bare `-1`/`-2`/`-o`/`-t` without a value) — this module models each option
//! independently, so the caller supplies the values it does not set as defaults.
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
    /// Emit the matched-line output. `-v` clears it so only the unpaired lines
    /// from the specified file print (C `joinout` global, `join.c:104,207-208`).
    pub joinout: bool,
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
            joinout: true,
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
            fields.push(OField {
                which: Which::Join,
                idx: 0,
            });
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

/// `join` 的 argv 尾部解析结果：选项 + 两个文件操数。
#[derive(Debug, Clone)]
pub struct JoinArgs<'a> {
    pub opts: JoinOptions,
    pub files: Vec<&'a str>,
}

/// `join` 参数解析失败。每个变体对应 C `join.c` 的一支 `warnx`+`usage()`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    /// 表外选项字母、缺选项值、或非两个操数（C `getopt` default / `usage()`）。
    Usage,
    /// `-1`/`-2`/`-j` 的字段号 < 1。
    FieldLessThan1,
    /// `-1`/`-2`/`-j` 的字段号含非数字尾（C `*end` 非零）。
    IllegalFieldNumber,
    /// `-a`/`-v` 的文件号非 1/2。
    FileNumberNot12,
    /// `-a`/`-v` 的文件号含非数字尾。
    IllegalFileNumber,
    /// `-t` 的分隔符不是单字节（C `strlen(optarg) != 1`）。
    BadTabChar,
    /// `-o` 的字段列表不合法（`fieldarg`/`parse_olist` 拒）。
    BadOutputField,
    /// `-a` 与 `-v` 同时给（C `aflag && vflag` → `usage()`）。
    AAndVMutuallyExclusive,
}

/// 把 `join` 的 argv 尾部按 BSD `getopt("\01a:e:j:1:2:o:t:v:")` 拆成 [`JoinArgs`]。
/// 除选项字母外的剩余部分（`-t,` 的 `,`）或是附值或是紧跟的下一项，都是选项值——
/// `join.c:133` 的 `getopt` 对带 `:` 的选项接受两种形。旧前门 `match flag{…}`
/// 只比整个 `a[1..]`，故 `join -t, f1 f2`、`-a1`、`-o0,1` 这些 C 合法的附值形
/// 会误落「unknown option」。`--` 结束选项扫描（BSD getopt），裸 `-` 与非连字符
/// 项都是文件操数。C 的 `obsolete(argv)`（`:132`）对裸 `-a`、`-j[12]` 等 legacy 写法
/// 重写 argv 为 `\01`/`-1`/`-2`，本层不实现该 legacy 重写（登记为余面）。
pub fn parse_join_args<'a>(args: &'a [&'a str]) -> Result<JoinArgs<'a>, JoinError> {
    let mut opts = JoinOptions::default();
    let mut files: Vec<&'a str> = Vec::new();
    let (mut aflag, mut vflag) = (false, false);
    let mut i = 0;
    // 取一个选项的值：附在字母后（`&arg[2..]`）或紧跟的下一项。
    macro_rules! value {
        () => {{
            let cur = args[i];
            if cur.len() > 2 {
                &cur[2..]
            } else {
                i += 1;
                if i >= args.len() {
                    return Err(JoinError::Usage);
                }
                args[i]
            }
        }};
    }
    // 一个必为十进制且 >=1 的字段号（C `strtol` + `<1` + `*end`）。
    fn field_no(v: &str) -> Result<usize, JoinError> {
        if v.is_empty() {
            // C `strtol("")` → 0，`< 1` 分支。
            return Err(JoinError::FieldLessThan1);
        }
        if !v.bytes().all(|b| b.is_ascii_digit()) {
            // `*end` 非零：带非数字尾。
            return Err(JoinError::IllegalFieldNumber);
        }
        let n: usize = v.parse().map_err(|_| JoinError::IllegalFieldNumber)?;
        if n < 1 {
            return Err(JoinError::FieldLessThan1);
        }
        Ok(n - 1)
    }
    // 一个只能是 1/2 的文件号（C `strtol` + switch + `*end`）。
    fn file_no(v: &str, unpair1: &mut bool, unpair2: &mut bool) -> Result<(), JoinError> {
        match v {
            "1" => *unpair1 = true,
            "2" => *unpair2 = true,
            _ => {
                // C 先 strtol 取 1/2、否则 "not 1 or 2"；尾字符另报 "illegal file number"。
                if v.bytes().all(|b| b.is_ascii_digit()) && !v.is_empty() {
                    let n: i64 = v.parse().unwrap_or(-1);
                    return if n == 1 || n == 2 {
                        Ok(())
                    } else {
                        Err(JoinError::FileNumberNot12)
                    };
                }
                return Err(JoinError::FileNumberNot12);
            }
        }
        Ok(())
    }
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        let b = a.as_bytes();
        if b.len() < 2 || b[0] != b'-' {
            files.push(a);
            i += 1;
            continue;
        }
        match b[1] {
            b'1' => opts.field1 = field_no(value!())?,
            b'2' => opts.field2 = field_no(value!())?,
            b't' => {
                let v = value!();
                let bytes = v.as_bytes();
                if bytes.len() != 1 {
                    return Err(JoinError::BadTabChar);
                }
                opts.delim = bytes[0];
            }
            b'a' => {
                aflag = true;
                file_no(value!(), &mut opts.unmatched1, &mut opts.unmatched2)?;
            }
            b'v' => {
                vflag = true;
                opts.joinout = false;
                file_no(value!(), &mut opts.unmatched1, &mut opts.unmatched2)?;
            }
            b'e' => opts.empty = Some(value!().as_bytes().to_vec()),
            b'j' => {
                let n = field_no(value!())?;
                opts.field1 = n;
                opts.field2 = n;
            }
            b'o' => {
                opts.output_fields =
                    parse_olist(value!()).map_err(|_| JoinError::BadOutputField)?;
            }
            _ => return Err(JoinError::Usage),
        }
        i += 1;
    }
    if aflag && vflag {
        return Err(JoinError::AAndVMutuallyExclusive);
    }
    if files.len() != 2 {
        return Err(JoinError::Usage);
    }
    Ok(JoinArgs { opts, files })
}

/// Split a line into fields on runs of space/tab (the C default `tabchar`
/// `" \t"` with `spans`), or on the single `-t` byte; empty runs are skipped.
fn split_fields(line: &[u8], delim: u8) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = 0usize;
    for i in 0..=line.len() {
        let is_delim = i < line.len() && (line[i] == delim || (delim == b' ' && line[i] == b'\t'));
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
                // C's `if (joinout) joinlines(F1, F2)` (`join.c:259`): with `-v`
                // clearing `joinout`, matched groups still advance but emit
                // nothing.
                if opts.joinout {
                    for la in f1[i..g1].iter().copied() {
                        let fa = split_fields(la, opts.delim);
                        for lb in f2[j..g2].iter().copied() {
                            let fb = split_fields(lb, opts.delim);
                            out.push(twoline(&fa, &fb, opts));
                        }
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
    while end < lines.len() && key(&split_fields(lines[end], delim), field).to_vec() == base {
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
fn olist_token(
    entry: &OField,
    a: Option<&[&[u8]]>,
    b: Option<&[&[u8]]>,
    opts: &JoinOptions,
) -> Vec<u8> {
    let fallback = opts.empty.clone().unwrap_or_default();
    let pick = |side: Option<&[&[u8]]>, idx: usize| -> Vec<u8> {
        match side {
            Some(fields) => fields
                .get(idx)
                .map(|f| f.to_vec())
                .unwrap_or_else(|| fallback.clone()),
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
            push_tok(
                &mut line,
                opts.delim,
                &mut first,
                &olist_token(entry, Some(a), Some(b), opts),
            );
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
            push_tok(
                &mut line,
                opts.delim,
                &mut first,
                &olist_token(entry, a, b, opts),
            );
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
        assert_eq!(
            join(f1, f2, &JoinOptions::default()),
            "1 a x\n1 a y\n1 b x\n1 b y"
        );
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
                OField {
                    which: Which::One,
                    idx: 1
                },
                OField {
                    which: Which::Two,
                    idx: 0
                },
                OField {
                    which: Which::Join,
                    idx: 0
                },
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

    #[test]
    fn test_v1_emits_only_file1_unpaired() {
        // `-v 1` (joinout off, unpair file 1): the key 1 match prints nothing,
        // key 3 alone from file 1 prints, file 2's key 4 stays silent.
        let o = JoinOptions {
            joinout: false,
            unmatched1: true,
            ..Default::default()
        };
        assert_eq!(join("1 a\n3 c", "1 x\n4 z", &o), "3 c");
    }

    #[test]
    fn test_v2_emits_only_file2_unpaired() {
        let o = JoinOptions {
            joinout: false,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join("1 a\n3 c", "1 x\n4 z", &o), "4 z");
    }

    #[test]
    fn test_joinout_off_without_unpair_emits_nothing() {
        // Defensive: `-v` with no matching unpair file yields empty output —
        // matched groups are silent and no side emits singletons.
        let o = JoinOptions {
            joinout: false,
            ..Default::default()
        };
        assert_eq!(join("1 a\n2 b", "1 x\n3 z", &o), "");
    }

    fn parse<'a>(v: &'a [&'a str]) -> JoinArgs<'a> {
        parse_join_args(v).expect("parse ok")
    }

    #[test]
    fn parse_attached_option_values_accepted() {
        // 关键回归：BSD getopt 的附值形不被旧前门接受。
        let a = parse(&["-t,", "f1", "f2"]);
        assert_eq!(a.opts.delim, b',');
        assert_eq!(a.files, vec!["f1", "f2"]);
        let b = parse(&["-a1", "-o0,1.2", "f1", "f2"]);
        assert!(b.opts.unmatched1 && !b.opts.unmatched2);
        assert_eq!(b.opts.output_fields.len(), 2);
        // 分离值仍正确（向后兼容）。
        let c = parse(&["-t", "|", "-1", "2", "f1", "f2"]);
        assert_eq!(c.opts.delim, b'|');
        assert_eq!(c.opts.field1, 1);
    }

    #[test]
    fn parse_terminator_and_operands() {
        // `--` 后全为操数；裸 `-` 是操数（stdin）。
        let a = parse(&["-t,", "--", "-a", "f2"]);
        assert_eq!(a.files, vec!["-a", "f2"]); // `-a` 在 `--` 后是文件名的字串
        let b = parse(&["-", "f2"]);
        assert_eq!(b.files, vec!["-", "f2"]);
    }

    #[test]
    fn parse_validations_and_mutual_exclusion() {
        // 非两个操数、表外字母、-a&-v 互斥、非法号、非单字节 tab。
        assert_eq!(parse_join_args(&["only1"]).err(), Some(JoinError::Usage));
        assert_eq!(
            parse_join_args(&["-z", "f1", "f2"]).err(),
            Some(JoinError::Usage)
        );
        assert_eq!(
            parse_join_args(&["-a1", "-v2", "f1", "f2"]).err(),
            Some(JoinError::AAndVMutuallyExclusive)
        );
        assert_eq!(
            parse_join_args(&["-1", "0", "f1", "f2"]).err(),
            Some(JoinError::FieldLessThan1)
        );
        assert_eq!(
            parse_join_args(&["-1", "2x", "f1", "f2"]).err(),
            Some(JoinError::IllegalFieldNumber)
        );
        assert_eq!(
            parse_join_args(&["-t", "ab", "f1", "f2"]).err(),
            Some(JoinError::BadTabChar)
        );
        assert_eq!(
            parse_join_args(&["-a3", "f1", "f2"]).err(),
            Some(JoinError::FileNumberNot12)
        );
        assert_eq!(
            parse_join_args(&["-j", "f1", "f2"]).err(),
            Some(JoinError::IllegalFieldNumber) // -j 吃 f1 作值→非数字→IllegalFieldNumber
        );
    }
}
