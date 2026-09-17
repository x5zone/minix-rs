//! The POSIX `expr` expression evaluator.
//!
//! Ground truth: `minix3/bin/expr/expr.y` (NetBSD 1.x, yacc). Precedence
//! climbs through `|`, `&`, the six comparisons, `+ -`, `* / %`, and `:`
//! (left associative throughout, expr.y:62-68); `length` is a prefix
//! unary above `:`; parentheses group. Values are strings: arithmetic
//! goes through `is_integer` (empty string counts as integer zero) and
//! the overflow/underflow checks of `perform_arith_op` (expr.y:273-373),
//! comparisons go numeric only when both sides are integers (the C's
//! `strcoll` byte ordering in the C locale otherwise, expr.y:170-221),
//! `:` anchors a basic regular expression at the first byte and yields
//! the match length or the first captured group (expr.y:102-142, "0"
//! when the pattern has no groups and nothing matched, empty string when
//! it has), and `length` counts bytes (expr.y:223-233). Errors — syntax,
//! non-integer arithmetic operands, division by zero, overflow, value
//! range — all leave with status 2 (expr.y:429-439). The leading `--`
//! operand quirk of `yylex` (expr.y:402-416) is carried over too.
//!
//! `:` needs a basic regular expression engine, which is the 08-stage
//! library — the same position the C's `expr` occupies when it links
//! libc's `regcomp`.

use alloc::format;
use alloc::string::{String, ToString};
use minix_regex::pattern::{compile_basic, Pattern};

/// What went wrong; every variant leaves with status 2 in the doing half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    /// The token stream does not form an expression ("syntax error").
    Syntax,
    /// Arithmetic met a non-integer operand ("non-integer argument").
    NonInteger(String),
    /// A numeric operand exceeded 64-bit range.
    OutOfRange(String),
    /// Division or remainder by zero; the operator rides along.
    DivideByZero(String),
    /// Signed overflow or underflow in `+ - *`.
    Overflow {
        /// Left operand.
        left: String,
        /// Operator.
        op: String,
        /// Right operand.
        right: String,
    },
    /// The `:` pattern did not compile.
    BadRegex,
}

/// One binary operator with its yacc precedence (lower binds later).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Binary<'a> {
    Or,
    And,
    Compare(&'a str),
    AddSub(&'a str),
    MulDiv(&'a str),
    MatchOp,
}

fn binary_of<'a>(token: &'a str) -> Option<(Binary<'a>, u8)> {
    match token {
        "|" => Some((Binary::Or, 1)),
        "&" => Some((Binary::And, 2)),
        "=" | ">" | ">=" | "<" | "<=" | "!=" => Some((Binary::Compare(token), 3)),
        "+" | "-" => Some((Binary::AddSub(token), 4)),
        "*" | "/" | "%" => Some((Binary::MulDiv(token), 5)),
        ":" => Some((Binary::MatchOp, 6)),
        _ => None,
    }
}

/// Evaluates one `expr` invocation (the tokens after the program name).
///
/// The leading `--` quirk is `yylex`'s (expr.y:402-416): a first `--`
/// token is swallowed when the following token is an operand or
/// parenthesis, and becomes a literal string operand otherwise.
pub fn evaluate(tokens: &[&str]) -> Result<String, ExprError> {
    let mut position = 0usize;
    if tokens.first() == Some(&"--") {
        let next = tokens.get(1);
        let swallowed = match next {
            None => false,
            Some(token) => !matches!(binary_of(token), Some(_)) && *token != "length"
                && *token != "(" && *token != ")",
        };
        if swallowed {
            position = 1;
        } else {
            // "--" itself is a string operand; fall through without the
            // swallow, mirroring the C's rewind.
            return parse_binary(tokens, &mut position, 1);
        }
    }
    parse_binary(tokens, &mut position, 1)
}

/// Precedence climbing: parse while the next binary operator binds at
/// least `min_level` (left associative, like the yacc `%left` rows).
fn parse_binary<'a>(
    tokens: &'a [&'a str],
    position: &mut usize,
    min_level: u8,
) -> Result<String, ExprError> {
    let mut left = parse_unary(tokens, position)?;
    loop {
        let Some(token) = tokens.get(*position) else {
            break;
        };
        let Some((op, level)) = binary_of(token) else {
            break;
        };
        if level < min_level {
            break;
        }
        *position += 1;
        let right = parse_binary(tokens, position, level + 1)?;
        left = apply(op, left, right)?;
    }
    Ok(left)
}

/// Prefix unary forms and plain operands: `length` and parentheses.
fn parse_unary<'a>(
    tokens: &'a [&'a str],
    position: &mut usize,
) -> Result<String, ExprError> {
    let Some(token) = tokens.get(*position) else {
        return Err(ExprError::Syntax);
    };
    if *token == "length" {
        *position += 1;
        let value = parse_unary(tokens, position)?;
        return Ok(format!("{}", value.len()));
    }
    if *token == "(" {
        *position += 1;
        let value = parse_binary(tokens, position, 1);
        match tokens.get(*position) {
            Some(&")") => *position += 1,
            _ => return Err(ExprError::Syntax),
        }
        return value;
    }
    if *token == ")" || binary_of(token).is_some() {
        // An operator token can be an operand (`item` rule), but not in a
        // slot where an operand is structurally required — the yacc
        // grammar answers that with a syntax error too.
        return Err(ExprError::Syntax);
    }
    *position += 1;
    Ok((*token).to_string())
}

/// Applies one binary operator (the grammar actions of expr.y:79-233).
fn apply<'a>(
    op: Binary<'a>,
    left: String,
    right: String,
) -> Result<String, ExprError> {
    match op {
        Binary::Or => {
            if !is_zero_or_null(&left) {
                Ok(left)
            } else {
                Ok(right)
            }
        }
        Binary::And => {
            if !is_zero_or_null(&left) && !is_zero_or_null(&right) {
                Ok(left)
            } else {
                Ok("0".to_string())
            }
        }
        Binary::Compare(operator) => {
            // Numeric when both sides are integers — the empty string is
            // an integer zero (`is_integer`, expr.y:263-271) — otherwise
            // byte order stands in for the C locale's `strcoll`.
            let result = if is_integer(&left) && is_integer(&right) {
                compare(saturating_i64(&left), saturating_i64(&right))
            } else {
                compare_bytes(left.as_bytes(), right.as_bytes())
            };
            let truth = match operator {
                "=" => result == 0,
                "!=" => result != 0,
                ">" => result > 0,
                ">=" => result >= 0,
                "<" => result < 0,
                _ => result <= 0,
            };
            Ok(if truth { "1".to_string() } else { "0".to_string() })
        }
        Binary::AddSub(operator) => {
            let (l, r) = integer_pair(&left, &right)?;
            let res = match operator {
                "+" => {
                    let candidate = l.wrapping_add(r);
                    if (candidate < 0 && l > 0 && r > 0)
                        || (candidate > 0 && l < 0 && r < 0)
                    {
                        return Err(ExprError::Overflow {
                            left: left.clone(),
                            op: operator.to_string(),
                            right: right.clone(),
                        });
                    }
                    candidate
                }
                _ => {
                    let candidate = l.wrapping_sub(r);
                    if (candidate < 0 && l > 0 && l > r)
                        || (candidate > 0 && l < 0 && l < r)
                    {
                        return Err(ExprError::Overflow {
                            left: left.clone(),
                            op: operator.to_string(),
                            right: right.clone(),
                        });
                    }
                    candidate
                }
            };
            Ok(format!("{}", res))
        }
        Binary::MulDiv(operator) => {
            let (l, r) = integer_pair(&left, &right)?;
            if operator == "/" && r == 0 {
                return Err(ExprError::DivideByZero(operator.to_string()));
            }
            if operator == "%" && r == 0 {
                return Err(ExprError::DivideByZero(operator.to_string()));
            }
            if operator == "*" && (l == 0 || r == 0) {
                return Ok("0".to_string());
            }
            let res = match operator {
                "*" => {
                    let candidate = l.wrapping_mul(r);
                    let sign = (l < 0) ^ (r < 0);
                    if (candidate < 0 && !sign) || (candidate > 0 && sign) || candidate == 0 {
                        return Err(ExprError::Overflow {
                            left: left.clone(),
                            op: operator.to_string(),
                            right: right.clone(),
                        });
                    }
                    candidate
                }
                "/" => l.wrapping_div(r),
                _ => l.wrapping_rem(r),
            };
            Ok(format!("{}", res))
        }
        Binary::MatchOp => {
            let pattern: Pattern = compile_basic(&right).map_err(|_| ExprError::BadRegex)?;
            let anchored = pattern
                .find_bytes(left.as_bytes(), 0)
                .filter(|(start, _, _)| *start == 0);
            match anchored {
                Some((start, end, captures)) => match captures.spans[1] {
                    Some((group_start, group_end)) => {
                        Ok(left[group_start as usize..group_end as usize].to_string())
                    }
                    None => Ok(format!("{}", end - start)),
                },
                None if count_groups(&right) == 0 => Ok("0".to_string()),
                None => Ok(String::new()),
            }
        }
    }
}

fn compare(l: i64, r: i64) -> i64 {
    if l < r {
        -1
    } else if l > r {
        1
    } else {
        0
    }
}

fn compare_bytes(l: &[u8], r: &[u8]) -> i64 {
    for (lb, rb) in l.iter().zip(r.iter()) {
        match lb.cmp(rb) {
            core::cmp::Ordering::Less => return -1,
            core::cmp::Ordering::Greater => return 1,
            core::cmp::Ordering::Equal => {}
        }
    }
    (l.len() as i64) - (r.len() as i64)
}

/// `is_integer` (expr.y:263-271): an optional sign then digits, with the
/// empty string counting as a valid integer zero.
fn is_integer(text: &str) -> bool {
    let body = skip_blanks(strip_sign(skip_blanks(text)).1);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit())
        || text.is_empty()
}

/// `is_zero_or_null` (expr.y:250-258): empty, or a fully parsed zero.
///
/// Public because it also decides the doing half's exit status
/// (expr.y:73-76: status 1 for a null-or-zero result).
pub fn is_zero_or_null(text: &str) -> bool {
    text.is_empty() || (is_integer(text) && saturating_i64(text) == 0)
}

fn integer_pair<'a>(
    left: &'a str,
    right: &'a str,
) -> Result<(i64, i64), ExprError> {
    if !is_integer(left) {
        return Err(ExprError::NonInteger(left.to_string()));
    }
    if !is_integer(right) {
        return Err(ExprError::NonInteger(right.to_string()));
    }
    let l = ranged_i64(left)?;
    let r = ranged_i64(right)?;
    Ok((l, r))
}

/// `is_integer`-style parse with the C's range check: out-of-range
/// values are an error (expr.y:290-306), not a saturation.
fn ranged_i64(text: &str) -> Result<i64, ExprError> {
    let body = skip_blanks(strip_sign(skip_blanks(text)).1).to_string();
    if body.is_empty() {
        return Ok(0);
    }
    let mut magnitude: u64 = 0;
    for &byte in body.as_bytes() {
        if !byte.is_ascii_digit() {
            return Ok(magnitude as i64);
        }
        magnitude = magnitude
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u64))
            .ok_or_else(|| ExprError::OutOfRange(text.to_string()))?;
    }
    let limit = u64::MAX / 2 + 1;
    let negative = strip_sign(skip_blanks(text)).0;
    if negative {
        if magnitude > limit {
            return Err(ExprError::OutOfRange(text.to_string()));
        }
        Ok(if magnitude == limit {
            i64::MIN
        } else {
            -(magnitude as i64)
        })
    } else {
        if magnitude > i64::MAX as u64 {
            return Err(ExprError::OutOfRange(text.to_string()));
        }
        Ok(magnitude as i64)
    }
}

fn saturating_i64(text: &str) -> i64 {
    let (negative, body) = strip_sign(skip_blanks(text));
    let mut value: i64 = 0;
    for &byte in body.as_bytes() {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value.saturating_mul(10).saturating_add((byte - b'0') as i64);
    }
    if negative {
        -value
    } else {
        value
    }
}

fn skip_blanks(word: &str) -> &str {
    let mut rest = word;
    while matches!(
        rest.as_bytes().first(),
        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(0x0B) | Some(0x0C)
    ) {
        rest = &rest[1..];
    }
    rest
}

fn strip_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

/// Counts capturing groups `\(`...`\)`, the basic-regex shape of
/// `re_nsub`, so a no-match result picks "0" versus empty string.
fn count_groups(pattern: &str) -> usize {
    let bytes = pattern.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'\\' && bytes[index + 1] == b'(' {
            count += 1;
        }
        index += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(tokens: &[&str]) -> Result<String, ExprError> {
        evaluate(tokens)
    }

    #[test]
    fn test_arithmetic_and_precedence() {
        assert_eq!(eval(&["1", "+", "2", "*", "3"]), Ok("7".to_string()));
        assert_eq!(eval(&["(", "1", "+", "2", ")", "*", "3"]), Ok("9".to_string()));
        assert_eq!(eval(&["10", "-", "4", "-", "3"]), Ok("3".to_string()));
        assert_eq!(eval(&["10", "/", "3"]), Ok("3".to_string()));
        assert_eq!(eval(&["10", "%", "3"]), Ok("1".to_string()));
    }

    #[test]
    fn test_or_and_returns() {
        assert_eq!(eval(&["1", "|", "x"]), Ok("1".to_string()));
        assert_eq!(eval(&["", "|", "x"]), Ok("x".to_string()));
        assert_eq!(eval(&["0", "|", "x"]), Ok("x".to_string()));
        assert_eq!(eval(&["1", "&", "x"]), Ok("1".to_string()));
        assert_eq!(eval(&["0", "&", "x"]), Ok("0".to_string()));
        assert_eq!(eval(&["1", "&", ""]), Ok("0".to_string()));
    }

    #[test]
    fn test_comparisons_numeric_and_string() {
        assert_eq!(eval(&["2", ">", "10"]), Ok("0".to_string()));
        assert_eq!(eval(&["abc", ">", "abd"]), Ok("0".to_string()));
        assert_eq!(eval(&["5", "=", "5"]), Ok("1".to_string()));
        assert_eq!(eval(&["5", "!=", "5"]), Ok("0".to_string()));
        assert_eq!(eval(&["", "<=", ""]), Ok("1".to_string()));
    }

    #[test]
    fn test_match_returns_length_or_group() {
        assert_eq!(eval(&["abcdef", ":", "abc"]), Ok("3".to_string()));
        assert_eq!(eval(&["abcdef", ":", "abc\\(.*\\)"]), Ok("def".to_string()));
        // Anchored: a match later in the string does not count. A
        // groupless pattern misses as "0" (expr.y:135-136); a grouped
        // pattern misses as the empty string.
        assert_eq!(eval(&["abcdef", ":", "def"]), Ok("0".to_string()));
        assert_eq!(eval(&["abcdef", ":", "xyz"]), Ok("0".to_string()));
        assert_eq!(eval(&["abcdef", ":", "xyz\\(.*\\)"]), Ok("".to_string()));
    }

    #[test]
    fn test_length_keyword() {
        assert_eq!(eval(&["length", "abc"]), Ok("3".to_string()));
        assert_eq!(eval(&["length", ""]), Ok("0".to_string()));
        // The C binds `length` to the next unary expression, so the
        // addition happens outside: length(2) + 2 = 4.
        assert_eq!(eval(&["length", "ab", "+", "2"]), Ok("4".to_string()));
    }

    #[test]
    fn test_empty_operand_is_integer_zero() {
        assert_eq!(eval(&["", "+", "1"]), Ok("1".to_string()));
        assert_eq!(eval(&["", "=", "0"]), Ok("1".to_string()));
    }

    #[test]
    fn test_divide_by_zero_is_an_error() {
        assert_eq!(
            eval(&["1", "/", "0"]),
            Err(ExprError::DivideByZero("/".to_string()))
        );
        assert_eq!(
            eval(&["1", "%", "0"]),
            Err(ExprError::DivideByZero("%".to_string()))
        );
    }

    #[test]
    fn test_overflow_is_an_error() {
        let big = "9223372036854775807";
        assert!(matches!(
            eval(&[big, "+", "1"]),
            Err(ExprError::Overflow { .. })
        ));
        assert!(matches!(
            eval(&["-9223372036854775808", "-", "1"]),
            Err(ExprError::Overflow { .. })
        ));
        // Multiplication: the C's sign check catches the mixed-sign
        // overflow, and even flags a zero result (i64::MIN times two
        // wraps to zero, expr.y:365-368).
        assert!(matches!(
            eval(&["-9223372036854775808", "*", "2"]),
            Err(ExprError::Overflow { .. })
        ));
        // Declared quirk of the C check: all-positive operands wrap to a
        // positive value and slip through, e.g. big * big renders "1".
        assert_eq!(eval(&[big, "*", big]), Ok("1".to_string()));
    }

    #[test]
    fn test_non_integer_arithmetic_operand_rejected() {
        assert_eq!(
            eval(&["abc", "+", "1"]),
            Err(ExprError::NonInteger("abc".to_string()))
        );
    }

    #[test]
    fn test_out_of_range_operand_rejected() {
        assert!(matches!(
            eval(&["9223372036854775808", "+", "1"]),
            Err(ExprError::OutOfRange(_))
        ));
    }

    #[test]
    fn test_leading_dash_operand_needs_no_quirk() {
        // `expr - 5` treats the single-character "-" as the subtraction
        // operator; the string form arrives via the leading `--` quirk.
        assert_eq!(eval(&["10", "-", "5"]), Ok("5".to_string()));
    }

    #[test]
    fn test_leading_double_dash_quirk() {
        // A first "--" followed by an operand is swallowed.
        assert_eq!(eval(&["--", "5", "+", "1"]), Ok("6".to_string()));
        // A first "--" followed by an operator stays an operand — which
        // arithmetic then rejects as non-integer.
        assert_eq!(
            eval(&["--", "+", "1"]),
            Err(ExprError::NonInteger("--".to_string()))
        );
    }

    #[test]
    fn test_trailing_operator_is_syntax_error() {
        assert_eq!(eval(&["5", "+"]), Err(ExprError::Syntax));
        assert_eq!(eval(&["+"]), Err(ExprError::Syntax));
    }

    #[test]
    fn test_exit_relevance_zero_result_shape() {
        // "0" and "" are the null-or-zero shapes that drive status 1 in
        // the doing half.
        assert!(is_zero_or_null("0"));
        assert!(is_zero_or_null(""));
        assert!(!is_zero_or_null("1"));
    }
}
