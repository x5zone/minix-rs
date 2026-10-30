//! Random filtering: one over denominator selection.
//!
//! Ground truth: `minix3/games/random/random.c`. Seeding mixes the clock with
//! the process identifier (near line 107). The denominator is a `double` read by
//! `strtod` (near line 95), not an integer: a line is printed when
//! `(int)(denom * random() / RANDOM_MAX)` comes out zero (near line 126), and
//! exit status mode returns that same product truncated toward zero (near line
//! 111). Drawing stays with the execution layer; this module owns the
//! denominator grammar and the two predicates that read it.

/// C's whole `usage()` line (`minix3/games/random/random.c:143-147`).
///
/// The name is hardwired in the string: `fprintf(stderr, "usage: random [-er]
/// [denominator]\n")` uses the literal `random` and never goes through
/// `getprogname()`, so renaming the invocation leaves this line unchanged. C's
/// `errx`/`err` paths are the opposite, with the prefix following `argv[0]` — two
/// origins inside one command, which the thin shell has to copy separately.
pub const USAGE: &str = "usage: random [-er] [denominator]\n";

/// The sentence inside C's `errx(1, "denominator is not valid.")`
/// (`random.c:98-99`).
///
/// The trailing period comes from C, and the operand is **not** in this sentence —
/// `errx` has no `%s` slot the way `err` does, so this path cannot report which
/// number killed the denominator.
pub const NOT_VALID_SENTENCE: &str = "denominator is not valid.";

/// The denominator C's `case 0` fixes: with no operand at all, `denom = 2`
/// (`random.c:90-92`). `denom` is a `double` in C (`random.c:67`), so this constant
/// is declared as `f64` too.
pub const NO_OPERAND_DENOMINATOR: f64 = 2.0;

/// Where the operand step lands: C's `switch (argc)` (`random.c:89-104`, with
/// `argc` already the count left after `optind`) has four exits, and this type
/// matches them one by one.
// `PartialEq` only: the `Accepted` slot holds C's `double`, and floats have no total
// equality.
#[derive(Debug, Clone, PartialEq)]
pub enum OperandLeg {
    /// `case 0`: no operand, so the denominator is [`NO_OPERAND_DENOMINATOR`].
    NoOperandDefault,
    /// `case 1` accepted this operand: the denominator is the `double` that `strtod`
    /// reported.
    Accepted(f64),
    /// `case 1` rejected this operand (`denom == 0 || *ep != '\0'`, `:98`):
    /// [`NOT_VALID_SENTENCE`].
    NotValid,
    /// `case 1` hit the numeric range (`errno == ERANGE`, `:96`): the three-part
    /// `err(1, "%s", *argv)`.
    OutOfRange,
    /// `default`: two or more operands (`:101-103`), [`USAGE`] and exit 1.
    TooMany,
}

/// What is left after C's optstring `"er"` runs the ring: the two flags and the
/// operand start (`random.c:72-87`).
///
/// Neither letter **takes a value**, so libc's `option requires an argument` is
/// unreachable on this path; the only reachable one is an off-letter (`random.c:80-83`
/// sends both `?` and `default` into `usage()`). A bare `-` is neither an option nor
/// an end marker: the optstring has no `-`, libc returns `-1` and **does not advance
/// the cursor** (`minix3/lib/libc/stdlib/getopt.c:86-93`), so it stays among the
/// operands for `strtod` to reject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    /// The ring ended normally; `operands_at` is where the operands start in
    /// `argv[1..]` (C's `optind` translated).
    Done {
        /// `-e`: return the draw as the exit status, before reading standard input
        /// (`random.c:109-111`).
        exit_status: bool,
        /// `-r`: turn off stdio's output buffering (`random.c:117-118`).
        unbuffered: bool,
        /// The operands start at this index.
        operands_at: usize,
    },
    /// An off-letter: libc prints `unknown option -- <letter>` first, and the
    /// `usage()` line follows immediately.
    ///
    /// The slot holds **one byte**, not one character:
    /// `minix3/lib/libc/stdlib/getopt.c:79`'s `optopt = *place++` takes one byte at a
    /// time, and `:103` writes it out as is with `%c`. The same multibyte UTF-8 flag
    /// (`-é` is `c3 a9`) is reported in C as its first byte `c3`, while holding a
    /// character would go through UTF-8 encoding and add a second byte, `83`; the
    /// report must therefore carry the byte, not a re-encoded character.
    Unknown(u8),
}

/// The whole libc line: `<program name>: unknown option -- <one byte>\n`
/// (`getopt.c:101-104`).
///
/// Handing back a byte string rather than a `String` is exactly this defect's shape:
/// the middle of the sentence is ASCII, but the final letter slot is **a raw,
/// unencoded byte**, and pushing it into a `String` and calling `.as_bytes()` lets
/// Rust's UTF-8 rules decide how many bytes it occupies. The reference binary
/// `tmp/random-421/random` (`random.c` linked against Minix's own `getopt.c`,
/// started through a same-named symlink) measures `-é` as the single byte `303`
/// followed by a newline.
#[must_use]
pub fn unknown_option_line(prog: &str, letter: u8) -> alloc::vec::Vec<u8> {
    let mut line = alloc::format!("{prog}: {UNKNOWN_OPTION}").into_bytes();
    line.push(letter);
    line.push(b'\n');
    line
}

/// The middle of C's libc line with the program name and the newline removed
/// (`minix3/lib/libc/stdlib/getopt.c:101-104` spells `"%s: unknown option -- %c\n"`).
/// There are **no** quotes around the letter — the reference was built on the host,
/// and its `invalid option -- 'Z'` is glibc's wording, outside this contract.
pub const UNKNOWN_OPTION: &str = "unknown option -- ";

/// Walk the option ring. Only tokens that start with `-` and are longer than one
/// count as options; `--` is the end marker; everything else (including a bare `-`)
/// counts as an operand from that token on. An off-letter returns on the spot and the
/// ring stops (C's `usage()` exits).
pub fn scan_ring(tokens: &[&[u8]]) -> Ring {
    let mut exit_status = false;
    let mut unbuffered = false;
    let mut index = 0usize;
    while index < tokens.len() {
        let body: &[u8] = tokens[index];
        if body.first() != Some(&b'-') || body.len() == 1 {
            break;
        }
        if &body[1..] == b"-" {
            index += 1;
            break;
        }
        for &letter in &body[1..] {
            match letter {
                b'e' => exit_status = true,
                b'r' => unbuffered = true,
                other => {
                    return Ring::Unknown(other);
                }
            }
        }
        index += 1;
    }
    Ring::Done {
        exit_status,
        unbuffered,
        operands_at: index,
    }
}

/// C's scale: the upper bound of `random()` (`random.c:111` and `:126` both divide
/// by it).
pub const RANDOM_MAX: u64 = 2_147_483_647;

/// C's `-e` path: `(denom * random()) / RANDOM_MAX` (`random.c:111`).
///
/// The source comment says "between zero and denom minus one", but the expression
/// reaches exactly `denom` when the draw hits the top — **the upper bound is closed**,
/// and the face pins the expression rather than the comment. The return value is the
/// `double`→`int` conversion C performs before `return`: truncation toward zero. A
/// non-finite value or one beyond `i32` is undefined behavior in C; here the
/// saturating semantics of `as` apply (the x86-64 reference gives `0x80000000`),
/// recorded as an accepted divergence.
pub fn exit_status_value(denominator: f64, draw: u64) -> i32 {
    (denominator * draw as f64 / RANDOM_MAX as f64) as i32
}

/// The predicate for one filter line: C's `(int)(denom * random() / RANDOM_MAX) == 0`
/// (`random.c:126`).
///
/// Truncating the quotient toward zero gives 0 exactly when the quotient lies strictly
/// inside `(-1, 1)`. The open-interval comparison is used instead of converting to an
/// integer first because that conversion is undefined behavior in C for `inf`/`nan`,
/// while the interval test gives the same answer as the x86-64 reference on both:
/// neither is 0, so neither selects. `inf * 0` is NaN and also falls outside the
/// interval.
pub fn line_selected(denominator: f64, draw: u64) -> bool {
    let scaled = denominator * draw as f64 / RANDOM_MAX as f64;
    scaled > -1.0 && scaled < 1.0
}

/// Decide which exit the operand step takes.
///
/// The count follows C's `switch (argc)`; the one-operand branch hands the value to
/// [`scan_strtod`] and then splits by C's two tests: `errno == ERANGE` takes the
/// three-part form that carries the operand, while `denom == 0` or `*ep != '\0'`
/// belongs to the `errx` sentence without the operand. **`ERANGE` is tested first**
/// (`random.c:96` is earlier than `:98`), so `18e308` reports the errno rather than
/// "not a valid denominator" — even though that value could not serve as one anyway.
pub fn judge_operands(operands: &[&[u8]]) -> OperandLeg {
    let [only] = operands else {
        return if operands.is_empty() {
            OperandLeg::NoOperandDefault
        } else {
            OperandLeg::TooMany
        };
    };
    let read = scan_strtod(only);
    if read.erange {
        OperandLeg::OutOfRange
    } else if read.value == 0.0 || read.rest != only.len() {
        OperandLeg::NotValid
    } else {
        OperandLeg::Accepted(read.value)
    }
}

/// The three things `strtod` reports back, which is exactly what `random.c:95-99`
/// uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strtod {
    /// The value `strtod` returns: a `double`.
    pub value: f64,
    /// The start of the unconsumed run, i.e. C's `ep` index. `rest == word.len()` is
    /// `*ep == '\0'`; when not one character was consumed (C's `ret0`) it is 0. Forms
    /// whose value is judged 0 do not look at this number — `denom == 0` blocks them
    /// first.
    pub rest: usize,
    /// C's `errno == ERANGE`: both the overflow leg (`strtod.c`'s `ovfl`) and the
    /// underflow-to-zero leg on a nonzero digit string (`undfl`) set it, and both
    /// converge on `range_err`.
    pub erange: bool,
}

/// A rewrite of the **decimal leg** of C's `strtod(*argv, &ep)` (`random.c:95`):
/// leading whitespace, one optional sign, digits and one decimal point, an optional
/// `e`/`E` exponent, plus the `inf`/`nan` forms that `#ifdef INFNAN_CHECK` keeps on
/// (`minix3/lib/libc/gdtoa/gdtoaimp.h:191` and `:643-645` both enable it).
///
/// The shape comes from `_int_strtod_l` in `minix3/lib/libc/gdtoa/strtod.c`:
/// whitespace and the sign in the leading `switch (*s)`, the exponent in
/// `if (c == 'e' || c == 'E')`, and `inf`/`nan` in the "no digit seen" block. `match`
/// folds case (`minix3/lib/libc/gdtoa/smisc.c:133-135`), so `INF`, `Infinity` and
/// `NaN` all count. Two forms are outside the contract and recorded as open items:
/// hexadecimal floating point `0x1p3` (the `#ifndef NO_HEX_FP` block in the same
/// function; this repository never defines that macro) and the payload of
/// `nan(0x…)`. Rounding is not bit-for-bit either: C goes through gdtoa's bignum
/// refinement, while this multiplies `significand × 10^exponent` and drops the tail
/// of a digit string longer than 17 digits, so extreme digit counts can differ by one
/// unit in the last place — affecting only the selection decision at the exact
/// threshold.
pub fn scan_strtod(word: &[u8]) -> Strtod {
    let bytes = word;
    let mut index = 0usize;
    while matches!(bytes.get(index), Some(&c) if matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    {
        index += 1;
    }
    let mut negative = false;
    match bytes.get(index) {
        Some(b'-') => {
            negative = true;
            index += 1;
        }
        Some(b'+') => index += 1,
        _ => {}
    }
    // Nothing follows the sign: C goes straight to `ret0` from the sign position, with
    // value 0 and no character consumed.
    if index == bytes.len() {
        return Strtod {
            value: 0.0,
            rest: 0,
            erange: false,
        };
    }

    // Right after C's `if (*s == '0')`, one look at `s[1]`: if it is `x`/`X` the whole
    // thing goes to `gethex`, and neither the decimal leg nor `inf`/`nan` runs (the
    // `#ifndef NO_HEX_FP` block in `strtod.c` does `goto ret`).
    if matches!(bytes.get(index), Some(&b'0'))
        && matches!(bytes.get(index + 1), Some(b'x') | Some(b'X'))
    {
        return scan_hex(bytes, index, negative);
    }

    let mut mantissa = 0.0f64;
    let mut kept = 0u32; // digits that entered mantissa
    let mut dropped = 0i32; // digits dropped past the limit, folded into the power
    let mut fraction = 0i32; // digits after the decimal point
    let mut any_digit = false;
    let mut nonzero = false;
    let mut past_dot = false;
    while let Some(&c) = bytes.get(index) {
        if c == b'.' && !past_dot {
            past_dot = true;
            index += 1;
            continue;
        }
        if !c.is_ascii_digit() {
            break;
        }
        any_digit = true;
        if nonzero || c != b'0' {
            nonzero = true;
            if kept < 17 {
                mantissa = mantissa * 10.0 + f64::from(c - b'0');
                kept += 1;
            } else {
                dropped += 1;
            }
        }
        if past_dot {
            fraction += 1;
        }
        index += 1;
    }
    let mut exponent = dropped - fraction;

    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        if !any_digit {
            // C's `if (!nd && !nz && !nz0) goto ret0`: the `e5` form is not consumed at
            // all.
            return Strtod {
                value: 0.0,
                rest: 0,
                erange: false,
            };
        }
        let mark = index;
        index += 1;
        let mut esign = 1i32;
        match bytes.get(index) {
            Some(b'-') => {
                esign = -1;
                index += 1;
            }
            Some(b'+') => index += 1,
            _ => {}
        }
        if matches!(bytes.get(index), Some(&c) if c.is_ascii_digit()) {
            while matches!(bytes.get(index), Some(&b'0')) {
                index += 1;
            }
            let mut magnitude = 0i32;
            let mut counted = 0u32;
            while matches!(bytes.get(index), Some(&c) if c.is_ascii_digit()) {
                counted += 1;
                if magnitude <= 19_999 {
                    magnitude = magnitude * 10 + i32::from(bytes[index] - b'0');
                }
                index += 1;
            }
            // C's overflow gate: more than 8 digits or a value above 19999 is clamped
            // to 19999 (the `e = 19999; /* safe for 16 bit ints */` line in
            // `strtod.c`).
            if counted > 8 || magnitude > 19_999 {
                magnitude = 19_999;
            }
            exponent = exponent.saturating_add(esign.saturating_mul(magnitude));
        } else {
            // No digit in the exponent slot (`1e`, `1e-`): C backs the cursor up to the
            // `e` and still reports the value.
            index = mark;
        }
    }

    // `inf` and `nan` are checked only when "no digit was seen" (C puts this block
    // inside `if (!nd)`, with `switch (c)` accepting an `i`/`I` or `n`/`N` start).
    if !any_digit {
        // C's `--s` then `match(&s,"inity")`: consume it all when it fits, otherwise
        // fall back to just after `nf`, so both `inf` and `infinity` count and `infi`
        // does not.
        let infinity = match bytes.get(index) {
            Some(b'i') | Some(b'I') => match_fold(bytes, index + 1, b"nf")
                .map(|after_nf| match_fold(bytes, after_nf, b"inity").unwrap_or(after_nf)),
            _ => None,
        };
        if let Some(rest) = infinity {
            return Strtod {
                value: if negative {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                rest,
                erange: false,
            };
        }
        let not_a_number = match bytes.get(index) {
            Some(b'n') | Some(b'N') => match_fold(bytes, index + 1, b"an"),
            _ => None,
        };
        if let Some(rest) = not_a_number {
            // The payload slot: `#ifndef No_Hex_NaN` (this repository never defines
            // that macro) sends the text after `nan(` to `hexnan`, whose only gift to
            // the caller is "where the cursor lands".
            let rest = if matches!(bytes.get(rest), Some(&b'(')) {
                nan_payload(bytes, rest)
            } else {
                rest
            };
            return Strtod {
                value: f64::NAN,
                rest,
                erange: false,
            };
        }
        return Strtod {
            value: 0.0,
            rest: 0,
            erange: false,
        };
    }

    let mut value = mantissa;
    if nonzero {
        // `10^n` by repeated squaring, the same shape as gdtoa's `tens[]`/`bigtens[]`;
        // beyond `f64`'s range it becomes `inf` itself, so the two checks below catch
        // overflow and underflow.
        value = if exponent >= 0 {
            mantissa * pow10(exponent as u32)
        } else {
            mantissa / pow10(exponent.unsigned_abs())
        };
    }
    Strtod {
        value: if negative { -value } else { value },
        rest: index,
        erange: nonzero && (value == 0.0 || value.is_infinite()),
    }
}

/// `match(&s, t)` (`minix3/lib/libc/gdtoa/smisc.c:123-140`): compare a literal
/// starting at `at`, folding **both sides** to lowercase when they match, and return
/// the index after the consumed text. C increments once before comparing, so the first
/// letter the caller passed in (the one `switch (c)` accepted) is not part of the
/// comparison.
fn match_fold(bytes: &[u8], at: usize, lit: &[u8]) -> Option<usize> {
    let tail = bytes.get(at..at + lit.len())?;
    tail.iter()
        .zip(lit)
        .all(|(got, want)| got.to_ascii_lowercase() == *want)
        .then_some(at + lit.len())
}

/// An integer power of ten (non-negative exponent).
fn pow10(power: u32) -> f64 {
    let mut base = 10.0f64;
    let mut out = 1.0f64;
    let mut rest = power;
    while rest != 0 {
        if rest & 1 != 0 {
            out *= base;
        }
        rest >>= 1;
        if rest != 0 {
            base *= base;
        }
    }
    out
}

/// An integer power of two (non-negative exponent), the same shape as [`pow10`].
/// Hexadecimal floating point is scaled by powers of two anyway, so this path is
/// **exact** while the significand stays within 13 digits (52 binary bits).
fn pow2(power: u32) -> f64 {
    let mut base = 2.0f64;
    let mut out = 1.0f64;
    let mut rest = power;
    while rest != 0 {
        if rest & 1 != 0 {
            out *= base;
        }
        rest >>= 1;
        if rest != 0 {
            base *= base;
        }
    }
    out
}

/// All `hexnan` (`minix3/lib/libc/gdtoa/hexnan.c:56-152`) leaves its caller is one
/// thing: where the cursor lands. The payload's bit pattern is **invisible** on this
/// `random` path — it reads the value only through `denom == 0` and the two
/// expressions, and every NaN gives the same answer (`denom == 0` is always false and
/// a non-finite value never selects). So what is rewritten here is "how much is
/// consumed", not "which bits go into `bits[]`": consume from `(` through the **first
/// `)`**; a run with no `)` leaves the cursor on the `(`. The two exits in the source
/// are `hexnan.c:105-109` (the digit string meets `)`, `*sp = s + 1`) and `:112-118`
/// (after a non-hex character the `do { … } while` looks forward for `)`, and a hit is
/// also `*sp = s + 1`); the misses are `:146-147`'s `if (!havedig) return STRTOG_NaN`
/// and the loop meeting `'\0'`, neither of which touches `*sp`. This repository does
/// not define `GDTOA_NON_PEDANTIC_NANCHECK`, so the forward search for `)` is live.
fn nan_payload(bytes: &[u8], open: usize) -> usize {
    match bytes[open..].iter().position(|&byte| byte == b')') {
        Some(offset) => open + offset + 1,
        None => open,
    }
}

/// The value of one hexadecimal digit, or `None` when it is not hexadecimal. This
/// mirrors gdtoa's `hexdig[]` table: it lays `'0'`..`'9'` out as `0x10`..`0x19` and
/// `'a'`..`'f'` with `'A'`..`'F'` as `0x1a`..`0x1f`, so `gethex.c`'s `n > 0x19` means
/// "the exponent slot accepts decimal digits only".
fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a') + 10),
        b'A'..=b'F' => Some(u32::from(byte - b'A') + 10),
        _ => None,
    }
}

/// C's `gethex` leg (`minix3/lib/libc/gdtoa/gethex.c:54-131`), called from the
/// `#ifndef NO_HEX_FP` block in `strtod.c` when `*s == '0'` and `s[1]` is `x`/`X`.
///
/// Three things differ from the decimal leg, all copied from the source:
/// (1) the binary exponent is **optional** — the `pcheck:` `switch (*s)` has only the
///     `p`/`P` cases and no rejecting `default` (`gethex.c:110-112`), so `0x10` is 16.
/// (2) this leg **never sets `errno`**: overflow and underflow return
///     `STRTOG_Overflow`/`STRTOG_Underflow` in `gethex.c`, while `strtod.c`'s
///     hexadecimal `switch` distinguishes only `NoNumber`/`Zero`, and the remaining
///     branch goes to `ULtod` and then straight to `goto ret`, never through the
///     decimal `range_err`. So `0x1p99999999` is "an accepted infinity", not the
///     `err(1, "%s", *argv)` form — the host reference links glibc, which raises
///     ERANGE here, and its `Numerical result out of range` sentence is outside this
///     contract.
/// (3) the exponent slot accepts decimal digits only, and an incomplete run backs the
///     cursor up before the `p` (`gethex.c:122-126`'s `s = s1`), the same shape as the
///     decimal leg's `1e`: `0x1p` reports 1 with `ep` stopped at `p`, so
///     `*ep != '\0'` rejects it.
///
/// A significand longer than 13 digits is truncated (folded into the power of two),
/// sharing the same last-unit residual note as the decimal leg.
fn scan_hex(bytes: &[u8], at: usize, negative: bool) -> Strtod {
    let mut index = at + 2;
    while bytes.get(index) == Some(&b'0') {
        index += 1;
    }
    let mut mantissa: u64 = 0;
    let mut kept = 0u32;
    let mut extra = 0i32;
    let mut fraction = 0i32;
    let mut any_hex_digit = false;
    let mut nonzero = false;
    let mut past_dot = false;
    while let Some(&c) = bytes.get(index) {
        if c == b'.' && !past_dot {
            past_dot = true;
            index += 1;
            continue;
        }
        let Some(digit) = hex_value(c) else {
            break;
        };
        any_hex_digit = true;
        if nonzero || digit != 0 {
            nonzero = true;
            if kept < 13 {
                mantissa = mantissa * 16 + u64::from(digit);
                kept += 1;
            } else {
                extra += 1;
            }
        }
        if past_dot {
            fraction += 1;
        }
        index += 1;
    }
    if !any_hex_digit {
        // C's `if (!havedig) *sp = s0 - 1;` plus `return STRTOG_Zero`: value 0 and
        // the cursor backed up to `x`.
        return Strtod {
            value: 0.0,
            rest: at + 1,
            erange: false,
        };
    }
    let mut exponent = (extra << 2) - (fraction << 2);
    if matches!(bytes.get(index), Some(b'p') | Some(b'P')) {
        let mark = index;
        index += 1;
        let mut esign = 1i32;
        match bytes.get(index) {
            Some(b'-') => {
                esign = -1;
                index += 1;
            }
            Some(b'+') => index += 1,
            _ => {}
        }
        if matches!(bytes.get(index), Some(&c) if c.is_ascii_digit()) {
            while matches!(bytes.get(index), Some(&b'0')) {
                index += 1;
            }
            let mut magnitude = 0i32;
            let mut big = false;
            while matches!(bytes.get(index), Some(&c) if c.is_ascii_digit()) {
                if magnitude > 0x07ff_ffff {
                    big = true;
                } else {
                    magnitude = magnitude * 10 + i32::from(bytes[index] - b'0');
                }
                index += 1;
            }
            if !big {
                exponent += esign * magnitude;
            } else if esign < 0 {
                // `ret_tiny`: value 0, and per (2) no `errno`.
                return Strtod {
                    value: 0.0,
                    rest: index,
                    erange: false,
                };
            } else {
                // `ret_huge`: value `±inf`, also without setting `errno`.
                return Strtod {
                    value: if negative {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    },
                    rest: index,
                    erange: false,
                };
            }
        } else {
            index = mark;
        }
    }
    let magnitude = if exponent >= 0 {
        mantissa as f64 * pow2(exponent as u32)
    } else {
        mantissa as f64 / pow2(exponent.unsigned_abs())
    };
    Strtod {
        value: if negative { -magnitude } else { magnitude },
        rest: index,
        erange: false,
    }
}

/// Mix a clock reading with a process identifier into a seed (mirrors the
/// seeding blend of time plus process in the original source).
pub fn mix_seed(seconds: u64, micros: u32, pid: u32) -> u64 {
    seconds
        .wrapping_add(micros as u64)
        .wrapping_add(pid as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_decimal_grammar_is_the_one_strtod_reads() {
        // Value, `ep` index, and whether `ERANGE` is set: all three readings pinned
        // together, shaped after the decimal leg in `strtod.c`.
        let cases: [(&str, f64, usize, bool); 14] = [
            ("6", 6.0, 1, false),
            (" 3", 3.0, 2, false), // leading whitespace consumed
            ("3 ", 3.0, 1, false), // the tail is not consumed: the caller tests `*ep != '\0'`
            ("1e3", 1000.0, 3, false),
            ("1e-3", 0.001, 4, false),
            (".5", 0.5, 2, false),
            ("5.", 5.0, 2, false),
            ("0.001", 0.001, 5, false),
            ("010", 10.0, 3, false),
            ("1e00", 1.0, 4, false), // only zeros in the exponent slot: C consumes them and records the exponent as 0
            ("-2", -2.0, 2, false),
            ("4294967296", 4.294967296e9, 10, false),
            ("18e308", f64::INFINITY, 6, true),
            ("1e-400", 0.0, 6, true),
        ];
        for (word, value, rest, erange) in cases {
            let read = scan_strtod(word.as_bytes());
            assert_eq!(read.value, value, "{word:?} 的值");
            assert_eq!((read.rest, read.erange), (rest, erange), "{word:?}");
        }
        // Bit progression only within 17 digits: the tail is dropped and folded into
        // the decimal power, so the 21-digit string lands on 2.1e20.
        assert_eq!(scan_strtod(b"210000000000000000000").value, 2.1e20);
        // `inf` and `nan` come from the `INFNAN_CHECK` block; `match` folds case, so
        // all three forms count.
        assert_eq!(scan_strtod(b"inf").value, f64::INFINITY);
        assert_eq!(scan_strtod(b"infinity").rest, 8);
        assert_eq!(scan_strtod(b"INF").rest, 3);
        assert_eq!(scan_strtod(b"-inf").value, f64::NEG_INFINITY);
        assert!(scan_strtod(b"nan").value.is_nan());
        assert!(scan_strtod(b"NaN").value.is_nan());
        assert_eq!(scan_strtod(b"nan").rest, 3);
        // For `infi`, C consumes nothing beyond the start of `inity`: `ep` stops at
        // position 3.
        assert_eq!(scan_strtod(b"infi").rest, 3);
    }

    #[test]
    fn a_nan_payload_moves_the_end_pointer_and_nothing_else() {
        // The value is always NaN (this path cannot see which bits the payload holds);
        // the only difference is `ep`: a `)` marks the whole string consumed, and with
        // no `)` the cursor stays on the `(` for `*ep != '\0'` to reject.
        for (word, expected_rest) in [
            ("nan(0x1f)", 9),
            ("nan()", 5),
            ("nan(xy)", 7),
            ("nan( 0x1f )", 11),
            ("NAN(0123456789abcdef)", 21),
        ] {
            let read = scan_strtod(word.as_bytes());
            assert!(read.value.is_nan(), "{word:?} 应当是 NaN");
            assert_eq!((read.rest, read.erange), (expected_rest, false), "{word:?}");
            assert_eq!(read.rest, word.len(), "{word:?} 应当整串吃掉");
        }
        for word in ["nan(0x1f", "nan(", "nan(garbage"] {
            let open = word.find('(').expect("测试串里就有左括号");
            let read = scan_strtod(word.as_bytes());
            assert!(read.value.is_nan(), "{word:?}");
            assert_eq!(
                read.rest, open,
                "{word:?} 没有右括号，游标该留在左括号那一格"
            );
            assert_eq!(
                judge_operands(&[word.as_bytes()]),
                OperandLeg::NotValid,
                "{word:?}"
            );
        }
        assert!(matches!(
            judge_operands(&[b"nan(0x1f)"]),
            OperandLeg::Accepted(value) if value.is_nan()
        ));
    }

    #[test]
    fn the_hex_leg_is_gethex_shaped() {
        // The three places that differ from the decimal leg are pinned after
        // `gethex.c`: the binary exponent is optional, `ERANGE` is never set, and the
        // exponent slot accepts decimal digits only and backs up before `p` when short.
        let cases: [(&str, f64, usize); 11] = [
            ("0x10", 16.0, 4),
            ("0xff", 255.0, 4),
            ("0x1p3", 8.0, 5),
            ("0X1P3", 8.0, 5),
            ("0x.8p0", 0.5, 6),
            ("0x1.8p1", 3.0, 7),
            ("-0x10", -16.0, 5),
            ("0x1p", 1.0, 3), // exponent slot empty: `s = s1` backs up to `p`
            ("0x", 0.0, 1),   // no digit at all: `*sp = s0 - 1`, cursor stops at `x`
            ("0xg", 0.0, 1),
            ("0x1p-99999999", 0.0, 13), // the `ret_tiny` path: value 0 and **no** errno
        ];
        for (word, value, rest) in cases {
            let read = scan_strtod(word.as_bytes());
            assert_eq!(read.value, value, "{word:?} 的值");
            assert_eq!((read.rest, read.erange), (rest, false), "{word:?}");
        }
        // The overflow form splits from the decimal `18e308`: the hexadecimal branch
        // bypasses `range_err` entirely.
        let huge = scan_strtod(b"0x1p99999999");
        assert_eq!(huge.value, f64::INFINITY);
        assert!(!huge.erange, "十六进制溢出立号就成了另一句报文");
        assert_eq!(huge.rest, 12);
    }

    #[test]
    fn the_selection_predicate_is_the_cast_then_compare_itself() {
        // C's line is "truncate to an integer, then compare with 0" (`random.c:126`):
        // truncating the quotient toward zero gives 0 exactly when it lies in (-1,1).
        assert!(line_selected(1.0, 0));
        assert!(line_selected(1.0, RANDOM_MAX - 1));
        assert!(!line_selected(1.0, RANDOM_MAX)); // quotient exactly 1.0: `(int)1.0 == 1` → no selection
        assert!(line_selected(2.0, 1));
        assert!(!line_selected(2.0, 1_073_741_824)); // quotient just past 1.0
        assert!(!line_selected(2.0, RANDOM_MAX)); // quotient exactly 2.0
        assert!(line_selected(0.5, RANDOM_MAX)); // a denominator below one always selects; the quotient tops out at 0.5
        assert!(line_selected(-1e-3, RANDOM_MAX)); // negative truncates toward zero: (-1, 0] always becomes 0
        assert!(!line_selected(-1.0, RANDOM_MAX)); // quotient exactly -1.0 → truncates to -1 → no selection
        assert!(line_selected(-1.0, RANDOM_MAX - 1));
        assert!(!line_selected(f64::INFINITY, 7));
        assert!(!line_selected(f64::NEG_INFINITY, 7));
        assert!(!line_selected(f64::NAN, 7));
        assert!(!line_selected(f64::INFINITY, 0)); // `inf * 0` is NaN and also outside the interval
    }

    #[test]
    fn test_seed_mixes() {
        assert_ne!(mix_seed(100, 5, 7), mix_seed(100, 5, 8));
        assert_ne!(mix_seed(100, 5, 7), mix_seed(101, 5, 7));
    }

    #[test]
    fn the_operand_count_picks_the_leg() {
        // The four exits of `switch (argc)` (`random.c:89-104`): zero operands is not
        // "missing argument" but "denominator 2", two or more is usage, and only the
        // middle one reaches the value test.
        assert_eq!(judge_operands(&[]), OperandLeg::NoOperandDefault);
        assert_eq!(judge_operands(&[b"3"]), OperandLeg::Accepted(3.0));
        assert_eq!(judge_operands(&[b"3", b"4"]), OperandLeg::TooMany);
        // A third operand that is an empty string does not change the count: the count
        // is judged, not the content.
        assert_eq!(judge_operands(&[b"3", b"", b"4"]), OperandLeg::TooMany);
        assert_eq!(NO_OPERAND_DENOMINATOR, 2.0);
    }

    #[test]
    fn a_rejected_operand_and_an_overflowed_one_are_two_different_legs() {
        // `denom == 0 || *ep != '\0'` (`:98`) and `errno == ERANGE` (`:96`) are two
        // different sentences: the first has no operand, the second puts the operand in
        // a slot. Merging the second into the first would lose the errno.
        for word in ["0", "", "abc", "-", "6x", "e5", "1e", "infi"] {
            assert_eq!(
                judge_operands(&[word.as_bytes()]),
                OperandLeg::NotValid,
                "{word:?}"
            );
        }
        for word in ["18e308", "1e-400"] {
            assert_eq!(
                judge_operands(&[word.as_bytes()]),
                OperandLeg::OutOfRange,
                "{word:?}"
            );
        }
        // An all-zero digit string sets no ERANGE (C's `nz0` path goes straight to
        // `goto ret`) and falls back to "not a valid denominator", however absurd the
        // exponent.
        assert_eq!(judge_operands(&[b"0e99999"]), OperandLeg::NotValid);
        assert_eq!(judge_operands(&[b"1"]), OperandLeg::Accepted(1.0));
        // `0x10` goes through `gethex`: the `pcheck:` `switch (*s)` has no rejecting
        // `default` (`gethex.c:110-112`), so hexadecimal without a binary exponent is
        // still accepted, as 16.
        assert_eq!(judge_operands(&[b"0x10"]), OperandLeg::Accepted(16.0));
        // `2^32` is exact in a `double`, so it must not report overflow.
        assert_eq!(
            judge_operands(&[b"4294967296"]),
            OperandLeg::Accepted(4.294967296e9)
        );
        // `nan` is accepted (C's `denom == 0` is false for NaN), but the value cannot
        // be compared: NaN is not equal to itself under `PartialEq`, so this only checks
        // that it lands in the `Accepted` branch.
        assert!(matches!(
            judge_operands(&[b"nan"]),
            OperandLeg::Accepted(value) if value.is_nan()
        ));
    }

    #[test]
    fn the_two_hardwired_lines_keep_c_spelling() {
        // The usage line starts with the literal `random` and carries its own newline
        // (`:143-147`); the `errx` sentence has a trailing period and no operand
        // (`:98-99`). Both are compared byte for byte, so one changed character means
        // changing this assertion.
        assert_eq!(USAGE, "usage: random [-er] [denominator]\n");
        assert_eq!(NOT_VALID_SENTENCE, "denominator is not valid.");
    }

    #[test]
    fn the_option_ring_stops_at_the_first_offender() {
        // The ring accepts only `e`/`r`, and an off-letter returns on the spot. A
        // cluster reports its **first** off-letter, so `-eZ` and `-Ze` both stop at `Z`
        // (C's ring calls usage() and exits at the first `?`, never scanning further).
        assert_eq!(
            scan_ring(&[]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 0
            }
        );
        assert_eq!(
            scan_ring(&[b"-e"]),
            Ring::Done {
                exit_status: true,
                unbuffered: false,
                operands_at: 1
            }
        );
        assert_eq!(
            scan_ring(&[b"-er", b"-r", b"3"]),
            Ring::Done {
                exit_status: true,
                unbuffered: true,
                // The index is into `argv[1..]`: after two option tokens are consumed,
                // `3` is at position 2.
                operands_at: 2
            }
        );
        assert_eq!(scan_ring(&[b"-Z"]), Ring::Unknown(b'Z'));
        assert_eq!(scan_ring(&[b"-eZ"]), Ring::Unknown(b'Z'));
        assert_eq!(scan_ring(&[b"-Ze"]), Ring::Unknown(b'Z'));
    }

    /// The off-letter slot hands back the **first byte** after the dash, not the first
    /// character: `-é` is `c3 a9` in bytes, and the reference binary
    /// `tmp/random-421/random` measures a single `303` (=`c3`) followed by a newline.
    /// The reader is byte-wise already (`for letter in body.bytes()`); this test pins
    /// that it is, so a future change back to reading characters can be attributed
    /// alone; the byte-wise face is in `tests/random_face.rs`.
    #[test]
    fn the_off_letter_is_a_byte_and_not_a_character() {
        // `é` is two UTF-8 bytes, and the ring stops on the first.
        assert_eq!(scan_ring(&[b"-\xc3\xa9"]), Ring::Unknown(0xc3));
        // A legal letter first changes nothing: `-eé` still reports `c3` (`e` is
        // consumed and the ring moves on to the next byte).
        assert_eq!(scan_ring(&[b"-e\xc3\xa9"]), Ring::Unknown(0xc3));
        // The U+0080 slot is this rendering rule's decisive position: UTF-8 encoding
        // needs two bytes for it, while C prints one raw byte (`-\u{80}` is
        // `c2 80`, and the first `c2` is reported).
        assert_eq!(scan_ring(&[b"-\xc2\x80"]), Ring::Unknown(0xc2));
        // The other way round: `--` is the end marker and is not reported as an
        // off-letter (`getopt.c:81-85`).
        assert_ne!(scan_ring(&[b"--"]), Ring::Unknown(b'-'));
    }

    /// The sentence side has exactly one packing: the letter slot is the raw byte,
    /// with no re-encoding.
    #[test]
    fn the_libc_line_writes_the_letter_as_the_byte_it_was_given() {
        assert_eq!(
            unknown_option_line("random", b'Z'),
            b"random: unknown option -- Z\n".to_vec()
        );
        // The same U+00C3: written byte-wise it is the first of the two bytes, written
        // character-wise it would add an `83`.
        assert_eq!(
            unknown_option_line("random", 0xc3),
            b"random: unknown option -- \xc3\n".to_vec()
        );
        assert_eq!(
            unknown_option_line("random", 0xff),
            b"random: unknown option -- \xff\n".to_vec()
        );
    }

    #[test]
    fn end_of_options_and_a_bare_dash_are_two_different_things() {
        // `--` is the end marker: it does not count as an operand itself, only what
        // follows does. A bare `-` is the opposite — it **is** an operand.
        assert_eq!(
            scan_ring(&[b"--"]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 1
            }
        );
        assert_eq!(
            scan_ring(&[b"--", b"3"]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 1
            }
        );
        assert_eq!(
            scan_ring(&[b"-"]),
            Ring::Done {
                exit_status: false,
                unbuffered: false,
                operands_at: 0
            }
        );
        assert_eq!(
            scan_ring(&[b"-e", b"-"]),
            Ring::Done {
                exit_status: true,
                unbuffered: false,
                operands_at: 1
            }
        );
    }

    #[test]
    fn the_exit_status_scales_by_the_expression_not_by_the_comment() {
        // The upper bound is closed: at the top draw the result is the denominator
        // itself.
        assert_eq!(exit_status_value(1.0, 0), 0);
        // Half the draws give 1, not 2: the multiplication runs first and the division
        // follows, so `4 * (RANDOM_MAX/2)` falls just short of 2.
        assert_eq!(exit_status_value(4.0, RANDOM_MAX / 2), 1);
        // Three-quarters lands on **2**, not 3: `RANDOM_MAX * 3` does not divide evenly
        // by 4, the division truncates a little first, and the multiplication cannot
        // even it back out.
        assert_eq!(exit_status_value(4.0, RANDOM_MAX * 3 / 4), 2);
        assert_eq!(exit_status_value(4.0, RANDOM_MAX / 4), 0);
        assert_eq!(exit_status_value(4.0, 0), 0);
        assert_eq!(exit_status_value(4.0, RANDOM_MAX), 4);
        // Truncation toward zero holds for negative quotients too:
        // `-1 * RANDOM_MAX / RANDOM_MAX` is -1.0.
        assert_eq!(exit_status_value(-1.0, RANDOM_MAX), -1);
        // A denominator below one cannot draw a nonzero exit status.
        assert_eq!(exit_status_value(0.5, RANDOM_MAX), 0);
        assert_eq!(RANDOM_MAX, u64::from(u32::MAX) / 2);
    }
}
