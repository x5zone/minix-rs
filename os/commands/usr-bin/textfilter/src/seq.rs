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
//! rejected by name (seq.c:147 and following).
//!
//! The C generates in floating point (`double`/`long double` with
//! `%g`-style formats); floating increments and `-f` formats are their
//! own design decision (the same float-rendering adjudication as
//! printf's `e E f g G`) and are rejected by this module.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// What went wrong building a sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqError {
    /// The `last` operand is missing.
    MissingLast,
    /// An operand is not an integer.
    InvalidNumber(String),
    /// The increment is zero.
    ZeroIncrement,
    /// `first < last` with a non-positive increment (seq.c:154-156).
    NeedsPositiveIncrement,
    /// `first > last` with a non-negative increment (seq.c:157-159).
    NeedsNegativeDecrement,
    /// `-f` requested a floating render, declared unsupported.
    FloatNotModelled,
}

/// One parsed `seq` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqOptions {
    pub first: i64,
    /// `None` when defaulted: the direction follows `first` versus
    /// `last` (seq.c:150-152).
    pub incr: Option<i64>,
    pub last: i64,
    /// `-w`: zero-pad every rendering to the common width.
    pub equalize: bool,
    /// `-s`: separator between numbers (default newline).
    pub separator: String,
    /// `-t`: terminator appended after the last number.
    pub terminator: Option<String>,
}

/// Parses the operand words (`last` | `first last` | `first incr last`)
/// and the flag words. `-f` is declared unsupported.
pub fn parse(args: &[&str]) -> Result<SeqOptions, SeqError> {
    let mut equalize = false;
    let mut separator = "\n".to_string();
    let mut terminator: Option<String> = None;
    let mut operands: Vec<i64> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        // Negative numbers are operands: egetopt treats a minus
        // followed by a digit as the end of the options.
        if !arg.starts_with('-')
            || arg.len() == 1
            || (arg.len() > 1 && arg.as_bytes()[1].is_ascii_digit())
        {
            operands.push(number(arg)?);
            index += 1;
            continue;
        }
        let mut chars = arg[1..].chars().peekable();
        while let Some(flag) = chars.next() {
            match flag {
                'w' => equalize = true,
                'f' => return Err(SeqError::FloatNotModelled),
                's' | 't' => {
                    let mut value: String = chars.by_ref().collect();
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = (*word).to_string(),
                            None => return Err(SeqError::MissingLast),
                        }
                    }
                    if flag == 's' {
                        separator = value;
                    } else {
                        terminator = Some(value);
                    }
                    break;
                }
                _ => return Err(SeqError::InvalidNumber(arg.to_string())),
            }
        }
        index += 1;
    }
    let (first, incr, last) = match operands.len() {
        1 => (1, None, operands[0]),
        2 => (operands[0], None, operands[1]),
        3 => (operands[0], Some(operands[1]), operands[2]),
        _ => return Err(SeqError::MissingLast),
    };
    if incr == Some(0) {
        return Err(SeqError::ZeroIncrement);
    }
    Ok(SeqOptions {
        first,
        incr,
        last,
        equalize,
        separator,
        terminator,
    })
}

fn number(word: &str) -> Result<i64, SeqError> {
    word.parse::<i64>()
        .map_err(|_| SeqError::InvalidNumber(word.to_string()))
}

/// The rendered width of one integer (sign takes a slot).
fn width_of(value: i64) -> usize {
    let mut digits = value.unsigned_abs().to_string().len();
    if value < 0 {
        digits += 1;
    }
    digits
}

/// Renders one value, zero-filled to `width` when equalising.
fn render(value: i64, width: usize, equalize: bool) -> String {
    let magnitude = value.unsigned_abs().to_string();
    if !equalize {
        if value < 0 {
            return alloc::format!("-{magnitude}");
        }
        return magnitude;
    }
    let fill = width.saturating_sub(magnitude.len() + usize::from(value < 0));
    if value < 0 {
        alloc::format!("-{}{magnitude}", "0".repeat(fill))
    } else if fill > 0 {
        alloc::format!("{}{magnitude}", "0".repeat(fill))
    } else {
        magnitude
    }
}

/// Generates the sequence.
///
/// The C prints with a `for` whose condition runs before the first
/// value (seq.c:180-190), so `seq 5 1` with an explicit positive
/// increment prints nothing, while an omitted increment follows the
/// direction of `first` versus `last` (seq.c:150-152) and a zero
/// increment is an error (seq.c:154-159).
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
    // Wrong-direction strides are named errors (seq.c:154-159).
    if incr < 0 && options.first < options.last {
        return Err(SeqError::NeedsPositiveIncrement);
    }
    if incr > 0 && options.first > options.last {
        return Err(SeqError::NeedsNegativeDecrement);
    }
    let width = width_of(options.first).max(width_of(options.last));
    let mut values: Vec<String> = Vec::new();
    let mut value = options.first;
    let forward = incr > 0;
    while (forward && value <= options.last) || (!forward && value >= options.last) {
        values.push(render(value, width, options.equalize));
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
    fn test_first_last_and_stride() {
        assert_eq!(generate_words(&["2", "2", "8"]), Ok("2\n4\n6\n8\n".to_string()));
        // Two operands: the increment follows the direction
        // (seq.c:150-152), so `seq 5 0` descends.
        assert_eq!(generate_words(&["5", "0"]), Ok("5\n4\n3\n2\n1\n0\n".to_string()));
    }

    #[test]
    fn test_direction_errors() {
        // seq.c:154-159 — wrong-direction increments are named errors.
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
        assert_eq!(generate_words(&["5", "1"]), Ok("5\n4\n3\n2\n1\n".to_string()));
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
        // included (seq.c:184-188).
        assert_eq!(generate_words(&["-s", " ", "3"]), Ok("1 2 3 ".to_string()));
    }

    #[test]
    fn test_equalize_pads_to_common_width() {
        assert_eq!(generate_words(&["-w", "8", "10"]), Ok("08\n09\n10\n".to_string()));
        assert_eq!(generate_words(&["-w", "-2", "2"]), Ok("-2\n-1\n00\n01\n02\n".to_string()));
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
        assert_eq!(generate_words(&["1", "0", "5"]), Err(SeqError::ZeroIncrement));
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
        assert_eq!(generate_words(&["-f", "%g", "3"]), Err(SeqError::FloatNotModelled));
    }
}
