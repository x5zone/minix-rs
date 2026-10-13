//! Morse code tables.
//!
//! Ground truth: `minix3/games/morse/morse.c`. Three tables carry the codes: `digit`
//! (ten entries, `:53-64`), `alph` (twenty-six, `:65-92`) and `other` (twelve
//! punctuation pairs plus a `'\0'` sentinel, `:97-111`). The usage line is
//! `morse [-ds] [string ...]` (`:140`), and the two letters do two unrelated things:
//! `-d` picks the direction (`:132-134`), while `-s` only changes **how a code is
//! rendered** — `show()` prints the symbol run itself instead of spelling every dot
//! as `dit` and every dash as `daw` (`:135-137`, `:253-261`). It is not a
//! code-table dump and it does not touch error tolerance: `decode()` never consults
//! it, so `-s` under `-d` is inert.
//!
//! This module owns the tables and the two render forms; the flag handling and the
//! byte-at-a-time I/O stay with the execution layer.

use crate::GameError;

/// International morse code for capitals A through Z then digits 0 through 9.
pub const MORSE_CODES: &[(char, &str)] = &[
    ('A', ".-"),
    ('B', "-..."),
    ('C', "-.-."),
    ('D', "-.."),
    ('E', "."),
    ('F', "..-."),
    ('G', "--."),
    ('H', "...."),
    ('I', ".."),
    ('J', ".---"),
    ('K', "-.-"),
    ('L', ".-.."),
    ('M', "--"),
    ('N', "-."),
    ('O', "---"),
    ('P', ".--."),
    ('Q', "--.-"),
    ('R', ".-."),
    ('S', "..."),
    ('T', "-"),
    ('U', "..-"),
    ('V', "...-"),
    ('W', ".--"),
    ('X', "-..-"),
    ('Y', "-.--"),
    ('Z', "--.."),
    ('0', "-----"),
    ('1', ".----"),
    ('2', "..---"),
    ('3', "...--"),
    ('4', "....-"),
    ('5', "....."),
    ('6', "-...."),
    ('7', "--..."),
    ('8', "---.."),
    ('9', "----."),
];

/// C's `other[]` (`morse.c:97-111`), punctuation plus the two brackets and the quote
/// forms. The order is the order C's `decode()` walks when it falls through the
/// letter and digit tables, so it is part of the matching rule, not cosmetics.
pub const PUNCT_CODES: &[(u8, &str)] = &[
    (b'.', ".-.-.-"),
    (b',', "--..--"),
    (b':', "---..."),
    (b'?', "..--.."),
    (b'\'', ".----."),
    (b'-', "-....-"),
    (b'/', "-..-."),
    (b'(', "-.--."),
    (b')', "-.--.-"),
    (b'"', ".-..-."),
    (b'=', "-...-"),
    (b'+', ".-.-."),
];

/// The end-of-message mark: after everything else, C calls `show("...-.-")` once
/// (`morse.c:195`) — the `SK` prosign, spelled out like any other code.
pub const SK_CODE: &str = "...-.-";

/// Empty code, which C uses to close each argument and to render a whitespace byte
/// (`morse.c:191`, `:239-240`).
pub const BLANK_CODE: &str = "";

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

/// C's `morse(int c)` (`morse.c:231-251`) as a pure lookup, byte in and code out.
///
/// Letters resolve case insensitively and digits directly, as in C. A whitespace byte
/// is the **empty** code, because C calls `show("")` for it rather than skipping it:
/// the separator produces a line of its own. C's `isspace` in the C locale covers
/// vertical tab (0x0B) as well, which `u8::is_ascii_whitespace` leaves out — dropping
/// that one byte would swallow a separator the C program prints.
///
/// `None` means C printed **nothing at all**: its `else` arm scans `other[]` and falls
/// off the end without calling `show()` when the byte is not listed there, so an
/// unlisted byte (`!`, `@`, anything at 0x80 or above) contributes no line.
pub fn code_for_byte(byte: u8) -> Option<&'static str> {
    if byte.is_ascii_alphabetic() || byte.is_ascii_digit() {
        return StaticMorse.code_of(char::from(byte));
    }
    if byte == 0x0b || byte.is_ascii_whitespace() {
        return Some(BLANK_CODE);
    }
    PUNCT_CODES
        .iter()
        .find(|(known, _)| *known == byte)
        .map(|(_, code)| *code)
}

/// C's `show()` (`morse.c:253-261`) for one code, written into `out`.
///
/// The two forms differ by more than the alphabet. Without `-s` every symbol gets its
/// own leading space and the word for it, and C's test is `*s == '.'` — so **anything**
/// that is not a dot reads as a dash. With `-s` the whole run is printed after a single
/// leading space, which turns the empty code into a line holding exactly one space
/// while the spelled-out form of the same code is an empty line. Both end in a newline.
///
/// A buffer too small for the whole line is reported rather than truncated; the longest
/// line any table code can make is `SK_CODE`'s six symbols (`MAX_SHOW_LINE`).
pub fn show_line(code: &str, short: bool, out: &mut [u8]) -> Result<usize, GameError> {
    let needed = if short {
        code.len() + 2
    } else {
        // One leading space plus three letters per symbol — the loop below walks bytes,
        // and every code in the C tables is ASCII, so the byte count is the symbol count.
        code.len() * 4 + 1
    };
    if out.len() < needed {
        return Err(GameError::OutOfRange);
    }
    let mut written = 0;
    if short {
        out[written] = b' ';
        written += 1;
        out[written..written + code.len()].copy_from_slice(code.as_bytes());
        written += code.len();
    } else {
        for byte in code.bytes() {
            out[written] = b' ';
            written += 1;
            let word: &[u8] = if byte == b'.' { b"dit" } else { b"daw" };
            out[written..written + 3].copy_from_slice(word);
            written += 3;
        }
    }
    out[written] = b'\n';
    written += 1;
    Ok(written)
}

/// Longest line [`show_line`] can produce: the six-symbol `SK_CODE` spelled out.
pub const MAX_SHOW_LINE: usize = SK_CODE.len() * 4 + 1;

/// Decode one code group (dots and dashes) into its letter.
pub fn decode_group<T: MorseTable>(table: &T, group: &str) -> Result<char, GameError> {
    if group.is_empty() || !group.bytes().all(|byte| byte == b'.' || byte == b'-') {
        return Err(GameError::InvalidArgument);
    }
    table.letter_of(group).ok_or(GameError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(code: &str, short: bool) -> String {
        let mut out = [0u8; MAX_SHOW_LINE];
        let len = show_line(code, short, &mut out).expect("the buffer holds any table line");
        core::str::from_utf8(&out[..len])
            .expect("every rendered line is ascii")
            .to_string()
    }

    #[test]
    fn the_default_form_spells_each_symbol_and_reads_any_non_dot_as_a_daw() {
        // C's `show()` prints " dit" / " daw" per symbol and one newline at the end
        // (`morse.c:258-260`), so `SOS` is three lines and the mark is six symbols.
        assert_eq!(shown("...", false), " dit dit dit\n");
        assert_eq!(shown(SK_CODE, false), " dit dit dit daw dit daw\n");
        // The C test is `*s == '.'`, so any other byte — here a stray `x` — is a dash.
        assert_eq!(shown(".x-", false), " dit daw daw\n");
        // The spelled form of the empty code is a bare newline, not a space.
        assert_eq!(shown(BLANK_CODE, false), "\n");
    }

    #[test]
    fn the_short_form_prints_the_run_after_one_space_and_even_a_blank_run_keeps_it() {
        // `printf(" %s", s)` (`morse.c:257`) means the run itself goes out, and the
        // leading space is printed even when there is no run: under `-s` a separator
        // is a line holding one space, where without it the same line is empty.
        assert_eq!(shown("...", true), " ...\n");
        assert_eq!(shown(BLANK_CODE, true), " \n");
        assert_eq!(shown(SK_CODE, true), " ...-.-\n");
        // The two forms are genuinely different output, not a rename of one another.
        assert_ne!(shown("...", true), shown("...", false));
    }

    #[test]
    fn whitespace_is_the_empty_code_and_vertical_tab_counts_as_whitespace() {
        // `morse.c:239-240` sends any `isspace` byte to `show("")`. The C locale also
        // counts 0x0B, which `is_ascii_whitespace` does not — that byte is why this
        // lookup cannot simply trust the Rust helper.
        for byte in [b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'] {
            assert_eq!(
                code_for_byte(byte),
                Some(BLANK_CODE),
                "{byte:?} is a separator"
            );
        }
        assert_eq!(code_for_byte(b'a'), Some(".-"));
        assert_eq!(code_for_byte(b'A'), Some(".-"));
        assert_eq!(code_for_byte(b'5'), Some("....."));
        assert_eq!(code_for_byte(b'9'), Some("----."));
    }

    #[test]
    fn punctuation_comes_from_the_other_table_and_unlisted_bytes_print_nothing() {
        // All twelve entries of `other[]` (`morse.c:97-111`), and note that `.` and `-`
        // are punctuation here even though they are also the symbols themselves.
        assert_eq!(PUNCT_CODES.len(), 12);
        for (byte, code) in PUNCT_CODES {
            assert_eq!(code_for_byte(*byte), Some(*code), "{byte:?}");
        }
        // C's `else` arm falls off the table without printing, so these are silent.
        assert_eq!(code_for_byte(b'!'), None);
        assert_eq!(code_for_byte(b'@'), None);
        assert_eq!(code_for_byte(0xff), None);
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
        assert_eq!(code_for_byte(b'!'), None);
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
    fn a_short_buffer_is_refused_rather_than_truncated() {
        let mut out = [0u8; 3];
        assert_eq!(
            show_line("...", false, &mut out),
            Err(GameError::OutOfRange)
        );
        // The same call with room for one symbol only (" dit" plus the newline) must
        // still be refused: the guard counts the whole line, not each piece.
        let mut tight = [0u8; 4];
        assert_eq!(
            show_line(".", false, &mut tight),
            Err(GameError::OutOfRange)
        );
        let mut just = [0u8; 5];
        assert_eq!(show_line(".", false, &mut just), Ok(5));
    }
}
