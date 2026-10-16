//! Number sequence generation for `seq`.
//!
//! Ground truth: `minix3/usr.bin/seq/seq.c` (NetBSD): `seq [-w] [-s
//! string] [first [incr]] last`, defaults first 1 and increment 1, the
//! loop runs while the value has not passed `last`, `-s` changes the
//! separator (default newline), `-w` equalises all renderings to the
//! widest one (the C's `generate_format`, lines 424-474 — for the
//! integer slice of this module the widest of the first and last
//! rendering, zero-filled with the sign taking a slot), and `-t`
//! appends a terminator after the last number. Zero increments are
//! rejected by name (seq.c:147-148).
//!
//! The C generates in floating point (`double`/`long double` with
//! `%g`-style formats); floating increments and `-f` formats are their
//! own design decision (the same float-rendering adjudication as
//! printf's `e E f g G`) and are rejected by this module.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// What went wrong building a sequence.
///
/// 出口分三家，退码不同：`Usage` 与 `Zero*`/`Needs*`/`BadFormat*` 走 C 的
/// `exit(1)`/`errx(1, ...)` 一族，`InvalidNumber`/`OutOfRange` 走 `e_atof()` 的
/// `err(2, "%s", num)` 与 `errx(2, "invalid floating point argument: %s", num)` 一族
/// （`seq.c:383-400`）——退 2 不是退 1 的变体，两族的句子主人也不同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqError {
    /// `errflg` 攒出来的那一支：libc 的行（可能零条、一条或多条）之后**一行** usage。
    /// `-h`、操作数个数不对、环内字母落 `default` 都是 `reports` 为空的这一支。
    Usage { reports: Vec<crate::GetoptReport> },
    /// 操作数过不了 `strtod` 的「尾巴必须有字符」判据（`seq.c:394-396`）。
    InvalidNumber(String),
    /// 操作数溢出或下溢到 `ERANGE`（`seq.c:391-393`，`err` 的三段形带 `strerror`）。
    OutOfRange(String),
    /// 第三个操作数给的是零步长，`first < last` 那一支（`seq.c:147-148`）。
    ZeroIncrement,
    /// 零步长的另一支名（`(first < last) ? "in" : "de"`，所以 `2 0 2` 也叫 `decrement`）。
    ZeroDecrement,
    /// `first < last` with a non-positive increment (seq.c:155-156).
    NeedsPositiveIncrement,
    /// `first > last` with a non-negative increment (seq.c:158-159).
    NeedsNegativeDecrement,
    /// `-f` 的串过不了 `valid_format`，句子带原文（`seq.c:162-163`）。
    BadFormat(String),
    /// `unescape` 之后再判一次不过，句子**不带**原文（`seq.c:165-166`）。
    /// 本机没能构造出触发它的串（转义只会把 `\X` 收成单字符，改不出新的 `%`），
    /// 所以这一支只有句子没有门钉。
    BadFormatPlain,
    /// `-f`  requested a floating render, declared unsupported.
    FloatNotModelled,
}

/// One parsed `seq` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqOptions {
    pub first: i64,
    /// `None` when defaulted: the direction follows `first` versus
    /// `last` (seq.c:151-153).
    pub incr: Option<i64>,
    pub last: i64,
    /// `-w`（第一次）：补位到共同宽度。
    pub equalize: bool,
    /// 补的字符：`-w` 一次是 `0`，两次起是空格（`seq.c:115-120` 的 `equalize++` 后置 `pad = SPACE`）。
    pub pad: char,
    /// `-s`: separator between numbers (default newline).
    pub separator: String,
    /// `-t`: terminator appended after the last number.
    pub terminator: Option<String>,
}

/// 一次 `seq` 调用跑完选项环之后手上有的东西（`seq.c:101-125` 的 `switch` 各支置的量）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqArgs<'a> {
    /// `-f` 的**原文**：C 在环里只存 `optarg`，`valid_format`/`unescape` 排在读操作数之后。
    pub fmt: Option<&'a str>,
    /// `-s` 的值，环内就过了 `unescape`（`seq.c:110`）。
    pub separator: String,
    /// `-t` 的值，环内就过了 `unescape`（`seq.c:113`）。
    pub terminator: Option<String>,
    /// `-w` 在环里出现的次数。C 写的是 `if (!fmt) if (equalize++) pad = SPACE;`，
    /// 所以第一次只把 `equalize` 置起来（补 `0`），**第二次起**才换成补空格。
    pub equalize: u32,
    /// 环之后的操作数原词（C 的 `argv += optind`）。
    pub operands: Vec<&'a str>,
}

/// C 的 `numeric()`（`seq.c` 环前护栏 `!numeric(argv[optind])`）。
///
/// 它**不是**「能解析成数」：先跳一个符号，吃一串十进制数字，允许一个小数点，
/// 而 `e`/`E` 之后**必须紧跟符号**才算数字。参照件量出的两形因此分道：
/// `-1e-2` 是数字（护栏放行，`-5` 那一类以短横开头的操作数就是靠它护住的），
/// 而 `1e2` 不是数字——护栏不挡它，但 getopt 自己在首字符不是 `-` 处立刻收环
/// （`getopt.c:74-78`），所以 `seq 1e2` 照样把 `1e2` 当操作数。
#[must_use]
pub fn numeric(word: &str) -> bool {
    let bytes = word.as_bytes();
    let mut i = usize::from(matches!(bytes.first(), Some(b'-') | Some(b'+')));
    let mut seen_point = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            i += 1;
            continue;
        }
        if !seen_point && b == b'.' {
            seen_point = true;
            i += 1;
            continue;
        }
        if b == b'e' || b == b'E' {
            i += 1;
            if matches!(bytes.get(i), Some(b'-') | Some(b'+')) {
                i += 1;
                continue;
            }
        }
        return false;
    }
    true
}

/// C 的 `unescape()`：`\a \b \e \f \n \r \t \v \\ \' \"`、`\` 后最多三位八进制、
/// `\x` 后最多两位十六进；**认不出的转义把反斜杠原样留着**（参照件 `-s '\q' 1 3`
/// 打的是 `1\q2\q3\q`），串尾单个反斜杠则整个丢掉（`-t '\'` 打得出来但没有 terminator）。
#[must_use]
pub fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let Some(&esc) = bytes.get(i + 1) else {
            break; // 收尾的单个反斜杠：C 的循环条件让下一个字符是 `\0`，整段不再产出
        };
        i += 2;
        match esc {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'e' => out.push(0x1b),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'\\' => out.push(b'\\'),
            b'\'' => out.push(b'\''),
            b'"' => out.push(b'"'),
            b'0'..=b'7' => {
                // `\\101` → `A`：反斜杠后最多三位八进制，且**转义后的那位就是第一位**
                let mut j = i - 1;
                let mut value = 0u32;
                let mut count = 0usize;
                while count < 3 && matches!(bytes.get(j), Some(&d) if (b'0'..=b'7').contains(&d)) {
                    value = (value << 3) | u32::from(bytes[j] - b'0');
                    j += 1;
                    count += 1;
                }
                i = j;
                out.push(value as u8);
            }
            b'x' => {
                // `\\xd0` → 0xD0：最多两位十六进（`isxdigit` 认大写**数字**，
                // 但引导字母只有小写 `x` 一支——C 的 `switch` 没有 `case 'X'`）
                let mut j = i;
                let mut value = 0u32;
                let mut count = 0usize;
                while count < 2 && matches!(bytes.get(j), Some(&d) if d.is_ascii_hexdigit()) {
                    let d = bytes[j];
                    let digit = if d.is_ascii_digit() {
                        d - b'0'
                    } else {
                        (d | 0x20) - b'a' + 10
                    };
                    value = (value << 4) | u32::from(digit);
                    j += 1;
                    count += 1;
                }
                i = j;
                out.push(value as u8);
            }
            other => {
                out.push(b'\\');
                out.push(other);
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// C 的 `valid_format()`：**只有** `A a E e F G g` 算转换，`%%` 跳过且不计数，
/// 结尾判据是 `conversions <= 1`。参照件量出的接受集因此反直觉：
/// `-f '%d'` 与 `-f '%d\n'` **都是坏的**（`d` 不是浮点转换，落 `default: return 0`），
/// `-f 'abc'`、`-f '%%'` 倒是合法（零个转换），`-f '%g%g'` 坏在两个转换。
#[must_use]
pub fn valid_format(fmt: &str) -> bool {
    let bytes = fmt.as_bytes();
    let mut i = 0usize;
    let mut conversions = 0u32;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if bytes.get(i) == Some(&b'%') {
            i += 1;
            continue;
        }
        while i < bytes.len() && b"#0- +'".contains(&bytes[i]) {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        match bytes.get(i) {
            Some(b'A' | b'a' | b'E' | b'e' | b'F' | b'f' | b'G' | b'g') => {
                conversions += 1;
                i += 1;
            }
            // 包括 `%` 收尾（C 在 `switch (*fmt)` 上看到 `\0` 走 default）
            _ => return false,
        }
    }
    conversions <= 1
}

/// `seq.c:101` 的选项环：`while ((optind < argc) && !numeric(argv[optind]) && (c = getopt(...)) != -1)`。
///
/// 这一支与同族其它环有三处不同，全部经参照件现量：
/// ① 环**不在第一个坏字母处停**——`errflg` 是累加的，`seq -+- 3` 打两行 libc
///    （`+` 与 `-`，第二个 `+` 已在 `-+-` 这个 token 之外）再加一行 usage；
/// ② `h` 在 optstring 里，libc 不会为它印任何东西，而 C 的 `case 'h': /* help (GNU) */`
///    后面没有 `break`，**直接落进 `default: errflg++`**，所以 `-h` 是「只有 usage 一行」；
/// ③ 环前那道 `!numeric(argv[optind])` 护栏只作用于**当前 token 是不是 `-` 开头**——
///    不以 `-` 开头的 token 由 libc 自己收环（`getopt.c:74-78` 不推进 `optind`），
///    所以 `seq 1e2`（`e` 后无符号，护栏认它不是数字）照旧打出 1..100；
/// ④ 于是护栏真正挡的是负数操作数：`-1` 是操作数，而 `-5e310` 进环被拆成一串集外字母。
pub fn parse_seq_args<'a>(args: &'a [&'a str]) -> Result<SeqArgs<'a>, SeqError> {
    const OPTSTRING: &[u8] = b"f:hs:t:w";
    let mut parsed = SeqArgs {
        fmt: None,
        separator: "\n".to_string(),
        terminator: None,
        equalize: 0,
        operands: Vec::new(),
    };
    let mut reports: Vec<crate::GetoptReport> = Vec::new();
    let mut errflg = false;
    let mut i = 0usize;
    while i < args.len() && !numeric(args[i]) {
        let token = args[i].as_bytes();
        // getopt 自己的收环判据：没有首字符 `-`、或光一个 `-`，都算「不是选项」，
        // 且**不推进** `optind`（`getopt.c:74-78`、`:87-92`）；`--` 结束环并吃掉自身（`:81-85`）。
        if token.len() < 2 || token[0] != b'-' {
            break;
        }
        if token == b"--" {
            i += 1;
            break;
        }
        let mut k = 1usize;
        while k < token.len() {
            let letter = token[k];
            k += 1;
            // `getopt.c:95-104`：集外字母**不叫环停下来**。libc 印一行，`optind` 只在
            // token 见底的那一次推进，于是 `-ABC` 是三行、`-wé` 是两个裸字节各一行。
            let Some(slot) = OPTSTRING.iter().position(|&o| o == letter && o != b':') else {
                reports.push(crate::GetoptReport::UnknownOption(char::from(letter)));
                errflg = true;
                if k >= token.len() {
                    i += 1;
                }
                continue;
            };
            if OPTSTRING.get(slot + 1) == Some(&b':') {
                let value = if k < token.len() {
                    &args[i][k..]
                } else {
                    i += 1;
                    match args.get(i) {
                        Some(next) => *next,
                        None => {
                            // `getopt.c:127-136`：值不存在 → libc 一行 + BADCH，`optind` 已越过末尾
                            reports.push(crate::GetoptReport::MissingValue(char::from(letter)));
                            errflg = true;
                            break;
                        }
                    }
                };
                match letter {
                    b'f' => parsed.fmt = Some(value),
                    b's' => parsed.separator = unescape(value),
                    b't' => parsed.terminator = Some(unescape(value)),
                    _ => {}
                }
                // 值若是下一个 token 吃进来的，`optind` 要越过它（`getopt.c:138-139`）；
                // 值若紧贴在本 token 上，这一跳就是移到下一个 token。两种情形都只加一次。
                i += 1;
                break;
            }
            match letter {
                // `if (!fmt) if (equalize++) pad = SPACE;`：计数交给壳去分成两级
                b'w' => {
                    if parsed.fmt.is_none() {
                        parsed.equalize += 1;
                    }
                }
                // `-h` 在集内，libc 无话可说；C 让它落进 default 置 errflg
                b'h' => errflg = true,
                _ => {}
            }
            if k >= token.len() {
                i += 1;
            }
        }
    }
    parsed.operands = args[i..].to_vec();
    if parsed.operands.is_empty() || parsed.operands.len() > 3 {
        errflg = true;
    }
    if errflg {
        return Err(SeqError::Usage { reports });
    }
    Ok(parsed)
}

/// C 的 `e_atof()`：`strtod` 之后**尾巴必须到结尾**，否则
/// `errx(2, "invalid floating point argument: %s", num)`；`errno == ERANGE` 走
/// `err(2, "%s", num)` 的三段形（参照件 `seq 1e310` → rc 2，`seq 1e-40000` 同形）。
/// 判序也照 C：先看 `ERANGE` 再看杂字符。值为 `-0.0` 时 C 抹掉符号（`seq.c:e_atof`
/// 的「zero shall have no sign」那一支），这里同一处理。
///
/// 两处量不出来只能登记的差：① 下溢到**非零**次正规数（`1e-310`）在本机链的 glibc
/// 上仍置 `ERANGE`，而 Minix 用的 gdtoa 那份实现不在本仓库里（`minix3/lib/libc/
/// arch/x86_64/gdtoa/` 只有头文件），所以这条规则只按「结果为 0」来判——登记
/// `SEQ-SUBNORMAL-ERANGE`；② `inf`/`nan` 在 C 是一路走到底的形状（`seq inf` 因
/// `first <= last` 永真而刷不完，参照件被 `timeout` 杀掉；`seq nan` 打空并退 0），
/// 本层的整数引擎把非有限值挡在渲染之前——登记 `SEQ-NONFINITE-OPERAND`。
pub fn e_atof(word: &str) -> Result<f64, SeqError> {
    // 空串是合法的 0：`strtod` 一个字符都没转时把 `endptr` 停在**起点**，而起点就是
    // 那个 `\0`，于是 `*endp != '\0'` 这一判过得了（参照件 `seq ""` 打 `1\n0\n`，
    // `tmp/seq-320/measure5.log`）。非空而转不出前缀的（`-Z`、`3x`）才落 `errx`。
    if word.is_empty() {
        return Ok(0.0);
    }
    match crate::floatfmt::parse_float_prefix(word) {
        Some((value, consumed)) if consumed == word.len() => {
            if value.is_infinite() && !word.bytes().any(|b| b == b'i' || b == b'I') {
                return Err(SeqError::OutOfRange(word.to_string()));
            }
            if value == 0.0 && word.bytes().any(|b| b"123456789".contains(&b)) {
                return Err(SeqError::OutOfRange(word.to_string()));
            }
            if value == -0.0 { Ok(0.0) } else { Ok(value) }
        }
        _ => Err(SeqError::InvalidNumber(word.to_string())),
    }
}

/// Parses the operand words (`last` | `first last` | `first incr last`)
/// and the flag words. 前门是 [`parse_seq_args`]，这一支只做 C 在环之后那一串：
/// 操作数按 `last`→`first`→`incr` 的次序解析（所以报错先报最后一个操作数）、
/// 零步长与反方向两判、`-f` 的两道 `valid_format`。
pub fn parse(args: &[&str]) -> Result<SeqOptions, SeqError> {
    let parsed = parse_seq_args(args)?;
    let operands = &parsed.operands;
    let last = e_atof(operands[operands.len() - 1])?;
    let first = if operands.len() > 1 {
        e_atof(operands[0])?
    } else {
        1.0
    };
    let incr = if operands.len() > 2 {
        let incr = e_atof(operands[1])?;
        if incr == 0.0 {
            return Err(if first < last {
                SeqError::ZeroIncrement
            } else {
                SeqError::ZeroDecrement
            });
        }
        Some(incr)
    } else {
        None
    };
    if let Some(fmt) = parsed.fmt {
        if !valid_format(fmt) {
            return Err(SeqError::BadFormat(fmt.to_string()));
        }
        let unescaped = unescape(fmt);
        if !valid_format(&unescaped) {
            return Err(SeqError::BadFormatPlain);
        }
        // 说明符本身合法之后，按 `%g`/`%.2f` 之类渲染浮点是另一件事：
        // 与 printf 家同一处裁决，本引擎只走整数。
        return Err(SeqError::FloatNotModelled);
    }
    // `f64::fract` 不在 core 里，整数性改用截断回读判；先由 `is_finite` 与上限挡住越界。
    let whole =
        |v: f64| v.is_finite() && v.abs() <= (i64::MAX as f64) && (v - v as i64 as f64) == 0.0;
    if !whole(first) || !whole(last) || incr.is_some_and(|i| !whole(i)) {
        return Err(SeqError::FloatNotModelled);
    }
    let (first, last) = (first as i64, last as i64);
    let incr = incr.map(|i| i as i64);
    if incr == Some(0) {
        return Err(if first < last {
            SeqError::ZeroIncrement
        } else {
            SeqError::ZeroDecrement
        });
    }
    Ok(SeqOptions {
        first,
        incr,
        last,
        equalize: parsed.equalize >= 1,
        pad: if parsed.equalize >= 2 { ' ' } else { '0' },
        separator: parsed.separator,
        terminator: parsed.terminator,
    })
}

/// The rendered width of one integer (sign takes a slot).
fn width_of(value: i64) -> usize {
    let mut digits = value.unsigned_abs().to_string().len();
    if value < 0 {
        digits += 1;
    }
    digits
}

/// `generate_format` 的向下取整除法（`seq.c:440-442` 的 `floor((last - first) / incr)`）。
/// Rust 的 `/` 是截断，`-2.67` 会得 `-2`，而 C 要 `-3`，所以余数非零且两数异号时退一格。
fn floor_div(a: i128, b: i128) -> i128 {
    let q = a / b;
    if a % b != 0 && (a < 0) != (b < 0) {
        q - 1
    } else {
        q
    }
}

/// **实际被打印的最后一个数**：C 在算宽度之前先把它折回来
/// （`if (first > last) last = first - incr * floor((first - last) / incr); else last =
/// first + incr * floor((last - first) / incr);`，`seq.c:439-442`）。
/// 参照件 `seq -w 1 5 10` 打 `1`、`6` 两个一位数 ⇒ 宽度是 1 而不是 `10` 的 2。
fn printed_last(first: i64, incr: i64, last: i64) -> i64 {
    let (first, incr) = (i128::from(first), i128::from(incr));
    let gap = i128::from(last) - first;
    let value = if gap < 0 {
        first - incr * floor_div(-gap, incr)
    } else {
        first + incr * floor_div(gap, incr)
    };
    value as i64
}

/// Renders one value, filled to `width` when equalising.
///
/// 两级的补位不对称，因为 C 只是把 `pad` 塞进 printf 的格式串
/// （`sprintf(buf, "%%%c%d%c", pad, MAX(width1, width2), 'g')`，`seq.c:467-469`）：
/// `0` 那一级是 `%0<width>g`，零填在符号之后；空格那一级是 `% <width>g`，**非负数也占一个
/// 符号位**，宽度不足时才再补空格，所以 `seq -w -w 8 10` 的 `10` 打成三列的 `" 10"`
/// （超宽不截），`seq -w -w -10 10` 的 `0` 打成 `"  0"` 而 `-9` 打成 `" -9"`。
fn render(value: i64, width: usize, equalize: bool, pad: char) -> String {
    let magnitude = value.unsigned_abs().to_string();
    if !equalize {
        if value < 0 {
            return alloc::format!("-{magnitude}");
        }
        return magnitude;
    }
    let sign = if value < 0 {
        "-"
    } else if pad == ' ' {
        " "
    } else {
        ""
    };
    let core = magnitude.len() + sign.len();
    let filler = alloc::string::String::from(pad).repeat(width.saturating_sub(core));
    if pad == ' ' {
        alloc::format!("{filler}{sign}{magnitude}")
    } else {
        alloc::format!("{sign}{filler}{magnitude}")
    }
}

/// Generates the sequence.
///
/// The C prints with a `for` whose condition runs before the first
/// value (seq.c:175-182), so `seq 5 1` with an explicit positive
/// increment prints nothing, while an omitted increment follows the
/// direction of `first` versus `last` (seq.c:151-153) and a zero
/// increment is an error (seq.c:155-159).
pub fn generate(options: &SeqOptions) -> Result<Vec<String>, SeqError> {
    let incr = match options.incr {
        Some(incr) => incr,
        None => {
            if options.first <= options.last {
                1
            } else {
                -1
            }
        }
    };
    if incr == 0 {
        return Err(SeqError::ZeroIncrement);
    }
    // Wrong-direction strides are named errors (seq.c:155-159).
    if incr < 0 && options.first < options.last {
        return Err(SeqError::NeedsPositiveIncrement);
    }
    if incr > 0 && options.first > options.last {
        return Err(SeqError::NeedsNegativeDecrement);
    }
    let width =
        width_of(options.first).max(width_of(printed_last(options.first, incr, options.last)));
    let mut values: Vec<String> = Vec::new();
    let mut value = options.first;
    let forward = incr > 0;
    while (forward && value <= options.last) || (!forward && value >= options.last) {
        values.push(render(value, width, options.equalize, options.pad));
        value = value.wrapping_add(incr);
    }
    Ok(values)
}

/// Joins the generated numbers with the separator and appends the
/// terminator, returning the full output text.
pub fn render_output(values: &[String], options: &SeqOptions) -> String {
    let mut out = values.join(&options.separator);
    if !values.is_empty() {
        out.push_str(&options.separator);
    }
    if let Some(terminator) = &options.terminator {
        out.push_str(terminator);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_words(args: &[&str]) -> Result<String, SeqError> {
        let options = parse(args)?;
        let values = generate(&options)?;
        Ok(render_output(&values, &options))
    }

    #[test]
    fn test_default_first_and_increment() {
        assert_eq!(generate_words(&["3"]), Ok("1\n2\n3\n".to_string()));
    }

    #[test]
    fn test_ring_keeps_going_after_a_bad_letter() {
        use crate::GetoptReport;
        // 单个集外字母：libc 一行 + 调用方的一行 usage
        assert_eq!(
            parse_seq_args(&["-Z"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::UnknownOption('Z')]
            })
        );
        // 聚簇里合法字母先被吃掉，坏的照报（`seq -wZ` 与 `-Zw` 同一形）
        assert_eq!(
            parse_seq_args(&["-wZ"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::UnknownOption('Z')]
            })
        );
        // errflg 是累加的，环不在第一个坏字母处停：`-+- 3` 的两行都留得住
        assert_eq!(
            parse_seq_args(&["-+-", "3"]).err(),
            Some(SeqError::Usage {
                reports: vec![
                    GetoptReport::UnknownOption('+'),
                    GetoptReport::UnknownOption('-')
                ]
            })
        );
    }

    #[test]
    fn test_missing_value_reaches_only_the_three_letters() {
        use crate::GetoptReport;
        for letter in ['f', 's', 't'] {
            let flag = format!("-{letter}");
            assert_eq!(
                parse_seq_args(std::slice::from_ref(&flag.as_str())).err(),
                Some(SeqError::Usage {
                    reports: vec![GetoptReport::MissingValue(letter)]
                }),
                "{flag}"
            );
        }
        // 聚簇里 `-w` 先被吃掉，缺值报的是 `f`（`seq -wf` 的现量同一形）
        assert_eq!(
            parse_seq_args(&["-wf"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::MissingValue('f')]
            })
        );
        // `w` 自己不带值，所以它永远不会触发这一支
        assert_eq!(parse_seq_args(&["-w", "3"]).err(), None);
    }

    #[test]
    fn test_h_is_in_the_set_and_says_nothing() {
        // `h` 在 optstring 里 ⇒ libc 不印任何东西；C 的 `case 'h'` 没有 break，
        // 直接落进 `default: errflg++`，所以这一支只有 usage 一行、reports 为空。
        assert_eq!(
            parse_seq_args(&["-h"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            parse_seq_args(&["-h", "3"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            parse_seq_args(&["-hw", "3"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
    }

    #[test]
    fn test_operand_count_face_and_double_dash() {
        // 操作数必须是 1..3 个；`--` 被吃掉、自身不是操作数（`getopt.c:81-85`）
        assert_eq!(
            parse_seq_args(&[]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            parse_seq_args(&["-w"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            parse_seq_args(&["1", "2", "3", "4"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(parse_seq_args(&["--", "3"]).unwrap().operands, vec!["3"]);
        // 裸 `-` 结束环却不推进，于是它自己是操作数（`getopt.c:87-92`）
        assert_eq!(parse_seq_args(&["-"]).unwrap().operands, vec!["-"]);
    }

    #[test]
    fn test_numeric_guard_is_not_number_parsing() {
        // 护栏认的是「这个 token 会不会被误当选项」，`e`/`E` 后必须紧跟符号才算数字
        assert!(numeric("-5"));
        assert!(numeric("-1e-2"));
        // `e` 后紧跟数字（无符号）⇒ 护栏**不认**它是数字；但参照件 `seq 1e2` 照旧
        // 打出 1..100——不以 `-` 开头的 token 由 libc 自己收环，护栏只在负号形上起作用
        assert!(!numeric("1e2"));
        assert!(
            !numeric("-5e310"),
            "e 后无符号 ⇒ 不是 numeric ⇒ 进环被拆成集外字母"
        );
        assert!(!numeric("-w"));
        // 空 token：C 的 `while (*s)` 一步不走，判据是 `*s == '\0'` ⇒ 反而是数字
        assert!(numeric(""));
        // `-5` 因此是操作数而不是选项
        assert_eq!(parse_seq_args(&["-5"]).unwrap().operands, vec!["-5"]);
    }

    #[test]
    fn test_unescape_matches_the_c_table() {
        assert_eq!(unescape("a\\tb"), "a\tb");
        assert_eq!(unescape("\\101"), "A"); // 八进制最多三位
        assert_eq!(unescape("\\x41z"), "Az"); // 十六进最多两位
        assert_eq!(
            unescape("\\X41"),
            "\\X41",
            "引导字母只有小写 x（C 没有 case 'X'）"
        );
        assert_eq!(unescape("\\q"), "\\q", "认不出的转义把反斜杠原样留着");
        assert_eq!(unescape("x\\"), "x", "串尾单个反斜杠整体丢掉");
        assert_eq!(unescape("\\\\"), "\\");
    }

    #[test]
    fn test_valid_format_only_counts_float_conversions() {
        assert!(valid_format("%g"));
        assert!(valid_format("%10g"));
        assert!(valid_format("%-.3f"));
        assert!(
            valid_format("abc"),
            "零个转换也算合法（参照件 `-f abc` 照样打字面）"
        );
        assert!(valid_format("%%"));
        assert!(!valid_format("%d"), "整数说明符在 C 眼里就是非法");
        assert!(!valid_format("%g%g"), "两个转换超过 `conversions <= 1`");
        assert!(!valid_format("%q"));
        assert!(!valid_format("%"));
    }

    #[test]
    fn test_zero_stride_and_direction_sentences_split() {
        // 三个操作数是 first、incr、last；零步长那支的名随 `first < last` 变
        assert_eq!(parse(&["1", "0", "3"]).err(), Some(SeqError::ZeroIncrement));
        assert_eq!(parse(&["5", "0", "1"]).err(), Some(SeqError::ZeroDecrement));
        assert_eq!(
            parse(&["2", "0", "2"]).err(),
            Some(SeqError::ZeroDecrement),
            "相等走 de 支"
        );
        // 反方向两判排在零步长之后（`seq.c:155-159`），落在默认步长选定之后的
        // `generate` 一侧，所以这两枚走 `generate_words` 而不是 `parse`
        assert_eq!(
            generate_words(&["1", "-1", "5"]),
            Err(SeqError::NeedsPositiveIncrement)
        );
        assert_eq!(
            generate_words(&["5", "1", "1"]),
            Err(SeqError::NeedsNegativeDecrement)
        );
    }

    #[test]
    fn test_operand_errors_name_the_last_operand_first() {
        // C 的解析次序是 last→first→incr，所以 `seq 3 -w` 报的是 `-w`（环不重排，
        // `-w` 落在操作数里）。退码由壳按 `errx(2, ...)` 一族给。
        assert_eq!(
            parse(&["3", "-w"]).err(),
            Some(SeqError::InvalidNumber("-w".to_string()))
        );
        assert_eq!(
            parse(&["3x"]).err(),
            Some(SeqError::InvalidNumber("3x".to_string()))
        );
        assert_eq!(
            parse(&["1e310"]).err(),
            Some(SeqError::OutOfRange("1e310".to_string()))
        );
        assert_eq!(
            parse(&["1e-40000"]).err(),
            Some(SeqError::OutOfRange("1e-40000".to_string())),
            "下溢到 0 与上溢同一条 err(2, ...) 出口"
        );
        assert_eq!(
            parse(&["--", "-Z"]).err(),
            Some(SeqError::InvalidNumber("-Z".to_string()))
        );
        // 空串是合法的 0：`strtod` 转不出任何东西时把 endptr 停在起点，而起点就是
        // 那个 `\0` ⇒ 「尾巴有杂字符」判据过得了
        assert_eq!(e_atof(""), Ok(0.0));
        assert_eq!(
            e_atof("-Z").err(),
            Some(SeqError::InvalidNumber("-Z".to_string()))
        );
    }

    #[test]
    fn test_equalize_width_follows_the_last_printed_value() {
        // C 的 floor 与 Rust 的截断除法在异号时差一格
        assert_eq!(floor_div(8, -3), -3);
        assert_eq!(floor_div(9, -3), -3);
        assert_eq!(floor_div(8, 3), 2);
        assert_eq!(floor_div(-8, 3), -3);
        // 宽度取自**实际打印的**首尾两值：`seq -w 1 5 10` 只打 1 与 6
        assert_eq!(printed_last(1, 5, 10), 6);
        assert_eq!(printed_last(10, -3, 1), 1);
        assert_eq!(printed_last(1, 1, 10), 10);
        assert_eq!(width_of(printed_last(1, 5, 10)), 1);
        assert_eq!(width_of(printed_last(1, 1, 10)), 2);
        // `% 2g` 让非负数也占符号位，超宽不截；`%02g` 把零填在符号之后
        assert_eq!(render(10, 2, true, ' '), String::from(" 10"));
        assert_eq!(render(10, 2, true, '0'), String::from("10"));
        assert_eq!(render(0, 3, true, ' '), String::from("  0"));
        assert_eq!(render(-9, 3, true, ' '), String::from(" -9"));
        assert_eq!(render(-5, 2, true, '0'), String::from("-5"));
    }

    #[test]
    fn test_format_rejected_before_the_float_gap() {
        assert_eq!(
            parse(&["-f", "%q", "3"]).err(),
            Some(SeqError::BadFormat("%q".to_string()))
        );
        assert_eq!(
            parse(&["-f", "%d", "3"]).err(),
            Some(SeqError::BadFormat("%d".to_string())),
            "参照件对 `%d' 报的是同一句，不是「浮点未接上」"
        );
        // 说明符合法之后才轮到浮点渲染那处裁决（本引擎只走整数，登记为 SEQ-FORMAT-RENDER）
        assert_eq!(
            parse(&["-f", "%g", "3"]).err(),
            Some(SeqError::FloatNotModelled)
        );
        assert_eq!(
            parse(&["-f", "abc", "3"]).err(),
            Some(SeqError::FloatNotModelled)
        );
    }

    #[test]
    fn test_equalize_has_two_levels() {
        let one = parse(&["-w", "1", "10"]).expect("一级等宽");
        assert!(one.equalize);
        assert_eq!(one.pad, '0');
        let two = parse(&["-w", "-w", "1", "10"]).expect("二级等宽");
        assert_eq!(two.pad, ' ');
        assert_eq!(
            parse(&["-w", "-w", "-w", "1", "10"])
                .expect("三级仍是空格")
                .pad,
            ' '
        );
        // `-f` 已给定时 `-w` 整个作废（`if (!fmt)` 那一层），但 C 仍会因浮点支先退
        assert_eq!(
            parse(&["-f", "%g", "-w", "1", "3"]).err(),
            Some(SeqError::FloatNotModelled)
        );
    }

    #[test]
    fn test_first_last_and_stride() {
        assert_eq!(
            generate_words(&["2", "2", "8"]),
            Ok("2\n4\n6\n8\n".to_string())
        );
        // Two operands: the increment follows the direction
        // (seq.c:151-153), so `seq 5 0` descends.
        assert_eq!(
            generate_words(&["5", "0"]),
            Ok("5\n4\n3\n2\n1\n0\n".to_string())
        );
    }

    #[test]
    fn test_direction_errors() {
        // seq.c:155-159 — wrong-direction increments are named errors.
        assert_eq!(
            generate_words(&["5", "1", "0"]),
            Err(SeqError::NeedsNegativeDecrement)
        );
        assert_eq!(
            generate_words(&["1", "-1", "5"]),
            Err(SeqError::NeedsPositiveIncrement)
        );
        // An omitted increment takes the direction instead: `seq 5 1`
        // descends.
        assert_eq!(
            generate_words(&["5", "1"]),
            Ok("5\n4\n3\n2\n1\n".to_string())
        );
        // The C errors on a wrong-direction stride before printing, so
        // the for-condition's empty case never reaches output; assert
        // the error instead.
        assert_eq!(
            generate_words(&["1", "-1", "5"]),
            Err(SeqError::NeedsPositiveIncrement)
        );
    }

    #[test]
    fn test_separator_flag() {
        // The C prints the separator after every number, the last one
        // included (seq.c:175-182).
        assert_eq!(generate_words(&["-s", " ", "3"]), Ok("1 2 3 ".to_string()));
    }

    #[test]
    fn test_equalize_pads_to_common_width() {
        assert_eq!(
            generate_words(&["-w", "8", "10"]),
            Ok("08\n09\n10\n".to_string())
        );
        assert_eq!(
            generate_words(&["-w", "-2", "2"]),
            Ok("-2\n-1\n00\n01\n02\n".to_string())
        );
    }

    #[test]
    fn test_terminator_flag() {
        assert_eq!(
            generate_words(&["-t", "END", "-s", " ", "2"]),
            Ok("1 2 END".to_string())
        );
    }

    #[test]
    fn test_zero_increment_rejected() {
        assert_eq!(
            generate_words(&["1", "0", "5"]),
            Err(SeqError::ZeroIncrement)
        );
    }

    #[test]
    fn test_non_integer_operand_rejected() {
        assert!(matches!(
            generate_words(&["abc"]),
            Err(SeqError::InvalidNumber(_))
        ));
    }

    #[test]
    fn test_float_format_declared_unsupported() {
        assert_eq!(
            generate_words(&["-f", "%g", "3"]),
            Err(SeqError::FloatNotModelled)
        );
    }
}
