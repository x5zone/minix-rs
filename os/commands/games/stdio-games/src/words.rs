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

/// Ones through nineteen.
const SMALL_WORDS: &[&str] = &[
    "zero",
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

/// Tens multiples from twenty to ninety.
const TENS_WORDS: &[&str] = &[
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// Write the English words for `value` (zero through 999999) into `out`.
pub fn number_words(mut value: u32, out: &mut [u8]) -> Result<usize, GameError> {
    if value > 999_999 {
        return Err(GameError::OutOfRange);
    }
    let mut written = 0;
    if value >= 1000 {
        let thousands = value / 1000;
        write_below_thousand(thousands, out, &mut written)?;
        put_word(out, &mut written, "thousand")?;
        value %= 1000;
        if value == 0 {
            return Ok(written);
        }
    }
    if value > 0 {
        write_below_thousand(value, out, &mut written)?;
    } else if written == 0 {
        put_word(out, &mut written, "zero")?;
    }
    Ok(written)
}

fn put_word(out: &mut [u8], written: &mut usize, word: &str) -> Result<(), GameError> {
    if *written > 0 {
        if *written >= out.len() {
            return Err(GameError::OutOfRange);
        }
        out[*written] = b' ';
        *written += 1;
    }
    if out.len() - *written < word.len() {
        return Err(GameError::OutOfRange);
    }
    out[*written..*written + word.len()].copy_from_slice(word.as_bytes());
    *written += word.len();
    Ok(())
}

fn write_below_thousand(value: u32, out: &mut [u8], written: &mut usize) -> Result<(), GameError> {
    let mut rest = value;
    if rest >= 100 {
        let hundreds = (rest / 100) as usize;
        put_word(out, written, SMALL_WORDS[hundreds])?;
        put_word(out, written, "hundred")?;
        rest %= 100;
    }
    if rest >= 20 {
        put_word(out, written, TENS_WORDS[(rest / 10) as usize])?;
        rest %= 10;
        if rest > 0 {
            put_word(out, written, SMALL_WORDS[rest as usize])?;
        }
    } else if rest > 0 {
        put_word(out, written, SMALL_WORDS[rest as usize])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pig(text: &str) -> String {
        let mut out = [0u8; 64];
        let len = pig_word(text, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    fn words(value: u32) -> String {
        let mut out = [0u8; 96];
        let len = number_words(value, &mut out).unwrap();
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
    fn test_number_words() {
        assert_eq!(words(0), "zero");
        assert_eq!(words(13), "thirteen");
        assert_eq!(words(42), "forty two");
        assert_eq!(words(100), "one hundred");
        assert_eq!(words(1234), "one thousand two hundred thirty four");
    }

    #[test]
    fn test_bad_words_rejected() {
        let mut out = [0u8; 8];
        assert_eq!(pig_word("", &mut out), Err(GameError::InvalidArgument));
        assert_eq!(
            number_words(1_000_000, &mut out),
            Err(GameError::OutOfRange)
        );
        assert_eq!(
            pig_word("averylongwordindeed", &mut out),
            Err(GameError::OutOfRange)
        );
    }
}
