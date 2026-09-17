//! Key parsing and comparison for `sort`.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The C extracts each
//! record's sort keys into transformed key buffers and compares those
//! buffers byte-wise (`fields.c`); the observable semantics — which this
//! module reimplements at the value level — are: key specifications
//! `-k F[.C][flags][,F[.C][flags]]` with per-key flags `b d f i M n r`
//! (unknown flag letters skip silently, `optval`'s default, init.c:229-247;
//! `.0` is rejected as an illegal offset, init.c:168-170), fields
//! separated by blank runs or by the `-t` delimiter byte, default
//! whole-line keys, the modifier transformations dictionary (`d`), fold
//! (`f`), ignore non-printable (`i`), numeric (`n`, blank-skipping sign
//! and fraction aware), month (`M`, `jan` lowest), and reverse (`r`; a
//! local `r` is dropped when a global `-r` stands — init.c:239-242).
//! Ties fall back to the whole line in byte order, and the global `-r`
//! inverts the final outcome.
//!
//! The engine (external merge over temporary files) is the execution
//! layer; this module compares slices, which is what the observable
//! output order is made of. Nothing allocates: modifiers apply as
//! predicates over the existing bytes. The option surface itself needs
//! one growable list (the key specifications), served by `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Blank bytes for field separation: space and tab (the C locale's
/// `<blank>` class).
fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// One parsed `-k` key specification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeySpec {
    /// First field (1 based).
    pub start_field: usize,
    /// First character within the field (1 based; zero means field start).
    pub start_char: usize,
    /// Skip leading blanks at the key start (`b` on the start segment).
    pub blank_start: bool,
    /// Last field (1 based; zero means the last field of the line).
    pub end_field: usize,
    /// Last character within the end field (zero means field end).
    pub end_char: usize,
    /// Skip leading blanks in the end field (`b` on the end segment).
    pub blank_end: bool,
    /// Per-key modifiers.
    pub dictionary: bool,
    pub fold: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    pub month: bool,
    /// This key's comparison inverts (dropped when a global `-r` stands).
    pub reverse: bool,
}

impl KeySpec {
    /// The default whole-line key (no `-k` given).
    pub fn whole_line() -> Self {
        KeySpec {
            start_field: 1,
            start_char: 1,
            blank_start: false,
            end_field: 0,
            end_char: 0,
            blank_end: false,
            dictionary: false,
            fold: false,
            ignore_nonprint: false,
            numeric: false,
            month: false,
            reverse: false,
        }
    }
}

/// Global options for one `sort` run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Key specifications (empty = one whole-line key).
    pub keys: Vec<KeySpec>,
    /// Global reverse.
    pub reverse: bool,
    pub fold: bool,
    pub dictionary: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    pub month: bool,
    /// Leading-blank skip as a global flag.
    pub blank_skip: bool,
    /// Suppress lines that compare equal to their predecessor.
    pub unique: bool,
    /// Check only: report disorder and duplicates, sort nothing.
    pub check: bool,
    /// Field delimiter byte (`-t`).
    pub delimiter: Option<u8>,
}

/// An unparsable option (`-k`/`-t` value faults included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    BadOption(char),
}

/// Parse global options from the argument words, returning the options
/// plus the remaining operand words (file paths in the C; the doing half
/// rejects them while the open-existing call is gated). `-k` and `-t`
/// take a value, given separately or inline. Unknown options and missing
/// values are [`ParseError::BadOption`].
pub fn parse_options<'a>(args: &[&'a str]) -> Result<(Options, Vec<&'a str>), ParseError> {
    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if !arg.starts_with('-') || arg.len() <= 1 {
            operands.push(arg);
            index += 1;
            continue;
        }
        let mut chars = arg[1..].chars().peekable();
        while let Some(flag) = chars.next() {
            match flag {
                'r' => options.reverse = true,
                'f' => options.fold = true,
                'd' => options.dictionary = true,
                'i' => options.ignore_nonprint = true,
                'n' => options.numeric = true,
                'M' => options.month = true,
                'b' => options.blank_skip = true,
                'u' => options.unique = true,
                'c' => options.check = true,
                'k' | 't' => {
                    let mut value: String = chars.by_ref().collect();
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = (*word).to_string(),
                            None => return Err(ParseError::BadOption(flag)),
                        }
                    }
                    if flag == 'k' {
                        match parse_key(&value, &options) {
                            Some(spec) => options.keys.push(spec),
                            None => return Err(ParseError::BadOption(flag)),
                        }
                    } else {
                        let mut bytes = value.bytes();
                        match (bytes.next(), bytes.next()) {
                            (Some(one), None) => options.delimiter = Some(one),
                            _ => return Err(ParseError::BadOption(flag)),
                        }
                    }
                    break;
                }
                _ => return Err(ParseError::BadOption(flag)),
            }
        }
        index += 1;
    }
    Ok((options, operands))
}

/// Parses one `-k` specification. Modifiers ride the start segment, the
/// end segment, or both; unknown flag letters are skipped silently. A
/// `.0` character offset is rejected (the C's "illegal offset").
pub fn parse_key(spec: &str, global: &Options) -> Option<KeySpec> {
    let (start_text, end_text) = match spec.split_once(',') {
        Some((a, b)) => (a, Some(b)),
        None => (spec, None),
    };
    let (start_field, start_char, start_flags, blank_start) = parse_column(start_text)?;
    let mut key = KeySpec {
        start_field: if start_field == 0 { 1 } else { start_field },
        start_char,
        blank_start: blank_start || global.blank_skip,
        end_field: 0,
        end_char: 0,
        blank_end: false,
        dictionary: start_flags.contains('d') || global.dictionary,
        fold: start_flags.contains('f') || global.fold,
        ignore_nonprint: start_flags.contains('i') || global.ignore_nonprint,
        numeric: start_flags.contains('n') || global.numeric,
        month: start_flags.contains('M') || global.month,
        reverse: start_flags.contains('r') && !global.reverse,
    };
    if let Some(end_text) = end_text {
        let (end_field, end_char, end_flags, blank_end) = parse_column(end_text)?;
        key.end_field = end_field;
        key.end_char = end_char;
        key.blank_end = blank_end || global.blank_skip;
        key.dictionary |= end_flags.contains('d');
        key.fold |= end_flags.contains('f');
        key.ignore_nonprint |= end_flags.contains('i');
        key.numeric |= end_flags.contains('n');
        key.month |= end_flags.contains('M');
        key.reverse |= end_flags.contains('r') && !global.reverse;
    }
    if global.reverse {
        // init.c:239-242 — a local `r` does not invert a global `-r`.
        key.reverse = false;
    }
    Some(key)
}

/// Parses `F[.C]` plus trailing modifier flags. Returns the field, the
/// character (`.C`; absent and `.1` are both offset zero, `.0` is an
/// error), the modifier letters, and whether a `b` modifier was present.
fn parse_column(text: &str) -> Option<(usize, usize, String, bool)> {
    let bytes = text.as_bytes();
    let mut position = 0;
    let mut field = 0usize;
    while position < bytes.len() && bytes[position].is_ascii_digit() {
        field = field * 10 + (bytes[position] - b'0') as usize;
        position += 1;
    }
    let mut character_given = false;
    let mut character = 0usize;
    if position < bytes.len() && bytes[position] == b'.' {
        position += 1;
        character_given = true;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            character = character * 10 + (bytes[position] - b'0') as usize;
            position += 1;
        }
        if character == 0 {
            // The C's indent decrement turns `.0` negative: illegal offset.
            return None;
        }
    }
    let mut flags = String::new();
    let mut blank = false;
    while position < bytes.len() {
        match bytes[position] {
            b'b' => blank = true,
            b'd' | b'f' | b'i' | b'n' | b'M' | b'r' => flags.push(bytes[position] as char),
            // Unknown flag letters skip silently (optval's default).
            _ => {}
        }
        position += 1;
    }
    let _ = character_given;
    Some((field, character, flags, blank))
}

/// The span of the `field`-th field of a line (1 based), byte offsets
/// exclusive at the end.
///
/// With a delimiter the pieces split on every delimiter byte (empty
/// pieces are real fields). Without one, fields are the maximal
/// non-blank runs; the first field starts at byte zero so leading blanks
/// ride with it (the reason plain `sort` often needs `-b`), and a field
/// beyond the last run is the empty span at end of line.
fn field_span(line: &[u8], field: usize, delimiter: Option<u8>) -> (usize, usize) {
    match delimiter {
        Some(delimiter) => {
            let mut start = 0usize;
            let mut index = 1usize;
            let mut cursor = 0usize;
            while cursor < line.len() {
                if line[cursor] == delimiter {
                    if index == field {
                        return (start, cursor);
                    }
                    index += 1;
                    start = cursor + 1;
                }
                cursor += 1;
            }
            if index == field {
                (start, line.len())
            } else {
                (line.len(), line.len())
            }
        }
        None => {
            if field == 1 {
                let mut cursor = 0usize;
                while cursor < line.len() && is_blank(line[cursor]) {
                    cursor += 1;
                }
                while cursor < line.len() && !is_blank(line[cursor]) {
                    cursor += 1;
                }
                return (0, cursor);
            }
            // Field `field` is the (`field` - 1)th non-blank run,
            // 0 based (field 1 is handled above with its leading blanks).
            let target = field - 1;
            let mut cursor = 0usize;
            let mut run = 0usize;
            while cursor < line.len() {
                while cursor < line.len() && is_blank(line[cursor]) {
                    cursor += 1;
                }
                if cursor >= line.len() {
                    break;
                }
                let start = cursor;
                while cursor < line.len() && !is_blank(line[cursor]) {
                    cursor += 1;
                }
                if run == target {
                    return (start, cursor);
                }
                run += 1;
            }
            (line.len(), line.len())
        }
    }
}

/// Extracts one key's bytes from a line per the specification.
fn key_bytes<'a>(line: &'a str, key: &KeySpec, delimiter: Option<u8>) -> &'a str {
    let bytes = line.as_bytes();
    let (start_field_start, start_field_end) = field_span(bytes, key.start_field, delimiter);
    let mut start = start_field_start;
    if key.blank_start {
        while start < start_field_end && is_blank(bytes[start]) {
            start += 1;
        }
    }
    start = start
        .saturating_add(key.start_char.saturating_sub(1))
        .min(line.len());

    if key.end_field == 0 {
        return &line[start..];
    }
    let (end_field_start, end_field_end) = field_span(bytes, key.end_field, delimiter);
    let mut base = end_field_start;
    if key.blank_end {
        while base < end_field_end && is_blank(bytes[base]) {
            base += 1;
        }
    }
    let mut end = if key.end_char > 0 {
        (base + key.end_char).min(end_field_end)
    } else {
        end_field_end
    };
    if end < start {
        end = start;
    }
    &line[start..end]
}

/// A byte passes this key's `d`/`i` filters; `f` folds through the
/// mapping below.
fn visible_and_folded(byte: u8, key: &KeySpec) -> Option<u8> {
    if key.dictionary && !(byte.is_ascii_alphanumeric() || is_blank(byte)) {
        return None;
    }
    if key.ignore_nonprint && !(0x20..=0x7E).contains(&byte) {
        return None;
    }
    Some(if key.fold {
        byte.to_ascii_uppercase()
    } else {
        byte
    })
}

/// Compares two key slices through the `d`/`f`/`i` filters without
/// materialising them: both streams are filtered lazily and compared in
/// lockstep, then by visible length.
fn cmp_filtered(a: &[u8], b: &[u8], key: &KeySpec) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let mut ai = a.iter().filter_map(|b| visible_and_folded(*b, key));
    let mut bi = b.iter().filter_map(|b| visible_and_folded(*b, key));
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
            }
        }
    }
}

/// Month names, `jan` lowest through `dec`; anything shorter than three
/// bytes or outside the names is zero (blank and unknown fields sort
/// first, per the C month table).
fn month_of(key_bytes: &[u8]) -> u32 {
    const MONTHS: [&[u8]; 12] = [
        b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep",
        b"oct", b"nov", b"dec",
    ];
    if key_bytes.len() < 3 {
        return 0;
    }
    for (index, name) in MONTHS.iter().enumerate() {
        if key_bytes[..3].eq_ignore_ascii_case(name) {
            return index as u32 + 1;
        }
    }
    0
}

/// A `-n` numeric key parsed in place: blank-skipping optional sign,
/// digit run, optional fraction; an empty parse is zero.
#[derive(Debug, PartialEq, Eq)]
struct NumericKey<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
}

fn numeric_key(bytes: &[u8]) -> NumericKey<'_> {
    let mut cursor = 0usize;
    while cursor < bytes.len() && is_blank(bytes[cursor]) {
        cursor += 1;
    }
    let negative = bytes.get(cursor) == Some(&b'-');
    if negative {
        cursor += 1;
    }
    let integer_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    let integer = &bytes[integer_start..cursor];
    let fraction_start = cursor + 1;
    if cursor < bytes.len() && bytes[cursor] == b'.' {
        cursor += 1;
        let fraction = &bytes[fraction_start..cursor
            + bytes[fraction_start..].iter()
                .take_while(|b| b.is_ascii_digit())
                .count()];
        return NumericKey {
            negative,
            integer,
            fraction,
        };
    }
    NumericKey {
        negative,
        integer,
        fraction: &[],
    }
}

fn trim_leading_zeros(mut digits: &[u8]) -> &[u8] {
    while digits.len() > 1 && digits[0] == b'0' {
        digits = &digits[1..];
    }
    digits
}

fn is_numeric_zero(key: &NumericKey<'_>) -> bool {
    trim_leading_zeros(key.integer) == b"0" && {
        let mut fraction = key.fraction;
        while fraction.last() == Some(&b'0') {
            fraction = &fraction[..fraction.len() - 1];
        }
        fraction.is_empty()
    }
}

fn compare_numeric(a: &NumericKey<'_>, b: &NumericKey<'_>) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    if is_numeric_zero(a) && is_numeric_zero(b) {
        return Ordering::Equal;
    }
    match (a.negative, b.negative) {
        (false, true) => return Ordering::Greater,
        (true, false) => return Ordering::Less,
        _ => {}
    }
    let magnitude = {
        let ai = trim_leading_zeros(a.integer);
        let bi = trim_leading_zeros(b.integer);
        let by_length = ai.len().cmp(&bi.len());
        let by_lex = ai.cmp(bi);
        if by_length != Ordering::Equal {
            by_length
        } else {
            by_lex
        }
    };
    let magnitude = match magnitude {
        Ordering::Equal => {
            let mut longer = a.fraction.len().max(b.fraction.len());
            let mut index = 0usize;
            let mut ordering = Ordering::Equal;
            while index < longer {
                let x = a.fraction.get(index).copied().unwrap_or(b'0');
                let y = b.fraction.get(index).copied().unwrap_or(b'0');
                if x != y {
                    ordering = x.cmp(&y);
                    break;
                }
                index += 1;
            }
            longer = 0;
            let _ = longer;
            ordering
        }
        other => other,
    };
    if a.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Compares two lines per the options: each key spec contributes a
/// comparison, ties fall through to the next key and then to the whole
/// line in byte order, and the global `-r` inverts the outcome.
pub fn compare(a: &str, b: &str, options: &Options) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    // Without `-k` the global modifiers apply to the whole line: one
    // whole-line key carrying them (and the global reverse) is built
    // here.
    let owned_key;
    let keys: &[KeySpec] = if options.keys.is_empty() {
        owned_key = KeySpec {
            fold: options.fold,
            dictionary: options.dictionary,
            ignore_nonprint: options.ignore_nonprint,
            numeric: options.numeric,
            month: options.month,
            blank_start: options.blank_skip,
            ..KeySpec::whole_line()
        };
        alloc::slice::from_ref(&owned_key)
    } else {
        &options.keys[..]
    };
    let mut result = Ordering::Equal;
    for key in keys {
        let ka = key_bytes(a, key, options.delimiter);
        let kb = key_bytes(b, key, options.delimiter);
        let ordering = if key.numeric {
            compare_numeric(&numeric_key(ka.as_bytes()), &numeric_key(kb.as_bytes()))
        } else if key.month {
            month_of(ka.as_bytes()).cmp(&month_of(kb.as_bytes()))
        } else {
            cmp_filtered(ka.as_bytes(), kb.as_bytes(), key)
        };
        let ordering = if key.reverse {
            ordering.reverse()
        } else {
            ordering
        };
        if ordering != Ordering::Equal {
            result = ordering;
            break;
        }
    }
    if result == Ordering::Equal {
        // The C's last-resort comparison: the raw whole lines in byte
        // order, with the global reverse applied (the modifiers belong
        // to the keys, not to this fallback).
        let raw = a.as_bytes().cmp(b.as_bytes());
        return if options.reverse {
            raw.reverse()
        } else {
            raw
        };
    }
    if options.reverse {
        result.reverse()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    fn opts(extra: &str) -> Options {
        let mut words: Vec<&str> = Vec::new();
        for flag in extra.split_whitespace() {
            words.push(flag);
        }
        match parse_options(&words) {
            Ok((options, _operands)) => options,
            Err(_) => panic!("options should parse"),
        }
    }

    #[test]
    fn test_plain_byte_order() {
        let options = opts("");
        assert_eq!(compare("apple", "banana", &options), Ordering::Less);
        assert_eq!(compare("ab", "ab", &options), Ordering::Equal);
    }

    #[test]
    fn test_reverse_inverts() {
        let options = opts("-r");
        assert_eq!(compare("apple", "banana", &options), Ordering::Greater);
    }

    #[test]
    fn test_numeric_with_signs_and_fractions() {
        let options = opts("-n");
        assert_eq!(compare("  -3.5", "2", &options), Ordering::Less);
        assert_eq!(compare("2.10", "2.5", &options), Ordering::Less);
        // POSIX -n has no plus sign: "+7" is not a number and sorts as
        // zero, equal to "007"; the whole-line fallback then separates
        // them by raw bytes ("+" sorts before "0").
        assert_eq!(compare("+7", "007", &options), Ordering::Less);
        // Non-numeric prefixes compare as zero; the whole-line fallback
        // then orders them by raw bytes.
        assert_eq!(compare("abc", "  ", &options), Ordering::Greater);
    }

    #[test]
    fn test_fold_and_dictionary() {
        // The fold makes the keys equal; the C's last-resort comparison
        // then falls back to the raw whole line, where 'A' < 'a'.
        let options = opts("-f");
        assert_eq!(compare("Apple", "apple", &options), Ordering::Less);
        assert_eq!(compare("Apple", "Apple", &options), Ordering::Equal);
        let dictionary = opts("-d");
        // The '+' byte filters away on both sides: "ab" versus "abc".
        assert_eq!(compare("a+b", "a+bc", &dictionary), Ordering::Less);
    }

    #[test]
    fn test_month_table() {
        let options = opts("-M");
        assert_eq!(compare("jan 1", "feb 1", &options), Ordering::Less);
        assert_eq!(compare("DEC", "jan", &options), Ordering::Greater);
        // Blank or unknown months sort first.
        assert_eq!(compare("???", "jan", &options), Ordering::Less);
    }

    #[test]
    fn test_key_spec_field_selection_with_delimiter() {
        let global = opts("-t:");
        let spec = parse_key("2,2", &global).unwrap();
        let mut options = opts("-t:");
        options.keys.clear();
        options.keys.push(spec);
        // -t: -k2,2 compares the second colon field only; different keys
        // order by it, and the whole line breaks the tie.
        assert_eq!(compare("x:b:zz", "a:a:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:zz", "a:b:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:1", "x:b:2", &options), Ordering::Less);
    }

    #[test]
    fn test_key_spec_dot_zero_rejected() {
        assert!(parse_key("1.0", &opts("")).is_none());
        assert!(parse_key("1.2", &opts("")).is_some());
    }

    #[test]
    fn test_local_reverse_drops_under_global() {
        // init.c:239-242: a local `r` does not invert a global `-r`; the
        // key parses without reversal.
        let global = opts("-r");
        let key = parse_key("1r", &global).unwrap();
        assert!(!key.reverse);
    }

    #[test]
    fn test_fields_split_on_blank_runs() {
        let global = opts("");
        let spec = parse_key("2,2", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        assert_eq!(compare("aa b cc", "xx y zz", &options), Ordering::Less);
        assert_eq!(compare("a y z", "b x w", &options), Ordering::Greater);
    }

    #[test]
    fn test_tie_falls_back_to_whole_line() {
        let global = opts("");
        let spec = parse_key("1,1.1", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        // The one-byte key ties; the whole line breaks the tie.
        assert_eq!(compare("a123", "a456", &options), Ordering::Less);
    }

    #[test]
    fn test_parse_options_rejects_unknown() {
        assert_eq!(parse_options(&["-Z"]), Err(ParseError::BadOption('Z')));
    }
}
