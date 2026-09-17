//! units — conversion program (minix3/usr.bin/units/units.c).
//!
//! Deciding half: units-file parsing (definitions and `-`-suffixed
//! prefixes), expression parsing (`a|b` rationals, trailing-digit
//! repeats, `/` denominators), primitive reduction via lookup (with
//! the `^`/plural/prefix fallbacks), sorting+cancellation, the
//! conformability check and the `\t* \t/` answer rendering. The
//! interactive prompt face and units-file path search stay in the
//! bin (registered as 待开放路径 for the file operand).
use alloc::vec::Vec;
use crate::floatfmt::{decimal_exponent, parse_float_prefix, pow10};
use alloc::string::{String, ToString};
use alloc::format;

/// Precision of `%.*g` in showunit/showanswer (units.c:43).
pub const PRECISION: usize = 8;
/// `-L` uses DBL_DIG.
pub const PRECISION_LIST_EXPAND: usize = 15;
/// Definitions with a leading `!` are primitives.
pub const PRIMITIVECHAR: char = '!';
pub const POWERSTRING: &str = "^";

/// The parsed units database.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitsTable {
    pub units: Vec<(String, String)>,
    pub prefixes: Vec<(String, String)>,
}

/// Errors mirroring the C warnings (which go to stderr, or stdout as
/// `/ ` comments in list mode).
#[derive(Debug, PartialEq)]
pub enum UnitsError {
    UnknownUnit(String),
    ReducesToZero,
    JunkBeforeBar,
    ReadError(usize),
    Conformability,
}

/// A reduced unit: factor times numerator/denominator products.
/// Cancelled entries hold the empty string (the C's NULLUNIT).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitType {
    pub numerator: Vec<String>,
    pub denominator: Vec<String>,
    pub factor: f64,
}

/// Parses the units file line by line (readunits, units.c:134-229):
/// lines starting with `/` are comments; a first token ending in `-`
/// defines a prefix; otherwise `name definition`.
pub fn read_units(lines: &[&str]) -> UnitsTable {
    let mut table = UnitsTable::default();
    for line in lines {
        if line.starts_with('/') {
            continue;
        }
        let trimmed = line.trim_matches(|c: char| c == ' ' || c == '\n' || c == '\t');
        let mut parts = trimmed.splitn(2, |c: char| c == ' ' || c == '\n' || c == '\t');
        let name = match parts.next() {
            Some(n) => n,
            None => continue,
        };
        let rest = match parts.next() {
            Some(r) => r.trim_start_matches(|c: char| c == ' ' || c == '\n' || c == '\t'),
            None => "",
        };
        if name.is_empty() {
            continue;
        }
        if let Some(stem) = name.strip_suffix('-') {
            // A prefix definition; duplicates are ignored.
            if stem.is_empty() || rest.is_empty() {
                continue;
            }
            if !table.prefixes.iter().any(|(n, _)| n == stem) {
                table.prefixes.push((stem.to_string(), rest.to_string()));
            }
        } else if rest.is_empty() {
            continue;
        } else if !table.units.iter().any(|(n, _)| n == name) {
            table.units.push((name.to_string(), rest.to_string()));
        }
    }
    table
}

/// addunit: parses one unit expression into `theunit`, flipping into
/// the denominator past `/` (units.c:332-424).
pub fn add_unit(expr: &str, flip: bool) -> Result<UnitType, UnitsError> {
    let mut unit = UnitType { factor: 1.0, ..UnitType::default() };
    add_into(&mut unit, expr, flip)?;
    Ok(unit)
}

fn add_into(unit: &mut UnitType, expr: &str, flip: bool) -> Result<(), UnitsError> {
    // The C rewrites '-' to ' ' unless it is an exponent sign
    // (preceded by e/E and followed by a digit or dot).
    let bytes: Vec<u8> = expr.as_bytes().to_vec();
    let mut scratch = String::with_capacity(bytes.len());
    for i in 0..bytes.len() {
        let c = bytes[i];
        if c == b'-'
            && (i == 0
                || bytes[i - 1].to_ascii_lowercase() != b'e'
                || i + 1 >= bytes.len()
                || !(bytes[i + 1].is_ascii_digit() || bytes[i + 1] == b'.'))
        {
            scratch.push(' ');
        } else {
            scratch.push(c as char);
        }
    }
    let (top, bottom) = match scratch.find('/') {
        Some(slash) => (&scratch[..slash], Some(&scratch[slash + 1..])),
        None => (scratch.as_str(), None),
    };
    let mut doingtop = true;
    loop {
        add_product(unit, top, doingtop != flip)?;
        match bottom {
            Some(b) if doingtop => {
                doingtop = false;
                add_product(unit, b, doingtop != flip)?;
                break;
            }
            _ => break,
        }
    }
    Ok(())
}

/// One product side: items split on spaces/`*`/tabs.
fn add_product(unit: &mut UnitType, product: &str, top: bool) -> Result<(), UnitsError> {
    for item in product.split(|c: char| c == ' ' || c == '*' || c == '\t' || c == '\n' || c == '/') {
        if item.is_empty() {
            continue;
        }
        if item.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            // Numeric factor, possibly "6|2" rational, possibly with
            // trailing unit names ("3foo" is "3 foo").
            let mut rest = item;
            loop {
                // strtod-shaped numeric head: digits, dot, exponent.
                let (num, used) = match parse_float_prefix(rest) {
                    Some(x) => x,
                    None => break,
                };
                let tail = &rest[used..];
                if let Some(after) = tail.strip_prefix('|') {
                    // Rational: num|den.
                    let den_len = after
                        .bytes()
                        .take_while(|b| b.is_ascii_digit() || *b == b'.')
                        .count();
                    if den_len == 0 || num == 0.0 {
                        if num == 0.0 {
                            return Err(UnitsError::ReducesToZero);
                        }
                        return Err(UnitsError::JunkBeforeBar);
                    }
                    let (den_str, tail2) = after.split_at(den_len);
                    let den: f64 = den_str.parse().unwrap_or(0.0);
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
                    if tail.contains('|') {
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
                if rest.is_empty() || !rest.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
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
fn add_names(unit: &mut UnitType, item: &str, top: bool) -> Result<(), UnitsError> {
    let (name, repeat) = match item.as_bytes().last() {
        Some(c) if (b'2'..=b'9').contains(c) => (&item[..item.len() - 1], c - b'0'),
        _ => (item, 1),
    };
    for _ in 0..repeat {
        if top {
            unit.numerator.push(name.to_string());
        } else {
            unit.denominator.push(name.to_string());
        }
    }
    Ok(())
}

/// lookupunit: exact name, `^`-stripped, singular (`s`/`es`), then
/// prefix expansion (units.c:470-537). Returns the definition text.
pub fn lookup_unit<'a>(table: &'a UnitsTable, unit: &str) -> Option<String> {
    for (n, v) in &table.units {
        if n == unit {
            return Some(v.clone());
        }
    }
    if let Some(stem) = unit.strip_suffix('^') {
        for (n, _) in &table.units {
            if n == stem {
                return Some(stem.to_string());
            }
        }
    }
    if let Some(stem) = unit.strip_suffix('s') {
        for (n, _) in &table.units {
            if n == stem {
                return Some(stem.to_string());
            }
        }
        if let Some(stem2) = stem.strip_suffix('e') {
            for (n, _) in &table.units {
                if n == stem2 {
                    return Some(stem2.to_string());
                }
            }
        }
    }
    for (p, pv) in &table.prefixes {
        if unit.starts_with(p.as_str()) {
            // A bare prefix name is legal: the rest may be empty.
            let rest = &unit[p.len()..];
            if rest.is_empty() || lookup_unit(table, rest).is_some() {
                return Some(format!("{} {}", pv, rest));
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
            let names: Vec<String> = (if side { &unit.numerator } else { &unit.denominator }).clone();
            for name in names {
                if name.is_empty() {
                    continue;
                }
                let def = match lookup_unit(table, &name) {
                    Some(d) => d,
                    None => return Err(UnitsError::UnknownUnit(name)),
                };
                if def.contains(PRIMITIVECHAR) {
                    continue;
                }
                let product = if side { &mut unit.numerator } else { &mut unit.denominator };
                for p in product.iter_mut() {
                    if *p == name {
                        *p = String::new();
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
    let mut den = unit.denominator.clone().into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let mut num = unit.numerator.clone().into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
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
    let an: Vec<&str> = a.numerator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    let bn: Vec<&str> = b.numerator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    let ad: Vec<&str> = a.denominator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    let bd: Vec<&str> = b.denominator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    an == bn && ad == bd
}

/// compareunitsreciprocal: a's numerator matches b's denominator.
pub fn compare_units_reciprocal(a: &UnitType, b: &UnitType) -> bool {
    let an: Vec<&str> = a.numerator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    let bd: Vec<&str> = b.denominator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    let ad: Vec<&str> = a.denominator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    let bn: Vec<&str> = b.numerator.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
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
    let body = if body.is_empty() { "0".to_string() } else { body };
    let sign = if neg { "-" } else { "" };
    if exp_used {
        format!("{}{}e{}{:02}", sign, body, if exp < 0 { "-" } else { "+" }, exp.abs())
    } else {
        format!("{}{}", sign, body)
    }
}

/// showunit: `\t<factor>` + ` name^2` products and ` / name` denominators.
pub fn show_unit(unit: &UnitType, prec: usize) -> String {
    let mut out = format!("\t{}", format_g(unit.factor, prec));
    let mut counter = 1;
    for (i, name) in unit.numerator.iter().enumerate() {
        if i > 0 && !name.is_empty() && name == &unit.numerator[i - 1] {
            counter += 1;
        } else {
            if counter > 1 {
                out.push_str(POWERSTRING);
                out.push_str(&counter.to_string());
            }
            if !name.is_empty() {
                out.push(' ');
                out.push_str(name);
            }
            counter = 1;
        }
    }
    if counter > 1 {
        out.push_str(POWERSTRING);
        out.push_str(&counter.to_string());
    }
    let mut printed_slash = false;
    counter = 1;
    for (i, name) in unit.denominator.iter().enumerate() {
        if i > 0 && !name.is_empty() && name == &unit.denominator[i - 1] {
            counter += 1;
        } else {
            if counter > 1 {
                out.push_str(POWERSTRING);
                out.push_str(&counter.to_string());
            }
            if !name.is_empty() {
                if !printed_slash {
                    out.push_str(" /");
                    printed_slash = true;
                }
                out.push(' ');
                out.push_str(name);
            }
            counter = 1;
        }
    }
    if counter > 1 {
        out.push_str(POWERSTRING);
        out.push_str(&counter.to_string());
    }
    out.push('\n');
    out
}

/// Renders an Answer into the exact stdout text.
pub fn render_answer(answer: &Answer, prec: usize) -> String {
    match answer {
        Answer::Factor(mul, div) => format!("\t* {}\n\t/ {}\n", format_g(*mul, prec), format_g(*div, prec)),
        Answer::Reciprocal(mul, div) => format!(
            "\treciprocal conversion\n\t* {}\n\t/ {}\n",
            format_g(*mul, prec),
            format_g(*div, prec)
        ),
        Answer::Conformability(have, want) => {
            format!("conformability error\n{}{}", show_unit(have, prec), show_unit(want, prec))
        }
    }
}

/// listunits (`-l`/`-L`): dump primitives, prefixes, then all other
/// units; `expand` reduces each definition to primitive form
/// (units.c:694-779). Returns the printed text and the error count.
pub fn list_units(table: &UnitsTable, expand: bool, prec: usize) -> (String, usize) {
    let mut out = String::new();
    let mut errors = 0usize;
    out.push_str("/ Primitive units\n");
    for (name, defn) in &table.units {
        if defn.starts_with(PRIMITIVECHAR) {
            out.push_str(&format!("{}\t{}\n", name, defn));
        }
    }
    out.push_str("/ Prefixes\n");
    for (name, defn) in &table.prefixes {
        let mut printable = true;
        if expand {
            match add_unit(defn, false)
                .and_then(|mut u| {
                    complete_reduce(table, &mut u)?;
                    Ok(u)
                }) {
                Err(_) => {
                    errors += 1;
                    printable = false;
                }
                Ok(u) => {
                    out.push_str(&format!("{}-{}\n", name, show_unit(&u, prec)));
                    continue;
                }
            }
        }
        if printable {
            out.push_str(&format!("{}-\t{}\n", name, defn));
        }
    }
    out.push_str("/ Other units\n");
    for (name, defn) in &table.units {
        if defn.starts_with(PRIMITIVECHAR) {
            continue;
        }
        let mut printable = true;
        if expand {
            // The C expands the NAME, catching bad names too.
            match add_unit(name, false)
                .and_then(|mut u| {
                    complete_reduce(table, &mut u)?;
                    Ok(u)
                }) {
                Err(_) => {
                    errors += 1;
                    printable = false;
                }
                Ok(u) => {
                    out.push_str(&format!("{}{}\n", name, show_unit(&u, prec)));
                    continue;
                }
            }
        }
        if printable {
            out.push_str(&format!("{}\t{}\n", name, defn));
        }
    }
    (out, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> UnitsTable {
        read_units(&[
            "meter !m!",
            "centimeter meter 0.01",
            "inch centimeter 2.54",
            "foot inch 12",
            "yard foot 3",
            "mile yard 1760",
            "second !s!",
            "minute 60 second",
            "hour 60 minute",
            "mph mile / hour",
            "grams !gm!",
            "gram grams",
            "kilogram- 1000 grams",
            "g grams",
        ])
    }

    #[test]
    fn test_read_units_prefixes_and_comments() {
        let t = table();
        assert!(t.units.contains(&("inch".to_string(), "centimeter 2.54".to_string())));
        assert_eq!(t.prefixes, vec![("kilogram".to_string(), "1000 grams".to_string())]);
    }

    #[test]
    fn test_simple_conversion_meters_to_feet_shape() {
        let t = table();
        let mut have = add_unit("meter", false).unwrap();
        complete_reduce(&t, &mut have).unwrap();
        let mut want = add_unit("inch", false).unwrap();
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
        let mut u = add_unit("3|4 inch2", false).unwrap();
        assert!((u.factor - 3.0 / 4.0).abs() < 1e-12);
        assert_eq!(u.numerator, vec!["inch".to_string(), "inch".to_string()]);
        complete_reduce(&t, &mut u).unwrap();
        // 3/4 in² reduced to meters: 0.75 × 0.0254².
        assert!((u.factor - 0.75 * 0.0254 * 0.0254).abs() < 1e-12);
        assert_eq!(u.numerator, vec!["meter".to_string(), "meter".to_string()]);
    }

    #[test]
    fn test_exponent_sign_kept() {
        // "1e-3 meter": the '-' after 'e' stays an exponent sign.
        let t = table();
        let mut u = add_unit("1e-3 meter", false).unwrap();
        complete_reduce(&t, &mut u).unwrap();
        assert!((u.factor - 0.001).abs() < 1e-15);
        assert_eq!(u.numerator, vec!["meter".to_string()]);
    }

    #[test]
    fn test_prefix_expansion() {
        let t = table();
        let mut u = add_unit("kilogram", false).unwrap();
        complete_reduce(&t, &mut u).unwrap();
        assert!((u.factor - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_plural_fallback() {
        let t = table();
        let mut u = add_unit("grams", false).unwrap();
        complete_reduce(&t, &mut u).unwrap();
        // The exact table entry wins, so grams stays "grams".
        assert_eq!(u.numerator, vec!["grams".to_string()]);
        assert!((u.factor - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_speed_conformability_with_denominators() {
        let t = table();
        let mut mph = add_unit("mph", false).unwrap();
        complete_reduce(&t, &mut mph).unwrap();
        let mut mps = add_unit("mile / minute", false).unwrap();
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
        let mut a = add_unit("meter", false).unwrap();
        complete_reduce(&t, &mut a).unwrap();
        let mut b = add_unit("second", false).unwrap();
        complete_reduce(&t, &mut b).unwrap();
        match show_answer(&a, &b) {
            Answer::Conformability(h, w) => {
                assert_eq!(h.numerator, vec!["meter".to_string()]);
                assert_eq!(w.numerator, vec!["second".to_string()]);
            }
            other => panic!("unexpected {:?}", other),
        }
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
            numerator: vec!["m".to_string(), "m".to_string()],
            denominator: vec!["s".to_string()],
            factor: 2.5,
        };
        assert_eq!(show_unit(&u, 8), "\t2.5 m^2 / s\n");
    }
}
