//! `join` deciding half: merge two field-structured inputs on a key field.
//!
//! Ground truth: `minix3/usr.bin/join/join.c` — inputs are read grouped by
//! their (equal) join field (`slurp`, lines 300-403), the two files are
//! merged key by key (`cmp`/`joinlines`, lines 405-450), and each matched pair
//! prints the join field then the remaining fields of file 1 then of file 2
//! (`outtwoline` default branch, lines 476-489); unmatched lines print all
//! their own fields when `-a` asks (`outoneline`, lines 447-466). Fields come
//! from `strsep(&bp, tabchar)` (`:378`), which cuts **one field per delimiter
//! byte** — so `a,,c` is three fields and a trailing delimiter leaves a
//! trailing empty one. `spans` (default on, cleared by `-t`, `:106`/`:200`)
//! decides whether those empty fields are dropped; dropping them is what makes
//! the default `" \t"` behave as runs.
//!
//! Wired here: the default join (field 1 both sides), `-t` (delimiter +
//! output separator = that byte), `-1`/`-2` (per-file join field), `-j` (both
//! join fields at once — the CLI alias for `-1 N -2 N`), `-a` (unmatched
//! lines), `-v` (report only unpairables from one file, suppressing matched
//! lines via the C `joinout` global), `-o` (output-field list) and `-e`
//! (empty replacement). Declared residual: the obsolete one-argument form (a
//! bare `-1`/`-2`/`-o`/`-t` without a value) — this module models each option
//! independently, so the caller supplies the values it does not set as defaults.
//!
//! Pure: takes the two inputs as byte-line slices and returns the output
//! lines, so the merge is unit-tested without files or the IPC layer.

use crate::GetoptReport;
use alloc::vec::Vec;

/// Which side an output-field entry draws from (the `F` of `join -o F.N`).
///
/// This tree has only two arms: C's `fieldarg` only looks at whether the
/// token's first character is `1`/`2` (`join.c:526-527`), so the `0` arm
/// ("the join field itself") exists on BSD/GNU but **does not exist in this
/// tree's ground truth** — the reference measured `-o 0` as
/// `malformed -o option field`. This project is a rewrite that preserves
/// external behaviour and does not treat BSD convention as truth, so no
/// `Join` variant is kept here (keeping one would be a dead end with no
/// production source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// A field of file 1.
    One,
    /// A field of file 2.
    Two,
}

/// One `join -o` output field: a `Which` and a 0-based field index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OField {
    pub which: Which,
    pub idx: usize,
}

/// Join options, mirroring the C globals `join` carries per run.
#[derive(Debug, Clone)]
pub struct JoinOptions {
    /// Field index (0-based) that keys file 1 (`-1`, minus one).
    pub field1: usize,
    /// Field index that keys file 2 (`-2`, minus one).
    pub field2: usize,
    /// Delimiter **set** — C's `tabchar` (`join.c:108`): the two bytes `" \t"`
    /// when no `-t` was given, otherwise the single `-t` byte (C rejects any
    /// argument that isn't exactly one byte, `:201-204`).
    pub tabchars: Vec<u8>,
    /// C's `spans` (`join.c:106`): span runs of the delimiters, i.e. drop the
    /// empty fields the cut produces. The default sets it; `-t` clears it.
    pub spans: bool,
    /// Emit unpaired lines from file 1 (`-a 1`).
    pub unmatched1: bool,
    /// Emit unpaired lines from file 2 (`-a 2`).
    pub unmatched2: bool,
    /// Explicit output field list (`-o`); empty selects the default layout
    /// (join field, then file 1's rest, then file 2's rest).
    pub output_fields: Vec<OField>,
    /// Replacement for a missing/unpaired field (`-e`); `None` leaves it blank.
    pub empty: Option<Vec<u8>>,
    /// Emit the matched-line output. `-v` clears it so only the unpaired lines
    /// from the specified file print (C `joinout` global, `join.c:104,207-208`).
    pub joinout: bool,
}

impl Default for JoinOptions {
    fn default() -> Self {
        JoinOptions {
            field1: 0,
            field2: 0,
            tabchars: b" \t".to_vec(),
            spans: true,
            unmatched1: false,
            unmatched2: false,
            output_fields: Vec::new(),
            empty: None,
            joinout: true,
        }
    }
}

impl JoinOptions {
    /// The output separator: C prints `*tabchar` (`join.c:497`), the **first
    /// byte** of the delimiter set — a space by default, the `-t` byte with `-t`.
    pub fn sep(&self) -> u8 {
        self.tabchars.first().copied().unwrap_or(b' ')
    }

    /// Cut this line's fields with its `tabchars` + `spans` (the `strsep`
    /// statement inside C's `slurp`). It is a method so that a caller cannot
    /// pair the "delimiter set" with the wrong "skip empty spans" setting —
    /// they come from the same source in C.
    fn fields_of<'l>(&self, line: &'l [u8]) -> Vec<&'l [u8]> {
        split_fields(line, &self.tabchars, self.spans)
    }
}

/// Parse a `join -o` field list (`"1.2,2.3"`) into entries; tokens are
/// comma/space/tab separated (the C `strsep(", \t")`), each token must be
/// `F.N` (1-based), a field of file `F`; the two refusal shapes are kept apart
/// because C's `fieldarg` prints a different sentence for each (see [`OListError`]).
pub fn parse_olist(spec: &[u8]) -> Result<Vec<OField>, OListError> {
    let mut fields = Vec::new();
    for token in spec.split(|byte| matches!(byte, b',' | b' ' | b'\t')) {
        if token.is_empty() {
            continue;
        }
        // No `0` arm: C's prefix guard only recognizes `1.`/`2.`, and `0`
        // lands on "malformed -o option field" on the spot (`join.c:526-527`;
        // the reference exits 1 for all four shapes `-o 0`, `-o 0,1.1`,
        // `-o 1.1,0` and `-o0`).
        // C's `fieldarg` has two distinct `errx` sentences, so the next two
        // guards cannot be merged: a prefix other than `1.`/`2.`, or a
        // trailing character, takes "malformed -o option field"; a correct
        // shape with field number 0 takes "field numbers are 1 based"
        // (`join.c:526-532`).
        if !matches!(token.first(), Some(b'1' | b'2')) || token.get(1) != Some(&b'.') {
            return Err(OListError::Malformed);
        }
        let num = &token[2..];
        let (value, consumed) = strtol_decimal(num);
        if consumed != num.len() {
            // C's `if (*end)` comes first: any trailing character counts as
            // malformed.
            return Err(OListError::Malformed);
        }
        if value == 0 {
            return Err(OListError::OneBased);
        }
        // Negative and over-wide values are rejected in this tree: C assigns
        // the `strtol` result to `u_long fieldno`, so `-o 1.-1` actually passes
        // in C (wrapping to a huge value). Reproducing that shape would need an
        // output-level reference first, and the probe binary crashes before
        // reaching output (registered in the stage-18 ledger), so it is
        // recorded as a declared difference rather than silently matching C.
        if value < 0 || value > i64::from(u32::MAX) {
            return Err(OListError::Malformed);
        }
        let which = if token[0] == b'1' {
            Which::One
        } else {
            Which::Two
        };
        fields.push(OField {
            which,
            idx: (value as u32 - 1) as usize,
        });
    }
    Ok(fields)
}

/// The two `-o` field failures, matching the two `errx` arms of C's
/// `fieldarg` word for word (different sentences, neither followed by usage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OListError {
    /// The prefix is not `1.`/`2.`, or the field number has a trailing
    /// character (`join.c:527-530`).
    Malformed,
    /// The shape is right but the field number is 0 (`join.c:531-532`).
    OneBased,
}

/// The part of C's `strtol(optarg, &end, 10)` this command actually needs:
/// skip leading whitespace (space, `\t`, `\n`, vertical tab `\x0b`, form feed
/// `\x0c`, carriage return — the same set as `isspace`), then an optional
/// sign, then decimal digits. **With not a single digit consumed the value is
/// 0 and `end` goes back to the start** — this shape is why `-1 x` lands on
/// "less than 1" rather than "illegal field number" (the reference reads the
/// same; `join.c:140-147`). Returns (value, bytes consumed); the caller treats
/// "consumed < length" as equivalent to C's `*end != '\0'`. Saturation is out
/// of scope here (field numbers are far below `LONG_MAX`, C stays `>= 1` after
/// saturating, and clamping here avoids overflow).
#[must_use]
fn strtol_decimal(text: &[u8]) -> (i64, usize) {
    let bytes = text;
    let mut at = 0usize;
    while matches!(
        bytes.get(at),
        Some(b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
    ) {
        at += 1;
    }
    let negative = match bytes.get(at) {
        Some(b'+') => {
            at += 1;
            false
        }
        Some(b'-') => {
            at += 1;
            true
        }
        _ => false,
    };
    let digits_start = at;
    let mut value: i64 = 0;
    while matches!(bytes.get(at), Some(d) if d.is_ascii_digit()) {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(bytes[at] - b'0'));
        at += 1;
    }
    if at == digits_start {
        // No digits: value 0, `end` stops after the sign/whitespace — same
        // shape as C (the sign counts as consumed).
        return (0, at);
    }
    (if negative { -value } else { value }, at)
}

/// The parsed tail of `join`'s argv: options plus the two file operands.
#[derive(Debug, Clone)]
pub struct JoinArgs<'a> {
    pub opts: JoinOptions,
    pub files: Vec<&'a [u8]>,
}

/// A `join` argument failure. Each variant corresponds to one arm of C
/// `join.c` and **carries that arm's own option letter and rejected value**:
/// C's sentences put the letter first (`"-1 option field number less than 1"`)
/// or extend the `-- ` with `optarg` (`"illegal field number -- %s"`), so both
/// must come back from the deciding half; otherwise the shell can only compose
/// a generic sentence (the earlier state: `join: option field number less than
/// 1`, with the letter and the value lost).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError<'a> {
    /// Only a count other than two operands takes this arm (`join.c:236-237`'s
    /// `if (argc != 2) usage();`): the option loop has already finished, so
    /// there is **no** libc line before it.
    Usage,
    /// A letter outside the set, or a value-taking option without its value:
    /// libc prints one line first, then the same two usage lines.
    Getopt(crate::GetoptReport),
    /// `-1`/`-2`/`-j`: the `strtol` value is < 1 (C `join.c:141`/`:152`/`:186`).
    /// The payload is the letter string as it appears in C's sentence.
    FieldLessThan1(&'static str),
    /// The same three arms but with a non-zero `*end` (`:145`/`:156`/`:190`): C
    /// puts the whole `optarg` after `-- `, and **this arm's sentence omits the
    /// option letter** (so only the value is carried, not an unused letter).
    /// The value handed back is the **raw bytes** of argv — `optarg` is that
    /// slice of `argv` in C, and the echo position must not be rewritten by any
    /// decoding.
    IllegalFieldNumber(&'a [u8]),
    /// `-a`/`-v` file number is not 1/2 (`:171`/`:217`).
    FileNumberNot12(&'static str),
    /// `-a`/`-v` with a non-zero `*end` (`:176`/`:222`); the same shape as the
    /// previous arm, C omits the option letter there too, and the value is
    /// again raw argv bytes.
    IllegalFileNumber(&'a [u8]),
    /// The `-t` string is not a single byte (`:202`) — this arm writes no
    /// option letter.
    BadTabChar,
    /// The `-o` field shape is wrong (the first `fieldarg` `errx`): no usage.
    MalformedOutputField,
    /// The `-o` field number is 0 (the second `fieldarg` `errx`, a different
    /// sentence): no usage.
    FieldNumbersAre1Based,
    /// `-a` and `-v` given together (`:235`'s `errx`): print and leave, no
    /// usage.
    AAndVMutuallyExclusive,
    /// A malformed legacy `-j` spelling: `obsolete()` scans the whole argv
    /// **before** `getopt`, and a `-j` not followed by `1`/`2`, or followed by
    /// a digit run longer than one, immediately calls
    /// `errx(1, "illegal option -- %s", ap)` (`join.c:592`'s `jbad`); the
    /// sentence carries the **whole token** (leading hyphen included). The
    /// reference reads `join -j3 f1 f2` → `illegal option -- -j3`. Because this
    /// pass runs before the option loop, any other error hidden after `-j3`
    /// never gets to be reported first. The token handed back is the raw argv
    /// bytes — C passes `ap` whole into `%s`, and the echo position must not be
    /// rewritten by any decoding.
    IllegalOptionToken(&'a [u8]),
}

/// The field specs in the legacy `-o arg arg` form that get rewritten to
/// `-o<entry>`, matching `join.c:604-609` word for word: the first character
/// is `1` or `2`, the second is `.`, and at least one decimal digit follows
/// (C expresses "at least one, all digits" as
/// `len - 2 != strspn(p + 2, "0123456789")` — the threshold is `>= 3`, not
/// `>= 4`: a one-digit form such as `1.1` really is rewritten in C).
fn is_field_spec(word: &[u8]) -> bool {
    let b = word;
    b.len() >= 3
        && matches!(b[0], b'1' | b'2')
        && b[1] == b'.'
        && b[2..].iter().all(u8::is_ascii_digit)
}

/// Split `join`'s argv tail by BSD `getopt("\01a:e:j:1:2:o:t:v:")` into
/// [`JoinArgs`]; tokens and values are the **raw bytes** of argv. Whatever
/// follows the option letter (`-t,`'s `,`) is either an attached value or the
/// immediately following word, and both are option values — `join.c:133`'s
/// `getopt` accepts either form for options marked with `:`. The old front
/// door `match flag{…}` compared the whole `a[1..]`, so C-legal attached forms
/// such as `join -t, f1 f2`, `-a1` and `-o0,1` fell into "unknown option" by
/// mistake. `--` ends option scanning (BSD getopt); a bare `-` and any
/// non-hyphen token are file operands. C's `obsolete(argv)` (`:132`) rewrites
/// argv in place for three legacy forms — a bare `-a`, `-j[12]`, and
/// `-o arg arg` — and that pass runs before this loop and stops at `--`.
/// The `\01` at the head of the optstring is not decoration: `obsolete()`
/// rewrites the second character of a bare `-a` into `\01` (`join.c:568`), and
/// BSD `getopt` recognizes it with `strchr(ostr, optopt)` (`getopt.c:98`), so
/// that byte must be **in the option string** for `case '\01'` to be reached;
/// its neighbour is `a` and `oli[1] != ':'` (`getopt.c:109`), so it is treated
/// as a valueless letter — that is exactly why the legacy bare `-a` swallows
/// no value.
pub fn parse_join_args<'a>(args: &'a [&'a [u8]]) -> Result<JoinArgs<'a>, JoinError<'a>> {
    let mut opts = JoinOptions::default();
    let mut files: Vec<&'a [u8]> = Vec::new();
    let (mut aflag, mut vflag) = (false, false);
    // The malformed `-j` shape of `obsolete()` calls `errx` before `getopt`
    // (`join.c:570-595`): when what follows `-j` is neither `1`/`2` nor
    // nothing, or is one of those but drags more characters (`-j12`, `-j1x`,
    // `-j3`), it reports "illegal option -- <whole token>" on the spot. This
    // pass only detects errors; it does not rewrite (each arm does its own
    // rewrite).
    {
        let mut k = 0;
        while k < args.len() {
            let tok = args[k];
            if tok == b"--" {
                break;
            }
            let tb = tok;
            if tb.len() >= 2 && tb[0] == b'-' && tb[1] == b'j' {
                let bad = match tb.len() {
                    2 => false,
                    3 => !matches!(tb[2], b'1' | b'2'),
                    _ => true,
                };
                if bad {
                    return Err(JoinError::IllegalOptionToken(tok));
                }
            }
            k += 1;
        }
    }
    let mut i = 0;
    // Take an option value: attached after the letter (`&arg[2..]`) or the
    // immediately following word. On a missing value **that letter** must be
    // handed out: C's arm first has libc print `"<prog>: option requires an
    // argument -- <letter>"` (`getopt.c:132-136`) and only then reaches
    // `usage()`.
    macro_rules! value {
        ($letter:expr) => {{
            let cur = args[i];
            if cur.len() > 2 {
                &cur[2..]
            } else {
                i += 1;
                if i >= args.len() {
                    return Err(JoinError::Getopt(GetoptReport::MissingValue($letter)));
                }
                args[i]
            }
        }};
    }
    // A field number that must be decimal and >= 1. The check order matches C
    // word for word: `strtol` first, then **`< 1` first**, then the trailing
    // character (`join.c:140-147`/`:151-158`/`:184-191`). The order has an
    // observable consequence: `x` converts no digit at all → value 0 → "less
    // than 1" rather than "illegal field number" (the earlier implementation
    // branched on "is it all digits" and sent `x` to the wrong arm; the
    // reference reads the same and pins this shape).
    fn field_no<'b>(v: &'b [u8], opt: &'static str) -> Result<usize, JoinError<'b>> {
        let (value, consumed) = strtol_decimal(v);
        if value < 1 {
            return Err(JoinError::FieldLessThan1(opt));
        }
        if consumed != v.len() {
            return Err(JoinError::IllegalFieldNumber(v));
        }
        Ok(value as usize - 1)
    }
    // A file number that can only be 1/2. C also has two guards and **switches
    // first, then checks the trailing character** (`:165-179`/`:209-224`), so
    // `1x` lands on "illegal file number -- 1x" (the switch accepts the 1
    // first), while `x`, `0` and `3` land on "not 1 or 2" (the earlier
    // implementation put `1x` in the latter arm).
    fn file_no<'b>(
        v: &'b [u8],
        opt: &'static str,
        unpair1: &mut bool,
        unpair2: &mut bool,
    ) -> Result<(), JoinError<'b>> {
        let (value, consumed) = strtol_decimal(v);
        match value {
            1 => *unpair1 = true,
            2 => *unpair2 = true,
            _ => return Err(JoinError::FileNumberNot12(opt)),
        }
        if consumed != v.len() {
            return Err(JoinError::IllegalFileNumber(v));
        }
        Ok(())
    }
    while i < args.len() {
        let a = args[i];
        if a == b"--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        let b = a;
        if b.len() < 2 || b[0] != b'-' {
            files.push(a);
            i += 1;
            continue;
        }
        // The three legacy rewrites of `join.c:obsolete()` (`:547`) land here.
        // That pass runs **before** `getopt` and stops at `--`, so malformed
        // shapes must be reported first, and rewritten shapes must be
        // dispatched as rewritten.
        // Legacy `-j1`/`-j2`: C rewrites the token in place to `-1`/`-2` and
        // hands it to getopt, so the value comes from the **next word** (the
        // inline character is part of the letter, not a value). The reference
        // confirms it: `join -j1 f1 f2` reports
        // `-1 option field number less than 1` — the next word is the file
        // name, which converts no digit at all.
        if b.len() == 3 && b[1] == b'j' && matches!(b[2], b'1' | b'2') {
            i += 1;
            if i >= args.len() {
                return Err(JoinError::Getopt(GetoptReport::MissingValue(char::from(b[2]))));
            }
            let v = args[i];
            if b[2] == b'1' {
                opts.field1 = field_no(v, "-1")?;
            } else {
                opts.field2 = field_no(v, "-2")?;
            }
            i += 1;
            continue;
        }
        // Legacy bare `-a` (no file number): C rewrites `ap[1]` into `\01`,
        // which reaches `case '\01'` (`join.c:135-138`) — `aflag` is raised and
        // **both files** report unpaired lines. This shell used to treat `-a`
        // as a value-taking option, so `join -a f1 f2` swallowed the first
        // file name and tried to parse `f1` as a number, landing on
        // "field number less than 1" — a demonstrated misreading of the legacy
        // form as the new one.
        if b.len() == 2 && b[1] == b'a' {
            aflag = true;
            opts.unmatched1 = true;
            opts.unmatched2 = true;
            i += 1;
            continue;
        }
        match b[1] {
            b'1' => opts.field1 = field_no(value!('1'), "-1")?,
            b'2' => opts.field2 = field_no(value!('2'), "-2")?,
            b't' => {
                let v = value!('t');
                let bytes = v;
                if bytes.len() != 1 {
                    return Err(JoinError::BadTabChar);
                }
                // C does two things at once in `join.c:200-201`: `spans = 0`
                // and `tabchar = optarg`. Changing the delimiter set therefore
                // also turns off "skip empty spans" — they are not two
                // independent switches.
                opts.tabchars = bytes.to_vec();
                opts.spans = false;
            }
            b'a' => {
                aflag = true;
                file_no(
                    value!('a'),
                    "-a",
                    &mut opts.unmatched1,
                    &mut opts.unmatched2,
                )?;
            }
            b'v' => {
                vflag = true;
                opts.joinout = false;
                file_no(
                    value!('v'),
                    "-v",
                    &mut opts.unmatched1,
                    &mut opts.unmatched2,
                )?;
            }
            b'e' => opts.empty = Some(value!('e').to_vec()),
            b'j' => {
                let n = field_no(value!('j'), "-j")?;
                opts.field1 = n;
                opts.field2 = n;
            }
            b'o' => {
                // C's `-o arg arg` rewrite happens only when the token is
                // **exactly a bare `-o`** (`join.c:601-602`'s
                // `if (ap[2] != '\0') break;`), so in the attached form
                // `-o1.1` the following `2.1` is still an operand. That
                // condition must be recorded before taking the value.
                let bare_o = a.len() == 2;
                // Both `fieldarg` arms are `errx`, but with different
                // sentences, so they cannot be collapsed into one error type.
                let push_list =
                    |list: &'a [u8], acc: &mut Vec<OField>| -> Result<(), JoinError<'a>> {
                        match parse_olist(list) {
                            Ok(fields) => {
                                // C's `fieldarg` **appends at the tail of
                                // olist** (`join.c:540-542`'s
                                // `olist[olistcnt] = ...; ++olistcnt`), so a
                                // repeated `-o` accumulates rather than
                                // overwrites.
                                acc.extend(fields);
                                Ok(())
                            }
                            Err(OListError::Malformed) => Err(JoinError::MalformedOutputField),
                            Err(OListError::OneBased) => Err(JoinError::FieldNumbersAre1Based),
                        }
                    };
                push_list(value!('o'), &mut opts.output_fields)?;
                // legacy `-o arg arg`: in `obsolete()` C rewrites each
                // immediately following `[12].<digits>` item into `-o<item>`
                // one by one (`join.c:596-617`), so they are **no longer
                // operands** but each contributes an `-o` list; the first item
                // that is not a field spec stops the rewrite (the rest still
                // count as operands).
                while bare_o && i + 1 < args.len() && is_field_spec(args[i + 1]) {
                    i += 1;
                    push_list(args[i], &mut opts.output_fields)?;
                }
            }
            // A letter outside the set: C likewise has a libc line first
            // (`getopt.c:101-104`) and then falls into `join.c:226-228`'s
            // `case '?'`/`default: usage()`.
            _ => {
                return Err(JoinError::Getopt(GetoptReport::UnknownOption(char::from(b[1]))));
            }
        }
        i += 1;
    }
    if aflag && vflag {
        return Err(JoinError::AAndVMutuallyExclusive);
    }
    if files.len() != 2 {
        return Err(JoinError::Usage);
    }
    Ok(JoinArgs { opts, files })
}

/// Cut one line into fields, shaped like C's `strsep(&bp, tabchar)`
/// (`join.c:378`): **one cut per delimiter byte**, so `a,,c` is three fields
/// (the middle one empty) and a trailing delimiter leaves a trailing empty
/// field (`strsep` hands out one extra time at the end of the string). `spans`
/// decides whether those empty fields are dropped — the default `" \t"` uses it
/// to treat delimiter runs as one, while an explicit `-t` (C sets `spans` to 0
/// at `join.c:200`) must **keep** the empty fields, or field numbers, the match
/// set, `-o` and `-v` would all shift together (golden samples in
/// `join_face.rs`).
fn split_fields<'l>(line: &'l [u8], tabchars: &[u8], spans: bool) -> Vec<&'l [u8]> {
    let mut fields = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i <= line.len() {
        if i == line.len() || tabchars.contains(&line[i]) {
            let tok = &line[start..i];
            if !(spans && tok.is_empty()) {
                fields.push(tok);
            }
            start = i + 1;
        }
        i += 1;
    }
    fields
}

/// Compare the two sides' join fields, shaped like C's `cmp`
/// (`minix3/usr.bin/join/join.c:405-414`).
///
/// It has **three** answers rather than two: "the field number exceeds this
/// line's field count" and "the field exists but is itself empty" are not the
/// same thing (`join.c` checks `fieldcnt <= fieldno` first and only `strcmp`s
/// when neither side is out of range). So: both sides out of range counts as
/// equal (C's `return 0`); when only one side is out of range, **that side
/// counts as greater** (C returns 1 / -1 respectively) rather than comparing a
/// missing field as an empty string — otherwise a line lacking that column
/// would be judged the same key as a line whose column is an empty string, and
/// not only would the join output invent pairings that do not exist, but
/// grouping within one file by `cmp != 0` (`:398`) would drift as well.
fn cmp_keys(a: &[&[u8]], fa: usize, b: &[&[u8]], fb: usize) -> core::cmp::Ordering {
    match (a.get(fa), b.get(fb)) {
        (None, None) => core::cmp::Ordering::Equal,
        (None, Some(_)) => core::cmp::Ordering::Greater,
        (Some(_), None) => core::cmp::Ordering::Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

/// Join `f1` and `f2` (byte-line slices, each already newline-stripped) into
/// output lines. Inputs are assumed sorted by their join field (as `join(1)`
/// requires); equal keys cross-multiply, the lesser key advances alone.
pub fn join_lines(f1: &[&[u8]], f2: &[&[u8]], opts: &JoinOptions) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < f1.len() && j < f2.len() {
        let g1 = group_end(f1, i, opts.field1, opts);
        let g2 = group_end(f2, j, opts.field2, opts);
        match cmp_keys(
            &opts.fields_of(f1[i]),
            opts.field1,
            &opts.fields_of(f2[j]),
            opts.field2,
        ) {
            core::cmp::Ordering::Equal => {
                // C's `if (joinout) joinlines(F1, F2)` (`join.c:259`): with `-v`
                // clearing `joinout`, matched groups still advance but emit
                // nothing.
                if opts.joinout {
                    for la in f1[i..g1].iter().copied() {
                        let fa = opts.fields_of(la);
                        for lb in f2[j..g2].iter().copied() {
                            let fb = opts.fields_of(lb);
                            out.push(twoline(&fa, &fb, opts));
                        }
                    }
                }
                i = g1;
                j = g2;
            }
            core::cmp::Ordering::Less => {
                if opts.unmatched1 {
                    for la in f1[i..g1].iter().copied() {
                        out.push(onlyline(&opts.fields_of(la), true, opts));
                    }
                }
                i = g1;
            }
            core::cmp::Ordering::Greater => {
                if opts.unmatched2 {
                    for lb in f2[j..g2].iter().copied() {
                        out.push(onlyline(&opts.fields_of(lb), false, opts));
                    }
                }
                j = g2;
            }
        }
    }
    // A file may have trailing groups past the other's end.
    if opts.unmatched1 {
        while i < f1.len() {
            let g = group_end(f1, i, opts.field1, opts);
            for la in f1[i..g].iter().copied() {
                out.push(onlyline(&opts.fields_of(la), true, opts));
            }
            i = g;
        }
    }
    if opts.unmatched2 {
        while j < f2.len() {
            let g = group_end(f2, j, opts.field2, opts);
            for lb in f2[j..g].iter().copied() {
                out.push(onlyline(&opts.fields_of(lb), false, opts));
            }
            j = g;
        }
    }
    out
}

/// Index one past the run of lines from `start` sharing the key of `start`.
/// `field` is **this side's own** join field number (C uses `F->joinf`, which
/// `slurp` takes from the caller's `INPUT*`, so the two sides may differ —
/// `-1 2 -2 1` does). C's `slurp` compares a new line against the **previous**
/// line (`join.c:398`'s `cmp(lp, F->joinf, lp - 1, F->joinf)`); this function
/// compares against the **group's first line**. The two are equivalent at the
/// first mismatch position: every compared line equals the group head, so
/// `key(k-1) == key(start)`.
fn group_end(lines: &[&[u8]], start: usize, field: usize, opts: &JoinOptions) -> usize {
    let base = opts.fields_of(lines[start]);
    let mut end = start + 1;
    while end < lines.len()
        && cmp_keys(&base, field, &opts.fields_of(lines[end]), field) == core::cmp::Ordering::Equal
    {
        end += 1;
    }
    end
}

/// Append a field to the output line being assembled. **The separator slot is
/// claimed unconditionally first**, and only afterwards is content decided:
/// C's `outfield()` starts with `if (needsep++) printf("%c", *tabchar);`
/// (`minix3/usr.bin/join/join.c:496-497`), and only a "field exists but is
/// itself empty" returns (`:503-504`), leaving the slot in place. So under
/// `-t,` the three fields `a`, ``, `c` are `a,,c` in C; the earlier
/// implementation tested for empty first and returned outright, wiping the
/// slot with it. The reference measurement (`join_ref`, golden sample in
/// `join_face.rs`'s `output_slots_match_the_bsd_reference_byte_for_byte`) pins
/// this shape.
fn push_tok(out: &mut Vec<u8>, sep: u8, first: &mut bool, tok: &[u8]) {
    if !*first {
        out.push(sep);
    }
    *first = false;
    out.extend_from_slice(tok);
}

/// Pick the token for one `join -o` entry given the (possibly absent) field
/// vectors of each file; a missing field or the absent side yields the `-e`
/// replacement (empty string when none was given).
fn olist_token(
    entry: &OField,
    a: Option<&[&[u8]]>,
    b: Option<&[&[u8]]>,
    opts: &JoinOptions,
) -> Vec<u8> {
    let fallback = opts.empty.clone().unwrap_or_default();
    let pick = |side: Option<&[&[u8]]>, idx: usize| -> Vec<u8> {
        match side {
            Some(fields) => fields
                .get(idx)
                .map(|f| f.to_vec())
                .unwrap_or_else(|| fallback.clone()),
            None => fallback.clone(),
        }
    };
    match entry.which {
        Which::One => pick(a, entry.idx),
        Which::Two => pick(b, entry.idx),
    }
}

/// The pair output line: the `-o` field list when set (each entry resolved
/// against both files), else the default join field + file 1 rest + file 2 rest.
fn twoline(a: &[&[u8]], b: &[&[u8]], opts: &JoinOptions) -> Vec<u8> {
    let mut line = Vec::new();
    let mut first = true;
    if !opts.output_fields.is_empty() {
        for entry in &opts.output_fields {
            push_tok(
                &mut line,
                opts.sep(),
                &mut first,
                &olist_token(entry, Some(a), Some(b), opts),
            );
        }
        return line;
    }
    // An absent join field still claims a slot: C calls
    // `outfield(lp1, F1->joinf)`, and `outfield` takes the `-e` branch
    // (`:499-501`) when "the field number exceeds this line's field count";
    // without `-e` that is an empty slot.
    let owned_join: Vec<u8>;
    let join_tok: &[u8] = match a.get(opts.field1) {
        Some(f) => f,
        None => {
            owned_join = opts.empty.clone().unwrap_or_default();
            &owned_join
        }
    };
    push_tok(&mut line, opts.sep(), &mut first, join_tok);
    for (idx, f) in a.iter().enumerate() {
        if idx != opts.field1 {
            push_tok(&mut line, opts.sep(), &mut first, f);
        }
    }
    for (idx, f) in b.iter().enumerate() {
        if idx != opts.field2 {
            push_tok(&mut line, opts.sep(), &mut first, f);
        }
    }
    line
}

/// An unpaired line: with `-o` the other file's fields yield the `-e`
/// replacement; otherwise every one of the line's own fields goes through
/// C's `outfield` (`join.c:493-509`) — **an empty field still claims a
/// separator slot**, it just contributes no content.
fn onlyline(fields: &[&[u8]], is_file1: bool, opts: &JoinOptions) -> Vec<u8> {
    let mut line = Vec::new();
    let mut first = true;
    if !opts.output_fields.is_empty() {
        let a: Option<&[&[u8]]> = if is_file1 { Some(fields) } else { None };
        let b: Option<&[&[u8]]> = if is_file1 { None } else { Some(fields) };
        for entry in &opts.output_fields {
            push_tok(
                &mut line,
                opts.sep(),
                &mut first,
                &olist_token(entry, a, b, opts),
            );
        }
        return line;
    }
    for f in fields {
        push_tok(&mut line, opts.sep(), &mut first, f);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<Vec<u8>> {
        s.lines().map(|l| l.as_bytes().to_vec()).collect()
    }

    fn refs(v: &[Vec<u8>]) -> Vec<&[u8]> {
        v.iter().map(|l| l.as_slice()).collect()
    }

    fn join(f1: &str, f2: &str, opts: &JoinOptions) -> String {
        let (a, b) = (lines(f1), lines(f2));
        let out = join_lines(&refs(&a), &refs(&b), opts);
        out.iter()
            .map(|l| core::str::from_utf8(l).unwrap().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_default_inner_join() {
        let f1 = "1 a\n2 b\n3 c";
        let f2 = "1 x\n2 y\n4 z";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a x\n2 b y");
    }

    #[test]
    fn test_unmatched_a_flags() {
        let f1 = "1 a\n3 c";
        let f2 = "1 x\n4 z";
        let o = JoinOptions {
            unmatched1: true,
            ..Default::default()
        };
        // -a 1: file 1's unpaired key 3 survives; file 2's key 4 dropped.
        assert_eq!(join(f1, f2, &o), "1 a x\n3 c");
        let o2 = JoinOptions {
            unmatched1: true,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join(f1, f2, &o2), "1 a x\n3 c\n4 z");
    }

    #[test]
    fn test_equal_keys_cross_multiply() {
        // Multiple lines share a key on both sides -> full cross product.
        let f1 = "1 a\n1 b";
        let f2 = "1 x\n1 y";
        assert_eq!(
            join(f1, f2, &JoinOptions::default()),
            "1 a x\n1 a y\n1 b x\n1 b y"
        );
    }

    #[test]
    fn test_custom_join_fields() {
        // -1 2 -2 1: join file1's field 2 against file2's field 1.
        let f1 = "x A\ny B";
        let f2 = "A 1\nB 2";
        let o = JoinOptions {
            field1: 1,
            field2: 0,
            ..Default::default()
        };
        // key is file1's field2; output = key then file1-rest then file2-rest.
        assert_eq!(join(f1, f2, &o), "A x 1\nB y 2");
    }

    #[test]
    fn test_dash_t_delimiter() {
        let f1 = "1:a\n2:b";
        let f2 = "1:x\n3:z";
        let o = JoinOptions {
            tabchars: b":".to_vec(),
            // `-t` also clears `spans` in C (`join.c:200`), so this shape is
            // "single-byte set + keep empty fields", not just a different
            // delimiter.
            spans: false,
            ..Default::default()
        };
        // split and join both use ':'; unmatched file lines dropped (no -a).
        assert_eq!(join(f1, f2, &o), "1:a:x");
    }

    /// The default delimiter set (`" \t"` + `spans`): runs of delimiters count
    /// as one, and empty spans do not become fields. Delimiters at the start or
    /// end of the line also leave no empty field behind (C's `strsep` would
    /// hand one out, but `:379` drops it).
    #[test]
    fn split_default_set_spans_runs_and_drops_empty_fields() {
        let f = split_fields(b"a  b\tc ", b" \t", true);
        assert_eq!(f, vec![&b"a"[..], &b"b"[..], &b"c"[..]]);
        assert_eq!(split_fields(b"  a", b" \t", true), vec![&b"a"[..]]);
        assert_eq!(split_fields(b"", b" \t", true), Vec::<&[u8]>::new());
    }

    /// An explicit `-t` (`spans = 0`): every cut makes a field, so `a,,c` is
    /// three fields and a trailing delimiter leaves one more empty field (the
    /// golden sample comes from the reference `join_ref`).
    #[test]
    fn split_with_explicit_dash_t_keeps_every_empty_field() {
        assert_eq!(
            split_fields(b"a,,c", b",", false),
            vec![&b"a"[..], &b""[..], &b"c"[..]]
        );
        assert_eq!(split_fields(b"a,", b",", false), vec![&b"a"[..], &b""[..]]);
        assert_eq!(split_fields(b",a", b",", false), vec![&b""[..], &b"a"[..]]);
        assert_eq!(split_fields(b"", b",", false), vec![&b""[..]]);
    }

    /// `-t ' '` makes only the space a delimiter (C's `tabchar` is that one
    /// byte), so a tab is still content; the earlier implementation treated
    /// "delimiter is a space" as "the default set" and cut on tabs too. The
    /// reference measured `join -t ' ' -j 2` on `a\tb` as ` a\tb a\tb` (the
    /// whole string is one field).
    #[test]
    fn dash_t_space_does_not_make_tab_a_delimiter() {
        assert_eq!(split_fields(b"a\tb", b" ", false), vec![&b"a\tb"[..]]);
        assert_eq!(
            split_fields(b"a  b", b" ", false),
            vec![&b"a"[..], &b""[..], &b"b"[..]]
        );
    }

    /// An unpaired line (C `outoneline`) sends **every** field through
    /// `outfield`: an empty field holds its slot without printing, so
    /// `-t, -a1` gives `a,` rather than `a` for `a,` (reference measurement).
    /// Both lines must have unequal keys, otherwise the paired path runs
    /// instead of `outoneline`.
    #[test]
    fn unpaired_line_gives_an_empty_field_its_separator_slot() {
        let o = JoinOptions {
            tabchars: b",".to_vec(),
            spans: false,
            unmatched1: true,
            ..Default::default()
        };
        assert_eq!(join("a,", "b", &o), "a,");
        assert_eq!(join("a,,c", "x", &o), "a,,c");
    }

    /// C's `cmp` has three answers: out of range and empty string are not the
    /// same thing. All four shapes are pinned one by one against
    /// `join.c:405-414`: both sides out of range is equal, only the first side
    /// out of range makes the first side greater, only the second side out of
    /// range makes the second side greater, and only when both have the field
    /// are the bytes compared.
    #[test]
    fn cmp_keys_answers_three_cases_not_two() {
        let two: &[&[u8]] = &[&b"a"[..], &b""[..]];
        let one: &[&[u8]] = &[&b"a"[..]];
        let nine: &[&[u8]] = &[&b"x"[..], &b"9"[..]];
        assert_eq!(
            cmp_keys(one, 1, one, 1),
            core::cmp::Ordering::Equal,
            "两边都没这一列"
        );
        assert_eq!(
            cmp_keys(one, 1, two, 1),
            core::cmp::Ordering::Greater,
            "越界那一侧算大，不是把它当空串去比"
        );
        assert_eq!(cmp_keys(two, 1, one, 1), core::cmp::Ordering::Less);
        assert_eq!(
            cmp_keys(two, 1, nine, 1),
            core::cmp::Ordering::Less,
            "两边都有字段才落到 strcmp：\"\" < \"9\""
        );
    }

    /// Which side is out of range decides which side yields, so the two
    /// directions have different golden samples: for `solo`/`x 9` with
    /// `-1 2 -2 2 -a1 -a2`, C prints `x 9` first and then `solo`
    /// (`join.c:263-273`, the `cval < 0` / `> 0` arms). Both directions are the
    /// same fact, so they share one test.
    #[test]
    fn join_lines_gives_the_lead_to_the_side_that_has_the_field() {
        let o = JoinOptions {
            field1: 1,
            field2: 1,
            unmatched1: true,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join("solo", "x 9", &o), "x 9\nsolo");
        assert_eq!(join("x 9", "solo", &o), "x 9\nsolo");
    }

    /// Grouping inside one file also goes through `cmp` (C `slurp`,
    /// `join.c:398`): a line "whose column is an empty string" and a line
    /// "that has no such column at all" do not belong to the same group, so it
    /// cannot cross-multiply with same-key lines on the other side. Golden
    /// sample: `-t, -1 2 -2 2 -a1 -a2` on a three-line file and a one-line file
    /// → `,a,x,a,x`, `b`, `c,,y`.
    #[test]
    fn grouping_breaks_where_the_join_field_is_absent() {
        let o = JoinOptions {
            tabchars: b",".to_vec(),
            spans: false,
            field1: 1,
            field2: 1,
            unmatched1: true,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(
            join("a,,x\nb\nc,,y", "a,,x", &o),
            ",a,x,a,x\nb\nc,,y",
            "旧写法把三行归成一组，会多拼出两行根本不存在的配对"
        );
    }

    #[test]
    fn test_multifield_lines() {
        let f1 = "1 a b\n2 c d";
        let f2 = "1 x\n2 y";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a b x\n2 c d y");
    }

    #[test]
    fn test_parse_olist_forms() {
        assert_eq!(
            parse_olist(b"1.2,2.1").unwrap(),
            vec![
                OField {
                    which: Which::One,
                    idx: 1
                },
                OField {
                    which: Which::Two,
                    idx: 0
                },
            ]
        );
        assert!(parse_olist(b"3.1").is_err()); // no file 3
        assert!(parse_olist(b"1.0").is_err()); // fields are 1-based
        assert!(parse_olist(b"5").is_err()); // no dot, not `0`
    }

    /// This tree's ground truth does not accept `-o 0` (the "the join field
    /// itself" arm is a BSD/GNU convention): all four shapes fall on
    /// `Malformed` (C's `fieldarg` only recognizes a leading `1`/`2`,
    /// `join.c:526-527`). An earlier test accepted `0` as a legal entry, which
    /// endorsed invented behaviour instead of following the ground truth.
    #[test]
    fn parse_olist_refuses_the_zero_join_field_entry() {
        assert_eq!(parse_olist(b"0"), Err(OListError::Malformed));
        assert_eq!(parse_olist(b"0,1.1"), Err(OListError::Malformed));
        assert_eq!(parse_olist(b"1.1,0"), Err(OListError::Malformed));
        assert_eq!(parse_olist(b"0.1"), Err(OListError::Malformed));
        // Columns with a space behave the same: C splits entries with
        // `strsep(", \t")`, so a lone `0` entry is rejected.
        assert_eq!(parse_olist(b"1.1 0"), Err(OListError::Malformed));
    }

    #[test]
    fn test_olist_selects_numbered_fields() {
        // This tree has no `0` arm, so `1.1` expresses the same thing (the
        // default join field is file one's field 1).
        let o = JoinOptions {
            output_fields: parse_olist(b"1.1,2.2,1.2").unwrap(),
            ..Default::default()
        };
        assert_eq!(join("1 a b\n2 c d", "1 x\n2 y", &o), "1 x a\n2 y c");
    }

    #[test]
    fn test_olist_empty_replacement_on_unmatched() {
        // 1.1 replaces the `0` of the earlier form (this tree does not accept
        // that letter): a file-1 line standing alone has no file-2 field.
        let mut o = JoinOptions {
            output_fields: parse_olist(b"1.1,1.2,2.2").unwrap(),
            empty: Some(b"-".to_vec()),
            unmatched1: true,
            ..Default::default()
        };
        o.unmatched2 = false;
        // key 3 pairs with nothing -> file-2 field replaced by '-'.
        assert_eq!(join("1 a\n3 c", "1 x", &o), "1 a x\n3 c -");
    }

    #[test]
    fn test_v1_emits_only_file1_unpaired() {
        // `-v 1` (joinout off, unpair file 1): the key 1 match prints nothing,
        // key 3 alone from file 1 prints, file 2's key 4 stays silent.
        let o = JoinOptions {
            joinout: false,
            unmatched1: true,
            ..Default::default()
        };
        assert_eq!(join("1 a\n3 c", "1 x\n4 z", &o), "3 c");
    }

    #[test]
    fn test_v2_emits_only_file2_unpaired() {
        let o = JoinOptions {
            joinout: false,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join("1 a\n3 c", "1 x\n4 z", &o), "4 z");
    }

    #[test]
    fn test_joinout_off_without_unpair_emits_nothing() {
        // Defensive: `-v` with no matching unpair file yields empty output —
        // matched groups are silent and no side emits singletons.
        let o = JoinOptions {
            joinout: false,
            ..Default::default()
        };
        assert_eq!(join("1 a\n2 b", "1 x\n3 z", &o), "");
    }

    /// Fixture helper: hand back the textual argv word list in byte form. The
    /// literals in these tests are all ASCII, so the two shapes are equivalent;
    /// this layer only spells out the argument type `parse_join_args` takes and
    /// carries no criterion of its own.
    /// Returning `Vec<&'static [u8]>` (not a temporary) is required: `JoinArgs`
    /// borrows the bytes in the word list, a temporary `Vec` does not outlive a
    /// statement, and using `a` after `let a = parse(&argv(&[…]))` would
    /// dangle.
    fn argv(words: &'static [&'static str]) -> Vec<&'static [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    fn parse<'a>(v: &'a [&'a [u8]]) -> JoinArgs<'a> {
        parse_join_args(v).expect("parse ok")
    }

    #[test]
    fn parse_attached_option_values_accepted() {
        // Key regression: the BSD getopt attached-value forms were not accepted
        // by the old front door.
        let w_a = argv(&["-t,", "f1", "f2"]);
        let a = parse(&w_a);
        assert_eq!(a.opts.sep(), b',');
        assert_eq!(a.opts.tabchars, b",".to_vec());
        assert!(
            !a.opts.spans,
            "`-t` 在 C 里同时清掉 `spans`（`join.c:200`）"
        );
        assert_eq!(a.files, argv(&["f1", "f2"]));
        let w_b = argv(&["-a1", "-o1.1,1.2", "f1", "f2"]);
        let b = parse(&w_b);
        assert!(b.opts.unmatched1 && !b.opts.unmatched2);
        assert_eq!(b.opts.output_fields.len(), 2);
        // Separated values still work (kept compatible).
        let w_c = argv(&["-t", "|", "-1", "2", "f1", "f2"]);
        let c = parse(&w_c);
        assert_eq!(c.opts.sep(), b'|');
        assert_eq!(c.opts.field1, 1);
    }

    #[test]
    fn parse_terminator_and_operands() {
        // Everything after `--` is an operand; a bare `-` is an operand
        // (stdin).
        let w_a = argv(&["-t,", "--", "-a", "f2"]);
        let a = parse(&w_a);
        assert_eq!(a.files, argv(&["-a", "f2"])); // after `--`, `-a` is the literal file name
        let w_b = argv(&["-", "f2"]);
        let b = parse(&w_b);
        assert_eq!(b.files, argv(&["-", "f2"]));
    }

    /// The three legacy rewrites of `join.c:obsolete()` (run before `getopt`,
    /// stopping at `--`). The reading is counter-intuitive, so every shape is
    /// pinned separately:
    /// (1) a bare `-a` is rewritten to `case '\01'` (`join.c:135-138`) —
    ///     `aflag` plus **two** unpair flags;
    /// (2) because the rewrite is unconditional, the `3` in `join -a 3 f1 f2`
    ///     is **no longer `-a`'s value** but an operand, so the three operands
    ///     hit `if (argc != 2) usage()` (`join.c:236-237`) — the reference
    ///     produces no output for that shape and dies in `usage()`, the same
    ///     shape as here;
    /// (3) `-j1`/`-j2` become `-1`/`-2`, with the value taken from the next
    ///     word (the reference reports `-1 option field number less than 1` for
    ///     `join -j1 f1 f2`);
    /// (4) a malformed `-j` shape makes `illegal option -- <whole token>` come
    ///     out before the option loop even runs.
    #[test]
    fn obsolete_legacy_forms_rewrite_before_getopt() {
        // (1) Bare `-a`: both files emit unpaired lines, and it swallows no
        // value.
        let w_bare = argv(&["-a", "f1", "f2"]);
        let bare = parse_join_args(&w_bare).unwrap();
        assert!(bare.opts.unmatched1 && bare.opts.unmatched2);
        assert_eq!(bare.files, argv(&["f1", "f2"]));
        // (2) A separated value becomes an operand → three operands → the bare
        // usage arm.
        assert_eq!(
            parse_join_args(&argv(&["-a", "3", "f1", "f2"])).err(),
            Some(JoinError::Usage)
        );
        // (3) `-j1` becomes `-1`: the value comes from the next word, and field
        // one is what that word converts to (it converts nothing here → the
        // `< 1` sentence).
        assert_eq!(
            parse_join_args(&argv(&["-j1", "f1", "f2"])).err(),
            Some(JoinError::FieldLessThan1("-1"))
        );
        let w_conv = argv(&["-j2", "2", "f1", "f2"]);
        let conv = parse_join_args(&w_conv).unwrap();
        assert_eq!(conv.opts.field2, 1, "`-j2 2` 等价于 `-2 2`");
        assert_eq!(conv.opts.field1, 0, "开始列没被 `-j2` 动过");
        // (4) A malformed `-j` shape: the whole token enters the sentence, and
        // it outranks other errors.
        assert_eq!(
            parse_join_args(&argv(&["-j3", "f1", "f2"])).err(),
            Some(JoinError::IllegalOptionToken(b"-j3"))
        );
        assert_eq!(
            parse_join_args(&argv(&["-j12", "f1", "f2"])).err(),
            Some(JoinError::IllegalOptionToken(b"-j12"))
        );
        assert_eq!(
            parse_join_args(&argv(&["-j1", "-j3", "f1", "f2"])).err(),
            Some(JoinError::IllegalOptionToken(b"-j3")),
            "pre-pass 先扫完整条 argv，所以后面的畸形形赢过前面的改写"
        );
        // After `--` the scan stops: that malformed shape falls into the
        // operands and the bare usage is reported.
        assert_eq!(
            parse_join_args(&argv(&["--", "-j3", "f1", "f2"])).err(),
            Some(JoinError::Usage)
        );
    }

    /// legacy `-o arg arg`: in `obsolete()` C rewrites each `[12].<digits>`
    /// item following the `-o` value into `-o<item>` one by one
    /// (`join.c:596-617`), and `fieldarg` always **appends**
    /// (`join.c:540-542`), so repeated `-o` accumulates rather than overwrites.
    /// The first item that is not a field spec stops the rewrite; it and the
    /// items after it remain operands.
    #[test]
    fn legacy_o_lists_accumulate_instead_of_replacing() {
        let w_one = argv(&["-o", "1.1", "f1", "f2"]);
        let one = parse_join_args(&w_one).unwrap();
        assert_eq!(one.opts.output_fields.len(), 1);
        let w_legacy = argv(&["-o", "1.1", "2.1", "f1", "f2"]);
        let legacy = parse_join_args(&w_legacy).unwrap();
        assert_eq!(
            legacy.opts.output_fields.len(),
            2,
            "`-o 1.1 2.1` 是 legacy 的两条列表，`2.1` 不是操作数"
        );
        assert_eq!(legacy.files, argv(&["f1", "f2"]));
        // The attached form `-o1.1` does not trigger the rewrite
        // (`join.c:601-602`'s `ap[2] != '\0'` breaks out directly), so `2.1`
        // stays an operand — two operands fit exactly; three would hit the bare
        // usage.
        let w_inline = argv(&["-o1.1", "2.1", "f1"]);
        let inline = parse_join_args(&w_inline).unwrap();
        assert_eq!(inline.opts.output_fields.len(), 1);
        assert_eq!(inline.files, argv(&["2.1", "f1"]));
        // Two attached `-o` forms also accumulate: this is C's `olistcnt++`,
        // not an overwrite.
        let w_twice = argv(&["-o1.1", "-o2.1", "f1", "f2"]);
        let twice = parse_join_args(&w_twice).unwrap();
        assert_eq!(twice.opts.output_fields.len(), 2);
    }

    #[test]
    fn parse_validations_and_mutual_exclusion() {
        // Not two operands, a letter outside the set, -a&-v exclusivity, an
        // illegal number, and a non-single-byte tab.
        // A count other than two takes the bare usage; a letter outside the set
        // takes "the libc line + usage" — the two are different types.
        assert_eq!(parse_join_args(&argv(&["only1"])).err(), Some(JoinError::Usage));
        assert_eq!(
            parse_join_args(&argv(&["-z", "f1", "f2"])).err(),
            Some(JoinError::Getopt(GetoptReport::UnknownOption('z')))
        );
        // A value-taking option without its value carries the letter too
        // (`-t` reaches the end of the argument list with no value).
        assert_eq!(
            parse_join_args(&argv(&["f1", "f2", "-t"])).err(),
            Some(JoinError::Getopt(GetoptReport::MissingValue('t')))
        );
        assert_eq!(
            parse_join_args(&argv(&["-a1", "-v2", "f1", "f2"])).err(),
            Some(JoinError::AAndVMutuallyExclusive)
        );
        assert_eq!(
            parse_join_args(&argv(&["-1", "0", "f1", "f2"])).err(),
            Some(JoinError::FieldLessThan1("-1"))
        );
        assert_eq!(
            parse_join_args(&argv(&["-1", "2x", "f1", "f2"])).err(),
            Some(JoinError::IllegalFieldNumber(b"2x"))
        );
        assert_eq!(
            parse_join_args(&argv(&["-t", "ab", "f1", "f2"])).err(),
            Some(JoinError::BadTabChar)
        );
        assert_eq!(
            parse_join_args(&argv(&["-a3", "f1", "f2"])).err(),
            Some(JoinError::FileNumberNot12("-a"))
        );
        assert_eq!(
            parse_join_args(&argv(&["-j", "f1", "f2"])).err(),
            // C's `strtol("f1")` converts no digit → value 0 → **the `< 1`
            // guard runs first**, so it lands on "less than 1" rather than
            // "illegal field number" (`join.c:184-189`; the reference reads the
            // same). The earlier implementation went to the wrong arm here
            // because it branched on "is it all digits" first.
            Some(JoinError::FieldLessThan1("-j"))
        );
        // A value that converts no digit always lands on the `< 1` arm first
        // (same shape as above; applies to all three arms).
        assert_eq!(
            parse_join_args(&argv(&["-1", "x", "f1", "f2"])).err(),
            Some(JoinError::FieldLessThan1("-1"))
        );
        // The two file-number arms check in the order "switch first, then the
        // trailing character": the 1 in `1x` is already accepted by the switch,
        // so it takes the illegal sentence rather than not-1-or-2
        // (`:165-179`).
        assert_eq!(
            parse_join_args(&argv(&["-a1x", "f1", "f2"])).err(),
            Some(JoinError::IllegalFileNumber(b"1x")),
            "附值形才走 file_no：分开的 `-a 1x` 会被 obsolete() 改写成裸 `-a`（见下行测）"
        );
        assert_eq!(
            parse_join_args(&argv(&["-v", "x", "f1", "f2"])).err(),
            Some(JoinError::FileNumberNot12("-v"))
        );
        // The two `-o` `errx` arms are two different error types (different
        // sentences), and neither is followed by usage.
        assert_eq!(
            parse_join_args(&argv(&["-o", "1.0", "f1", "f2"])).err(),
            Some(JoinError::FieldNumbersAre1Based)
        );
        assert_eq!(
            parse_join_args(&argv(&["-o", "3.1", "f1", "f2"])).err(),
            Some(JoinError::MalformedOutputField)
        );
        // `-o 0` and its attached form `-o0` both land on the malformed
        // sentence (C does not accept the "the join field itself" arm).
        assert_eq!(
            parse_join_args(&argv(&["-o", "0", "f1", "f2"])).err(),
            Some(JoinError::MalformedOutputField)
        );
        assert_eq!(
            parse_join_args(&argv(&["-o0", "f1", "f2"])).err(),
            Some(JoinError::MalformedOutputField)
        );
    }

    /// How the **letter position is taken** needs a pin in the library too
    /// (this gate came out of negative control J4: after replacing
    /// `char::from(b[1])` with "take the whole cluster by character", the
    /// cross-shell table and this branch's end-to-end gate both went red while
    /// **all 622 library tests stayed green** — evidence that this branch's
    /// letter leg had never been pinned in the library; adding a gate for `cut`
    /// was the second realization of that criterion).
    /// Every letter in `join`'s optstring `"\01a:e:j:1:2:o:t:v:"` except `\01`
    /// takes a value, so there is no "legal letter followed by multibyte" shape
    /// to pin; the counter-shape is `-1<é>`: `é` is swallowed as `-1`'s value,
    /// and the answer is "no digit converts → value 0 → the `< 1` guard runs
    /// first", unrelated to the letter position (`join.c:140-143`).
    #[test]
    fn the_ring_names_the_first_byte_of_a_multibyte_letter() {
        let two: &[&[u8]] = &[b"-\xc3\xa9", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(two).err(),
            Some(JoinError::Getopt(GetoptReport::UnknownOption(char::from(
                0xc3_u8
            )))),
            "`-é` 报的字母是短横后的第一字节 `c3`，不是整个码位"
        );
        let lone: &[&[u8]] = &[b"-\xff", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(lone).err(),
            Some(JoinError::Getopt(GetoptReport::UnknownOption(char::from(
                0xff_u8
            )))),
            "一枚非法字节本身就是一个字母位，不借任何解码通道"
        );
        let stop: &[&[u8]] = &[b"-Z\xc3\xa9", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(stop).err(),
            Some(JoinError::Getopt(GetoptReport::UnknownOption('Z'))),
            "环停在第一个坏字母上，后面的 `é` 根本不该被读到（`join.c:226-228`）"
        );
        let value: &[&[u8]] = &[b"-1\xc3\xa9", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(value).err(),
            Some(JoinError::FieldLessThan1("-1")),
            "带值字母后面那串是值不是字母：`é` 转不出数字，落 `< 1` 那一句"
        );
    }

    #[test]
    fn strtol_decimal_mirrors_the_no_digit_case() {
        // The four shapes of `strtol`: leading whitespace and the sign count as
        // consumed; with no digits the value is 0 and no error is raised.
        assert_eq!(strtol_decimal(b"12"), (12, 2));
        assert_eq!(strtol_decimal(b" 12"), (12, 3));
        assert_eq!(strtol_decimal(b"+12"), (12, 3));
        assert_eq!(strtol_decimal(b"-3"), (-3, 2));
        assert_eq!(strtol_decimal(b"x"), (0, 0));
        assert_eq!(strtol_decimal(b"1x"), (1, 1));
        assert_eq!(strtol_decimal(b""), (0, 0));
    }

    /// The argument seam on `join` is not only about messages: **the value is
    /// the behaviour**. `join.c:200-204` uses the `-t` value as the delimiter
    /// and tests it with `strlen(optarg) != 1` — a bare `ff` that passes
    /// through `from_utf8_lossy` before argv reaches the library becomes three
    /// bytes, flipping this shape from **accepted** to **rejected** (the
    /// earlier build measured `join: illegal tab character specification` and
    /// exit 1, while the reference exits 0 for the same shape and cuts the
    /// fields). The `-e` value is even more direct: `join.c:198` stores it as
    /// the replacement string for empty fields, and `outfield` (`:497-508`)
    /// prints it verbatim into every output line. Only the remaining three arms
    /// are echoes: `illegal field number -- %s` (`:145`, `:156`, `:190`),
    /// `illegal file number -- %s` (`:176`, `:222`), and `obsolete()`'s
    /// `illegal option -- %s` (`:592`, carrying the whole token with its
    /// leading hyphen).
    #[test]
    fn a_raw_byte_is_the_delimiter_the_replacement_and_the_three_echoes() {
        // (1) The attached form of `-t`: a bare `ff` is a **legal** one-byte
        // delimiter, and the same statement clears `spans`.
        let attached: &[&[u8]] = &[b"-t\xff", b"f1", b"f2"];
        let cut = parse_join_args(attached).expect("`-t<ff>` 是一个字节，不该被拒");
        assert_eq!(cut.opts.tabchars, vec![0xff]);
        assert!(!cut.opts.spans, "`join.c:200` 取分隔符时把 `spans` 一起清成 0");
        assert_eq!(cut.opts.sep(), 0xff, "输出分隔符取的是 `*tabchar`，也是那一枚字节");
        // (2) The `-e` value is output content: the whole raw string travels,
        // not one byte folded.
        let repl: &[&[u8]] = &[b"-e\xffx", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(repl).expect("`-e` 不查形状").opts.empty,
            Some(vec![0xff, b'x']),
            "替代串交回的必须是 argv 原字节"
        );
        // (3)(4)(5) The three echo arms: `optarg` and the whole token are raw
        // bytes, and no length changes through decoding.
        let field: &[&[u8]] = &[b"-1", b"2\xff", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(field).err(),
            Some(JoinError::IllegalFieldNumber(&[b'2', 0xff][..])),
            "`2` 之后那枚裸字节转不进数、`*end` 非零，落 illegal 那句且带原字节"
        );
        let file: &[&[u8]] = &[b"-a1\xff", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(file).err(),
            Some(JoinError::IllegalFileNumber(&[b'1', 0xff][..])),
            "文件号那支的判序是先 switch 认下 1、再查尾字符"
        );
        let token: &[&[u8]] = &[b"-j3\xff", b"f1", b"f2"];
        assert_eq!(
            parse_join_args(token).err(),
            Some(JoinError::IllegalOptionToken(&[b'-', b'j', b'3', 0xff][..])),
            "畸形 `-j` 形在选项环之前就把整个 token 交回"
        );
        // (6) Operands are raw bytes as well (the open leg is invisible from the
        // library; that half belongs to the shell side and the two registered
        // `&str` entry points).
        let names: &[&[u8]] = &[b"a\xff-b", b"c"];
        assert_eq!(
            parse_join_args(names).expect("两个操数").files,
            vec![&[b'a', 0xff, b'-', b'b'][..], &b"c"[..]]
        );
    }
}
