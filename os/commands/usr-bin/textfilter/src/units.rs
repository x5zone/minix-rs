//! units — conversion program (minix3/usr.bin/units/units.c).
//!
//! Deciding half: units-file parsing (definitions and `-`-suffixed
//! prefixes), expression parsing (`a|b` rationals, trailing-digit
//! repeats, `/` denominators), primitive reduction via lookup (with
//! the `^`/plural/prefix fallbacks), sorting+cancellation, the
//! conformability check and the `\t* \t/` answer rendering. The
//! interactive prompt face and units-file path search stay in the
//! bin. The `-f` operand is the one byte-valued argument that still
//! needs a `&str` to be opened (`ARGV-BYTE-PATH-IN-LIBS`).
//!
//! Unit names are bytes, never text. The C compares them with
//! `strcmp`/`strncmp` (`units.c:503`, `:511`, `:542`), prints them with
//! `%s` (`units.c:554`'s `mywarnx("Unknown unit '%s'")`, `showunit`,
//! `listunits`) and splits the expression with `strtok(scratch, " *\t\n/")`
//! — no step ever asks what a byte means in UTF-8. So the table keys, the
//! reduced product's entries, the `UnknownUnit` payload and the listings
//! all carry `Vec<u8>`: one kind of object for the whole chain, because in
//! the C the table key and the expression node are the same bytes.
use crate::floatfmt::{decimal_exponent, parse_float_prefix_bytes, pow10};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Precision of `%.*g` in showunit/showanswer (units.c:43).
pub const PRECISION: usize = 8;
/// `-L` uses DBL_DIG.
pub const PRECISION_LIST_EXPAND: usize = 15;
/// Definitions with a leading `!` are primitives (`units.c:64`'s
/// `PRIMITIVECHAR`, tested as `thedefn[0] == PRIMITIVECHAR`).
pub const PRIMITIVECHAR: u8 = b'!';
pub const POWERSTRING: &[u8] = b"^";

/// The parsed units database: name and definition, both as the bytes the
/// units file carried.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitsTable {
    pub units: Vec<(Vec<u8>, Vec<u8>)>,
    pub prefixes: Vec<(Vec<u8>, Vec<u8>)>,
}

/// Errors mirroring the C warnings (which go to stderr, or stdout as
/// `/ ` comments in list mode).
#[derive(Debug, PartialEq)]
pub enum UnitsError {
    UnknownUnit(Vec<u8>),
    ReducesToZero,
    JunkBeforeBar,
    ReadError(usize),
    Conformability,
}

/// A reduced unit: factor times numerator/denominator products.
/// Cancelled entries hold no bytes at all (the C's NULLUNIT).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitType {
    pub numerator: Vec<Vec<u8>>,
    pub denominator: Vec<Vec<u8>>,
    pub factor: f64,
}

/// The bytes that separate two items, `" *\t\n/"` in
/// `strtok(scratch, " *\t\n/")` (units.c:357).
fn is_item_break(byte: &u8) -> bool {
    matches!(byte, b' ' | b'*' | b'\t' | b'\n' | b'/')
}

/// The bytes `strspn`/`strcspn` skip in `readunits` (`" \n\t"`,
/// units.c:188-190).
fn is_space_tab_newline(byte: &u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\t')
}

/// Parses the units file line by line (readunits, units.c:134-229):
/// lines starting with `/` are comments; a first token ending in `-`
/// defines a prefix; otherwise `name definition`.
pub fn read_units(lines: &[&[u8]]) -> UnitsTable {
    let mut table = UnitsTable::default();
    for line in lines {
        if line.first() == Some(&b'/') {
            continue;
        }
        let trimmed = strip_bytes(line, is_space_tab_newline);
        let (name, rest) = match trimmed.iter().position(is_space_tab_newline) {
            Some(split) => (
                &trimmed[..split],
                strip_leading(&trimmed[split..], is_space_tab_newline),
            ),
            None => (trimmed, &[][..]),
        };
        if name.is_empty() {
            continue;
        }
        // `lineptr[strlen(lineptr) - 1] == '-'` — the last **byte** decides
        // whether this line names a prefix (units.c:200).
        if name.last() == Some(&b'-') {
            let stem = &name[..name.len() - 1];
            // A prefix definition; duplicates are ignored.
            if stem.is_empty() || rest.is_empty() {
                continue;
            }
            if !table.prefixes.iter().any(|(n, _)| n == stem) {
                table.prefixes.push((stem.to_vec(), rest.to_vec()));
            }
        } else if rest.is_empty() {
            continue;
        } else if !table.units.iter().any(|(n, _)| n == name) {
            table.units.push((name.to_vec(), rest.to_vec()));
        }
    }
    table
}

/// `strspn` then `strcspn` from the left: drop leading matched bytes.
fn strip_leading(bytes: &[u8], pred: fn(&u8) -> bool) -> &[u8] {
    let start = bytes.iter().position(|b| !pred(b)).unwrap_or(bytes.len());
    &bytes[start..]
}

/// Trim on both ends by a byte predicate.
fn strip_bytes(bytes: &[u8], pred: fn(&u8) -> bool) -> &[u8] {
    let start = bytes.iter().position(|b| !pred(b)).unwrap_or(bytes.len());
    let mut end = bytes.len();
    while end > start && pred(&bytes[end - 1]) {
        end -= 1;
    }
    &bytes[start..end]
}

/// addunit: parses one unit expression into `theunit`, flipping into
/// the denominator past `/` (units.c:332-424).
pub fn add_unit(expr: &[u8], flip: bool) -> Result<UnitType, UnitsError> {
    let mut unit = UnitType {
        factor: 1.0,
        ..UnitType::default()
    };
    add_into(&mut unit, expr, flip)?;
    Ok(unit)
}

fn add_into(unit: &mut UnitType, expr: &[u8], flip: bool) -> Result<(), UnitsError> {
    // The C rewrites '-' to ' ' unless it is an exponent sign
    // (preceded by e/E and followed by a digit or dot).
    let mut scratch = Vec::with_capacity(expr.len());
    for i in 0..expr.len() {
        let c = expr[i];
        if c == b'-'
            && (i == 0
                || expr[i - 1].to_ascii_lowercase() != b'e'
                || i + 1 >= expr.len()
                || !(expr[i + 1].is_ascii_digit() || expr[i + 1] == b'.'))
        {
            scratch.push(b' ');
        } else {
            scratch.push(c);
        }
    }
    let (top, bottom) = match scratch.iter().position(|&b| b == b'/') {
        Some(slash) => (&scratch[..slash], Some(&scratch[slash + 1..])),
        None => (scratch.as_slice(), None),
    };
    // C `addunit` (`minix3/usr.bin/units/units.c:359-436`) 是 `do { …; doingtop--;
    // if (slash) { scratch = slash + 1; } else doingtop--; } while (doingtop >= 0);`
    // ——`doingtop` 初值 1，每轮尾 `doingtop--`；无斜杠时额外一次 `--`，共两轮 `--`
    // 到 `-1`，下一轮 `>= 0` 假就退出。至多两轮（先顶后底），永远没有第三轮。
    // 下面 Rust 翻译里的 `loop` 就是这个“至多两轮”的直译（`doingtop: bool` 首帧 true
    // 后下一轮已固定为 false，`match bottom` 两臂均 break——clippy::unused_loop 拓到的就
    // 是它）。展开为顺序两步，语义与 do-while 一致：首轮 `doingtop != flip == true != flip ==
    // !flip`，若有底轮 `doingtop != flip == false != flip == flip`。
    add_product(unit, top, !flip)?;
    if let Some(b) = bottom {
        add_product(unit, b, flip)?;
    }
    Ok(())
}

/// `strtod` on a window the scan already proved to be digits and a dot.
fn ascii_float(bytes: &[u8]) -> f64 {
    core::str::from_utf8(bytes)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// One product side: items split on spaces/`*`/tabs.
fn add_product(unit: &mut UnitType, product: &[u8], top: bool) -> Result<(), UnitsError> {
    for item in product.split(is_item_break) {
        if item.is_empty() {
            continue;
        }
        // `strchr("0123456789.", *item)` (units.c:365) inspects the **first
        // byte**, so a multi-byte name never enters the numeric branch by
        // accident.
        if item[0].is_ascii_digit() || item[0] == b'.' {
            // Numeric factor, possibly "6|2" rational, possibly with
            // trailing unit names ("3foo" is "3 foo").
            let mut rest = item;
            loop {
                // strtod-shaped numeric head: digits, dot, exponent.
                let (num, used) = match parse_float_prefix_bytes(rest) {
                    Some(x) => x,
                    None => break,
                };
                let tail = &rest[used..];
                if tail.first() == Some(&b'|') {
                    // Rational: num|den.
                    let after = &tail[1..];
                    let den_len = after
                        .iter()
                        .take_while(|&&b| b.is_ascii_digit() || b == b'.')
                        .count();
                    if den_len == 0 || num == 0.0 {
                        if num == 0.0 {
                            return Err(UnitsError::ReducesToZero);
                        }
                        return Err(UnitsError::JunkBeforeBar);
                    }
                    let (den_bytes, tail2) = after.split_at(den_len);
                    let den = ascii_float(den_bytes);
                    if den == 0.0 {
                        return Err(UnitsError::ReducesToZero);
                    }
                    if top {
                        unit.factor *= num / den;
                    } else {
                        unit.factor *= den / num;
                    }
                    rest = tail2;
                } else {
                    if num == 0.0 {
                        return Err(UnitsError::ReducesToZero);
                    }
                    if tail.contains(&b'|') {
                        // "6x|2" — junk between the number and the bar.
                        return Err(UnitsError::JunkBeforeBar);
                    }
                    if top {
                        unit.factor *= num;
                    } else {
                        unit.factor /= num;
                    }
                    rest = tail;
                }
                if rest.is_empty() || !(rest[0].is_ascii_digit() || rest[0] == b'.') {
                    break;
                }
            }
            if !rest.is_empty() {
                add_names(unit, rest, top)?;
            }
        } else {
            add_names(unit, item, top)?;
        }
    }
    Ok(())
}

/// A symbolic item: trailing digit 2-9 means a repeat count.
fn add_names(unit: &mut UnitType, item: &[u8], top: bool) -> Result<(), UnitsError> {
    let (name, repeat) = match item.last() {
        Some(c) if (b'2'..=b'9').contains(c) => (&item[..item.len() - 1], c - b'0'),
        _ => (item, 1),
    };
    for _ in 0..repeat {
        if top {
            unit.numerator.push(name.to_vec());
        } else {
            unit.denominator.push(name.to_vec());
        }
    }
    Ok(())
}

/// lookupunit: exact name, `^`-stripped, singular (`s`/`es`), then
/// prefix expansion (units.c:470-537). Returns the definition text.
pub fn lookup_unit(table: &UnitsTable, unit: &[u8]) -> Option<Vec<u8>> {
    if let Some((_, defn)) = table.units.iter().find(|(n, _)| n == unit) {
        return Some(defn.clone());
    }
    if unit.last() == Some(&b'^') {
        let stem = &unit[..unit.len() - 1];
        if table.units.iter().any(|(n, _)| n == stem) {
            return Some(stem.to_vec());
        }
    }
    if unit.last() == Some(&b's') {
        let stem = &unit[..unit.len() - 1];
        if table.units.iter().any(|(n, _)| n == stem) {
            return Some(stem.to_vec());
        }
        if stem.last() == Some(&b'e') {
            let stem2 = &stem[..stem.len() - 1];
            if table.units.iter().any(|(n, _)| n == stem2) {
                return Some(stem2.to_vec());
            }
        }
    }
    for (prefix, value) in &table.prefixes {
        // `strncmp(prefixname, unit, strlen(prefixname))` (units.c:535).
        if unit.starts_with(prefix.as_slice()) {
            // A bare prefix name is legal: the rest may be empty.
            let rest = &unit[prefix.len()..];
            if rest.is_empty() || lookup_unit(table, rest).is_some() {
                // `strlcpy(buffer, prefixval) + strlcat(" ") + strlcat(unit)`
                let mut def = value.clone();
                def.push(b' ');
                def.extend_from_slice(rest);
                return Some(def);
            }
        }
    }
    None
}

/// reduceproduct/reduceunit: expand one symbolic occurrence into its
/// definition per pass until a fixed point (units.c:563-613). The C
/// expands each occurrence through its own inner loop; expanding one
/// occurrence per pass reaches the same fixpoint.
pub fn reduce_unit(table: &UnitsTable, unit: &mut UnitType) -> Result<(), UnitsError> {
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for side in [true, false] {
            let names: Vec<Vec<u8>> = (if side {
                &unit.numerator
            } else {
                &unit.denominator
            })
            .clone();
            for name in names {
                if name.is_empty() {
                    continue;
                }
                let def = match lookup_unit(table, &name) {
                    Some(d) => d,
                    None => return Err(UnitsError::UnknownUnit(name)),
                };
                if def.contains(&PRIMITIVECHAR) {
                    continue;
                }
                let product = if side {
                    &mut unit.numerator
                } else {
                    &mut unit.denominator
                };
                for p in product.iter_mut() {
                    if *p == name {
                        *p = Vec::new();
                        break;
                    }
                }
                add_into(unit, &def, !side)?;
                changed = true;
                break 'outer;
            }
        }
    }
    Ok(())
}

/// completereduce: reduce, sort, cancel (units.c:721-727).
pub fn complete_reduce(table: &UnitsTable, unit: &mut UnitType) -> Result<(), UnitsError> {
    reduce_unit(table, unit)?;
    unit.numerator.sort();
    unit.denominator.sort();
    cancel_unit(unit);
    Ok(())
}

/// cancelunit: drop names appearing on both sides (units.c:386-407).
pub fn cancel_unit(unit: &mut UnitType) {
    let mut den = unit
        .denominator
        .clone()
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut num = unit
        .numerator
        .clone()
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    den.sort();
    num.sort();
    let mut out_num = Vec::new();
    let mut out_den = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < num.len() && j < den.len() {
        match num[i].cmp(&den[j]) {
            core::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
            core::cmp::Ordering::Less => {
                out_num.push(num[i].clone());
                i += 1;
            }
            core::cmp::Ordering::Greater => {
                out_den.push(den[j].clone());
                j += 1;
            }
        }
    }
    out_num.extend_from_slice(&num[i..]);
    out_den.extend_from_slice(&den[j..]);
    unit.numerator = out_num;
    unit.denominator = out_den;
}

/// compareunits: zero when numerator and denominator products match.
pub fn compare_units(a: &UnitType, b: &UnitType) -> bool {
    let an: Vec<&[u8]> = a
        .numerator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    let bn: Vec<&[u8]> = b
        .numerator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    let ad: Vec<&[u8]> = a
        .denominator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    let bd: Vec<&[u8]> = b
        .denominator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    an == bn && ad == bd
}

/// compareunitsreciprocal: a's numerator matches b's denominator.
pub fn compare_units_reciprocal(a: &UnitType, b: &UnitType) -> bool {
    let an: Vec<&[u8]> = a
        .numerator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    let bd: Vec<&[u8]> = b
        .denominator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    let ad: Vec<&[u8]> = a
        .denominator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    let bn: Vec<&[u8]> = b
        .numerator
        .iter()
        .map(Vec::as_slice)
        .filter(|s| !s.is_empty())
        .collect();
    an == bd && ad == bn
}

/// showanswer result: the two printed lines or the error shape.
#[derive(Debug, PartialEq)]
pub enum Answer {
    /// `\t* x\n\t/ y\n`
    Factor(f64, f64),
    /// `\treciprocal conversion\n\t* x\n\t/ y\n`
    Reciprocal(f64, f64),
    /// `conformability error\n` plus both reduced units.
    Conformability(UnitType, UnitType),
}

pub fn show_answer(have: &UnitType, want: &UnitType) -> Answer {
    if compare_units(have, want) {
        Answer::Factor(have.factor / want.factor, want.factor / have.factor)
    } else if compare_units_reciprocal(have, want) {
        Answer::Reciprocal(1.0 / (have.factor * want.factor), want.factor * have.factor)
    } else {
        Answer::Conformability(have.clone(), want.clone())
    }
}

/// printf "%.*g" for showunit/showanswer (precision from the CLI).
pub fn format_g(value: f64, prec: usize) -> String {
    let prec = if prec == 0 { 1 } else { prec };
    if value == 0.0 {
        return "0".to_string();
    }
    let neg = value < 0.0;
    let v = value.abs();
    let exp = decimal_exponent(v);
    let (body, exp_used) = if exp < -4 || exp >= prec as i32 {
        let mant = format!("{:.*}", prec.saturating_sub(1), v / pow10(exp));
        (mant, true)
    } else {
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        (format!("{:.*}", decimals, v), false)
    };
    let body = body.trim_end_matches('0').trim_end_matches('.').to_string();
    let body = if body.is_empty() {
        "0".to_string()
    } else {
        body
    };
    let sign = if neg { "-" } else { "" };
    if exp_used {
        format!(
            "{}{}e{}{:02}",
            sign,
            body,
            if exp < 0 { "-" } else { "+" },
            exp.abs()
        )
    } else {
        format!("{}{}", sign, body)
    }
}

/// showunit: `\t<factor>` + ` name^2` products and ` / name` denominators.
/// The names go out as the bytes that carried them (`showunit` prints each
/// with `printf("%s", …)`, units.c:426-460).
pub fn show_unit(unit: &UnitType, prec: usize) -> Vec<u8> {
    let mut out = format!("\t{}", format_g(unit.factor, prec)).into_bytes();
    let mut counter = 1;
    for (i, name) in unit.numerator.iter().enumerate() {
        if i > 0 && !name.is_empty() && name == &unit.numerator[i - 1] {
            counter += 1;
        } else {
            if counter > 1 {
                out.extend_from_slice(POWERSTRING);
                out.extend_from_slice(counter.to_string().as_bytes());
            }
            if !name.is_empty() {
                out.push(b' ');
                out.extend_from_slice(name);
            }
            counter = 1;
        }
    }
    if counter > 1 {
        out.extend_from_slice(POWERSTRING);
        out.extend_from_slice(counter.to_string().as_bytes());
    }
    let mut printed_slash = false;
    counter = 1;
    for (i, name) in unit.denominator.iter().enumerate() {
        if i > 0 && !name.is_empty() && name == &unit.denominator[i - 1] {
            counter += 1;
        } else {
            if counter > 1 {
                out.extend_from_slice(POWERSTRING);
                out.extend_from_slice(counter.to_string().as_bytes());
            }
            if !name.is_empty() {
                if !printed_slash {
                    out.extend_from_slice(b" /");
                    printed_slash = true;
                }
                out.push(b' ');
                out.extend_from_slice(name);
            }
            counter = 1;
        }
    }
    if counter > 1 {
        out.extend_from_slice(POWERSTRING);
        out.extend_from_slice(counter.to_string().as_bytes());
    }
    out.push(b'\n');
    out
}

/// Renders an Answer into the exact stdout bytes.
pub fn render_answer(answer: &Answer, prec: usize) -> Vec<u8> {
    match answer {
        Answer::Factor(mul, div) => format!(
            "\t* {}\n\t/ {}\n",
            format_g(*mul, prec),
            format_g(*div, prec)
        )
        .into_bytes(),
        Answer::Reciprocal(mul, div) => format!(
            "\treciprocal conversion\n\t* {}\n\t/ {}\n",
            format_g(*mul, prec),
            format_g(*div, prec)
        )
        .into_bytes(),
        Answer::Conformability(have, want) => {
            let mut out = Vec::from(&b"conformability error\n"[..]);
            out.extend_from_slice(&show_unit(have, prec));
            out.extend_from_slice(&show_unit(want, prec));
            out
        }
    }
}

/// listunits (`-l`/`-L`): dump primitives, prefixes, then all other
/// units; `expand` reduces each definition to primitive form
/// (units.c:694-779). Returns the printed bytes and the error count.
pub fn list_units(table: &UnitsTable, expand: bool, prec: usize) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut errors = 0usize;
    out.extend_from_slice(b"/ Primitive units\n");
    for (name, defn) in &table.units {
        if defn.first() == Some(&PRIMITIVECHAR) {
            out.extend_from_slice(name);
            out.push(b'\t');
            out.extend_from_slice(defn);
            out.push(b'\n');
        }
    }
    out.extend_from_slice(b"/ Prefixes\n");
    for (name, defn) in &table.prefixes {
        let mut printable = true;
        if expand {
            match add_unit(defn, false).and_then(|mut u| {
                complete_reduce(table, &mut u)?;
                Ok(u)
            }) {
                Err(_) => {
                    errors += 1;
                    printable = false;
                }
                Ok(u) => {
                    out.extend_from_slice(name);
                    out.push(b'-');
                    out.extend_from_slice(&show_unit(&u, prec));
                    continue;
                }
            }
        }
        if printable {
            out.extend_from_slice(name);
            out.extend_from_slice(b"-\t");
            out.extend_from_slice(defn);
            out.push(b'\n');
        }
    }
    out.extend_from_slice(b"/ Other units\n");
    for (name, defn) in &table.units {
        if defn.first() == Some(&PRIMITIVECHAR) {
            continue;
        }
        let mut printable = true;
        if expand {
            // The C expands the NAME, catching bad names too.
            match add_unit(name, false).and_then(|mut u| {
                complete_reduce(table, &mut u)?;
                Ok(u)
            }) {
                Err(_) => {
                    errors += 1;
                    printable = false;
                }
                Ok(u) => {
                    out.extend_from_slice(name);
                    out.extend_from_slice(&show_unit(&u, prec));
                    continue;
                }
            }
        }
        if printable {
            out.extend_from_slice(name);
            out.push(b'\t');
            out.extend_from_slice(defn);
            out.push(b'\n');
        }
    }
    (out, errors)
}

/// 前门跑完那个选项环之后手上有的东西（`units.c:818-833` 的 `switch` 各支置的变量）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitsArgs<'a> {
    /// `-f` 的值，C 的 `userfile`。它是**路径**：真正打开那一档的腿要等库侧的
    /// 字节口（登记 `ARGV-BYTE-PATH-IN-LIBS`），本层只保证不再提前折成文本。
    pub userfile: Option<&'a [u8]>,
    /// `-l` 或 `-L` 都置它（C 的 `list`）。
    pub list: bool,
    /// `-L` 才置它（C 的 `listexpand`）。
    pub listexpand: bool,
    /// `-q`（C 的 `quiet`）。
    pub quiet: bool,
    /// 环结束后的操作数，即 C 的 `argv += optind`。它们就是单位名表达式，
    /// 所以与原字节直接接上 `fn add_unit`（C 在 `units.c:855-875` 把 `argv` 原样交出去）。
    pub operands: Vec<&'a [u8]>,
}

/// 前门的三类出口。句式各有主人，所以不是一支的详略版：
/// [`UnitsArgError::Getopt`] 那一行由 libc `getopt` 印、其后必接 usage
/// （`units.c:843-845` 的 `default: usage()`）；[`UnitsArgError::Version`] 是
/// `-v` 那两行加 usage（`units.c:836-841`，两行都在**标准错误**上）；
/// [`UnitsArgError::Usage`] 只有 usage 块、没有 libc 那一行（操作数个数不对，
/// `units.c:849-851`）。三支都退 3。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitsArgError {
    Getopt(crate::GetoptReport),
    Version,
    Usage,
}

/// `units.c:817` 的 `getopt(argc, argv, "lLvqf:")`。
///
/// 只有 `f` 带值，所以「带值选项缺值」那一支在本支只有 `f` 一个字母可达；
/// `l`/`L` 是两个不同字母而不是大小写同义（`-l` 列原名、`-L` 列展开形），
/// `v` 在环里就退出，所以 `-vZ` 只有版本块、`-Zv` 只有集外字母那一行加 usage，
/// 两支都经参照件现量。
///
/// 游标走**字节**而非 `chars()`：参照件对 `-é` 报的是首字节 `\303`
/// （libc 的 `%c` 打的是那个字节本身），按字符走会在非 ASCII 字节上越出字符边界
/// （与 `column`/`col` 那两处同形）。
pub fn parse_units_args<'a>(args: &'a [&'a [u8]]) -> Result<UnitsArgs<'a>, UnitsArgError> {
    const OPTSTRING: &[u8] = b"lLvqf:";
    let mut parsed = UnitsArgs::default();
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i];
        let bytes = arg;
        // 不以 `-` 开头的 token、以及单破折号本身，都在 `getopt.c:74-78` 与 `:87-92`
        // 结束选项环且**不被吃掉**，于是它自己就是第一个操作数（不重排）。
        if bytes.len() < 2 || bytes[0] != b'-' {
            break;
        }
        // `--` 在 `getopt.c:81-85` 结束环且自身被推进掉，所以它不是操作数。
        if bytes == b"--" {
            i += 1;
            break;
        }
        let mut k = 1usize;
        while k < bytes.len() {
            let letter = bytes[k];
            k += 1;
            // `:` 在 optstring 里只是「前一个字母要值」的记号：libc 的判据是
            // `optopt == ':' || strchr(ostr, optopt) == NULL`（`getopt.c:98`），所以它
            // 永远不是可选字母，`-:` 落进集外字母那一支。
            let Some(slot) = OPTSTRING.iter().position(|&o| o == letter && o != b':') else {
                return Err(UnitsArgError::Getopt(crate::GetoptReport::UnknownOption(
                    char::from(letter),
                )));
            };
            if OPTSTRING.get(slot + 1) == Some(&b':') {
                // 带值项：值要么紧贴在本 token 的尾巴上，要么占下一个 token
                // （`getopt.c:112-141`）。两种情形都结束本 token，尾巴不再当字母扫。
                let value = if k < bytes.len() {
                    &arg[k..]
                } else {
                    i += 1;
                    let Some(next) = args.get(i) else {
                        return Err(UnitsArgError::Getopt(crate::GetoptReport::MissingValue(
                            char::from(letter),
                        )));
                    };
                    *next
                };
                if letter == b'f' {
                    parsed.userfile = Some(value);
                }
                // 值一旦取走，本 token 剩下的字节不再当字母扫（libc 直接推进 `optind`）。
                break;
            }
            match letter {
                b'l' => parsed.list = true,
                b'L' => {
                    parsed.list = true;
                    parsed.listexpand = true;
                }
                b'q' => parsed.quiet = true,
                b'v' => return Err(UnitsArgError::Version),
                _ => {}
            }
        }
        i += 1;
    }
    parsed.operands = args[i..].to_vec();
    // C 的这个判断排在环之后、`readunits` 之前（`units.c:849-851`），所以
    // 「操作数个数不对 + 档打不开」报的是 usage 而不是开档失败——顺序是可观测的。
    if !matches!(parsed.operands.len(), 0 | 2 | 3) || (parsed.list && !parsed.operands.is_empty()) {
        return Err(UnitsArgError::Usage);
    }
    Ok(parsed)
}

/// `UnitsError` 对应到 C 侧那一句 `mywarnx`（`units.c:109-124`）。
///
/// 非列表模式下 `errprefix` 是 `NULL`，所以句子走 `vwarnx` 到**标准错误**，
/// 形如 `<prog>: Unknown unit '<原字节>'`。列表模式下同一句换成走标准输出并带
/// `/ ` 前缀，那一段归 `list_units`（登记未做）。
///
/// 交回字节串而不是 `String`：`mywarnx("Unknown unit '%s'", *product)` 把
/// 单位名按原字节交出去（`units.c:554`），名字里有什么字节就打印什么字节。
#[must_use]
pub fn units_error_line(prog: &str, error: &UnitsError) -> Option<Vec<u8>> {
    // 词典行的错误由 `read_units` 那半处理，操作数推导这一路交不出这一句。
    if matches!(error, UnitsError::ReadError(_) | UnitsError::Conformability) {
        return None;
    }
    let mut line = Vec::with_capacity(prog.len() + 32);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    match error {
        UnitsError::UnknownUnit(name) => {
            line.extend_from_slice(b"Unknown unit '");
            line.extend_from_slice(name);
            line.extend_from_slice(b"'\n");
        }
        UnitsError::ReducesToZero => line.extend_from_slice(b"Unit reduces to zero\n"),
        UnitsError::JunkBeforeBar => line.extend_from_slice(b"Junk before '|'\n"),
        UnitsError::ReadError(_) | UnitsError::Conformability => return None,
    }
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> UnitsTable {
        let lines: &[&[u8]] = &[
            b"meter !m!",
            b"centimeter meter 0.01",
            b"inch centimeter 2.54",
            b"foot inch 12",
            b"yard foot 3",
            b"mile yard 1760",
            b"second !s!",
            b"minute 60 second",
            b"hour 60 minute",
            b"mph mile / hour",
            b"grams !gm!",
            b"gram grams",
            b"kilogram- 1000 grams",
            b"g grams",
        ];
        read_units(lines)
    }

    #[test]
    fn test_read_units_prefixes_and_comments() {
        let t = table();
        assert!(
            t.units
                .contains(&(b"inch".to_vec(), b"centimeter 2.54".to_vec()))
        );
        assert_eq!(
            t.prefixes,
            vec![(b"kilogram".to_vec(), b"1000 grams".to_vec())]
        );
    }

    #[test]
    fn test_simple_conversion_meters_to_feet_shape() {
        let t = table();
        let mut have = add_unit(b"meter", false).unwrap();
        complete_reduce(&t, &mut have).unwrap();
        let mut want = add_unit(b"inch", false).unwrap();
        complete_reduce(&t, &mut want).unwrap();
        // meter = 1 primitive (named "meter"); inch = 0.0254 m.
        let ans = show_answer(&have, &want);
        match ans {
            Answer::Factor(mul, div) => {
                assert!((mul - 1.0 / 0.0254).abs() < 1e-9);
                assert!((div - 0.0254).abs() < 1e-12);
            }
            other => panic!("unexpected answer {:?}", other),
        }
    }

    #[test]
    fn test_rational_and_repeat_parsing() {
        let t = table();
        let mut u = add_unit(b"3|4 inch2", false).unwrap();
        assert!((u.factor - 3.0 / 4.0).abs() < 1e-12);
        assert_eq!(u.numerator, vec![b"inch".to_vec(), b"inch".to_vec()]);
        complete_reduce(&t, &mut u).unwrap();
        // 3/4 in² reduced to meters: 0.75 × 0.0254².
        assert!((u.factor - 0.75 * 0.0254 * 0.0254).abs() < 1e-12);
        assert_eq!(u.numerator, vec![b"meter".to_vec(), b"meter".to_vec()]);
    }

    #[test]
    fn test_exponent_sign_kept() {
        // "1e-3 meter": the '-' after 'e' stays an exponent sign.
        let t = table();
        let mut u = add_unit(b"1e-3 meter", false).unwrap();
        complete_reduce(&t, &mut u).unwrap();
        assert!((u.factor - 0.001).abs() < 1e-15);
        assert_eq!(u.numerator, vec![b"meter".to_vec()]);
    }

    #[test]
    fn test_prefix_expansion() {
        let t = table();
        let mut u = add_unit(b"kilogram", false).unwrap();
        complete_reduce(&t, &mut u).unwrap();
        assert!((u.factor - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_plural_fallback() {
        let t = table();
        let mut u = add_unit(b"grams", false).unwrap();
        complete_reduce(&t, &mut u).unwrap();
        // The exact table entry wins, so grams stays "grams".
        assert_eq!(u.numerator, vec![b"grams".to_vec()]);
        assert!((u.factor - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_speed_conformability_with_denominators() {
        let t = table();
        let mut mph = add_unit(b"mph", false).unwrap();
        complete_reduce(&t, &mut mph).unwrap();
        let mut mps = add_unit(b"mile / minute", false).unwrap();
        complete_reduce(&t, &mut mps).unwrap();
        let ans = show_answer(&mph, &mps);
        match ans {
            Answer::Factor(mul, _) => assert!((mul - 1.0 / 60.0).abs() < 1e-12),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn test_conformability_error() {
        let t = table();
        let mut a = add_unit(b"meter", false).unwrap();
        complete_reduce(&t, &mut a).unwrap();
        let mut b = add_unit(b"second", false).unwrap();
        complete_reduce(&t, &mut b).unwrap();
        match show_answer(&a, &b) {
            Answer::Conformability(h, w) => {
                assert_eq!(h.numerator, vec![b"meter".to_vec()]);
                assert_eq!(w.numerator, vec![b"second".to_vec()]);
            }
            other => panic!("unexpected {:?}", other),
        }
    }

    /// 单位名的每一条腿都交回原字节：表键、表达式节点、`UnknownUnit` 载荷、
    /// `units_error_line` 那句的 `'%s'` 回显位。真源判据是 `units.c:503`/`:511`/
    /// `:542` 的 `strcmp`/`strncmp` 逐字节比，与 `units.c:554` 的
    /// `mywarnx("Unknown unit '%s'", *product)`——C 从不问这串字节是什么编码。
    #[test]
    fn a_unit_name_keeps_its_raw_bytes_through_the_table_and_the_warning() {
        // `café` 写作字面字节 `63 61 66 c3 a9`（表里没有这一名）。
        let name: &[u8] = &[b'c', b'a', b'f', 0xc3, 0xa9];
        let t = table();
        let mut u = add_unit(name, false).unwrap();
        assert_eq!(
            complete_reduce(&t, &mut u),
            Err(UnitsError::UnknownUnit(name.to_vec()))
        );
        assert_eq!(
            units_error_line("units", &UnitsError::UnknownUnit(name.to_vec())).as_deref(),
            Some(&b"units: Unknown unit 'caf\xc3\xa9'\n"[..])
        );
        // 表键同一串字节时命中：`strcmp` 只比字节。词典的一行是「名 定义」，
        // 所以这一枚夹具把两个字段拼在同一串里（一个元素就是一行）。
        let mut line = name.to_vec();
        line.extend_from_slice(b" !k!");
        // 第二枚表键含**非法** UTF-8 的裸 `ff`：只有逐字节比才认得它。任何在比较
        // 边界折一次 lossy 的写法（批次四百四十九否证的乙路）在非法侧解不开，
        // 于是查不到。
        let mut bad_line = Vec::from(&b"weird\xff"[..]);
        bad_line.extend_from_slice(b" 2 m");
        let raw_table = read_units(&[b"m !a!".as_slice(), line.as_slice(), bad_line.as_slice()]);
        assert_eq!(
            raw_table.units,
            vec![
                (b"m".to_vec(), b"!a!".to_vec()),
                (name.to_vec(), b"!k!".to_vec()),
                (b"weird\xff".to_vec(), b"2 m".to_vec())
            ]
        );
        assert_eq!(lookup_unit(&raw_table, name).as_deref(), Some(&b"!k!"[..]));
        assert_eq!(
            lookup_unit(&raw_table, b"weird\xff").as_deref(),
            Some(&b"2 m"[..]),
            "表键是非法 UTF-8 时也要按字节命中"
        );
        // 命中之后按字节继续换算：`weird\xff` = `2 m`，而 `m` 的定义带
        // PRIMITIVECHAR，`reduceproduct` 在这一格停下（`units.c:583-585` 的
        // `if (strchr(toadd, PRIMITIVECHAR)) break;`），所以因子是 2、名字剩 `m`。
        let mut u = add_unit(b"weird\xff", false).unwrap();
        complete_reduce(&raw_table, &mut u).unwrap();
        assert!((u.factor - 2.0).abs() < 1e-12);
        assert_eq!(u.numerator, vec![b"m".to_vec()]);
        // 第三道形：两串**不同**的字节折成 lossy 之后会同副面孔，而 `strcmp` 不上当
        // （这正是批次四百四十九否证乙路的那条理由）。表里先放字面替换字符那一枚
        // （`weird` + `ef bf bd`，定义 `5 m`），再放裸 `ff` 那一枚（定义 `2 m`）：
        // 按字节查 `weird\xff` 该命中后者；任何在比较边界折一次 lossy 的写法都会先
        // 撞上前者，把因子查成 5。
        let mut twin = Vec::from(&b"weird"[..]);
        twin.extend_from_slice("\u{fffd}".to_string().as_bytes());
        twin.extend_from_slice(b" 5 m");
        let colliding = read_units(&[b"m !a!".as_slice(), twin.as_slice(), bad_line.as_slice()]);
        assert_eq!(
            colliding.units.len(),
            3,
            "两枚键在 C 里是两串不同字节，都该进表"
        );
        assert_eq!(
            lookup_unit(&colliding, b"weird\xff").as_deref(),
            Some(&b"2 m"[..]),
            "查 `weird` + 裸 ff 该命中按字节那一枚，而不是折出来同形的字面替换字符那枚"
        );
    }

    /// 一行的**第二个**字段（定义原文）也按字节进表并按字节回显在 `-l` 清单里；
    /// `read_units` 不做任何解码，所以一枚裸字节不会让整份词典消失。
    #[test]
    fn a_units_line_with_a_raw_byte_still_reaches_the_table() {
        // 裸 `ff` 不是任何 UTF-8 序列的开头；旧写法在调用方解一次 UTF-8，
        // 解不过去就把**整份**词典丢掉（登记 UNITS-NONUTF8-DROP 的那一格）。
        let t = read_units(&[b"weird! 1\xff meter".as_slice()]);
        assert_eq!(t.units, vec![(b"weird!".to_vec(), b"1\xff meter".to_vec())]);
        let (listing, errors) = list_units(&t, false, PRECISION);
        assert_eq!(errors, 0);
        assert_eq!(
            listing,
            b"/ Primitive units\n/ Prefixes\n/ Other units\nweird!\t1\xff meter\n".as_slice()
        );
    }

    #[test]
    fn test_format_g_matches_printf() {
        assert_eq!(format_g(3.2808399, 8), "3.2808399");
        assert_eq!(format_g(2.54, 8), "2.54");
        assert_eq!(format_g(0.3048, 8), "0.3048");
        assert_eq!(format_g(60.0, 8), "60");
        assert_eq!(format_g(1e-5, 8), "1e-05");
        assert_eq!(format_g(1e24, 8), "1e+24");
        assert_eq!(format_g(-42.5, 8), "-42.5");
    }

    #[test]
    fn test_show_unit_shapes() {
        let u = UnitType {
            numerator: vec![b"m".to_vec(), b"m".to_vec()],
            denominator: vec![b"s".to_vec()],
            factor: 2.5,
        };
        assert_eq!(show_unit(&u, 8), b"\t2.5 m^2 / s\n".as_slice());
    }

    #[test]
    fn test_ring_only_f_takes_a_value() {
        // `"lLvqf:"` 里 `l`/`L`/`q` 都不带值，所以「带值选项缺值」那一支在本支只有 `f` 可达。
        let argv: &[&[u8]] = &[b"-lq".as_slice(), b"-L".as_slice()];
        let parsed = parse_units_args(argv).expect("-l/-q/-L 都不缺值");
        assert!(parsed.list);
        assert!(parsed.listexpand);
        assert!(parsed.quiet);
        assert!(parsed.operands.is_empty());
        assert_eq!(
            parse_units_args(&[b"-f".as_slice()]).err(),
            Some(UnitsArgError::Getopt(crate::GetoptReport::MissingValue(
                'f'
            )))
        );
        // 缺值那一行只由 libc 印，句子后面接的是调用方的 usage（`units.c:843-845`）。
        assert_eq!(
            parse_units_args(&[b"-lq".as_slice(), b"-f".as_slice()]).err(),
            Some(UnitsArgError::Getopt(crate::GetoptReport::MissingValue(
                'f'
            )))
        );
    }

    #[test]
    fn test_off_letter_follows_the_c_cluster_order() {
        // 参照件：`-Z`、`-qZ`、`-Zq` 三形的第一行都是 `units: unknown option -- Z`，
        // 差别只在 `q`/`l` 有没有先被置上——那一支走到 usage 就退出了，所以只有字母可见。
        use crate::GetoptReport;
        assert_eq!(
            parse_units_args(&[b"-Z".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption('Z')))
        );
        assert_eq!(
            parse_units_args(&[b"-qZ".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption('Z')))
        );
        assert_eq!(
            parse_units_args(&[b"-Zq".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption('Z')))
        );
        // 游标按字节走：短横后那一枚字节就是字母，`2d c3 a9` 交回的是 `c3`
        // （`getopt.c:79` 的 `optopt = *place++` 一次进一个字节）。载荷是 `char`，
        // 到报文那一步由 `GetoptReport::line_bytes` 把 `<= 0xFF` 的码位写回那一枚原字节。
        assert_eq!(
            parse_units_args(&[b"-\xc3\xa9".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption(
                '\u{00c3}'
            )))
        );
    }

    #[test]
    fn test_v_exits_inside_the_ring() {
        // `-v` 那两行打完就 `usage()`，所以它后面的字母永远扫不到；
        // 反过来 `-Zv` 只能看到集外字母那一行。两支都经参照件现量。
        assert_eq!(
            parse_units_args(&[b"-v".as_slice()]).err(),
            Some(UnitsArgError::Version)
        );
        assert_eq!(
            parse_units_args(&[b"-q".as_slice(), b"-v".as_slice()]).err(),
            Some(UnitsArgError::Version)
        );
        assert_eq!(
            parse_units_args(&[b"-vZ".as_slice()]).err(),
            Some(UnitsArgError::Version)
        );
        assert_eq!(
            parse_units_args(&[b"-Z".as_slice(), b"-v".as_slice()]).err(),
            Some(UnitsArgError::Getopt(crate::GetoptReport::UnknownOption(
                'Z'
            )))
        );
    }

    #[test]
    fn test_attached_value_and_next_token_value_both_work() {
        assert_eq!(
            parse_units_args(&[b"-fmy.lib".as_slice()])
                .expect("紧贴的值")
                .userfile,
            Some(b"my.lib".as_slice())
        );
        assert_eq!(
            parse_units_args(&[b"-f".as_slice(), b"my.lib".as_slice()])
                .expect("分开的值")
                .userfile,
            Some(b"my.lib".as_slice())
        );
        // 值取自下一个 token 时**不看它的脸色**：`-f -` 的档名就是 `-`。
        let argv: &[&[u8]] = &[
            b"-f".as_slice(),
            b"-".as_slice(),
            b"m".as_slice(),
            b"sec".as_slice(),
        ];
        let parsed = parse_units_args(argv).expect("- 可以被 -f 取走");
        assert_eq!(parsed.userfile, Some(b"-".as_slice()));
        assert_eq!(parsed.operands, vec![b"m".as_slice(), b"sec".as_slice()]);
        // 值取走之后，同一 token 剩下的字节不再当字母扫：`-fql` 的档名是整条尾巴 `ql`，
        // 而不是「`-f` 取 `q` 当值、`l` 再置列表」。
        let argv: &[&[u8]] = &[b"-fql".as_slice()];
        let parsed = parse_units_args(argv).expect("尾巴整段是值");
        assert_eq!(parsed.userfile, Some(b"ql".as_slice()));
        assert!(!parsed.list);
    }

    #[test]
    fn test_double_dash_is_consumed_and_bare_dash_is_an_operand() {
        // `getopt.c:81-85`：`--` 结束环且自身被推进掉；`:87-92`：裸 `-` 结束环却不推进，
        // 于是它自己是操作数（参照件对 `-f <档> - sec` 报的是 `Unknown unit '-'`）。
        assert!(
            parse_units_args(&[b"--".as_slice()])
                .expect("光写 -- 是零操作数")
                .operands
                .is_empty()
        );
        assert_eq!(
            parse_units_args(&[b"-".as_slice(), b"m".as_slice(), b"sec".as_slice()])
                .expect("裸 - 算操作数")
                .operands,
            vec![b"-".as_slice(), b"m".as_slice(), b"sec".as_slice()]
        );
        assert_eq!(
            parse_units_args(&[b"--".as_slice(), b"m".as_slice(), b"sec".as_slice()])
                .expect("-- 之后的两个是操作数")
                .operands,
            vec![b"m".as_slice(), b"sec".as_slice()]
        );
        // `--` 之后的 `-Z` 不是选项，于是这一形只剩一个操作数——走 usage 且没有 libc 那一行。
        assert_eq!(
            parse_units_args(&[b"--".as_slice(), b"-Z".as_slice()]).err(),
            Some(UnitsArgError::Usage)
        );
    }

    #[test]
    fn test_operand_count_check_precedes_any_file_access() {
        // C 的判据 `(argc != 3 && argc != 2 && argc != 0) || (list && argc != 0)`
        // 排在 `readunits` 之前，所以坏档加一个操作数报的是 usage 而不是开档失败。
        assert_eq!(
            parse_units_args(&[b"-f".as_slice(), b"db".as_slice(), b"m".as_slice()]).err(),
            Some(UnitsArgError::Usage)
        );
        assert_eq!(
            parse_units_args(&[
                b"-f".as_slice(),
                b"db".as_slice(),
                b"m".as_slice(),
                b"sec".as_slice(),
                b"km".as_slice(),
                b"cm".as_slice()
            ])
            .err(),
            Some(UnitsArgError::Usage)
        );
        assert_eq!(
            parse_units_args(&[b"-l".as_slice(), b"m".as_slice(), b"sec".as_slice()]).err(),
            Some(UnitsArgError::Usage)
        );
        assert!(
            parse_units_args(&[
                b"-f".as_slice(),
                b"db".as_slice(),
                b"m".as_slice(),
                b"sec".as_slice()
            ])
            .is_ok()
        );
        assert!(
            parse_units_args(&[
                b"-f".as_slice(),
                b"db".as_slice(),
                b"3".as_slice(),
                b"km".as_slice(),
                b"cm".as_slice()
            ])
            .is_ok()
        );
    }

    #[test]
    fn test_ring_does_not_rearrange() {
        // `m -l sec`：环在第一个非选项 token 就结束，`-l` 落进操作数（参照件把
        // `units -f <档> m sec -l` 的 `-l` 当成了 want 侧的单位名并报 `Unknown unit '-l'`）。
        let argv: &[&[u8]] = &[b"m".as_slice(), b"-l".as_slice(), b"sec".as_slice()];
        let parsed = parse_units_args(argv).expect("三个都是操作数");
        assert!(!parsed.list);
        assert_eq!(
            parsed.operands,
            vec![b"m".as_slice(), b"-l".as_slice(), b"sec".as_slice()]
        );
    }

    #[test]
    fn test_error_lines_quote_the_c_sentences() {
        // 非列表模式下 `errprefix` 是 `NULL`，所以 `mywarnx` 走 `vwarnx`：两段形、
        // 前缀是程序名、不带 `strerror`。三句的措辞各自抄 `units.c`。
        assert_eq!(
            units_error_line("units", &UnitsError::UnknownUnit(b"bogus".to_vec())).as_deref(),
            Some(&b"units: Unknown unit 'bogus'\n"[..])
        );
        assert_eq!(
            units_error_line("units", &UnitsError::ReducesToZero).as_deref(),
            Some(&b"units: Unit reduces to zero\n"[..])
        );
        assert_eq!(
            units_error_line("units", &UnitsError::JunkBeforeBar).as_deref(),
            Some(&b"units: Junk before '|'\n"[..])
        );
        // 词典行与可折算这两支不由 `mywarnx` 在推导路径上句子，交回 `None` 让调用方不印。
        assert_eq!(units_error_line("units", &UnitsError::Conformability), None);
        assert_eq!(units_error_line("units", &UnitsError::ReadError(7)), None);
    }

    #[test]
    fn test_colon_in_optstring_is_a_marker_not_a_letter() {
        // `getopt.c:98` 把 `:` 排除在可选字母之外，所以 `-:` 与聚簇里的 `-l:` 都走集外
        // 字母那一支；而 `-f:` 的冒号是**值**（值取自本 token 的尾巴，不查字母集合）。
        use crate::GetoptReport;
        assert_eq!(
            parse_units_args(&[b"-:".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption(':')))
        );
        assert_eq!(
            parse_units_args(&[b"-l:".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption(':')))
        );
        assert_eq!(
            parse_units_args(&[b"-f:".as_slice()])
                .expect("冒号是值")
                .userfile,
            Some(b":".as_slice())
        );
    }
}
