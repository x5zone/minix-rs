//! Morse code tables.
//!
//! Ground truth: `minix3/games/morse/morse.c`. The code table sits near line
//! 96, usage is `morse [-ds] [string ...]` (near line 140): without flags text
//! encodes to code, `-d` decodes code to text, `-s` keeps processing after
//! errors instead of stopping. Decoding matches the full code string against
//! the table (near line 219). This module owns the table and both directions;
//! the flag handling stays with the execution layer.

use crate::GameError;

/// International morse code for capitals A through Z then digits 0 through 9.
pub const MORSE_CODES: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."),
    ('F', "..-."), ('G', "--."), ('H', "...."), ('I', ".."), ('J', ".---"),
    ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"),
    ('P', ".--."), ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"),
    ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"), ('Y', "-.--"),
    ('Z', "--.."), ('0', "-----"), ('1', ".----"), ('2', "..---"),
    ('3', "...--"), ('4', "....-"), ('5', "....."), ('6', "-...."),
    ('7', "--..."), ('8', "---.."), ('9', "----."),
];

/// Code table behind encoding and decoding.
pub trait MorseTable {
    /// Code for `letter` (case insensitive), or `None` when absent.
    fn code_of(&self, letter: char) -> Option<&'static str>;
    /// Letter for `code`, or `None` when no letter carries it.
    fn letter_of(&self, code: &str) -> Option<char>;
}

/// Static international table.
pub struct StaticMorse;

impl MorseTable for StaticMorse {
    fn code_of(&self, letter: char) -> Option<&'static str> {
        let upper = letter.to_ascii_uppercase();
        MORSE_CODES
            .iter()
            .find(|(known, _)| *known == upper)
            .map(|(_, code)| *code)
    }

    fn letter_of(&self, code: &str) -> Option<char> {
        MORSE_CODES
            .iter()
            .find(|(_, known)| *known == code)
            .map(|(letter, _)| *letter)
    }
}

/// Empty table (every lookup misses).
pub struct EmptyMorse;

impl MorseTable for EmptyMorse {
    fn code_of(&self, _letter: char) -> Option<&'static str> {
        None
    }

    fn letter_of(&self, _code: &str) -> Option<char> {
        None
    }
}

/// Encode one word into code groups separated by single blanks.
///
/// Letters map through the table (case insensitive); digits map directly;
/// blanks between words become `/`. Unknown characters report not found.
pub fn encode_word<T: MorseTable>(
    table: &T,
    word: &str,
    out: &mut [u8],
) -> Result<usize, GameError> {
    let mut written = 0;
    let mut first = true;
    for ch in word.chars() {
        if ch == ' ' {
            if out.len() - written < 3 {
                return Err(GameError::OutOfRange);
            }
            out[written..written + 3].copy_from_slice(b" / ");
            written += 3;
            first = true;
            continue;
        }
        let code = table.code_of(ch).ok_or(GameError::NotFound)?;
        if !first {
            if written >= out.len() {
                return Err(GameError::OutOfRange);
            }
            out[written] = b' ';
            written += 1;
        }
        if out.len() - written < code.len() {
            return Err(GameError::OutOfRange);
        }
        out[written..written + code.len()].copy_from_slice(code.as_bytes());
        written += code.len();
        first = false;
    }
    Ok(written)
}

/// Decode one code group (dots and dashes) into its letter.
pub fn decode_group<T: MorseTable>(table: &T, group: &str) -> Result<char, GameError> {
    if group.is_empty()
        || !group.bytes().all(|byte| byte == b'.' || byte == b'-')
    {
        return Err(GameError::InvalidArgument);
    }
    table.letter_of(group).ok_or(GameError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sos_encodes() {
        let table = StaticMorse;
        let mut out = [0u8; 32];
        let len = encode_word(&table, "SOS", &mut out).unwrap();
        assert_eq!(&out[..len], b"... --- ...");
    }

    #[test]
    fn test_case_insensitive() {
        let table = StaticMorse;
        assert_eq!(table.code_of('a'), table.code_of('A'));
    }

    #[test]
    fn test_digits_present() {
        let table = StaticMorse;
        assert_eq!(table.code_of('0'), Some("-----"));
        assert_eq!(table.letter_of("----."), Some('9'));
    }

    #[test]
    fn test_unknown_letter_reports_not_found() {
        let table = StaticMorse;
        let mut out = [0u8; 32];
        assert_eq!(encode_word(&table, "hi!", &mut out), Err(GameError::NotFound));
        assert_eq!(decode_group(&table, "...---..."), Err(GameError::NotFound));
    }

    #[test]
    fn test_bad_groups_rejected() {
        let table = StaticMorse;
        assert_eq!(decode_group(&table, ""), Err(GameError::InvalidArgument));
        assert_eq!(decode_group(&table, "..x"), Err(GameError::InvalidArgument));
    }

    #[test]
    fn test_empty_table_misses() {
        let table = EmptyMorse;
        assert_eq!(table.code_of('A'), None);
        assert_eq!(table.letter_of(".-"), None);
    }

    #[test]
    fn test_small_buffer_rejected() {
        let table = StaticMorse;
        let mut out = [0u8; 2];
        assert_eq!(encode_word(&table, "SOS", &mut out), Err(GameError::OutOfRange));
    }
}
