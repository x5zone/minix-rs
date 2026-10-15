//! Rotation cipher.
//!
//! Ground truth: `minix3/games/caesar/caesar.c`. The rotation table builder is
//! `init_rottbl` (`:81-97`): the rotation folds with `rot %= LETTERS` (`:86`, the
//! overflow guard), non-letters keep their own entry of the identity table, and upper
//! and lower case shift inside their own ranges. Rotation parsing is `get_rotation`
//! (`:120-137`). The argument-free mode is `guess_and_rotate` (`:141-187`), whose
//! deciding half lives here too: count letters, dot them against a *log-weighted* copy of
//! the published frequency table for each of the 26 rotations, and take the best.
//!
//! Two details of the cracker are load-bearing. The table is transformed once, before
//! any scoring, by `stdf[i] = log(stdf[i]) + log(LETTERS / 100.0)` (`:150-151`) — the
//! log is what makes rare letters dominate the score, and it is why the values below are
//! negative for the common letters. And the winner test is `dot > winnerdot` with the
//! first rotation seeded by `try == 0` (`:176-180`), so a tie always keeps the earlier
//! rotation rather than the later one.
//!
//! Because `log` is not available in the freestanding build, the weighted table is
//! stored as the constant it evaluates to; a test in this module recomputes every entry
//! from `STD_PERCENT` under `std` and compares.

use minix_sys::{EINVAL, ERANGE, Errno};

/// Letters in the Latin alphabet.
pub const LETTERS: u32 = 26;

/// Why C's `get_rotation` refuses a value: the two `errno`s it sets (`caesar.c:133`/`:135`).
///
/// This is deliberately not the crate's `GameError`: that vocabulary collapses "malformed"
/// and "out of range" into one arm each way (22 and 34), while C keeps them apart because
/// the suffix of its message differs. Each arm names the `errno` C sets, so the sentence is
/// finished by [`Errno::message`] — the table this crate now has.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RotationFailure {
    /// `arg[0] == '\0' || *endp != '\0'`: nothing to parse, or trailing junk.
    Invalid,
    /// `rot < 0 || rot > INT_MAX`, or `strtol` itself overflowed.
    Range,
}

impl RotationFailure {
    /// The `errno` C's `get_rotation` sets before it calls `err(3)` (`caesar.c:133/:135`):
    /// `EINVAL` for a value that will not parse, `ERANGE` for one that parses but is
    /// refused. The message suffix is then C's own — not a wording invented here.
    pub const fn errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::Range => Errno::from_i32(ERANGE),
        }
    }
}

/// C's `get_rotation` (`caesar.c:124-139`), acceptance rules and all.
///
/// 这里最容易搬错的一件事：`caesar` 的 `EINVAL` 是**它自己合成**的（`caesar.c:131-133`
/// 在 `errno == 0` 且「空串或有尾字符」时手动置起），不依赖 `strtol` 在无转换时是否置号；
/// 而 `primes` 走的是「只看 `strtoumax` 留下的 errno」那条路，真源在无转换时不置号
/// （`_strtol.h:184-186` 只把 `endptr` 留在原处），所以同一串 `abc` 在两个命令里分属两句。
/// 别把 `primes` 的结论搬过来「修」这里，反之亦然。
///
/// The parsing is `strtol`, which is more generous than "digits only" in two ways a
/// caller cannot see from the outside: it **skips leading white space** and it takes an
/// optional sign. A minus sign survives parsing and is then rejected by
/// `rot < 0 || rot > INT_MAX` — as `ERANGE`, not as malformed input — while an empty
/// argument or trailing junk is `EINVAL`; `err` fires only when `errno` was still clear,
/// so which of the two tests tripped decides the suffix C prints. The failure carries
/// that distinction rather than flattening it.
///
/// A value that survives is non-negative and at most `INT_MAX`, and `init_rottbl` folds
/// it with `% LETTERS` (`:86`), so the returned number is kept as parsed rather than
/// folded here.
pub fn parse_rotation(word: &str) -> Result<u32, RotationFailure> {
    let bytes = word.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    // `strtol` consumes either sign; only the minus one is later refused, and as a range
    // failure rather than malformed input.
    let negative = index < bytes.len() && bytes[index] == b'-';
    if index < bytes.len() && matches!(bytes[index], b'-' | b'+') {
        index += 1;
    }
    let mut value: u64 = 0;
    let mut digits = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        let Some(scaled) = value.checked_mul(10) else {
            return Err(RotationFailure::Range);
        };
        let Some(summed) = scaled.checked_add((bytes[index] - b'0') as u64) else {
            return Err(RotationFailure::Range);
        };
        value = summed;
        digits += 1;
        index += 1;
    }
    // `errno == 0 && (arg[0] == '\0' || *endp != '\0')`: an empty argument, or anything
    // left over after the digits, is the invalid case — including a lone sign, which
    // `strtol` reports with nothing consumed.
    if digits == 0 || index != bytes.len() {
        return Err(RotationFailure::Invalid);
    }
    if negative || value > i32::MAX as u64 {
        return Err(RotationFailure::Range);
    }
    Ok(value as u32)
}

/// Rotate one byte by `rot` positions (upper and lower case wrap within
/// their ranges, every other byte passes through unchanged).
pub fn rotate_byte(byte: u8, rot: u32) -> u8 {
    let shift = (rot % LETTERS) as u8;
    if byte.is_ascii_uppercase() {
        b'A' + (byte - b'A' + shift) % 26
    } else if byte.is_ascii_lowercase() {
        b'a' + (byte - b'a' + shift) % 26
    } else {
        byte
    }
}

/// Letters counted by the cracker, same `NCHARS` width as C's `obs` (`caesar.c:62,145`).
pub const BYTE_VALUES: usize = 256;

/// Bytes of input the cracker looks at: C's `unsigned char inbuf[2048]` (`:144`).
/// Everything past this window is still rotated, it simply does not vote.
pub const GUESS_WINDOW: usize = 2048;

/// Published letter frequencies in percent (`stdf` before the adjustment, `:71-76`).
pub const STD_PERCENT: [f64; 26] = [
    7.97, 1.35, 3.61, 4.78, 12.37, 2.01, 1.46, 4.49, 6.39, 0.04, 0.42, 3.81, 2.69, 5.92, 6.96,
    2.91, 0.08, 6.63, 8.77, 9.68, 2.62, 0.81, 1.88, 0.23, 2.07, 0.06,
];

/// `STD_PERCENT` after C's one adjustment, `log(p) + log(26 / 100)` (`:150-151`).
/// Stored because `log` needs the host library; the test below derives it again.
pub const WEIGHTED_PERCENT: [f64; 26] = [
    0.728_610_844_835_514_6,
    -1.046_969_055_516_271,
    -0.063_365_875_621_819_61,
    0.217_366_898_536_755_4,
    1.168_200_538_437_787_4,
    -0.648_938_925_895_625,
    -0.968_637_212_246_364_2,
    0.154_779_053_787_553_74,
    0.507_660_620_422_834_2,
    -4.565_949_472_834_809,
    -2.214_574_215_671_332,
    -0.009_444_458_827_999_602,
    -0.357_532_454_352_861_5,
    0.431_262_800_929_305_13,
    0.593_105_826_379_719_1,
    -0.278_920_566_783_208,
    -3.872_802_292_274_865,
    0.544_531_156_231_161_9,
    0.824_263_158_417_482_4,
    0.922_988_253_321_876_5,
    -0.383_899_330_193_603_67,
    -1.557_794_679_282_261_7,
    -0.715_801_871_124_751_5,
    -2.816_749_618_025_551,
    -0.619_525_040_689_331_6,
    -4.160_484_364_726_646,
];

/// Per-byte observation counts, the shape of C's `obs[NCHARS]` (`:145`).
pub type Observations = [u32; BYTE_VALUES];

/// Counts the bytes of `input` the way C does before scoring (`:161-164`).
pub fn observe(input: &[u8]) -> Observations {
    let mut obs = [0u32; BYTE_VALUES];
    for &byte in input {
        obs[byte as usize] = obs[byte as usize].saturating_add(1);
    }
    obs
}

/// C's scoring loop (`caesar.c:167-182`): the rotation whose dot product wins.
///
/// For each candidate the observed counts of a letter's two cases are folded together
/// (`obs[upper[i]] + obs[lower[i]]`) and multiplied by the weighted table *shifted* by the
/// candidate — `(i + try) % 26`, which is how a rotation to the right is detected. The
/// comparison is strict and `try == 0` seeds the winner, so equal scores keep the earlier
/// rotation. C would wrap its `unsigned int` counters after four billion of one byte;
/// `saturating_add` in [`observe`] can only differ there, far outside the 2048-byte window.
pub fn best_rotation(obs: &Observations) -> u32 {
    let mut winner = 0u32;
    let mut winner_dot = 0.0f64;
    for try_rotation in 0..LETTERS {
        let mut dot = 0.0f64;
        for index in 0..LETTERS as usize {
            let observed = obs[b'A' as usize + index].saturating_add(obs[b'a' as usize + index]);
            dot += (observed as f64) * WEIGHTED_PERCENT[(index + try_rotation as usize) % 26];
        }
        if try_rotation == 0 || dot > winner_dot {
            winner = try_rotation;
            winner_dot = dot;
        }
    }
    winner
}

/// Both halves of `guess_and_rotate` in one call: score the first
/// [`GUESS_WINDOW`] bytes and hand back the rotation that wins (`:156-186`).
pub fn guess_rotation(input: &[u8]) -> u32 {
    best_rotation(&observe(&input[..input.len().min(GUESS_WINDOW)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotated(text: &str, rot: u32) -> String {
        String::from_utf8(text.bytes().map(|byte| rotate_byte(byte, rot)).collect()).unwrap()
    }

    #[test]
    fn the_weighted_table_is_the_log_form_of_the_published_frequencies() {
        // C folds the published percentages once, before any scoring:
        // `stdf[i] = log(stdf[i]) + log(LETTERS / 100.0)` (`caesar.c:150-151`). The
        // constants above have to be exactly that, so they are recomputed here — `ln` is
        // available because the test build links std.
        let bias = (26.0f64 / 100.0).ln();
        let source = [
            7.97, 1.35, 3.61, 4.78, 12.37, 2.01, 1.46, 4.49, 6.39, 0.04, 0.42, 3.81, 2.69, 5.92,
            6.96, 2.91, 0.08, 6.63, 8.77, 9.68, 2.62, 0.81, 1.88, 0.23, 2.07, 0.06,
        ];
        assert_eq!(STD_PERCENT, source);
        for index in 0..26 {
            let expected = source[index].ln() + bias;
            assert!(
                (WEIGHTED_PERCENT[index] - expected).abs() < 1e-12,
                "letter {index}: baked {:?}, formula {:?}",
                WEIGHTED_PERCENT[index],
                expected
            );
        }
        // The peak of the weighted table is still `e` and its floor is still `j`, which is
        // what the scoring loop searches for; derived here rather than asserted against
        // fixed subscripts so a mistyped constant above cannot pass by matching itself.
        let peak = (0..26).max_by(|a, b| {
            WEIGHTED_PERCENT[*a]
                .partial_cmp(&WEIGHTED_PERCENT[*b])
                .unwrap()
        });
        let floor = (0..26).min_by(|a, b| {
            WEIGHTED_PERCENT[*a]
                .partial_cmp(&WEIGHTED_PERCENT[*b])
                .unwrap()
        });
        assert_eq!(peak, Some(4), "the highest log weight belongs to e");
        assert_eq!(floor, Some(9), "the lowest belongs to j");
    }

    #[test]
    fn the_log_step_changes_the_answer_on_a_thin_alphabet() {
        // The adjustment at `caesar.c:150-151` is not decoration: with only four letters
        // observed, this particular mix is where the two tables disagree. Scoring the raw
        // percentages would answer rotation 2, scoring the logarithms answers 16 — and 16
        // is what C prints, because `j`-weight and `q`-weight have gone strongly negative
        // and dominate the sum instead of vanishing next to `e`'s 12.37.
        let mut text = String::new();
        for (letter, count) in [('c', 29), ('s', 17), ('y', 32), ('z', 8)] {
            for _ in 0..count {
                text.push(letter);
            }
        }
        let obs = observe(text.as_bytes());
        assert_eq!(best_rotation(&obs), 16);
        assert_eq!(guess_rotation(text.as_bytes()), 16);
    }

    #[test]
    fn the_cracker_finds_the_shift_that_was_applied() {
        // English prose scores zero, and rotating it by any amount makes the cracker answer
        // the complement: `(i + try) % 26` has to walk the observed letter back onto its
        // standard index, so a shift of 3 is answered by 23 and the rotation then restores
        // the plaintext (a shift of 13 answers 13, which is why rot13 looks its own mirror).
        let plain = "the quick brown fox jumps over the lazy dog, and the                      frequency of letters in english prose is what this program counts";
        assert_eq!(guess_rotation(plain.as_bytes()), 0);
        for shift in [3u32, 13, 25] {
            let cipher: Vec<u8> = plain.bytes().map(|byte| rotate_byte(byte, shift)).collect();
            let winner = guess_rotation(&cipher);
            assert_eq!(winner, (26 - shift) % 26, "shift {shift}");
            let restored: Vec<u8> = cipher
                .iter()
                .map(|&byte| rotate_byte(byte, winner))
                .collect();
            assert_eq!(restored, plain.as_bytes(), "the winner must undo the shift");
        }
    }

    #[test]
    fn a_single_letter_observation_picks_the_shift_that_puts_it_on_the_peak() {
        // With everything on one letter, `dot(try)` is a multiple of one table entry, so
        // the winner is exactly the `try` that lands that letter on index 4 — the peak of
        // the weighted table, `e` at 12.37%. This pins the direction of the shift, which
        // is the part a reader is most likely to get backwards.
        let mut obs = [0u32; BYTE_VALUES];
        obs[b'A' as usize] = 40;
        assert_eq!(best_rotation(&obs), 4);
        let mut obs = [0u32; BYTE_VALUES];
        obs[b'e' as usize] = 40;
        assert_eq!(best_rotation(&obs), 0);
        let mut obs = [0u32; BYTE_VALUES];
        obs[b'B' as usize] = 40;
        assert_eq!(best_rotation(&obs), 3);
        // Uniform counts really are a tie in exact arithmetic but not in floating point:
        // the 26 candidates sum the same 26 terms in different orders, and the strict `>`
        // then keeps whichever the summation put first. C is in the same position, adding
        // over `i` in the same order, so the outcome is reproduced rather than fixed here.
        let mut uniform = [0u32; BYTE_VALUES];
        for index in 0..26 {
            uniform[b'A' as usize + index] = 7;
            uniform[b'a' as usize + index] = 7;
        }
        assert!(best_rotation(&uniform) < LETTERS);
        // Past `GUESS_WINDOW` the bytes no longer count, which is observable when the two
        // halves want different rotations.
        let english = "the quick brown fox jumps over the lazy dog, and the                        frequency of letters in english prose is what this program counts";
        let mut input = english.as_bytes().repeat(GUESS_WINDOW / english.len() + 1);
        input.truncate(GUESS_WINDOW);
        let noise = english
            .bytes()
            .map(|byte| rotate_byte(byte, 5))
            .collect::<Vec<u8>>();
        input.extend_from_slice(&noise);
        assert_eq!(
            guess_rotation(&input),
            0,
            "the tail beyond the window does not vote"
        );
    }

    #[test]
    fn test_rot_thirteen_round_trip() {
        assert_eq!(rotated("Hello, World!", 13), "Uryyb, Jbeyq!");
        assert_eq!(rotated("Uryyb, Jbeyq!", 13), "Hello, World!");
    }

    #[test]
    fn test_zero_and_full_turns() {
        assert_eq!(rotated("Abc", 0), "Abc");
        assert_eq!(rotated("Abc", 26), "Abc");
        assert_eq!(rotated("Abc", 52), "Abc");
    }

    #[test]
    fn test_non_letters_pass_through() {
        assert_eq!(rotated("123 !?", 5), "123 !?");
    }

    #[test]
    fn rotations_parse_the_way_strtol_does_and_then_reject_by_range() {
        // C's acceptance comes from `strtol` (`caesar.c:131`): leading white space is
        // skipped and a sign is consumed. The rejections then come from the two explicit
        // tests (`:132-135`), and they land on different errnos: nothing-or-junk is the
        // invalid case, a sign that survived parsing or a value past `INT_MAX` is the
        // range case. So `-1` is not "malformed", it is out of range.
        assert_eq!(parse_rotation("13"), Ok(13));
        assert_eq!(parse_rotation("0"), Ok(0));
        assert_eq!(parse_rotation("+5"), Ok(5), "a plus sign is consumed");
        assert_eq!(parse_rotation(" 7"), Ok(7), "leading blanks are skipped");
        assert_eq!(parse_rotation("\t9"), Ok(9));
        assert_eq!(
            parse_rotation("2147483647"),
            Ok(2147483647),
            "INT_MAX is allowed"
        );
        assert_eq!(parse_rotation(""), Err(RotationFailure::Invalid));
        assert_eq!(parse_rotation("5x"), Err(RotationFailure::Invalid));
        assert_eq!(
            parse_rotation("-"),
            Err(RotationFailure::Invalid),
            "no digits at all"
        );
        assert_eq!(parse_rotation(" 5x"), Err(RotationFailure::Invalid));
        assert_eq!(parse_rotation("-1"), Err(RotationFailure::Range));
        assert_eq!(parse_rotation("2147483648"), Err(RotationFailure::Range));
        assert_eq!(parse_rotation("99999999999"), Err(RotationFailure::Range));
        assert_eq!(
            parse_rotation("99999999999999999999999"),
            Err(RotationFailure::Range),
            "the overflow is caught while scanning, as ERANGE is set by strtol"
        );
    }

    /// 两个失败各自缀哪一句，由 Minix3 自己的 `sys_errlist` 决定（本树那份表在
    /// `minix-types`）：`EINVAL` 是 `Invalid argument`，`ERANGE` 是
    /// `Result too large or too small`。后者是这张表与 glibc 的差别之一——照抄 glibc 会
    /// 写成 `Numerical result out of range`，而 Minix3 的 `errno.h:78` 注释本身就写
    /// `Result too large or too small`。
    #[test]
    fn each_failure_carries_the_errno_c_sets_and_minix3_words_it() {
        assert_eq!(
            RotationFailure::Invalid.errno().message(),
            Some("Invalid argument")
        );
        assert_eq!(
            RotationFailure::Range.errno().message(),
            Some("Result too large or too small")
        );
        // 标识符面与句子面各归各：`err(3)` 缀的是句子。
        assert_eq!(RotationFailure::Invalid.errno().name(), Some("EINVAL"));
        assert_eq!(RotationFailure::Range.errno().to_i32(), ERANGE);
    }
}
