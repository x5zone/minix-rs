//! The POSIX `printf` runtime format engine.
//!
//! Ground truth: `minix3/usr.bin/printf/printf.c` (NetBSD). The C passes
//! its parsed flags, width, and precision straight to the C library's
//! `printf` (the `PF` macro, lines 98-117), so flags and padding follow
//! standard C `printf` semantics for the supported conversions; this
//! module reimplements exactly those semantics for the non-floating
//! conversions. The surrounding behaviour is the C's own: escapes
//! (`conv_escape`, lines 424-479, including 1-3 digit octal and 2-digit
//! hex, unknown escapes warning and passing through), `\c` halting
//! everything (the `rval & 0x100` flag, lines 314-319, 373-375), numeric
//! operands through `strtoimax`/`strtoumax` with base zero and the
//! character-constant prefix (`getintmax`/`getuintmax`, lines 542-597),
//! conversion warnings (`check_conversion`, lines 671-685: "expected
//! numeric value" for an empty parse, "not completely converted" for
//! trailing text, saturation on range), operand defaults when the list
//! runs dry (empty string, zero), and format reuse while operands remain
//! (the `do ... while` at lines 316-319).
//!
//! Floating conversions (`e E f g G`) are declared unsupported here —
//! their rendering is its own design decision (the C delegates to the
//! float renderer this tree has not chosen yet) — and surface as
//! [`FormatError::FloatNotModelled`] so the doing half can say so loudly.
//!
//! The engine is pure: text goes out through the `emit` sink, diagnostics
//! through the `warn` sink, and nothing allocates.

/// What went wrong while formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError<'a> {
    /// A `%` ended the format without a conversion character
    /// (printf.c:212: "missing format character").
    MissingFormatCharacter,
    /// An unknown conversion (printf.c:305: "invalid directive"); the
    /// specification text rides along.
    InvalidDirective(&'a str),
    /// A floating conversion, declared unsupported (see the module header);
    /// the specification text rides along.
    FloatNotModelled(&'a str),
    /// The output sink refused a piece (the C's "print failed" path).
    WriteFailed,
    /// A rendered piece exceeded the fixed digit capacity.
    TooLong,
}

/// Formats `format` over `operands`, feeding pieces to `emit`.
///
/// Returns the process status: 0, or 1 when a conversion warned (bad
/// number, unknown escape — the C's `rval`). `emit` returning `false`
/// aborts with [`FormatError::WriteFailed`]. The format is reused while
/// operands remain and at least one was consumed, exactly as the C's
/// reuse loop does.
pub fn format<'a, 'f>(
    operands: &'a [&'a str],
    format: &'f str,
    emit: &mut dyn FnMut(&[u8]) -> bool,
    warn: &mut dyn FnMut(&[u8]),
) -> Result<i32, FormatError<'f>> {
    let mut state = State::new(operands);
    loop {
        scan_pass(&mut state, format, emit, warn)?;
        if state.halted || !state.moved || state.next >= operands.len() {
            break;
        }
    }
    Ok(if state.warned { 1 } else { 0 })
}

struct State<'a> {
    operands: &'a [&'a str],
    next: usize,
    /// An operand was consumed during the current pass (the C's
    /// `gargv != argv` half of the reuse condition).
    moved: bool,
    /// A `\c` was seen: stop all output (the `rval & 0x100` flag).
    halted: bool,
    /// A conversion warned; the status leaves as 1 (the C's `rval`).
    warned: bool,
}

impl<'a> State<'a> {
    fn new(operands: &'a [&'a str]) -> Self {
        State {
            operands,
            next: 0,
            moved: false,
            halted: false,
            warned: false,
        }
    }

    /// The next operand, consuming it; `None` keeps the list still.
    fn fetch(&mut self) -> Option<&'a str> {
        if self.next < self.operands.len() {
            self.moved = true;
            let word = self.operands[self.next];
            self.next += 1;
            Some(word)
        } else {
            None
        }
    }

    /// Signed operand per `getintmax` (printf.c:542-565): the
    /// character-constant prefix, else `strtoimax` base-zero semantics
    /// (leading blanks, sign, `0x`/octal prefixes, longest valid prefix
    /// wins with a warning, range saturation with a warning). A dry list
    /// is zero.
    fn fetch_i64(&mut self, warn: &mut dyn FnMut(&[u8])) -> i64 {
        let Some(word) = self.fetch() else {
            return 0;
        };
        let bytes = word.as_bytes();
        if matches!(bytes.first(), Some(b'"') | Some(b'\'')) && bytes.len() > 1 {
            return bytes[1] as i64;
        }
        let body = skip_blanks(word);
        let (negative, body) = strip_sign(body);
        match parse_base_zero_u64(body) {
            None => {
                warn(b"expected numeric value");
                self.warned = true;
                0
            }
            Some((magnitude, consumed)) => {
                if consumed != body.len() {
                    warn(b"not completely converted");
                    self.warned = true;
                }
                if negative {
                    let capped = magnitude.min(1u64 << 63);
                    (capped as i64).wrapping_neg()
                } else {
                    magnitude.min(i64::MAX as u64) as i64
                }
            }
        }
    }

    /// Unsigned operand per `getuintmax` (printf.c:567-597): the
    /// character-constant prefix, a negative value rejected by name,
    /// else `strtoumax` base-zero semantics with saturation and warnings.
    fn fetch_u64(&mut self, warn: &mut dyn FnMut(&[u8])) -> u64 {
        let Some(word) = self.fetch() else {
            return 0;
        };
        let bytes = word.as_bytes();
        if matches!(bytes.first(), Some(b'"') | Some(b'\'')) && bytes.len() > 1 {
            return bytes[1] as u64;
        }
        let body = skip_blanks(word);
        if body.starts_with('-') {
            warn(b"expected positive numeric value");
            self.warned = true;
            return 0;
        }
        match parse_base_zero_u64(body) {
            None => {
                warn(b"expected numeric value");
                self.warned = true;
                0
            }
            Some((value, consumed)) => {
                if consumed != body.len() {
                    warn(b"not completely converted");
                    self.warned = true;
                }
                value
            }
        }
    }
}

fn skip_blanks(word: &str) -> &str {
    let mut rest = word;
    while matches!(
        rest.as_bytes().first(),
        Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(0x0B) | Some(0x0C)
    ) {
        rest = &rest[1..];
    }
    rest
}

fn strip_sign(body: &str) -> (bool, &str) {
    match body.as_bytes().first() {
        Some(b'-') => (true, &body[1..]),
        Some(b'+') => (false, &body[1..]),
        _ => (false, body),
    }
}

/// `strtoumax` base-zero prefix parse: `0x`/`0X` hex, leading-zero
/// octal, else decimal. Returns the value (saturated) and how many bytes
/// were consumed; a word with no digits at all is `None`.
fn parse_base_zero_u64(text: &str) -> Option<(u64, usize)> {
    let bytes = text.as_bytes();
    let mut radix = 10;
    let mut start = 0;
    if bytes.len() > 1 && bytes[0] == b'0' && (bytes[1] | 0x20) == b'x' {
        radix = 16;
        start = 2;
    } else if bytes.len() > 1 && bytes[0] == b'0' {
        radix = 8;
        start = 1;
    }
    let mut value: u64 = 0;
    let mut consumed = 0;
    for &byte in &bytes[start..] {
        let digit = match byte {
            b'0'..=b'9' => (byte - b'0') as u64,
            b'a'..=b'f' if radix == 16 => (byte - b'a' + 10) as u64,
            b'A'..=b'F' if radix == 16 => (byte - b'A' + 10) as u64,
            _ => break,
        };
        value = value
            .checked_mul(radix)
            .and_then(|scaled| scaled.checked_add(digit))
            .unwrap_or(u64::MAX);
        consumed += 1;
    }
    if consumed == 0 {
        return None;
    }
    Some((value, start + consumed))
}

/// One reuse pass over the format. Sets `state.moved` when an operand is
/// consumed and `state.halted` on `\c`.
fn scan_pass<'a, 'f>(
    state: &mut State<'a>,
    format: &'f str,
    emit: &mut dyn FnMut(&[u8]) -> bool,
    warn: &mut dyn FnMut(&[u8]),
) -> Result<(), FormatError<'f>> {
    let bytes = format.as_bytes();
    let mut position = 0;
    while position < bytes.len() && !state.halted {
        let byte = bytes[position];
        position += 1;
        if byte == b'\\' {
            let mut literal = [0u8; 1];
            let advance = escape_of(&bytes[position..], &mut literal, state, warn);
            let is_c = literal[0] == HALT_BYTE;
            if !is_c && !emit(&literal[..1]) {
                return Err(FormatError::WriteFailed);
            }
            position += advance;
            if is_c {
                state.halted = true;
            }
            continue;
        }
        if byte != b'%' {
            if !emit(&bytes[position - 1..position]) {
                return Err(FormatError::WriteFailed);
            }
            continue;
        }
        if bytes.get(position) == Some(&b'%') {
            position += 1;
            if !emit(b"%") {
                return Err(FormatError::WriteFailed);
            }
            continue;
        }
        // A conversion specification: flags, width, precision, conversion
        // (printf.c:170-193).
        let spec_start = position - 1;
        let flag_len = flag_span(&bytes[position..]);
        let flags = &format[position..position + flag_len];
        position += flag_len;
        let mut width: Option<usize> = None;
        if let Some(span) = digit_span(&bytes[position..]) {
            width = Some(decimal_of(&bytes[position..position + span]) as usize);
            position += span;
        } else if bytes.get(position) == Some(&b'*') {
            position += 1;
            width = Some(state.fetch_i64(warn).max(0) as usize);
        }
        let mut precision: Option<usize> = None;
        if bytes.get(position) == Some(&b'.') {
            position += 1;
            if let Some(span) = digit_span(&bytes[position..]) {
                precision = Some(decimal_of(&bytes[position..position + span]) as usize);
                position += span;
            } else {
                // `%.d` means precision zero in C printf.
                precision = Some(0);
            }
        }
        let conversion = match bytes.get(position) {
            Some(&ch) => ch,
            None => return Err(FormatError::MissingFormatCharacter),
        };
        let spec = &format[spec_start..=position];
        position += 1;
        match conversion {
            b'd' | b'i' => {
                let value = state.fetch_i64(warn);
                let mut digits = [0u8; DIGIT_CAP];
                let length = signed_digits(value, precision, &mut digits)?;
                emit_padded_integer(&digits[..length], value < 0, flags, width, precision.is_none(), emit)?;
            }
            b'o' | b'u' | b'x' | b'X' => {
                let value = state.fetch_u64(warn);
                let (radix, upper) = match conversion {
                    b'o' => (8, false),
                    b'x' => (16, false),
                    b'X' => (16, true),
                    _ => (10, false),
                };
                let mut digits = [0u8; DIGIT_CAP];
                let length = unsigned_digits(value, radix, upper, precision, &mut digits)?;
                // `#`: octal gains a leading zero (unless the digits
                // already start with one), hex gains 0x/0X on a nonzero
                // value.
                let alternate = flags.contains('#');
                let prefix: &[u8] = if alternate && conversion == b'o' && !digits.starts_with(b"0")
                {
                    b"0"
                } else if alternate && conversion != b'o' && value != 0 {
                    if upper {
                        b"0X"
                    } else {
                        b"0x"
                    }
                } else {
                    b""
                };
                emit_padded_integer_prefixed(&digits[..length], false, prefix, flags, width, precision.is_none(), emit)?;
            }
            b'c' => {
                if let Some(word) = state.fetch() {
                    if let Some(&first) = word.as_bytes().first() {
                        pad_emit(&[first], flags.contains('-'), width, emit)?;
                    }
                }
            }
            b's' | b'b' => {
                let Some(word) = state.fetch() else {
                    continue;
                };
                if conversion == b's' {
                    let truncated = precision.map_or(word.len(), |limit| limit.min(word.len()));
                    pad_emit(&word.as_bytes()[..truncated], flags.contains('-'), width, emit)?;
                } else {
                    // `%b`: the operand's escapes expand first (with `\c`
                    // halting everything), then precision truncates and
                    // width pads like a string.
                    let expanded = escape_span_all(word.as_bytes());
                    let truncated = precision.map_or(expanded, |limit| limit.min(expanded));
                    if !flags.contains('-') {
                        emit_pad(width.map_or(0, |w| w.saturating_sub(truncated)), emit)?;
                    }
                    let mut emitted = 0usize;
                    let mut cursor = 0usize;
                    let mut literal = [0u8; 1];
                    let word_bytes = word.as_bytes();
                    while cursor < word_bytes.len() && emitted < truncated && !state.halted {
                        if word_bytes[cursor] == b'\\' {
                            let advance =
                                escape_of(&word_bytes[cursor + 1..], &mut literal, state, warn);
                            cursor += 1 + advance;
                            if literal[0] == HALT_BYTE {
                                state.halted = true;
                                break;
                            }
                            emit(&literal[..1]);
                        } else {
                            emit(&word_bytes[cursor..cursor + 1]);
                            cursor += 1;
                        }
                        emitted += 1;
                    }
                    if flags.contains('-') {
                        emit_pad(width.map_or(0, |w| w.saturating_sub(truncated)), emit)?;
                    }
                }
            }
            b'e' | b'E' | b'f' | b'g' | b'G' => {
                return Err(FormatError::FloatNotModelled(spec));
            }
            _ => return Err(FormatError::InvalidDirective(spec)),
        }
    }
    Ok(())
}

/// The escape value that means "halt everything" (`\c`).
const HALT_BYTE: u8 = 0xFF;

/// Resolves one escape after the backslash into `out[0]`; returns how
/// many bytes of `rest` the escape consumed. `\c` stores [`HALT_BYTE`]
/// (a value no real escape produces); unknown escapes warn, flag
/// `state.warned`, and pass the character through (printf.c:476-479).
fn escape_of(
    rest: &[u8],
    out: &mut [u8; 1],
    state: &mut State<'_>,
    warn: &mut dyn FnMut(&[u8]),
) -> usize {
    if rest.is_empty() {
        out[0] = b'\\';
        return 0;
    }
    let ch = rest[0];
    let (value, advance) = match ch {
        b'0'..=b'7' => {
            let mut value = (ch - b'0') as u32;
            let mut taken = 1usize;
            for &next in rest.iter().skip(1).take(2) {
                if !(b'0'..=b'7').contains(&next) {
                    break;
                }
                value = value * 8 + (next - b'0') as u32;
                taken += 1;
            }
            ((value & 0xFF) as u8, taken)
        }
        b'x' => {
            let mut value: u32 = 0;
            let mut taken = 0usize;
            for &next in rest.iter().skip(1).take(2) {
                let digit = match next {
                    b'0'..=b'9' => (next - b'0') as u32,
                    b'a'..=b'f' => (next - b'a' + 10) as u32,
                    b'A'..=b'F' => (next - b'A' + 10) as u32,
                    _ => break,
                };
                value = value * 16 + digit;
                taken += 1;
            }
            ((value & 0xFF) as u8, taken + 1)
        }
        b'\\' => (b'\\', 1),
        b'\'' => (b'\'', 1),
        b'"' => (b'"', 1),
        b'a' => (0x07, 1),
        b'b' => (0x08, 1),
        b'e' => (0x1B, 1),
        b'f' => (0x0C, 1),
        b'n' => (b'\n', 1),
        b'r' => (b'\r', 1),
        b't' => (b'\t', 1),
        b'v' => (0x0B, 1),
        b'c' => (HALT_BYTE, 1),
        _ => {
            const HEAD: &[u8] = b"unknown escape sequence `\\";
            let mut message = [0u8; HEAD.len() + 2];
            message[..HEAD.len()].copy_from_slice(HEAD);
            message[HEAD.len()] = ch;
            message[HEAD.len() + 1] = b'\n';
            warn(&message[..HEAD.len() + 2]);
            state.warned = true;
            (ch, 1)
        }
    };
    out[0] = value;
    advance
}

/// Counts the expanded size of a whole `%b` operand (the count pass of
/// the C's two-pass trick, printf.c:240-256).
fn escape_span_all(bytes: &[u8]) -> usize {
    let mut cursor = 0usize;
    let mut count = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
            let rest = &bytes[cursor + 1..];
            // The advance includes the backslash itself.
            let advance = match rest[0] {
                b'0'..=b'7' => 2 + octal_tail(rest),
                b'x' => 2 + hex_tail(rest),
                _ => 2,
            };
            cursor += advance;
        } else {
            cursor += 1;
        }
        count += 1;
    }
    count
}

fn octal_tail(rest: &[u8]) -> usize {
    rest.iter()
        .skip(1)
        .take(2)
        .filter(|b| (b'0'..=b'7').contains(b))
        .count()
}

fn hex_tail(rest: &[u8]) -> usize {
    rest.iter()
        .skip(1)
        .take(2)
        .filter(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
        .count()
}

fn flag_span(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|b| matches!(b, b'#' | b'-' | b'+' | b' ' | b'0' | b'\''))
        .count()
}

fn digit_span(bytes: &[u8]) -> Option<usize> {
    let count = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if count == 0 {
        None
    } else {
        Some(count)
    }
}

fn decimal_of(bytes: &[u8]) -> u64 {
    let mut value: u64 = 0;
    for &byte in bytes {
        value = value * 10 + (byte - b'0') as u64;
    }
    value
}

/// Fixed capacity for one rendered integer: 64 binary digits would be
/// the floor; 80 leaves room for prefixes. Precisions beyond this are a
/// `TooLong`, never a silent truncation.
pub const DIGIT_CAP: usize = 80;

/// Signed digits: the decimal magnitude, zero-extended to the precision;
/// precision zero renders a zero value as no digits at all. Returns the
/// usable prefix of `out`.
fn signed_digits(value: i64, precision: Option<usize>, out: &mut [u8; DIGIT_CAP]) -> Result<usize, FormatError<'static>> {
    let magnitude = value.unsigned_abs();
    if magnitude == 0 && precision == Some(0) {
        return Ok(0);
    }
    let mut length = radix_digits(magnitude, 10, false, out);
    let limit = match precision {
        Some(limit) => limit,
        None => return Ok(length),
    };
    if limit > DIGIT_CAP - 2 {
        return Err(FormatError::TooLong);
    }
    while length < limit {
        out.copy_within(0..length, 1);
        out[0] = b'0';
        length += 1;
    }
    Ok(length)
}

/// Unsigned digits in the radix, zero-extended to the precision with the
/// same zero-value rule.
fn unsigned_digits(value: u64, radix: u32, upper: bool, precision: Option<usize>, out: &mut [u8; DIGIT_CAP]) -> Result<usize, FormatError<'static>> {
    if value == 0 && precision == Some(0) {
        return Ok(0);
    }
    let mut length = radix_digits(value, radix, upper, out);
    let limit = match precision {
        Some(limit) => limit,
        None => return Ok(length),
    };
    if limit > DIGIT_CAP - 2 {
        return Err(FormatError::TooLong);
    }
    while length < limit {
        out.copy_within(0..length, 1);
        out[0] = b'0';
        length += 1;
    }
    Ok(length)
}

/// Renders `value` into digits (lowest last), upper-casing when asked.
fn radix_digits(mut value: u64, radix: u32, upper: bool, out: &mut [u8; DIGIT_CAP]) -> usize {
    const LOWER: &[u8] = b"0123456789abcdef";
    const UPPER: &[u8] = b"0123456789ABCDEF";
    let table = if upper { UPPER } else { LOWER };
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut count = 0;
    while value > 0 {
        out[count] = table[(value % radix as u64) as usize];
        value /= radix as u64;
        count += 1;
    }
    out[..count].reverse();
    count
}

/// Assembles sign + prefix + digits and pads: `-` left-aligns, a `0`
/// flag (only when no precision was given) zero-fills between the sign
/// and the digits, otherwise spaces lead.
fn emit_padded_integer_prefixed(
    digits: &[u8],
    negative: bool,
    prefix: &[u8],
    flags: &str,
    width: Option<usize>,
    no_precision: bool,
    emit: &mut dyn FnMut(&[u8]) -> bool,
) -> Result<(), FormatError<'static>> {
    let mut sign: &[u8] = b"";
    if negative {
        sign = b"-";
    } else if flags.contains('+') {
        sign = b"+";
    } else if flags.contains(' ') {
        sign = b" ";
    }
    let body_len = sign.len() + prefix.len() + digits.len();
    let width = width.unwrap_or(0);
    let padding = width.saturating_sub(body_len);
    let zero_fill = flags.contains('0') && no_precision && !flags.contains('-');
    let spaces = [b' '; 64];
    let zeros = [b'0'; 64];
    let mut put = |bytes: &[u8]| -> Result<(), FormatError<'static>> {
        if !emit(bytes) {
            return Err(FormatError::WriteFailed);
        }
        Ok(())
    };
    if flags.contains('-') {
        put(sign)?;
        put(prefix)?;
        put(digits)?;
        pad_spaces(padding, &spaces, &mut put)?;
    } else if zero_fill {
        put(sign)?;
        put(prefix)?;
        pad_zeros(padding, &zeros, &mut put)?;
        put(digits)?;
    } else {
        pad_spaces(padding, &spaces, &mut put)?;
        put(sign)?;
        put(prefix)?;
        put(digits)?;
    }
    Ok(())
}

fn emit_padded_integer(
    digits: &[u8],
    negative: bool,
    flags: &str,
    width: Option<usize>,
    no_precision: bool,
    emit: &mut dyn FnMut(&[u8]) -> bool,
) -> Result<(), FormatError<'static>> {
    emit_padded_integer_prefixed(digits, negative, b"", flags, width, no_precision, emit)
}

fn pad_spaces(mut remaining: usize, spaces: &[u8; 64], put: &mut dyn FnMut(&[u8]) -> Result<(), FormatError<'static>>) -> Result<(), FormatError<'static>> {
    while remaining > 0 {
        let take = remaining.min(spaces.len());
        put(&spaces[..take])?;
        remaining -= take;
    }
    Ok(())
}

fn pad_zeros(mut remaining: usize, zeros: &[u8; 64], put: &mut dyn FnMut(&[u8]) -> Result<(), FormatError<'static>>) -> Result<(), FormatError<'static>> {
    while remaining > 0 {
        let take = remaining.min(zeros.len());
        put(&zeros[..take])?;
        remaining -= take;
    }
    Ok(())
}

/// String-style padding: spaces before, or after with `-`.
fn pad_emit(body: &[u8], minus: bool, width: Option<usize>, emit: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), FormatError<'static>> {
    let mut padding = width.unwrap_or(0).saturating_sub(body.len());
    let spaces = [b' '; 64];
    if minus {
        if !emit(body) {
            return Err(FormatError::WriteFailed);
        }
        while padding > 0 {
            let take = padding.min(spaces.len());
            if !emit(&spaces[..take]) {
                return Err(FormatError::WriteFailed);
            }
            padding -= take;
        }
    } else {
        while padding > 0 {
            let take = padding.min(spaces.len());
            if !emit(&spaces[..take]) {
                return Err(FormatError::WriteFailed);
            }
            padding -= take;
        }
        if !emit(body) {
            return Err(FormatError::WriteFailed);
        }
    }
    Ok(())
}

fn emit_pad(mut remaining: usize, emit: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), FormatError<'static>> {
    let spaces = [b' '; 64];
    while remaining > 0 {
        let take = remaining.min(spaces.len());
        if !emit(&spaces[..take]) {
            return Err(FormatError::WriteFailed);
        }
        remaining -= take;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Formats and collects the pieces into one owned buffer.
    fn render(fmt: &str, operands: &[&str]) -> (String, i32) {
        let mut out = Vec::new();
        let mut warn = |_bytes: &[u8]| {};
        let status = format(operands, fmt, &mut |piece: &[u8]| {
            out.extend_from_slice(piece);
            true
        }, &mut warn).unwrap();
        (String::from_utf8_lossy(&out).into_owned(), status)
    }

    #[test]
    fn test_literal_text_passes_through() {
        assert_eq!(render("hello world", &[]), ("hello world".to_string(), 0));
    }

    #[test]
    fn test_percent_percent_is_one_percent() {
        assert_eq!(render("100%%\\n", &[]), ("100%\n".to_string(), 0));
    }

    #[test]
    fn test_decimal_and_string_basics() {
        assert_eq!(render("%d-%s", &["42", "hi"]), ("42-hi".to_string(), 0));
        assert_eq!(render("%i", &["-7"]), ("-7".to_string(), 0));
    }

    #[test]
    fn test_missing_operands_are_zero_and_empty() {
        assert_eq!(render("[%d][%s]", &[]), ("[0][]".to_string(), 0));
    }

    #[test]
    fn test_format_reuses_over_extra_operands() {
        assert_eq!(render("%s-", &["a", "b", "c"]), ("a-b-c-".to_string(), 0));
    }

    #[test]
    fn test_flags_plus_space_and_minus() {
        assert_eq!(render("%+d", &["5"]), ("+5".to_string(), 0));
        assert_eq!(render("% d", &["5"]), (" 5".to_string(), 0));
        assert_eq!(render("%5d|%-5d|", &["42", "42"]), ("   42|42   |".to_string(), 0));
    }

    #[test]
    fn test_zero_flag_zero_fills_against_width() {
        assert_eq!(render("%05d", &["42"]), ("00042".to_string(), 0));
        // A precision disables the zero fill.
        assert_eq!(render("%05.1d", &["42"]), ("   42".to_string(), 0));
    }

    #[test]
    fn test_precision_on_integers() {
        assert_eq!(render("%.5d", &["42"]), ("00042".to_string(), 0));
        // Precision zero renders a zero value as nothing.
        assert_eq!(render("[%d]", &["0"]), ("[0]".to_string(), 0));
        assert_eq!(render("[%.0d]", &["0"]), ("[]".to_string(), 0));
    }

    #[test]
    fn test_unsigned_octal_hex_upper() {
        assert_eq!(render("%u", &["42"]), ("42".to_string(), 0));
        assert_eq!(render("%x", &["255"]), ("ff".to_string(), 0));
        assert_eq!(render("%X", &["255"]), ("FF".to_string(), 0));
        assert_eq!(render("%o", &["8"]), ("10".to_string(), 0));
    }

    #[test]
    fn test_negative_unsigned_rejected() {
        // getuintmax rejects the value by name; the status leaves as 1.
        assert_eq!(render("%u", &["-1"]).1, 1);
    }

    #[test]
    fn test_alternate_form_prefixes() {
        assert_eq!(render("%#x", &["255"]), ("0xff".to_string(), 0));
        assert_eq!(render("%#X", &["255"]), ("0XFF".to_string(), 0));
        assert_eq!(render("%#o", &["8"]), ("010".to_string(), 0));
        assert_eq!(render("%#d", &["8"]), ("8".to_string(), 0));
    }

    #[test]
    fn test_string_width_and_precision() {
        assert_eq!(render("%5s|", &["ab"]), ("   ab|".to_string(), 0));
        assert_eq!(render("%-5s|", &["ab"]), ("ab   |".to_string(), 0));
        assert_eq!(render("%.1s", &["abc"]), ("a".to_string(), 0));
    }

    #[test]
    fn test_char_takes_first_byte() {
        assert_eq!(render("%c%c", &["ab", "cd"]), ("ac".to_string(), 0));
    }

    #[test]
    fn test_star_width_comes_from_operands() {
        assert_eq!(render("%*d|", &["5", "42"]), ("   42|".to_string(), 0));
    }

    #[test]
    fn test_escapes_expand_in_the_format() {
        // Octal escapes take at most three digits: \010 is one byte (0x08)
        // and the trailing `1` is literal text; \x42 is 'B'.
        assert_eq!(
            render("a\\tb\\0101\\x42\\n", &[]),
            ("a\tb\u{8}1B\n".to_string(), 0)
        );
    }

    #[test]
    fn test_c_escape_halts_output() {
        let (out, status) = render("stop\\c after", &[]);
        assert_eq!(out, "stop".to_string());
        assert_eq!(status, 0);
    }

    #[test]
    fn test_unknown_escape_warns_but_passes_through() {
        let (out, status) = render("\\q", &[]);
        assert_eq!(out, "q".to_string());
        assert_eq!(status, 1);
    }

    #[test]
    fn test_numeric_operands_use_base_zero() {
        assert_eq!(render("%d %d %x", &["010", "0x10", "16"]), ("8 16 10".to_string(), 0));
    }

    #[test]
    fn test_character_constant_prefix_operand() {
        assert_eq!(render("%d", &["\"A"]), ("65".to_string(), 0));
    }

    #[test]
    fn test_bad_number_warns_status_one() {
        assert_eq!(render("%d", &["abc"]).1, 1);
        assert_eq!(render("%d", &["12abc"]).1, 1);
    }

    #[test]
    fn test_b_expands_operand_escapes() {
        assert_eq!(render("%b", &["a\\tn"]), ("a\tn".to_string(), 0));
    }

    #[test]
    fn test_float_conversions_are_declared_unsupported() {
        assert!(matches!(
            format(&["1.5"], "%f", &mut |_: &[u8]| true, &mut |_: &[u8]| {}),
            Err(FormatError::FloatNotModelled(_))
        ));
    }

    #[test]
    fn test_missing_format_character_is_an_error() {
        assert!(matches!(
            format(&[], "abc %", &mut |_: &[u8]| true, &mut |_: &[u8]| {}),
            Err(FormatError::MissingFormatCharacter)
        ));
    }
}
