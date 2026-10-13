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

/// C's `other[]` (`morse.c:97-111`). The order is the order C's `decode()` walks after
/// it falls through the letter and digit tables, and C's `morse()` walks it looking for
/// a byte. Every code in the table is distinct and none collides with a letter or digit
/// code, so this particular order has no observable effect — unlike `unvis`, whose
/// entity table is prefix-scanned and where reordering really would change the accepted
/// set.
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

/// C's `decode(const char *s)` (`morse.c:201-228`) as one whole-string lookup.
///
/// A code is compared whole against the digit table, then the alphabet, then `other[]`:
/// `".-"` is `A`, `".-."` is `R` and `".-.."` is `L`, and no shorter code ever matches as
/// a prefix of a longer one. `SK_CODE` is the one input that produces **no byte at all**
/// (`:225-226`); anything else that misses every table is line noise and answers `x`
/// (`:227`). `None` therefore means "silently consumed", not "error".
///
/// The answer is a byte rather than a `char` because `putchar` is what C writes, and the
/// table keys are ASCII by construction — a test below pins that invariant.
pub fn decode_code(code: &str) -> Option<u8> {
    if let Some(letter) = StaticMorse.letter_of(code) {
        return Some(letter as u8);
    }
    if let Some((byte, _)) = PUNCT_CODES.iter().find(|(_, known)| *known == code) {
        return Some(*byte);
    }
    if code == SK_CODE { None } else { Some(b'x') }
}

/// C's `-d` standard-input loop (`morse.c:151-185`) driven by a byte stream.
///
/// Three shapes in it are not what a first reading suggests:
///
/// * a separator that finds a group pending only flushes it, and clearing `is_blank` is
///   part of that flush (`morse.c:170-174`). The space belongs to the pair of separators
///   that both find nothing pending (`:175-182`), so `...  ---` prints no blank at all
///   while `...   ---` prints one: word separation is a by-product of runs of blanks, not
///   of the gaps between code groups.
/// * ten accumulated symbols are gibberish: C writes one `x`, resets, and then reads
///   **past the remainder of that symbol run**, which also consumes the first non-symbol
///   byte after it (`:160-169`). That byte can no longer act as a separator, which is why
///   the overrun path sets the blank state itself.
/// * the closing `putchar('\n')` sits outside the loop (`:185`), so it is written even for
///   an empty stream.
///
/// A byte that is neither a symbol nor a separator, arriving with no group pending, hits
/// none of C's `else if` arms and is ignored (`:170-182`).
pub fn decode_stream<F: FnMut(u8)>(bytes: &[u8], mut sink: F) {
    // C's `char foo[10]`: nine symbols max, because the tenth trips the overrun path
    // before it could ever be decoded (`:152`, `:158-169`).
    let mut group = [0u8; 10];
    let mut len = 0usize;
    let mut blank = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte == b'-' || byte == b'.' {
            group[len] = byte;
            len += 1;
            if len == group.len() {
                len = 0;
                sink(b'x');
                while index < bytes.len() && (bytes[index] == b'.' || bytes[index] == b'-') {
                    index += 1;
                }
                if index < bytes.len() {
                    // The byte that ended that scan is consumed and dropped, as in C.
                    index += 1;
                }
                blank = true;
            }
        } else if len != 0 {
            // The pending bytes are all `.` or `-`, so the view is always valid UTF-8;
            // the fallback keeps a hypothetical miss on C's own `x` answer.
            let code = core::str::from_utf8(&group[..len]).unwrap_or(BLANK_CODE);
            if let Some(answer) = decode_code(code) {
                sink(answer);
            }
            len = 0;
            blank = false;
        } else if byte == 0x0b || byte.is_ascii_whitespace() {
            if blank {
                sink(b' ');
                blank = false;
            } else {
                blank = true;
            }
        }
    }
    sink(b'\n');
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

    /// Runs C's decoder loop over a byte string and returns what it wrote.
    fn decoded(input: &[u8]) -> String {
        let mut out = Vec::new();
        decode_stream(input, &mut |byte| out.push(byte));
        String::from_utf8(out).expect("the decoder writes table bytes")
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
    fn whole_string_lookups_answer_letters_digits_punctuation_noise_and_the_silent_mark() {
        // `decode()` never prefix-matches: `.-` is A, `.-.` is R, `.-..` is L, and the
        // digit entry `....-` is the character `4` (`morse.c:206-224`).
        assert_eq!(decode_code(".-"), Some(b'A'));
        assert_eq!(decode_code(".-."), Some(b'R'));
        assert_eq!(decode_code(".-.."), Some(b'L'));
        assert_eq!(decode_code("....-"), Some(b'4'));
        assert_eq!(decode_code("....."), Some(b'5'));
        // Punctuation answers from `other[]`, and the end mark is the one input that
        // writes nothing at all (`:225-226`).
        assert_eq!(decode_code(".-.-.-"), Some(b'.'));
        assert_eq!(decode_code("-....-"), Some(b'-'));
        assert_eq!(decode_code(SK_CODE), None);
        // Anything else is line noise, not an error (`:227`) — including the empty code.
        assert_eq!(decode_code("......."), Some(b'x'));
        assert_eq!(decode_code(""), Some(b'x'));
        // The byte answer rests on the table keys being ASCII; that is a fact about the
        // C tables (`digit[]` `:53-64`, `alph[]` `:65-92`), so pin it rather than assume.
        for (key, code) in MORSE_CODES {
            assert!(key.is_ascii(), "{key:?} is not an ascii key");
            assert!(
                code.bytes().all(|byte| byte == b'.' || byte == b'-'),
                "{code:?} holds symbols only"
            );
        }
    }

    #[test]
    fn one_separator_between_groups_prints_nothing_and_a_trailing_group_is_dropped() {
        // C's loop only flushes a group when a non-symbol byte arrives (`morse.c:170-174`)
        // and never flushes at end of input, so the last group of `... --- ...` is lost:
        // the closing `putchar('\n')` (`:185`) is not a separator.
        assert_eq!(decoded(b"... --- ..."), "SO\n");
        assert_eq!(decoded(b"... --- ...\n"), "SOS\n");
        // The separator that flushes a group does not also print: the blank comes from a
        // later separator that finds nothing pending (`:175-182`). Counting the terminating
        // newline as a separator too, two trailing spaces print one blank, three print one,
        // four print two.
        assert_eq!(decoded(b"... \n"), "S\n");
        assert_eq!(decoded(b"...  \n"), "S \n");
        assert_eq!(decoded(b"...   \n"), "S \n");
        assert_eq!(decoded(b"...    \n"), "S  \n");
        // A group sandwiched between separators still decodes with no blank around it, and
        // a three-separator run is what puts a space between two letters.
        assert_eq!(decoded(b"...  ---\n"), "SO\n");
        assert_eq!(decoded(b"...   ---\n"), "S O\n");
        // The flush clears the flag, so a separator set before a group does not carry over
        // across it: the space here has to wait for another free separator (`:174`).
        assert_eq!(decoded(b"...  .. \n"), "SI\n");
        assert_eq!(decoded(b"...  ..  \n"), "SI \n");
    }

    #[test]
    fn ten_symbols_are_gibberish_and_the_swallow_eats_the_separator_behind_them() {
        // The tenth symbol trips `putchar('x')`, resets the counter and then reads past the
        // rest of the run — including the first byte after it, which is consumed and
        // cannot serve as a separator any more (`morse.c:160-169`).
        assert_eq!(decoded(b"..........\n"), "x\n");
        assert_eq!(decoded(b"............\n"), "x\n");
        assert_eq!(
            decoded(b"............\n\n"),
            "x \n",
            "the first newline is eaten, the second prints a blank"
        );
        // Nine symbols are still a group, and the buffer never holds a tenth to decode.
        assert_eq!(
            decoded(b".........\n"),
            "x\n",
            "nine dots match no table entry"
        );
        // The threshold is exactly ten, and what makes it observable is the separator the
        // swallow eats: after twelve symbols the first newline is gone, so the second one
        // prints the blank; after nine there is no swallow, so the first space only sets the
        // flag and the second prints nothing until the group closes.
        assert_eq!(decoded(b"............  ...\n"), "x S\n");
        assert_eq!(decoded(b".........  ...\n"), "xS\n");
    }

    #[test]
    fn bytes_that_are_neither_symbols_nor_separators_are_ignored_while_nothing_is_pending() {
        // C's `else if` chain has no fall-through arm for such a byte: with no group
        // pending it simply reads the next one (`morse.c:170-182`).
        assert_eq!(decoded(b"z"), "\n");
        assert_eq!(
            decoded(b""),
            "\n",
            "an empty stream still gets its closing newline"
        );
        assert_eq!(decoded(b"z\n"), "\n");
        // Tabs are separators like any other, so they split groups without printing.
        assert_eq!(decoded(b"...\t...\n"), "SS\n");
        assert_eq!(decoded(b"...\t\t\t...\n"), "S S\n");
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
    fn unlisted_bytes_stay_silent_on_the_encode_side() {
        // The encode lookup has no error arm either: C's `else` branch simply finds
        // nothing in `other[]` and prints nothing (`morse.c:241-250`).
        assert_eq!(code_for_byte(b'!'), None);
        assert_eq!(decode_code("...---..."), Some(b'x'));
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
