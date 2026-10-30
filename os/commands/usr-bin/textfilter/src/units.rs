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
    // C `addunit` (`minix3/usr.bin/units/units.c:359-436`) is `do { …; doingtop--;
    // if (slash) { scratch = slash + 1; } else doingtop--; } while (doingtop >= 0);`
    // — `doingtop` starts at 1 and every round ends with `doingtop--`; without a
    // slash there is one extra `--`, so two `--` in total bring it to `-1` and
    // the next `>= 0` test exits. At most two rounds (top first, then bottom),
    // never a third. The `loop` in the Rust translation below is that "at most
    // two rounds" read literally (`doingtop: bool` is true on the first frame
    // and already fixed to false for the next, and both `match bottom` arms
    // break — which is what clippy::unused_loop picked out). Unrolled into two
    // sequential steps with the same semantics as do-while: the first round is
    // `doingtop != flip == true != flip == !flip`, and a bottom round, when
    // present, is `doingtop != flip == false != flip == flip`.
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

/// What the front door holds after running that option ring (the variables
/// each `switch` arm of `units.c:818-833` sets).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitsArgs<'a> {
    /// The `-f` value, C's `userfile`. It is a **path**: the leg that actually
    /// opens it waits on the library-side byte interface, so this layer only
    /// guarantees the value is not folded to text ahead of time.
    pub userfile: Option<&'a [u8]>,
    /// Set by either `-l` or `-L` (C's `list`).
    pub list: bool,
    /// Set by `-L` only (C's `listexpand`).
    pub listexpand: bool,
    /// `-q` (C's `quiet`).
    pub quiet: bool,
    /// The operands after the ring ends, C's `argv += optind`. They are the
    /// unit-name expressions, so raw bytes flow straight into `fn add_unit`
    /// (C hands `argv` over as is at `units.c:855-875`).
    pub operands: Vec<&'a [u8]>,
}

/// The front door's three kinds of exit. Each sentence has its own owner, so
/// none of these is an abridged or expanded version of another:
/// [`UnitsArgError::Getopt`]'s line is printed by libc `getopt` and is always
/// followed by usage (`units.c:843-845`'s `default: usage()`);
/// [`UnitsArgError::Version`] is the two `-v` lines plus usage
/// (`units.c:836-841`, both lines on **standard error**);
/// [`UnitsArgError::Usage`] is the usage block alone, without libc's line (the
/// operand count is wrong, `units.c:849-851`). All three exit 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitsArgError {
    Getopt(crate::GetoptReport),
    Version,
    Usage,
}

/// `getopt(argc, argv, "lLvqf:")` of `units.c:817`.
///
/// Only `f` takes a value, so the "value-taking option is missing its value"
/// arm is reachable through `f` alone in this ring; `l`/`L` are two distinct
/// letters rather than a case pair (`-l` lists original names, `-L` lists the
/// expanded form), and `v` exits inside the ring, so `-vZ` yields only the
/// version block and `-Zv` only the unknown-letter line plus usage — both
/// measured against the reference binary.
///
/// The cursor walks **bytes** rather than `chars()`: the reference binary
/// reports the leading byte `\303` for `-é` (libc's `%c` prints that byte
/// itself), and a character-wise walk would step past a character boundary on
/// non-ASCII bytes (the same shape as the `column`/`col` sites).
pub fn parse_units_args<'a>(args: &'a [&'a [u8]]) -> Result<UnitsArgs<'a>, UnitsArgError> {
    const OPTSTRING: &[u8] = b"lLvqf:";
    let mut parsed = UnitsArgs::default();
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i];
        let bytes = arg;
        // A token not starting with `-`, and the single dash itself, both end
        // the option ring at `getopt.c:74-78` and `:87-92` and are **not
        // consumed**, so the token becomes the first operand itself (no
        // permutation).
        if bytes.len() < 2 || bytes[0] != b'-' {
            break;
        }
        // `--` ends the ring at `getopt.c:81-85` and is advanced past itself,
        // so it is not an operand.
        if bytes == b"--" {
            i += 1;
            break;
        }
        let mut k = 1usize;
        while k < bytes.len() {
            let letter = bytes[k];
            k += 1;
            // `:` in the optstring is only the marker "the previous letter
            // takes a value": libc's test is `optopt == ':' ||
            // strchr(ostr, optopt) == NULL` (`getopt.c:98`), so it is never an
            // acceptable letter, and `-:` lands in the unknown-letter arm.
            let Some(slot) = OPTSTRING.iter().position(|&o| o == letter && o != b':') else {
                return Err(UnitsArgError::Getopt(crate::GetoptReport::UnknownOption(
                    char::from(letter),
                )));
            };
            if OPTSTRING.get(slot + 1) == Some(&b':') {
                // A value-taking letter: the value either hugs the tail of
                // this token or occupies the next token (`getopt.c:112-141`).
                // Either way this token ends, and the tail is no longer scanned
                // as letters.
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
                // Once the value is taken, the remaining bytes of this token
                // are no longer scanned as letters (libc advances `optind`
                // directly).
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
    // C's test runs after the ring and before `readunits`
    // (`units.c:849-851`), so "wrong operand count + unusable file" reports
    // usage rather than a file-open failure — the order is observable.
    if !matches!(parsed.operands.len(), 0 | 2 | 3) || (parsed.list && !parsed.operands.is_empty()) {
        return Err(UnitsArgError::Usage);
    }
    Ok(parsed)
}

/// The C-side `mywarnx` sentence a `UnitsError` corresponds to
/// (`units.c:109-124`).
///
/// Outside list mode `errprefix` is `NULL`, so the sentence goes through
/// `vwarnx` to **standard error**, shaped `<prog>: Unknown unit '<raw
/// bytes>'`. In list mode the same sentence goes to standard output with a
/// `/ ` prefix instead, and that face belongs to `list_units` (declared as not
/// done).
///
/// It returns a byte string rather than a `String`:
/// `mywarnx("Unknown unit '%s'", *product)` hands the unit name out as raw
/// bytes (`units.c:554`), so whatever bytes the name has are the bytes
/// printed.
#[must_use]
pub fn units_error_line(prog: &str, error: &UnitsError) -> Option<Vec<u8>> {
    // Errors from a dictionary line are handled by the `read_units` half; the
    // operand-derivation path cannot produce this sentence.
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

    /// Every leg of a unit name hands back raw bytes: the table key, the
    /// expression node, the `UnknownUnit` payload, and the `'%s'` echo field of
    /// the `units_error_line` sentence. The ground truth is the byte-wise
    /// `strcmp`/`strncmp` at `units.c:503`/`:511`/`:542` together with
    /// `units.c:554`'s `mywarnx("Unknown unit '%s'", *product)` — C never asks
    /// what encoding these bytes are.
    #[test]
    fn a_unit_name_keeps_its_raw_bytes_through_the_table_and_the_warning() {
        // `café` is written as the literal bytes `63 61 66 c3 a9` (the table
        // has no such name).
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
        // A table key of the very same bytes hits: `strcmp` compares bytes
        // only. One dictionary line is "name definition", so this fixture
        // joins both fields into a single string (one element is one line).
        let mut line = name.to_vec();
        line.extend_from_slice(b" !k!");
        // The second table key carries a raw `ff`, which is **illegal**
        // UTF-8: only a byte-wise comparison recognizes it. Any variant that
        // folds a lossy conversion at the comparison boundary (a route
        // rejected earlier) cannot decode the illegal side, so the lookup
        // misses.
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
        // After the hit the conversion continues byte-wise: `weird\xff` =
        // `2 m`, and the definition of `m` carries PRIMITIVECHAR, so
        // `reduceproduct` stops on this cell (`units.c:583-585`'s
        // `if (strchr(toadd, PRIMITIVECHAR)) break;`), leaving a factor of 2 and
        // the name `m`.
        let mut u = add_unit(b"weird\xff", false).unwrap();
        complete_reduce(&raw_table, &mut u).unwrap();
        assert!((u.factor - 2.0).abs() < 1e-12);
        assert_eq!(u.numerator, vec![b"m".to_vec()]);
        // Third shape: two **different** byte strings fold to the same lossy
        // face, and `strcmp` is not fooled (exactly the reason the lossy route
        // was rejected). The table gets the literal replacement character
        // first (`weird` + `ef bf bd`, defined as `5 m`), then the raw `ff` one
        // (defined as `2 m`): a byte-wise lookup of `weird\xff` must hit the
        // latter, while any variant folding a lossy conversion at the
        // comparison boundary hits the former and finds a factor of 5.
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

    /// The **second** field of a line (the raw definition) also enters the
    /// table as bytes and is echoed as bytes in the `-l` listing; `read_units`
    /// does no decoding, so a single raw byte cannot make the whole dictionary
    /// vanish.
    #[test]
    fn a_units_line_with_a_raw_byte_still_reaches_the_table() {
        // A raw `ff` does not start any UTF-8 sequence; the older code decoded
        // UTF-8 once at the caller and, on failure, dropped the **whole**
        // dictionary.
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
        // In `"lLvqf:"` the letters `l`/`L`/`q` take no value, so the
        // "value-taking option is missing its value" arm is reachable only
        // through `f` in this ring.
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
        // The missing-value line is printed by libc alone, and the caller's
        // usage follows the sentence (`units.c:843-845`).
        assert_eq!(
            parse_units_args(&[b"-lq".as_slice(), b"-f".as_slice()]).err(),
            Some(UnitsArgError::Getopt(crate::GetoptReport::MissingValue(
                'f'
            )))
        );
    }

    #[test]
    fn test_off_letter_follows_the_c_cluster_order() {
        // Reference binary: `-Z`, `-qZ` and `-Zq` all print
        // `units: unknown option -- Z` as their first line; the only
        // difference is whether `q`/`l` were already set — that arm exits into
        // usage, so only the letter is observable.
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
        // The cursor walks bytes: the byte after the dash is the letter, and
        // `2d c3 a9` hands back `c3` (`getopt.c:79`'s `optopt = *place++`
        // consumes one byte at a time). The payload is a `char`, and at the
        // message stage `GetoptReport::line_bytes` writes a code point `<= 0xFF`
        // back as that raw byte.
        assert_eq!(
            parse_units_args(&[b"-\xc3\xa9".as_slice()]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption(
                '\u{00c3}'
            )))
        );
    }

    #[test]
    fn test_v_exits_inside_the_ring() {
        // Once `-v` prints its two lines it calls `usage()`, so letters after
        // it are never scanned; conversely `-Zv` can only show the
        // unknown-letter line. Both measured against the reference binary.
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
        // A value taken from the next token is accepted **without inspecting
        // it**: the file name of `-f -` is `-`.
        let argv: &[&[u8]] = &[
            b"-f".as_slice(),
            b"-".as_slice(),
            b"m".as_slice(),
            b"sec".as_slice(),
        ];
        let parsed = parse_units_args(argv).expect("- 可以被 -f 取走");
        assert_eq!(parsed.userfile, Some(b"-".as_slice()));
        assert_eq!(parsed.operands, vec![b"m".as_slice(), b"sec".as_slice()]);
        // Once the value is taken, the remaining bytes of the same token are
        // no longer scanned as letters: the file name of `-fql` is the whole
        // tail `ql`, not "`-f` takes `q` as its value and `l` then sets the
        // list".
        let argv: &[&[u8]] = &[b"-fql".as_slice()];
        let parsed = parse_units_args(argv).expect("尾巴整段是值");
        assert_eq!(parsed.userfile, Some(b"ql".as_slice()));
        assert!(!parsed.list);
    }

    #[test]
    fn test_double_dash_is_consumed_and_bare_dash_is_an_operand() {
        // `getopt.c:81-85`: `--` ends the ring and is advanced past itself;
        // `:87-92`: a bare `-` ends the ring but is not advanced, so it is an
        // operand itself (the reference binary reports `Unknown unit '-'` for
        // `-f <file> - sec`).
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
        // The `-Z` after `--` is not an option, so this shape has just one
        // operand — it goes to usage and has no libc line.
        assert_eq!(
            parse_units_args(&[b"--".as_slice(), b"-Z".as_slice()]).err(),
            Some(UnitsArgError::Usage)
        );
    }

    #[test]
    fn test_operand_count_check_precedes_any_file_access() {
        // C's test `(argc != 3 && argc != 2 && argc != 0) || (list && argc != 0)`
        // runs before `readunits`, so a bad dictionary plus one operand reports
        // usage rather than a file-open failure.
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
        // `m -l sec`: the ring ends at the first non-option token and `-l`
        // falls among the operands (the reference binary takes the `-l` of
        // `units -f <file> m sec -l` as a unit name on the want side and
        // reports `Unknown unit '-l'`).
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
        // Outside list mode `errprefix` is `NULL`, so `mywarnx` goes through
        // `vwarnx`: two segments, the program name as the prefix, no
        // `strerror`. Each of the three sentences copies its wording from
        // `units.c`.
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
        // The dictionary-line and conformability arms are not `mywarnx`
        // sentences on the derivation path; they return `None` so the caller
        // prints nothing.
        assert_eq!(units_error_line("units", &UnitsError::Conformability), None);
        assert_eq!(units_error_line("units", &UnitsError::ReadError(7)), None);
    }

    #[test]
    fn test_colon_in_optstring_is_a_marker_not_a_letter() {
        // `getopt.c:98` excludes `:` from the acceptable letters, so `-:` and
        // a `-l:` inside a cluster both take the unknown-letter arm; in `-f:`
        // the colon is the **value** (taken from this token's tail, with no
        // letter set consulted).
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
