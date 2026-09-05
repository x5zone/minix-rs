//! Arithmetic quiz scoring.
//!
//! Ground truth: `minix3/games/arithmetic/arithmetic.c`. Operands confine to
//! zero through the range (default upper bound 10, near line 99); the score
//! keeps right and wrong counters (near line 100); ranges come from `+-`
//! option letters (near line 107); over large ranges results may look odd
//! (header warning near line 63). Question generation and prompting stay with
//! the execution layer; this module owns the range, the scoring, and answer
//! checking.

use crate::GameError;

/// Default operand upper bound.
pub const DEFAULT_RANGE: u32 = 10;

/// Quiz score (right and wrong answer counters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Score {
    /// Correct answers so far.
    pub right: u32,
    /// Wrong answers so far.
    pub wrong: u32,
}

impl Score {
    /// Total questions answered.
    pub fn total(self) -> u32 {
        self.right.saturating_add(self.wrong)
    }

    /// Success percentage (zero when nothing answered yet).
    pub fn percent(self) -> u32 {
        if self.total() == 0 {
            return 0;
        }
        self.right * 100 / self.total()
    }
}

/// Parse a range bound (must be strictly positive).
pub fn parse_range(word: &str) -> Result<u32, GameError> {
    if word.is_empty() {
        return Err(GameError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(GameError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(GameError::OutOfRange)?;
    }
    if value == 0 {
        return Err(GameError::InvalidArgument);
    }
    Ok(value)
}

/// Question operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// Addition.
    Add,
    /// Subtraction (floored at zero for young players).
    Subtract,
    /// Multiplication.
    Multiply,
    /// Integer division (generated divisible by construction).
    Divide,
}

/// One question (operands plus operator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Question {
    /// Left operand.
    pub left: u32,
    /// Right operand.
    pub right: u32,
    /// Operator.
    pub op: Operator,
}

/// Correct answer for `question` (subtraction floors at zero).
pub fn answer_of(question: Question) -> u32 {
    match question.op {
        Operator::Add => question.left.saturating_add(question.right),
        Operator::Subtract => question.left.saturating_sub(question.right),
        Operator::Multiply => question.left.saturating_mul(question.right),
        Operator::Divide => {
            if question.right == 0 {
                0
            } else {
                question.left / question.right
            }
        }
    }
}

/// Grade `given` against `question`, advancing `score`.
pub fn grade(score: &mut Score, question: Question, given: u32) -> bool {
    if given == answer_of(question) {
        score.right = score.right.saturating_add(1);
        true
    } else {
        score.wrong = score.wrong.saturating_add(1);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ranges_parse() {
        assert_eq!(parse_range("10"), Ok(10));
        assert_eq!(parse_range("0"), Err(GameError::InvalidArgument));
        assert_eq!(parse_range(""), Err(GameError::InvalidArgument));
        assert_eq!(parse_range("+"), Err(GameError::InvalidArgument));
    }

    #[test]
    fn test_answers_computed() {
        let add = Question { left: 3, right: 4, op: Operator::Add };
        assert_eq!(answer_of(add), 7);
        let sub = Question { left: 3, right: 9, op: Operator::Subtract };
        assert_eq!(answer_of(sub), 0);
        let mul = Question { left: 3, right: 4, op: Operator::Multiply };
        assert_eq!(answer_of(mul), 12);
        let div = Question { left: 12, right: 4, op: Operator::Divide };
        assert_eq!(answer_of(div), 3);
    }

    #[test]
    fn test_grading_counts() {
        let mut score = Score::default();
        let question = Question { left: 2, right: 2, op: Operator::Add };
        assert!(grade(&mut score, question, 4));
        assert!(!grade(&mut score, question, 5));
        assert_eq!(score.right, 1);
        assert_eq!(score.wrong, 1);
        assert_eq!(score.percent(), 50);
    }

    #[test]
    fn test_empty_score_zero() {
        assert_eq!(Score::default().percent(), 0);
    }
}
