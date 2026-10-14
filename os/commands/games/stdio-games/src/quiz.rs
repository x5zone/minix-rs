//! Arithmetic quiz scoring and the wrong-answer penalty lists.
//!
//! Ground truth: `minix3/games/arithmetic/arithmetic.c`. Operands confine to zero through
//! the range (default upper bound 10, `:102`); the score keeps right and wrong counters
//! (`:101`); the key set comes from `-o` over `+-x/` (`:95-97`, `:120-129`); over large
//! ranges results may look odd (header warning near `:63`). Question generation and
//! prompting stay with the execution layer; this module owns the range, the scoring,
//! the answer check, the penalty lists of `penalise()`/`getrandom()` (`:283-364`), and
//! the two numbers of C's score line — the accumulated whole seconds of `qtime`
//! (`:271-272`) and the one-decimal seconds-per-problem text (`:172-173`).

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

/// Microseconds in one second, the granularity C stamps at (`arithmetic.c:271`).
pub const MICROS_PER_SEC: u64 = 1_000_000;

/// C's `qtime += finish - start` (`arithmetic.c:271-272`), where both stamps come from
/// `time(2)` — whole seconds. The stamps truncate **before** the subtraction, so a question
/// opened 0.9 s into a second and answered 2.1 s later is charged two seconds, not one;
/// C's own comment (`:265-268`) expects those partial seconds to cancel out across
/// questions, and says nothing about the rounding, so the truncation is the contract.
pub fn elapsed_secs(start_micros: u64, finish_micros: u64) -> u64 {
    (finish_micros / MICROS_PER_SEC).saturating_sub(start_micros / MICROS_PER_SEC)
}

/// The one-decimal number C prints at `arithmetic.c:172-173`, split so the shell can lay it
/// out with `utoa` and one digit instead of owning a float formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneDecimal {
    /// The digits before the point.
    pub whole: u64,
    /// The single digit after the point.
    pub tenth: u8,
}

/// `printf("%.1f", (float)qtime / nright)` — three things about that expression are
/// load-bearing, and each one changes the printed digit somewhere:
///
/// * the division happens in **`float`**, not `double`: `3/20` is `0.15000000596…` as a
///   `float` (so `%.1f` answers `0.2`), while the same quotient computed in `double` is
///   `0.1499999999999999944…` and would print `0.1`;
/// * the promoted value is then rounded to one decimal **to nearest, ties to even** —
///   Minix3's `printf` is `vfprintf.c` over `vfwprintf.c`, which asks `gdtoa` for the
///   digits, and `gdtoa.c:782-790` rounds up only when the remainder exceeds half or
///   equals half and the last digit is odd. `ROUND_BIASED` (ties away from zero) is set
///   only for non-IEEE arithmetic (`gdtoaimp.h:225-232`, `:430-437`), which none of this
///   tree's targets is. So `5/4` prints `1.2` and `15/4` prints `3.8`;
/// * the argument is guarded by `if (nright > 0)` (`:171`), so a quiz with no correct
///   answer never divides by zero and never prints the line at all — `None` here.
///
/// Multiplying a `float` by ten needs 28 mantissa bits, so the scaling into `f64` is exact
/// and the tie test below is against the true value, not an approximation of it.
pub fn seconds_per_problem(qtime_secs: u64, right: u32) -> Option<OneDecimal> {
    if right == 0 {
        return None;
    }
    // The quotient itself is the hardware `float` division C performs; only the decimal
    // rounding is done below, in integers, because `f64::round_ties_even` is not reachable
    // from this crate's `no_std` build.
    let value = (qtime_secs as f32) / (right as f32);
    let tenths = round_half_even_after_scale(value)?;
    Some(OneDecimal {
        whole: tenths / 10,
        tenth: (tenths % 10) as u8,
    })
}

/// Multiply a `float` by ten exactly and round the result to the nearest integer, ties to
/// even — the same decision `gdtoa.c:782-790` makes (`j > 0 || (j == 0 && dig & 1)`).
///
/// The value is split into its mantissa and power of two (`m * 2^exp`, `m` up to 24 bits for
/// a normal `float`), so `10 * m` and the shift are integer work with no rounding error of
/// their own: a `float` times ten needs 28 bits, and `u128` holds the shifted forms for every
/// exponent a realistic quiz reaches.
pub fn round_half_even_after_scale(value: f32) -> Option<u64> {
    let bits = value.to_bits();
    let biased = ((bits >> 23) & 0xff) as i32;
    let fraction = bits & 0x007f_ffff;
    if biased == 0xff || value.is_sign_negative() {
        return None; // NaN / infinity: no such shape comes out of a seconds count
    }
    let (mantissa, exp) = if biased == 0 {
        (u128::from(fraction), -149) // subnormal and zero
    } else {
        (u128::from(fraction) + (1 << 23), biased - 150)
    };
    let scaled = mantissa * 10;
    let tenths = if exp >= 0 {
        let shifted = scaled.checked_shl(u32::try_from(exp).ok()?)?;
        u64::try_from(shifted).ok()?
    } else {
        let shift = u32::try_from(-exp).ok()?;
        if shift >= 128 {
            return Some(0); // far below 0.05 seconds per problem
        }
        let whole = u64::try_from(scaled >> shift).ok()?;
        let remainder = scaled & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        if remainder > half || (remainder == half && whole % 2 == 1) {
            whole.checked_add(1)?
        } else {
            whole
        }
    };
    Some(tenths)
}

/// Whether C's `showstats` prints the score block at all: `nright + nwrong > 0` (`:168`).
pub fn shows_score(score: Score) -> bool {
    score.total() > 0
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

/// C's `WRONGPENALTY` (`arithmetic.c:291`): a wrong answer charges the number five extra
/// chances to come up again.
pub const WRONG_PENALTY: u32 = 5;

/// Entries per operand list. C allocates one node per wrong answer and **forgets the
/// penalty when `malloc` fails** (`:305-307`); this fixed budget is the same rule with a
/// ceiling, so an exhausted list silently drops the new penalty instead of failing.
pub const PENALTY_CAPACITY: usize = 32;

/// What a roll turned into — C's `getrandom()` uses the number directly or walks the list
/// (`:326-363`).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Drawn {
    /// Below the range, so the value is the roll itself.
    Direct(u32),
    /// A position inside the accumulated penalties; that entry's value was taken.
    Penalised(u32),
    /// A position past the end of the list. C treats this as a bug and exits with
    /// `arithmetic: bug: inconsistent penalties.` (`:358-363`).
    Inconsistent,
}

/// The operator's slot in C's `keylist` `+-x/`, which is also `Penalties`' first index.
pub fn slot_of(op: Operator) -> usize {
    match op {
        Operator::Add => 0,
        Operator::Subtract => 1,
        Operator::Multiply => 2,
        Operator::Divide => 3,
    }
}

/// What C charges after a wrong answer: the right operand always, then the **left operand**
/// for `+` and `x` but the **answer** for `-` and `/` (`arithmetic.c:255-262`). Each entry
/// is `(slot, operand, value)`, so a caller feeds these straight into [`Penalties::add`].
pub fn charged_operands(
    op: Operator,
    left: u32,
    right: u32,
    answer: u32,
) -> [(usize, usize, u32); 2] {
    let slot = slot_of(op);
    let first_operand = if matches!(op, Operator::Add | Operator::Multiply) {
        left
    } else {
        answer
    };
    [(slot, 1, right), (slot, 0, first_operand)]
}

/// One operand's list, newest entry first exactly like C's head insertion.
#[derive(Debug, Clone, Copy)]
pub struct PenaltyList {
    values: [u32; PENALTY_CAPACITY],
    penalties: [u32; PENALTY_CAPACITY],
    len: usize,
    total: u32,
}

impl Default for PenaltyList {
    fn default() -> Self {
        PenaltyList {
            values: [0; PENALTY_CAPACITY],
            penalties: [0; PENALTY_CAPACITY],
            len: 0,
            total: 0,
        }
    }
}

impl PenaltyList {
    /// C's `penalise()`: push a fresh entry worth `WRONG_PENALTY` and grow the running sum.
    pub fn add(&mut self, value: u32) {
        if self.len == PENALTY_CAPACITY {
            return;
        }
        self.values.copy_within(0..self.len, 1);
        self.penalties.copy_within(0..self.len, 1);
        self.values[0] = value;
        self.penalties[0] = WRONG_PENALTY;
        self.len += 1;
        self.total += WRONG_PENALTY;
    }

    /// Sum of the live penalties, the term C adds to `maxval` before the modulo.
    pub fn total(&self) -> u32 {
        self.total
    }

    /// Live entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing is penalised here.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// C's selection once the roll exists: below `maxval` the roll *is* the value, and at
    /// or above it the roll counts through the list in order, consuming one penalty from the
    /// entry it lands on and dropping that entry when its penalty reaches zero.
    pub fn draw(&mut self, maxval: u32, roll: u32) -> Drawn {
        if roll < maxval {
            return Drawn::Direct(roll);
        }
        let mut position = roll - maxval;
        let mut index = 0usize;
        while index < self.len {
            if self.penalties[index] > position {
                let value = self.values[index];
                self.total -= 1;
                self.penalties[index] -= 1;
                if self.penalties[index] == 0 {
                    self.values.copy_within(index + 1..self.len, index);
                    self.penalties.copy_within(index + 1..self.len, index);
                    self.len -= 1;
                }
                return Drawn::Penalised(value);
            }
            position -= self.penalties[index];
            index += 1;
        }
        Drawn::Inconsistent
    }
}

/// The eight lists C keeps: `sizeof(keylist) - 1` operators (`opnum` over `+-x/`) by two
/// operands (`:283-287`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Penalties {
    slots: [[PenaltyList; 2]; 4],
}

impl Penalties {
    /// Charge `value` for operand `operand` of the operator slot `slot`. `problem()` does
    /// this twice per wrong answer: the right operand, then the left operand for `+`/`x`
    /// or the result for `-`/`/` (`:255-260`).
    pub fn add(&mut self, slot: usize, operand: usize, value: u32) {
        self.slots[slot][operand].add(value);
    }

    /// Draw a value: `rand` is the raw generator output, taken modulo
    /// `maxval + total` so a penalised entry crowds out the plain range (`:332`), and
    /// `roll`-handling is [`PenaltyList::draw`].
    pub fn draw(&mut self, slot: usize, operand: usize, maxval: u32, rand: u32) -> Drawn {
        let list = &mut self.slots[slot][operand];
        let span = maxval.saturating_add(list.total()).max(1);
        list.draw(maxval, rand % span)
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
        let add = Question {
            left: 3,
            right: 4,
            op: Operator::Add,
        };
        assert_eq!(answer_of(add), 7);
        let sub = Question {
            left: 3,
            right: 9,
            op: Operator::Subtract,
        };
        assert_eq!(answer_of(sub), 0);
        let mul = Question {
            left: 3,
            right: 4,
            op: Operator::Multiply,
        };
        assert_eq!(answer_of(mul), 12);
        let div = Question {
            left: 12,
            right: 4,
            op: Operator::Divide,
        };
        assert_eq!(answer_of(div), 3);
    }

    #[test]
    fn test_grading_counts() {
        let mut score = Score::default();
        let question = Question {
            left: 2,
            right: 2,
            op: Operator::Add,
        };
        assert!(grade(&mut score, question, 4));
        assert!(!grade(&mut score, question, 5));
        assert_eq!(score.right, 1);
        assert_eq!(score.wrong, 1);
        assert_eq!(score.percent(), 50);
    }

    #[test]
    fn a_wrong_answer_charges_the_right_operand_and_the_side_that_was_solved_for() {
        // The pairing is not symmetric: `problem()` draws the answer first for `-` and `/`
        // (that is what it solves for), so those are the values charged, while `+` and `x`
        // draw and charge the left operand (`arithmetic.c:198-227`, `:255-262`).
        assert_eq!(
            charged_operands(Operator::Add, 3, 4, 7),
            [(0, 1, 4), (0, 0, 3)]
        );
        assert_eq!(
            charged_operands(Operator::Subtract, 9, 4, 5),
            [(1, 1, 4), (1, 0, 5)]
        );
        assert_eq!(
            charged_operands(Operator::Multiply, 3, 4, 12),
            [(2, 1, 4), (2, 0, 3)]
        );
        assert_eq!(
            charged_operands(Operator::Divide, 17, 5, 3),
            [(3, 1, 5), (3, 0, 3)]
        );
        assert_eq!(slot_of(Operator::Add), 0);
        assert_eq!(slot_of(Operator::Divide), 3);
    }

    #[test]
    fn penalties_charge_five_and_pay_off_in_insertion_order() {
        // Head insertion (`:311-314`) means the most recent number is the first one the
        // walk meets, and each position consumes one penalty from the entry it lands on.
        let mut list = PenaltyList::default();
        assert_eq!(list.draw(10, 3), Drawn::Direct(3));
        list.add(7);
        assert_eq!((list.total(), list.len()), (5, 1));
        assert_eq!(list.draw(10, 10), Drawn::Penalised(7));
        assert_eq!(
            (list.total(), list.len()),
            (4, 1),
            "the entry survives with four left"
        );
        list.add(9);
        // Newest first, so position 0 now belongs to 9 and 7 moves one step back.
        assert_eq!(list.draw(10, 10), Drawn::Penalised(9));
        assert_eq!(list.draw(10, 14), Drawn::Penalised(7));
    }

    #[test]
    fn an_entry_dies_when_its_penalty_is_paid_out() {
        let mut list = PenaltyList::default();
        list.add(4);
        // A take consumes one penalty from the entry it lands on, so the same position is
        // offered `WRONG_PENALTY` times before the entry disappears: rolls at or above the
        // range are the ones that reach the list.
        for roll in 0..WRONG_PENALTY {
            assert_eq!(list.draw(1, 1), Drawn::Penalised(4), "take {roll}");
            let _ = roll;
        }
        assert_eq!(list.draw(1, 1), Drawn::Inconsistent, "the entry is gone");
        assert!(list.is_empty(), "five takes and the entry is gone");
        assert_eq!(list.total(), 0);
    }

    #[test]
    fn a_roll_past_the_end_of_the_list_is_the_inconsistency_c_reports() {
        let mut list = PenaltyList::default();
        list.add(3);
        assert_eq!(list.draw(10, 15), Drawn::Inconsistent);
    }

    #[test]
    fn a_full_list_drops_the_new_penalty_as_c_drops_a_failed_malloc() {
        let mut list = PenaltyList::default();
        for value in 0..PENALTY_CAPACITY as u32 {
            list.add(value);
        }
        let before = (list.total(), list.len());
        list.add(999);
        assert_eq!((list.total(), list.len()), before, "no room, no penalty");
    }

    #[test]
    fn the_draw_spans_the_range_plus_the_live_penalties() {
        let mut penalties = Penalties::default();
        penalties.add(0, 1, 6);
        // The modulo happens over `maxval + total`, so a roll inside the penalty band lands
        // on the charged value even though it is above the range.
        assert_eq!(penalties.draw(0, 1, 10, 10), Drawn::Penalised(6));
        assert_eq!(penalties.draw(2, 0, 10, 3), Drawn::Direct(3));
        assert_eq!(
            penalties.draw(3, 1, 0, 7),
            Drawn::Inconsistent,
            "zero range with nothing charged"
        );
    }

    #[test]
    fn test_empty_score_zero() {
        assert_eq!(Score::default().percent(), 0);
    }

    #[test]
    fn elapsed_seconds_are_the_difference_of_truncated_stamps() {
        // C stamps twice with `time(2)` and subtracts (`arithmetic.c:271-272`), so partial
        // seconds at each end do not cancel within one question: 0.9 s → 2.1 s charges two.
        assert_eq!(elapsed_secs(900_000, 2_100_000), 2);
        assert_eq!(
            elapsed_secs(0, 999_999),
            0,
            "under a second is charged nothing"
        );
        assert_eq!(elapsed_secs(1_000_000, 1_000_000), 0, "same second");
        assert_eq!(
            elapsed_secs(5_000_000, 4_000_000),
            0,
            "a clock going backwards cannot subtract below zero"
        );
    }

    #[test]
    fn the_seconds_per_problem_table_is_what_c_prints() {
        // Every expected value below was read off a host-compiled reference program that
        // reproduces `arithmetic.c:172-173` verbatim — `printf("Total time %ld seconds; %.1f
        // seconds per problem", (long)q, (float)q / n)` — not hand-computed.
        let cases: &[(u64, u32, u64, u8)] = &[
            (40, 20, 2, 0),
            (1, 3, 0, 3),
            (2, 3, 0, 7),
            (0, 5, 0, 0),
            (1, 7, 0, 1),
            (2, 7, 0, 3),
            (1, 6, 0, 2),
            (1, 8, 0, 1),
            (100, 3, 33, 3),
            (1_000_000, 7, 142_857, 1),
            (599, 7, 85, 6),
            (12_345_678, 3, 4_115_226, 0),
            (1, 25, 0, 0),
            (1, 250, 0, 0),
            (1_000_000, 1_048_576, 1, 0),
            (1_048_575, 1_048_576, 1, 0),
        ];
        for &(q, n, whole, tenth) in cases {
            assert_eq!(
                seconds_per_problem(q, n),
                Some(OneDecimal { whole, tenth }),
                "qtime={q} right={n}"
            );
        }
        assert_eq!(
            seconds_per_problem(5, 0),
            None,
            "C guards the divisor (`:171`)"
        );
    }

    #[test]
    fn exact_ties_round_to_the_even_digit_and_a_float_quotient_differs_from_a_double_one() {
        // The two discriminating families. Ties: `x.25`/`x.75` at one decimal are exactly
        // representable, and `gdtoa.c:782-790` keeps the even digit — `5/4` is `1.2` while
        // `3/4`, `7/4` and `11/4` are `0.8`, `1.8` and `2.8`.
        assert_eq!(
            seconds_per_problem(5, 4),
            Some(OneDecimal { whole: 1, tenth: 2 })
        );
        assert_eq!(
            seconds_per_problem(3, 4),
            Some(OneDecimal { whole: 0, tenth: 8 })
        );
        assert_eq!(
            seconds_per_problem(7, 4),
            Some(OneDecimal { whole: 1, tenth: 8 })
        );
        assert_eq!(
            seconds_per_problem(11, 4),
            Some(OneDecimal { whole: 2, tenth: 8 })
        );
        assert_eq!(
            seconds_per_problem(15, 4),
            Some(OneDecimal { whole: 3, tenth: 8 })
        );
        // Not-ties-after-float-rounding: C divides in `float`, so `3/20` is
        // 0.15000000596… and prints `0.2`, while `1/20` is 0.05000000074… and prints `0.1`;
        // a `double` path would answer `0.1` for `3/20`, which is wrong against C.
        assert_eq!(
            seconds_per_problem(3, 20),
            Some(OneDecimal { whole: 0, tenth: 2 })
        );
        assert_eq!(
            seconds_per_problem(1, 20),
            Some(OneDecimal { whole: 0, tenth: 1 })
        );
        assert_eq!(
            seconds_per_problem(7, 20),
            Some(OneDecimal { whole: 0, tenth: 3 })
        );
    }

    #[test]
    fn an_untouched_quiz_prints_no_score_block() {
        // `showstats` opens with `if (nright + nwrong > 0)` (`arithmetic.c:168`).
        assert!(!shows_score(Score::default()));
        assert!(shows_score(Score { right: 0, wrong: 1 }));
        assert!(shows_score(Score { right: 1, wrong: 0 }));
    }
}
