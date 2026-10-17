//! Random filtering: one over denominator selection.
//!
//! Ground truth: `minix3/games/random/random.c`. Seeding mixes the clock with
//! the process identifier (near line 107). Filtering prints each input line
//! with probability one over the denominator (near line 126: selected when a
//! draw below the denominator equals zero); exit status mode returns a value
//! between zero and the denominator minus one. Drawing stays with the
//! execution layer; this module owns the denominator rule.

use crate::TextGameError;

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
pub const NO_OPERAND_DENOMINATOR: u32 = 2;

/// 操作数那一步的落点：C 的 `switch (argc)`（`random.c:89-104`，`argc` 已是 `optind`
/// 之后的剩余个数）有四个出口，本类型逐一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperandLeg {
    /// `case 0`：没有操作数，分母取 [`NO_OPERAND_DENOMINATOR`]。
    NoOperandDefault,
    /// `case 1` 认下了这枚操作数：分母就是它报出的数。
    Accepted(u32),
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
/// `denom`——**上界是闭的**，门按表达式钉，不按注释。
pub fn exit_status_value(denominator: u32, draw: u64) -> u64 {
    (denominator as u64).wrapping_mul(draw) / RANDOM_MAX
}

/// 判操作数这一步落在哪一出。
///
/// 射程只到「个数 + 两句报文」：取值仍走 [`parse_denominator`] 的十进制整数面，而 C 用的是
/// `strtod`，于是 `1e3`、`1e-3`、`inf`、`nan` 这些 C 认下的形在这里落进 [`OperandLeg::NotValid`]。
/// 那是「分母的取值域」一件事，与这里的「报文形状」一件事分开的两条腿。
pub fn judge_operands(operands: &[&str]) -> OperandLeg {
    let [only] = operands else {
        return if operands.is_empty() {
            OperandLeg::NoOperandDefault
        } else {
            OperandLeg::TooMany
        };
    };
    match parse_denominator(only) {
        Ok(value) => OperandLeg::Accepted(value),
        // C 的 `case 1` 只有两个出口：`errno == ERANGE` 走带号的三段形，其余一切
        // （`denom == 0`、`*ep != '\0'`、一个字符都没吃掉）都归同一句 `errx`。
        Err(TextGameError::OutOfRange) => OperandLeg::OutOfRange,
        Err(_) => OperandLeg::NotValid,
    }
}

/// Parse a denominator (strictly positive).
///
/// 十进制整数面，非空且每一位都是 ASCII 数字；溢出报 [`TextGameError::OutOfRange`]，
/// 零与非数字报 [`TextGameError::InvalidArgument`]。C 那一侧是 `strtod`，取值域更宽，
/// 差集归「分母的取值域」那条腿。
pub fn parse_denominator(word: &str) -> Result<u32, TextGameError> {
    if word.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(TextGameError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(TextGameError::OutOfRange)?;
    }
    if value == 0 {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(value)
}

/// True when a uniform `draw` below `denominator` selects the line (the draw
/// equals zero, which happens with probability one over the denominator).
pub fn is_selected(draw: u32, denominator: u32) -> Result<bool, TextGameError> {
    if denominator == 0 || draw >= denominator {
        return Err(TextGameError::InvalidArgument);
    }
    Ok(draw == 0)
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
    fn test_denominators_parse() {
        assert_eq!(parse_denominator("6"), Ok(6));
        assert_eq!(parse_denominator("0"), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_denominator(""), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_denominator("6x"), Err(TextGameError::InvalidArgument));
    }

    #[test]
    fn test_selection_rule() {
        assert!(is_selected(0, 6).unwrap());
        assert!(!is_selected(5, 6).unwrap());
        assert_eq!(
            is_selected(6, 6).map(|_| ()),
            Err(TextGameError::InvalidArgument)
        );
        assert_eq!(
            is_selected(0, 0).map(|_| ()),
            Err(TextGameError::InvalidArgument)
        );
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
        assert_eq!(judge_operands(&["3"]), OperandLeg::Accepted(3));
        assert_eq!(judge_operands(&["3", "4"]), OperandLeg::TooMany);
        // 第三枚是空串也不改变个数：判的是个数，不是内容
        assert_eq!(judge_operands(&["3", "", "4"]), OperandLeg::TooMany);
        assert_eq!(NO_OPERAND_DENOMINATOR, 2);
    }

    #[test]
    fn a_rejected_operand_and_an_overflowed_one_are_two_different_legs() {
        // `denom == 0 || *ep != '\0'`（`:98`）与 `errno == ERANGE`（`:96`）是两句话：
        // 前者不含操作数，后者把操作数放在位上。把后者并到前者就丢掉了号
        for word in ["0", "", "abc", "-", "6x", "1e3"] {
            assert_eq!(judge_operands(&[word]), OperandLeg::NotValid, "{word:?}");
        }
        assert_eq!(judge_operands(&["4294967296"]), OperandLeg::OutOfRange);
        assert_eq!(judge_operands(&["1"]), OperandLeg::Accepted(1));
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
        assert_eq!(exit_status_value(1, 0), 0);
        // 折半抽到的是 1 而不是 2：乘法先做再整除，`4 * (RANDOM_MAX/2)` 差一点点才到 2
        assert_eq!(exit_status_value(4, RANDOM_MAX / 2), 1);
        // 3/4 处落在 **2** 而不是 3：`RANDOM_MAX * 3` 除不尽 4，整除先截掉一点，乘法再摊不平
        assert_eq!(exit_status_value(4, RANDOM_MAX * 3 / 4), 2);
        assert_eq!(exit_status_value(4, RANDOM_MAX / 4), 0);
        assert_eq!(exit_status_value(4, 0), 0);
        assert_eq!(exit_status_value(4, RANDOM_MAX), 4);
        assert_eq!(RANDOM_MAX, u64::from(u32::MAX) / 2);
    }
}
