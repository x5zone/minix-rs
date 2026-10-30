//! Character set translation, deletion, and squeezing behind `tr`.
//!
//! Ground truth: `minix3/usr.bin/tr/tr.c` (283 lines; usage collected
//! around lines 58 to 125). Three operations over two character sets:
//!
//! - Translation: each byte in set 1 becomes the corresponding byte of set
//!   2 (a short set 2 repeats its last byte, matching the C tool).
//! - Deletion (`-d`): bytes in set 1 vanish.
//! - Squeezing (`-s`): runs of bytes in the squeeze set collapse to one.
//!
//! Sets are built from literals, ranges (`a-z`), repeats (`[a*5]`), and
//! classes (`[:alpha:]` and friends — recognised by name, ASCII only).
//! Membership tests go through [`CharClass`] with one implementation per
//! set shape, so callers never special case ranges versus lists.

use alloc::vec;
use alloc::vec::Vec;

use crate::TextError;

/// Maximum members in an explicit byte list set.
pub const MAX_MEMBERS: usize = 64;

/// One character set flavour: how bytes join the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    /// Explicit member bytes (up to 64).
    List(CharList),
    /// Inclusive byte range.
    Range(u8, u8),
    /// Named class (`alpha`, `digit`, `space`, `upper`, `lower`, `alnum`).
    Named(NamedClass),
}

/// Explicit member list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharList {
    /// Member bytes.
    pub members: [u8; MAX_MEMBERS],
    /// How many of `members` are used.
    pub count: u8,
}

/// Named character classes (ASCII only, C locale behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedClass {
    /// Letters.
    Alpha,
    /// Letters and digits.
    Alnum,
    /// Digits.
    Digit,
    /// Blanks (space and tab).
    Blank,
    /// Whitespace (space, tab, newline, carriage return, vertical tab,
    /// form feed).
    Space,
    /// Uppercase letters.
    Upper,
    /// Lowercase letters.
    Lower,
}

impl CharClass {
    /// Decide whether `byte` belongs to the set.
    pub fn contains(self, byte: u8) -> bool {
        match self {
            CharClass::List(list) => list.members[..list.count as usize].contains(&byte),
            CharClass::Range(low, high) => low <= byte && byte <= high,
            CharClass::Named(class) => match class {
                NamedClass::Alpha => byte.is_ascii_alphabetic(),
                NamedClass::Alnum => byte.is_ascii_alphanumeric(),
                NamedClass::Digit => byte.is_ascii_digit(),
                NamedClass::Blank => matches!(byte, b' ' | b'\t'),
                NamedClass::Space => matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C),
                NamedClass::Upper => byte.is_ascii_uppercase(),
                NamedClass::Lower => byte.is_ascii_lowercase(),
            },
        }
    }
}

/// Parse one set expression (`abc`, `a-z`, `[a*5]`, `[:alpha:]`, escapes
/// like `\n`) into up to 8 classes, expanding to at most 256 member bytes
/// in `expanded` (returns the used count).
///
/// Repeat counts that would overflow the 256 table are an error, matching
/// the C tool refusing absurd sets rather than wrapping.
/// C's `str2set()` carries an `s->which` (`minix3/usr.bin/tr/str.c`) that decides
/// how far the same characters are legal in each set: a repeat `[c*n]` is legal
/// only in string2. The shell side makes it a parameter too, rather than letting
/// both sets share one undifferentiated parse. Which set is being parsed; C's
/// `STRING1`/`STRING2`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetSite {
    String1,
    String2,
}

/// The `errx(1, ...)` failures `str.c` raises from its own grammar. The sentences
/// are copied verbatim from C, because `errx` is a two-part form (program name
/// plus sentence, no `strerror` suffix), and C exits on the spot at every one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SetGrammar {
    /// `errx(1, "unknown class %.*s", (int)len, class)`（`str.c:253`）。
    UnknownClass(Vec<u8>),
    /// The first check in `genseq()`:
    /// `if (s->which == STRING1) errx(1, "Sequences only valid in string2")`
    /// (`str.c:353-356`) — it runs before consuming any character, so whatever the
    /// second character is makes no difference.
    SequencesOnlyInString2,
    /// After taking the member character,
    /// `if (*s->str != '*') errx(1, "Misplaced sequence asterisk")`
    /// (`str.c:362-366`). So `[ab*2]` reports this sentence: what follows `a` is
    /// `b`, not `*`.
    MisplacedSequenceAsterisk,
    /// When the count is neither `]` nor "digits immediately followed by `]`":
    /// `errx(1, "illegal sequence count")` (`str.c:368-386`).
    IllegalSequenceCount,
    /// After `genequiv()` takes the member character,
    /// `if (*s->str != '=') errx(1, "Misplaced equivalence equals sign")`
    /// (`str.c:307-320`). The member consumes **one** character (or one backslash
    /// escape), so `[=ab=]`, `[==]` and `[=a b=]` all hit this sentence — whether
    /// the form ends with `=]` was recognized beforehand by `bracket()`
    /// (`str.c:193-199`); it recognizes the form, and `genequiv()` checks inside it.
    MisplacedEquivalenceEquals,
    /// After the equals sign,
    /// `if (*s->str != ']') errx(1, "Misplaced equivalence right bracket")`
    /// (`str.c:321-323`). `[=a=b=]` takes this sentence: the first `=]` that
    /// `bracket()` finds is at the end of the form, while `genequiv()` walks through
    /// `a` and `=` and then hits `b`.
    MisplacedEquivalenceRightBracket,
}

impl SetGrammar {
    /// The exact sentence C prints (without the program name and colon; those two
    /// parts are `errx`'s prefix job).
    pub fn sentence_bytes(&self) -> Vec<u8> {
        match self {
            SetGrammar::UnknownClass(name) => {
                let mut out = b"unknown class ".to_vec();
                out.extend_from_slice(name);
                out
            }
            SetGrammar::SequencesOnlyInString2 => b"Sequences only valid in string2".to_vec(),
            SetGrammar::MisplacedSequenceAsterisk => b"Misplaced sequence asterisk".to_vec(),
            SetGrammar::IllegalSequenceCount => b"illegal sequence count".to_vec(),
            SetGrammar::MisplacedEquivalenceEquals => b"Misplaced equivalence equals sign".to_vec(),
            SetGrammar::MisplacedEquivalenceRightBracket => {
                b"Misplaced equivalence right bracket".to_vec()
            }
        }
    }
}

/// Every failure exit of parsing one set expression: grammar sentences plus the two
/// not-yet-shaped cases (unclosed form, full member table).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SetError {
    Grammar(SetGrammar),
    /// `[...)` with no closing bracket, or `[:name:]` with no terminator — whether C
    /// keeps consuming these two shapes as literals or reports another sentence has
    /// not been checked line by line, so the existing silent exit 1 remains
    /// (registered in the document-18 ledger).
    Malformed,
    /// More than 256 members, i.e. `TextError::TooLong`.
    TooLong,
}

impl From<TextError> for SetError {
    fn from(error: TextError) -> Self {
        match error {
            TextError::TooLong => SetError::TooLong,
            TextError::InvalidArgument => SetError::Malformed,
        }
    }
}

/// The class names recognized in C's name table (`classes[]` in `str.c`); a name
/// outside the table is always `unknown class <name>`, never "make do as literals".
pub fn parse_set(
    text: &[u8],
    site: SetSite,
    expanded: &mut [u8; 256],
) -> Result<(usize, Set2Tail), SetError> {
    let bytes = text;
    // The kind of the expression's last item decides what pads string2 once it is
    // exhausted (see `Set2Tail`).
    let mut tail = Set2Tail::LastByte;
    let mut pos = 0;
    let mut used = 0;
    while pos < bytes.len() {
        // `next()` asks `bracket()` as soon as it sees `[`; if that returns false it
        // FALLTHROUGHs on the spot to a **literal left bracket** (`str.c:124-132`).
        // So the shapes that look special but are not formed — `[x]`, `[:alpha`,
        // `[x`, `[x]y*` — are ordinary characters in C, exiting 0. These shape
        // predicates must be copied one by one from `bracket()`'s switch
        // (`str.c:176-210`); missing one adds a class of false errors.
        let class_end = if marker_at(bytes, pos, b':') {
            find_pair(bytes, pos + 2, b':', b']')
        } else {
            None
        };
        let equiv_end = if marker_at(bytes, pos, b'=') {
            find_pair(bytes, pos + 2, b'=', b']')
        } else {
            None
        };
        let sequence_form = bytes[pos] == b'['
            && pos + 1 < bytes.len()
            && !matches!(bytes[pos + 1], b':' | b'=')
            && sequence_ahead(bytes, pos + 2);
        if let Some(end) = class_end {
            let name = &text[pos + 2..end];
            let class = match name {
                b"alpha" => NamedClass::Alpha,
                b"alnum" => NamedClass::Alnum,
                b"digit" => NamedClass::Digit,
                b"blank" => NamedClass::Blank,
                b"space" => NamedClass::Space,
                b"upper" => NamedClass::Upper,
                b"lower" => NamedClass::Lower,
                _ => return Err(SetError::Grammar(SetGrammar::UnknownClass(name.to_vec()))),
            };
            tail = Set2Tail::Oobch;
            for byte in 0..=255u16 {
                let byte = byte as u8;
                if CharClass::Named(class).contains(byte) {
                    push_byte(expanded, &mut used, byte)?;
                }
            }
            pos = end + 2;
        } else if let Some(end) = equiv_end {
            // `[=c=]`: `bracket()` (`str.c:193-199`) only checks that the expression
            // contains a `=]`; once recognized, `genequiv()` (`str.c:305-330`) checks
            // the inside character by character — the member consumes **one**
            // character (or one backslash escape), then there must be `=`, then there
            // must be `]`. C's comment states
            // "English doesn't have any equivalence classes, so for now we just
            // syntax check and grab the character", so this class always has exactly
            // one member, and any extra character inside the form triggers one of the
            // two sentences "misplaced equals sign" / "misplaced right bracket".
            let mut cursor = pos + 2;
            let member = if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                let escaped = bytes[cursor + 1];
                cursor += 2;
                escape_literal(escaped)
            } else {
                let ch = bytes[cursor];
                cursor += 1;
                ch
            };
            if bytes.get(cursor) != Some(&b'=') {
                return Err(SetError::Grammar(SetGrammar::MisplacedEquivalenceEquals));
            }
            cursor += 1;
            if bytes.get(cursor) != Some(&b']') {
                return Err(SetError::Grammar(
                    SetGrammar::MisplacedEquivalenceRightBracket,
                ));
            }
            tail = Set2Tail::Oobch;
            push_byte(expanded, &mut used, member)?;
            // Both checks passed, so the `=]` that `bracket()` recognized must be
            // the two bytes just checked; closing still uses `end + 2` (same shape as
            // C's `s->str = p + 2`, `str.c:200`).
            pos = end + 2;
        } else if sequence_form {
            // The **order** of the three sentences is itself behavior; copy
            // `genseq()`: first ask which string this is (before consuming any
            // character, `str.c:353-356`), then take the member character, only then
            // check the asterisk (`:362-366`), and only then the count (`:368-386`).
            // So `[c*n]` in string1 never gets to discuss its contents, while
            // `[ab*2]` in string2 reports the asterisk sentence, not "illegal count".
            if site == SetSite::String1 {
                return Err(SetError::Grammar(SetGrammar::SequencesOnlyInString2));
            }
            let mut cursor = pos + 1;
            let member = if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                let escaped = bytes[cursor + 1];
                cursor += 2;
                escape_literal(escaped)
            } else {
                let ch = bytes[cursor];
                cursor += 1;
                ch
            };
            if bytes.get(cursor) != Some(&b'*') {
                return Err(SetError::Grammar(SetGrammar::MisplacedSequenceAsterisk));
            }
            cursor += 1;
            let repeat = match bytes.get(cursor) {
                // Both `case ']'` and "a digit whose value is 0" land on the INFINITE
                // side of `s->state = s->cnt ? SEQUENCE : INFINITE`: `[x*]` and `[x*0]`
                // mean **infinite** repetition, not an empty set.
                // This layer renders infinity as one member — `translate()` repeats
                // the last byte of a short set2 (`tr.c:205-211`), which is exactly the
                // effect of INFINITE on the translation path.
                Some(b']') => {
                    cursor += 1;
                    1
                }
                _ => {
                    let start = cursor;
                    while matches!(bytes.get(cursor), Some(ch) if ch.is_ascii_digit()) {
                        cursor += 1;
                    }
                    if cursor == start || bytes.get(cursor) != Some(&b']') {
                        return Err(SetError::Grammar(SetGrammar::IllegalSequenceCount));
                    }
                    cursor += 1;
                    let mut count = 0usize;
                    for digit in &bytes[start..cursor - 1] {
                        count = count
                            .checked_mul(10)
                            .and_then(|v| v.checked_add((digit - b'0') as usize))
                            .ok_or(SetError::Malformed)?;
                    }
                    if count == 0 { 1 } else { count }
                }
            };
            tail = Set2Tail::LastByte;
            for _ in 0..repeat {
                push_byte(expanded, &mut used, member)?;
            }
            pos = cursor;
        } else if pos + 2 < bytes.len() && bytes[pos + 1] == b'-' {
            let (low, high) = (bytes[pos], bytes[pos + 2]);
            if high < low {
                // `genrange()`'s
                // `if (stopval < s->lastch) { s->str = savestart; return 0; }`
                // hands the whole range back to the main loop, so the left endpoint
                // enters the set as a literal and the next round starts at `-`:
                // `tr z-a y` has set1 = z, -, a (three members) and exits 0, not
                // "illegal range".
                tail = Set2Tail::LastByte;
                push_byte(expanded, &mut used, low)?;
                pos += 1;
                continue;
            }
            tail = Set2Tail::LastByte;
            let mut byte = low;
            loop {
                push_byte(expanded, &mut used, byte)?;
                if byte == high {
                    break;
                }
                byte += 1;
            }
            pos += 3;
        } else if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
            let byte = escape_literal(bytes[pos + 1]);
            tail = Set2Tail::LastByte;
            push_byte(expanded, &mut used, byte)?;
            pos += 2;
        } else {
            tail = Set2Tail::LastByte;
            push_byte(expanded, &mut used, bytes[pos])?;
            pos += 1;
        }
    }
    Ok((used, tail))
}

/// Whether the byte right after this `[` is that special marker (`bracket()`'s
/// `switch (s->str[1])`).
#[must_use]
fn marker_at(bytes: &[u8], pos: usize, marker: u8) -> bool {
    bytes.get(pos) == Some(&b'[') && bytes.get(pos + 1) == Some(&marker)
}

/// Find the paired terminator `ab` starting at `from` (`bracket()`'s
/// `strstr(s->str + 2, ":]")` and `strstr(s->str + 2, "=]")`), returning the index
/// of `a`; not finding one means this `[` is not a special form.
#[must_use]
fn find_pair(bytes: &[u8], from: usize, a: u8, b: u8) -> Option<usize> {
    (from..bytes.len().saturating_sub(1)).find(|&i| bytes[i] == a && bytes[i + 1] == b)
}

/// The sequence form requires that `strpbrk(s->str + 2, "*]")` hits **`*` first**
/// and that a `]` follows that `*` (`str.c:200-206`). Missing either one, C treats
/// this `[` as a literal: `[x]` and `[x]y*` fall back to ordinary characters
/// because `]` is hit first, and `[x` falls back because there is nothing after it.
#[must_use]
fn sequence_ahead(bytes: &[u8], from: usize) -> bool {
    let hit = (from..bytes.len())
        .position(|i| bytes[i] == b'*' || bytes[i] == b']')
        .map(|i| i + from);
    match hit {
        Some(i) => bytes[i] == b'*' && bytes[i..].contains(&b']'),
        None => false,
    }
}

/// What the character after `\` stands for — of `backslash()`'s escape table this
/// branch wires up only the three named forms; the octal part (the loop at
/// `str.c:398-415`) is registered as `TR-BACKSLASH-OCTAL` and not wired.
#[must_use]
fn escape_literal(escaped: u8) -> u8 {
    match escaped {
        b'n' => b'\n',
        b't' => b'\t',
        b'r' => b'\r',
        other => other,
    }
}

/// C's `#define OOBCH (UCHAR_MAX + 1)` at `extern.h:39`: a value that **does not
/// fit the byte table**, used as a sentinel. This layer keeps it in the `u16`
/// domain (the squeeze predicate in `translate_squeeze` compares against exactly
/// this 256) and truncates it to `unsigned char`, i.e. `0`, only when writing out.
const OOBCH: u16 = 256;

/// Whether `next()` still yields anything new after the expression is exhausted —
/// the "repeat the last byte" of the comment at `tr.c:226-232` holds only for
/// **ordinary** expressions; one ending in a character class yields `OOBCH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set2Tail {
    /// The last item is an ordinary character, a range or `[x*n]`: `lastch` rests on
    /// that member, so exhaustion repeats the last byte.
    LastByte,
    /// The last item is `[:class:]` or `[=c=]`: the `SET` branch at
    /// `str.c:158-166` does `s->lastch = s->set[s->cnt++]`, and on reaching the
    /// sentinel it pushes `lastch` to `OOBCH`; the `case '\0'` branch then yields
    /// this 256 (`:103-121`). `tr.c:220` stores it in the mapping table and
    /// `putchar` truncates it to `0`; the `-s` leg compares in the `int` domain, so
    /// 256 and an ordinary 0 are two different things.
    Oobch,
}

/// Append one expanded member byte; the 256 table bounds every set.
fn push_byte(expanded: &mut [u8; 256], used: &mut usize, byte: u8) -> Result<(), TextError> {
    if *used >= expanded.len() {
        return Err(TextError::TooLong);
    }
    expanded[*used] = byte;
    *used += 1;
    Ok(())
}

/// C's mapping table: `int string1[NCHARS]` (`tr.c:55`) is filled with the identity
/// first (`:178-181`), then each position of string1 gets the value yielded by
/// string2 (`:217`). **The value domain is `int`, not bytes**, because a character
/// class yields `OOBCH` (256) once exhausted; `translate` and
/// `translate_and_squeeze` share this table, differing only in whether the second
/// half squeezes by "values that were yielded".
fn build_map(set1: &[u8], set2: &[u8], tail: Set2Tail) -> [u16; 256] {
    let mut map = [0u16; 256];
    for (index, slot) in map.iter_mut().enumerate() {
        *slot = index as u16;
    }
    for (index, byte) in set1.iter().enumerate() {
        map[*byte as usize] = if index < set2.len() {
            u16::from(set2[index])
        } else {
            // string2 is shorter than string1: C does not take a new value but keeps
            // repeating the `lastch` that `next()` held (exactly what the comment at
            // `tr.c:226-232` describes). Ordinary expressions stop on the last byte,
            // character classes stop on OOBCH.
            match tail {
                Set2Tail::Oobch => OOBCH,
                Set2Tail::LastByte => u16::from(*set2.last().unwrap_or(&0)),
            }
        };
    }
    map
}

/// Translate `input` through two expanded sets into `out`.
///
/// `set1`/`set2` are the expanded member lists; a short second set repeats its
/// last byte **unless it ends with a character class**, in which case C repeats
/// `OOBCH` and `putchar` writes that as a NUL byte. Returns the used byte count.
///
/// **An empty `set1` is legal input** and goes through the identity map: on the
/// non-complement path C first fills the mapping table with `string1[ch] = ch`
/// (`tr.c:178-181`), then the `next(s1)` for string1's first element fails on the
/// empty set and breaks out of the mapping loop (`:214-215`), so an identity table
/// nobody modified is used for filtering as-is — hence the reference binary
/// `tr '' x` "exits 0 and copies stdin byte for byte". An empty `set2` is not in
/// this exception: C rejects it as `empty string2` with `next(s2)` before the loop
/// (`:187-188`); this layer's check lives in the shell.
pub fn translate(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    tail: Set2Tail,
    out: &mut [u8],
) -> Result<usize, TextError> {
    if set2.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    let map = build_map(set1, set2, tail);
    if input.len() > out.len() {
        return Err(TextError::TooLong);
    }
    for (index, byte) in input.iter().enumerate() {
        // `putchar(string1[ch])`: C's argument is an `int`, truncated to
        // `unsigned char` at `putchar`, so writing OOBCH (256) emits `\0` (the
        // reference binary `tr abc '[=c=]'` ← `abc` yields `c\0\0`).
        out[index] = map[*byte as usize] as u8;
    }
    Ok(input.len())
}

/// The `tr [-c] -s string1 string2` leg (`tr.c:239-246`): **mapping and squeezing
/// happen in the same pass**, and the squeeze predicate compares the `int` value
/// **after** mapping — `lastch` starts as `OOBCH`, and what gets recorded in
/// `string2[]` is exactly the values yielded along the way (`tr.c:224` records
/// them in the mapping pass).
///
/// Together these explain a seemingly contradictory case: `tr -s abc '[=c=]'` ←
/// `abc` yields `c\0`, i.e. the two NULs that came from OOBCH **squeeze into
/// one another**, while NULs already present in the input are unaffected.
/// Splitting this leg into "translate, then squeeze" loses the distinction —
/// between the two steps 256 has already been truncated to 0, so 0 and a real NUL
/// become indistinguishable and the squeeze predicate moves into the byte domain.
pub fn translate_and_squeeze(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    tail: Set2Tail,
    out: &mut [u8],
) -> Result<usize, TextError> {
    if set2.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    let map = build_map(set1, set2, tail);
    // C's `string2[ch2] = 1` sits inside the mapping loop (`tr.c:222-224`) and
    // records **values that were yielded**; the domain includes the sentinel, so
    // the table is one slot wider than the map.
    let mut yielded = [false; 257];
    for byte in set1 {
        yielded[map[*byte as usize] as usize] = true;
    }
    if input.len() > out.len() {
        return Err(TextError::TooLong);
    }
    let mut written = 0;
    let mut lastch = OOBCH;
    for byte in input {
        let ch = map[*byte as usize];
        if !yielded[ch as usize] || lastch != ch {
            lastch = ch;
            out[written] = ch as u8;
            written += 1;
        }
    }
    Ok(written)
}

/// Delete every byte of `set` from `input` into `out`.
pub fn delete(input: &[u8], set: &[u8], out: &mut [u8]) -> Result<usize, TextError> {
    let mut table = [false; 256];
    for byte in set {
        table[*byte as usize] = true;
    }
    let mut written = 0;
    for byte in input {
        if !table[*byte as usize] {
            if written >= out.len() {
                return Err(TextError::TooLong);
            }
            out[written] = *byte;
            written += 1;
        }
    }
    Ok(written)
}

/// Squeeze runs of `set` bytes in `input` down to one each.
pub fn squeeze(input: &[u8], set: &[u8], out: &mut [u8]) -> Result<usize, TextError> {
    let mut table = [false; 256];
    for byte in set {
        table[*byte as usize] = true;
    }
    let mut written = 0;
    let mut previous: Option<u8> = None;
    for byte in input {
        if table[*byte as usize] && previous == Some(*byte) {
            continue;
        }
        if written >= out.len() {
            return Err(TextError::TooLong);
        }
        out[written] = *byte;
        written += 1;
        previous = Some(*byte);
    }
    Ok(written)
}

/// The option loop at `tr.c:68`: `while ((ch = getopt(argc, argv, "cds")) != -1)` —
/// `c`/`d`/`s` are **all flags**, so the "takes a value / missing value" branch of
/// `getopt.c:108-140` is **unreachable** here; the only reachable libc report is
/// "unknown option letter" (`getopt.c:98-106`). More importantly,
/// `tr.c:79-81`'s `case '?': default: usage();` **sits in the loop and exits
/// immediately**, so this side emits **at most one** libc report (the reference
/// binary reports only `Z` for `-ZQ`, and only `Z` for `-sZ a`), the opposite of
/// `seq`'s accumulating form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrArgs<'a> {
    /// `-c`: complement set1.
    pub complement: bool,
    /// `-d`: delete the bytes in set1.
    pub delete: bool,
    /// `-s`: squeeze.
    pub squeeze: bool,
    /// The loop does not reorder: the first non-option token and everything after it
    /// **are operands** (`argc -= optind; argv += optind;` at `tr.c:83-84`). So the
    /// `-d` in `tr a -d` is the second set name, not a flag.
    pub operands: Vec<&'a [u8]>,
}

/// Front-door errors: every failure exit here prints the **same four-line usage
/// block** and exits 1 (`tr.c:276-282`); the only difference is whether a libc
/// report line precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrError {
    /// The libc line (at most one) plus the whole usage block, sent to standard
    /// error by the shell.
    Usage { reports: Vec<crate::GetoptReport> },
}

/// The loop, the count check and the combination check, in the **same order** as
/// `tr.c`: an off-string letter in the loop ⇒ usage with a libc line;
/// the `switch (argc)` at `tr.c:86-97` rejects 0 and 3+; `-ds` needs two operands
/// (`:104-106`); `-d` with two is an error (`:123-125`); with neither `-d` nor
/// `-s` there must be exactly two (`:156-157`).
pub fn parse_tr_args<'a>(args: &[&'a [u8]]) -> Result<TrArgs<'a>, TrError> {
    const OPTSTRING: &[u8] = b"cds";
    let mut parsed = TrArgs {
        complement: false,
        delete: false,
        squeeze: false,
        operands: Vec::new(),
    };
    let mut i = 0usize;
    while i < args.len() {
        let token = args[i];
        // `getopt.c:74-78`: a first character other than `-` ends the loop and does
        // **not** advance the cursor; in `getopt.c:86-93` a bare `-` takes the same
        // route because optstring has no `-`, so it lands among the operands.
        if token.len() < 2 || token[0] != b'-' {
            break;
        }
        // `getopt.c:80-85`: **exactly** two dashes are recognized as the terminator,
        // and it consumes itself.
        if token == b"--" {
            i += 1;
            break;
        }
        let mut k = 1usize;
        while k < token.len() {
            let letter = token[k];
            k += 1;
            if !OPTSTRING.contains(&letter) {
                // `getopt.c:98-106` prints one line, and `tr.c:79-81`'s
                // `default: usage()` exits right after
                return Err(TrError::Usage {
                    reports: vec![crate::GetoptReport::UnknownOption(char::from(letter))],
                });
            }
            match letter {
                b'c' => parsed.complement = true,
                b'd' => parsed.delete = true,
                _ => parsed.squeeze = true,
            }
            // None of the three letters takes a value, so advance to the next token
            // only when this token is exhausted
            if k >= token.len() {
                i += 1;
            }
        }
    }
    parsed.operands = args[i..].to_vec();
    let count = parsed.operands.len();
    // `tr.c:86-97`: both 0 and 3+ only fall into `default: usage();`
    if count == 0 || count > 2 {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    let two_sets = count == 2;
    // The three checks below correspond one-to-one, in the **original order**, to
    // three pieces of C code; each is one of C's `if` statements: `-ds` missing
    // string2 (`:104-106`), `-d` with an extra string2 (`:123-125`), and the
    // translation path without string2 (`:156-157`). `-s` with a single operand is
    // its own case at `:139-140`, so it does not enter any of them.
    if parsed.delete && parsed.squeeze && !two_sets {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    if parsed.delete && !parsed.squeeze && two_sets {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    if !parsed.delete && !parsed.squeeze && !two_sets {
        return Err(TrError::Usage {
            reports: Vec::new(),
        });
    }
    Ok(parsed)
}

/// The **four lines** of `tr.c:276-282`: the first starts with `usage: `, the other
/// three are aligned continuation lines with seven spaces; the block has no
/// program name placeholder (C hard-codes `tr` in the string) and goes to
/// **standard error**.
/// `concat!` is used because Rust's string continuation would eat the next line's
/// leading whitespace, and those seven spaces are the alignment itself.
#[must_use]
pub const fn usage_block() -> &'static [u8] {
    concat!(
        "usage: tr [-cs] string1 string2\n",
        "       tr [-c] -d string1\n",
        "       tr [-c] -s string1\n",
        "       tr [-c] -ds string1 string2\n"
    )
    .as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The hop from the text fixtures to the byte front door: the byte form of a
    /// `str` literal is what the command line carries.
    fn argv_of<'w>(list: &[&'w str]) -> Vec<&'w [u8]> {
        list.iter().map(|word| word.as_bytes()).collect()
    }

    fn tr_args<'w>(list: &[&'w str]) -> Result<TrArgs<'w>, TrError> {
        parse_tr_args(&argv_of(list))
    }

    fn expanded_set(text: &str) -> ([u8; 256], usize) {
        let mut expanded = [0u8; 256];
        let (used, _) = parse_set(text.as_bytes(), SetSite::String2, &mut expanded).unwrap();
        (expanded, used)
    }

    /// Only the tail predicate: how the expression ends decides what pads string2
    /// once it is exhausted.
    fn tail_of(text: &str) -> Set2Tail {
        let mut expanded = [0u8; 256];
        parse_set(text.as_bytes(), SetSite::String2, &mut expanded)
            .unwrap()
            .1
    }

    /// What pads string2 once exhausted depends on the **expression's last item**,
    /// not on whether the expression contains a class anywhere: `[:digit:]x` stops
    /// on an ordinary character (repeats `x`), `x[:digit:]` stops on a class
    /// (repeats OOBCH).
    /// All eight shapes were checked against the reference binary
    /// (`tmp/tr-343/ref-padding.log`).
    #[test]
    fn only_a_class_at_the_end_changes_what_exhaustion_yields() {
        assert_eq!(tail_of("[=c=]"), Set2Tail::Oobch, "等价类收尾");
        assert_eq!(tail_of("[:digit:]"), Set2Tail::Oobch, "名字类收尾");
        assert_eq!(tail_of("x[:digit:]"), Set2Tail::Oobch, "集合类是最后一项");
        assert_eq!(tail_of("[=c=][=d=]"), Set2Tail::Oobch, "两枚等价类");
        assert_eq!(
            tail_of("[:digit:]x"),
            Set2Tail::LastByte,
            "集合类之后还有普通字符"
        );
        assert_eq!(tail_of("xy"), Set2Tail::LastByte, "普通两枚");
        assert_eq!(tail_of("a-z"), Set2Tail::LastByte, "区间收尾");
        assert_eq!(tail_of("[x*2]"), Set2Tail::LastByte, "重复式收尾");
    }

    /// Once a character class exhausts string2 the map stores OOBCH and the output
    /// is NUL; a plain short string2 keeps repeating its last byte. One case of each
    /// is compared; this is exactly the cell they differ in.
    #[test]
    fn the_class_tail_maps_to_nul_bytes_while_a_plain_tail_repeats_its_last_byte() {
        let mut out = [0u8; 32];
        let (set1, len1) = expanded_set("abcdefghijklmnop");
        let (digits, len2) = expanded_set("[:digit:]");
        let len = translate(
            b"abcdefghijklmnop",
            &set1[..len1],
            &digits[..len2],
            tail_of("[:digit:]"),
            &mut out,
        )
        .unwrap();
        assert_eq!(
            &out[..len],
            b"0123456789\0\0\0\0\0\0",
            "十枚数字之后是六枚 NUL（OOBCH 被 putchar 截成 0）"
        );

        let (plain, plain_len) = expanded_set("xy");
        let (set3, len3) = expanded_set("abc");
        let len = translate(
            b"abc",
            &set3[..len3],
            &plain[..plain_len],
            tail_of("xy"),
            &mut out,
        )
        .unwrap();
        assert_eq!(&out[..len], b"xyy", "普通式子仍按末字节重复");
    }

    /// The `-s` leg squeezes in the `int` domain: adjacent NULs that came from OOBCH
    /// **collapse into one**, while NULs already in the input do not participate
    /// (they map to themselves and were never recorded in the "yielded" table).
    /// Splitting this leg into translate + squeeze conflates the two — between those
    /// steps 256 is already gone. All four expected values come from the reference
    /// binary (`tmp/tr-390/ab.log`, 26 shapes with 0 mismatches): the first run of
    /// this gate wrote its own expectation wrong (`c\0\0`), and the reference's
    /// `c\0\0\0` corrected it.
    #[test]
    fn the_squeeze_leg_collapses_padded_nuls_but_leaves_a_real_nul_alone() {
        let mut out = [0u8; 32];
        let (set1, len1) = expanded_set("abc");
        let (class, len2) = expanded_set("[=c=]");
        for (input, expected) in [
            (&b"abc"[..], &b"c\0"[..]),
            (&b"abbc"[..], &b"c\0"[..]),
            (&b"ab\0c"[..], &b"c\0\0\0"[..]),
            (&b"a\0b\0c"[..], &b"c\0\0\0\0"[..]),
            // `tr.c:240`: `lastch` starts as `OOBCH`, so the leading OOBCH also counts
            // as "same as the previous one" and is squeezed away — initializing it to
            // any real byte would print one extra `\0` (the reference `tr -s ab [=c=]`
            // ← `b` yields the **empty string**; the first version of this gate did not
            // test this cell, so the negative control passed vacuously).
            (&b"b"[..], &b""[..]),
        ] {
            let len = translate_and_squeeze(
                input,
                &set1[..len1],
                &class[..len2],
                tail_of("[=c=]"),
                &mut out,
            )
            .unwrap();
            assert_eq!(&out[..len], expected, "{input:?}");
        }
    }

    /// This layer used to reserve eight slots for the class table and let **every**
    /// member occupy one, so nine literals (`tr 'abcdefghi' 'x'`) were a silent
    /// exit 1 — C has no such limit: literals, ranges and escapes are just an
    /// assignment to `lastch`; only `[:class:]`/`[=c=]` use the SET state. The
    /// reference binary on the same shape yields `xxx` and exits 0.
    #[test]
    fn a_literal_set_is_bounded_by_256_members_not_by_an_eight_slot_class_table() {
        for text in [
            "abcdefghi",
            "abcdefghijklmnopqrstuvwxyz",
            "1234567890 abcdefgh",
        ] {
            let (expanded, used) = expanded_set(text);
            assert_eq!(used, text.len(), "{text:?} 该按字面量整串展开");
            assert_eq!(
                &expanded[..used],
                text.as_bytes(),
                "{text:?} 成员顺序照式子"
            );
        }
        let mut out = [0u8; 8];
        let (set1, len1) = expanded_set("abcdefghi");
        let (set2, len2) = expanded_set("x");
        let len = translate(b"abc", &set1[..len1], &set2[..len2], tail_of("x"), &mut out).unwrap();
        assert_eq!(&out[..len], b"xxx", "九枚成员不是错误，是一枚都不少地映射");
    }

    /// Only the failure exit: all grammar-sentence assertions in this module go
    /// through here, at the string2 site (the side where repeats are legal).
    fn set_error(text: &str) -> SetError {
        let mut expanded = [0u8; 256];
        parse_set(text.as_bytes(), SetSite::String2, &mut expanded)
            .err()
            .unwrap_or_else(|| panic!("{text:?} 本应报错"))
    }

    fn grammar(text: &str, site: SetSite) -> SetGrammar {
        let mut expanded = [0u8; 256];
        match parse_set(text.as_bytes(), site, &mut expanded) {
            Err(SetError::Grammar(g)) => g,
            other => panic!("{text:?} 在 {site:?} 应为语法句，实得 {other:?}"),
        }
    }

    #[test]
    fn test_translate_basic() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("XYZ");
        let mut out = [0u8; 16];
        let len = translate(
            b"axbxc",
            &set1[..len1],
            &set2[..len2],
            Set2Tail::LastByte,
            &mut out,
        )
        .unwrap();
        // Only set members translate; `x` passes through twice.
        assert_eq!(&out[..len], b"XxYxZ");
    }

    #[test]
    fn test_empty_first_set_is_the_identity_map() {
        // `tr.c:178-181` first fills the map with the identity, and the empty
        // string1 makes the `:214-215` loop never run, so no position was ever
        // rewritten — hence the reference `tr '' x` copies stdin byte for byte and
        // exits 0
        let (set2, len2) = expanded_set("XYZ");
        let mut out = [0u8; 16];
        let len = translate(b"aXb", &[], &set2[..len2], Set2Tail::LastByte, &mut out).unwrap();
        assert_eq!(&out[..len], b"aXb");
    }

    #[test]
    fn test_short_second_set_repeats_last() {
        let (set1, len1) = expanded_set("abc");
        let (set2, len2) = expanded_set("X");
        let mut out = [0u8; 16];
        let len = translate(
            b"abc",
            &set1[..len1],
            &set2[..len2],
            Set2Tail::LastByte,
            &mut out,
        )
        .unwrap();
        assert_eq!(&out[..len], b"XXX");
    }

    #[test]
    fn test_range_and_repeat_sets() {
        let (set1, len1) = expanded_set("a-c");
        assert_eq!(len1, 3);
        let (set2, len2) = expanded_set("[x*2]");
        assert_eq!((len2, set2[0], set2[1]), (2, b'x', b'x'));
        let _ = set1;
    }

    #[test]
    fn test_named_class() {
        let (set, len) = expanded_set("[:digit:]");
        assert_eq!(len, 10);
        assert!(set[..len].contains(&b'5'));
    }

    #[test]
    fn test_delete_and_squeeze() {
        let (vowels, vowels_len) = expanded_set("aeiou");
        let mut out = [0u8; 16];
        let len = delete(b"hello", &vowels[..vowels_len], &mut out).unwrap();
        assert_eq!(&out[..len], b"hll");
        let (spaces, spaces_len) = expanded_set(" ");
        let len = squeeze(b"a  b   c", &spaces[..spaces_len], &mut out).unwrap();
        assert_eq!(&out[..len], b"a b c");
    }

    #[test]
    fn test_class_strategies_agree() {
        let list = CharClass::List(CharList {
            members: {
                let mut members = [0u8; MAX_MEMBERS];
                members[0] = b'a';
                members[1] = b'b';
                members
            },
            count: 2,
        });
        let range = CharClass::Range(b'a', b'b');
        for byte in 0..=255u16 {
            assert_eq!(list.contains(byte as u8), range.contains(byte as u8));
        }
    }

    #[test]
    fn a_reversed_range_is_handed_back_as_literals() {
        // genrange()'s `stopval < lastch` rewinds the pointer to `savestart` and
        // returns 0, so the main loop consumes literals: `tr z-a y` has the set
        // z, -, a (three members) and exits 0, not "illegal range"
        let (set, len) = expanded_set("z-a");
        assert_eq!(len, 3);
        assert_eq!(&set[..len], [b'z', b'-', b'a']);
    }

    #[test]
    fn an_unknown_class_says_which_name_it_rejected() {
        assert_eq!(
            set_error("[:nope:]"),
            SetError::Grammar(SetGrammar::UnknownClass("nope".as_bytes().to_vec()))
        );
        assert_eq!(
            SetGrammar::UnknownClass("nope".as_bytes().to_vec()).sentence_bytes(),
            b"unknown class nope".as_slice()
        );
    }

    #[test]
    fn a_repeat_in_string1_is_refused_before_its_own_shape_is_read() {
        // genseq()'s first check runs before consuming any character, so the
        // asterisk and the count never get a say in what the shape looks like
        for text in ["[a*2]", "[ab*2]", "[x*]", "[x*1a]"] {
            assert_eq!(
                grammar(text, SetSite::String1),
                SetGrammar::SequencesOnlyInString2,
                "{text:?}"
            );
        }
        assert_eq!(
            SetGrammar::SequencesOnlyInString2.sentence_bytes(),
            b"Sequences only valid in string2".as_slice()
        );
        // The same expression is legal at the string2 site, proving the split is by
        // site, not by the expression itself
        let (set, len) = expanded_set("[a*2]");
        assert_eq!(len, 2);
        assert_eq!(&set[..len], [b'a', b'a']);
    }

    #[test]
    fn the_asterisk_is_checked_before_the_count() {
        // The asterisk sentence runs first: in `[ab*2]` a `b` follows the `a`, so
        // "illegal count" never comes up
        for text in ["[ab*2]", "[abc*2]", "[a b*2]"] {
            assert_eq!(
                grammar(text, SetSite::String2),
                SetGrammar::MisplacedSequenceAsterisk,
                "{text:?}"
            );
        }
        assert_eq!(
            SetGrammar::MisplacedSequenceAsterisk.sentence_bytes(),
            b"Misplaced sequence asterisk".as_slice()
        );
    }

    #[test]
    fn an_unparsable_count_says_illegal_sequence_count() {
        for text in ["[x*1a]", "[x**2]", "[x*-1]", "[x*y]"] {
            assert_eq!(
                grammar(text, SetSite::String2),
                SetGrammar::IllegalSequenceCount,
                "{text:?}"
            );
        }
        assert_eq!(
            SetGrammar::IllegalSequenceCount.sentence_bytes(),
            b"illegal sequence count".as_slice()
        );
    }

    #[test]
    fn an_open_repeat_means_repeat_forever_not_empty() {
        // Both `case ']'` and "a digit whose value is 0" land on the INFINITE side
        // of `s->cnt ? SEQUENCE : INFINITE`
        for text in ["[x*]", "[x*0]"] {
            let (set, len) = expanded_set(text);
            assert_eq!(len, 1);
            assert_eq!(&set[..len], [b'x'], "{text:?}");
        }
        let (set1, len1) = expanded_set("ab");
        let (set2, len2) = expanded_set("[x*]");
        let mut out = [0u8; 8];
        let len = translate(
            b"aabb",
            &set1[..len1],
            &set2[..len2],
            Set2Tail::LastByte,
            &mut out,
        )
        .unwrap();
        assert_eq!(&out[..len], b"xxxx");
    }

    #[test]
    fn an_equivalence_with_a_second_member_blames_the_equals_sign() {
        // `genequiv()`'s member leg consumes **one** character, and the next one must
        // be `=` (`str.c:307-320`). Whether the form ends with `=]` was recognized
        // beforehand by `bracket()` (`str.c:193-199`), so shapes like `[=ab=]` that
        // "look formed but carry extra bytes inside" reach here and report this
        // sentence. The reference binary reports the same sentence and exit 1 for all
        // four shapes (`tmp/tr-341/measure341.log`).
        for text in ["[=ab=]", "[=abc=]", "[=a b=]", "[==]"] {
            for site in [SetSite::String1, SetSite::String2] {
                assert_eq!(
                    grammar(text, site),
                    SetGrammar::MisplacedEquivalenceEquals,
                    "{text:?}"
                );
            }
        }
        assert_eq!(
            SetGrammar::MisplacedEquivalenceEquals.sentence_bytes(),
            b"Misplaced equivalence equals sign".as_slice()
        );
    }

    #[test]
    fn an_equivalence_bracketed_by_more_than_the_equals_blames_the_right_bracket() {
        // `[=a=b=]` hits the **next** sentence (`str.c:321-323`): the member `a` is
        // indeed followed by `=`, but the `=` is followed by `b`, not `]`.
        // `bracket()` looked for the `=]` at the end of the form, so "was recognized"
        // and "passes the check" are not the same thing — both sites judge alike.
        for site in [SetSite::String1, SetSite::String2] {
            assert_eq!(
                grammar("[=a=b=]", site),
                SetGrammar::MisplacedEquivalenceRightBracket
            );
        }
        assert_eq!(
            SetGrammar::MisplacedEquivalenceRightBracket.sentence_bytes(),
            b"Misplaced equivalence right bracket".as_slice()
        );
    }

    #[test]
    fn the_equivalence_member_leg_consumes_two_bytes_for_an_escape() {
        // The member leg reads `if (*s->str == '\\') s->equiv[0] = backslash(s);`
        // (`str.c:309-312`), and `backslash()` consumes two bytes at once. So after
        // `\=` in `[=\=]` what remains is `]`, not `=`, and the equals sentence is
        // reported (the reference binary agrees); `[=\n=]` instead yields the single
        // member **newline** (the reference `tr '[=\n=]' x` turns newlines in the
        // input into `x`).
        assert_eq!(
            grammar(r"[=\=]", SetSite::String1),
            SetGrammar::MisplacedEquivalenceEquals
        );
        let (set, len) = expanded_set(r"[=\n=]");
        assert_eq!((len, &set[..len]), (1, &[b'\n'][..]));
        let (set, len) = expanded_set(r"[=\t=]");
        assert_eq!((len, &set[..len]), (1, &[b'\t'][..]));
    }

    #[test]
    fn an_unshaped_bracket_is_a_literal_bracket() {
        // `bracket()` returns false ⇒ `next()` FALLTHROUGHs to a literal `[`: all
        // four shapes exit 0 in the reference binary, and missing one predicate would
        // report a false sentence here, so each shape is pinned as literal members
        for (text, want) in [
            ("[x]", "[x]".as_bytes()),
            ("[x]y*", "[x]y*".as_bytes()),
            ("[x", "[x".as_bytes()),
            ("[:alpha", "[:alpha".as_bytes()),
        ] {
            let (set, len) = expanded_set(text);
            assert_eq!(&set[..len], want, "{text:?}");
        }
    }

    #[test]
    fn a_wellformed_equivalence_is_its_own_single_member() {
        // The reference `tr_ref abc [=c=]` exits 0; its two sentences "misplaced
        // equals sign / right bracket" want shapes with extra characters inside the
        // form, which this branch does not wire up; registered as TR-EQUIV-SENTENCES
        let (set, len) = expanded_set("[=c=]");
        assert_eq!(len, 1);
        assert_eq!(&set[..len], [b'c']);
    }

    #[test]
    fn test_ring_stops_at_the_first_off_letter() {
        // usage in the loop exits immediately ⇒ at most one report, naming the
        // **first** bad letter
        assert_eq!(
            tr_args(&["-ZQ"]),
            Err(TrError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
        assert_eq!(
            tr_args(&["-sZ", "a"]),
            Err(TrError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
        // All three letters are flags ⇒ the missing-value branch is unreachable, and
        // the `d` in `-d` is not a "value-taking letter"
        assert_eq!(
            tr_args(&["-d", "a"]).map(|p| (p.delete, p.operands)),
            Ok((true, argv_of(&["a"])))
        );
    }

    #[test]
    fn test_operand_count_and_combination_all_say_usage() {
        for args in [
            &[][..],               // zero operands
            &["a"][..],            // one operand without -d/-s
            &["a", "b", "c"][..],  // three
            &["-d"][..],           // flags only
            &["-d", "a", "b"][..], // -d with an extra operand
            &["-ds", "a"][..],     // -ds with one missing
            &["-c"][..],
        ] {
            assert_eq!(
                tr_args(args).map(|_| ()),
                Err(TrError::Usage {
                    reports: Vec::new()
                }),
                "{args:?}"
            );
        }
        // Legal shapes: `-s` with one, `-ds` with two, `-c` with two, a bare `-` is a
        // set name
        assert_eq!(
            tr_args(&["-s", "a"]).map(|p| (p.squeeze, p.operands.len())),
            Ok((true, 1))
        );
        assert_eq!(
            tr_args(&["-ds", "a", "b"]).map(|p| (p.delete, p.squeeze, p.operands.len())),
            Ok((true, true, 2))
        );
        assert_eq!(
            tr_args(&["-", "a"]).map(|p| p.operands),
            Ok(argv_of(&["-", "a"]))
        );
        // `--` is consumed and is not an operand itself; with only `--` there are zero
        // operands ⇒ usage (the reference binary agrees)
        assert_eq!(
            tr_args(&["--", "a", "b"]).map(|p| p.operands),
            Ok(argv_of(&["a", "b"]))
        );
        assert_eq!(
            tr_args(&["--"]).map(|p| p.operands),
            Err(TrError::Usage {
                reports: Vec::new()
            })
        );
        // The loop does not reorder: both tokens of `a -d` are set names
        assert_eq!(
            tr_args(&["a", "-d"]).map(|p| (p.delete, p.operands)),
            Ok((false, argv_of(&["a", "-d"])))
        );
    }

    #[test]
    fn test_usage_block_is_four_aligned_lines() {
        let block = core::str::from_utf8(usage_block()).expect("ascii");
        let lines: Vec<&str> = block.split('\n').collect();
        assert_eq!(lines.len(), 5, "四行 + 结尾换行后的空段");
        assert_eq!(lines[0], "usage: tr [-cs] string1 string2");
        for line in &lines[1..4] {
            assert!(
                line.starts_with("       tr "),
                "续行是七个空格对齐：{line:?}"
            );
        }
    }

    /// A raw `ff` in a set expression is **one member**, not three folded bytes.
    /// C's `str.c` consumes expressions byte by byte (`escape()` and `nextch()`
    /// both go through `unsigned char`), and the reference binary
    /// `tmp/tr-322/tr_ref` measured `tr -d a<ff>b` on input `a\xffbq` printing `q`
    /// (the `ff` was deleted);
    /// the text seam on the same shape yields `\xffq` — it deletes the three bytes
    /// `ef`, `bf`, `bd`, none of which are in the input.
    #[test]
    fn a_raw_ff_set_member_is_one_byte_of_the_set() {
        let mut expanded = [0u8; 256];
        let (used, _) = parse_set(b"a\xffb".as_slice(), SetSite::String1, &mut expanded).unwrap();
        assert_eq!(used, 3, "式子该交出三枚字节成员");
        assert_eq!(&expanded[..used], b"a\xffb".as_slice());
    }

    /// The class-name echo is the **original bytes** from the expression:
    /// `str.c:253`'s `errx(1, "unknown class %.*s",
    /// (int)len, class)` prints that byte string with its length, and the reference
    /// answers `tr $'[:\xfffoo:]' x` with
    /// `tr: unknown class \xfffoo`. The sentence returns `Vec<u8>` precisely so this
    /// one can be pinned.
    #[test]
    fn an_unknown_class_name_is_echoed_as_its_own_bytes() {
        assert_eq!(
            SetGrammar::UnknownClass(b"\xfffoo".to_vec()).sentence_bytes(),
            b"unknown class \xfffoo".as_slice()
        );
    }

    /// An off-string letter yields the byte after the dash (`getopt.c:79`'s
    /// `optopt = *place++`); the report is a raw `ff`: the text seam folds it to `ef`
    /// here, while the reference `tr -<ff> a b` prints `ff`.
    #[test]
    fn parse_tr_names_a_raw_ff_option_letter_by_that_byte() {
        use crate::GetoptReport;
        let error = parse_tr_args(&[b"-\xff".as_slice(), b"a".as_slice(), b"b".as_slice()]).err();
        assert_eq!(
            error,
            Some(TrError::Usage {
                reports: vec![GetoptReport::UnknownOption('\u{00ff}')]
            })
        );
    }
}
