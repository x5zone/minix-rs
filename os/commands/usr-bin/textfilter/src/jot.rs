//! jot — print sequential or random data (minix3/usr.bin/jot/jot.c).
//!
//! Deciding half: option derivation (the right-to-left positional
//! arguments), format selection and value rendering. The RNG is
//! injected so `-r` is testable without libc's random(3); the exact
//! NetBSD random() bit stream is not reproduced (registered in 07
//! §4.4 as a remaining corner).
use crate::floatfmt::{decimal_exponent, floor, pow10};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// NetBSD jot's default repetitions.
pub const REPS_DEF: i64 = 100;
pub const BEGIN_DEF: f64 = 1.0;
pub const ENDER_DEF: f64 = 100.0;
pub const STEP_DEF: f64 = 1.0;

/// Which positional operands were given (`-` means defaulted).
pub const HAVE_BEGIN: u8 = 1;
pub const HAVE_STEP: u8 = 2;
pub const HAVE_REPS: u8 = 4;
pub const HAVE_ENDER: u8 = 8;

/// Errors mirroring the C `errx` exits.
#[derive(Debug, PartialEq)]
pub enum JotError {
    BadPrecision,
    BadReps(String),
    BadStep(String),
    BadFormat(&'static str),
    ImpossibleStepsize,
    MustSpecifyBegin,
    InfiniteUnbounded,
    TooManyArgs(String),
}

/// The parsed conversion of a `-w` word (or the synthesized default).
#[derive(Debug, PartialEq)]
pub enum Format {
    /// `-b word`: the word itself, repeated verbatim.
    Literal(String),
    /// One conversion with the surrounding word text.
    Conv(Conv),
}

/// A single printf conversion plus its literal context.
#[derive(Debug, PartialEq)]
pub struct Conv {
    /// Text before the conversion (the whole word when it had no `%`).
    pub prefix: String,
    /// Text after the conversion.
    pub suffix: String,
    /// printf flags seen (`-0+ #` subset).
    pub left: bool,
    pub zero: bool,
    pub plus: bool,
    pub space: bool,
    pub hash: bool,
    pub width: usize,
    /// `None` means no `.precision` was spelled.
    pub prec: Option<usize>,
    pub conv: ConvKind,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ConvKind {
    /// Integer conversions set `dox` in the C (value printed via floor).
    Int(IntKind),
    Real(RealKind),
    /// `%%` — a literal percent.
    Percent,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum IntKind {
    D,
    I,
    O,
    X,
    U,
    C,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RealKind {
    F,
    E,
    G,
    BigE,
    BigG,
}

/// `is dox` in the C: integer rendering uses `(long)floor(x)`.
pub fn is_dox(fmt: &Format) -> bool {
    match fmt {
        Format::Literal(_) => false,
        Format::Conv(c) => matches!(c.conv, ConvKind::Int(_)),
    }
}

/// Options after `getargs` (before the derivation switch).
#[derive(Debug, Clone)]
pub struct JotOptions {
    pub reps: i64,
    pub begin: f64,
    pub ender: f64,
    pub step: f64,
    /// Explicit `-p` precision; `None` until inferred from the operands.
    pub prec: Option<usize>,
    pub sep: String,
    pub word: String,
    pub boring: bool,
    pub chardata: bool,
    pub nofinalnl: bool,
    pub randomize: bool,
    /// Bits from HAVE_* recording which operands were spelled.
    pub have: u8,
}

impl Default for JotOptions {
    fn default() -> Self {
        JotOptions {
            reps: REPS_DEF,
            begin: BEGIN_DEF,
            ender: ENDER_DEF,
            step: STEP_DEF,
            prec: None,
            sep: "\n".to_string(),
            word: String::new(),
            boring: false,
            chardata: false,
            nofinalnl: false,
            randomize: false,
            have: 0,
        }
    }
}

pub use crate::floatfmt::parse_float_prefix;

/// `getprec`: digits after the first dot.
pub fn get_prec(num_str: &str) -> usize {
    match num_str.find('.') {
        None => 0,
        Some(dot) => num_str[dot + 1..]
            .bytes()
            .take_while(|c| c.is_ascii_digit())
            .count(),
    }
}

/// Applies a non-numeric operand the way the C does: it falls back to
/// the last character's code point (`begin = argv[i][strlen-1]`).
pub fn operand_to_number(s: &str) -> f64 {
    match parse_float_prefix(s) {
        Some((v, _)) => v,
        None => s.as_bytes()[s.len() - 1] as f64,
    }
}

/// 选项环的出口：两类由 libc `getopt` 印，一类由 `jot.c` 自己的 `errx` 印。
#[derive(Debug, PartialEq)]
pub enum JotArgError {
    /// 集外字母或缺值字母。libc 先印一行，`jot.c:151-152` 的 `default: usage()` 紧接一行。
    Getopt(crate::GetoptReport),
    /// `jot.c:161-165`：一句 `errx(EXIT_FAILURE, "Bad precision value")`，
    /// 只有这一行、不接 usage。
    BadPrecision,
}

/// 选项环的产物：已定的旗标与**按原序**交回的操作数。
pub struct JotArgs<'a> {
    pub options: JotOptions,
    pub operands: Vec<&'a str>,
}

/// `jot.c:150` 那句 `getopt(argc, argv, "b:cnp:rs:w:")` 的选项环。
///
/// optstring 里 `b`、`p`、`s`、`w` 四个字母带值，`c`、`n`、`r` 三个不带值，
/// 所以可达的出口是四类而不是一类：①集外字母、②带值字母缺值（各由 libc 先印一行，
/// C 那边再补 usage 一行）、③`-p` 的值过不了 `strtol` 判定（`errx` 单独一行、不接
/// usage）、④环正常结束。`-b` 与 `-w` 落进**同一个** `format` 槽位
/// （`jot.c:172-176` 的 `case 'b': boring = 1; /* FALLTHROUGH */ case 'w':`），
/// 所以后写的那个词覆盖先写的，而 `boring` 一旦置起就不再落下。
///
/// 环不重排：第一个非选项 token 一到，`getopt.c:74-78` 当场返回 -1 且不推进 `optind`，
/// 于是它自己就是第一个操作数（参照件现量：`jot 3 -Z` 里 `-Z` 是 begin 而不是选项）。
/// 裸 `-` 同理留在操作数里，而 `jot.c:66` 的 `is_default(s)` 把它当成「这一格用默认值」，
/// 与同族其它命令里「`-` 是标准输入」的读法无关。
pub fn parse_jot_args<'a>(args: &'a [&'a str]) -> Result<JotArgs<'a>, JotArgError> {
    let mut options = JotOptions::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        let bytes = arg.as_bytes();
        if arg == "--" {
            // `getopt.c:81-85`：`++optind` 之后才返回 -1，所以 `--` 自身不是操作数。
            break args[index + 1..].to_vec();
        }
        if bytes.len() < 2 || bytes[0] != b'-' {
            // `getopt.c:74-78`：非选项 token 一到就结束，不推进 `optind`。
            break args[index..].to_vec();
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'c' => options.chardata = true,
                b'n' => options.nofinalnl = true,
                b'r' => options.randomize = true,
                ch @ (b'b' | b'p' | b's' | b'w') => {
                    // 值是「本串去掉 `-` 与字母之后的那一段」，没有剩余就取下一个 token
                    // （`getopt.c:118-141`）；两种取法都把本串吃到尽头。
                    let value = if pos + 1 < bytes.len() {
                        &arg[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(JotArgError::Getopt(
                                    crate::GetoptReport::MissingValue(ch as char),
                                ));
                            }
                        }
                    };
                    match ch {
                        b'b' => {
                            options.boring = true;
                            options.word = value.to_string();
                        }
                        b'w' => options.word = value.to_string(),
                        b's' => options.sep = value.to_string(),
                        _ => {
                            let Some(prec) = parse_precision(value) else {
                                return Err(JotArgError::BadPrecision);
                            };
                            options.prec = Some(prec);
                        }
                    }
                    pos = bytes.len();
                }
                _ => {
                    // 报的是**这一处的第一个字节**：C 的 `optopt` 是 `char`，`%c` 打的就是
                    // 那一个字节。游标 `pos` 是字节下标，非 ASCII 字母时它会落进多字节字符
                    // 中间，所以这里按字节取而不切字符串（切串当场 panic）。
                    return Err(JotArgError::Getopt(crate::GetoptReport::UnknownOption(
                        char::from(bytes[pos]),
                    )));
                }
            }
            pos += 1;
        }
        index += 1;
    };
    Ok(JotArgs { options, operands })
}

/// C 那句 `strtol(optarg, &ep, 0)` 配上 `*ep != 0 || prec < 0` 的合并判定
/// （`jot.c:161-165`）。四个后果都由它来：
/// ①base 0 认前导空白与 `+`/`-` 号，`0x` 走十六进、前导 `0` 走八进，所以 `-p 0x2`
/// 是合法的 2（参照件现量：打出 `1.00 2.00 3.00`）；
/// ②尾巴上一个字符都不许剩，所以 `3x` 与 `09`（八进吃到 `0` 就停）都拒；
/// ③一个数字也没转出来时 `strtol` 把 `ep` 留在原起点，故只有空串算合法（值 0，
/// 参照件现量：`jot -p '' 3` 打整数形），纯空白或单独的 `-` 都拒；
/// ④值先饱和到 `long` 的边界，再像 C 那样截成 `int` 参与判定——`99999999999999999999`
/// 截成 `-1` 因而被拒，`4294967298` 截成 `2` 因而被接受。三块目标板与宿主都是 64 位
/// `long`，这个截断形状在四处一致。
#[must_use]
pub fn parse_precision(value: &str) -> Option<usize> {
    const LONG_MAX: i128 = i64::MAX as i128;
    let bytes = value.as_bytes();
    let mut pos = 0usize;
    while matches!(
        bytes.get(pos),
        Some(b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
    ) {
        pos += 1;
    }
    let mut negative = false;
    match bytes.get(pos) {
        Some(b'+') => pos += 1,
        Some(b'-') => {
            negative = true;
            pos += 1;
        }
        _ => {}
    }
    let mut radix = 10u32;
    if bytes.get(pos) == Some(&b'0') {
        if matches!(bytes.get(pos + 1), Some(b'x') | Some(b'X'))
            && bytes.get(pos + 2).is_some_and(|c| c.is_ascii_hexdigit())
        {
            radix = 16;
            pos += 2;
        } else {
            radix = 8;
        }
    }
    let mut magnitude: i128 = 0;
    let mut converted = false;
    while let Some(&byte) = bytes.get(pos) {
        let Some(digit) = (byte as char).to_digit(radix) else {
            break;
        };
        magnitude = (magnitude.saturating_mul(radix as i128) + i128::from(digit)).min(LONG_MAX);
        converted = true;
        pos += 1;
    }
    if !converted {
        // `ep` 停在原起点：只有整串本来就空时 `*ep` 才是 `\0`。
        return if value.is_empty() { Some(0) } else { None };
    }
    if pos != bytes.len() {
        return None;
    }
    let long = if negative { -magnitude } else { magnitude };
    let as_int = long as i32;
    if as_int < 0 {
        return None;
    }
    Some(as_int as usize)
}

/// Parses `reps`: strtoul semantics with a full-parse check.
pub fn parse_reps(s: &str) -> Result<i64, JotError> {
    let t = s.trim_start();
    let digits: String = t
        .bytes()
        .take_while(|c| c.is_ascii_digit())
        .map(|c| c as char)
        .collect();
    if digits.is_empty() || digits.len() != t.len() {
        return Err(JotError::BadReps(s.to_string()));
    }
    digits
        .parse::<i64>()
        .map_err(|_| JotError::BadReps(s.to_string()))
}

/// Parses `step`/seed: strtod with a full-parse check (`*ep == 0`).
pub fn parse_step(s: &str) -> Result<f64, JotError> {
    match parse_float_prefix(s) {
        Some((v, used)) if used == s.len() => Ok(v),
        _ => Err(JotError::BadStep(s.to_string())),
    }
}

/// `getformat`: pick or validate the `-w` word and report whether the
/// conversion is integer (`dox`).
pub fn get_format(options: &JotOptions) -> Result<(Format, bool), JotError> {
    if options.boring {
        return Ok((Format::Literal(options.word.clone()), false));
    }
    let word = &options.word;
    let bytes = word.as_bytes();
    // First `%` not doubled.
    let mut at: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'%' {
                i += 2;
                continue;
            }
            at = Some(i);
            break;
        }
        i += 1;
    }
    match at {
        None => {
            // Append the default conversion.
            let prec = options.prec.unwrap_or(0);
            if options.chardata || prec == 0 {
                let kind = if options.chardata { "c" } else { "ld" };
                let full = format!("{}%{}", word, kind);
                let conv = parse_conv_spec(&full, word.len())?;
                Ok((Format::Conv(conv), true))
            } else {
                let full = format!("{}%.{}f", word, prec);
                let conv = parse_conv_spec(&full, word.len())?;
                Ok((Format::Conv(conv), false))
            }
        }
        Some(pct) => {
            if pct + 1 == bytes.len() {
                // Cannot end in a single `%`: append another.
                let full = format!("{}%", word);
                let conv = parse_conv_spec(&full, pct)?;
                Ok((Format::Conv(conv), false))
            } else {
                let conv = parse_conv_spec(word, pct)?;
                let dox = matches!(conv.conv, ConvKind::Int(_));
                Ok((Format::Conv(conv), dox))
            }
        }
    }
}

/// Parses one conversion at `pct` following the C validation: allowed
/// flags `0123456789#-+. ` (no `*`), an optional `l`, then a legal
/// conversion character; anything after (outside `%%`) is an error.
fn parse_conv_spec(word: &str, pct: usize) -> Result<Conv, JotError> {
    let bytes = word.as_bytes();
    let prefix = word[..pct].to_string();
    let mut i = pct + 1;
    let mut left = false;
    let mut zero = false;
    let mut plus = false;
    let mut space = false;
    let mut hash = false;
    let mut width = 0usize;
    let mut digits = String::new();
    // Flags and width are interleaved in the C's char scan; digits
    // form the width.
    while i < bytes.len() {
        match bytes[i] {
            b'-' => left = true,
            b'0' => {
                if digits.is_empty() {
                    zero = true;
                }
                digits.push('0');
            }
            b'1'..=b'9' => digits.push(bytes[i] as char),
            b'+' => plus = true,
            b' ' => space = true,
            b'#' => hash = true,
            b'.' => break,
            b'l' => break,
            _ if bytes[i].is_ascii_alphabetic() || bytes[i] == b'%' => break,
            _ => return Err(JotError::BadFormat("unknown or invalid format")),
        }
        if bytes[i] == b'.' || bytes[i] == b'l' {
            break;
        }
        i += 1;
    }
    width = digits.parse().unwrap_or(0);
    let mut prec: Option<usize> = None;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let ds = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        prec = Some(word[ds..i].parse().unwrap_or(0));
    }
    if i < bytes.len() && bytes[i] == b'l' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(JotError::BadFormat("unknown or invalid format"));
    }
    let conv = match bytes[i] {
        b'f' => ConvKind::Real(RealKind::F),
        b'e' => ConvKind::Real(RealKind::E),
        b'g' => ConvKind::Real(RealKind::G),
        b'E' => ConvKind::Real(RealKind::BigE),
        b'G' => ConvKind::Real(RealKind::BigG),
        b'%' => ConvKind::Percent,
        b's' => {
            return Err(JotError::BadFormat(
                "cannot convert numeric data to strings",
            ));
        }
        b'd' => ConvKind::Int(IntKind::D),
        b'i' => ConvKind::Int(IntKind::I),
        b'o' => ConvKind::Int(IntKind::O),
        b'x' => ConvKind::Int(IntKind::X),
        b'u' => ConvKind::Int(IntKind::U),
        b'D' => ConvKind::Int(IntKind::D),
        b'O' => ConvKind::Int(IntKind::O),
        b'X' => ConvKind::Int(IntKind::X),
        b'U' => ConvKind::Int(IntKind::U),
        b'c' => ConvKind::Int(IntKind::C),
        _ => return Err(JotError::BadFormat("unknown or invalid format")),
    };
    // Trailing stuff (outside %%) is an error.
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'%' {
            if j + 1 < bytes.len() && bytes[j + 1] == b'%' {
                j += 2;
                continue;
            }
            return Err(JotError::BadFormat("unknown or invalid format"));
        }
        j += 1;
    }
    let suffix = word[i + 1..].replace("%%", "%");
    Ok(Conv {
        prefix,
        suffix,
        left,
        zero,
        plus,
        space,
        hash,
        width,
        prec,
        conv,
    })
}

/// The derivation switch at the end of `getargs` (jot.c:247-296).
pub fn derive(options: &mut JotOptions) -> Result<(), JotError> {
    if options.randomize {
        // Randomize takes the operands as given (step is the seed).
        return Ok(());
    }
    let have = options.have;
    if have & HAVE_ENDER != 0 && have & HAVE_STEP != 0 && have & HAVE_REPS == 0
        || have & HAVE_ENDER != 0 && have & HAVE_STEP != 0 && have & HAVE_BEGIN != 0
    {
        if options.step == 0.0 {
            options.reps = 0;
        } else {
            let reps = ((options.ender - options.begin + options.step) / options.step) as i64;
            if reps <= 0 {
                return Err(JotError::ImpossibleStepsize);
            }
            options.reps = reps;
        }
        return Ok(());
    }
    if have & HAVE_REPS != 0 && have & HAVE_ENDER != 0 && have & HAVE_BEGIN == 0 {
        if options.reps == 0 {
            return Err(JotError::MustSpecifyBegin);
        }
        options.begin = options.ender - options.reps as f64 * options.step + options.step;
        return Ok(());
    }
    if have & HAVE_REPS != 0
        && have & HAVE_BEGIN != 0
        && have & HAVE_ENDER != 0
        && have & HAVE_STEP == 0
    {
        if options.reps == 0 {
            return Err(JotError::InfiniteUnbounded);
        }
        if options.reps == 1 {
            options.step = 0.0;
        } else {
            options.step = (options.ender - options.begin) / (options.reps - 1) as f64;
        }
        return Ok(());
    }
    if have & HAVE_REPS != 0
        && have & HAVE_BEGIN != 0
        && have & HAVE_ENDER != 0
        && have & HAVE_STEP != 0
    {
        if options.step == 0.0 {
            return Ok(());
        }
        let t = ((options.ender - options.begin + options.step) / options.step) as i64;
        if t <= 0 {
            return Err(JotError::ImpossibleStepsize);
        }
        if t < options.reps {
            options.reps = t;
        }
    }
    Ok(())
}

/// Minimal linear congruential generator standing in for libc
/// `random()`; produces values in `[0, 2^31)` like the C.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Lcg(seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }
    pub fn next31(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) & 0x7fff_ffff) as u32
    }
}

/// Value stream for one jot run. `reps == 0` is the C's unbounded
/// loop; `next()` then always yields.
pub struct Jot {
    options: JotOptions,
    format: Format,
    dox: bool,
    i: i64,
    x: f64,
    rng: Option<Lcg>,
    range: f64,
    exhausted: bool,
}

impl Jot {
    /// Assembles a run: derivation + format resolution. `seed` is the
    /// `-r` seed (the `step` operand); ignored when not randomizing.
    pub fn new(mut options: JotOptions, seed: u64) -> Result<Jot, JotError> {
        let prec_inferred = match options.prec {
            None => 0,
            Some(p) => p,
        };
        let _ = prec_inferred;
        derive(&mut options)?;
        let prec = options.prec.unwrap_or(0);
        let (format, dox) = get_format(&options)?;
        let _ = prec;
        if options.randomize {
            let mut range = options.ender - options.begin;
            if range < 0.0 {
                range = -range;
                options.begin = options.ender;
            }
            let range = if dox {
                (range + 1.0) / (1u64 << 31) as f64
            } else {
                range / ((1u32 << 31) - 1) as f64
            };
            Ok(Jot {
                range,
                rng: Some(Lcg::new(seed)),
                x: options.begin,
                i: 0,
                options,
                format,
                dox,
                exhausted: false,
            })
        } else {
            // Integer display pre-adds 0.5 for sane rounding.
            let x = if dox {
                options.begin + 0.5
            } else {
                options.begin
            };
            Ok(Jot {
                range: 0.0,
                rng: None,
                x,
                i: 0,
                options,
                format,
                dox,
                exhausted: false,
            })
        }
    }

    /// How many items remain (`None` = unbounded).
    pub fn remaining(&self) -> Option<i64> {
        if self.options.reps == 0 {
            None
        } else {
            Some(self.options.reps - self.i)
        }
    }

    /// Advances and renders the next value, or `None` at the end.
    pub fn next(&mut self) -> Option<String> {
        if self.remaining() == Some(0) {
            return None;
        }
        let value = match self.rng.as_mut() {
            Some(rng) => rng.next31() as f64 * self.range + self.x,
            None => {
                let v = self.x;
                self.x += self.options.step;
                v
            }
        };
        self.i += 1;
        let item = render(&self.format, value);
        Some(item)
    }

    /// Separator state: the C prints `sep` unless this was the last
    /// of a bounded run (`notlast = reps - i`).
    pub fn separator_after(&self) -> Option<&str> {
        match self.remaining() {
            // The last item of a bounded run prints no separator
            // (putdata's `notlast = reps - i` is zero there).
            Some(0) => None,
            _ => Some(self.options.sep.as_str()),
        }
    }

    pub fn nofinalnl(&self) -> bool {
        self.options.nofinalnl
    }
}

/// `putdata`: renders one value through the format.
pub fn render(format: &Format, value: f64) -> String {
    match format {
        Format::Literal(word) => word.clone(),
        Format::Conv(c) => {
            let v = floor(value) as i64;
            match c.conv {
                ConvKind::Percent => finish(c, "%".to_string()),
                ConvKind::Int(kind) => match kind {
                    IntKind::C => {
                        // printf %c converts through unsigned char.
                        finish(c, format!("{}", v as u8 as char))
                    }
                    IntKind::O => finish_signed(c, false, format!("{:o}", v)),
                    IntKind::X => finish_signed(c, false, format!("{:x}", v)),
                    IntKind::U => finish_signed(c, false, format!("{}", v.unsigned_abs())),
                    _ => finish_signed(c, v < 0, v.unsigned_abs().to_string()),
                },
                ConvKind::Real(kind) => {
                    let prec = c.prec.unwrap_or(6);
                    let digits = match kind {
                        RealKind::F => format!("{:.*}", prec, value),
                        RealKind::E => format_e(value, prec, false),
                        RealKind::BigE => format_e(value, prec, true),
                        RealKind::G => format_g(value, prec, false),
                        RealKind::BigG => format_g(value, prec, true),
                    };
                    finish_signed(c, value < 0.0 && !digits.starts_with('-'), digits)
                }
            }
        }
    }
}

/// Applies width/zero-pad/left-align around `body`.
fn finish(c: &Conv, body: String) -> String {
    let mut out = body;
    if c.zero && !c.left && c.width > out.len() {
        // Zero padding goes after any sign.
        let pad = c.width - out.len();
        let (sign, rest) = if out.starts_with('-') || out.starts_with('+') {
            out.split_at(1)
        } else {
            ("", out.as_str())
        };
        out = format!("{}{}{}", sign, "0".repeat(pad), rest);
    } else if c.width > out.len() {
        if c.left {
            out = format!("{}{}", out, " ".repeat(c.width - out.len()));
        } else {
            out = format!("{}{}", " ".repeat(c.width - out.len()), out);
        }
    }
    format!("{}{}{}", c.prefix, out, c.suffix)
}

fn finish_signed(c: &Conv, negative: bool, digits: String) -> String {
    let sign = if negative {
        "-"
    } else if c.plus {
        "+"
    } else if c.space {
        " "
    } else {
        ""
    };
    let body = format!("{}{}", sign, digits);
    finish(c, body)
}

/// printf `%e`: `d.ddde±XX` with `prec` decimals (point omitted at
/// precision zero, exponent at least two digits).
fn format_e(value: f64, prec: usize, upper: bool) -> String {
    let neg = value < 0.0;
    let v = value.abs();
    let mut exp: i32 = decimal_exponent(v);
    let mut mant = if v == 0.0 { 0.0 } else { v / pow10(exp) };
    if mant >= 10.0 {
        exp += 1;
        mant /= 10.0;
    }
    let mut m = format!("{:.*}", prec, mant);
    if m.starts_with("10.") || m == "10" {
        // Rounding carried into the next decade.
        exp += 1;
        m = format!("{:.*}", prec, 1.0);
    }
    let (ip, fp) = match m.split_once('.') {
        Some((a, b)) => (a, b),
        None => (m.as_str(), ""),
    };
    let e = if upper { "E" } else { "e" };
    let esign = if exp < 0 { "-" } else { "+" };
    let sign = if neg { "-" } else { "" };
    if prec > 0 {
        format!("{}{}.{}{}{}{:02}", sign, ip, fp, e, esign, exp.abs())
    } else {
        format!("{}{}{}{}{:02}", sign, ip, e, esign, exp.abs())
    }
}

/// printf `%g`: `prec` significant digits, scientific when the
/// exponent is out of [-4, prec), trailing zeros stripped.
pub fn format_g(value: f64, prec: usize, upper: bool) -> String {
    let prec = if prec == 0 { 1 } else { prec };
    if value == 0.0 {
        return "0".to_string();
    }
    let neg = value < 0.0;
    let v = value.abs();
    let exp = decimal_exponent(v);
    let (body, use_exp) = if exp < -4 || exp >= prec as i32 {
        // Scientific: mantissa with prec-1 decimals.
        let mant = format!("{:.*}", prec - 1, v / pow10(exp));
        (mant, true)
    } else {
        // Fixed with prec-1-exp decimals.
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        let fixed = format!("{:.*}", decimals, v);
        (fixed, false)
    };
    let body = strip_trailing_zeros(&body);
    let sign = if neg { "-" } else { "" };
    if use_exp {
        let e = if upper { "E" } else { "e" };
        let esign = if exp < 0 { "-" } else { "+" };
        format!("{}{}{}{}{:02}", sign, body, e, esign, exp.abs())
    } else {
        format!("{}{}", sign, body)
    }
}

fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> JotOptions {
        JotOptions::default()
    }

    #[test]
    fn test_default_jot_5_counts_from_one() {
        // `jot 5` → 1 2 3 4 5 as integers (prec 0 → "%ld").
        let mut o = opts();
        o.reps = 5;
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 0).unwrap();
        let mut items = Vec::new();
        while let Some(s) = j.next() {
            items.push(s);
        }
        assert_eq!(items, vec!["1", "2", "3", "4", "5"]);
        assert_eq!(j.separator_after(), None);
    }

    #[test]
    fn test_sep_between_not_after() {
        let mut o = opts();
        o.reps = 3;
        o.sep = ",".to_string();
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 0).unwrap();
        assert_eq!(j.next(), Some("1".into()));
        assert_eq!(j.separator_after(), Some(","));
        assert_eq!(j.next(), Some("2".into()));
        assert_eq!(j.next(), Some("3".into()));
        assert_eq!(j.separator_after(), None);
    }

    #[test]
    fn test_reps_begin_ender_derives_step() {
        // `jot 4 10 20` → step = (20-10)/(4-1) = 10/3.
        let mut o = opts();
        o.reps = 4;
        o.begin = 10.0;
        o.ender = 20.0;
        o.prec = Some(2);
        o.have = HAVE_REPS | HAVE_BEGIN | HAVE_ENDER;
        let mut j = Jot::new(o, 0).unwrap();
        assert_eq!(j.next(), Some("10.00".into()));
        assert_eq!(j.next(), Some("13.33".into()));
        assert_eq!(j.next(), Some("16.67".into()));
        assert_eq!(j.next(), Some("20.00".into()));
    }

    #[test]
    fn test_reps_ender_derives_begin() {
        // `jot 4 21 5`: begin = ender - reps*step + step = 18, so the
        // run ends exactly at ender with the default step 1.
        let mut o = opts();
        o.reps = 4;
        o.ender = 21.0;
        o.have = HAVE_REPS | HAVE_ENDER;
        let mut j = Jot::new(o, 0).unwrap();
        let items = (0..4).map(|_| j.next().unwrap()).collect::<Vec<_>>();
        assert_eq!(items, vec!["18", "19", "20", "21"]);
    }

    #[test]
    fn test_ender_step_derives_reps() {
        // `jot - 0 10 5` → reps = (10-0+5)/5 = 3 → 0 5 10.
        let mut o = opts();
        o.begin = 0.0;
        o.ender = 10.0;
        o.step = 5.0;
        o.have = HAVE_ENDER | HAVE_STEP;
        let mut j = Jot::new(o, 0).unwrap();
        let items = (0..3).map(|_| j.next().unwrap()).collect::<Vec<_>>();
        assert_eq!(items, vec!["0", "5", "10"]);
        assert_eq!(j.remaining(), Some(0));
    }

    #[test]
    fn test_non_numeric_operand_uses_last_char() {
        // jot 3 a → begin = 'a' = 97 (integer mode).
        assert_eq!(operand_to_number("a"), 97.0);
        assert_eq!(operand_to_number("12x"), 12.0);
        assert_eq!(get_prec("1.250"), 3);
        assert_eq!(get_prec("42"), 0);
    }

    #[test]
    fn test_word_without_percent_appends_conversion() {
        // `-w x` + integer prec → "x%ld"; -c → "x%c".
        let mut o = opts();
        o.word = "x".to_string();
        o.prec = Some(0);
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox);
        assert_eq!(render(&f, 3.0), "x3");
        o.chardata = true;
        let (f, _) = get_format(&o).unwrap();
        assert_eq!(render(&f, 65.0), "xA");
    }

    #[test]
    fn test_boring_repeats_word_verbatim() {
        let mut o = opts();
        o.boring = true;
        o.word = "xy".to_string();
        let (f, _) = get_format(&o).unwrap();
        assert_eq!(f, Format::Literal("xy".into()));
        assert_eq!(render(&f, 1.0), "xy");
    }

    #[test]
    fn test_width_and_conversion_flags() {
        let mut o = opts();
        o.word = "%5.2f|".to_string();
        o.prec = None;
        let (f, dox) = get_format(&o).unwrap();
        assert!(!dox);
        // 不用 π 的近似当用例：`3.14159` 与 `3.1416` 都会撞上 deny 级
        // `clippy::approx_constant`——它的常量表含**舍入后的短形**（实测 1.94.0 连四位的
        // `3.1416` 也报），比的是数值而不是写法，加下划线分隔也挡不住。
        // 本测要钉的是「`%5.2f` 把值补宽到五列」，与值的来历无关：`9.5` 二进制下精确，
        // 两个小数位就是 `.50`；`7.9375`（127/16，同样精确）第三位是 `7`，必然进位成 `7.94`。
        assert_eq!(render(&f, 9.5), " 9.50|");
        assert_eq!(render(&f, 7.9375), " 7.94|");
        o.word = "%04x".to_string();
        let (f, _) = get_format(&o).unwrap();
        assert!(is_dox(&f));
        assert_eq!(render(&f, 11.0), "000b");
    }

    #[test]
    fn test_string_conversion_rejected() {
        let mut o = opts();
        o.word = "%s".to_string();
        assert_eq!(
            get_format(&o),
            Err(JotError::BadFormat(
                "cannot convert numeric data to strings"
            ))
        );
        o.word = "%d %d".to_string();
        assert!(get_format(&o).is_err());
    }

    #[test]
    fn test_random_integers_stay_in_range() {
        // -r with integer mode: values in [begin, ender] inclusive.
        let mut o = opts();
        o.randomize = true;
        o.reps = 40;
        o.begin = 1.0;
        o.ender = 6.0;
        o.prec = Some(0);
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 42).unwrap();
        let mut seen = Vec::new();
        while let Some(s) = j.next() {
            let v: i64 = s.parse().unwrap();
            assert!((1..=6).contains(&v), "value {} out of range", v);
            seen.push(v);
        }
        assert_eq!(seen.len(), 40);
    }

    #[test]
    fn test_g_format_matches_printf() {
        assert_eq!(format_g(3.2808399, 8, false), "3.2808399");
        assert_eq!(format_g(2.54, 8, false), "2.54");
        assert_eq!(format_g(0.0001, 8, false), "0.0001");
        assert_eq!(format_g(0.00001, 8, false), "1e-05");
        assert_eq!(format_g(1e24, 8, false), "1e+24");
        assert_eq!(format_g(-42.0, 8, false), "-42");
    }

    /// 集外字母与缺值都交回 libc 那两个形状，由壳去接 `usage()`。聚簇只报**第一个**坏字母。
    #[test]
    fn an_unreachable_option_shape_is_reported_by_the_libc_line() {
        assert_eq!(
            parse_jot_args(&["-Z"]).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            parse_jot_args(&["-cz"]).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('z'))
        );
        assert_eq!(
            parse_jot_args(&["-cb"]).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::MissingValue('b'))
        );
        // `--x` 报的字母是第二个连字符：optstring 里没有 `-`。
        assert_eq!(
            parse_jot_args(&["--x"]).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('-'))
        );
    }

    /// 坏值字母排在前面就先报它：`-p abc` 的 `errx` 与 `-Z` 的 libc 行各走各的出口，
    /// 而两者都在环里发生，早于任何操作数判定。
    #[test]
    fn a_rejected_precision_is_the_bad_value_sentence_not_the_libc_one() {
        assert_eq!(
            parse_jot_args(&["-p", "abc"]).err().unwrap(),
            JotArgError::BadPrecision
        );
        assert_eq!(
            parse_jot_args(&["-p", "abc", "-Z"]).err().unwrap(),
            JotArgError::BadPrecision
        );
        assert_eq!(
            parse_jot_args(&["-Z", "-p", "abc"]).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('Z'))
        );
    }

    /// 环**不重排**：第一个非选项 token 一到就结束，它自己就是第一个操作数，
    /// 后面那个 `-Z` 也留在操作数里（参照件 `jot 3 -Z` 打的是 90/91/92，把 `-Z`
    /// 当成 begin 那个字符，根本没有二次扫描）。
    #[test]
    fn the_ring_ends_at_the_first_operand_and_keeps_everything_after_it() {
        let parsed = parse_jot_args(&["3", "-Z"]).expect("the ring should end at the operand");
        assert_eq!(parsed.operands, ["3", "-Z"]);
        assert!(
            !parsed.options.chardata,
            "`-Z` 不是选项，`-c` 那一位不该被点起"
        );
    }

    /// `--` 结束环且自身不是操作数；裸 `-` 结束环却**留在**操作数里
    /// （`jot.c:66` 的 `is_default` 把它读成「这一格用默认值」）。
    #[test]
    fn the_double_dash_is_dropped_but_a_bare_dash_stays_an_operand() {
        let parsed = parse_jot_args(&["--", "-Z"]).expect("`--` 之后全是操作数");
        assert_eq!(parsed.operands, ["-Z"]);
        let parsed = parse_jot_args(&["--"]).expect("只有 `--` 也是合法的一次扫描");
        assert!(parsed.operands.is_empty());
        let parsed = parse_jot_args(&["-", "3"]).expect("裸 `-` 不该被吃掉");
        assert_eq!(parsed.operands, ["-", "3"]);
    }

    /// 带值字母取值的两种写法都把下一个 token 吃进选项，绝不当成操作数；
    /// `-b` 与 `-w` 落进同一个词槽（`jot.c:172-176` 的 FALLTHROUGH）。
    #[test]
    fn a_value_letter_consumes_the_next_token_and_b_and_w_share_one_slot() {
        let parsed = parse_jot_args(&["-p", "0"]).expect("`0` 是 `-p` 的值");
        assert_eq!(parsed.options.prec, Some(0));
        assert!(
            parsed.operands.is_empty(),
            "被 `-p` 吃掉的 token 不再是操作数"
        );
        let parsed = parse_jot_args(&["-bX", "-wY", "3"]).expect("两个词都该收到");
        assert_eq!(parsed.options.word, "Y", "后写的覆盖先写的");
        assert!(parsed.options.boring, "`-b` 点起的位不会因 `-w` 落下");
        assert_eq!(parsed.operands, ["3"]);
    }

    /// `strtol(optarg, &ep, 0)` 配 `*ep != 0 || prec < 0`：base 0 的两条前缀、
    /// 尾巴零字符、以及「一个数字都没转出来时 `ep` 留在原起点」都在这张表里。
    #[test]
    fn the_precision_follows_strtol_base_zero_with_a_full_parse_check() {
        for (word, accepted) in [
            ("2", Some(2)),
            ("0x2", Some(2)),
            ("010", Some(8)),
            ("0", Some(0)),
            ("", Some(0)),
            (" 2", Some(2)),
            ("+2", Some(2)),
            ("3x", None),
            ("09", None),
            ("-1", None),
            ("abc", None),
            (" ", None),
            ("-", None),
            ("0x", None),
        ] {
            assert_eq!(parse_precision(word), accepted, "`-p {word}` 的接受性");
        }
        // 值先饱和到 `long`，再像 C 那样截成 `int`：`LONG_MAX` 截成 `-1` 因而被拒，
        // `2^32+2` 截成 `2` 因而被接受。
        assert_eq!(parse_precision("99999999999999999999"), None);
        assert_eq!(parse_precision("4294967298"), Some(2));
    }
}
