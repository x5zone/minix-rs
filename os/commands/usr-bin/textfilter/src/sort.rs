//! Key parsing and comparison for `sort`.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The C extracts each
//! record's sort keys into transformed key buffers and compares those
//! buffers byte-wise (`fields.c`); the observable semantics — which this
//! module reimplements at the value level — are: key specifications
//! `-k F[.C][flags][,F[.C][flags]]` with per-key flags from `optval`'s table
//! `b d f i l n x r` (init.c:222-242; the scan **stops at the first letter not in
//! that table** — init.c:173 — and the leftover decides the next sentence, while
//! `.0` on the start column is an `illegal offset` and on the end column is legal
//! because only the start column's indent gets decremented — init.c:168-171),
//! fields separated by blank runs or by the `-t` delimiter byte, default
//! whole-line keys, the modifier transformations dictionary (`d`), fold
//! (`f`), ignore non-printable (`i`), numeric (`n`, blank-skipping sign
//! and fraction aware), sort-by-field-length (`l`, `fields.c`'s `length()`: skip the
//! leading blanks, then take the span's byte count), and reverse (`r`; a local `r` is dropped when a global
//! `-r` stands — init.c:195-197). There is **no month flag in this C**: `M` is in
//! neither `SORT_OPTS` (sort.c:148) nor `optval`, so `-M` falls to `usage()` and
//! `-k 1M` to `illegal field descriptor`. The last letter of `optval`'s table, `x`
//! (hex numeric), is **not** modelled: C's `numhex()` applies `sscanf(… "%llx", &n)`
//! to the **destination-buffer cursor** `pos` rather than to the field itself (the three
//! `numhex` lines in `fields.c`), so its reading depends on key-buffer reuse and cannot be
//! reproduced at the value level — this shell leaves `x` in the out-of-set-letter branch and
//! lets it report itself rather than pretending to the same shape.
//! Ties fall back to the whole line in byte order, and the global `-r`
//! inverts the final outcome.
//!
//! The engine (external merge over temporary files) is the execution
//! layer; this module compares slices, which is what the observable
//! output order is made of. Nothing allocates: modifiers apply as
//! predicates over the existing bytes. The option surface itself needs
//! one growable list (the key specifications), served by `alloc`.

use alloc::string::String;
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
    /// Sort by the **length** of the selected span (`l`, `fields.c`'s `length()`).
    /// C ranks this bit ahead of the numeric bit in `enterfield` (`:208-211`), so when both `l`
    /// and `n` are present the length decides.
    pub by_length: bool,
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
            by_length: false,
            reverse: false,
        }
    }
}

/// Global options for one `sort` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Key specifications (empty = one whole-line key).
    pub keys: Vec<KeySpec>,
    /// Global reverse.
    pub reverse: bool,
    pub fold: bool,
    pub dictionary: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    /// Global sort-by-length (`-l`); same shape as the other flag letters.
    pub by_length: bool,
    /// Leading-blank skip as a global flag.
    pub blank_skip: bool,
    /// Suppress lines that compare equal to their predecessor.
    pub unique: bool,
    /// `-S`: break key ties by the **whole record** (C's `posix_sort`, `sort.c:202-212`).
    /// Without this bit C does not re-sort on the raw record (`:257`'s comment literally says
    /// "Don't sort on raw record if keys match"), so tied keys keep input order. `-u` zeroes it
    /// **after** the option ring (`:256-258`), so `-S -u` and `-u -S` are equivalent.
    pub posix_tiebreak: bool,
    /// Check only: report disorder and duplicates, sort nothing.
    pub check: bool,
    /// Field delimiter byte (`-t`).
    pub delimiter: Option<u8>,
    /// Record delimiter — C's `REC_D` (`sort.c:92`, initial `'\n'`; `main` sets
    /// `d_mask[REC_D = '\n'] = REC_D_F` before the option ring (`:139`)). `-R` changes it; it
    /// also decides the "does `-t` collide with the record delimiter" test (C checks
    /// `d_mask[c] & REC_D_F`, and that bit only follows `REC_D`).
    /// C's `int REC_D` (declared around `sort.c:96`, assigned at `:226`). The type cannot be
    /// narrowed to `u8`: the **character path** of `-R` is `REC_D = *optarg` with a signed
    /// `char`, so a `ff` byte arrives as **-1**; the **numeric path** (`-R\377`, `-R 255`)
    /// goes through `strtol` and returns a positive 0..=255. The same `0xff` therefore has two
    /// outcomes: the character path matches no input byte and never hits `-t`'s mask slot (C
    /// writes out of bounds at `d_mask[-1]`; this layer does not copy the out-of-bounds write,
    /// only its one observable consequence, "not a clash"); the numeric path lands exactly on
    /// `d_mask[255]` and **is a clash** with `-t`'s raw byte (`sort.c:219-220` compares
    /// `d_mask[(u_char)*optarg] & REC_D_F`). What is written out is still the low byte
    /// (`putchar`'s `int`→`unsigned char` conversion), so `payload` needs `as u8` there.
    pub record_delim: i32,
    /// Merge already-sorted runs instead of sorting (`-m`, C `sort -m`).
    pub merge: bool,
    /// `-o path`: where the result goes. `None` is C's null `outpath`, which writes
    /// to standard output (`sort.c:320-323`); `Some("")` is *not* the same thing —
    /// C carries the empty string into `lstat`/`fopen` and fails there, so the
    /// option has to be distinguishable from its absence. The path also decides
    /// *how* it is written, see [`decide_output`], because C replaces an existing
    /// file through a temporary rather than truncating it in place.
    pub output: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            keys: Vec::new(),
            reverse: false,
            fold: false,
            dictionary: false,
            ignore_nonprint: false,
            numeric: false,
            by_length: false,
            blank_skip: false,
            unique: false,
            posix_tiebreak: false,
            check: false,
            delimiter: None,
            // C's global initial value: `int REC_D = '\n';` (`sort.c:92`).
            record_delim: 10,
            merge: false,
            output: None,
        }
    }
}

/// Reduce `-R`'s multi-byte value to one byte the way C's `strtol(optarg, &ep, t)` does
/// (`sort.c:229-238`). `Some(byte)` is the shape C continues with; `None` is that `invalid
/// record delimiter` sentence.
/// The three rejection criteria follow C clause by clause: `*ep != '\0'` (anything left at the
/// end, including one space), a value below zero, and a value beyond `NBINS` (256). Leading
/// blanks and one sign are consumed (`strtol` consumes them itself), so the code below requires
/// "the whole string was consumed" rather than "all digits once the blanks are removed" —
/// `"1 "` is an error in C.
/// base 0 follows C's prefix rules: `0x`/`0X` is hex, a leading `0` is octal, otherwise
/// decimal.
fn record_delim_number(digits: &[u8], radix: u32) -> Option<u8> {
    let mut pos = 0usize;
    while pos < digits.len() && matches!(digits[pos], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        pos += 1;
    }
    let mut negative = false;
    if pos < digits.len() && matches!(digits[pos], b'+' | b'-') {
        negative = digits[pos] == b'-';
        pos += 1;
    }
    let mut base = radix;
    if base == 0 {
        base = 10;
        if digits.get(pos) == Some(&b'0') {
            if matches!(digits.get(pos + 1), Some(b'x') | Some(b'X')) {
                base = 16;
                pos += 2;
            } else {
                base = 8;
                pos += 1;
            }
        }
    }
    let mut value: i64 = 0;
    let mut any = false;
    while pos < digits.len() {
        let digit = (digits[pos] as char).to_digit(base)?;
        value = value
            .saturating_mul(base as i64)
            .saturating_add(digit as i64);
        any = true;
        pos += 1;
    }
    if !any {
        // C's `strtol` leaves `endptr` at the **original start** when no digit was converted
        // (not at the position past blanks and sign), so `*ep` is `\0` only when the whole
        // string was empty to begin with. That is the `-R '\'` shape: an empty remainder → no
        // error and REC_D = 0; while `'-x'` and `'q'` are both errors.
        if !digits.is_empty() {
            return None;
        }
        value = 0;
    } else if pos != digits.len() {
        // Digits consumed but something is left at the end (including a space): C's
        // `*ep != '\0'` item.
        return None;
    }
    let value = if negative { -value } else { value };
    if value < 0 || value > u8::MAX as i64 {
        return None;
    }
    Some(value as u8)
}

/// Merge several already-sorted runs into one sorted vector, the way C's
/// `sort -m` concatenates sorted inputs by merging rather than re-sorting. Each
/// run is assumed already ordered under `options`; equal keys keep the earlier
/// run's line first (a stable, left-biased merge). Because nothing is
/// re-sorted, `-m` on runs that are not each sorted yields a merge of the runs,
/// exactly as the C does.
pub fn merge_sorted(runs: &[&[String]], options: &Options) -> Vec<String> {
    let mut acc: Vec<String> = Vec::new();
    for run in runs {
        acc = merge_two(&acc, run, options);
    }
    acc
}

fn merge_two(a: &[String], b: &[String], options: &Options) -> Vec<String> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if compare(&b[j], &a[i], options) == core::cmp::Ordering::Less {
            out.push(b[j].clone());
            j += 1;
        } else {
            out.push(a[i].clone());
            i += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

/// One unparsable option-argument error. The three exits map to C's three real ways out (see
/// the variant comments), not to one blanket "bad option".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// libc `getopt`'s own line (out-of-set letter, or a value-taking option with a missing
    /// value), followed by `usage(NULL)` at `sort.c:252` — so the source of truth is two lines.
    /// Both classes carry their letter.
    Getopt(crate::GetoptReport),
    /// The `-k` key shape is wrong: one of the five `errx(2, …)` sentences in
    /// `setcolumn`/`setfield` ([`KeySpecError`]). Each of the five has its own criterion, but the
    /// exit is the same: exit 2, **no usage**. Previously the five were squeezed into one
    /// `IllegalFieldDescriptor`, so four of the messages signed with someone else's sentence.
    KeySpec(KeySpecError),
    /// `-t` given a second time: `usage("multiple field delimiters")` at `sort.c:214-215`, i.e.
    /// one sentence carrying the program name first, then the whole usage block.
    MultipleFieldDelimiters,
    /// `-R` given a second time and the first one really changed the default:
    /// `usage("multiple record delimiters")` at `sort.c:224-225` (also the "one sentence + usage
    /// block" family, exit 2).
    MultipleRecordDelimiters,
    /// `-R`'s numeric form does not parse, or exceeds `d_mask`'s size (`errx(2, "invalid record
    /// delimiter %s", optarg)` at `sort.c:233-238`): that sentence alone, **no usage**, exit 2.
    /// It carries the `optarg` C had in hand — the backslash prefix has already been skipped, so
    /// `\q` reports `q`.
    InvalidRecordDelimiter(Vec<u8>),
    /// The byte chosen by `-t` is exactly the current record delimiter:
    /// `errx(2, "record/field delimiter clash")` at `sort.c:217-218`, likewise no usage, exit 2.
    RecordFieldDelimiterClash,
}

/// The five `errx(2, …)` sentences of `setcolumn()` and `setfield()` in
/// `minix3/usr.bin/sort/init.c`. The sentence text has its single source in
/// [`KeySpecError::sentence`]; the shell only adds the program name and exit 2.
///
/// All five were measured shape by shape against a compiled reference build (the real
/// `minix3/usr.bin/sort/` sources plus shims), not inferred:
///
/// | shape | C's sentence |
/// |---|---|
/// | `-k 0`, `-k x`, `-k ''`, `-k ,5`, `-k 1,-2` | `field numbers must be positive` |
/// | `-k 1.0` | `illegal offset` |
/// | `-k 1,0.5` | `cannot indent end of line` |
/// | `-k 1 2`, `-k 1n2`, `-k 1.2.3` | `illegal field descriptor` |
/// | `-k 2.5,1.2` | `fields out of order` |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySpecError {
    /// `init.c:160-161`'s `col->num <= 0 && !(col->num == 0 && col == &tcol)`: the start column
    /// must be positive; the end column may be 0 (meaning end of line), but a negative rejects
    /// both columns. Note the criterion uses `strtol`'s result, so `-k x` and `-k ''` are zero
    /// too (no digit converted).
    FieldNumbersNotPositive,
    /// `init.c:163-164`: when the column number is 0 (end of line), no `.C` indent may be given.
    CannotIndentEndOfLine,
    /// `init.c:168-171`: the computed indent is negative. Only the **start column** does
    /// `col->indent--`, so `1.0` errors while `1,2.0` is legal — the `.0` on the two columns is
    /// not symmetric.
    IllegalOffset,
    /// `init.c:188-190`: after the start column stops, the position is neither end of string nor
    /// a comma.
    IllegalFieldDescriptor,
    /// `init.c:210-216`: the end column's (field number, indent) pair lies entirely before the
    /// start column. All three conditions must hold to report; missing one means no report —
    /// `-k 2,1` does not report (with no `.C` on the end column the indent is 0, and the comment
    /// in the source says "indent if 0 -> end of field, i.e. okay").
    FieldsOutOfOrder,
}

impl KeySpecError {
    /// C's `errx` format string verbatim (none of the five has `%s`, so bad text is never
    /// spliced back into the sentence).
    pub fn sentence(self) -> &'static str {
        match self {
            KeySpecError::FieldNumbersNotPositive => "field numbers must be positive",
            KeySpecError::CannotIndentEndOfLine => "cannot indent end of line",
            KeySpecError::IllegalOffset => "illegal offset",
            KeySpecError::IllegalFieldDescriptor => "illegal field descriptor",
            KeySpecError::FieldsOutOfOrder => "fields out of order",
        }
    }
}

/// Parse global options from the argument words, returning the options
/// plus the remaining operand words (file paths in the C; the doing half
/// rejects them while the open-existing call is gated). `-k` and `-t`
/// take a value, given separately or inline; an out-of-set letter and a missing value each
/// become one [`ParseError::Getopt`], while a wrong key shape and a second `-t` take two other
/// exits.
///
/// The value-taking letters modelled by this parser are `k`, `o` and `t`: C's option string is
/// `sort.c:148`'s `#define SORT_OPTS "bcdD:fHik:lmno:rR:sSt:T:ux"`, where `:` marks the six
/// letters `D`, `k`, `o`, `R`, `t`, `T`; but `D`/`R`/`T` (and the flags `S`/`x`) are not
/// modelled in this tree yet, so they currently fall into the "out-of-set letter" branch — a gap
/// on the **option face** (registered separately). The split into two libc reports (out-of-set
/// letters per `getopt.c:101-104`, missing values per `:132-136`) shows up in the arms below.
pub fn parse_options<'a>(args: &[&'a [u8]]) -> Result<(Options, Vec<&'a [u8]>), ParseError> {
    let mut options = Options::default();
    let mut operands: Vec<&[u8]> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg.first() != Some(&b'-') || arg.len() <= 1 {
            operands.push(arg);
            index += 1;
            continue;
        }
        // The ring walks **bytes** and records indices: C's `optopt` is one `char`
        // (`getopt.c:79`'s `optopt = *place++` consumes one byte at a time, and `:103`'s `%c`
        // writes it out as is), so `-é` (`c3 a9`) reports `c3`. Going through `chars()` used to
        // treat the two bytes of one character as one letter, and the report then rendered as two
        // bytes through UTF-8. The "rest of this token" of a value-taking letter is therefore
        // sliced by byte index, with the same semantics as `chars.by_ref()`.
        let tail = &arg[1..];
        let letters = tail;
        let mut at = 0usize;
        while at < letters.len() {
            let flag = letters[at];
            at += 1;
            match flag {
                b'r' => options.reverse = true,
                b'f' => options.fold = true,
                b'd' => options.dictionary = true,
                b'i' => options.ignore_nonprint = true,
                b'n' => options.numeric = true,
                // `-l` shares C's arm with `-d`/`-f`/`-i`/`-n` (`sort.c:168`), and its effect is
                // that same `fldtab[0].flags |= optval(ch, 0)` (`:172`), so it is a global default
                // flag.
                b'l' => options.by_length = true,
                b'b' => options.blank_skip = true,
                b'u' => options.unique = true,
                b'c' => options.check = true,
                b'm' => options.merge = true,
                // `-H`: a documented no-op in C — `sort.c:174-176` comments that
                // `-H` "was; use merge sort for blocks of large files — that is
                // now the default", so it accepts the flag and does nothing. We
                // match that: accept, no effect (previously it errored as bad).
                b'H' => {}
                // `-s` (`sort.c:194-201`) and `-S` (`:202-212`) are two sides of one switch: the
                // former zeroes `posix_sort`, the latter sets it. Zeroing is C's default (tied
                // keys are not re-sorted by whole record, so input order is kept stably), so `-s`
                // really is a no-op here; `-S` is **not** — it turns on the `posix_tiebreak` field
                // below, and treating both as no-ops lost that half of `-S`'s semantics
                // (reference: with four equal-key lines, `-k 2,2` gives the input order
                // `b a c d`, while `-S -k 2,2` gives the record order `a b c d`).
                b's' => {}
                b'S' => options.posix_tiebreak = true,
                // `-o` takes a value, inline (`-oNAME`) or as the next word
                // (`sort.c:188-190`); a missing one is C's `?` and lands on the
                // usage path. A later `-o` overwrites the earlier one, as C's
                // `outpath = optarg` assignment does.
                b'o' => {
                    let mut value: &[u8] = &tail[at..];
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = *word,
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    char::from(flag),
                                )));
                            }
                        }
                    }
                    options.output = Some(value.to_vec());
                    break;
                }
                b'k' | b't' => {
                    let mut value: &[u8] = &tail[at..];
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = *word,
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    char::from(flag),
                                )));
                            }
                        }
                    }
                    if flag == b'k' {
                        match parse_key(value, &options) {
                            // C does not write usage on this branch: it is one of the five
                            // `errx(2, …)` sentences in `setfield`/`setcolumn`
                            // (`init.c:160-216`), and each sentence differs.
                            Ok(spec) => options.keys.push(spec),
                            Err(error) => return Err(ParseError::KeySpec(error)),
                        }
                    } else {
                        // `-t` takes only the first byte in C (`d_mask[(u_char)*optarg]` at
                        // `sort.c:219`) and never checks the length; but the same option given a
                        // second time is `usage("multiple field delimiters")` (`:214-215`).
                        if options.delimiter.is_some() {
                            return Err(ParseError::MultipleFieldDelimiters);
                        }
                        // An empty value (`-t ''`) picks up that `\0` in C, with no error.
                        let byte = value.first().copied().unwrap_or(0);
                        // `:217-218` then checks `d_mask[byte] & REC_D_F`. That bit only lives
                        // on `REC_D` (the default `main` installs is `'\n'`, `:139`; `-R x`
                        // moves it to `x`), so the criterion is "this byte is exactly the current
                        // record delimiter" — note `-R` need not have been given first:
                        // `-t '\n'` reports under the default shape (the reference measures the
                        // same sentence).
                        if i32::from(byte) == options.record_delim {
                            return Err(ParseError::RecordFieldDelimiterClash);
                        }
                        options.delimiter = Some(byte);
                    }
                    break;
                }
                // `-R char`: move the record delimiter off the default `'\n'`
                // (`sort.c:223-242`). The order follows C clause by clause: (1) if it was already
                // changed and no longer equals `'\n'`, report
                // `usage("multiple record delimiters")` — note the test is "is the current value
                // still `'\n'`", not "how many times was it given", so `-R '\12' -R ,` is legal
                // (the reference reads the same, because the first computes back to 10); (2)
                // `*optarg` is taken as the value first, and if it is `'\n'` the arm **ends right
                // there**, skipping the numeric form and the mask entirely (`-R $'\nx'` is
                // therefore the same as `-R '\n'`); (3) only a value longer than one byte goes to
                // `strtol`: with a backslash prefix in octal, otherwise base 0, and leftover tail
                // characters / a negative / a value beyond `NBINS` all land on
                // `errx(2, "invalid record delimiter %s", optarg)`.
                b'R' => {
                    let mut value: &[u8] = &tail[at..];
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = *word,
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    char::from(flag),
                                )));
                            }
                        }
                    }
                    if options.record_delim != 10 {
                        return Err(ParseError::MultipleRecordDelimiters);
                    }
                    let bytes = value;
                    let first = bytes.first().copied().unwrap_or(0);
                    // Character path: C assigns `*optarg` (a signed `char`) to the `int REC_D`,
                    // so a high byte becomes negative, and the later `first != '\n'` and the
                    // matching criteria all follow that negative value.
                    options.record_delim = i32::from(first as i8);
                    if first != b'\n' && bytes.len() > 1 {
                        // C does `optarg++` first to skip the backslash prefix and reports with
                        // the then-current `optarg`, so the message carries the **remaining**
                        // string.
                        let (digits, radix) = if bytes[0] == b'\\' {
                            (&bytes[1..], 8u32)
                        } else {
                            (bytes, 0u32)
                        };
                        match record_delim_number(digits, radix) {
                            Some(byte) => options.record_delim = i32::from(byte),
                            None => {
                                return Err(ParseError::InvalidRecordDelimiter(digits.to_vec()));
                            }
                        }
                    }
                    // C also sets `d_mask['\n'] = d_mask[' ']` and `d_mask[REC_D] = REC_D_F`
                    // here, folding newline into the "blank/field separator" class. This layer's
                    // field scanner does not read that mask, and the reference's reading of the
                    // only shape that can probe it (`-R , -k 2`, last line containing `\n`) agrees
                    // with this shell (both give `b,a,d,c\n,`), so the mask is left alone; the
                    // shape difference is registered.
                    break;
                }
                _ => {
                    return Err(ParseError::Getopt(crate::GetoptReport::UnknownOption(
                        char::from(flag),
                    )));
                }
            }
        }
        index += 1;
    }
    // `sort.c:256-258`: `if (UNIQUE) posix_sort = 0;` is checked **after the option ring
    // finishes**. Its position matters: it is outside the ring, so `-S -u` and `-u -S` are
    // equivalent (the reference keeps only the first line in both shapes). And "tied keys are
    // not sorted by record" is exactly what `sort -u` wants: otherwise a stack of equal-key
    // lines would be split by record order.
    if options.unique {
        options.posix_tiebreak = false;
    }
    Ok((options, operands))
}

/// What a `lstat` of the requested output path tells the C, narrowed to the two
/// questions its branch asks (`sort.c:324-326`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputProbe {
    /// `st_mode & ALLPERMS`, the permission bits C copies onto the temporary before
    /// it becomes the output name (`sort.c:366`, `ALLPERMS` = `0o7777`).
    pub mode: u32,
    /// `S_ISCHR(st.st_mode) || S_ISBLK(st.st_mode)`: writing straight through is
    /// correct for a device, and C says so out loud.
    pub is_device: bool,
}

/// Where the sorted text goes, the three ways C reads `sort.c:320-355`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// No `-o`: `stdout`, and no path is ever touched.
    StandardOutput,
    /// Open the named path for writing and truncate it — the branch C takes when
    /// `lstat` fails (no such file) or reports a character/block special
    /// (`sort.c:352-355`).
    Direct { path: Vec<u8> },
    /// Write `<path>XXXXXX`, then copy the probed mode onto it, unlink the original
    /// name, hard-link the temporary to it, and unlink the temporary
    /// (`sort.c:327-379`). A crash mid-write therefore leaves the old file standing.
    ViaTemp { path: Vec<u8>, mode: u32 },
}

/// C's choice between those three, as a function of the option and the probe. An
/// absent `-o` is the only way to reach standard output; an empty one still names a
/// path, and C's `fopen("", "w")` fails on it (`sort.c:354-355`).
pub fn decide_output(output: Option<&[u8]>, probe: Option<OutputProbe>) -> Output {
    let Some(output) = output else {
        return Output::StandardOutput;
    };
    let path = output.to_vec();
    match probe {
        Some(found) if !found.is_device => Output::ViaTemp {
            path,
            mode: found.mode,
        },
        _ => Output::Direct { path },
    }
}

/// The six characters that replace the `XXXXXX` of `sort.c:338-339`. C's `mkstemp`
/// fills them from `padchar` — sixty-two characters in the order digits, uppercase,
/// lowercase (`lib/libc/stdio/gettemp.c:49-50`) — drawing one at random per slot, and
/// then carries the name forward while a create keeps failing (`:120-170`). This
/// shell takes the same alphabet and walks it the same way from a per-process seed
/// instead of a random one; all the caller relies on is that the name keeps the
/// requested prefix, stays six characters long, and differs between attempts.
pub fn temp_candidate(path: &[u8], seed: u64, attempt: usize) -> Vec<u8> {
    const PAD_CHAR: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut value = seed.wrapping_add(attempt as u64);
    let mut suffix = [0u8; 6];
    for slot in suffix.iter_mut().rev() {
        *slot = PAD_CHAR[(value % PAD_CHAR.len() as u64) as usize];
        value /= PAD_CHAR.len() as u64;
    }
    let mut out = path.to_vec();
    out.extend_from_slice(&suffix);
    out
}

/// Parse one `-k` specification. The shape follows `setfield()` at `init.c:181-220` clause by
/// clause, including **the order of the criteria**: parse both columns first (each with its own
/// sentence), then look at the remainder (`illegal field descriptor`), then do the four flag
/// combinations (inherit the globals only when the local set is empty, a global `-r` wipes the
/// local `r`, the global BI/BT are ORed in unconditionally, BT is dropped when the end column has
/// no indent), and only then check `fields out of order`.
///
/// Two substantive differences from the previous form: (1) it squeezed the five `errx` sentences
/// into one, so four of the reports signed with someone else's sentence; (2) it wrote the global
/// flags as a bitwise OR, while C inherits them wholesale only when the local segment has no
/// letter at all (`init.c:193-194`), so a shape like `-n -k 1r` sorted numerically here but
/// reverse-lexicographically in C.
///
/// The only modifier letter still unmodelled is `x` (hex numeric, `__minix`-specific): C's
/// `numhex()` applies `sscanf(… "%llx", &n)` to the destination buffer's write cursor rather
/// than to the field itself (the few `numhex` lines in `fields.c`), so its reading depends on
/// key-buffer reuse and cannot be reproduced at the value level. The loop below therefore **does
/// not accept `x`** (it is currently rejected as a leftover character, reporting
/// `illegal field descriptor`), and a global `-x` still takes the out-of-set-letter branch on
/// purpose — silently accepting it would disguise "wrong order" as "right order".
/// `M` is in neither the `optval` table nor `SORT_OPTS` (`sort.c:148`), and the `switch` has no
/// `case 'M'` either — so C accepts neither `-M` nor `-k 1M` (measured against the reference: the
/// former falls to `usage`, the latter reports `illegal field descriptor`); treating month-name
/// sorting as a feature here was modelling without a source.
pub fn parse_key(spec: &[u8], global: &Options) -> Result<KeySpec, KeySpecError> {
    let start = scan_column(spec, false)?;
    // `setfield` looks only at where the start column stopped: end of string means "the key runs
    // to end of line", a comma starts the end column, and any other character is
    // `illegal field descriptor` (`init.c:188-190`).
    let end_text = if start.rest.is_empty() {
        None
    } else if let Some(tail) = start.rest.strip_prefix(&b","[..]) {
        Some(tail)
    } else {
        return Err(KeySpecError::IllegalFieldDescriptor);
    };
    let end = match end_text {
        Some(text) => scan_column(text, true)?,
        // No end column given: C leaves `tcol.num` at 0 (end of line) and scans no letters.
        None => Column::absent(),
    };

    let local = start.merge(&end);
    // `init.c:193-194`: `if (!cur_fld->flags) cur_fld->flags = gflag;` — a local segment with
    // even one modifier letter stops inheriting the global flags.
    let (dictionary, fold, ignore_nonprint, numeric, by_length, reverse) = if local.any {
        (
            local.dictionary,
            local.fold,
            local.ignore_nonprint,
            local.numeric,
            local.by_length,
            local.reverse,
        )
    } else {
        (
            global.dictionary,
            global.fold,
            global.ignore_nonprint,
            global.numeric,
            global.by_length,
            local.reverse,
        )
    };
    // `init.c:195-197`: when the global `-r` is present the local `r` is wiped (not toggled a
    // second time).
    let reverse = if global.reverse { false } else { reverse };
    // `init.c:206`: the global BI/BT are **ORed in unconditionally** (unlike the clause above),
    // and C's global `-b` lights both bits at once (`fldtab[0].flags |= BI | BT` at
    // `sort.c:159`).
    let blank_start = local.bi || global.blank_skip;
    let mut blank_end = local.bt || global.blank_skip;
    // `init.c:207-208`: `if (!cur_fld->tcol.indent) flags &= ~BT` — BT is meaningless when the
    // end column has no indent.
    if end.eff_indent == 0 {
        blank_end = false;
    }
    // All three conditions of `init.c:210-216` must hold: the end column has a field number, it
    // is not "BT without BI", and the end column (number, indent) lies before the start column. C
    // writes `!(!(flags & BI) && (flags & BT))` (`:211`); the line below is its De Morgan rewrite
    // (clippy rejects the double negation; same value).
    let not_bt_without_bi = !local.bt || local.bi;
    if end.num != 0
        && not_bt_without_bi
        && end.num <= start.num
        && end.eff_indent != 0
        && end.eff_indent < start.eff_indent
    {
        return Err(KeySpecError::FieldsOutOfOrder);
    }
    Ok(KeySpec {
        start_field: start.num as usize,
        start_char: start.raw_indent as usize,
        blank_start,
        end_field: end.num as usize,
        end_char: end.eff_indent as usize,
        blank_end,
        dictionary,
        fold,
        ignore_nonprint,
        numeric,
        by_length,
        reverse,
    })
}

/// The shape one column scan produces, mapping C's `struct column` (`sort.h`).
#[derive(Debug, Clone, Copy)]
struct Column<'a> {
    /// Field number; the end column may be 0 (meaning end of line).
    num: i64,
    /// The raw reading of `.C` (0 when absent), handed to [`KeySpec::start_char`].
    raw_indent: i64,
    /// The number C stores in `col->indent`: the start column decrements it on `.C`
    /// (`init.c:168-169`), and without `.C` both columns are 0 (the struct is memset).
    eff_indent: i64,
    /// The eight bits below are the flags `optval` sets on this column.
    bi: bool,
    bt: bool,
    dictionary: bool,
    fold: bool,
    ignore_nonprint: bool,
    numeric: bool,
    by_length: bool,
    reverse: bool,
    /// Not a single modifier letter was scanned (C tests this with `if (!cur_fld->flags)`).
    any: bool,
    /// The leftover after the stop point, exactly the `pos` C returns to `setfield` (one byte
    /// per cell, the same shape as C's `char *`).
    rest: &'a [u8],
}

impl<'a> Column<'a> {
    /// The shape when the end column is absent: in C it is that memset, untouched `tcol`.
    fn absent() -> Self {
        Column {
            num: 0,
            raw_indent: 0,
            eff_indent: 0,
            bi: false,
            bt: false,
            dictionary: false,
            fold: false,
            ignore_nonprint: false,
            numeric: false,
            by_length: false,
            reverse: false,
            any: false,
            rest: &[],
        }
    }

    /// The two columns' flags are an OR onto the same `cur_fld->flags` in C; same shape here.
    fn merge(&self, other: &Self) -> Self {
        Column {
            num: self.num,
            raw_indent: self.raw_indent,
            eff_indent: self.eff_indent,
            bi: self.bi || other.bi,
            bt: self.bt || other.bt,
            dictionary: self.dictionary || other.dictionary,
            fold: self.fold || other.fold,
            ignore_nonprint: self.ignore_nonprint || other.ignore_nonprint,
            numeric: self.numeric || other.numeric,
            by_length: self.by_length || other.by_length,
            reverse: self.reverse || other.reverse,
            any: self.any || other.any,
            rest: &[],
        }
    }
}

/// C's `setcolumn()` (`init.c:152-178`) clause by clause. `filling_end` corresponds to C's
/// `col = cur_fld->icol.num ? (&cur_fld->tcol) : (&cur_fld->icol)` (`:157`): it decides whether
/// `.C` gets decremented. Whether `b` is recorded as BI or BT uses a different criterion —
/// `optval`'s `tcolflag` is `cur_fld->tcol.num` (`:173`) — so in the shape where the end column
/// is filled with 0 (`-k 1,0b`), `b` still counts as BI. The two criteria must not be merged.
fn scan_column<'a>(text: &'a [u8], filling_end: bool) -> Result<Column<'a>, KeySpecError> {
    let bytes = text;
    let (num, mut position) = scan_long(text, 0);
    // `init.c:160-161`: a non-positive start column is an error; the end column may be 0 (which
    // means end of line).
    if num <= 0 && !(num == 0 && filling_end) {
        return Err(KeySpecError::FieldNumbersNotPositive);
    }
    let tcolflag = filling_end && num != 0;
    let mut raw_indent = 0i64;
    let mut eff_indent = 0i64;
    if bytes.get(position) == Some(&b'.') {
        // `init.c:162-164`: when the column number is 0 (end of line), `.C` makes no sense.
        if num == 0 {
            return Err(KeySpecError::CannotIndentEndOfLine);
        }
        position += 1;
        let (value, next) = scan_long(text, position);
        position = next;
        raw_indent = value;
        eff_indent = if filling_end { value } else { value - 1 };
        if eff_indent < 0 {
            return Err(KeySpecError::IllegalOffset);
        }
    }
    let mut column = Column {
        num,
        raw_indent,
        eff_indent,
        bi: false,
        bt: false,
        dictionary: false,
        fold: false,
        ignore_nonprint: false,
        numeric: false,
        by_length: false,
        reverse: false,
        any: false,
        rest: &[],
    };
    // `init.c:173`'s `for (; (tmp = optval(*pos, cur_fld->tcol.num)); pos++)`: `optval`
    // returning 0 **stops** the scan, leaving that character for `setfield`'s `*pos` test; it is
    // not "skip and continue".
    while position < bytes.len() {
        match bytes[position] {
            b'b' => {
                if tcolflag {
                    column.bt = true;
                } else {
                    column.bi = true;
                }
            }
            b'd' => column.dictionary = true,
            b'f' => column.fold = true,
            b'i' => column.ignore_nonprint = true,
            b'n' => column.numeric = true,
            // `optval`'s `case 'l': return L;` (`init.c:234`).
            b'l' => column.by_length = true,
            b'r' => column.reverse = true,
            _ => break,
        }
        column.any = true;
        position += 1;
    }
    column.rest = &text[position..];
    Ok(column)
}

/// C's `strtol(pos, &npos, 10)` (`init.c:158`, `:166`): skip leading blanks, allow one sign,
/// then the decimal digits. When no digit is converted the value is 0 and the pointer **goes back
/// to the start** (which is why `-k x` and `-k ''` land on "field numbers must be positive"). On
/// overflow past `i64` C saturates to `LONG_MAX`/`LONG_MIN` and **ignores `errno`**, so there is
/// no error there; this tree saturates the same way.
fn scan_long(text: &[u8], from: usize) -> (i64, usize) {
    let bytes = text;
    let mut position = from;
    while position < bytes.len() && bytes[position].is_ascii_whitespace() {
        position += 1;
    }
    let mut negative = false;
    match bytes.get(position) {
        Some(b'-') => {
            negative = true;
            position += 1;
        }
        Some(b'+') => position += 1,
        _ => {}
    }
    let digits_start = position;
    let mut value: i64 = 0;
    while position < bytes.len() && bytes[position].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(bytes[position] - b'0'));
        position += 1;
    }
    if position == digits_start {
        return (0, from);
    }
    (if negative { -value } else { value }, position)
}

/// The step C's `fields.c:length()` takes: `SKIP_BLANKS(line)` first, then `lineend - line`.
/// So it measures neither the padded span nor the filtered key bytes but the byte count from
/// after the leading blanks to the end of the span (`start`/`end` in `enterfield`, the same
/// arithmetic as [`key_bytes`]).
fn span_length(line: &str, key: &KeySpec, delimiter: Option<u8>) -> usize {
    let span = key_bytes(line, key, delimiter);
    let bytes = span.as_bytes();
    let mut offset = 0usize;
    while offset < bytes.len() && is_blank(bytes[offset]) {
        offset += 1;
    }
    bytes.len() - offset
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
    // C's `number()` does `SKIP_BLANKS` first, then takes one sign: `-` flips negative, `+` is
    // merely stepped over (`fields.c:283-288`). Recognising only `-` used to send `+7` down the
    // "no digit converted" branch (empty integer part), differing from `7`/`007` — the reference
    // measures all three as equal.
    let negative = bytes.get(cursor) == Some(&b'-');
    if matches!(bytes.get(cursor), Some(b'-') | Some(b'+')) {
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
        let fraction = &bytes[fraction_start
            ..cursor
                + bytes[fraction_start..]
                    .iter()
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

/// Normalise a key into a comparable shape: strip the integer part's leading zeros. C does it
/// with two sentences, "eat initial zeroes" (`fields.c:290-292`) and "Add key terminator,
/// deleting any trailing \"00\"" (`:373-374`, using `last_nz_pos` to step the trailing-zero
/// bytes back).
/// **This tree does not need the trailing-fraction-zero sentence**: `compare_numeric`'s
/// digit-by-digit comparison treats the shorter side as zero-padded, so `1.5` and `1.50` already
/// land on the same side (this was forced by a negative control — delete the trailing-zero
/// stripping and both the new and the existing tests stay green, which shows those lines are a
/// redundant branch rather than a criterion; better to delete redundancy than to invent a test
/// that calls it necessary).
fn normalized_parts<'a>(key: &NumericKey<'a>) -> (&'a [u8], &'a [u8]) {
    let mut integer = key.integer;
    while let Some((b'0', rest)) = integer.split_first() {
        integer = rest;
    }
    (integer, key.fraction)
}

/// The zero criterion normalises to "both parts empty": C's "If the first/next character isn't a
/// digit … value is zero" (`fields.c:307-312`) folds `0`, `000`, `0.00`, `.0` and a `fubar` that
/// converts nothing into the same zero key (the terminator it writes does not even carry
/// `negate`, so `-0` and `+0` are equal too; the reference measures the same). `0.00` lands here
/// through zero-padded comparison: its integer part is eaten by the leading zeros and its
/// fraction digits each equal the other side's empty positions.
fn is_numeric_zero(key: &NumericKey<'_>) -> bool {
    let (integer, fraction) = normalized_parts(key);
    integer.is_empty() && fraction.is_empty()
}

fn compare_numeric(a: &NumericKey<'_>, b: &NumericKey<'_>) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    // Check zero first: two zeros are a tie and never reach the sign bit (otherwise `-0` and
    // `+0` would be split by the sign).
    if is_numeric_zero(a) && is_numeric_zero(b) {
        return Ordering::Equal;
    }
    match (a.negative, b.negative) {
        (false, true) => return Ordering::Greater,
        (true, false) => return Ordering::Less,
        _ => {}
    }
    let (ai, af) = normalized_parts(a);
    let (bi, bf) = normalized_parts(b);
    // Integer parts: compare the normalised digit counts first (i.e. the magnitude), then digit
    // by digit lexicographically — for unsigned decimal strings that is numeric order, matching
    // C's "exponent + mantissa" byte-order encoding.
    let magnitude = match ai.len().cmp(&bi.len()) {
        Ordering::Equal => ai.cmp(bi),
        other => other,
    };
    let magnitude = if magnitude != Ordering::Equal {
        magnitude
    } else {
        // Fraction digits compared one by one, with the shorter side treated as zero-padded
        // (`1.5` against `1.45`).
        let mut ordering = Ordering::Equal;
        for index in 0..af.len().max(bf.len()) {
            let x = af.get(index).copied().unwrap_or(b'0');
            let y = bf.get(index).copied().unwrap_or(b'0');
            if x != y {
                ordering = x.cmp(&y);
                break;
            }
        }
        ordering
    };
    if a.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Compares two lines per the options: each key spec contributes a
/// comparison, ties fall through to the next key; only `-S` breaks them by the
/// whole record (`posix_tiebreak`, see `compare`), and the global `-r` inverts
/// the outcome.
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
            by_length: options.by_length,
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
        let ordering = if key.by_length {
            // C's branch is `length()`: after skipping the leading blanks it renders "how many
            // bytes remain in the span" as a decimal string and hands it to `number()`. For a pure
            // decimal string numeric order is integer order, so comparing the lengths directly is
            // equivalent.
            span_length(a, key, options.delimiter).cmp(&span_length(b, key, options.delimiter))
        } else if key.numeric {
            compare_numeric(&numeric_key(ka.as_bytes()), &numeric_key(kb.as_bytes()))
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
    if result == Ordering::Equal && options.posix_tiebreak {
        // C's `-S` branch: when keys are equal, compare the whole record (`fields.c:168` extends
        // `keylen` to include the original record), and the global `-r` applies to this step as
        // well. Modifier letters belong to the key, not to the fallback, so the raw, unfiltered
        // bytes are used here. Without `-S` **this branch is not entered**: by default C does not
        // re-sort by record and tied keys keep input order (the stability of the sorting path and
        // the inversion under `-r` live in `bin/sort.rs`, the only place that sees input sequence
        // numbers).
        let raw = a.as_bytes().cmp(b.as_bytes());
        return if options.reverse { raw.reverse() } else { raw };
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
    use alloc::string::ToString;

    /// How the test fixture takes argv: literals are still written as `&[&str]`, and the raw byte
    /// slice of each word is returned (borrowed from the literal, the same layer as the thin
    /// shell's `args_bytes()`). `parse_options` takes `&[&[u8]]`, and both lifetimes are the same
    /// one, so a call whose result is kept for assertions must be bound to a variable first.
    fn argv<'a>(words: &[&'a str]) -> Vec<&'a [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    /// An out-of-set letter returns the **first byte** after the dash, not the first character:
    /// `-\u{e9}` (`c3 a9`) is U+00C3 in the `char` domain. For the absolute reading see the probe
    /// note in the face-door comment of `tests/sort_face.rs` (a probe built against Minix's
    /// `getopt.c` returns `optopt=0303`).
    /// What the argument seam changes for `sort` is **the value itself**, not the message
    /// rendering: `-t` takes only the first byte (`d_mask[(u_char)*optarg]` at `sort.c:219`; C
    /// never checks the length), `-R` takes the first byte as the value and then decides from the
    /// remainder whether it is numeric or an error (`:226-238`), and the error sentence carries the
    /// `optarg`'s **raw bytes**. Under a text seam a bare `ff` is first folded into `ef bf bd`, so
    /// all three go wrong together: the delimiter becomes `ef`, the record delimiter becomes `ef`,
    /// and the echoed position is a three-chunk folded string. The worst shape is a **false
    /// clash** — `-R\xef -t\xff` are two different bytes in C (accepted), but after one fold both
    /// become `ef`, and `:217-218`'s `record/field delimiter clash` fires out of nowhere. The
    /// reference measured `-R\xef -t\xff` returning 0 and `-Rq\xff` returning
    /// `sort: invalid record delimiter q\xff`.
    #[test]
    fn the_value_letters_take_argv_bytes_not_the_lossy_form_of_them() {
        // The clash criterion follows C's two different types: `-R`'s **character path** hands
        // `*optarg` (a signed `char`) to the `int REC_D` (`sort.c:226`), which is then written to
        // `d_mask[REC_D]` (`:241`) — a high byte therefore lands outside the table: it neither
        // clashes with `-t` (`:219-220` compares `d_mask[(u_char)*optarg] & REC_D_F`) nor ever
        // matches an input byte (`getrec` compares the 0..=255 that `getc` returns). Both sides
        // agree, and the five shapes were measured live.
        let high_char: Vec<&[u8]> = vec![b"-R\xff", b"-t\xff"];
        let (options, _) = parse_options(&high_char).expect("-R<ff> -t<ff> 在 C 里不冲突");
        assert_eq!(options.record_delim, -1, "字符路径把 ff 落成 -1");
        assert_eq!(
            options.delimiter,
            Some(0xff),
            "字段分隔符取的是那一枚原字节"
        );

        // The same 255 goes down the **numeric path** instead (`-R\377` through `strtol`) and is
        // the positive 255, landing on `d_mask[255]`, so it clashes with `-t`'s raw byte — one `ff`
        // with two outcomes.
        let numeric_255: Vec<&[u8]> = vec![b"-R\\377", b"-t\xff"];
        assert_eq!(
            parse_options(&numeric_255).err(),
            Some(ParseError::RecordFieldDelimiterClash),
            "数值路径的 255 与字符路径的 ff 撞在同一个掩码槽里"
        );

        // Control: the same byte in ASCII still clashes (`-R,` with `-t,`).
        let ascii_clash: Vec<&[u8]> = vec![b"-R,", b"-t,"];
        assert_eq!(
            parse_options(&ascii_clash).err(),
            Some(ParseError::RecordFieldDelimiterClash)
        );

        // Two different high bytes (-17 and 255) each miss the other ⇒ accepted.
        let both: Vec<&[u8]> = vec![b"-R\xef", b"-t\xff"];
        let (options, _) = parse_options(&both).expect("-R<ef> -t<ff> 是两个不同字节");
        assert_eq!(options.record_delim, -17, "ef 经带符号 `char` 落地是 -17");
        assert_eq!(
            options.delimiter,
            Some(0xff),
            "分隔符必须是那一枚字节，不是折出来的 ef"
        );

        // The numeric-form failure sentence carries `optarg`'s raw bytes from the cursor (only the
        // backslash-prefixed branch does `optarg++` first).
        let bad: Vec<&[u8]> = vec![b"-Rq\xff"];
        assert_eq!(
            parse_options(&bad).err(),
            Some(ParseError::InvalidRecordDelimiter(b"q\xff".to_vec()))
        );

        // Operands are raw bytes too: the file name's **name slot** is spliced by the shell into
        // that `err(2, "%s", argv[i])` sentence.
        let named: Vec<&[u8]> = vec![b"/tmp/nosuch-\xff"];
        let (_, operands) = parse_options(&named).expect("操作数应当收下");
        assert_eq!(operands, vec![&b"/tmp/nosuch-\xff"[..]]);

        // `-k`'s tail takes no part in the echo (none of the five `errx` sentences has `%s`), but
        // the branch criterion looks at bytes: the unrecognised one stops at `setfield`'s `*pos`
        // (`init.c:188-190`).
        let spec: Vec<&[u8]> = vec![b"2\xff"];
        assert_eq!(
            parse_key(spec[0], &Options::default()).err(),
            Some(KeySpecError::IllegalFieldDescriptor)
        );
    }

    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        let unknown = |words: &[&str]| match parse_options(&argv(words)) {
            Err(ParseError::Getopt(crate::GetoptReport::UnknownOption(letter))) => letter,
            other => panic!("应该是集外字母那一支，实际 {other:?}"),
        };
        assert_eq!(
            unknown(&["-\u{e9}"]),
            '\u{00c3}',
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // Cluster: `l` is legal and consumed first, so the report is the first byte after it.
        assert_eq!(unknown(&["-l\u{e9}"]), '\u{00c3}', "-l\u{e9}");
        // The ASCII shape must not be affected by this change.
        assert_eq!(unknown(&["-q"]), 'q', "-q");
        // A value-taking letter swallows the multi-byte value; it is not a letter position.
        let w_args = argv(&["-o\u{e9}out"]);
        let (options, _) = parse_options(&w_args).unwrap();
        assert_eq!(options.output, Some("éout".as_bytes().to_vec()));
    }

    use core::cmp::Ordering;

    fn opts(extra: &str) -> Options {
        let mut words: Vec<&[u8]> = Vec::new();
        for flag in extra.split_whitespace() {
            words.push(flag.as_bytes());
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
        // Tied keys are not split in the comparison half: two lines with the same numeric value
        // and different tails are a tie (the reference: `-n` outputs `1 y`/`1 x` in input order for
        // both orders); only `-S` sorts by record.
        assert_eq!(compare("1 y", "1 x", &options), Ordering::Equal);
        assert_eq!(compare("1 x", "1 y", &options), Ordering::Equal);
        assert_eq!(compare("1 y", "1 x", &opts("-n -S")), Ordering::Greater);
        // Non-numeric prefixes all count as zero and are likewise not split by the record in the
        // comparison half (the reference's `-n` also outputs `abc`/two blanks in input order).
        assert_eq!(compare("abc", "  ", &options), Ordering::Equal);
        // Note: the `+7`/`007`/`7` group **cannot** serve as this example — the reference measures
        // all three equal in C (both input orders come back unchanged), while this tree's numeric
        // key treats `+` as "not a digit", giving 0 for `+7` and 7 for `007` — a difference that the
        // old whole-line fallback happened to mask. It is registered for a separate fix and is not
        // signed off here with the old readings.
    }

    #[test]
    fn test_fold_and_dictionary() {
        // Folding makes the two keys equal, and without `-S` C **does not re-sort by whole
        // record**: the reference outputs `Apple`/`apple` (and the reverse input) in input order, so
        // this must return `Equal`. The old assertion's "fall back to the whole line, `A` before
        // `a`" was an order this tree invented.
        let options = opts("-f");
        assert_eq!(compare("Apple", "apple", &options), Ordering::Equal);
        assert_eq!(compare("apple", "Apple", &options), Ordering::Equal);
        assert_eq!(compare("Apple", "Apple", &options), Ordering::Equal);
        // Only `-S` brings the record in: then `A` (0x41) comes before `a` (0x61), the same shape
        // as the reference's `printf 'ab\nAB\n' | sort -f -S` putting `AB` first.
        assert_eq!(compare("apple", "Apple", &opts("-f -S")), Ordering::Greater);
        let dictionary = opts("-d");
        // The `+` byte is filtered out on both sides: "ab" against "abc" — the keys themselves
        // differ, which has nothing to do with ties.
        assert_eq!(compare("a+b", "a+bc", &dictionary), Ordering::Less);
        // When the filtered forms really are equal it is still a tie (the reference's `-d` outputs
        // `a+b`/`ab` in input order).
        assert_eq!(compare("a+b", "ab", &dictionary), Ordering::Equal);
    }

    #[test]
    fn test_key_spec_field_selection_with_delimiter() {
        let global = opts("-t:");
        let spec = parse_key(b"2,2", &global).unwrap();
        let mut options = opts("-t:");
        options.keys.clear();
        options.keys.push(spec);
        // -t: -k2,2 compares only the second colon field; different fields sort by it, and equal
        // fields are **not split here**.
        assert_eq!(compare("x:b:zz", "a:a:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:zz", "a:b:yy", &options), Ordering::Equal);
        // Both keys are field two's `b`, so a difference in the third segment creates no order (the
        // old form reported `Less` via the whole-line fallback — an invented order); only `-S` can
        // split them.
        assert_eq!(compare("x:b:1", "x:b:2", &options), Ordering::Equal);
        assert_eq!(
            compare("x:b:1", "x:b:2", &opts("-t: -k 2,2 -S")),
            Ordering::Less
        );
        // Breaking ties by the tail needs `-S`: on the reference, lines with equal keys and
        // different tails come back in input order under `-k 2,2`, while `-S -k 2,2` sorts by
        // record.
        assert_eq!(
            compare("x:b:zz", "a:b:yy", &opts("-t: -k 2,2 -S")),
            Ordering::Greater
        );
    }

    /// `.0` is not symmetric between the two columns: only the start column does `col->indent--`
    /// (`init.c:168-169`), so `.0` on the start column computes -1 and takes `illegal offset`,
    /// while `.0` on the end column (and a lone dot) has indent 0, meaning "to end of field" — the
    /// reference accepts that too.
    #[test]
    fn dot_zero_is_an_error_only_on_the_start_column() {
        assert_eq!(
            parse_key(b"1.0", &opts("")).err(),
            Some(KeySpecError::IllegalOffset)
        );
        assert_eq!(parse_key(b"1.2", &opts("")).unwrap().start_char, 2);
        assert!(parse_key(b"1,2.0", &opts("")).is_ok());
        assert!(parse_key(b"1,2.", &opts("")).is_ok());
    }

    /// Each of the five `errx` sentences has its own shape and its own sentence; the readings were
    /// measured shape by shape against the reference. The old form squeezed the five into one, so
    /// four of them signed with someone else's sentence.
    #[test]
    fn the_five_key_spec_sentences_are_distinct() {
        // `field numbers must be positive` (`init.c:160-161`): zero, a negative, and a shape that
        // converts no digit (`strtol` returns 0 and leaves the pointer alone).
        for spec in ["0", "x", "", ",5", "1,-2", "-1"] {
            assert_eq!(
                parse_key(spec.as_bytes(), &opts("")).err(),
                Some(KeySpecError::FieldNumbersNotPositive),
                "键形 {spec:?}"
            );
        }
        // `illegal offset` (`:168-170`): the start column's `.0` minus one goes negative.
        assert_eq!(
            parse_key(b"1.0", &opts("")).err(),
            Some(KeySpecError::IllegalOffset)
        );
        // `cannot indent end of line` (`:162-163`): the column number is 0 yet `.C` is given.
        assert_eq!(
            parse_key(b"1,0.5", &opts("")).err(),
            Some(KeySpecError::CannotIndentEndOfLine)
        );
        // `illegal field descriptor` (`:186-191`): the start column stops on a character that is
        // not a comma.
        for spec in ["1 2", "1n2", "1.2.3", "1M"] {
            assert_eq!(
                parse_key(spec.as_bytes(), &opts("")).err(),
                Some(KeySpecError::IllegalFieldDescriptor),
                "键形 {spec:?}"
            );
        }
        // `fields out of order` (`:213-217`): reported only when all three conditions hold.
        assert_eq!(
            parse_key(b"2.5,1.2", &opts("")).err(),
            Some(KeySpecError::FieldsOutOfOrder)
        );
        // Missing one means no report: the end column gives no `.C` (indent 0, and the source
        // comment says "i.e. okay"), or the end column number is larger than the start column — the
        // normal way to chain them.
        assert!(parse_key(b"2,1", &opts("")).is_ok());
        assert!(parse_key(b"1,2.1", &opts("")).is_ok());
        // `strtol`'s tolerant shapes are accepted too: leading blanks, a plus sign, extra leading
        // zeros.
        for spec in ["+1", " 1", "01"] {
            assert_eq!(
                parse_key(spec.as_bytes(), &opts("")).unwrap().start_field,
                1,
                "{spec:?}"
            );
        }
    }

    /// `-l` sorts by "how many bytes remain in the span" (C `fields.c:length()`:
    /// `SKIP_BLANKS(line)` first, then `lineend - line` rendered as a decimal string and handed to
    /// `number()`). The four readings below were measured directly against the reference: an empty
    /// line (0), `b 2` (1), `cc 3` (2), `aaa 1` (3). Because it measures length rather than
    /// content, `b 2` sorts before `aaa 1` (the lexicographic order is inverted).
    #[test]
    fn sort_by_length_orders_by_span_length_not_content() {
        let by_length = opts("-l");
        assert_eq!(compare("b 2", "aaa 1", &by_length), Ordering::Less);
        assert_eq!(compare("cc 3", "aaa 1", &by_length), Ordering::Less);
        assert_eq!(compare("", "b 2", &by_length), Ordering::Less);
        // Equal lengths are a tie: without `-S` the comparison half does not split by record (`-l`
        // on the reference outputs equal-length lines in input order), and the sorting step keeps
        // input order.
        assert_eq!(compare("ab 1", "cd 2", &by_length), Ordering::Equal);
        // A global `-l` and a local `-k 1l` order identically (`-l` goes through `fldtab[0].flags`,
        // `sort.c:168-172`).
        assert_eq!(compare("b 2", "aaa 1", &opts("-k 1l")), Ordering::Less);
        // The leading-blank skip happens only in this key's branch (`SKIP_BLANKS`), so under `-t:`
        // the length of "two blanks plus aa" is 2, not 4 — the reference gives the order `b:2`,
        // `  aa:1`, `ccc:3`.
        let delimited = opts("-t: -k 1l");
        assert_eq!(compare("  aa:1", "b:2", &delimited), Ordering::Greater);
        assert_eq!(compare("  aa:1", "ccc:3", &delimited), Ordering::Less);
    }

    /// When `l` and `n` are both present the length wins: C checks `L` before `N` in `enterfield`
    /// (`fields.c:208-211`), and the reference measures `-k 1ln` and `-k 1nl` with exactly the same
    /// readings as `-k 1l` (not numeric order: neither `aaa` nor `cc` has a numeric prefix, so as
    /// numbers both are 0 and tie).
    #[test]
    fn the_length_flag_wins_over_numeric() {
        for spec in ["1ln", "1nl"] {
            let key = parse_key(spec.as_bytes(), &opts("")).unwrap();
            assert!(key.by_length && key.numeric, "{spec} 两位都该点亮");
            assert_eq!(
                compare("cc 3", "aaa 1", &opts(&format!("-k {spec}"))),
                Ordering::Less,
                "{spec}：长度序优先，不是数值序也不是字典序"
            );
        }
    }

    /// Numeric-key normalisation follows C's `number()`: the nine pairs below all come back in
    /// input order from the reference (i.e. equal) — a plus sign is merely stepped over
    /// (`fields.c:283-288`), leading zeros are eaten (`:290-292`), trailing fraction zeros are
    /// stepped back with the terminator (`:373-374`), and the zero key carries no `negate`, so `-0`
    /// and `+0` are equal too.
    #[test]
    fn numeric_keys_normalize_sign_leading_and_trailing_zeros() {
        for (a, b) in [
            ("007", "7"),
            ("+7", "7"),
            ("1.5", "1.50"),
            (".5", "0.5"),
            ("0.0", "0"),
            ("+0", "-0"),
            ("12.0", "12"),
            ("  12", "12"),
            ("fubar", "0"),
        ] {
            assert_eq!(compare(a, b, &opts("-n")), Ordering::Equal, "{a} 对 {b}");
            assert_eq!(compare(b, a, &opts("-n")), Ordering::Equal, "{b} 对 {a}");
        }
        // Zero-padded comparison, not stripping, is what carries "trailing zero equivalence":
        // `1.4` and `1.40` are equal (both input orders come back unchanged), yet `1.4` and `1.45`
        // are **not** equal — the deciding digit lies beyond the shorter side's length, and only
        // digit-by-digit comparison with zero padding separates them (the reference puts `1.4` first
        // in both directions).
        assert_eq!(compare("1.4", "1.40", &opts("-n")), Ordering::Equal);
        assert_eq!(compare("1.40", "1.4", &opts("-n")), Ordering::Equal);
        assert_eq!(compare("1.4", "1.45", &opts("-n")), Ordering::Less);
        assert_eq!(compare("1.45", "1.4", &opts("-n")), Ordering::Greater);
        // Normalising is not discarding the magnitude: the digit-count comparison runs after zero
        // stripping, and the sign still dominates.
        assert_eq!(compare("0007", "0006", &opts("-n")), Ordering::Greater);
        assert_eq!(compare("-0.5", "-0.05", &opts("-n")), Ordering::Less);
        // C does not recognise exponents (`fields.c:315`'s comment reads "Maybe here we should allow
        // for e+12 (etc)"): the key of `1e3` is just 1 (it stops at `e`), so it comes before `1000`
        // — the reference gives that order in both directions.
        assert_eq!(compare("1e3", "1000", &opts("-n")), Ordering::Less);
        assert_eq!(compare("1000", "1e3", &opts("-n")), Ordering::Greater);
    }

    /// The inheritance criterion at `init.c:193-194` is "not one letter in the local segment",
    /// which then takes the global flags wholesale — not a bitwise OR. This shape was measured
    /// against the reference: `-n -k 2r` and `-k 2r` order identically (reverse lexicographic), not
    /// like the reverse of `-n -k 2` (which is what ORing the global bit in would give).
    #[test]
    fn a_local_modifier_switches_the_global_flags_off() {
        // Global `-n` and no local letter: inherit, so 2 comes before 10.
        assert_eq!(compare("x 2", "y 10", &opts("-n -k 2")), Ordering::Less);
        // Global `-n` but a local `r`: this branch's **key** does not inherit `n` and only
        // reverses the lexicographic key — so the two commands produce the same order. What is
        // compared is that one key spec, not the whole [`Options`] (the global bit is of course
        // still recorded on Options, where there is no difference to check).
        let w_args = argv(&["-n", "-k", "2r"]);
        let with_global = parse_options(&w_args).unwrap().0;
        let w_args = argv(&["-k", "2r"]);
        let without_global = parse_options(&w_args).unwrap().0;
        assert_eq!(
            with_global.keys, without_global.keys,
            "局部有字母时全局标志不得逐位渗进来"
        );
        assert!(!with_global.keys[0].numeric);
        assert!(with_global.numeric, "全局位本身仍照 C 存在 gflag 里");
        // The "empty local" branch does take the global: the same `-k 2` with and without `-n`
        // gives two different orders.
        assert!(parse_options(&argv(&["-n", "-k", "2"])).unwrap().0.keys[0].numeric);
    }

    #[test]
    fn test_local_reverse_drops_under_global() {
        // init.c:195-197: a local `r` does not invert a global `-r`; the
        // key parses without reversal.
        let global = opts("-r");
        let key = parse_key(b"1r", &global).unwrap();
        assert!(!key.reverse);
    }

    #[test]
    fn test_fields_split_on_blank_runs() {
        let global = opts("");
        let spec = parse_key(b"2,2", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        assert_eq!(compare("aa b cc", "xx y zz", &options), Ordering::Less);
        assert_eq!(compare("a y z", "b x w", &options), Ordering::Greater);
    }

    /// The tie rule (fixed against the reference): without `-S` the comparison half returns
    /// `Equal` and the sorting step's stability keeps input order; only `-S` splits ties by the
    /// whole record.
    /// The `-R`/`-t` relationship follows C's mask criterion (`sort.c:217-218` checks
    /// `d_mask[c] & REC_D_F` only on the `-t` branch): the clash is `-t` hitting the current record
    /// delimiter, not the reverse.
    /// The reference reads the same (`-R , -t ,` reports clash and exits 2; `-t , -R ,` passes
    /// silently).
    #[test]
    fn dash_r_moves_the_record_delimiter_and_dash_t_clashes_with_it() {
        let w_args = argv(&["-R", ","]);
        let (o, _) = parse_options(&w_args).unwrap();
        assert_eq!(o.record_delim, 44);
        assert_eq!(
            parse_options(&argv(&["-R", ",", "-t", ","])).err(),
            Some(ParseError::RecordFieldDelimiterClash)
        );
        let w_args = argv(&["-t", ",", "-R", ","]);
        let (o, _) = parse_options(&w_args).unwrap();
        assert_eq!(o.record_delim, 44, "反向在 C 里不查，所以只有后一个生效");
        assert_eq!(o.delimiter, Some(b','));
        // The default record delimiter is `'\n'`, so without `-R`, `-t '\n'` clashes too (the
        // reference reports the same sentence).
        assert_eq!(
            parse_options(&argv(&["-t", "\n"])).err(),
            Some(ParseError::RecordFieldDelimiterClash)
        );
    }

    /// `record_delim_number` clause by clause against C's `strtol` plus the three rejection
    /// criteria.
    #[test]
    fn record_delim_number_mirrors_strtol_and_its_three_refusals() {
        assert_eq!(record_delim_number(b"011", 8), Some(9), "-R '\\011' → 制表");
        assert_eq!(record_delim_number(b"12", 8), Some(10), "-R '\\12' → 换行");
        assert_eq!(
            record_delim_number(b"011", 0),
            Some(9),
            "base 0 的前导 0 走八进制"
        );
        assert_eq!(record_delim_number(b"0x10", 0), Some(16));
        assert_eq!(
            record_delim_number(b" 12", 0),
            Some(12),
            "前导空白被 strtol 吃掉"
        );
        assert_eq!(
            record_delim_number(b"", 8),
            Some(0),
            "`-R '\\'` 是零字节不是报错"
        );
        assert_eq!(record_delim_number(b"q", 8), None);
        assert_eq!(record_delim_number(b"300", 0), None, "超出 NBINS(256)");
        assert_eq!(record_delim_number(b"-5", 0), None, "负数");
        assert_eq!(
            record_delim_number(b"1 ", 0),
            None,
            "尾空白：`*ep != '\\0'`"
        );
    }

    /// The numeric form is taken only when the value is longer than one byte, and a single byte is
    /// a **literal byte**: the character of `-R 0` is `'0'`.
    #[test]
    fn dash_r_single_byte_is_literal_and_newline_short_circuits_the_arm() {
        let w_args = argv(&["-R", "0"]);
        let (o, _) = parse_options(&w_args).unwrap();
        assert_eq!(o.record_delim, 48);
        let w_args = argv(&["-R", "\n"]);
        let (o, _) = parse_options(&w_args).unwrap();
        assert_eq!(o.record_delim, 10, "C 在 `REC_D == '\\n'` 处直接 break");
        // Ending early on the same branch means the following characters are ignored entirely.
        let w_args = argv(&["-R", "\nxy"]);
        let (o, _) = parse_options(&w_args).unwrap();
        assert_eq!(o.record_delim, 10);
        assert_eq!(
            parse_options(&argv(&["-R"])).err(),
            Some(ParseError::Getopt(crate::GetoptReport::MissingValue('R')))
        );
    }

    /// The "given twice" criterion is "the current value no longer equals `'\n'`", not a count:
    /// when the first computes back to 10 the second is legal (the reference gives normal output
    /// for `-R '\12' -R ,`).
    #[test]
    fn a_second_dash_r_only_reports_when_the_first_one_stuck() {
        assert_eq!(
            parse_options(&argv(&["-R", ",", "-R", "x"])).err(),
            Some(ParseError::MultipleRecordDelimiters)
        );
        let w_args = argv(&["-R", "\\12", "-R", ","]);
        let (o, _) = parse_options(&w_args).unwrap();
        assert_eq!(o.record_delim, 44);
        assert_eq!(
            parse_options(&argv(&["-R", "300"])).err(),
            Some(ParseError::InvalidRecordDelimiter(b"300".to_vec()))
        );
        // The backslash prefix is skipped, so the message carries the remaining string.
        assert_eq!(
            parse_options(&argv(&["-R", "\\q"])).err(),
            Some(ParseError::InvalidRecordDelimiter(b"q".to_vec()))
        );
    }

    #[test]
    fn a_tie_stays_tied_and_capital_s_breaks_it_by_the_record() {
        let mut options = opts("");
        options.keys.push(parse_key(b"1,1.1", &opts("")).unwrap());
        // A one-byte key that is equal is a tie (the old form fell back to the whole line,
        // inventing an order).
        assert_eq!(compare("a123", "a456", &options), Ordering::Equal);
        assert_eq!(compare("a456", "a123", &options), Ordering::Equal);
        options.posix_tiebreak = true;
        assert_eq!(compare("a123", "a456", &options), Ordering::Less);
        // `-u` zeroes this bit **after** the option ring (`sort.c:256-258`), so both spellings are
        // equivalent.
        assert!(
            !parse_options(&argv(&["-S", "-u"]))
                .unwrap()
                .0
                .posix_tiebreak
        );
        assert!(
            !parse_options(&argv(&["-u", "-S"]))
                .unwrap()
                .0
                .posix_tiebreak
        );
        assert!(parse_options(&argv(&["-S"])).unwrap().0.posix_tiebreak);
        // `-s` is still a no-op: it writes the default side (`sort.c:194-201`).
        assert!(!parse_options(&argv(&["-s"])).unwrap().0.posix_tiebreak);
    }

    #[test]
    fn test_parse_options_takes_the_output_path() {
        // `-o` takes its value inline or as the next word, and a later one replaces
        // the earlier, because C's `case 'o'` is a plain `outpath = optarg`
        // assignment (`sort.c:188-190`). Without the option the field stays `None`,
        // which is what distinguishes standard output from `-o ""`.
        let w_args = argv(&["-o", "out.txt", "in.txt"]);
        let (options, operands) = parse_options(&w_args).unwrap();
        assert_eq!(options.output.as_deref(), Some("out.txt".as_bytes()));
        assert_eq!(operands, vec![b"in.txt".as_slice()]);
        let w_args = argv(&["-oout", "in"]);
        let (inline, _) = parse_options(&w_args).unwrap();
        assert_eq!(inline.output.as_deref(), Some("out".as_bytes()));
        let w_args = argv(&["-oa", "-ob"]);
        let (replaced, _) = parse_options(&w_args).unwrap();
        assert_eq!(replaced.output.as_deref(), Some("b".as_bytes()));
        let w_args = argv(&["-n"]);
        let (none, _) = parse_options(&w_args).unwrap();
        assert_eq!(none.output, None);
        assert_eq!(
            parse_options(&argv(&["-o"])),
            Err(ParseError::Getopt(crate::GetoptReport::MissingValue('o')))
        );
    }

    /// Each of the three exits has its own type, and only two of them carry libc's line in front
    /// (the shell picks the line count from the type). The out-of-set-letter branch has its own test
    /// (`test_parse_options_rejects_unknown`) and is not repeated here.
    #[test]
    fn test_parse_options_splits_the_three_error_paths() {
        use crate::GetoptReport;
        // Missing value: the same `Getopt` type as an out-of-set letter, but a different sentence.
        assert_eq!(
            parse_options(&argv(&["-t"])),
            Err(ParseError::Getopt(GetoptReport::MissingValue('t')))
        );
        // Wrong key shape: one of C's five `errx(2, …)` sentences, with no usage.
        assert_eq!(
            parse_options(&argv(&["-k", "x"])),
            Err(ParseError::KeySpec(KeySpecError::FieldNumbersNotPositive))
        );
        assert_eq!(
            parse_options(&argv(&["-k", "1 2"])),
            Err(ParseError::KeySpec(KeySpecError::IllegalFieldDescriptor))
        );
        // Only a second `-t` is `usage("multiple field delimiters")`; a multi-byte value itself is
        // **not** an error (C takes only the first byte).
        let w_args = argv(&["-t,", "f"]);
        let (one, _) = parse_options(&w_args).unwrap();
        assert_eq!(one.delimiter, Some(b','));
        let w_args = argv(&["-t::", "f"]);
        let (two, _) = parse_options(&w_args).unwrap();
        assert_eq!(two.delimiter, Some(b':'), "多字节取首字节，不报错");
        assert_eq!(
            parse_options(&argv(&["-t,", "-t;"])).err(),
            Some(ParseError::MultipleFieldDelimiters)
        );
        // An empty value picks up that `\0` (C's `*optarg`).
        let w_args = argv(&["-t", "", "f"]);
        let (nul, _) = parse_options(&w_args).unwrap();
        assert_eq!(nul.delimiter, Some(0));
    }

    #[test]
    fn test_output_target_follows_what_lstat_said() {
        // The three answers of `sort.c:320-355`, and the fourth row that keeps
        // `-o ""` distinct from no option at all: an empty path is still a path, so
        // it goes to the direct branch and the open fails there.
        assert_eq!(decide_output(None, None), Output::StandardOutput);
        assert_eq!(
            decide_output(Some(b"new"), None),
            Output::Direct {
                path: b"new".to_vec()
            }
        );
        assert_eq!(
            decide_output(Some(b""), None),
            Output::Direct { path: Vec::new() }
        );
        assert_eq!(
            decide_output(
                Some(b"dev"),
                Some(OutputProbe {
                    mode: 0o666,
                    is_device: true
                }),
            ),
            Output::Direct {
                path: b"dev".to_vec()
            }
        );
        assert_eq!(
            decide_output(
                Some(b"old"),
                Some(OutputProbe {
                    mode: 0o640,
                    is_device: false
                }),
            ),
            Output::ViaTemp {
                path: b"old".to_vec(),
                mode: 0o640
            }
        );
    }

    #[test]
    fn test_temporary_names_keep_the_prefix_and_walk_forward() {
        // C writes `%sXXXXXX` and lets `mkstemp` fill the six slots from a
        // sixty-two-character alphabet (`gettemp.c:49-50`), carrying forward while a
        // create collides. What the caller relies on is the prefix, the length, and
        // that consecutive attempts differ.
        let first = temp_candidate(b"out.txt", 0x1234, 0);
        let second = temp_candidate(b"out.txt", 0x1234, 1);
        assert_eq!(first.len(), b"out.txt".len() + 6);
        assert!(first.starts_with(b"out.txt"));
        assert_ne!(first, second, "consecutive attempts must differ");
        for byte in &first[b"out.txt".len()..] {
            assert!(
                b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".contains(byte),
                "suffix byte {byte:#04x} is outside C's padchar set"
            );
        }
    }

    #[test]
    fn test_parse_options_rejects_unknown() {
        assert_eq!(
            parse_options(&argv(&["-Z"])),
            Err(ParseError::Getopt(crate::GetoptReport::UnknownOption('Z')))
        );
    }

    #[test]
    fn test_parse_options_accepts_merge() {
        let w_args = argv(&["-m", "a.txt"]);
        let (options, operands) = parse_options(&w_args).unwrap();
        assert!(options.merge);
        assert_eq!(operands, [b"a.txt".as_slice()]);
    }

    #[test]
    fn test_parse_options_accepts_h_and_s_as_noops() {
        // C accepts `-H` ("now the default") and `-s` (stable = default) with no
        // observable effect; they must parse cleanly (not BadOption) and change
        // no flag — the previous behavior errored them, diverging from C.
        let w_args = argv(&["-H", "-s", "data.txt"]);
        let (options, operands) = parse_options(&w_args).unwrap();
        assert!(!options.merge);
        assert!(!options.unique);
        assert!(!options.check);
        assert!(!options.numeric);
        assert_eq!(operands, [b"data.txt".as_slice()]);
        // Clustered in one argument too (getopt treats each letter separately).
        let w_args = argv(&["-sH"]);
        let (clustered, _) = parse_options(&w_args).unwrap();
        assert!(!clustered.numeric);
    }

    #[test]
    fn test_merge_sorted_two_runs_interleave() {
        let options = Options::default();
        let a = ["a1".to_string(), "a3".to_string(), "a5".to_string()];
        let b = ["a2".to_string(), "a4".to_string()];
        let merged = merge_sorted(&[&a, &b], &options);
        assert_eq!(
            merged,
            ["a1", "a2", "a3", "a4", "a5"].map(String::from).to_vec()
        );
    }

    #[test]
    fn test_merge_sorted_equal_keys_keep_earlier_run_first() {
        let options = Options::default();
        let a = ["x".to_string(), "y".to_string()];
        let b = ["x".to_string()];
        let merged = merge_sorted(&[&a, &b], &options);
        // run a's "x" precedes run b's "x": a stable, left-biased merge.
        assert_eq!(merged, ["x", "x", "y"].map(String::from).to_vec());
    }
}
