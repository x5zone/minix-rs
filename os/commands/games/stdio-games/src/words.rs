//! Word play: pig latin and number words.
//!
//! Ground truth: `minix3/games/pig/pig.c` — `pigout()` (`:94-130`) rotates one accumulated
//! word and `main()` (`:62-92`) decides what a word is — and
//! `minix3/games/number/number.c` (arabic numbers become English words). This module owns
//! both conversions; the byte-at-a-time stream stays with the execution layer.
//!
//! Three details of `pigout()` are what a first reading usually gets wrong.
//!
//! * Two different vowel sets. The test that decides whether the word merely gains `way`
//!   (`:103`) looks for `aeiouAEIOU` — **no `y`** — while the consonant-cluster loop (`:114`)
//!   stops at `aeiouyAEIOUY` and tests `buf[start]` from `start == 0` onward. So `yellow`
//!   never rotates: the loop's very first test matches its `y`, and the word only gains `ay`
//!   (`yelloway`); `yes` is `yesay`.
//! * The unit `qu` moves together (`:117-119`), so `quay` is `ayquay` rather than `uayqay`.
//! * Case is moved, not normalized. Only `buf[0]` is lower-cased, and only when the word is
//!   not all capitals (`:111-112`); only the new first character is upper-cased when the word
//!   started with a capital (`:124-125`). Everything else keeps the case it had, so `Pig`
//!   becomes `Igpay` and the mixed `sTring` becomes `ingsTray` — the cluster that lands at
//!   the end keeps its interior capitals.
//!
//! Whether a word is all capitals is decided by every character of it (`:98-100`), and then
//! the suffix is capital too (`WAY`/`AY`).

use crate::GameError;

/// The five vowels in both cases — the set that earns a `way` suffix (`pig.c:103`).
fn is_vowel(byte: &u8) -> bool {
    matches!(
        byte,
        b'a' | b'e' | b'i' | b'o' | b'u' | b'A' | b'E' | b'I' | b'O' | b'U'
    )
}

/// The same five plus `y` — the set that stops the consonant-cluster loop (`pig.c:114`).
fn is_stop(byte: &u8) -> bool {
    is_vowel(byte) || matches!(byte, b'y' | b'Y')
}

/// C's `pigout()` (`pig.c:94-130`) for one word made of alphabetic bytes, written to `out`.
///
/// An empty word is `InvalidArgument`, which C never hands it because `main()` only calls
/// `pigout()` when a word is pending (`:84-87`). A buffer smaller than the word plus the
/// three-letter suffix is `OutOfRange`; C instead grows into `buf[1024]` and answers
/// `ate too much!` (`:78-79`), and reporting that is the execution layer's job.
pub fn pig_word(word: &str, out: &mut [u8]) -> Result<usize, GameError> {
    let bytes = word.as_bytes();
    if bytes.is_empty() {
        return Err(GameError::InvalidArgument);
    }
    if out.len() < bytes.len() + 3 {
        return Err(GameError::OutOfRange);
    }
    let first_upper = bytes[0].is_ascii_uppercase();
    let all_upper = bytes.iter().all(|byte| byte.is_ascii_uppercase());
    // `pig.c:103-108`: a vowel start keeps the whole word and only gains the suffix.
    if is_vowel(&bytes[0]) {
        let suffix: &[u8] = if all_upper { b"WAY" } else { b"way" };
        let end = bytes.len() + suffix.len();
        out[..bytes.len()].copy_from_slice(bytes);
        out[bytes.len()..end].copy_from_slice(suffix);
        return Ok(end);
    }
    let suffix: &[u8] = if all_upper { b"AY" } else { b"ay" };
    // Only the first byte is lower-cased, and only when the word is not all capitals.
    let lowered_first = if all_upper {
        bytes[0]
    } else {
        bytes[0].to_ascii_lowercase()
    };
    // The cluster length `start`, walking C's loop including the `qu` unit (`:114-121`).
    let mut start = 0usize;
    while start < bytes.len() && !is_stop(&bytes[start]) {
        let moved = if start == 0 {
            lowered_first
        } else {
            bytes[start]
        };
        start += 1;
        if (moved == b'q' || moved == b'Q')
            && start < bytes.len()
            && matches!(bytes[start], b'u' | b'U')
        {
            start += 1;
        }
    }
    // C prints `%.*s` of `buf + start` for `olen` characters: the untouched tail of the word
    // and then the appended copies of the moved cluster (`:126-127`).
    let mut written = 0usize;
    for offset in 0..bytes.len() {
        let index = start + offset;
        let byte = if index < bytes.len() {
            if index == 0 {
                lowered_first
            } else {
                bytes[index]
            }
        } else {
            let source = index - bytes.len();
            if source == 0 {
                lowered_first
            } else {
                bytes[source]
            }
        };
        out[written] = byte;
        written += 1;
    }
    // `if (firstupper) buf[start] = toupper(buf[start])` — the character printed first.
    if first_upper {
        out[0] = out[0].to_ascii_uppercase();
    }
    out[written..written + suffix.len()].copy_from_slice(suffix);
    written += suffix.len();
    Ok(written)
}

/// C's three word tables (`number.c:57-73`): `name1` covers 0 through 19 (index 0 is the
/// empty string, so a zero group speaks nothing), `name2` the tens decades, and `name3`
/// the scale names by group of three digits — 22 of them, which is why the ceiling is
/// `MAXNUM` = 65 digits.
pub const NAME1: &[&str] = &[
    "",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
pub const NAME2: &[&str] = &[
    "", "ten", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
pub const NAME3: &[&str] = &[
    "hundred",
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion",
    "sextillion",
    "septillion",
    "octillion",
    "nonillion",
    "decillion",
    "undecillion",
    "duodecillion",
    "tredecillion",
    "quattuordecillion",
    "quindecillion",
    "sexdecillion",
    "septendecillion",
    "octodecillion",
    "novemdecillion",
    "vigintillion",
];

/// Prefixes for the fraction name, indexed by digits modulo three (`number.c:287`).
const FRACTION_PREFIX: &[&str] = &["", "ten-", "hundred-"];

/// Biggest number C handles, in digits, on either side of the point (`MAXNUM`, `:53`).
pub const MAXNUM: usize = 65;

/// Why C's `convert` refuses a line (`number.c:128-158`).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum NumberError {
    /// A blank inside the number, a `-` that is not leading, or a second `.`. C prints
    /// `illegal number: ` plus whatever its `line` still holds: from the current start
    /// (leading blanks are consumed one by one) up to the `.` it already replaced with a
    /// terminator, if any — so `1.2 3` reports `1`, not the whole line.
    Illegal { start: usize, end: usize },
    /// More than `MAXNUM` digits on either side of the point (`:161-163`).
    TooLarge,
    /// The output buffer was too small. Legal input needs a few hundred bytes at most, so
    /// this is a plumbing guard rather than a C behaviour: C's `printf` has no such limit.
    NoRoom,
}

/// The accumulating output plus the `-l` face of every separator C prints.
struct Speaker<'a> {
    out: &'a mut [u8],
    written: usize,
    linear: bool,
    failed: bool,
}

impl<'a> Speaker<'a> {
    fn put(&mut self, text: &str) {
        let bytes = text.as_bytes();
        if self.written + bytes.len() > self.out.len() {
            self.failed = true;
            return;
        }
        self.out[self.written..self.written + bytes.len()].copy_from_slice(bytes);
        self.written += bytes.len();
    }

    /// C's `lflag ? " " : ".\n"`, the tail a scale group ends with (`:229-230`, `:244-246`).
    fn end_group(&mut self) {
        if self.linear {
            self.put(" ");
        } else {
            self.put(".\n");
        }
    }
}

/// C's `number()` (`:252-284`): speak at most three digits of `bytes`, and say whether
/// anything was spoken. A three-digit group whose hundreds digit is zero prints nothing
/// for that digit and falls through — so `105` is `one hundred five`, and `007` is `seven`.
fn speak_group(bytes: &[u8], len: usize, s: &mut Speaker) -> bool {
    let mut rval = false;
    let mut offset = 0usize;
    if len == 3 {
        if bytes[0] != b'0' {
            rval = true;
            s.put(NAME1[(bytes[0] - b'0') as usize]);
            s.put(" hundred");
        }
        offset = 1;
    }
    let rest = len - offset;
    if rest == 2 {
        let val = (bytes[offset + 1] - b'0') as usize + (bytes[offset] - b'0') as usize * 10;
        if val != 0 {
            if rval {
                s.put(" ");
            }
            if val < 20 {
                s.put(NAME1[val]);
            } else {
                s.put(NAME2[val / 10]);
                if !val.is_multiple_of(10) {
                    s.put("-");
                    s.put(NAME1[val % 10]);
                }
            }
            rval = true;
        }
    } else if rest == 1 && bytes[offset] != b'0' {
        rval = true;
        s.put(NAME1[(bytes[offset] - b'0') as usize]);
    }
    rval
}

/// C's `unit()` (`:215-249`): speak a group of up to `MAXNUM` digits with the scale names
/// between them. The scale index is the number of *remaining* groups of three, which is
/// why `1000` is `one thousand` and not `one thousand zero`.
fn speak_units(bytes: &[u8], mut len: usize, s: &mut Speaker) -> bool {
    let mut p = bytes;
    let mut rval = false;
    if len > 3 {
        if !len.is_multiple_of(3) {
            let off = len % 3;
            len -= off;
            if speak_group(p, off, s) {
                rval = true;
                s.put(" ");
                s.put(NAME3[len / 3]);
                s.end_group();
            }
            p = &p[off..];
        }
        while len > 3 {
            len -= 3;
            if speak_group(p, 3, s) {
                rval = true;
                s.put(" ");
                s.put(NAME3[len / 3]);
                s.end_group();
            }
            p = &p[3.min(p.len())..];
        }
    }
    if speak_group(p, len, s) {
        if !s.linear {
            s.put(".\n");
        }
        rval = true;
    }
    rval
}

/// C's `pfract()` (`:286-298`): name the place value of a fraction. One digit is tenths,
/// two is hundredths, and beyond that the prefix table and the scale names compose — and
/// like every other group in C it ends with a period, which `-l` does not suppress here.
fn speak_fraction_name(len: usize, s: &mut Speaker) {
    match len {
        1 => s.put("tenths.\n"),
        2 => s.put("hundredths.\n"),
        _ => {
            s.put(FRACTION_PREFIX[len % 3]);
            s.put(NAME3[len / 3]);
            s.put("ths.\n");
        }
    }
}

/// C's `convert()` (`number.c:112-212`) for one line or one operand.
///
/// The scan is the surprising part and it is deliberately unforgiving: a blank only counts
/// as leading while it sits at the current start of the number, so ` number 5` fails after
/// the first token; a `-` is only a sign at the start; a second `.` is refused. Whatever is
/// printed on failure is C's business — this returns the offending index.
///
/// `linear` is C's `lflag` from `-l`, which swaps the period-and-newline separators for
/// spaces and moves the line's final newline to the end.
pub fn convert(line: &[u8], linear: bool, out: &mut [u8]) -> Result<usize, NumberError> {
    let mut start = 0usize;
    let mut index = 0usize;
    let mut dot: Option<usize> = None;
    let mut s = Speaker {
        out,
        written: 0,
        linear,
        failed: false,
    };
    while index < line.len() && line[index] != b'\n' {
        let byte = line[index];
        if byte == b' ' || byte == b'\t' {
            if index == start {
                start += 1;
                index += 1;
                continue;
            }
            return Err(NumberError::Illegal {
                start,
                end: dot.unwrap_or(index),
            });
        }
        if byte.is_ascii_digit() {
            index += 1;
            continue;
        }
        match byte {
            b'.' => match dot {
                // A second point is the refusal, and the first one already ended the text.
                Some(at) => return Err(NumberError::Illegal { start, end: at }),
                None => {
                    dot = Some(index);
                    index += 1;
                }
            },
            b'-' if index == start => index += 1,
            _ => {
                return Err(NumberError::Illegal {
                    start,
                    end: dot.unwrap_or(index),
                });
            }
        }
    }
    let integer_end = dot.unwrap_or(index);
    let mut integer = &line[start..integer_end];
    let fraction = match dot {
        Some(at) => &line[at + 1..index],
        None => &[] as &[u8],
    };
    if integer.len() > MAXNUM || (dot.is_some() && fraction.len() > MAXNUM) {
        return Err(NumberError::TooLarge);
    }

    let mut rval = false;
    if integer.first() == Some(&b'-') {
        s.put("minus");
        if linear {
            s.put(" ");
        } else {
            s.put("\n");
        }
        integer = &integer[1..];
    }
    if !integer.is_empty() {
        rval = speak_units(integer, integer.len(), &mut s);
    }
    if !fraction.is_empty() {
        for (offset, byte) in fraction.iter().enumerate() {
            if *byte != b'0' {
                if rval {
                    if linear {
                        s.put(" and ");
                    } else {
                        s.put("and\n");
                    }
                }
                let tail = &fraction[offset..];
                if speak_units(tail, tail.len(), &mut s) {
                    if linear {
                        s.put(" ");
                    }
                    speak_fraction_name(fraction.len(), &mut s);
                    rval = true;
                }
                break;
            }
        }
    }
    if !rval {
        s.put("zero");
        if !linear {
            s.put(".\n");
        }
    }
    if linear {
        s.put("\n");
    }
    if s.failed {
        return Err(NumberError::NoRoom);
    }
    Ok(s.written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pig(text: &str) -> String {
        let mut out = [0u8; 64];
        let len = pig_word(text, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    fn spoken(text: &str) -> String {
        spoken_flag(text, false)
    }

    fn spoken_flag(text: &str, linear: bool) -> String {
        let mut out = [0u8; 4096];
        let len = convert(text.as_bytes(), linear, &mut out).expect("a legal number fits");
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_vowel_start_gains_way() {
        assert_eq!(pig("apple"), "appleway");
    }

    #[test]
    fn test_consonant_cluster_moves() {
        assert_eq!(pig("string"), "ingstray");
        assert_eq!(
            pig("yellow"),
            "yelloway",
            "the loop stops at the leading `y`"
        );
    }

    #[test]
    fn a_leading_y_never_moves_because_the_stop_set_contains_it() {
        // `pig.c:114` tests `buf[start]` from `start == 0` against `aeiouyAEIOUY`, while the
        // `way` test at `:103` has no `y`: a word that opens with `y` is left whole and only
        // gains `ay`.
        assert_eq!(pig("yes"), "yesay");
        assert_eq!(pig("yam"), "yamay");
        assert_eq!(pig("Yellow"), "Yelloway");
        assert_eq!(
            pig("MY"),
            "YMAY",
            "the capital suffix rides the moved cluster"
        );
    }

    #[test]
    fn the_qu_unit_moves_together() {
        // `pig.c:117-119` copies the `u` right after a moved `q`, so the pair leaves as one
        // unit and the printed tail starts after it.
        assert_eq!(pig("quay"), "ayquay");
        assert_eq!(pig("quizzical"), "izzicalquay");
        // The pairing is tested per moved character, so in `QQUAKES` the second `q` takes the
        // `u` with it and the cluster that lands at the end is `QQU` (`:117-119`).
        assert_eq!(pig("QQUAKES"), "AKESQQUAY");
    }

    #[test]
    fn case_travels_with_the_letters_it_stands_on() {
        // Only `buf[0]` is lower-cased (`:111-112`) and only the new first character is
        // upper-cased (`:124-125`); the rest of the cluster keeps its own case, so an
        // interior capital survives the move instead of being flattened.
        assert_eq!(pig("Pig"), "Igpay");
        assert_eq!(pig("String"), "Ingstray");
        assert_eq!(pig("sTring"), "ingsTray");
        assert_eq!(pig("STRING"), "INGSTRAY");
    }

    #[test]
    fn test_capitalization_kept() {
        assert_eq!(pig("Apple"), "Appleway");
        assert_eq!(pig("APPLE"), "APPLEWAY");
    }

    #[test]
    fn each_scale_group_ends_with_a_period_of_its_own() {
        // C's `unit()` prints the scale name and then `lflag ? " " : ".\n"` per group
        // (`number.c:229-236`), so a four-group number arrives as several lines. The decades
        // join with a hyphen (`number()` at `:271-278`), not a space.
        assert_eq!(spoken("0"), "zero.\n");
        assert_eq!(spoken("7"), "seven.\n");
        assert_eq!(spoken("007"), "seven.\n", "leading zeros speak nothing");
        assert_eq!(spoken("105"), "one hundred five.\n");
        assert_eq!(spoken("42"), "forty-two.\n");
        assert_eq!(spoken("1000"), "one thousand.\n");
        assert_eq!(spoken("1234"), "one thousand.\ntwo hundred thirty-four.\n");
        assert_eq!(spoken("2000000"), "two million.\n");
        assert_eq!(
            spoken("123456789"),
            "one hundred twenty-three million.\n\
             four hundred fifty-six thousand.\n\
             seven hundred eighty-nine.\n"
        );
        assert_eq!(
            spoken("-42"),
            "minus\nforty-two.\n",
            "the sign gets its own line"
        );
        assert_eq!(spoken("  7"), "seven.\n", "leading blanks are consumed");
    }

    #[test]
    fn the_fraction_half_is_named_after_its_own_digit_count() {
        // C walks the fraction to its first non-zero digit, joins with `and`, and then names
        // the whole fraction by length (`:184-203`, `pfract` at `:286-298`).
        assert_eq!(
            spoken("1.0"),
            "one.\n",
            "an all-zero fraction speaks nothing"
        );
        assert_eq!(spoken("0.5"), "five.\ntenths.\n");
        assert_eq!(spoken("0.05"), "five.\nhundredths.\n");
        assert_eq!(spoken("1.25"), "one.\nand\ntwenty-five.\nhundredths.\n");
        assert_eq!(
            spoken("12.345"),
            "twelve.\nand\nthree hundred forty-five.\nthousandths.\n"
        );
    }

    #[test]
    fn linear_mode_trades_the_periods_for_spaces() {
        // `-l` replaces each group's `".\n"` with `" "` and ends the line with one newline
        // (`number.c:229-230`, `:244-246`, `:209-211`); the fraction name keeps its period.
        assert_eq!(spoken_flag("6", true), "six\n");
        assert_eq!(
            spoken_flag("1234", true),
            "one thousand two hundred thirty-four\n"
        );
        assert_eq!(spoken_flag("-42", true), "minus forty-two\n");
        assert_eq!(spoken_flag("0.5", true), "five tenths.\n\n");
        assert_eq!(
            spoken_flag("1.25", true),
            "one and twenty-five hundredths.\n\n"
        );
    }

    #[test]
    fn refused_numbers_name_the_text_c_would_still_print() {
        // `convert` refuses a blank inside the number, a `-` that is not leading, and a
        // second `.` (`number.c:138-153`); the payload is the slice C's `line` still holds.
        let mut out = [0u8; 64];
        assert_eq!(
            convert(b"1e5", false, &mut out),
            Err(NumberError::Illegal { start: 0, end: 1 })
        );
        assert_eq!(
            convert(b"1.2 3", false, &mut out),
            Err(NumberError::Illegal { start: 0, end: 1 })
        );
        assert_eq!(
            convert(b"1..2", false, &mut out),
            Err(NumberError::Illegal { start: 0, end: 1 })
        );
        assert_eq!(
            convert(b"1-2", false, &mut out),
            Err(NumberError::Illegal { start: 0, end: 1 })
        );
        // The leading blanks are consumed, so the start is the digit; the refusal happens at
        // the blank after it, and C's `line` still reaches only that far.
        assert_eq!(
            convert(b"  7 x", false, &mut out),
            Err(NumberError::Illegal { start: 2, end: 3 })
        );
        // and 65 digits on either side of the point is the ceiling (`:161-163`).
        assert_eq!(
            convert(format!("1{}", "0".repeat(65)).as_bytes(), false, &mut out),
            Err(NumberError::TooLarge)
        );
        assert_eq!(
            convert(format!("0.{}", "1".repeat(66)).as_bytes(), false, &mut out),
            Err(NumberError::TooLarge)
        );
    }

    #[test]
    fn test_bad_words_rejected() {
        let mut out = [0u8; 8];
        assert_eq!(pig_word("", &mut out), Err(GameError::InvalidArgument));
        assert_eq!(
            pig_word("averylongwordindeed", &mut out),
            Err(GameError::OutOfRange)
        );
    }
}
