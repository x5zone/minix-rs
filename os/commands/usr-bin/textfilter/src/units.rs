//! units — conversion program (minix3/usr.bin/units/units.c).
//!
//! Deciding half: units-file parsing (definitions and `-`-suffixed
//! prefixes), expression parsing (`a|b` rationals, trailing-digit
//! repeats, `/` denominators), primitive reduction via lookup (with
//! the `^`/plural/prefix fallbacks), sorting+cancellation, the
//! conformability check and the `\t* \t/` answer rendering. The
//! interactive prompt face and units-file path search stay in the
//! bin (registered as 待开放路径 for the file operand).
use crate::floatfmt::{decimal_exponent, parse_float_prefix, pow10};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

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
    let mut unit = UnitType {
        factor: 1.0,
        ..UnitType::default()
    };
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

/// One product side: items split on spaces/`*`/tabs.
fn add_product(unit: &mut UnitType, product: &str, top: bool) -> Result<(), UnitsError> {
    for item in product.split(|c: char| c == ' ' || c == '*' || c == '\t' || c == '\n' || c == '/')
    {
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
            let names: Vec<String> = (if side {
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
                if def.contains(PRIMITIVECHAR) {
                    continue;
                }
                let product = if side {
                    &mut unit.numerator
                } else {
                    &mut unit.denominator
                };
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
    let an: Vec<&str> = a
        .numerator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let bn: Vec<&str> = b
        .numerator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let ad: Vec<&str> = a
        .denominator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let bd: Vec<&str> = b
        .denominator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    an == bn && ad == bd
}

/// compareunitsreciprocal: a's numerator matches b's denominator.
pub fn compare_units_reciprocal(a: &UnitType, b: &UnitType) -> bool {
    let an: Vec<&str> = a
        .numerator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let bd: Vec<&str> = b
        .denominator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let ad: Vec<&str> = a
        .denominator
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let bn: Vec<&str> = b
        .numerator
        .iter()
        .map(|s| s.as_str())
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
        Answer::Factor(mul, div) => format!(
            "\t* {}\n\t/ {}\n",
            format_g(*mul, prec),
            format_g(*div, prec)
        ),
        Answer::Reciprocal(mul, div) => format!(
            "\treciprocal conversion\n\t* {}\n\t/ {}\n",
            format_g(*mul, prec),
            format_g(*div, prec)
        ),
        Answer::Conformability(have, want) => {
            format!(
                "conformability error\n{}{}",
                show_unit(have, prec),
                show_unit(want, prec)
            )
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
            match add_unit(defn, false).and_then(|mut u| {
                complete_reduce(table, &mut u)?;
                Ok(u)
            }) {
                Err(_) => {
                    errors += 1;
                    printable = false;
                }
                Ok(u) => {
                    out.push_str(&format!("{}-{}", name, show_unit(&u, prec)));
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
            match add_unit(name, false).and_then(|mut u| {
                complete_reduce(table, &mut u)?;
                Ok(u)
            }) {
                Err(_) => {
                    errors += 1;
                    printable = false;
                }
                Ok(u) => {
                    out.push_str(&format!("{name}{}", show_unit(&u, prec)));
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

/// 前门跑完那个选项环之后手上有的东西（`units.c:818-833` 的 `switch` 各支置的变量）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitsArgs<'a> {
    /// `-f` 的值，C 的 `userfile`。
    pub userfile: Option<&'a str>,
    /// `-l` 或 `-L` 都置它（C 的 `list`）。
    pub list: bool,
    /// `-L` 才置它（C 的 `listexpand`）。
    pub listexpand: bool,
    /// `-q`（C 的 `quiet`）。
    pub quiet: bool,
    /// 环结束后的操作数，即 C 的 `argv += optind`。
    pub operands: Vec<&'a str>,
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
pub fn parse_units_args<'a>(args: &'a [&'a str]) -> Result<UnitsArgs<'a>, UnitsArgError> {
    const OPTSTRING: &[u8] = b"lLvqf:";
    let mut parsed = UnitsArgs::default();
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i];
        let bytes = arg.as_bytes();
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
/// 形如 `<prog>: Unknown unit '<名字>'`。列表模式下同一句换成走标准输出并带
/// `/ ` 前缀，那一段归 `list_units`（登记未做）。
#[must_use]
pub fn units_error_line(prog: &str, error: &UnitsError) -> Option<String> {
    let text = match error {
        UnitsError::UnknownUnit(name) => format!("Unknown unit '{name}'"),
        UnitsError::ReducesToZero => "Unit reduces to zero".to_string(),
        UnitsError::JunkBeforeBar => "Junk before '|'".to_string(),
        // 词典行的错误由 `read_units` 那半处理，操作数推导这一路交不出这一句。
        UnitsError::ReadError(_) | UnitsError::Conformability => return None,
    };
    Some(format!("{prog}: {text}\n"))
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
        assert!(
            t.units
                .contains(&("inch".to_string(), "centimeter 2.54".to_string()))
        );
        assert_eq!(
            t.prefixes,
            vec![("kilogram".to_string(), "1000 grams".to_string())]
        );
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

    #[test]
    fn test_ring_only_f_takes_a_value() {
        // `"lLvqf:"` 里 `l`/`L`/`q` 都不带值，所以「带值选项缺值」那一支在本支只有 `f` 可达。
        let parsed = parse_units_args(&["-lq", "-L"]).expect("-l/-q/-L 都不缺值");
        assert!(parsed.list);
        assert!(parsed.listexpand);
        assert!(parsed.quiet);
        assert!(parsed.operands.is_empty());
        assert_eq!(
            parse_units_args(&["-f"]).err(),
            Some(UnitsArgError::Getopt(crate::GetoptReport::MissingValue(
                'f'
            )))
        );
        // 缺值那一行只由 libc 印，句子后面接的是调用方的 usage（`units.c:843-845`）。
        assert_eq!(
            parse_units_args(&["-lq", "-f"]).err(),
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
            parse_units_args(&["-Z"]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption('Z')))
        );
        assert_eq!(
            parse_units_args(&["-qZ"]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption('Z')))
        );
        assert_eq!(
            parse_units_args(&["-Zq"]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption('Z')))
        );
        // 游标按字节走：`-é` 交回的是首字节 0xC3（libc 的 `%c` 打的就是那个字节）。
        // 报文里它渲染成 Latin-1 码点而非原始字节，这一处失真与 `rev`/`colrm` 同形。
        assert_eq!(
            parse_units_args(&["-é"]).err(),
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
            parse_units_args(&["-v"]).err(),
            Some(UnitsArgError::Version)
        );
        assert_eq!(
            parse_units_args(&["-q", "-v"]).err(),
            Some(UnitsArgError::Version)
        );
        assert_eq!(
            parse_units_args(&["-vZ"]).err(),
            Some(UnitsArgError::Version)
        );
        assert_eq!(
            parse_units_args(&["-Z", "-v"]).err(),
            Some(UnitsArgError::Getopt(crate::GetoptReport::UnknownOption(
                'Z'
            )))
        );
    }

    #[test]
    fn test_attached_value_and_next_token_value_both_work() {
        assert_eq!(
            parse_units_args(&["-fmy.lib"]).expect("紧贴的值").userfile,
            Some("my.lib")
        );
        assert_eq!(
            parse_units_args(&["-f", "my.lib"])
                .expect("分开的值")
                .userfile,
            Some("my.lib")
        );
        // 值取自下一个 token 时**不看它的脸色**：`-f -` 的档名就是 `-`。
        let parsed = parse_units_args(&["-f", "-", "m", "sec"]).expect("- 可以被 -f 取走");
        assert_eq!(parsed.userfile, Some("-"));
        assert_eq!(parsed.operands, vec!["m", "sec"]);
        // 值取走之后，同一 token 剩下的字节不再当字母扫：`-fql` 的档名是整条尾巴 `ql`，
        // 而不是「`-f` 取 `q` 当值、`l` 再置列表」。
        let parsed = parse_units_args(&["-fql"]).expect("尾巴整段是值");
        assert_eq!(parsed.userfile, Some("ql"));
        assert!(!parsed.list);
    }

    #[test]
    fn test_double_dash_is_consumed_and_bare_dash_is_an_operand() {
        // `getopt.c:81-85`：`--` 结束环且自身被推进掉；`:87-92`：裸 `-` 结束环却不推进，
        // 于是它自己是操作数（参照件对 `-f <档> - sec` 报的是 `Unknown unit '-'`）。
        assert!(
            parse_units_args(&["--"])
                .expect("光写 -- 是零操作数")
                .operands
                .is_empty()
        );
        assert_eq!(
            parse_units_args(&["-", "m", "sec"])
                .expect("裸 - 算操作数")
                .operands,
            vec!["-", "m", "sec"]
        );
        assert_eq!(
            parse_units_args(&["--", "m", "sec"])
                .expect("-- 之后的两个是操作数")
                .operands,
            vec!["m", "sec"]
        );
        // `--` 之后的 `-Z` 不是选项，于是这一形只剩一个操作数——走 usage 且没有 libc 那一行。
        assert_eq!(
            parse_units_args(&["--", "-Z"]).err(),
            Some(UnitsArgError::Usage)
        );
    }

    #[test]
    fn test_operand_count_check_precedes_any_file_access() {
        // C 的判据 `(argc != 3 && argc != 2 && argc != 0) || (list && argc != 0)`
        // 排在 `readunits` 之前，所以坏档加一个操作数报的是 usage 而不是开档失败。
        assert_eq!(
            parse_units_args(&["-f", "db", "m"]).err(),
            Some(UnitsArgError::Usage)
        );
        assert_eq!(
            parse_units_args(&["-f", "db", "m", "sec", "km", "cm"]).err(),
            Some(UnitsArgError::Usage)
        );
        assert_eq!(
            parse_units_args(&["-l", "m", "sec"]).err(),
            Some(UnitsArgError::Usage)
        );
        assert!(parse_units_args(&["-f", "db", "m", "sec"]).is_ok());
        assert!(parse_units_args(&["-f", "db", "3", "km", "cm"]).is_ok());
    }

    #[test]
    fn test_ring_does_not_rearrange() {
        // `m -l sec`：环在第一个非选项 token 就结束，`-l` 落进操作数（参照件把
        // `units -f <档> m sec -l` 的 `-l` 当成了 want 侧的单位名并报 `Unknown unit '-l'`）。
        let parsed = parse_units_args(&["m", "-l", "sec"]).expect("三个都是操作数");
        assert!(!parsed.list);
        assert_eq!(parsed.operands, vec!["m", "-l", "sec"]);
    }

    #[test]
    fn test_error_lines_quote_the_c_sentences() {
        // 非列表模式下 `errprefix` 是 `NULL`，所以 `mywarnx` 走 `vwarnx`：两段形、
        // 前缀是程序名、不带 `strerror`。三句的措辞各自抄 `units.c`。
        assert_eq!(
            units_error_line("units", &UnitsError::UnknownUnit("bogus".to_string())).as_deref(),
            Some("units: Unknown unit 'bogus'\n")
        );
        assert_eq!(
            units_error_line("units", &UnitsError::ReducesToZero).as_deref(),
            Some("units: Unit reduces to zero\n")
        );
        assert_eq!(
            units_error_line("units", &UnitsError::JunkBeforeBar).as_deref(),
            Some("units: Junk before '|'\n")
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
            parse_units_args(&["-:"]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption(':')))
        );
        assert_eq!(
            parse_units_args(&["-l:"]).err(),
            Some(UnitsArgError::Getopt(GetoptReport::UnknownOption(':')))
        );
        assert_eq!(
            parse_units_args(&["-f:"]).expect("冒号是值").userfile,
            Some(":")
        );
    }
}
