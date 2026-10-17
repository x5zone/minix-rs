//! Random filtering: one over denominator selection.
//!
//! Ground truth: `minix3/games/random/random.c`. Seeding mixes the clock with
//! the process identifier (near line 107). The denominator is a `double` read by
//! `strtod` (near line 95), not an integer: a line is printed when
//! `(int)(denom * random() / RANDOM_MAX)` comes out zero (near line 126), and
//! exit status mode returns that same product truncated toward zero (near line
//! 111). Drawing stays with the execution layer; this module owns the
//! denominator grammar and the two predicates that read it.

/// C 的 `usage()` 那一整行（`minix3/games/random/random.c:143-147`）。
///
/// 名字写死在串里：`fprintf(stderr, "usage: random [-er] [denominator]\n")` 用的是字面量
/// `random`，不经过 `getprogname()`，所以改名调用这一行也跟着不动。C 的 `errx`/`err` 那
/// 两路相反，前缀跟着 `argv[0]` 走——同一支命令里两种来历，薄壳必须分开抄。
pub const USAGE: &str = "usage: random [-er] [denominator]\n";

/// C 的 `errx(1, "denominator is not valid.")` 里那句话（`random.c:98-99`）。
///
/// 句尾的点是 C 自带的，操作数**不在**这句话里——`errx` 不像 `err` 那样有 `%s` 位，
/// 所以这一支报不出是哪一个数把分母判死的。
pub const NOT_VALID_SENTENCE: &str = "denominator is not valid.";

/// C 的 `case 0` 定下的分母：一枚操作数都没有时，`denom = 2`（`random.c:90-92`）。
/// `denom` 在 C 里是 `double`（`random.c:67`），所以这个常数也按 `f64` 定。
pub const NO_OPERAND_DENOMINATOR: f64 = 2.0;

/// 操作数那一步的落点：C 的 `switch (argc)`（`random.c:89-104`，`argc` 已是 `optind`
/// 之后的剩余个数）有四个出口，本类型逐一对应。
// 只到 `PartialEq`：`Accepted` 位上装的是 C 的那个 `double`，而浮点没有全等关系
#[derive(Debug, Clone, PartialEq)]
pub enum OperandLeg {
    /// `case 0`：没有操作数，分母取 [`NO_OPERAND_DENOMINATOR`]。
    NoOperandDefault,
    /// `case 1` 认下了这枚操作数：分母就是 `strtod` 报出的那个 `double`。
    Accepted(f64),
    /// `case 1` 否掉了这枚操作数（`denom == 0 || *ep != '\0'`，`:98`）：[`NOT_VALID_SENTENCE`]。
    NotValid,
    /// `case 1` 撞上数值范围（`errno == ERANGE`，`:96`）：`err(1, "%s", *argv)` 的三段形。
    OutOfRange,
    /// `default`：两枚以上操作数（`:101-103`），[`USAGE`] 退 1。
    TooMany,
}

/// C 的 optstring `"er"` 走完环之后剩下的事：两个旗标与操作数起点（`random.c:72-87`）。
///
/// 两个字母都**不带值**，所以 libc 那句 `option requires an argument` 在本支不可达；可达的
/// 只有一枚集外字母（`random.c:80-83` 把 `?` 与 `default` 都送进 `usage()`）。裸 `-` 既不是
/// 选项也不是结束标记：optstring 里没有 `-`，libc 回 `-1` 且**不推进游标**
/// （`minix3/lib/libc/stdlib/getopt.c:86-93`），于是它留在操作数里由 `strtod` 去否掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    /// 环正常结束；`operands_at` 是操作数在 `argv[1..]` 里的起点（C 的 `optind` 折算）。
    Done {
        /// `-e`：把抽到的数当退码返回，且在读标准输入之前就返回（`random.c:109-111`）。
        exit_status: bool,
        /// `-r`：关掉 stdio 的输出缓冲（`random.c:117-118`）。
        unbuffered: bool,
        /// 操作数从这一枚起。
        operands_at: usize,
    },
    /// 集外字母：libc 先打一行 `unknown option -- <字母>`，紧接着才是 `usage()` 那一行。
    Unknown(char),
}

/// C 的 libc 那一行去掉程序名与换行后的中段（`minix3/lib/libc/stdlib/getopt.c:101-104`
/// 的字面是 `"%s: unknown option -- %c\n"`）。字母前后**没有**引号——参照件是在宿主上编的，
/// 它那句 `invalid option -- 'Z'` 是 glibc 的字样，不在这条的射程内。
pub const UNKNOWN_OPTION: &str = "unknown option -- ";

/// 走完选项环。只认「以 `-` 开头且长度大于一」的 token 为选项；`--` 是结束标记；其余一切
/// （含裸 `-`）从这一枚起都算操作数。集外字母当场返回，环不再往下走（C 的 `usage()` 即退出）。
pub fn scan_ring(tokens: &[&str]) -> Ring {
    let mut exit_status = false;
    let mut unbuffered = false;
    let mut index = 0usize;
    while index < tokens.len() {
        let Some(body) = tokens[index].strip_prefix('-') else {
            break;
        };
        if body.is_empty() {
            break;
        }
        if body == "-" {
            index += 1;
            break;
        }
        for letter in body.bytes() {
            match letter {
                b'e' => exit_status = true,
                b'r' => unbuffered = true,
                other => {
                    return Ring::Unknown(other as char);
                }
            }
        }
        index += 1;
    }
    Ring::Done {
        exit_status,
        unbuffered,
        operands_at: index,
    }
}

/// C 那把标尺：`random()` 的上界（`random.c:111` 与 `:126` 都拿它除）。
pub const RANDOM_MAX: u64 = 2_147_483_647;

/// C 的 `-e` 那一支：`(denom * random()) / RANDOM_MAX`（`random.c:111`）。
///
/// 源码注释写的是「between zero and denom minus one」，可表达式在抽取值到顶时取到的正是
/// `denom`——**上界是闭的**，门按表达式钉，不按注释。返回值是 C 在 `return` 之前完成的那次
/// `double`→`int`：向零截。非有限值或超出 `i32` 射程的值在 C 里是未定义行为，这里取 `as`
/// 的饱和语义（参照件所在的 x86-64 给 `0x80000000`），登记为不追平的一格。
pub fn exit_status_value(denominator: f64, draw: u64) -> i32 {
    (denominator * draw as f64 / RANDOM_MAX as f64) as i32
}

/// 过滤器那一行的判据：C 的 `(int)(denom * random() / RANDOM_MAX) == 0`（`random.c:126`）。
///
/// 商向零截之后为 0，等价于商严格落在 `(-1, 1)`。写成开区间比较而不是先转整数，是因为那次
/// 转换在 `inf`/`nan` 上是 C 的未定义行为，而区间比较对这两形给出的答案与参照件所在的
/// x86-64 一致：都不为 0，也就都不掷中。`inf * 0` 是 NaN，同样落在区间外。
pub fn line_selected(denominator: f64, draw: u64) -> bool {
    let scaled = denominator * draw as f64 / RANDOM_MAX as f64;
    scaled > -1.0 && scaled < 1.0
}

/// 判操作数这一步落在哪一出。
///
/// 个数走 C 的 `switch (argc)`；一枚的那一支交给 [`scan_strtod`] 取值，再按 C 的两判据分流：
/// `errno == ERANGE` 走带操作数的三段形，`denom == 0` 或 `*ep != '\0'` 归 `errx` 那句不含
/// 操作数的话。**`ERANGE` 判在前**（`random.c:96` 早于 `:98`），所以 `18e308` 报的是号，
/// 不是「不是有效分母」——尽管那个值本身也确实当不了分母。
pub fn judge_operands(operands: &[&str]) -> OperandLeg {
    let [only] = operands else {
        return if operands.is_empty() {
            OperandLeg::NoOperandDefault
        } else {
            OperandLeg::TooMany
        };
    };
    let read = scan_strtod(only);
    if read.erange {
        OperandLeg::OutOfRange
    } else if read.value == 0.0 || read.rest != only.len() {
        OperandLeg::NotValid
    } else {
        OperandLeg::Accepted(read.value)
    }
}

/// `strtod` 报回来的三样东西，`random.c:95-99` 正好只用这三样。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strtod {
    /// `strtod` 的返回值：一个 `double`。
    pub value: f64,
    /// 没被吃掉那一段的起点，即 C 的 `ep` 下标。`rest == word.len()` 就是 `*ep == '\0'`；
    /// 一个字符都没吃（C 的 `ret0`）时是 0。值判为 0 的形不看这个数——`denom == 0` 先挡住。
    pub rest: usize,
    /// C 的 `errno == ERANGE`：溢出（`strtod.c` 的 `ovfl`）与非零数字串下溢成 0
    /// （`undfl`）两条腿都立它，二者都汇到 `range_err`。
    pub erange: bool,
}

/// C 的 `strtod(*argv, &ep)`（`random.c:95`）那条**十进制腿**的复写：前导空白、一个可选
/// 符号、数字与一个小数点、一个可选的 `e`/`E` 指数，再加上 `#ifdef INFNAN_CHECK` 开着的
/// `inf`/`nan`（`minix3/lib/libc/gdtoa/gdtoaimp.h:191` 与 `:643-645` 都把它立着）。
///
/// 形状取自 `minix3/lib/libc/gdtoa/strtod.c` 的 `_int_strtod_l`：空白与符号在开头的
/// `switch (*s)`，指数在 `if (c == 'e' || c == 'E')`，`inf`/`nan` 在「一个数字都没见到」
/// 那块里。`match` 折小写（`minix3/lib/libc/gdtoa/smisc.c:133-135`），所以 `INF`、
/// `Infinity`、`NaN` 都算数。两形不在射程内、账在 `RANDOM-DENOM-LEGS` 条目上：十六进制
/// 浮点 `0x1p3`（同一函数里 `#ifndef NO_HEX_FP` 那块，本仓库没定义过这个宏）与
/// `nan(0x…)` 的载荷。舍入也不逐位对齐：C 走 gdtoa 的大数精化，这里按
/// `有效数字 × 10^指数` 乘出来，超过 17 位的数字串丢掉尾巴，因此极端位数上可能差一个
/// 末位单位——只影响「恰好压在阈值那一格」的选中判定。
pub fn scan_strtod(word: &str) -> Strtod {
    let bytes = word.as_bytes();
    let mut index = 0usize;
    while matches!(bytes.get(index), Some(&c) if matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    {
        index += 1;
    }
    let mut negative = false;
    match bytes.get(index) {
        Some(b'-') => {
            negative = true;
            index += 1;
        }
        Some(b'+') => index += 1,
        _ => {}
    }
    // 符号之后什么都没有了：C 从符号那一格直接 `goto ret0`，值 0、一个字符都没吃
    if index == bytes.len() {
        return Strtod {
            value: 0.0,
            rest: 0,
            erange: false,
        };
    }

    let mut mantissa = 0.0f64;
    let mut kept = 0u32; // 进 mantissa 的位数
    let mut dropped = 0i32; // 超出位数上限被丢掉的位数，折进十幂
    let mut fraction = 0i32; // 小数点后的数字个数
    let mut any_digit = false;
    let mut nonzero = false;
    let mut past_dot = false;
    while let Some(&c) = bytes.get(index) {
        if c == b'.' && !past_dot {
            past_dot = true;
            index += 1;
            continue;
        }
        if !c.is_ascii_digit() {
            break;
        }
        any_digit = true;
        if nonzero || c != b'0' {
            nonzero = true;
            if kept < 17 {
                mantissa = mantissa * 10.0 + f64::from(c - b'0');
                kept += 1;
            } else {
                dropped += 1;
            }
        }
        if past_dot {
            fraction += 1;
        }
        index += 1;
    }
    let mut exponent = dropped - fraction;

    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        if !any_digit {
            // C 的 `if (!nd && !nz && !nz0) goto ret0`：`e5` 这一形整个不吃
            return Strtod {
                value: 0.0,
                rest: 0,
                erange: false,
            };
        }
        let mark = index;
        index += 1;
        let mut esign = 1i32;
        match bytes.get(index) {
            Some(b'-') => {
                esign = -1;
                index += 1;
            }
            Some(b'+') => index += 1,
            _ => {}
        }
        if matches!(bytes.get(index), Some(&c) if c.is_ascii_digit()) {
            while matches!(bytes.get(index), Some(&b'0')) {
                index += 1;
            }
            let mut magnitude = 0i32;
            let mut counted = 0u32;
            while matches!(bytes.get(index), Some(&c) if c.is_ascii_digit()) {
                counted += 1;
                if magnitude <= 19_999 {
                    magnitude = magnitude * 10 + i32::from(bytes[index] - b'0');
                }
                index += 1;
            }
            // C 的防溢出闸：位数超过 8 或数值超过 19999 就按 19999 走（`strtod.c` 里
            // 那句 `e = 19999; /* safe for 16 bit ints */`）
            if counted > 8 || magnitude > 19_999 {
                magnitude = 19_999;
            }
            exponent = exponent.saturating_add(esign.saturating_mul(magnitude));
        } else {
            // 指数位上没有数字（`1e`、`1e-`）：C 把游标退回 `e` 那一格，值照报
            index = mark;
        }
    }

    // `inf` 与 `nan` 只在「一个数字都没见到」时才检（C 把这段放在 `if (!nd)` 里面，
    // 由 `switch (c)` 收 `i`/`I`、`n`/`N` 起头）
    if !any_digit {
        // C 的 `--s` 再 `match(&s,"inity")`：吃得下就吃完，吃不下退回 `nf` 之后，于是
        // `inf` 与 `infinity` 两形都算数、`infi` 不算
        let infinity = match bytes.get(index) {
            Some(b'i') | Some(b'I') => match_fold(bytes, index + 1, b"nf")
                .map(|after_nf| match_fold(bytes, after_nf, b"inity").unwrap_or(after_nf)),
            _ => None,
        };
        if let Some(rest) = infinity {
            return Strtod {
                value: if negative {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                rest,
                erange: false,
            };
        }
        let not_a_number = match bytes.get(index) {
            Some(b'n') | Some(b'N') => match_fold(bytes, index + 1, b"an"),
            _ => None,
        };
        if let Some(rest) = not_a_number {
            return Strtod {
                value: f64::NAN,
                rest,
                erange: false,
            };
        }
        return Strtod {
            value: 0.0,
            rest: 0,
            erange: false,
        };
    }

    let mut value = mantissa;
    if nonzero {
        // `10^n` 按平方累乘，与 gdtoa 的 `tens[]`/`bigtens[]` 同形；超出 `f64` 射程时它
        // 自己就成了 `inf`，于是下面的两条判定接得住溢出与下溢
        value = if exponent >= 0 {
            mantissa * pow10(exponent as u32)
        } else {
            mantissa / pow10(exponent.unsigned_abs())
        };
    }
    Strtod {
        value: if negative { -value } else { value },
        rest: index,
        erange: nonzero && (value == 0.0 || value.is_infinite()),
    }
}

/// `match(&s, t)`（`minix3/lib/libc/gdtoa/smisc.c:123-140`）：从 `at` 起比一段字面，
/// 比中的话**两边都折小写**，返回吃掉之后的下标。C 的先递增量一个字符再比，所以调用者
/// 传进来的首字母（`switch (c)` 认过的那个）不在比较之列。
fn match_fold(bytes: &[u8], at: usize, lit: &[u8]) -> Option<usize> {
    let tail = bytes.get(at..at + lit.len())?;
    tail.iter()
        .zip(lit)
        .all(|(got, want)| got.to_ascii_lowercase() == *want)
        .then_some(at + lit.len())
}

/// 十的整数幂（非负指数）。
fn pow10(power: u32) -> f64 {
    let mut base = 10.0f64;
    let mut out = 1.0f64;
    let mut rest = power;
    while rest != 0 {
        if rest & 1 != 0 {
            out *= base;
        }
        rest >>= 1;
        if rest != 0 {
            base *= base;
        }
    }
    out
}

/// Mix a clock reading with a process identifier into a seed (mirrors the
/// seeding blend of time plus process in the original source).
pub fn mix_seed(seconds: u64, micros: u32, pid: u32) -> u64 {
    seconds
        .wrapping_add(micros as u64)
        .wrapping_add(pid as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_decimal_grammar_is_the_one_strtod_reads() {
        // 值、`ep` 下标、是否 `ERANGE`：三个读数一起钉，形状对着 `strtod.c` 那条十进制腿
        let cases: [(&str, f64, usize, bool); 14] = [
            ("6", 6.0, 1, false),
            (" 3", 3.0, 2, false), // 前导空白吃掉
            ("3 ", 3.0, 1, false), // 尾巴不吃：`*ep != '\0'` 由调用者去判
            ("1e3", 1000.0, 3, false),
            ("1e-3", 0.001, 4, false),
            (".5", 0.5, 2, false),
            ("5.", 5.0, 2, false),
            ("0.001", 0.001, 5, false),
            ("010", 10.0, 3, false),
            ("1e00", 1.0, 4, false), // 指数位上只有零：C 吃掉它们、把 e 记作 0
            ("-2", -2.0, 2, false),
            ("4294967296", 4.294967296e9, 10, false),
            ("18e308", f64::INFINITY, 6, true),
            ("1e-400", 0.0, 6, true),
        ];
        for (word, value, rest, erange) in cases {
            let read = scan_strtod(word);
            assert_eq!(read.value, value, "{word:?} 的值");
            assert_eq!((read.rest, read.erange), (rest, erange), "{word:?}");
        }
        // 位进展望只在 17 位以内：尾巴丢掉、折进十幂，所以 21 位那一串落在 2.1e20
        assert_eq!(scan_strtod("210000000000000000000").value, 2.1e20);
        // `inf` 与 `nan` 靠 `INFNAN_CHECK` 那块；`match` 折小写，所以三形都算数
        assert_eq!(scan_strtod("inf").value, f64::INFINITY);
        assert_eq!(scan_strtod("infinity").rest, 8);
        assert_eq!(scan_strtod("INF").rest, 3);
        assert_eq!(scan_strtod("-inf").value, f64::NEG_INFINITY);
        assert!(scan_strtod("nan").value.is_nan());
        assert!(scan_strtod("NaN").value.is_nan());
        assert_eq!(scan_strtod("nan").rest, 3);
        // `infi` 那一形 C 不吃 `inity` 的第一格之外的东西：`ep` 停在第 3 格
        assert_eq!(scan_strtod("infi").rest, 3);
    }

    #[test]
    fn the_selection_predicate_is_the_cast_then_compare_itself() {
        // C 的一句是「截成整数再与 0 比」（`random.c:126`）：商向零截为 0 等价于商在 (-1,1)
        assert!(line_selected(1.0, 0));
        assert!(line_selected(1.0, RANDOM_MAX - 1));
        assert!(!line_selected(1.0, RANDOM_MAX)); // 商正好 1.0：`(int)1.0 == 1` → 不掷中
        assert!(line_selected(2.0, 1));
        assert!(!line_selected(2.0, 1_073_741_824)); // 商刚过 1.0
        assert!(!line_selected(2.0, RANDOM_MAX)); // 商正好 2.0
        assert!(line_selected(0.5, RANDOM_MAX)); // 小于一的分母恒掷中，商最大 0.5
        assert!(line_selected(-1e-3, RANDOM_MAX)); // 负数向零截：(-1, 0] 一律截成 0
        assert!(!line_selected(-1.0, RANDOM_MAX)); // 商正好 -1.0 → 截成 -1 → 不掷中
        assert!(line_selected(-1.0, RANDOM_MAX - 1));
        assert!(!line_selected(f64::INFINITY, 7));
        assert!(!line_selected(f64::NEG_INFINITY, 7));
        assert!(!line_selected(f64::NAN, 7));
        assert!(!line_selected(f64::INFINITY, 0)); // `inf * 0` 是 NaN，同样在区间外
    }

    #[test]
    fn test_seed_mixes() {
        assert_ne!(mix_seed(100, 5, 7), mix_seed(100, 5, 8));
        assert_ne!(mix_seed(100, 5, 7), mix_seed(101, 5, 7));
    }

    #[test]
    fn the_operand_count_picks_the_leg() {
        // `switch (argc)` 的四个出口（`random.c:89-104`）：零枚不是「缺参数」而是「分母 2」，
        // 两枚以上才是 usage，中间那一枚才轮到取值判断
        assert_eq!(judge_operands(&[]), OperandLeg::NoOperandDefault);
        assert_eq!(judge_operands(&["3"]), OperandLeg::Accepted(3.0));
        assert_eq!(judge_operands(&["3", "4"]), OperandLeg::TooMany);
        // 第三枚是空串也不改变个数：判的是个数，不是内容
        assert_eq!(judge_operands(&["3", "", "4"]), OperandLeg::TooMany);
        assert_eq!(NO_OPERAND_DENOMINATOR, 2.0);
    }

    #[test]
    fn a_rejected_operand_and_an_overflowed_one_are_two_different_legs() {
        // `denom == 0 || *ep != '\0'`（`:98`）与 `errno == ERANGE`（`:96`）是两句话：
        // 前者不含操作数，后者把操作数放在位上。把后者并到前者就丢掉了号
        for word in ["0", "", "abc", "-", "6x", "e5", "1e", "0x10", "infi"] {
            assert_eq!(judge_operands(&[word]), OperandLeg::NotValid, "{word:?}");
        }
        for word in ["18e308", "1e-400"] {
            assert_eq!(judge_operands(&[word]), OperandLeg::OutOfRange, "{word:?}");
        }
        // 全零的数字串不立 ERANGE（C 的 `nz0` 那条路直接 `goto ret`），落回「不是有效分母」，
        // 哪怕指数大得离谱
        assert_eq!(judge_operands(&["0e99999"]), OperandLeg::NotValid);
        assert_eq!(judge_operands(&["1"]), OperandLeg::Accepted(1.0));
        // `2^32` 在 `double` 里是精确的，不该报溢出
        assert_eq!(
            judge_operands(&["4294967296"]),
            OperandLeg::Accepted(4.294967296e9)
        );
        // `nan` 认下（C 的 `denom == 0` 对 NaN 为假），但值不可比：`PartialEq` 上 NaN 不
        // 等于自己，所以这里只验它落在 `Accepted` 那一支
        assert!(matches!(
            judge_operands(&["nan"]),
            OperandLeg::Accepted(value) if value.is_nan()
        ));
    }

    #[test]
    fn the_two_hardwired_lines_keep_c_spelling() {
        // usage 那一行以字面量 `random` 开头并自带换行（`:143-147`）；`errx` 那句话带句尾的
        // 点且不含操作数（`:98-99`）。两处都是逐字节比对，改一个字符就得改这条断言
        assert_eq!(USAGE, "usage: random [-er] [denominator]\n");
        assert_eq!(NOT_VALID_SENTENCE, "denominator is not valid.");
    }

    #[test]
    fn the_option_ring_stops_at_the_first_offender() {
        // 环只认 `e`/`r`；集外字母当场返回。聚簇里报的是**第一个**集外字母，
        // `-eZ` 与 `-Ze` 都停在 `Z`（C 的环一遇 `?` 就 usage() 退出，不会继续往下扫）
        assert_eq!(
            scan_ring(&[]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 0
            }
        );
        assert_eq!(
            scan_ring(&["-e"]),
            Ring::Done {
                exit_status: true,
                unbuffered: false,
                operands_at: 1
            }
        );
        assert_eq!(
            scan_ring(&["-er", "-r", "3"]),
            Ring::Done {
                exit_status: true,
                unbuffered: true,
                // 下标落在 `argv[1..]` 上：两枚选项 token 吃掉之后，`3` 在第 2 格
                operands_at: 2
            }
        );
        assert_eq!(scan_ring(&["-Z"]), Ring::Unknown('Z'));
        assert_eq!(scan_ring(&["-eZ"]), Ring::Unknown('Z'));
        assert_eq!(scan_ring(&["-Ze"]), Ring::Unknown('Z'));
    }

    #[test]
    fn end_of_options_and_a_bare_dash_are_two_different_things() {
        // `--` 是结束标记：它自己不算操作数，下一枚起才算。裸 `-` 相反——它**就是**操作数
        assert_eq!(
            scan_ring(&["--"]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 1
            }
        );
        assert_eq!(
            scan_ring(&["--", "3"]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 1
            }
        );
        assert_eq!(
            scan_ring(&["-"]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 0
            }
        );
        assert_eq!(
            scan_ring(&["-e", "-"]),
            Ring::Done {
                exit_status: true,
                unbuffered: false,
                operands_at: 1
            }
        );
    }

    #[test]
    fn the_exit_status_scales_by_the_expression_not_by_the_comment() {
        // 上界闭着：抽取值到顶时结果就是分母本身
        assert_eq!(exit_status_value(1.0, 0), 0);
        // 折半抽到的是 1 而不是 2：乘法先做再整除，`4 * (RANDOM_MAX/2)` 差一点点才到 2
        assert_eq!(exit_status_value(4.0, RANDOM_MAX / 2), 1);
        // 3/4 处落在 **2** 而不是 3：`RANDOM_MAX * 3` 除不尽 4，整除先截掉一点，乘法再摊不平
        assert_eq!(exit_status_value(4.0, RANDOM_MAX * 3 / 4), 2);
        assert_eq!(exit_status_value(4.0, RANDOM_MAX / 4), 0);
        assert_eq!(exit_status_value(4.0, 0), 0);
        assert_eq!(exit_status_value(4.0, RANDOM_MAX), 4);
        // 向零截对负商一样成立：`-1 * RANDOM_MAX / RANDOM_MAX` 是 -1.0
        assert_eq!(exit_status_value(-1.0, RANDOM_MAX), -1);
        // 小于一的分母抽不出非零的退码
        assert_eq!(exit_status_value(0.5, RANDOM_MAX), 0);
        assert_eq!(RANDOM_MAX, u64::from(u32::MAX) / 2);
    }
}
