//! Word play: pig latin and number words.
//!
//! Ground truth: `minix3/games/pig/pig.c` (words starting with a vowel gain
//! `way`, other words move the leading consonant cluster to the end plus `ay`;
//! a leading `y` counts as a consonant, near line 103; all capital words keep
//! capital suffixes; usage is bare `pig` near line 133) and
//! `minix3/games/number/number.c` (arabic numbers become English words).
//! This module owns both conversions; line splitting stays with the execution
//! layer.

use crate::GameError;

/// True for the five vowels (both cases). `y` is handled by position, not here.
fn is_vowel(byte: u8) -> bool {
    matches!(byte, b'a' | b'e' | b'i' | b'o' | b'u' | b'A' | b'E' | b'I' | b'O' | b'U')
}

/// Convert one word to pig latin into `out`, returning the used length.
///
/// Rules: a vowel start gains `way` (`apple` becomes `appleway`); otherwise
/// the leading consonant cluster (where a first `y` counts as a consonant and
/// a later `y` counts as a vowel) moves to the end plus `ay` (`string` becomes
/// `ingstray`, `yellow` becomes `ellowyay`); all capital input keeps a capital
/// suffix (`APPLE` becomes `APPLEWAY`); words without any vowel gain `ay`
/// unchanged (`my` becomes `myay`). Non letters pass through untouched only
/// when the whole word holds no letters.
pub fn pig_word(word: &str, out: &mut [u8]) -> Result<usize, GameError> {
    if word.is_empty() {
        return Err(GameError::InvalidArgument);
    }
    let bytes = word.as_bytes();
    if !bytes.iter().any(|byte| byte.is_ascii_alphabetic()) {
        if out.len() < bytes.len() {
            return Err(GameError::OutOfRange);
        }
        out[..bytes.len()].copy_from_slice(bytes);
        return Ok(bytes.len());
    }
    let all_upper = bytes
        .iter()
        .filter(|byte| byte.is_ascii_alphabetic())
        .all(|byte| byte.is_ascii_uppercase());
    // Find the split point: first vowel, where a non first `y` counts.
    let mut split = bytes.len();
    for (index, byte) in bytes.iter().enumerate() {
        let vowel = is_vowel(*byte)
            || ((*byte == b'y' || *byte == b'Y') && index > 0);
        if byte.is_ascii_alphabetic() && vowel {
            split = index;
            break;
        }
    }
    let (suffix, head) = if split == 0 {
        ("way", "")
    } else if split >= bytes.len() {
        ("ay", "")
    } else {
        ("ay", &word[..split])
    };
    let tail = if split == 0 || split >= bytes.len() {
        word
    } else {
        &word[split..]
    };
    let suffix = if all_upper {
        match suffix {
            "way" => "WAY",
            _ => "AY",
        }
    } else {
        suffix
    };
    let needed = tail.len() + head.len() + suffix.len();
    if out.len() < needed {
        return Err(GameError::OutOfRange);
    }
    let mut written = 0;
    // Lowercase the moved head to match classic output (`String`→`ingStray`).
    for byte in tail.bytes() {
        out[written] = byte;
        written += 1;
    }
    for byte in head.bytes() {
        out[written] = byte.to_ascii_lowercase();
        written += 1;
    }
    // Capitalize the new first letter when the input was capitalized.
    if bytes[0].is_ascii_uppercase() && written > 0 {
        out[0] = out[0].to_ascii_uppercase();
        // Restore the moved head's original case is unnecessary: classic pig
        // latin lowercases the moved cluster except an all upper word.
        if !all_upper {
            for byte in out[1..written].iter_mut() {
                *byte = byte.to_ascii_lowercase();
            }
        }
    }
    out[written..written + suffix.len()].copy_from_slice(suffix.as_bytes());
    written += suffix.len();
    Ok(written)
}

/// Ones through nineteen.
const SMALL_WORDS: &[&str] = &[
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight",
    "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen",
    "sixteen", "seventeen", "eighteen", "nineteen",
];

/// Tens multiples from twenty to ninety.
const TENS_WORDS: &[&str] = &[
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty",
    "ninety",
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
        assert_eq!(pig("yellow"), "ellowyay");
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
        assert_eq!(number_words(1_000_000, &mut out), Err(GameError::OutOfRange));
        assert_eq!(pig_word("averylongwordindeed", &mut out), Err(GameError::OutOfRange));
    }
}
