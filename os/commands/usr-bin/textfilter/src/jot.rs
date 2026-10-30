//! jot — print sequential or random data (minix3/usr.bin/jot/jot.c).
//!
//! Deciding half: option derivation (the right-to-left positional
//! arguments), format selection and value rendering. The RNG is
//! injected so `-r` is testable without libc's random(3); the exact
//! NetBSD random() bit stream is not reproduced (registered as a
//! remaining corner in `rewrite-notes/18-stage-commands/07-text-filter.md`).
use crate::floatfmt::{decimal_exponent, floor, pow10};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// NetBSD jot's default repetitions.
pub const REPS_DEF: i64 = 100;
pub const BEGIN_DEF: f64 = 1.0;
pub const ENDER_DEF: f64 = 100.0;
pub const STEP_DEF: f64 = 1.0;

/// Which positional operands were given (`-` means defaulted).
pub const HAVE_BEGIN: u8 = 1;
pub const HAVE_STEP: u8 = 2;
pub const HAVE_REPS: u8 = 4;
pub const HAVE_ENDER: u8 = 8;

/// Errors mirroring the C `errx` exits.
#[derive(Debug, PartialEq)]
pub enum JotError {
    BadPrecision,
    /// `jot.c:215-216`: `Bad reps value:  %s` (two spaces); the value is the
    /// **raw bytes** of the reps operand.
    BadReps(Vec<u8>),
    /// `jot.c:189-190`: `Bad step value:  %s` (two spaces), also raw bytes.
    BadStep(Vec<u8>),
    /// `jot.c:385`, `:396`: `unknown or invalid format \`%s'` carries the
    /// **whole format word** — including the extra percent sign appended by
    /// `strcat(format, "%")` at `:360`, so what is echoed is the `format` as
    /// validated, not the original substring the user typed.
    InvalidFormat(Vec<u8>),
    /// `jot.c:376-377`: `cannot convert numeric data to strings` is a fixed
    /// sentence; C carries no value on this arm.
    FormatWantsString,
    ImpossibleStepsize,
    MustSpecifyBegin,
    InfiniteUnbounded,
    /// `jot.c:223-225`: `Too many arguments.  What do you mean by %s?` reports
    /// the **fifth** operand (`argv[4]`), raw bytes.
    TooManyArgs(Vec<u8>),
}

/// The parsed conversion of a `-w` word (or the synthesized default).
#[derive(Debug, PartialEq)]
pub enum Format {
    /// `-b word`: the word itself, repeated verbatim.
    Literal(Vec<u8>),
    /// One conversion with the surrounding word text.
    Conv(Conv),
}

/// A single printf conversion plus its literal context.
#[derive(Debug, PartialEq)]
pub struct Conv {
    /// Text before the conversion (the whole word when it had no `%`).
    pub prefix: Vec<u8>,
    /// Text after the conversion.
    pub suffix: Vec<u8>,
    /// printf flags seen (`-0+ #` subset).
    pub left: bool,
    pub zero: bool,
    pub plus: bool,
    pub space: bool,
    pub hash: bool,
    pub width: usize,
    /// `None` means no `.precision` was spelled.
    pub prec: Option<usize>,
    pub conv: ConvKind,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ConvKind {
    /// Integer conversions set `dox` in the C (value printed via floor).
    Int(IntKind),
    Real(RealKind),
    /// `%%` — a literal percent.
    Percent,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum IntKind {
    D,
    I,
    O,
    X,
    U,
    C,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RealKind {
    F,
    E,
    G,
    BigE,
    BigG,
}

/// `is dox` in the C: integer rendering uses `(long)floor(x)`.
pub fn is_dox(fmt: &Format) -> bool {
    match fmt {
        Format::Literal(_) => false,
        Format::Conv(c) => matches!(c.conv, ConvKind::Int(_)),
    }
}

/// Options after `getargs` (before the derivation switch).
#[derive(Debug, Clone)]
pub struct JotOptions {
    pub reps: i64,
    pub begin: f64,
    pub ender: f64,
    pub step: f64,
    /// Explicit `-p` precision; `None` until inferred from the operands.
    pub prec: Option<usize>,
    /// The `-s` value: the **raw bytes** printed between items (`jot.c:169-170`
    /// stores `sepstring`, `:310` writes it out verbatim with
    /// `fputs(sepstring, stdout)`). Defaults to a single newline.
    pub sep: Vec<u8>,
    /// The one slot shared by `-b`/`-w` (`jot.c:173-176` fallthrough): raw bytes.
    pub word: Vec<u8>,
    pub boring: bool,
    pub chardata: bool,
    pub nofinalnl: bool,
    pub randomize: bool,
    /// Bits from HAVE_* recording which operands were spelled.
    pub have: u8,
}

impl Default for JotOptions {
    fn default() -> Self {
        JotOptions {
            reps: REPS_DEF,
            begin: BEGIN_DEF,
            ender: ENDER_DEF,
            step: STEP_DEF,
            prec: None,
            sep: b"\n".to_vec(),
            word: Vec::new(),
            boring: false,
            chardata: false,
            nofinalnl: false,
            randomize: false,
            have: 0,
        }
    }
}

pub use crate::floatfmt::{parse_float_prefix, parse_float_prefix_bytes};

/// `getprec`: digits after the first dot.
pub fn get_prec(num_str: &[u8]) -> usize {
    match num_str.iter().position(|&byte| byte == b'.') {
        None => 0,
        Some(dot) => num_str[dot + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count(),
    }
}

/// Applies a non-numeric operand the way the C does: it falls back to
/// the last **byte** (`begin = argv[i][strlen(argv[i]) - 1]`, `jot.c:196`/`:204`).
/// Returning `None` means that cell keeps its default — only an empty operand
/// takes this arm, for the reason given in the comment inside the function.
pub fn operand_to_number(s: &[u8]) -> Option<f64> {
    match parse_float_prefix_bytes(s) {
        Some((v, _)) => Some(v),
        // `jot.c:196`/`:204` takes `argv[i][strlen(argv[i]) - 1]` — the last
        // **byte**, not the last character: against the reference binary,
        // `-c 3 1 <ff>` measures the third value as that `ff` itself, while
        // `-c 3 1 ab` takes `b`. For an empty operand C computes index `-1`,
        // an out-of-bounds read whose value depends on whatever precedes that
        // memory; this layer does not copy undefined behavior and instead
        // keeps the default, matching the host reference binary's measurement
        // (`jot 3 '' 5` returns `1 3 5`, i.e. that cell keeps its default).
        None if s.is_empty() => None,
        // That byte lands through a **signed char**: the assignment target at
        // `jot.c:196`/`:204` is `char`, and x86 `char` is signed, so `\xff` is
        // -1 rather than 255. Three measured shapes: `jot 3 \xff 5` →
        // `-1 2 5` (begin -1, step (5-(-1))/2 = 3), `jot 3 1 \xff` →
        // `1 0 -1`, and `jot -c 3 1 \xff` → bytes `01 00 ff`. Reading it as
        // unsigned would make ender 255 and turn the middle cell from 0 into
        // 128 (one byte off from the reference binary's measurement).
        None => Some(f64::from(*s.last().expect("上面已排除空串") as i8)),
    }
}

/// Exits from the option ring: two kinds printed by libc `getopt`, one printed
/// by `jot.c`'s own `errx`.
#[derive(Debug, PartialEq)]
pub enum JotArgError {
    /// An unknown letter or a value-taking letter without its value. libc
    /// prints one line; `jot.c:151-152`'s `default: usage()` follows with one
    /// more line.
    Getopt(crate::GetoptReport),
    /// `jot.c:161-165`: a single `errx(EXIT_FAILURE, "Bad precision value")`
    /// — only that line, no usage afterwards.
    BadPrecision,
}

/// The option ring's product: settled flags plus the operands **in original
/// order**.
pub struct JotArgs<'a> {
    pub options: JotOptions,
    pub operands: Vec<&'a [u8]>,
}

/// The option ring of the `getopt(argc, argv, "b:cnp:rs:w:")` call at
/// `jot.c:150`.
///
/// In the optstring, `b`, `p`, `s`, `w` take values while `c`, `n`, `r` do
/// not, so the reachable exits form four classes rather than one: (1) an
/// unknown letter, (2) a value-taking letter without a value (each first gets
/// a libc line, then C adds the usage line), (3) a `-p` value that fails the
/// `strtol` test (`errx`, one line of its own, no usage), (4) the ring ending
/// normally. `-b` and `-w` land in the **same** `format` slot
/// (`jot.c:172-176`'s `case 'b': boring = 1; /* FALLTHROUGH */ case 'w':`),
/// so the later word overrides the earlier one, while `boring` stays set once
/// it is raised.
///
/// The ring does not permute: as soon as the first non-option token arrives,
/// `getopt.c:74-78` returns -1 on the spot without advancing `optind`, so that
/// token is itself the first operand (measured against the reference binary:
/// in `jot 3 -Z`, `-Z` is begin rather than an option). A bare `-` likewise
/// stays among the operands, and `jot.c:66`'s `is_default(s)` reads it as
/// "use the default for this cell", unrelated to the "`-` is standard input"
/// reading of sibling commands.
pub fn parse_jot_args<'a>(args: &'a [&'a [u8]]) -> Result<JotArgs<'a>, JotArgError> {
    let mut options = JotOptions::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        let bytes = arg;
        if arg == b"--" {
            // `getopt.c:81-85`: -1 is returned only after `++optind`, so `--`
            // itself is not an operand.
            break args[index + 1..].to_vec();
        }
        if bytes.len() < 2 || bytes[0] != b'-' {
            // `getopt.c:74-78`: a non-option token ends the ring without
            // advancing `optind`.
            break args[index..].to_vec();
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'c' => options.chardata = true,
                b'n' => options.nofinalnl = true,
                b'r' => options.randomize = true,
                ch @ (b'b' | b'p' | b's' | b'w') => {
                    // The value is "the rest of this token after the dash and
                    // the letter", or the next token when nothing is left
                    // (`getopt.c:118-141`); both forms consume this token to
                    // the end.
                    let value = if pos + 1 < bytes.len() {
                        &bytes[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(JotArgError::Getopt(
                                    crate::GetoptReport::MissingValue(char::from(ch)),
                                ));
                            }
                        }
                    };
                    match ch {
                        b'b' => {
                            options.boring = true;
                            options.word = value.to_vec();
                        }
                        b'w' => options.word = value.to_vec(),
                        b's' => options.sep = value.to_vec(),
                        _ => {
                            let Some(prec) = parse_precision(value) else {
                                return Err(JotArgError::BadPrecision);
                            };
                            options.prec = Some(prec);
                        }
                    }
                    pos = bytes.len();
                }
                _ => {
                    // What is reported is **the first byte at this spot**: C's
                    // `optopt` is a `char`, and `%c` prints that one byte. The
                    // cursor `pos` is a byte index, and for a non-ASCII letter
                    // it lands inside a multi-byte character, so take a byte
                    // here rather than slicing a string (slicing would panic).
                    return Err(JotArgError::Getopt(crate::GetoptReport::UnknownOption(
                        char::from(bytes[pos]),
                    )));
                }
            }
            pos += 1;
        }
        index += 1;
    };
    Ok(JotArgs { options, operands })
}

/// C's `strtol(optarg, &ep, 0)` combined with the `*ep != 0 || prec < 0`
/// check (`jot.c:161-165`). Four consequences follow from it:
/// (1) base 0 accepts leading whitespace and a `+`/`-` sign, `0x` selects hex
/// and a leading `0` octal, so `-p 0x2` is a legal 2 (measured against the
/// reference binary: prints `1.00 2.00 3.00`);
/// (2) not a single character may remain at the tail, so both `3x` and `09`
/// (octal stops after the `0`) are rejected;
/// (3) when no digit converts, `strtol` leaves `ep` at the starting point, so
/// only the empty string is legal (value 0; measured: `jot -p '' 3` prints the
/// integer form), while pure whitespace or a lone `-` are rejected;
/// (4) the value saturates at `long` first, then truncates to `int` for the
/// check exactly as C does — `99999999999999999999` truncates to `-1` and is
/// therefore rejected, `4294967298` truncates to `2` and is accepted. All three
/// target boards and the host use 64-bit `long`, so this truncation shape is
/// identical in all four.
#[must_use]
pub fn parse_precision(value: &[u8]) -> Option<usize> {
    const LONG_MAX: i128 = i64::MAX as i128;
    let bytes = value;
    let mut pos = 0usize;
    while matches!(
        bytes.get(pos),
        Some(b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
    ) {
        pos += 1;
    }
    let mut negative = false;
    match bytes.get(pos) {
        Some(b'+') => pos += 1,
        Some(b'-') => {
            negative = true;
            pos += 1;
        }
        _ => {}
    }
    let mut radix = 10u32;
    if bytes.get(pos) == Some(&b'0') {
        if matches!(bytes.get(pos + 1), Some(b'x') | Some(b'X'))
            && bytes.get(pos + 2).is_some_and(|c| c.is_ascii_hexdigit())
        {
            radix = 16;
            pos += 2;
        } else {
            radix = 8;
        }
    }
    let mut magnitude: i128 = 0;
    let mut converted = false;
    while let Some(&byte) = bytes.get(pos) {
        let Some(digit) = (byte as char).to_digit(radix) else {
            break;
        };
        magnitude = (magnitude.saturating_mul(radix as i128) + i128::from(digit)).min(LONG_MAX);
        converted = true;
        pos += 1;
    }
    if !converted {
        // `ep` stays at the starting point: `*ep` is `\0` only when the whole
        // string was empty to begin with.
        return if value.is_empty() { Some(0) } else { None };
    }
    if pos != bytes.len() {
        return None;
    }
    let long = if negative { -magnitude } else { magnitude };
    let as_int = long as i32;
    if as_int < 0 {
        return None;
    }
    Some(as_int as usize)
}

/// Parses `reps`: strtoul semantics with a full-parse check.
pub fn parse_reps(s: &[u8]) -> Result<i64, JotError> {
    // `strtoul` skips the six `isspace` bytes, not Unicode whitespace classes:
    // non-ASCII whitespace (`c2 a0`) is not whitespace in C, so "no digit
    // converted" → Bad reps. Working byte-wise makes that boundary automatic;
    // `str::trim_start` would skip `c2 a0` and wrongly accept it.
    let mut at = 0usize;
    while matches!(s.get(at), Some(b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')) {
        at += 1;
    }
    let t = &s[at..];
    let digits: Vec<u8> = t
        .iter()
        .copied()
        .take_while(|byte| byte.is_ascii_digit())
        .collect();
    if t.is_empty() {
        // `strtoul("")` converts no digit and leaves `ep` at the start, so
        // `*ep == '\0'` holds and the value is 0 **without an error** (the
        // same reasoning as the `parse_precision` case). Measured against the
        // reference binary, `jot '' 1 2` reports `Infinite sequences cannot be
        // bounded` (reps 0 means unbounded, `jot.c:272-275`), not the Bad reps
        // sentence.
        return Ok(0);
    }
    if digits.is_empty() || digits.len() != t.len() {
        return Err(JotError::BadReps(s.to_vec()));
    }
    let text = core::str::from_utf8(&digits).map_err(|_| JotError::BadReps(s.to_vec()))?;
    text.parse::<i64>()
        .map_err(|_| JotError::BadReps(s.to_vec()))
}

/// Parses `step`/seed: strtod with a full-parse check (`*ep == 0`).
pub fn parse_step(s: &[u8]) -> Result<f64, JotError> {
    match parse_float_prefix_bytes(s) {
        Some((v, used)) if used == s.len() => Ok(v),
        _ => Err(JotError::BadStep(s.to_vec())),
    }
}

/// `jot.c:118`'s `srandom((unsigned long) step)`: truncate the `double`
/// **toward zero** by C rules, then **wrap** at the negative-to-unsigned step.
/// So `-0.5` and `0` are the same seed (both truncate to 0), and `-5.9` and
/// `-5` are also the same (truncate to -5, then wrap).
///
/// C's `unsigned long` is only 32 bits on Minix's ILP32, while this layer's
/// random stream is carried by `Lcg` with a 64-bit seed; the two differ in
/// **wrapping width**, but the stream values were never libc `random()` anyway
/// (registered in `rewrite-notes/18-stage-commands/07-text-filter.md`). What is
/// observable is the truncation direction and "negatives are legal", and the
/// function is shaped by those two points.
#[must_use]
pub fn seed_from_step(step: f64) -> u64 {
    (step as i64) as u64
}

/// `getformat`: pick or validate the `-w` word and report whether the
/// conversion is integer (`dox`).
pub fn get_format(options: &JotOptions) -> Result<(Format, bool), JotError> {
    if options.boring {
        return Ok((Format::Literal(options.word.clone()), false));
    }
    let word = &options.word;
    let bytes = word.as_slice();
    // First `%` not doubled.
    let mut at: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'%' {
                i += 2;
                continue;
            }
            at = Some(i);
            break;
        }
        i += 1;
    }
    match at {
        None => {
            // C uses the **unclamped** prec here: `jot.c:229-230`'s
            // `if (prec == -1) prec = 0;` comes **after** `getformat()`, so at
            // the point where the specifier is assembled it is still -1. The
            // shell-side `None` is that -1, while `Some(0)` is a different
            // thing — it corresponds to C's `prec == 0` and takes the integer
            // arm (`%ld`, `(long)floor(x)`, `jot.c:306`).
            //
            // On the -1 arm C assembles `%.-1f` (`jot.c:356`'s `"%%.%df"`).
            // That specifier is not broken on Minix: the libc descends from
            // NetBSD, and `minix3/lib/libc/stdio/vsnprintf_ss.c:213-226`'s
            // `case '.'` leaves `n` at 0 when it meets a non-digit after the
            // `.`, so `prec = 0`, then `goto reswitch` reads the remaining `-`
            // as the left-adjust flag, `1` as the field width and `f` as the
            // conversion. That is equivalent to `%-1.0f` — **rounding**, not
            // truncation. Host glibc answers the same string with the literal
            // `%.0-1f`; that is the behavior of the libc the reference binary
            // links against, not Minix ground truth, so this shape cannot be
            // pinned against the host reference binary.
            if options.chardata || options.prec == Some(0) {
                let kind: &[u8] = if options.chardata { b"c" } else { b"ld" };
                let mut full = word.clone();
                full.push(b'%');
                full.extend_from_slice(kind);
                let conv = parse_conv_spec(&full, word.len())?;
                Ok((Format::Conv(conv), true))
            } else {
                let mut full = word.clone();
                match options.prec {
                    Some(p) => full.extend_from_slice(format!("%.{}f", p).as_bytes()),
                    None => full.extend_from_slice(b"%-1.0f"),
                };
                let conv = parse_conv_spec(&full, word.len())?;
                Ok((Format::Conv(conv), false))
            }
        }
        Some(pct) => {
            if pct + 1 == bytes.len() {
                // Cannot end in a single `%`: append another.
                let mut full = word.clone();
                full.push(b'%');
                let conv = parse_conv_spec(&full, pct)?;
                Ok((Format::Conv(conv), false))
            } else {
                let conv = parse_conv_spec(bytes, pct)?;
                let dox = matches!(conv.conv, ConvKind::Int(_));
                Ok((Format::Conv(conv), dox))
            }
        }
    }
}

/// Parses one conversion at `pct` following the C validation: allowed
/// flags `0123456789#-+. ` (no `*`), an optional `l`, then a legal
/// conversion character; anything after (outside `%%`) is an error.
fn parse_conv_spec(word: &[u8], pct: usize) -> Result<Conv, JotError> {
    let bytes = word;
    // The prefix segment needs `%%` unfolded as well: C hands the **whole**
    // `format` to `printf` (`jot.c:306`, `:308`), and libc does the unfolding
    // without distinguishing which side of the conversion it is on. Measured
    // against the reference binary, `-p 2 -w '%%x%d' 3 1 5` prints `%x1` (one
    // percent sign); the version that did not unfold here printed two extra
    // percent signs before `%x`.
    let prefix = unfold_double_percent(&word[..pct]);
    let mut i = pct + 1;
    let mut left = false;
    let mut zero = false;
    let mut plus = false;
    let mut space = false;
    let mut hash = false;
    let mut width = 0usize;
    let mut digits = String::new();
    // Flags and width are interleaved in the C's char scan; digits
    // form the width.
    while i < bytes.len() {
        match bytes[i] {
            b'-' => left = true,
            b'0' => {
                if digits.is_empty() {
                    zero = true;
                }
                digits.push('0');
            }
            b'1'..=b'9' => digits.push(bytes[i] as char),
            b'+' => plus = true,
            b' ' => space = true,
            b'#' => hash = true,
            b'.' => break,
            b'l' => break,
            _ if bytes[i].is_ascii_alphabetic() || bytes[i] == b'%' => break,
            _ => return Err(JotError::InvalidFormat(word.to_vec())),
        }
        if bytes[i] == b'.' || bytes[i] == b'l' {
            break;
        }
        i += 1;
    }
    width = digits.parse().unwrap_or(0);
    let mut prec: Option<usize> = None;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let ds = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        prec = Some(byte_digits_to_usize(&word[ds..i]));
    }
    if i < bytes.len() && bytes[i] == b'l' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(JotError::InvalidFormat(word.to_vec()));
    }
    let conv = match bytes[i] {
        b'f' => ConvKind::Real(RealKind::F),
        b'e' => ConvKind::Real(RealKind::E),
        b'g' => ConvKind::Real(RealKind::G),
        b'E' => ConvKind::Real(RealKind::BigE),
        b'G' => ConvKind::Real(RealKind::BigG),
        b'%' => ConvKind::Percent,
        b's' => {
            // The `jot.c:376-377` sentence is fixed; C carries **no value** on
            // this arm.
            return Err(JotError::FormatWantsString);
        }
        b'd' => ConvKind::Int(IntKind::D),
        b'i' => ConvKind::Int(IntKind::I),
        b'o' => ConvKind::Int(IntKind::O),
        b'x' => ConvKind::Int(IntKind::X),
        b'u' => ConvKind::Int(IntKind::U),
        b'D' => ConvKind::Int(IntKind::D),
        b'O' => ConvKind::Int(IntKind::O),
        b'X' => ConvKind::Int(IntKind::X),
        b'U' => ConvKind::Int(IntKind::U),
        b'c' => ConvKind::Int(IntKind::C),
        _ => return Err(JotError::InvalidFormat(word.to_vec())),
    };
    // Trailing stuff (outside %%) is an error.
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'%' {
            if j + 1 < bytes.len() && bytes[j + 1] == b'%' {
                j += 2;
                continue;
            }
            return Err(JotError::InvalidFormat(word.to_vec()));
        }
        j += 1;
    }
    let suffix = unfold_double_percent(&word[i + 1..]);
    Ok(Conv {
        prefix,
        suffix,
        left,
        zero,
        plus,
        space,
        hash,
        width,
        prec,
        conv,
    })
}

/// Turn a run of decimal bytes into `usize` (the segment after `.` in a `-w`
/// specifier). Anything unconvertible counts as 0, the same shape as C's
/// `atoi`-style read: libc starts eating digits after the `.` and stops at the
/// first non-digit.
#[must_use]
fn byte_digits_to_usize(bytes: &[u8]) -> usize {
    core::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0)
}

/// `printf` unfolds `%%` into a single `%` across the **whole format string**
/// (`jot.c:306`, `:308` hand the entire `format` to it; the second scan at
/// `:391-397` only validates and does not unfold). This layer stores the
/// literal segments before and after the conversion separately, so both
/// segments must be unfolded here — unfolding only the suffix would leave a
/// half-semantics where the prefix prints two percent signs while the
/// reference binary prints one.
#[must_use]
fn unfold_double_percent(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] == b'%' && bytes.get(at + 1) == Some(&b'%') {
            out.push(b'%');
            at += 2;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    out
}

/// The derivation switch at the end of `getargs` (`jot.c:247-296`).
///
/// C's `switch (have)` matches on the **exact value**, not on "contains these
/// bits": `case ENDER | STEP` (`:249`) hits only when `have` is exactly those
/// two bits, and when all four operands are given the hit is
/// `case REPS | BEGIN | ENDER | STEP` (`:281`). A subset test would misroute
/// the four-flag shape into the `:249-259` arm, and the two arms end
/// oppositely: that arm writes `reps = 0` when `step == 0.0` (**unbounded**;
/// the print loop is `for (i = 1; i <= reps || reps == 0; i++)`, `jot.c:129`),
/// while `:281-284` writes `break` (**reps keeps the given 3**). So
/// `jot 3 1 100 0` would go from three lines to never terminating, and
/// `jot 3 1 100 5` from three lines to twenty.
pub fn derive(options: &mut JotOptions) -> Result<(), JotError> {
    if options.randomize {
        // Randomize takes the operands as given (step is the seed).
        return Ok(());
    }
    let have = options.have;
    let r = HAVE_REPS;
    let b = HAVE_BEGIN;
    let e = HAVE_ENDER;
    let s = HAVE_STEP;
    // `jot.c:249-259`: only ender and step were given (begin optional); reps
    // must be derived
    if have == e | s || have == e | s | b {
        if options.step == 0.0 {
            options.reps = 0;
        } else {
            let reps = ((options.ender - options.begin + options.step) / options.step) as i64;
            if reps <= 0 {
                return Err(JotError::ImpossibleStepsize);
            }
            options.reps = reps;
        }
        return Ok(());
    }
    // `jot.c:261-268`: reps and ender were given (step optional); begin must
    // be derived
    if have == r | e || have == r | e | s {
        if options.reps == 0 {
            return Err(JotError::MustSpecifyBegin);
        }
        options.begin = options.ender - options.reps as f64 * options.step + options.step;
        return Ok(());
    }
    // `jot.c:270-279`: reps, begin and ender were given; step must be derived
    if have == r | b | e {
        if options.reps == 0 {
            return Err(JotError::InfiniteUnbounded);
        }
        if options.reps == 1 {
            options.step = 0.0;
        } else {
            options.step = (options.ender - options.begin) / (options.reps - 1) as f64;
        }
        return Ok(());
    }
    // `jot.c:281-291`: all four given; take the smaller of the given and the
    // derived reps; a zero step **breaks** — reps keeps the given value,
    // neither unbounded nor an error
    if have == r | b | e | s {
        if options.step == 0.0 {
            return Ok(());
        }
        let t = ((options.ender - options.begin + options.step) / options.step) as i64;
        if t <= 0 {
            return Err(JotError::ImpossibleStepsize);
        }
        if t < options.reps {
            options.reps = t;
        }
        return Ok(());
    }
    // The `default` of `jot.c:293-295`: nothing can be derived, keep the
    // current values
    Ok(())
}

/// Minimal linear congruential generator standing in for libc
/// `random()`; produces values in `[0, 2^31)` like the C.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Lcg(seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }
    pub fn next31(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) & 0x7fff_ffff) as u32
    }
}

/// Value stream for one jot run. `reps == 0` is the C's unbounded
/// loop; `next()` then always yields.
pub struct Jot {
    options: JotOptions,
    format: Format,
    dox: bool,
    i: i64,
    x: f64,
    rng: Option<Lcg>,
    range: f64,
    exhausted: bool,
}

impl Jot {
    /// Assembles a run: derivation + format resolution. `seed` is the
    /// `-r` seed (the `step` operand); ignored when not randomizing.
    pub fn new(mut options: JotOptions, seed: u64) -> Result<Jot, JotError> {
        let prec_inferred = match options.prec {
            None => 0,
            Some(p) => p,
        };
        let _ = prec_inferred;
        derive(&mut options)?;
        let prec = options.prec.unwrap_or(0);
        let (format, dox) = get_format(&options)?;
        let _ = prec;
        if options.randomize {
            let mut range = options.ender - options.begin;
            if range < 0.0 {
                range = -range;
                options.begin = options.ender;
            }
            let range = if dox {
                (range + 1.0) / (1u64 << 31) as f64
            } else {
                range / ((1u32 << 31) - 1) as f64
            };
            Ok(Jot {
                range,
                rng: Some(Lcg::new(seed)),
                x: options.begin,
                i: 0,
                options,
                format,
                dox,
                exhausted: false,
            })
        } else {
            // Integer display pre-adds 0.5 for sane rounding.
            let x = if dox {
                options.begin + 0.5
            } else {
                options.begin
            };
            Ok(Jot {
                range: 0.0,
                rng: None,
                x,
                i: 0,
                options,
                format,
                dox,
                exhausted: false,
            })
        }
    }

    /// How many items remain (`None` = unbounded).
    pub fn remaining(&self) -> Option<i64> {
        if self.options.reps == 0 {
            None
        } else {
            Some(self.options.reps - self.i)
        }
    }

    /// Advances and renders the next value, or `None` at the end.
    pub fn next(&mut self) -> Option<Vec<u8>> {
        if self.remaining() == Some(0) {
            return None;
        }
        let value = match self.rng.as_mut() {
            Some(rng) => rng.next31() as f64 * self.range + self.x,
            None => {
                let v = self.x;
                self.x += self.options.step;
                v
            }
        };
        self.i += 1;
        let item = render(&self.format, value);
        Some(item)
    }

    /// Separator state: the C prints `sep` unless this was the last
    /// of a bounded run (`notlast = reps - i`).
    pub fn separator_after(&self) -> Option<&[u8]> {
        match self.remaining() {
            // The last item of a bounded run prints no separator
            // (putdata's `notlast = reps - i` is zero there).
            Some(0) => None,
            _ => Some(self.options.sep.as_slice()),
        }
    }

    pub fn nofinalnl(&self) -> bool {
        self.options.nofinalnl
    }
}

/// `putdata`: renders one value through the format.
pub fn render(format: &Format, value: f64) -> Vec<u8> {
    match format {
        Format::Literal(word) => word.clone(),
        Format::Conv(c) => {
            let v = floor(value) as i64;
            match c.conv {
                ConvKind::Percent => finish(c, b"%".to_vec()),
                ConvKind::Int(kind) => match kind {
                    IntKind::C => {
                        // `printf("%c", …)` goes through `putchar` and returns
                        // **one byte**: measured against the reference binary,
                        // `-p 0 -w '%c' 1 200 1` gives `c8`, whereas
                        // `v as u8 as char` encoded through UTF-8 would print
                        // `c3 88` (two bytes) — that arm's `char` channel
                        // drops out under the byte form.
                        finish(c, vec![v as u8])
                    }
                    IntKind::O => finish_signed(c, false, format!("{:o}", v).into_bytes()),
                    IntKind::X => finish_signed(c, false, format!("{:x}", v).into_bytes()),
                    IntKind::U => {
                        finish_signed(c, false, format!("{}", v.unsigned_abs()).into_bytes())
                    }
                    _ => finish_signed(c, v < 0, v.unsigned_abs().to_string().into_bytes()),
                },
                ConvKind::Real(kind) => {
                    let prec = c.prec.unwrap_or(6);
                    let digits = match kind {
                        RealKind::F => format!("{:.*}", prec, value),
                        RealKind::E => format_e(value, prec, false),
                        RealKind::BigE => format_e(value, prec, true),
                        RealKind::G => format_g(value, prec, false),
                        RealKind::BigG => format_g(value, prec, true),
                    };
                    finish_signed(
                        c,
                        value < 0.0 && !digits.starts_with('-'),
                        digits.into_bytes(),
                    )
                }
            }
        }
    }
}

/// Applies width/zero-pad/left-align around `body`.
fn finish(c: &Conv, body: Vec<u8>) -> Vec<u8> {
    let mut out = body;
    if c.zero && !c.left && c.width > out.len() {
        // Zero padding goes after any sign.
        let pad = c.width - out.len();
        let signed = out.first().is_some_and(|&b| b == b'-' || b == b'+');
        let mut padded = Vec::with_capacity(out.len() + pad);
        if signed {
            padded.push(out[0]);
            padded.extend(core::iter::repeat_n(b'0', pad));
            padded.extend_from_slice(&out[1..]);
        } else {
            padded.extend(core::iter::repeat_n(b'0', pad));
            padded.extend_from_slice(&out);
        }
        out = padded;
    } else if c.width > out.len() {
        let pad = c.width - out.len();
        let mut spaced = Vec::with_capacity(out.len() + pad);
        if c.left {
            spaced.extend_from_slice(&out);
            spaced.extend(core::iter::repeat_n(b' ', pad));
        } else {
            spaced.extend(core::iter::repeat_n(b' ', pad));
            spaced.extend_from_slice(&out);
        }
        out = spaced;
    }
    // The field width applies only to the converted segment (`jot.c:306`,
    // `:308` are `printf(format, x)`; the literal segments before and after the
    // percent sign are printed by the format string itself), so padding is
    // inserted between the segments rather than counted into the width.
    let mut full = Vec::with_capacity(c.prefix.len() + out.len() + c.suffix.len());
    full.extend_from_slice(&c.prefix);
    full.extend_from_slice(&out);
    full.extend_from_slice(&c.suffix);
    full
}

fn finish_signed(c: &Conv, negative: bool, digits: Vec<u8>) -> Vec<u8> {
    let sign = if negative {
        "-"
    } else if c.plus {
        "+"
    } else if c.space {
        " "
    } else {
        ""
    };
    let mut body = Vec::with_capacity(sign.len() + digits.len());
    body.extend_from_slice(sign.as_bytes());
    body.extend_from_slice(&digits);
    finish(c, body)
}

/// printf `%e`: `d.ddde±XX` with `prec` decimals (point omitted at
/// precision zero, exponent at least two digits).
fn format_e(value: f64, prec: usize, upper: bool) -> String {
    let neg = value < 0.0;
    let v = value.abs();
    let mut exp: i32 = decimal_exponent(v);
    let mut mant = if v == 0.0 { 0.0 } else { v / pow10(exp) };
    if mant >= 10.0 {
        exp += 1;
        mant /= 10.0;
    }
    let mut m = format!("{:.*}", prec, mant);
    if m.starts_with("10.") || m == "10" {
        // Rounding carried into the next decade.
        exp += 1;
        m = format!("{:.*}", prec, 1.0);
    }
    let (ip, fp) = match m.split_once('.') {
        Some((a, b)) => (a, b),
        None => (m.as_str(), ""),
    };
    let e = if upper { "E" } else { "e" };
    let esign = if exp < 0 { "-" } else { "+" };
    let sign = if neg { "-" } else { "" };
    if prec > 0 {
        format!("{}{}.{}{}{}{:02}", sign, ip, fp, e, esign, exp.abs())
    } else {
        format!("{}{}{}{}{:02}", sign, ip, e, esign, exp.abs())
    }
}

/// printf `%g`: `prec` significant digits, scientific when the
/// exponent is out of [-4, prec), trailing zeros stripped.
pub fn format_g(value: f64, prec: usize, upper: bool) -> String {
    let prec = if prec == 0 { 1 } else { prec };
    if value == 0.0 {
        return "0".to_string();
    }
    let neg = value < 0.0;
    let v = value.abs();
    let exp = decimal_exponent(v);
    let (body, use_exp) = if exp < -4 || exp >= prec as i32 {
        // Scientific: mantissa with prec-1 decimals.
        let mant = format!("{:.*}", prec - 1, v / pow10(exp));
        (mant, true)
    } else {
        // Fixed with prec-1-exp decimals.
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        let fixed = format!("{:.*}", decimals, v);
        (fixed, false)
    };
    let body = strip_trailing_zeros(&body);
    let sign = if neg { "-" } else { "" };
    if use_exp {
        let e = if upper { "E" } else { "e" };
        let esign = if exp < 0 { "-" } else { "+" };
        format!("{}{}{}{}{:02}", sign, body, e, esign, exp.abs())
    } else {
        format!("{}{}", sign, body)
    }
}

fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture helper: turn a textual argv word list into byte form. All test
    /// literals are ASCII, so the two forms are equivalent; this layer only
    /// spells out the argument type of `parse_jot_args` and carries no
    /// assertion itself. Returning `Vec<&'static [u8]>` (rather than making
    /// temporaries) is required: `JotArgs` borrows those bytes, a temporary
    /// `Vec` does not outlive one statement, and any shape kept for later
    /// assertions must first be named, e.g. `let w_x = argv(&[…])`.
    fn argv(words: &'static [&'static str]) -> Vec<&'static [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    /// Every item returned by the rendering half is bytes; array literals of
    /// uneven length would mix types in `vec![b"1", b"18"]`, so expectations
    /// over "whole-string items" go through this helper.
    fn words(items: &[&str]) -> Vec<Vec<u8>> {
        items.iter().map(|word| word.as_bytes().to_vec()).collect()
    }

    fn opts() -> JotOptions {
        JotOptions::default()
    }

    #[test]
    fn test_default_jot_5_counts_from_one() {
        // `jot 5` → 1 2 3 4 5 as integers (prec 0 → "%ld").
        let mut o = opts();
        o.reps = 5;
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 0).unwrap();
        let mut items = Vec::new();
        while let Some(s) = j.next() {
            items.push(s);
        }
        assert_eq!(items, words(&["1", "2", "3", "4", "5"]));
        assert_eq!(j.separator_after(), None);
    }

    #[test]
    fn test_sep_between_not_after() {
        let mut o = opts();
        o.reps = 3;
        o.sep = b",".to_vec();
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 0).unwrap();
        assert_eq!(j.next(), Some(b"1".to_vec()));
        assert_eq!(j.separator_after(), Some(b",".as_slice()));
        assert_eq!(j.next(), Some(b"2".to_vec()));
        assert_eq!(j.next(), Some(b"3".to_vec()));
        assert_eq!(j.separator_after(), None);
    }

    #[test]
    fn test_seed_cast_truncates_then_wraps() {
        // Truncation rounds **toward zero**: `-0.5` and `0` share a seed (the
        // reference binary's two number sequences are byte-identical; see
        // `a_negative_or_fractional_seed_is_accepted_like_c` in
        // `tests/jot_face.rs`)
        assert_eq!(seed_from_step(0.0), 0);
        assert_eq!(seed_from_step(-0.5), 0);
        assert_eq!(seed_from_step(0.9), 0);
        assert_eq!(seed_from_step(-0.9), 0);
        // A negative number truncates to an integer first, then wraps: `-5`
        // and `-5.9` share a seed
        assert_eq!(seed_from_step(-5.0), u64::MAX - 4);
        assert_eq!(seed_from_step(-5.9), seed_from_step(-5.0));
        // Positive numbers are unaffected by the wrap
        assert_eq!(seed_from_step(5.0), 5);
        assert_eq!(seed_from_step(2147483648.0), 2147483648);
        // This pins the negative control that previously slipped through: the
        // whole offset range is covered now
        assert_ne!(seed_from_step(-5.0).wrapping_add(1), seed_from_step(-5.0));
    }

    #[test]
    fn test_begin_ender_step_derives_reps() {
        // `jot - 1 100 5`: reps was not given but begin was, so the hit is C's
        // third label `case ENDER | STEP | BEGIN` (`jot.c:250`) — it shares the
        // same body with `case ENDER | STEP`, and a label with one bit missing
        // would drop the whole thing into `default`, leaving reps at its
        // default instead of the derived 20
        let mut o = opts();
        o.begin = 1.0;
        o.ender = 100.0;
        o.step = 5.0;
        o.have = HAVE_BEGIN | HAVE_ENDER | HAVE_STEP;
        let j = Jot::new(o, 0).unwrap();
        assert_eq!(j.remaining(), Some(20));
    }

    #[test]
    fn test_all_four_operands_with_zero_step_keeps_reps() {
        // `jot 3 1 100 0`: C's `case REPS | BEGIN | ENDER | STEP`
        // (`jot.c:281-284`) **breaks** on a zero step, so reps keeps the given
        // value; zeroing reps belongs to the other arm `case ENDER | STEP`
        // (`:251-253`). Switch case labels are exact values; one bit short and
        // it is not the same arm
        let mut o = opts();
        o.reps = 3;
        o.begin = 1.0;
        o.ender = 100.0;
        o.step = 0.0;
        o.have = HAVE_REPS | HAVE_BEGIN | HAVE_ENDER | HAVE_STEP;
        let mut j = Jot::new(o, 0).unwrap();
        let items = (0..3).map(|_| j.next().unwrap()).collect::<Vec<_>>();
        assert_eq!(items, words(&["1", "1", "1"]));
        assert_eq!(j.remaining(), Some(0));
    }

    #[test]
    fn test_derived_reps_branch_still_goes_unbounded() {
        // Pin the other direction too: `jot - - 100 0` takes
        // `case ENDER | STEP`, where `reps = 0` means **unbounded** (the print
        // loop is `for (i = 1; i <= reps || reps == 0; i++)`, `jot.c:129`)
        let mut o = opts();
        o.ender = 100.0;
        o.step = 0.0;
        o.have = HAVE_ENDER | HAVE_STEP;
        let j = Jot::new(o, 0).unwrap();
        assert_eq!(j.remaining(), None);
    }

    #[test]
    fn test_reps_begin_ender_derives_step() {
        // `jot 4 10 20` → step = (20-10)/(4-1) = 10/3.
        let mut o = opts();
        o.reps = 4;
        o.begin = 10.0;
        o.ender = 20.0;
        o.prec = Some(2);
        o.have = HAVE_REPS | HAVE_BEGIN | HAVE_ENDER;
        let mut j = Jot::new(o, 0).unwrap();
        assert_eq!(j.next(), Some(b"10.00".to_vec()));
        assert_eq!(j.next(), Some(b"13.33".to_vec()));
        assert_eq!(j.next(), Some(b"16.67".to_vec()));
        assert_eq!(j.next(), Some(b"20.00".to_vec()));
    }

    #[test]
    fn test_reps_ender_derives_begin() {
        // `jot 4 21 5`: begin = ender - reps*step + step = 18, so the
        // run ends exactly at ender with the default step 1.
        let mut o = opts();
        o.reps = 4;
        o.ender = 21.0;
        o.have = HAVE_REPS | HAVE_ENDER;
        let mut j = Jot::new(o, 0).unwrap();
        let items = (0..4).map(|_| j.next().unwrap()).collect::<Vec<_>>();
        assert_eq!(items, words(&["18", "19", "20", "21"]));
    }

    #[test]
    fn test_ender_step_derives_reps() {
        // `jot - 0 10 5` → reps = (10-0+5)/5 = 3 → 0 5 10.
        let mut o = opts();
        o.begin = 0.0;
        o.ender = 10.0;
        o.step = 5.0;
        o.have = HAVE_ENDER | HAVE_STEP;
        let mut j = Jot::new(o, 0).unwrap();
        let items = (0..3).map(|_| j.next().unwrap()).collect::<Vec<_>>();
        assert_eq!(items, words(&["0", "5", "10"]));
        assert_eq!(j.remaining(), Some(0));
    }

    #[test]
    fn test_non_numeric_operand_uses_last_char() {
        // jot 3 a → begin = 'a' = 97 (integer mode).
        assert_eq!(operand_to_number(b"a"), Some(97.0));
        assert_eq!(operand_to_number(b"12x"), Some(12.0));
        assert_eq!(get_prec(b"1.250"), 3);
        assert_eq!(get_prec(b"42"), 0);
    }

    #[test]
    fn test_word_without_percent_appends_conversion() {
        // `-w x` + integer prec → "x%ld"; -c → "x%c".
        let mut o = opts();
        o.word = b"x".to_vec();
        o.prec = Some(0);
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox);
        assert_eq!(render(&f, 3.0), b"x3");
        o.chardata = true;
        let (f, _) = get_format(&o).unwrap();
        assert_eq!(render(&f, 65.0), b"xA");
    }

    /// `prec` never spelled (C's `-1`, `jot.c:74`) and explicitly written as
    /// `0` (`-p 0` or derived 0) are two different things: the former takes
    /// C's `%%.%df` arm and assembles `%.-1f`, which Minix's libc parses as a
    /// float with precision 0 (`minix3/lib/libc/stdio/vsnprintf_ss.c:213-226`:
    /// a non-digit after `.` ⇒ `prec = 0`, then `-` is the left-adjust flag,
    /// `1` the width and `f` the conversion), i.e. **rounding**; the latter
    /// takes the integer arm, `(long)floor(x)` (`jot.c:306`, `dox`), i.e.
    /// **flooring**. The two arms support different values, and this test pins
    /// that boundary.
    #[test]
    fn an_unwritten_precision_rounds_while_an_explicit_zero_floors() {
        let mut o = opts();
        // No precision written: takes the float arm, so dox is false.
        o.prec = None;
        let (f, dox) = get_format(&o).unwrap();
        assert!(!dox, "未写精度不进整数支");
        // The derived specifier must carry all three elements Minix gets from
        // parsing `%.-1f`: the left-adjust flag, field width 1 and precision 0.
        // Missing any one copies only half of C's ground truth.
        match &f {
            Format::Conv(c) => {
                assert!(c.left, "`.` 后那个 `-` 在 Minix 的旗标循环里是 LADJUST");
                assert_eq!(c.width, 1, "剩下的 `1` 是字段宽");
                assert_eq!(c.prec, Some(0), "`.` 后非数字 ⇒ 精度 0");
                assert!(matches!(c.conv, ConvKind::Real(RealKind::F)), "结尾是 `f`");
            }
            Format::Literal(_) => panic!("未写精度该构造转换，而不是字面"),
        }
        assert_eq!(render(&f, 0.7), b"1", "0.7 该四舍五入成 1，不是取整成 0");
        assert_eq!(render(&f, 1.5), b"2", "半数上取");
        assert_eq!(render(&f, 2.5), b"2", "凑偶：与 NetBSD dtoa 同规则");
        assert_eq!(render(&f, 3.0), b"3", "整值两支撑得一样");
        // The `-c` character arm takes precedence over the whole precision
        // family: C's condition is `chardata || prec == 0`, so when `chardata`
        // is true it takes `%c` **whether or not prec was written**.
        o.chardata = true;
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox, "`-c` 在精度未写时仍走整数支");
        assert_eq!(render(&f, 65.0), b"A");
        o.chardata = false;

        // Explicit 0: takes the integer arm, floor.
        o.prec = Some(0);
        let (f, dox) = get_format(&o).unwrap();
        assert!(dox, "显式 0 走 %ld 支");
        assert_eq!(render(&f, 0.7), b"0", "floor 与上面的 round 在此分家");
        assert_eq!(render(&f, 1.5), b"1");
    }

    #[test]
    fn test_boring_repeats_word_verbatim() {
        let mut o = opts();
        o.boring = true;
        o.word = b"xy".to_vec();
        let (f, _) = get_format(&o).unwrap();
        assert_eq!(f, Format::Literal(b"xy".to_vec()));
        assert_eq!(render(&f, 1.0), b"xy");
    }

    #[test]
    fn test_width_and_conversion_flags() {
        let mut o = opts();
        o.word = b"%5.2f|".to_vec();
        o.prec = None;
        let (f, dox) = get_format(&o).unwrap();
        assert!(!dox);
        // Do not use an approximation of π as the test value: both `3.14159`
        // and `3.1416` trip the deny-level `clippy::approx_constant` — its
        // constant table contains **rounded short forms** (measured: 1.94.0
        // reports even the four-digit `3.1416`), it compares values rather than
        // spellings, and underscore separators do not help.
        // What this test pins is "`%5.2f` pads the value to five columns",
        // independent of where the value comes from: `9.5` is exact in binary,
        // so two decimals are `.50`; `7.9375` (127/16, also exact) has a `7` in
        // the third place and must carry to `7.94`.
        assert_eq!(render(&f, 9.5), b" 9.50|");
        assert_eq!(render(&f, 7.9375), b" 7.94|");
        o.word = b"%04x".to_vec();
        let (f, _) = get_format(&o).unwrap();
        assert!(is_dox(&f));
        assert_eq!(render(&f, 11.0), b"000b");
    }

    #[test]
    fn test_string_conversion_rejected() {
        let mut o = opts();
        o.word = b"%s".to_vec();
        assert_eq!(get_format(&o), Err(JotError::FormatWantsString));
        o.word = b"%d %d".to_vec();
        assert!(get_format(&o).is_err());
    }

    #[test]
    fn test_random_integers_stay_in_range() {
        // -r with integer mode: values in [begin, ender] inclusive.
        let mut o = opts();
        o.randomize = true;
        o.reps = 40;
        o.begin = 1.0;
        o.ender = 6.0;
        o.prec = Some(0);
        o.have = HAVE_REPS;
        let mut j = Jot::new(o, 42).unwrap();
        let mut seen = Vec::new();
        while let Some(s) = j.next() {
            let v: i64 = String::from_utf8(s).unwrap().parse().unwrap();
            assert!((1..=6).contains(&v), "value {} out of range", v);
            seen.push(v);
        }
        assert_eq!(seen.len(), 40);
    }

    #[test]
    fn test_g_format_matches_printf() {
        assert_eq!(format_g(3.2808399, 8, false), "3.2808399");
        assert_eq!(format_g(2.54, 8, false), "2.54");
        assert_eq!(format_g(0.0001, 8, false), "0.0001");
        assert_eq!(format_g(0.00001, 8, false), "1e-05");
        assert_eq!(format_g(1e24, 8, false), "1e+24");
        assert_eq!(format_g(-42.0, 8, false), "-42");
    }

    /// An unknown letter and a missing value both return libc's two shapes,
    /// leaving `usage()` to the shell. A cluster reports only the **first** bad
    /// letter.
    #[test]
    fn an_unreachable_option_shape_is_reported_by_the_libc_line() {
        assert_eq!(
            parse_jot_args(&argv(&["-Z"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            parse_jot_args(&argv(&["-cz"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('z'))
        );
        assert_eq!(
            parse_jot_args(&argv(&["-cb"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::MissingValue('b'))
        );
        // `--x` reports the second hyphen as the letter: the optstring has no
        // `-`.
        assert_eq!(
            parse_jot_args(&argv(&["--x"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('-'))
        );
    }

    /// A bad value letter that comes first is reported first: the `errx` of
    /// `-p abc` and the libc line of `-Z` take separate exits, and both happen
    /// inside the ring, before any operand decision.
    #[test]
    fn a_rejected_precision_is_the_bad_value_sentence_not_the_libc_one() {
        assert_eq!(
            parse_jot_args(&argv(&["-p", "abc"])).err().unwrap(),
            JotArgError::BadPrecision
        );
        assert_eq!(
            parse_jot_args(&argv(&["-p", "abc", "-Z"])).err().unwrap(),
            JotArgError::BadPrecision
        );
        assert_eq!(
            parse_jot_args(&argv(&["-Z", "-p", "abc"])).err().unwrap(),
            JotArgError::Getopt(crate::GetoptReport::UnknownOption('Z'))
        );
    }

    /// The ring does **not** permute: it ends at the first non-option token,
    /// which is itself the first operand, and the following `-Z` also stays
    /// among the operands (the reference binary's `jot 3 -Z` prints 90/91/92,
    /// treating `-Z` as the begin character; there is no second scan at all).
    #[test]
    fn the_ring_ends_at_the_first_operand_and_keeps_everything_after_it() {
        let w_ring = argv(&["3", "-Z"]);
        let parsed = parse_jot_args(&w_ring).expect("the ring should end at the operand");
        assert_eq!(parsed.operands, argv(&["3", "-Z"]));
        assert!(
            !parsed.options.chardata,
            "`-Z` 不是选项，`-c` 那一位不该被点起"
        );
    }

    /// `--` ends the ring and is not itself an operand; a bare `-` ends the
    /// ring but **stays** among the operands (`jot.c:66`'s `is_default` reads
    /// it as "use the default for this cell").
    #[test]
    fn the_double_dash_is_dropped_but_a_bare_dash_stays_an_operand() {
        let w_dash = argv(&["--", "-Z"]);
        let parsed = parse_jot_args(&w_dash).expect("`--` 之后全是操作数");
        assert_eq!(parsed.operands, argv(&["-Z"]));
        let w_only = argv(&["--"]);
        let parsed = parse_jot_args(&w_only).expect("只有 `--` 也是合法的一次扫描");
        assert!(parsed.operands.is_empty());
        let w_bare = argv(&["-", "3"]);
        let parsed = parse_jot_args(&w_bare).expect("裸 `-` 不该被吃掉");
        assert_eq!(parsed.operands, argv(&["-", "3"]));
    }

    /// Both spellings of a value letter consume the next token as the option's
    /// value, never as an operand; `-b` and `-w` land in the same word slot
    /// (`jot.c:173-176` fallthrough).
    #[test]
    fn a_value_letter_consumes_the_next_token_and_b_and_w_share_one_slot() {
        let w_prec = argv(&["-p", "0"]);
        let parsed = parse_jot_args(&w_prec).expect("`0` 是 `-p` 的值");
        assert_eq!(parsed.options.prec, Some(0));
        assert!(
            parsed.operands.is_empty(),
            "被 `-p` 吃掉的 token 不再是操作数"
        );
        let w_two = argv(&["-bX", "-wY", "3"]);
        let parsed = parse_jot_args(&w_two).expect("两个词都该收到");
        assert_eq!(parsed.options.word, b"Y", "后写的覆盖先写的");
        assert!(parsed.options.boring, "`-b` 点起的位不会因 `-w` 落下");
        assert_eq!(parsed.operands, argv(&["3"]));
    }

    /// `strtol(optarg, &ep, 0)` plus `*ep != 0 || prec < 0`: the two base-0
    /// prefixes, the zero-character tail, and "no digit converted leaves `ep`
    /// at the starting point" are all in this table.
    #[test]
    fn the_precision_follows_strtol_base_zero_with_a_full_parse_check() {
        for (word, accepted) in [
            ("2", Some(2)),
            ("0x2", Some(2)),
            ("010", Some(8)),
            ("0", Some(0)),
            ("", Some(0)),
            (" 2", Some(2)),
            ("+2", Some(2)),
            ("3x", None),
            ("09", None),
            ("-1", None),
            ("abc", None),
            (" ", None),
            ("-", None),
            ("0x", None),
        ] {
            assert_eq!(
                parse_precision(word.as_bytes()),
                accepted,
                "`-p {word}` 的接受性"
            );
        }
        // The value saturates at `long` first, then truncates to `int` as C
        // does: `LONG_MAX` truncates to `-1` and is rejected, `2^32+2`
        // truncates to `2` and is accepted.
        assert_eq!(parse_precision(b"99999999999999999999"), None);
        assert_eq!(parse_precision(b"4294967298"), Some(2));
    }

    /// Of jot's four value-taking letters, three take values that are
    /// **output content** and one takes a number: the `-b`/`-w` word is printed
    /// verbatim by `putdata` (`jot.c:300-311`), the `-s` string is written
    /// between items by `:310`'s `fputs(sepstring, stdout)`, and the `-w` word
    /// additionally goes through `getformat` (`:333-398`), a byte-wise
    /// validation. So one raw `ff` here decides three things at once: the bytes
    /// that print, whether the specifier splits pass validation, and how many
    /// bytes the failure echo has (`jot.c:385`, `:396`'s
    /// `unknown or invalid format \`%s'` carries the whole `format`). The two
    /// operand legs live under the same gate: `jot.c:196`/`:204` takes the last
    /// **byte** (`argv[i][strlen(argv[i]) - 1]`), and `strtoul` on the empty
    /// string is "no digit converted, `ep` stays at the start, `*ep ==
    /// '\0'`", so empty reps is a legal 0 (= unbounded) rather than a bad value.
    #[test]
    fn the_word_the_separator_and_the_operand_bytes_all_reach_the_output() {
        // (1) The `-b` word: the `boring` path prints the whole word as a
        // literal segment (`jot.c:304`'s `printf("%s", format)`).
        let raw: &[&[u8]] = &[b"-b\xffwd", b"2"];
        let parsed = parse_jot_args(raw).expect("`-b` 收整串当词");
        assert_eq!(
            parsed.options.word,
            vec![0xff, b'w', b'd'],
            "词里那枚裸字节必须原样到库"
        );
        assert!(parsed.options.boring);
        let (format, dox) = get_format(&parsed.options).unwrap();
        assert_eq!(format, Format::Literal(vec![0xff, b'w', b'd']));
        assert!(!dox, "字面段不产生转换，`dox` 那一位置起不来");
        assert_eq!(
            render(&format, 7.0),
            vec![0xff, b'w', b'd'],
            "打出去的必须是那三枚字节"
        );
        // (2) The `-s` string: the separator between items is the raw bytes
        // (measured: for `-s<ff>x` the reference binary prints `ff 78` between
        // items).
        let sep: &[&[u8]] = &[b"-s\xffx", b"2"];
        let parsed = parse_jot_args(sep).expect("`-s` 的值是一串字节");
        assert_eq!(parsed.options.sep, vec![0xff, b'x']);
        // (3) The `-w` word: the literal segment before the percent sign is
        // stored as bytes, and the padding after the conversion does not
        // interfere with either side.
        let mut o = opts();
        o.word = vec![b'a', 0xff, b'%', b'd'];
        let (format, dox) = get_format(&o).unwrap();
        assert!(dox, "`%d` 是整数支（`jot.c:382` 的 `dox = 1`）");
        let Format::Conv(conv) = &format else {
            panic!("带 `%d` 的词该构造转换，而不是字面段");
        };
        assert_eq!(
            conv.prefix,
            vec![b'a', 0xff],
            "百分号前那一段就是 argv 的原字节"
        );
        assert_eq!(render(&format, 3.0), vec![b'a', 0xff, b'3']);
        // (4) `%c` returns **one byte**: C goes through `putchar`, so 200 is
        //     `c8`; that leg going through the `char` channel would encode two
        //     bytes, `c3 88` (measured: `-p 0 -w '%c' 1 200 1` gives `c8`).
        let mut o = opts();
        o.word = b"%c".to_vec();
        o.prec = Some(0);
        let (format, _) = get_format(&o).unwrap();
        assert_eq!(render(&format, 200.0), vec![0xc8]);
        // (5) The validation-failure echo carries the **whole format word**
        //     (including raw bytes), not an empty sentence.
        let mut o = opts();
        o.word = vec![b'%', 0xff];
        assert_eq!(
            get_format(&o).err(),
            Some(JotError::InvalidFormat(vec![b'%', 0xff])),
            "`unknown or invalid format `%s'` 的值位是原字节"
        );
        // (6) An operand takes its last byte: `é` (`c3 a9`) in C yields `a9` =
        //     169.
        assert_eq!(operand_to_number(b"a"), Some(97.0));
        assert_eq!(
            operand_to_number(b"\xc3\xa9"),
            Some(-87.0),
            "取到的是最后一字节 `a9`，而 `char` 带符号"
        );
        // (7) Empty operand: C reads `argv[i][-1]` out of bounds; this layer
        //     does not copy undefined behavior and returns `None` so the caller
        //     keeps that cell's default (measured on the host reference binary:
        //     `jot 3 '' 5` → `1 3 5`).
        assert_eq!(
            operand_to_number(b""),
            None,
            "空操作数不该 panic，也不该编出一个值"
        );
        // (8) Empty reps is a legal 0 in C (`strtoul` leaves `ep` at the
        //     start), hence unbounded rather than a bad value.
        assert_eq!(parse_reps(b""), Ok(0));
        assert_eq!(
            parse_reps(b"x").err(),
            Some(JotError::BadReps(b"x".to_vec()))
        );
        // (9) Nothing outside the six `isspace` bytes counts as whitespace:
        //     non-ASCII whitespace (`c2 a0`) is not whitespace in C, so no
        //     digit converts and Bad reps follows — going by characters,
        //     `str::trim_start` would skip it and accept unexpectedly. This
        //     shape pins "byte-wise" rather than "character-wise".
        assert_eq!(
            parse_reps(&[0xc2, 0xa0, b'3'])
                .err()
                .map(|e| matches!(e, JotError::BadReps(_))),
            Some(true),
            "`c2 a0` 不是 C 的 isspace，不该被跳过"
        );
        // (11) The prefix segment's `%%` must be unfolded too: C hands the
        //    **whole** format to `printf` (`jot.c:306`, `:308`) and libc does
        //    the unfolding regardless of which side of the conversion it is
        //    on. For `-p 2 -w '%%x%d' 3 1 5` the reference binary measures
        //    `%x1` — the version that unfolded only the suffix prints one
        //    extra percent sign here.
        let mut o = opts();
        o.word = b"%%x%d".to_vec();
        o.prec = Some(2);
        let (format, _) = get_format(&o).unwrap();
        assert_eq!(
            render(&format, 1.0),
            b"%x1".to_vec(),
            "前缀里两枚百分号该展开成一枚"
        );
        // (10) When one of the three value-taking letters other than `-p` is
        //     missing its value, the missing letter is a byte too
        //     (`getopt.c:132-136`'s `%c`).
        assert_eq!(
            parse_jot_args(&argv(&["-s"])).err(),
            Some(JotArgError::Getopt(crate::GetoptReport::MissingValue('s'))),
        );
    }
}
