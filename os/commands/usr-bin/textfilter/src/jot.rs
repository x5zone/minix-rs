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
use alloc::vec;
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
    /// `jot.c:215-216`：`Bad reps value:  %s`（两个空格），带的是 reps 操作数的**原字节**。
    BadReps(Vec<u8>),
    /// `jot.c:189-190`：`Bad step value:  %s`（两个空格），同样是原字节。
    BadStep(Vec<u8>),
    /// `jot.c:385`、`:396`：`unknown or invalid format \`%s'` 带的是**整个 format 词**——
    /// 包括 `:360` 那句 `strcat(format, "%")` 之后多出来的那一枚百分号，所以回显的是
    /// 校验时那份 `format`，不是用户敲进来的原始子串。
    InvalidFormat(Vec<u8>),
    /// `jot.c:376-377`：`cannot convert numeric data to strings` 是固定句，C 在这一支不带值。
    FormatWantsString,
    ImpossibleStepsize,
    MustSpecifyBegin,
    InfiniteUnbounded,
    /// `jot.c:223-225`：`Too many arguments.  What do you mean by %s?` 报的是**第五个**
    /// 操作数（`argv[4]`），原字节。
    TooManyArgs(Vec<u8>),
}

/// The parsed conversion of a `-w` word (or the synthesized default).
#[derive(Debug, PartialEq)]
pub enum Format {
    /// `-b word`: the word itself, repeated verbatim.
    Literal(Vec<u8>),
    /// One conversion with the surrounding word text.
    Conv(Conv),
}

/// A single printf conversion plus its literal context.
#[derive(Debug, PartialEq)]
pub struct Conv {
    /// Text before the conversion (the whole word when it had no `%`).
    pub prefix: Vec<u8>,
    /// Text after the conversion.
    pub suffix: Vec<u8>,
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
    /// `-s` 的值：条目之间打出去的**原字节**（`jot.c:169-170` 存 `sepstring`，
    /// `:310` 的 `fputs(sepstring, stdout)` 原样写出）。缺省是一枚换行。
    pub sep: Vec<u8>,
    /// `-b`/`-w` 共用的那一个槽（`jot.c:173-176` 的落穿）：原字节。
    pub word: Vec<u8>,
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
            sep: b"\n".to_vec(),
            word: Vec::new(),
            boring: false,
            chardata: false,
            nofinalnl: false,
            randomize: false,
            have: 0,
        }
    }
}

pub use crate::floatfmt::{parse_float_prefix, parse_float_prefix_bytes};

/// `getprec`: digits after the first dot.
pub fn get_prec(num_str: &[u8]) -> usize {
    match num_str.iter().position(|&byte| byte == b'.') {
        None => 0,
        Some(dot) => num_str[dot + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count(),
    }
}

/// Applies a non-numeric operand the way the C does: it falls back to
/// the last **byte** (`begin = argv[i][strlen(argv[i]) - 1]`, `jot.c:196`/`:204`).
/// 交回 `None` 表示那一格保持缺省——只有空操作数走这一支，理由见函数体内的注释。
pub fn operand_to_number(s: &[u8]) -> Option<f64> {
    match parse_float_prefix_bytes(s) {
        Some((v, _)) => Some(v),
        // `jot.c:196`/`:204` 取的是 `argv[i][strlen(argv[i]) - 1]`——最后一个**字节**，
        // 不是最后一个字符：参照件对 `-c 3 1 <ff>` 现量第三个值是那一枚 `ff` 本身，
        // 而 `-c 3 1 ab` 取的是 `b`。空操作数在 C 是把下标算成 `-1` 的越界读，值取决于
        // 那块内存前面是什么；本层不复制未定义行为，按宿主参照件的现量（`jot 3 '' 5`
        // 交回 `1 3 5`，即那一格保持缺省）留缺省，登记 `JOT-EMPTY-OPERAND-READS-OOB`。
        None if s.is_empty() => None,
        // 那枚字节要先经 **signed char** 落地：`jot.c:196`/`:204` 的赋值目标是 `char`，
        // 而 x86 的 `char` 带符号，所以 `\xff` 是 -1 而不是 255。三形现量：
        // `jot 3 \xff 5` → `-1 2 5`（begin 取 -1，步长 (5-(-1))/2 = 3）、
        // `jot 3 1 \xff` → `1 0 -1`、`jot -c 3 1 \xff` → 字节 `01 00 ff`；
        // 按无符号读会把 ender 算成 255，中间那格从 0 变成 128（参照件同形现量差一枚）。
        None => Some(f64::from(*s.last().expect("上面已排除空串") as i8)),
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
    pub operands: Vec<&'a [u8]>,
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
pub fn parse_jot_args<'a>(args: &'a [&'a [u8]]) -> Result<JotArgs<'a>, JotArgError> {
    let mut options = JotOptions::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        let bytes = arg;
        if arg == b"--" {
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
                        &bytes[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(JotArgError::Getopt(
                                    crate::GetoptReport::MissingValue(char::from(ch)),
                                ));
                            }
                        }
                    };
                    match ch {
                        b'b' => {
                            options.boring = true;
                            options.word = value.to_vec();
                        }
                        b'w' => options.word = value.to_vec(),
                        b's' => options.sep = value.to_vec(),
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
pub fn parse_precision(value: &[u8]) -> Option<usize> {
    const LONG_MAX: i128 = i64::MAX as i128;
    let bytes = value;
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
pub fn parse_reps(s: &[u8]) -> Result<i64, JotError> {
    // `strtoul` 跳的是 `isspace` 那六枚字节，不是 Unicode 的空白类：非 ASCII 空白
    // （`c2 a0`）在 C 里不是空白，于是「一个数字也没转出来」→ 报 Bad reps。按字节走
    // 之后这条边界是自动的，按 `str::trim_start` 走则会把 `c2 a0` 当空白跳掉而接受它。
    let mut at = 0usize;
    while matches!(s.get(at), Some(b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')) {
        at += 1;
    }
    let t = &s[at..];
    let digits: Vec<u8> = t
        .iter()
        .copied()
        .take_while(|byte| byte.is_ascii_digit())
        .collect();
    if t.is_empty() {
        // `strtoul("")` 一个数字也没转、把 `ep` 留在起点，于是 `*ep == '\0'` 成立，
        // 值是 0 而**不报错**（与 `parse_precision` 那条同一个道理）。参照件现量
        // `jot '' 1 2` 报的是 `Infinite sequences cannot be bounded`（reps 为 0 就是无界，
        // `jot.c:272-275`），而不是那句 Bad reps。
        return Ok(0);
    }
    if digits.is_empty() || digits.len() != t.len() {
        return Err(JotError::BadReps(s.to_vec()));
    }
    let text = core::str::from_utf8(&digits).map_err(|_| JotError::BadReps(s.to_vec()))?;
    text.parse::<i64>()
        .map_err(|_| JotError::BadReps(s.to_vec()))
}

/// Parses `step`/seed: strtod with a full-parse check (`*ep == 0`).
pub fn parse_step(s: &[u8]) -> Result<f64, JotError> {
    match parse_float_prefix_bytes(s) {
        Some((v, used)) if used == s.len() => Ok(v),
        _ => Err(JotError::BadStep(s.to_vec())),
    }
}

/// `jot.c:118` 的 `srandom((unsigned long) step)`：先按 C 的规则把 `double` **向零截断**，
/// 再在负数变无符号那一步**回绕**。所以 `-0.5` 与 `0` 是同一个种子（截成 0），
/// `-5.9` 与 `-5` 也是同一个（先截成 -5 再回绕）。
///
/// C 的 `unsigned long` 在 Minix 的 ILP32 上只有 32 位，本层的随机流由 `Lcg` 承载、
/// 拿 64 位种子；两边回绕的**位宽**不同，但流的数值本就不是 libc `random()`（登记在
/// 07 篇 §4.4），可观察的是截断方向与「负数合法」这两点，函数按这两点定形。
#[must_use]
pub fn seed_from_step(step: f64) -> u64 {
    (step as i64) as u64
}

/// `getformat`: pick or validate the `-w` word and report whether the
/// conversion is integer (`dox`).
pub fn get_format(options: &JotOptions) -> Result<(Format, bool), JotError> {
    if options.boring {
        return Ok((Format::Literal(options.word.clone()), false));
    }
    let word = &options.word;
    let bytes = word.as_slice();
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
            // C 在这里用的是**未经钳位**的 prec：`jot.c:229-230` 的
            // `if (prec == -1) prec = 0;` 排在 `getformat()` **之后**，所以拼说明符那一步
            // 它还是 -1。壳侧的 `None` 就是那个 -1，而 `Some(0)` 是另一回事——
            // 后者对应 C 的 `prec == 0`，走整数支（`%ld`，`(long)floor(x)`，`jot.c:306`）。
            //
            // -1 那一支 C 拼出 `%.-1f`（`jot.c:356` 的 `"%%.%df"`）。这个说明符在 Minix 上
            // 不是坏的：libc 是 NetBSD 血脉，`minix3/lib/libc/stdio/vsnprintf_ss.c:213-226`
            // 的 `case '.'` 在 `.` 后遇到非数字时把 `n` 留在 0，于是 `prec = 0`，再 `goto reswitch`
            // 把剩下的 `-` 当左调旗标、`1` 当字段宽、`f` 当转换字符。等价于 `%-1.0f`——
            // **四舍五入**，不是取整。宿主 glibc 对同一串打回字面量 `%.0-1f`，那是参照件
            // 所链 libc 的行为，不是 Minix 的真值，所以这一形不能拿宿主参照件定判据。
            if options.chardata || options.prec == Some(0) {
                let kind: &[u8] = if options.chardata { b"c" } else { b"ld" };
                let mut full = word.clone();
                full.push(b'%');
                full.extend_from_slice(kind);
                let conv = parse_conv_spec(&full, word.len())?;
                Ok((Format::Conv(conv), true))
            } else {
                let mut full = word.clone();
                match options.prec {
                    Some(p) => full.extend_from_slice(format!("%.{}f", p).as_bytes()),
                    None => full.extend_from_slice(b"%-1.0f"),
                };
                let conv = parse_conv_spec(&full, word.len())?;
                Ok((Format::Conv(conv), false))
            }
        }
        Some(pct) => {
            if pct + 1 == bytes.len() {
                // Cannot end in a single `%`: append another.
                let mut full = word.clone();
                full.push(b'%');
                let conv = parse_conv_spec(&full, pct)?;
                Ok((Format::Conv(conv), false))
            } else {
                let conv = parse_conv_spec(bytes, pct)?;
                let dox = matches!(conv.conv, ConvKind::Int(_));
                Ok((Format::Conv(conv), dox))
            }
        }
    }
}

/// Parses one conversion at `pct` following the C validation: allowed
/// flags `0123456789#-+. ` (no `*`), an optional `l`, then a legal
/// conversion character; anything after (outside `%%`) is an error.
fn parse_conv_spec(word: &[u8], pct: usize) -> Result<Conv, JotError> {
    let bytes = word;
    // 前缀段同样要展开 `%%`：C 把**整个** `format` 交给 `printf`（`jot.c:306`、`:308`），
    // 展开由 libc 完成，不区分转换的哪一侧。参照件现量 `-p 2 -w '%%x%d' 3 1 5` 打
    // `%x1`（一枚百分号），不展开的那一版打的是 `%x` 之前多出来的两枚。
    let prefix = unfold_double_percent(&word[..pct]);
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
            _ => return Err(JotError::InvalidFormat(word.to_vec())),
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
        prec = Some(byte_digits_to_usize(&word[ds..i]));
    }
    if i < bytes.len() && bytes[i] == b'l' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(JotError::InvalidFormat(word.to_vec()));
    }
    let conv = match bytes[i] {
        b'f' => ConvKind::Real(RealKind::F),
        b'e' => ConvKind::Real(RealKind::E),
        b'g' => ConvKind::Real(RealKind::G),
        b'E' => ConvKind::Real(RealKind::BigE),
        b'G' => ConvKind::Real(RealKind::BigG),
        b'%' => ConvKind::Percent,
        b's' => {
            // `jot.c:376-377` 那句是固定句，C 在这一支**不带值**。
            return Err(JotError::FormatWantsString);
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
        _ => return Err(JotError::InvalidFormat(word.to_vec())),
    };
    // Trailing stuff (outside %%) is an error.
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'%' {
            if j + 1 < bytes.len() && bytes[j + 1] == b'%' {
                j += 2;
                continue;
            }
            return Err(JotError::InvalidFormat(word.to_vec()));
        }
        j += 1;
    }
    let suffix = unfold_double_percent(&word[i + 1..]);
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

/// 一串十进制字节转成 `usize`（`-w` 说明符里 `.` 之后那一段）。转不出就算 0，与 C 的
/// `atoi` 式读法同形：libc 从 `.` 之后开始吃数字，遇非数字即止。
#[must_use]
fn byte_digits_to_usize(bytes: &[u8]) -> usize {
    core::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0)
}

/// `printf` 在**整条格式串**上把 `%%` 展开成一枚 `%`（`jot.c:306`、`:308` 把整个 `format`
/// 交给它，`:391-397` 的第二遍扫描只做校验不做展开）。本层把转换前后的字面段分开存着，
/// 所以两段都要自己做这次展开——只展开后缀会留下「前缀打两枚、参照件打一枚」的半截语义。
#[must_use]
fn unfold_double_percent(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] == b'%' && bytes.get(at + 1) == Some(&b'%') {
            out.push(b'%');
            at += 2;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    out
}

/// The derivation switch at the end of `getargs`（`jot.c:247-296`）。
///
/// C 的 `switch (have)` 用的是**精确值**而不是「含某几位」：`case ENDER | STEP`
/// （`:249`）只在 `have` 恰好等于这两位时命中，四个操作数都给齐时命中的是
/// `case REPS | BEGIN | ENDER | STEP`（`:281`）。按子集判会把四旗标形误派到 `:249-259`
/// 那一支，而两支队尾相反：那一支在 `step == 0.0` 时写的是 `reps = 0`（**无界**，
/// 打印循环是 `for (i = 1; i <= reps || reps == 0; i++)`，`jot.c:129`），
/// `:281-284` 写的是 `break`（**reps 保持给定的 3**）。于是 `jot 3 1 100 0`
/// 从三行变成永不终止，`jot 3 1 100 5` 从三行变成二十行。
pub fn derive(options: &mut JotOptions) -> Result<(), JotError> {
    if options.randomize {
        // Randomize takes the operands as given (step is the seed).
        return Ok(());
    }
    let have = options.have;
    let r = HAVE_REPS;
    let b = HAVE_BEGIN;
    let e = HAVE_ENDER;
    let s = HAVE_STEP;
    // `jot.c:249-259`：只给了 ender 与 step（可带 begin），reps 要算出来
    if have == e | s || have == e | s | b {
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
    // `jot.c:261-268`：给了 reps 与 ender（可带 step），begin 要算出来
    if have == r | e || have == r | e | s {
        if options.reps == 0 {
            return Err(JotError::MustSpecifyBegin);
        }
        options.begin = options.ender - options.reps as f64 * options.step + options.step;
        return Ok(());
    }
    // `jot.c:270-279`：给了 reps、begin、ender，step 要算出来
    if have == r | b | e {
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
    // `jot.c:281-291`：四个都给，取「给定的」与「算出的」较小的那个 reps；
    // step 为零时 **break**——reps 保持给定值，既不无界也不报错
    if have == r | b | e | s {
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
        return Ok(());
    }
    // `jot.c:293-295` 的 `default`：什么也算不出来，全部按现值走
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
    pub fn next(&mut self) -> Option<Vec<u8>> {
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
    pub fn separator_after(&self) -> Option<&[u8]> {
        match self.remaining() {
            // The last item of a bounded run prints no separator
            // (putdata's `notlast = reps - i` is zero there).
            Some(0) => None,
            _ => Some(self.options.sep.as_slice()),
        }
    }

    pub fn nofinalnl(&self) -> bool {
        self.options.nofinalnl
    }
}

/// `putdata`: renders one value through the format.
pub fn render(format: &Format, value: f64) -> Vec<u8> {
    match format {
        Format::Literal(word) => word.clone(),
        Format::Conv(c) => {
            let v = floor(value) as i64;
            match c.conv {
                ConvKind::Percent => finish(c, b"%".to_vec()),
                ConvKind::Int(kind) => match kind {
                    IntKind::C => {
                        // `printf("%c", …)` 经 `putchar` 交回**一枚字节**：参照件对
                        // `-p 0 -w '%c' 1 200 1` 现量 `c8`，而把 `v as u8 as char` 再经
                        // UTF-8 编码会打成 `c3 88` 两枚——那一支的 `char` 通道在字节形下退出。
                        finish(c, vec![v as u8])
                    }
                    IntKind::O => finish_signed(c, false, format!("{:o}", v).into_bytes()),
                    IntKind::X => finish_signed(c, false, format!("{:x}", v).into_bytes()),
                    IntKind::U => {
                        finish_signed(c, false, format!("{}", v.unsigned_abs()).into_bytes())
                    }
                    _ => finish_signed(c, v < 0, v.unsigned_abs().to_string().into_bytes()),
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
                    finish_signed(
                        c,
                        value < 0.0 && !digits.starts_with('-'),
                        digits.into_bytes(),
                    )
                }
            }
        }
    }
}

/// Applies width/zero-pad/left-align around `body`.
fn finish(c: &Conv, body: Vec<u8>) -> Vec<u8> {
    let mut out = body;
    if c.zero && !c.left && c.width > out.len() {
        // Zero padding goes after any sign.
        let pad = c.width - out.len();
        let signed = out.first().is_some_and(|&b| b == b'-' || b == b'+');
        let mut padded = Vec::with_capacity(out.len() + pad);
        if signed {
            padded.push(out[0]);
            padded.extend(core::iter::repeat_n(b'0', pad));
            padded.extend_from_slice(&out[1..]);
        } else {
            padded.extend(core::iter::repeat_n(b'0', pad));
            padded.extend_from_slice(&out);
        }
        out = padded;
    } else if c.width > out.len() {
        let pad = c.width - out.len();
        let mut spaced = Vec::with_capacity(out.len() + pad);
        if c.left {
            spaced.extend_from_slice(&out);
            spaced.extend(core::iter::repeat_n(b' ', pad));
        } else {
            spaced.extend(core::iter::repeat_n(b' ', pad));
            spaced.extend_from_slice(&out);
        }
        out = spaced;
    }
    // 字段宽只作用于转换出来的那一段（`jot.c:306`、`:308` 就是 `printf(format, x)`，
    // 百分号前后的字面段由格式串自己打出去），所以补白夹在前后段之间而不是算进宽度。
    let mut full = Vec::with_capacity(c.prefix.len() + out.len() + c.suffix.len());
    full.extend_from_slice(&c.prefix);
    full.extend_from_slice(&out);
    full.extend_from_slice(&c.suffix);
    full
}

fn finish_signed(c: &Conv, negative: bool, digits: Vec<u8>) -> Vec<u8> {
    let sign = if negative {
        "-"
    } else if c.plus {
        "+"
    } else if c.space {
        " "
    } else {
        ""
    };
    let mut body = Vec::with_capacity(sign.len() + digits.len());
    body.extend_from_slice(sign.as_bytes());
    body.extend_from_slice(&digits);
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

    /// 夹具助手：把文本形的 argv 词表交回字节形。测试里的字面量全是 ASCII，两种形状等价，
    /// 所以这层只是把 `parse_jot_args` 的入参类型写出来，本身不承载判据。交回
    /// `Vec<&'static [u8]>`（而不是造临时量）是必需的：`JotArgs` 借的是那些字节，临时 `Vec`
    /// 撑不过一条语句，结果要留着断言的形必须先 `let w_x = argv(&[…])` 命名。
    fn argv(words: &'static [&'static str]) -> Vec<&'static [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    /// 渲染半交回的每一项都是字节；长度不齐的数组字面量在 `vec![b"1", b"18"]` 里会混型，
    /// 所以「整串条目」的期望走这个助手。
    fn words(items: &[&str]) -> Vec<Vec<u8>> {
        items.iter().map(|word| word.as_bytes().to_vec()).collect()
    }

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
        assert_eq!(items, words(&["1", "2", "3", "4", "5"]));
        assert_eq!(j.separator_after(), None);
    }

    #[test]
    fn test_sep_between_not_after() {
        let mut o = opts();
        o.reps = 3;
        o.sep = b",".to_vec();
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 0).unwrap();
        assert_eq!(j.next(), Some(b"1".to_vec()));
        assert_eq!(j.separator_after(), Some(b",".as_slice()));
        assert_eq!(j.next(), Some(b"2".to_vec()));
        assert_eq!(j.next(), Some(b"3".to_vec()));
        assert_eq!(j.separator_after(), None);
    }

    #[test]
    fn test_seed_cast_truncates_then_wraps() {
        // 截断是**向零**取整：`-0.5` 与 `0` 同种子（参照件现量两串数逐字节相同，见
        // `tests/jot_face.rs` 的 `a_negative_or_fractional_seed_is_accepted_like_c`）
        assert_eq!(seed_from_step(0.0), 0);
        assert_eq!(seed_from_step(-0.5), 0);
        assert_eq!(seed_from_step(0.9), 0);
        assert_eq!(seed_from_step(-0.9), 0);
        // 负数先截成整数再回绕：`-5` 与 `-5.9` 同种子
        assert_eq!(seed_from_step(-5.0), u64::MAX - 4);
        assert_eq!(seed_from_step(-5.9), seed_from_step(-5.0));
        // 正数不受回绕影响
        assert_eq!(seed_from_step(5.0), 5);
        assert_eq!(seed_from_step(2147483648.0), 2147483648);
        // 这条就是批次三百二十五那枚漏网负控要钉的位置：整幅偏移现在会被测到
        assert_ne!(seed_from_step(-5.0).wrapping_add(1), seed_from_step(-5.0));
    }

    #[test]
    fn test_begin_ender_step_derives_reps() {
        // `jot - 1 100 5`：reps 没给而 begin 给了，命中的是 C 的第三个标签
        // `case ENDER | STEP | BEGIN`（`jot.c:250`）——它与 `case ENDER | STEP` 共用同一段体，
        // 标签少写一个就整支落进 `default`，reps 停在默认值而不是算出的 20
        let mut o = opts();
        o.begin = 1.0;
        o.ender = 100.0;
        o.step = 5.0;
        o.have = HAVE_BEGIN | HAVE_ENDER | HAVE_STEP;
        let j = Jot::new(o, 0).unwrap();
        assert_eq!(j.remaining(), Some(20));
    }

    #[test]
    fn test_all_four_operands_with_zero_step_keeps_reps() {
        // `jot 3 1 100 0`：C 的 `case REPS | BEGIN | ENDER | STEP`（`jot.c:281-284`）在
        // step 为零时是 **break**，reps 保持给定值；把 reps 归零的是另一支
        // `case ENDER | STEP`（`:251-253`）。switch 的 case 标签是精确值，少一位就不是同一支
        let mut o = opts();
        o.reps = 3;
        o.begin = 1.0;
        o.ender = 100.0;
        o.step = 0.0;
        o.have = HAVE_REPS | HAVE_BEGIN | HAVE_ENDER | HAVE_STEP;
        let mut j = Jot::new(o, 0).unwrap();
        let items = (0..3).map(|_| j.next().unwrap()).collect::<Vec<_>>();
        assert_eq!(items, words(&["1", "1", "1"]));
        assert_eq!(j.remaining(), Some(0));
    }

    #[test]
    fn test_derived_reps_branch_still_goes_unbounded() {
        // 反方向也要钉住：`jot - - 100 0` 走 `case ENDER | STEP`，那里 `reps = 0` 就是**无界**
        // （打印循环是 `for (i = 1; i <= reps || reps == 0; i++)`，`jot.c:129`）
        let mut o = opts();
        o.ender = 100.0;
        o.step = 0.0;
        o.have = HAVE_ENDER | HAVE_STEP;
        let j = Jot::new(o, 0).unwrap();
        assert_eq!(j.remaining(), None);
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
        assert_eq!(j.next(), Some(b"10.00".to_vec()));
        assert_eq!(j.next(), Some(b"13.33".to_vec()));
        assert_eq!(j.next(), Some(b"16.67".to_vec()));
        assert_eq!(j.next(), Some(b"20.00".to_vec()));
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
        assert_eq!(items, words(&["18", "19", "20", "21"]));
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
        assert_eq!(items, words(&["0", "5", "10"]));
        assert_eq!(j.remaining(), Some(0));
    }

    #[test]
    fn test_non_numeric_operand_uses_last_char() {
        // jot 3 a → begin = 'a' = 97 (integer mode).
        assert_eq!(operand_to_number(b"a"), Some(97.0));
        assert_eq!(operand_to_number(b"12x"), Some(12.0));
        assert_eq!(get_prec(b"1.250"), 3);
        assert_eq!(get_prec(b"42"), 0);
    }

    #[test]
    fn test_word_without_percent_appends_conversion() {
        // `-w x` + integer prec → "x%ld"; -c → "x%c".
        let mut o = opts();
        o.word = b"x".to_vec();
        o.prec = Some(0);
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox);
        assert_eq!(render(&f, 3.0), b"x3");
        o.chardata = true;
        let (f, _) = get_format(&o).unwrap();
        assert_eq!(render(&f, 65.0), b"xA");
    }

    /// `prec` 从未被写过（C 的 `-1`，`jot.c:74`）与显式写成 `0`（`-p 0` 或推得 0）
    /// 是两件事：前者进 C 的 `%%.%df` 支拼出 `%.-1f`，Minix 的 libc 把它解析成
    /// 精度 0 的浮点（`minix3/lib/libc/stdio/vsnprintf_ss.c:213-226`：`.` 后非数字 ⇒
    /// `prec = 0`，随后 `-` 是左调旗标、`1` 是宽度、`f` 是转换），**四舍五入**；
    /// 后者进整数支，`(long)floor(x)`（`jot.c:306`、`dox`），**向下取整**。
    /// 两支撑得开的值不同，这一枚钉的就是那条分界。
    #[test]
    fn an_unwritten_precision_rounds_while_an_explicit_zero_floors() {
        let mut o = opts();
        // 未写精度：走浮点支，dox 为假。
        o.prec = None;
        let (f, dox) = get_format(&o).unwrap();
        assert!(!dox, "未写精度不进整数支");
        // 推导出来的说明符要把 Minix 解析 `%.-1f` 得到的三个要素都带上：左调旗标、
        // 字段宽 1、精度 0。少任何一个都是把 C 的真值少抄一半。
        match &f {
            Format::Conv(c) => {
                assert!(c.left, "`.` 后那个 `-` 在 Minix 的旗标循环里是 LADJUST");
                assert_eq!(c.width, 1, "剩下的 `1` 是字段宽");
                assert_eq!(c.prec, Some(0), "`.` 后非数字 ⇒ 精度 0");
                assert!(matches!(c.conv, ConvKind::Real(RealKind::F)), "结尾是 `f`");
            }
            Format::Literal(_) => panic!("未写精度该构造转换，而不是字面"),
        }
        assert_eq!(render(&f, 0.7), b"1", "0.7 该四舍五入成 1，不是取整成 0");
        assert_eq!(render(&f, 1.5), b"2", "半数上取");
        assert_eq!(render(&f, 2.5), b"2", "凑偶：与 NetBSD dtoa 同规则");
        assert_eq!(render(&f, 3.0), b"3", "整值两支撑得一样");
        // `-c` 的字符支优先于精度这一族：C 的条件是 `chardata || prec == 0`，
        // 所以 `chardata` 为真时**不论 prec 写没写**都走 `%c`。
        o.chardata = true;
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox, "`-c` 在精度未写时仍走整数支");
        assert_eq!(render(&f, 65.0), b"A");
        o.chardata = false;

        // 显式 0：走整数支，floor。
        o.prec = Some(0);
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox, "显式 0 走 %ld 支");
        assert_eq!(render(&f, 0.7), b"0", "floor 与上面的 round 在此分家");
        assert_eq!(render(&f, 1.5), b"1");
    }

    #[test]
    fn test_boring_repeats_word_verbatim() {
        let mut o = opts();
        o.boring = true;
        o.word = b"xy".to_vec();
        let (f, _) = get_format(&o).unwrap();
        assert_eq!(f, Format::Literal(b"xy".to_vec()));
        assert_eq!(render(&f, 1.0), b"xy");
    }

    #[test]
    fn test_width_and_conversion_flags() {
        let mut o = opts();
        o.word = b"%5.2f|".to_vec();
        o.prec = None;
        let (f, dox) = get_format(&o).unwrap();
        assert!(!dox);
        // 不用 π 的近似当用例：`3.14159` 与 `3.1416` 都会撞上 deny 级
        // `clippy::approx_constant`——它的常量表含**舍入后的短形**（实测 1.94.0 连四位的
        // `3.1416` 也报），比的是数值而不是写法，加下划线分隔也挡不住。
        // 本测要钉的是「`%5.2f` 把值补宽到五列」，与值的来历无关：`9.5` 二进制下精确，
        // 两个小数位就是 `.50`；`7.9375`（127/16，同样精确）第三位是 `7`，必然进位成 `7.94`。
        assert_eq!(render(&f, 9.5), b" 9.50|");
        assert_eq!(render(&f, 7.9375), b" 7.94|");
        o.word = b"%04x".to_vec();
        let (f, _) = get_format(&o).unwrap();
        assert!(is_dox(&f));
        assert_eq!(render(&f, 11.0), b"000b");
    }

    #[test]
    fn test_string_conversion_rejected() {
        let mut o = opts();
        o.word = b"%s".to_vec();
        assert_eq!(get_format(&o), Err(JotError::FormatWantsString));
        o.word = b"%d %d".to_vec();
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
            let v: i64 = String::from_utf8(s).unwrap().parse().unwrap();
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
            parse_jot_args(&argv(&["-Z"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            parse_jot_args(&argv(&["-cz"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('z'))
        );
        assert_eq!(
            parse_jot_args(&argv(&["-cb"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::MissingValue('b'))
        );
        // `--x` 报的字母是第二个连字符：optstring 里没有 `-`。
        assert_eq!(
            parse_jot_args(&argv(&["--x"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('-'))
        );
    }

    /// 坏值字母排在前面就先报它：`-p abc` 的 `errx` 与 `-Z` 的 libc 行各走各的出口，
    /// 而两者都在环里发生，早于任何操作数判定。
    #[test]
    fn a_rejected_precision_is_the_bad_value_sentence_not_the_libc_one() {
        assert_eq!(
            parse_jot_args(&argv(&["-p", "abc"])).err().unwrap(),
            JotArgError::BadPrecision
        );
        assert_eq!(
            parse_jot_args(&argv(&["-p", "abc", "-Z"])).err().unwrap(),
            JotArgError::BadPrecision
        );
        assert_eq!(
            parse_jot_args(&argv(&["-Z", "-p", "abc"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('Z'))
        );
    }

    /// 环**不重排**：第一个非选项 token 一到就结束，它自己就是第一个操作数，
    /// 后面那个 `-Z` 也留在操作数里（参照件 `jot 3 -Z` 打的是 90/91/92，把 `-Z`
    /// 当成 begin 那个字符，根本没有二次扫描）。
    #[test]
    fn the_ring_ends_at_the_first_operand_and_keeps_everything_after_it() {
        let w_ring = argv(&["3", "-Z"]);
        let parsed = parse_jot_args(&w_ring).expect("the ring should end at the operand");
        assert_eq!(parsed.operands, argv(&["3", "-Z"]));
        assert!(
            !parsed.options.chardata,
            "`-Z` 不是选项，`-c` 那一位不该被点起"
        );
    }

    /// `--` 结束环且自身不是操作数；裸 `-` 结束环却**留在**操作数里
    /// （`jot.c:66` 的 `is_default` 把它读成「这一格用默认值」）。
    #[test]
    fn the_double_dash_is_dropped_but_a_bare_dash_stays_an_operand() {
        let w_dash = argv(&["--", "-Z"]);
        let parsed = parse_jot_args(&w_dash).expect("`--` 之后全是操作数");
        assert_eq!(parsed.operands, argv(&["-Z"]));
        let w_only = argv(&["--"]);
        let parsed = parse_jot_args(&w_only).expect("只有 `--` 也是合法的一次扫描");
        assert!(parsed.operands.is_empty());
        let w_bare = argv(&["-", "3"]);
        let parsed = parse_jot_args(&w_bare).expect("裸 `-` 不该被吃掉");
        assert_eq!(parsed.operands, argv(&["-", "3"]));
    }

    /// 带值字母取值的两种写法都把下一个 token 吃进选项，绝不当成操作数；
    /// `-b` 与 `-w` 落进同一个词槽（`jot.c:173-176` 的 FALLTHROUGH）。
    #[test]
    fn a_value_letter_consumes_the_next_token_and_b_and_w_share_one_slot() {
        let w_prec = argv(&["-p", "0"]);
        let parsed = parse_jot_args(&w_prec).expect("`0` 是 `-p` 的值");
        assert_eq!(parsed.options.prec, Some(0));
        assert!(
            parsed.operands.is_empty(),
            "被 `-p` 吃掉的 token 不再是操作数"
        );
        let w_two = argv(&["-bX", "-wY", "3"]);
        let parsed = parse_jot_args(&w_two).expect("两个词都该收到");
        assert_eq!(parsed.options.word, b"Y", "后写的覆盖先写的");
        assert!(parsed.options.boring, "`-b` 点起的位不会因 `-w` 落下");
        assert_eq!(parsed.operands, argv(&["3"]));
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
            assert_eq!(
                parse_precision(word.as_bytes()),
                accepted,
                "`-p {word}` 的接受性"
            );
        }
        // 值先饱和到 `long`，再像 C 那样截成 `int`：`LONG_MAX` 截成 `-1` 因而被拒，
        // `2^32+2` 截成 `2` 因而被接受。
        assert_eq!(parse_precision(b"99999999999999999999"), None);
        assert_eq!(parse_precision(b"4294967298"), Some(2));
    }

    /// `jot` 的四支带值字母里，三支的值**直接是输出内容**、一支是数字：`-b`/`-w` 的词被
    /// `putdata`（`jot.c:300-311`）原样打出去，`-s` 的串由 `:310` 的 `fputs(sepstring, stdout)`
    /// 打在条目之间，而 `-w` 的词还要过 `getformat`（`:333-398`）那道按字节的校验。所以枚
    /// 裸 `ff` 在这里同时决定三件事：打得出来的字节、切分说明符能不能通过校验、以及校验失败
    /// 那句回显长几个字节（`jot.c:385`、`:396` 的 `unknown or invalid format \`%s\'` 带的就是
    /// 整个 `format`）。操作数那两条腿也在这枚门里：`jot.c:196`/`:204` 取的是最后一个**字节**
    /// （`argv[i][strlen(argv[i]) - 1]`），而 `strtoul` 对空串是「一个数字也没转、`ep` 停在起点、
    /// `*ep == '\0'`」，所以空 reps 是合法的 0（= 无界）而不是坏值。
    #[test]
    fn the_word_the_separator_and_the_operand_bytes_all_reach_the_output() {
        // ①`-b` 的词：`boring` 那一路把整个词当字面段打出（`jot.c:304` 的 `printf("%s", format)`）。
        let raw: &[&[u8]] = &[b"-b\xffwd", b"2"];
        let parsed = parse_jot_args(raw).expect("`-b` 收整串当词");
        assert_eq!(
            parsed.options.word,
            vec![0xff, b'w', b'd'],
            "词里那枚裸字节必须原样到库"
        );
        assert!(parsed.options.boring);
        let (format, dox) = get_format(&parsed.options).unwrap();
        assert_eq!(format, Format::Literal(vec![0xff, b'w', b'd']));
        assert!(!dox, "字面段不产生转换，`dox` 那一位置起不来");
        assert_eq!(
            render(&format, 7.0),
            vec![0xff, b'w', b'd'],
            "打出去的必须是那三枚字节"
        );
        // ②`-s` 的串：条目之间的分隔符是原字节（参照件对 `-s<ff>x` 现量条目间就是 `ff 78`）。
        let sep: &[&[u8]] = &[b"-s\xffx", b"2"];
        let parsed = parse_jot_args(sep).expect("`-s` 的值是一串字节");
        assert_eq!(parsed.options.sep, vec![0xff, b'x']);
        // ③`-w` 的词：百分号之前的字面段按字节存着，转换之后的补白与前后段互不干扰。
        let mut o = opts();
        o.word = vec![b'a', 0xff, b'%', b'd'];
        let (format, dox) = get_format(&o).unwrap();
        assert!(dox, "`%d` 是整数支（`jot.c:382` 的 `dox = 1`）");
        let Format::Conv(conv) = &format else {
            panic!("带 `%d` 的词该构造转换，而不是字面段");
        };
        assert_eq!(
            conv.prefix,
            vec![b'a', 0xff],
            "百分号前那一段就是 argv 的原字节"
        );
        assert_eq!(render(&format, 3.0), vec![b'a', 0xff, b'3']);
        // ④`%c` 交回**一枚字节**：C 走 `putchar`，所以 200 就是 `c8`；那一条腿若经 `char`
        //    通体会编成 `c3 88` 两枚（参照件对 `-p 0 -w '%c' 1 200 1` 现量 `c8`）。
        let mut o = opts();
        o.word = b"%c".to_vec();
        o.prec = Some(0);
        let (format, _) = get_format(&o).unwrap();
        assert_eq!(render(&format, 200.0), vec![0xc8]);
        // ⑤校验失败那句带的是**整个 format 词**（含裸字节），不是空壳句子。
        let mut o = opts();
        o.word = vec![b'%', 0xff];
        assert_eq!(
            get_format(&o).err(),
            Some(JotError::InvalidFormat(vec![b'%', 0xff])),
            "`unknown or invalid format `%s'` 的值位是原字节"
        );
        // ⑥操作数取最后一个字节：`é`（`c3 a9`）在 C 就是取到 `a9` = 169。
        assert_eq!(operand_to_number(b"a"), Some(97.0));
        assert_eq!(
            operand_to_number(b"\xc3\xa9"),
            Some(-87.0),
            "取到的是最后一字节 `a9`，而 `char` 带符号"
        );
        // ⑦空操作数：C 是 `argv[i][-1]` 的越界读，本层不复制未定义行为，交回 `None`
        //    让调用方留那一格缺省（宿主参照件现量 `jot 3 '' 5` → `1 3 5`）。
        assert_eq!(
            operand_to_number(b""),
            None,
            "空操作数不该 panic，也不该编出一个值"
        );
        // ⑧空 reps 在 C 是合法的 0（`strtoul` 把 `ep` 留在起点），于是无界而不是坏值。
        assert_eq!(parse_reps(b""), Ok(0));
        assert_eq!(
            parse_reps(b"x").err(),
            Some(JotError::BadReps(b"x".to_vec()))
        );
        // ⑨`isspace` 那六枚字节之外都不算空白：非 ASCII 空白（`c2 a0`）在 C 里不是空白，
        //    所以一个数字也没转出来，落 Bad reps——按字符走 `str::trim_start` 会把它跳掉而
        //    意外接受，这一形钉的是「按字节」而不是「按字符」。
        assert_eq!(
            parse_reps(&[0xc2, 0xa0, b'3'])
                .err()
                .map(|e| matches!(e, JotError::BadReps(_))),
            Some(true),
            "`c2 a0` 不是 C 的 isspace，不该被跳过"
        );
        // ⑪前缀段的 `%%` 也一样要展开：C 把**整个** format 交给 `printf`（`jot.c:306`、
        //    `:308`），展开由 libc 做，不分转换的哪一侧。参照件对 `-p 2 -w '%%x%d' 3 1 5`
        //    现量 `%x1`——只展开后缀的那一版在这里会多打一枚百分号。
        let mut o = opts();
        o.word = b"%%x%d".to_vec();
        o.prec = Some(2);
        let (format, _) = get_format(&o).unwrap();
        assert_eq!(
            render(&format, 1.0),
            b"%x1".to_vec(),
            "前缀里两枚百分号该展开成一枚"
        );
        // ⑩`-p` 之外的三支带值字母缺值时，缺的那枚字母也是字节（`getopt.c:132-136` 的 `%c`）。
        assert_eq!(
            parse_jot_args(&argv(&["-s"])).err(),
            Some(JotArgError::Getopt(crate::GetoptReport::MissingValue('s'))),
        );
    }
}
