//! Number sequence generation for `seq`.
//!
//! Ground truth: `minix3/usr.bin/seq/seq.c` (NetBSD): `seq [-w] [-s
//! string] [first [incr]] last`, defaults first 1 and increment 1, the
//! loop runs while the value has not passed `last`, `-s` changes the
//! separator (default newline), `-w` equalises all renderings to the
//! widest one (the C's `generate_format`, lines 424-474 — for the
//! integer slice of this module the widest of the first and last
//! rendering, zero-filled with the sign taking a slot), and `-t`
//! appends a terminator after the last number. Zero increments are
//! rejected by name (seq.c:147-148).
//!
//! The C generates in floating point (`double`/`long double` with
//! `%g`-style formats); floating increments and `-f` formats are their
//! own design decision (the same float-rendering adjudication as
//! printf's `e E f g G`) and are rejected by this module.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// What went wrong building a sequence.
///
/// The exits split three ways and the exit codes differ: `Usage` and
/// `Zero*`/`Needs*`/`BadFormat*` follow C's `exit(1)`/`errx(1, ...)` family, while
/// `InvalidNumber`/`OutOfRange` follow `e_atof()`'s `err(2, "%s", num)` and
/// `errx(2, "invalid floating point argument: %s", num)` family (`seq.c:383-400`) —
/// exit 2 is not a variant of exit 1, and the two families have different sentence
/// owners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqError {
    /// The branch `errflg` accumulates into: **one** usage line after libc's lines (zero,
    /// one or many). `-h`, a wrong operand count and an in-ring letter falling into
    /// `default` all land here with an empty `reports`.
    Usage { reports: Vec<crate::GetoptReport> },
    /// An operand failing `strtod`'s "the tail must have characters" criterion
    /// (`seq.c:394-396`).
    InvalidNumber(Vec<u8>),
    /// An operand overflowing or underflowing to `ERANGE` (`seq.c:391-393`; the
    /// three-part `err` form appends `strerror`).
    OutOfRange(Vec<u8>),
    /// A zero stride given as the third operand in the `first < last` branch
    /// (`seq.c:147-148`).
    ZeroIncrement,
    /// The other zero-stride name (`(first < last) ? "in" : "de"`, so `2 0 2` also
    /// reports `decrement`).
    ZeroDecrement,
    /// `first < last` with a non-positive increment (seq.c:155-156).
    NeedsPositiveIncrement,
    /// `first > last` with a non-negative increment (seq.c:158-159).
    NeedsNegativeDecrement,
    /// The `-f` string fails `valid_format`; the sentence carries the original text
    /// (`seq.c:162-163`).
    BadFormat(Vec<u8>),
    /// Fails the second check, after `unescape`; the sentence does **not** carry the
    /// original text (`seq.c:165-166`). No input could be constructed that triggers it here
    /// (unescaping only collapses `\X` into a single character and cannot create a new
    /// `%`), so this branch has a sentence but no test pinning it.
    BadFormatPlain,
    /// `-f`  requested a floating render, declared unsupported.
    FloatNotModelled,
}

/// One parsed `seq` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqOptions {
    pub first: i64,
    /// `None` when defaulted: the direction follows `first` versus
    /// `last` (seq.c:151-153).
    pub incr: Option<i64>,
    pub last: i64,
    /// `-w` (first occurrence): pad to the common width.
    pub equalize: bool,
    /// The padding character: `0` on the first `-w`, a space from the second on (after
    /// `equalize++` in `seq.c:115-120`, `pad = SPACE`).
    pub pad: char,
    /// `-s`: separator between numbers (default newline).
    pub separator: Vec<u8>,
    /// `-t`: terminator appended after the last number.
    pub terminator: Option<Vec<u8>>,
}

/// What one `seq` invocation holds after running the option ring (the quantities each arm
/// of `seq.c:101-125`'s `switch` sets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqArgs<'a> {
    /// The **original text** of `-f`: C stores only `optarg` in the ring and runs
    /// `valid_format`/`unescape` after reading the operands.
    pub fmt: Option<&'a [u8]>,
    /// The `-s` value; `unescape` runs inside the ring (`seq.c:110`).
    pub separator: Vec<u8>,
    /// The `-t` value; `unescape` runs inside the ring (`seq.c:113`).
    pub terminator: Option<Vec<u8>>,
    /// How many times `-w` appeared in the ring. C writes
    /// `if (!fmt) if (equalize++) pad = SPACE;`, so the first occurrence only sets
    /// `equalize` (pad `0`) and **from the second on** the pad becomes a space.
    pub equalize: u32,
    /// The raw operand words after the ring (C's `argv += optind`).
    pub operands: Vec<&'a [u8]>,
}

/// C's `numeric()` (the pre-ring guard `!numeric(argv[optind])` in `seq.c`).
///
/// It is **not** "parses as a number": it skips one sign, consumes a run of decimal
/// digits, allows one decimal point, and after `e`/`E` a sign **must** follow
/// immediately for the word to count as numeric. The two shapes the reference measured
/// therefore diverge: `-1e-2` is numeric (the guard lets it through — that is what
/// protects `-5`-style operands starting with a dash), while `1e2` is not numeric — the
/// guard does not stop it, but getopt itself ends the ring at the first character that
/// is not `-` (`getopt.c:74-78`), so `seq 1e2` still takes `1e2` as an operand.
#[must_use]
pub fn numeric(word: &[u8]) -> bool {
    let bytes = word;
    let mut i = usize::from(matches!(bytes.first(), Some(b'-') | Some(b'+')));
    let mut seen_point = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            i += 1;
            continue;
        }
        if !seen_point && b == b'.' {
            seen_point = true;
            i += 1;
            continue;
        }
        if b == b'e' || b == b'E' {
            i += 1;
            if matches!(bytes.get(i), Some(b'-') | Some(b'+')) {
                i += 1;
                continue;
            }
        }
        return false;
    }
    true
}

/// C's `unescape()`: `\a \b \e \f \n \r \t \v \\ \' \"`, up to three octal digits
/// after `\`, and up to two hex digits after `\x`; **an unrecognised escape keeps the
/// backslash as it is** (the reference prints `1\q2\q3\q` for `-s '\q' 1 3`), while a
/// single trailing backslash is dropped entirely (`-t '\'` prints, but has no
/// terminator).
#[must_use]
pub fn unescape(text: &[u8]) -> Vec<u8> {
    let bytes = text;
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let Some(&esc) = bytes.get(i + 1) else {
            break; // trailing single backslash: C's loop condition makes the next character `\0`, so nothing more is produced
        };
        i += 2;
        match esc {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'e' => out.push(0x1b),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'\\' => out.push(b'\\'),
            b'\'' => out.push(b'\''),
            b'"' => out.push(b'"'),
            b'0'..=b'7' => {
                // `\\101` → `A`: at most three octal digits after the backslash, and the
                // escaped digit **is the first one**
                let mut j = i - 1;
                let mut value = 0u32;
                let mut count = 0usize;
                while count < 3 && matches!(bytes.get(j), Some(&d) if (b'0'..=b'7').contains(&d)) {
                    value = (value << 3) | u32::from(bytes[j] - b'0');
                    j += 1;
                    count += 1;
                }
                i = j;
                out.push(value as u8);
            }
            b'x' => {
                // `\\xd0` → 0xD0: at most two hex digits (`isxdigit` accepts upper-case
                // **digits**, but the introducer letter is only lower-case `x` — C's
                // `switch` has no `case 'X'`)
                let mut j = i;
                let mut value = 0u32;
                let mut count = 0usize;
                while count < 2 && matches!(bytes.get(j), Some(&d) if d.is_ascii_hexdigit()) {
                    let d = bytes[j];
                    let digit = if d.is_ascii_digit() {
                        d - b'0'
                    } else {
                        (d | 0x20) - b'a' + 10
                    };
                    value = (value << 4) | u32::from(digit);
                    j += 1;
                    count += 1;
                }
                i = j;
                out.push(value as u8);
            }
            other => {
                out.push(b'\\');
                out.push(other);
            }
        }
    }
    out
}

/// C's `valid_format()`: **only** `A a E e F G g` count as conversions, `%%` is skipped
/// without counting, and the final criterion is `conversions <= 1`. The accepted set the
/// reference measured is therefore counter-intuitive: `-f '%d'` and `-f '%d\n'` are
/// **both bad** (`d` is not a float conversion, so it hits `default: return 0`), while
/// `-f 'abc'` and `-f '%%'` are legal (zero conversions) and `-f '%g%g'` is bad for
/// having two conversions.
#[must_use]
pub fn valid_format(fmt: &[u8]) -> bool {
    let bytes = fmt;
    let mut i = 0usize;
    let mut conversions = 0u32;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if bytes.get(i) == Some(&b'%') {
            i += 1;
            continue;
        }
        while i < bytes.len() && b"#0- +'".contains(&bytes[i]) {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        match bytes.get(i) {
            Some(b'A' | b'a' | b'E' | b'e' | b'F' | b'f' | b'G' | b'g') => {
                conversions += 1;
                i += 1;
            }
            // including a trailing `%` (C sees `\0` in `switch (*fmt)` and takes default)
            _ => return false,
        }
    }
    conversions <= 1
}

/// `seq.c:101`'s option ring: `while ((optind < argc) && !numeric(argv[optind]) && (c =
/// getopt(...)) != -1)`.
///
/// This branch differs from its siblings' rings in four ways, all measured against the
/// reference:
/// (1) the ring **does not stop at the first bad letter** — `errflg` accumulates, so
///     `seq -+- 3` prints two libc lines (`+` and `-`; the second `+` lies outside the
///     `-+-` token) plus one usage line;
/// (2) `h` is in the optstring so libc prints nothing for it, while C's
///     `case 'h': /* help (GNU) */` has no `break` and **falls straight into
///     `default: errflg++`**, so `-h` is "only the usage line";
/// (3) the pre-ring `!numeric(argv[optind])` guard only cares about whether the
///     **current token starts with `-`** — a token that does not is ended by libc itself
///     (`getopt.c:74-78` does not advance `optind`), so `seq 1e2` (no sign after `e`, so
///     the guard says it is not numeric) still prints 1..100;
/// (4) hence what the guard really blocks is negative operands: `-1` is an operand,
///     while `-5e310` enters the ring and is split into a run of out-of-set letters.
pub fn parse_seq_args<'a>(args: &[&'a [u8]]) -> Result<SeqArgs<'a>, SeqError> {
    const OPTSTRING: &[u8] = b"f:hs:t:w";
    let mut parsed = SeqArgs {
        fmt: None,
        separator: b"\n".to_vec(),
        terminator: None,
        equalize: 0,
        operands: Vec::new(),
    };
    let mut reports: Vec<crate::GetoptReport> = Vec::new();
    let mut errflg = false;
    let mut i = 0usize;
    while i < args.len() && !numeric(args[i]) {
        let token = args[i];
        // getopt's own end-of-ring criteria: no leading `-`, or a lone `-`, both mean
        // "not an option" and **do not advance** `optind` (`getopt.c:74-78`, `:87-92`);
        // `--` ends the ring and consumes itself (`:81-85`).
        if token.len() < 2 || token[0] != b'-' {
            break;
        }
        if token == b"--" {
            i += 1;
            break;
        }
        let mut k = 1usize;
        while k < token.len() {
            let letter = token[k];
            k += 1;
            // `getopt.c:95-104`: an out-of-set letter **does not stop the ring**. libc
            // prints one line, and `optind` advances only when the token is exhausted, so
            // `-ABC` yields three lines and `-wé` yields one line per raw byte.
            let Some(slot) = OPTSTRING.iter().position(|&o| o == letter && o != b':') else {
                reports.push(crate::GetoptReport::UnknownOption(char::from(letter)));
                errflg = true;
                if k >= token.len() {
                    i += 1;
                }
                continue;
            };
            if OPTSTRING.get(slot + 1) == Some(&b':') {
                let value = if k < token.len() {
                    &args[i][k..]
                } else {
                    i += 1;
                    match args.get(i) {
                        Some(next) => *next,
                        None => {
                            // `getopt.c:127-136`: no value → one libc line + BADCH, `optind` is past the end
                            reports.push(crate::GetoptReport::MissingValue(char::from(letter)));
                            errflg = true;
                            break;
                        }
                    }
                };
                match letter {
                    b'f' => parsed.fmt = Some(value),
                    b's' => parsed.separator = unescape(value),
                    b't' => parsed.terminator = Some(unescape(value)),
                    _ => {}
                }
                // If the value came from the next token, `optind` must skip it
                // (`getopt.c:138-139`); if the value sits on this token, this step moves to
                // the next token. Either way exactly one increment.
                i += 1;
                break;
            }
            match letter {
                // `if (!fmt) if (equalize++) pad = SPACE;`: the count is handed to the
                // shell, which splits it into two levels
                b'w' => {
                    if parsed.fmt.is_none() {
                        parsed.equalize += 1;
                    }
                }
                // `-h` is in the set so libc says nothing; C lets it fall into default
                // and sets errflg
                b'h' => errflg = true,
                _ => {}
            }
            if k >= token.len() {
                i += 1;
            }
        }
    }
    parsed.operands = args[i..].to_vec();
    if parsed.operands.is_empty() || parsed.operands.len() > 3 {
        errflg = true;
    }
    if errflg {
        return Err(SeqError::Usage { reports });
    }
    Ok(parsed)
}

/// C's `e_atof()`: after `strtod` the **tail must reach the end**, otherwise
/// `errx(2, "invalid floating point argument: %s", num)`; `errno == ERANGE` takes the
/// three-part `err(2, "%s", num)` form (the reference: `seq 1e310` → rc 2, and
/// `seq 1e-40000` takes the same shape). The test order is C's too: `ERANGE` first, then
/// leftover characters. For the value `-0.0` C wipes the sign (the "zero shall have no
/// sign" branch of `seq.c:e_atof`), and the same handling applies here.
///
/// Two differences that could not be measured and are only registered: (1) underflow to a
/// **non-zero** subnormal (`1e-310`) still sets `ERANGE` in this host's glibc, while the
/// gdtoa implementation Minix uses is not in this repository
/// (`minix3/lib/libc/arch/x86_64/gdtoa/` has headers only), so this rule is decided
/// purely by "the result is 0" — registered as `SEQ-SUBNORMAL-ERANGE`; (2) `inf`/`nan`
/// run all the way through in C (`seq inf` never finishes because `first <= last` always
/// holds, so the reference is killed by `timeout`; `seq nan` prints nothing and exits 0),
/// while this integer engine blocks non-finite values before rendering — registered as
/// `SEQ-NONFINITE-OPERAND`.
pub fn e_atof(word: &[u8]) -> Result<f64, SeqError> {
    // The empty string is a legal 0: when `strtod` converts no character it leaves
    // `endptr` at the **start**, and the start is that `\0`, so `*endp != '\0'` passes
    // (the reference prints `1\n0\n` for `seq ""`). Only a non-empty word with no
    // convertible prefix (`-Z`, `3x`) lands on `errx`.
    if word.is_empty() {
        return Ok(0.0);
    }
    // The number grammar is over ASCII: a byte that belongs to no number already lands on
    // the "tail has leftover characters" branch, so only `parse_float_prefix` needs `&str`,
    // and the error it returns carries the **original bytes**.
    let text = String::from_utf8_lossy(word);
    match crate::floatfmt::parse_float_prefix(&text) {
        Some((value, consumed)) if consumed == text.len() => {
            if value.is_infinite() && !word.iter().any(|&b| b == b'i' || b == b'I') {
                return Err(SeqError::OutOfRange(word.to_vec()));
            }
            if value == 0.0 && word.iter().any(|&b| b"123456789".contains(&b)) {
                return Err(SeqError::OutOfRange(word.to_vec()));
            }
            if value == -0.0 {
                Ok(0.0)
            } else {
                Ok(value)
            }
        }
        _ => Err(SeqError::InvalidNumber(word.to_vec())),
    }
}

/// Parses the operand words (`last` | `first last` | `first incr last`)
/// and the flag words. The front door is [`parse_seq_args`]; this branch only does what C
/// does after the ring: operands are parsed in the order `last`→`first`→`incr` (so the
/// error names the last operand first), the zero-stride and wrong-direction checks, and
/// `-f`'s two `valid_format` passes.
pub fn parse(args: &[&[u8]]) -> Result<SeqOptions, SeqError> {
    let parsed = parse_seq_args(args)?;
    let operands = &parsed.operands;
    let last = e_atof(operands[operands.len() - 1])?;
    let first = if operands.len() > 1 {
        e_atof(operands[0])?
    } else {
        1.0
    };
    let incr = if operands.len() > 2 {
        let incr = e_atof(operands[1])?;
        if incr == 0.0 {
            return Err(if first < last {
                SeqError::ZeroIncrement
            } else {
                SeqError::ZeroDecrement
            });
        }
        Some(incr)
    } else {
        None
    };
    if let Some(fmt) = parsed.fmt {
        if !valid_format(fmt) {
            return Err(SeqError::BadFormat(fmt.to_vec()));
        }
        let unescaped = unescape(fmt);
        if !valid_format(&unescaped) {
            return Err(SeqError::BadFormatPlain);
        }
        // Once the specifier itself is legal, rendering a float with `%g`/`%.2f` and the
        // like is a separate matter: the same adjudication as the printf family, and this
        // engine stays integer-only.
        return Err(SeqError::FloatNotModelled);
    }
    // `f64::fract` is not in core, so integrality is decided by truncating back;
    // `is_finite` and the bound keep out-of-range values out first.
    let whole =
        |v: f64| v.is_finite() && v.abs() <= (i64::MAX as f64) && (v - v as i64 as f64) == 0.0;
    if !whole(first) || !whole(last) || incr.is_some_and(|i| !whole(i)) {
        return Err(SeqError::FloatNotModelled);
    }
    let (first, last) = (first as i64, last as i64);
    let incr = incr.map(|i| i as i64);
    if incr == Some(0) {
        return Err(if first < last {
            SeqError::ZeroIncrement
        } else {
            SeqError::ZeroDecrement
        });
    }
    Ok(SeqOptions {
        first,
        incr,
        last,
        equalize: parsed.equalize >= 1,
        pad: if parsed.equalize >= 2 { ' ' } else { '0' },
        separator: parsed.separator,
        terminator: parsed.terminator,
    })
}

/// The rendered width of one integer (sign takes a slot).
fn width_of(value: i64) -> usize {
    let mut digits = value.unsigned_abs().to_string().len();
    if value < 0 {
        digits += 1;
    }
    digits
}

/// The floor division of `generate_format` (`floor((last - first) / incr)` at
/// `seq.c:440-442`). Rust's `/` truncates, so `-2.67` would give `-2` while C wants `-3`;
/// hence step down one when the remainder is non-zero and the two operands have opposite
/// signs.
fn floor_div(a: i128, b: i128) -> i128 {
    let q = a / b;
    if a % b != 0 && (a < 0) != (b < 0) {
        q - 1
    } else {
        q
    }
}

/// **The last number actually printed**: C folds it back before computing the width
/// (`if (first > last) last = first - incr * floor((first - last) / incr); else last =
/// first + incr * floor((last - first) / incr);`, `seq.c:439-442`).
/// The reference prints `1` and `6`, two one-digit numbers, for `seq -w 1 5 10`, so the
/// width is 1, not the 2 of `10`.
fn printed_last(first: i64, incr: i64, last: i64) -> i64 {
    let (first, incr) = (i128::from(first), i128::from(incr));
    let gap = i128::from(last) - first;
    let value = if gap < 0 {
        first - incr * floor_div(-gap, incr)
    } else {
        first + incr * floor_div(gap, incr)
    };
    value as i64
}

/// Renders one value, filled to `width` when equalising.
///
/// The two padding levels are asymmetric because C merely puts `pad` into printf's format
/// string (`sprintf(buf, "%%%c%d%c", pad, MAX(width1, width2), 'g')`, `seq.c:467-469`): the
/// `0` level is `%0<width>g` with zeros after the sign; the space level is `% <width>g`,
/// where **even a non-negative number occupies a sign slot**, and spaces are added only
/// when the width is still short, so `seq -w -w 8 10` prints the `10` as the three-column
/// `" 10"` (over-wide input is not truncated), while `seq -w -w -10 10` prints `0` as
/// `"  0"` and `-9` as `" -9"`.
fn render(value: i64, width: usize, equalize: bool, pad: char) -> String {
    let magnitude = value.unsigned_abs().to_string();
    if !equalize {
        if value < 0 {
            return alloc::format!("-{magnitude}");
        }
        return magnitude;
    }
    let sign = if value < 0 {
        "-"
    } else if pad == ' ' {
        " "
    } else {
        ""
    };
    let core = magnitude.len() + sign.len();
    let filler = alloc::string::String::from(pad).repeat(width.saturating_sub(core));
    if pad == ' ' {
        alloc::format!("{filler}{sign}{magnitude}")
    } else {
        alloc::format!("{sign}{filler}{magnitude}")
    }
}

/// Generates the sequence.
///
/// The C prints with a `for` whose condition runs before the first
/// value (seq.c:175-182), so `seq 5 1` with an explicit positive
/// increment prints nothing, while an omitted increment follows the
/// direction of `first` versus `last` (seq.c:151-153) and a zero
/// increment is an error (seq.c:155-159).
pub fn generate(options: &SeqOptions) -> Result<Vec<String>, SeqError> {
    let incr = match options.incr {
        Some(incr) => incr,
        None => {
            if options.first <= options.last {
                1
            } else {
                -1
            }
        }
    };
    if incr == 0 {
        return Err(SeqError::ZeroIncrement);
    }
    // Wrong-direction strides are named errors (seq.c:155-159).
    if incr < 0 && options.first < options.last {
        return Err(SeqError::NeedsPositiveIncrement);
    }
    if incr > 0 && options.first > options.last {
        return Err(SeqError::NeedsNegativeDecrement);
    }
    let width =
        width_of(options.first).max(width_of(printed_last(options.first, incr, options.last)));
    let mut values: Vec<String> = Vec::new();
    let mut value = options.first;
    let forward = incr > 0;
    while (forward && value <= options.last) || (!forward && value >= options.last) {
        values.push(render(value, width, options.equalize, options.pad));
        value = value.wrapping_add(incr);
    }
    Ok(values)
}

/// Joins the generated numbers with the separator and appends the
/// terminator, returning the full output text.
pub fn render_output(values: &[String], options: &SeqOptions) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(&options.separator);
        }
        out.extend_from_slice(value.as_bytes());
    }
    if !values.is_empty() {
        out.extend_from_slice(&options.separator);
    }
    if let Some(terminator) = &options.terminator {
        out.extend_from_slice(terminator);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hop from the text fixture to the byte front door: a `str` literal's bytes are
    /// exactly the bytes on the command line.
    fn argv_of<'w>(list: &[&'w str]) -> Vec<&'w [u8]> {
        list.iter().map(|word| word.as_bytes()).collect()
    }

    fn seq_args<'w>(list: &[&'w str]) -> Result<SeqArgs<'w>, SeqError> {
        parse_seq_args(&argv_of(list))
    }

    fn opts_of(list: &[&str]) -> Result<SeqOptions, SeqError> {
        parse(&argv_of(list))
    }

    fn generate_words(args: &[&str]) -> Result<String, SeqError> {
        let options = opts_of(args)?;
        let values = generate(&options)?;
        Ok(String::from_utf8_lossy(&render_output(&values, &options)).into_owned())
    }

    #[test]
    fn test_default_first_and_increment() {
        assert_eq!(generate_words(&["3"]), Ok("1\n2\n3\n".to_string()));
    }

    /// An out-of-set letter returns **every byte** after the dash, and this branch's ring
    /// accumulates: `-\u{e9}` (`c3 a9`) returns **two** reports (U+00C3, U+00A9), the same
    /// shape as the reference's measured two lines `303`/`251` (`getopt.c:79`'s
    /// `optopt = *place++` consumes one byte at a time, and `errflg` accumulation in
    /// `seq.c:101-113` does not stop the ring). The ring side is already
    /// `token = args[i]` (a byte slice) + `char::from(letter)`, so this test pins the fetch
    /// rule **as it already is**; the per-byte face door is `tests/seq_face.rs`.
    #[test]
    fn the_unknown_letters_are_the_first_bytes_not_the_first_characters() {
        use crate::GetoptReport;
        assert_eq!(
            seq_args(&["-\u{e9}"]).err(),
            Some(SeqError::Usage {
                reports: vec![
                    GetoptReport::UnknownOption('\u{00c3}'),
                    GetoptReport::UnknownOption('\u{00a9}')
                ]
            }),
            "`é` 是两个字节，libc 两行各报一枚"
        );
        // The same holds with the bad letter first: `-Zé` yields three lines (`Z`, `c3`,
        // `a9`).
        assert_eq!(
            seq_args(&["-Z\u{e9}"]).err(),
            Some(SeqError::Usage {
                reports: vec![
                    GetoptReport::UnknownOption('Z'),
                    GetoptReport::UnknownOption('\u{00c3}'),
                    GetoptReport::UnknownOption('\u{00a9}')
                ]
            }),
            "-Z\u{e9}"
        );
        // The ASCII shape must not be affected.
        assert_eq!(
            seq_args(&["-v"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::UnknownOption('v')]
            })
        );
        // The reverse: a value-taking letter swallows the following bytes as its value
        // (`-s`'s value may be any string).
        assert_eq!(
            seq_args(&["-s\u{e9}", "3"]).err(),
            None,
            "`-sé 3` 里 é 是分隔符的值，不该叫环报错"
        );
    }

    #[test]
    fn test_ring_keeps_going_after_a_bad_letter() {
        use crate::GetoptReport;
        // A single out-of-set letter: one libc line + the caller's usage line
        assert_eq!(
            seq_args(&["-Z"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::UnknownOption('Z')]
            })
        );
        // Legal letters in a cluster are consumed first and a bad one is still reported
        // (`seq -wZ` and `-Zw` share this shape)
        assert_eq!(
            seq_args(&["-wZ"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::UnknownOption('Z')]
            })
        );
        // errflg accumulates, so the ring does not stop at the first bad letter:
        // `-+- 3` keeps both lines
        assert_eq!(
            seq_args(&["-+-", "3"]).err(),
            Some(SeqError::Usage {
                reports: vec![
                    GetoptReport::UnknownOption('+'),
                    GetoptReport::UnknownOption('-')
                ]
            })
        );
    }

    #[test]
    fn test_missing_value_reaches_only_the_three_letters() {
        use crate::GetoptReport;
        for letter in ['f', 's', 't'] {
            let flag = format!("-{letter}");
            assert_eq!(
                parse_seq_args(std::slice::from_ref(&flag.as_bytes())).err(),
                Some(SeqError::Usage {
                    reports: vec![GetoptReport::MissingValue(letter)]
                }),
                "{flag}"
            );
        }
        // `-w` is consumed first inside the cluster, so the missing value reports `f`
        // (`seq -wf` measures the same way)
        assert_eq!(
            seq_args(&["-wf"]).err(),
            Some(SeqError::Usage {
                reports: vec![GetoptReport::MissingValue('f')]
            })
        );
        // `w` takes no value, so it can never trigger this branch
        assert_eq!(seq_args(&["-w", "3"]).err(), None);
    }

    #[test]
    fn test_h_is_in_the_set_and_says_nothing() {
        // `h` is in the optstring ⇒ libc prints nothing; C's `case 'h'` has no break and
        // falls straight into `default: errflg++`, so this branch has only the usage line
        // and empty `reports`.
        assert_eq!(
            seq_args(&["-h"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            seq_args(&["-h", "3"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            seq_args(&["-hw", "3"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
    }

    #[test]
    fn test_operand_count_face_and_double_dash() {
        // Operands must be 1..3; `--` is consumed and is not an operand itself
        // (`getopt.c:81-85`)
        assert_eq!(
            seq_args(&[]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            seq_args(&["-w"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(
            seq_args(&["1", "2", "3", "4"]).err(),
            Some(SeqError::Usage { reports: vec![] })
        );
        assert_eq!(seq_args(&["--", "3"]).unwrap().operands, argv_of(&["3"]));
        // A bare `-` ends the ring without advancing, so it is an operand itself
        // (`getopt.c:87-92`)
        assert_eq!(seq_args(&["-"]).unwrap().operands, argv_of(&["-"]));
    }

    #[test]
    fn test_numeric_guard_is_not_number_parsing() {
        // The guard decides "would this token be mistaken for an option"; after `e`/`E` a
        // sign must follow immediately for the word to count as numeric
        assert!(numeric("-5".as_bytes()));
        assert!(numeric("-1e-2".as_bytes()));
        // `e` followed directly by a digit (no sign) ⇒ the guard does **not** treat it as
        // numeric; but the reference still prints 1..100 for `seq 1e2` — a token that does
        // not start with `-` is ended by libc itself, and the guard only acts on the
        // dash-prefixed shape
        assert!(!numeric("1e2".as_bytes()));
        assert!(
            !numeric("-5e310".as_bytes()),
            "e 后无符号 ⇒ 不是 numeric ⇒ 进环被拆成集外字母"
        );
        assert!(!numeric("-w".as_bytes()));
        // Empty token: C's `while (*s)` never loops and the criterion is `*s == '\0'` ⇒
        // it counts as numeric
        assert!(numeric("".as_bytes()));
        // `-5` is therefore an operand, not an option
        assert_eq!(seq_args(&["-5"]).unwrap().operands, argv_of(&["-5"]));
    }

    #[test]
    fn test_unescape_matches_the_c_table() {
        assert_eq!(unescape("a\\tb".as_bytes()), "a\tb".as_bytes());
        assert_eq!(unescape("\\101".as_bytes()), "A".as_bytes()); // at most three octal digits
        assert_eq!(unescape("\\x41z".as_bytes()), "Az".as_bytes()); // at most two hex digits
        assert_eq!(
            unescape("\\X41".as_bytes()),
            "\\X41".as_bytes(),
            "引导字母只有小写 x（C 没有 case 'X'）"
        );
        assert_eq!(
            unescape("\\q".as_bytes()),
            "\\q".as_bytes(),
            "认不出的转义把反斜杠原样留着"
        );
        assert_eq!(
            unescape("x\\".as_bytes()),
            "x".as_bytes(),
            "串尾单个反斜杠整体丢掉"
        );
        assert_eq!(unescape("\\\\".as_bytes()), "\\".as_bytes());
    }

    #[test]
    fn test_valid_format_only_counts_float_conversions() {
        assert!(valid_format("%g".as_bytes()));
        assert!(valid_format("%10g".as_bytes()));
        assert!(valid_format("%-.3f".as_bytes()));
        assert!(
            valid_format("abc".as_bytes()),
            "零个转换也算合法（参照件 `-f abc` 照样打字面）"
        );
        assert!(valid_format("%%".as_bytes()));
        assert!(
            !valid_format("%d".as_bytes()),
            "整数说明符在 C 眼里就是非法"
        );
        assert!(
            !valid_format("%g%g".as_bytes()),
            "两个转换超过 `conversions <= 1`"
        );
        assert!(!valid_format("%q".as_bytes()));
        assert!(!valid_format("%".as_bytes()));
    }

    #[test]
    fn test_zero_stride_and_direction_sentences_split() {
        // Three operands are first, incr, last; the zero-stride branch's name follows
        // `first < last`
        assert_eq!(
            opts_of(&["1", "0", "3"]).err(),
            Some(SeqError::ZeroIncrement)
        );
        assert_eq!(
            opts_of(&["5", "0", "1"]).err(),
            Some(SeqError::ZeroDecrement)
        );
        assert_eq!(
            opts_of(&["2", "0", "2"]).err(),
            Some(SeqError::ZeroDecrement),
            "相等走 de 支"
        );
        // The two wrong-direction checks come after the zero-stride one (`seq.c:155-159`)
        // and live on `generate`'s side, after the default stride is chosen, so these two
        // go through `generate_words` rather than `parse`
        assert_eq!(
            generate_words(&["1", "-1", "5"]),
            Err(SeqError::NeedsPositiveIncrement)
        );
        assert_eq!(
            generate_words(&["5", "1", "1"]),
            Err(SeqError::NeedsNegativeDecrement)
        );
    }

    #[test]
    fn test_operand_errors_name_the_last_operand_first() {
        // C parses in the order last→first→incr, so `seq 3 -w` reports `-w` (the ring does
        // not reorder, and `-w` lands among the operands). The exit code is supplied by the
        // shell along the `errx(2, ...)` family.
        assert_eq!(
            opts_of(&["3", "-w"]).err(),
            Some(SeqError::InvalidNumber("-w".as_bytes().to_vec()))
        );
        assert_eq!(
            opts_of(&["3x"]).err(),
            Some(SeqError::InvalidNumber("3x".as_bytes().to_vec()))
        );
        assert_eq!(
            opts_of(&["1e310"]).err(),
            Some(SeqError::OutOfRange("1e310".as_bytes().to_vec()))
        );
        assert_eq!(
            opts_of(&["1e-40000"]).err(),
            Some(SeqError::OutOfRange("1e-40000".as_bytes().to_vec())),
            "下溢到 0 与上溢同一条 err(2, ...) 出口"
        );
        assert_eq!(
            opts_of(&["--", "-Z"]).err(),
            Some(SeqError::InvalidNumber("-Z".as_bytes().to_vec()))
        );
        // The empty string is a legal 0: when `strtod` converts nothing it leaves endptr at
        // the start, and the start is that `\0` ⇒ the "tail has leftover characters"
        // criterion passes
        assert_eq!(e_atof("".as_bytes()), Ok(0.0));
        assert_eq!(
            e_atof("-Z".as_bytes()).err(),
            Some(SeqError::InvalidNumber("-Z".as_bytes().to_vec()))
        );
    }

    #[test]
    fn test_equalize_width_follows_the_last_printed_value() {
        // C's floor and Rust's truncating division differ by one when the signs differ
        assert_eq!(floor_div(8, -3), -3);
        assert_eq!(floor_div(9, -3), -3);
        assert_eq!(floor_div(8, 3), 2);
        assert_eq!(floor_div(-8, 3), -3);
        // The width comes from the **actually printed** first and last values:
        // `seq -w 1 5 10` prints only 1 and 6
        assert_eq!(printed_last(1, 5, 10), 6);
        assert_eq!(printed_last(10, -3, 1), 1);
        assert_eq!(printed_last(1, 1, 10), 10);
        assert_eq!(width_of(printed_last(1, 5, 10)), 1);
        assert_eq!(width_of(printed_last(1, 1, 10)), 2);
        // `% 2g` makes even non-negative numbers occupy a sign slot and does not
        // truncate over-wide values; `%02g` puts the zeros after the sign
        assert_eq!(render(10, 2, true, ' '), String::from(" 10"));
        assert_eq!(render(10, 2, true, '0'), String::from("10"));
        assert_eq!(render(0, 3, true, ' '), String::from("  0"));
        assert_eq!(render(-9, 3, true, ' '), String::from(" -9"));
        assert_eq!(render(-5, 2, true, '0'), String::from("-5"));
    }

    #[test]
    fn test_format_rejected_before_the_float_gap() {
        assert_eq!(
            opts_of(&["-f", "%q", "3"]).err(),
            Some(SeqError::BadFormat("%q".as_bytes().to_vec()))
        );
        assert_eq!(
            opts_of(&["-f", "%d", "3"]).err(),
            Some(SeqError::BadFormat("%d".as_bytes().to_vec())),
            "参照件对 `%d' 报的是同一句，不是「浮点未接上」"
        );
        // Only after the specifier is legal does the float-rendering adjudication apply
        // (this engine is integer-only, registered as SEQ-FORMAT-RENDER)
        assert_eq!(
            opts_of(&["-f", "%g", "3"]).err(),
            Some(SeqError::FloatNotModelled)
        );
        assert_eq!(
            opts_of(&["-f", "abc", "3"]).err(),
            Some(SeqError::FloatNotModelled)
        );
    }

    #[test]
    fn test_equalize_has_two_levels() {
        let one = opts_of(&["-w", "1", "10"]).expect("一级等宽");
        assert!(one.equalize);
        assert_eq!(one.pad, '0');
        let two = opts_of(&["-w", "-w", "1", "10"]).expect("二级等宽");
        assert_eq!(two.pad, ' ');
        assert_eq!(
            opts_of(&["-w", "-w", "-w", "1", "10"])
                .expect("三级仍是空格")
                .pad,
            ' '
        );
        // When `-f` is given, `-w` is void entirely (the `if (!fmt)` layer), but C still
        // exits early on the float branch
        assert_eq!(
            opts_of(&["-f", "%g", "-w", "1", "3"]).err(),
            Some(SeqError::FloatNotModelled)
        );
    }

    #[test]
    fn test_first_last_and_stride() {
        assert_eq!(
            generate_words(&["2", "2", "8"]),
            Ok("2\n4\n6\n8\n".to_string())
        );
        // Two operands: the increment follows the direction
        // (seq.c:151-153), so `seq 5 0` descends.
        assert_eq!(
            generate_words(&["5", "0"]),
            Ok("5\n4\n3\n2\n1\n0\n".to_string())
        );
    }

    #[test]
    fn test_direction_errors() {
        // seq.c:155-159 — wrong-direction increments are named errors.
        assert_eq!(
            generate_words(&["5", "1", "0"]),
            Err(SeqError::NeedsNegativeDecrement)
        );
        assert_eq!(
            generate_words(&["1", "-1", "5"]),
            Err(SeqError::NeedsPositiveIncrement)
        );
        // An omitted increment takes the direction instead: `seq 5 1`
        // descends.
        assert_eq!(
            generate_words(&["5", "1"]),
            Ok("5\n4\n3\n2\n1\n".to_string())
        );
        // The C errors on a wrong-direction stride before printing, so
        // the for-condition's empty case never reaches output; assert
        // the error instead.
        assert_eq!(
            generate_words(&["1", "-1", "5"]),
            Err(SeqError::NeedsPositiveIncrement)
        );
    }

    #[test]
    fn test_separator_flag() {
        // The C prints the separator after every number, the last one
        // included (seq.c:175-182).
        assert_eq!(generate_words(&["-s", " ", "3"]), Ok("1 2 3 ".to_string()));
    }

    #[test]
    fn test_equalize_pads_to_common_width() {
        assert_eq!(
            generate_words(&["-w", "8", "10"]),
            Ok("08\n09\n10\n".to_string())
        );
        assert_eq!(
            generate_words(&["-w", "-2", "2"]),
            Ok("-2\n-1\n00\n01\n02\n".to_string())
        );
    }

    #[test]
    fn test_terminator_flag() {
        assert_eq!(
            generate_words(&["-t", "END", "-s", " ", "2"]),
            Ok("1 2 END".to_string())
        );
    }

    #[test]
    fn test_zero_increment_rejected() {
        assert_eq!(
            generate_words(&["1", "0", "5"]),
            Err(SeqError::ZeroIncrement)
        );
    }

    #[test]
    fn test_non_integer_operand_rejected() {
        assert!(matches!(
            generate_words(&["abc"]),
            Err(SeqError::InvalidNumber(_))
        ));
    }

    #[test]
    fn test_float_format_declared_unsupported() {
        assert_eq!(
            generate_words(&["-f", "%g", "3"]),
            Err(SeqError::FloatNotModelled)
        );
    }
    /// The `-s` and `-t` values **are exactly what gets printed**: a bare `ff` is one
    /// separator byte, not the three bytes of its folded form. The reference measured
    /// `seq -s<ff> 1 3` printing `1\xff2\xff3\xff` and `seq -t<ff> 1 3` printing
    /// `1\n2\n3\n\xff` (`seq.c:110`'s `sep = unescape(optarg)` together with `:174-186`'s
    /// `fputs(sep, stdout)`); the text seam prints `ef bf bd` (three bytes) on the same
    /// shape.
    #[test]
    fn a_raw_ff_separator_and_terminator_are_one_byte_each() {
        let parsed = parse_seq_args(&[b"-s\xff".as_slice(), b"1".as_slice(), b"3".as_slice()])
            .expect("字节形 -s 的值");
        assert_eq!(parsed.separator, vec![0xff]);
        let parsed = parse_seq_args(&[b"-t\xff".as_slice(), b"1".as_slice(), b"3".as_slice()])
            .expect("字节形 -t 的值");
        assert_eq!(parsed.terminator, Some(vec![0xff]));
        let options = parse(&[b"-s\xff".as_slice(), b"1".as_slice(), b"3".as_slice()]).unwrap();
        let values = generate(&options).unwrap();
        assert_eq!(
            render_output(&values, &options),
            b"1\xff2\xff3\xff".as_slice()
        );
    }

    /// An out-of-set letter returns **that one byte** from the command line: `-\xff` is one
    /// letter (`getopt.c:79`'s `optopt = *place++` consumes one byte at a time) and the
    /// message carries one bare `ff` (`:103`'s `%c`). The accumulating ring makes only one
    /// pass on this shape — the folded shape turns the same argument into three letters with
    /// one line each, which is the seam's shape, not C's, so the difference between the two
    /// shapes is pinned directly in the assertion.
    #[test]
    fn parse_seq_names_a_raw_ff_option_letter_by_that_byte() {
        use crate::GetoptReport;
        let error = parse_seq_args(&[b"-\xff".as_slice(), b"3".as_slice()]).err();
        assert_eq!(
            error,
            Some(SeqError::Usage {
                reports: vec![GetoptReport::UnknownOption('\u{00ff}')]
            }),
            "一枚原字节只该换一个字母"
        );
        let SeqError::Usage { reports } = error.expect("上面的 err 不该为空") else {
            panic!("集外字母走 Usage 那一支");
        };
        assert_eq!(
            reports[0].line_bytes("seq"),
            b"seq: unknown option -- \xff\n".as_slice()
        );
    }
}
