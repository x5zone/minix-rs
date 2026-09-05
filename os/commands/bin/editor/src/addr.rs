//! `ed` address parsing and evaluation.
//!
//! Ground truth: `minix3/bin/ed/main.c` (`extract_addr_range` at line 285,
//! `next_addr` at line 314, range checking in `check_addr_range` at line
//! 898). An `ed` command starts with an optional address range naming the
//! lines it acts on:
//!
//! - `.` the current line, `$` the last line, a number for that line.
//! - `'a` the line of mark `a` (marks are set by `k`, read here through a
//!   mark table the caller owns).
//! - `+n` / `-n` / `+` / `-` offsets from the current line (bare `+` and
//!   `-` mean one step).
//! - `/pattern/` / `?pattern?` search forward / backward (the pattern text
//!   is reported for the search crate; matching stays outside).
//! - Two addresses joined by `,` (whole buffer addressing: missing sides
//!   default to first and last line) or `;` (like `,`, but the current line
//!   moves to the first address before the second is read).
//! - A trailing `+`/`-` run adjusts the second address (`1,2+3`).
//!
//! Evaluation needs the line count, the current line, and the mark table:
//! pure numbers in, line numbers out.

use crate::EditorError;

/// Maximum lowercase marks (`a` to `z`).
pub const MAX_MARKS: usize = 26;

/// One address expression before evaluation: a base plus an accumulated
/// offset (`.-2+3` is base current with offset +1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address {
    /// What the address names before offsets.
    pub base: Base,
    /// Summed `+n`/`-n` adjustments (bare signs count one step).
    pub offset: i32,
}

/// The base of an address expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// `.`: the current line.
    Current,
    /// `$`: the last line.
    Last,
    /// A literal line number (1 based; 0 is rejected at evaluation).
    Number(u32),
    /// `'a`: the line carrying the mark (0 for `a` through 25 for `z`).
    Mark(u8),
    /// `/pattern/`: search forward from the line after current.
    SearchForward,
    /// `?pattern?`: search backward from the line before current.
    SearchBackward,
}

/// One parsed address range: optional first and second address plus whether
/// the separator was `;` (which moves current to the first address).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressRange {
    /// First address, if any.
    pub first: Option<Address>,
    /// Second address, if any.
    pub second: Option<Address>,
    /// True for `;`, false for `,` (or no separator).
    pub semicolon: bool,
}

/// Parse the address range at the start of `text`.
///
/// Returns the range plus how many bytes it consumed, so the caller can
/// continue with the command letter. No address at all yields two empty
/// sides (the command's default range applies later).
pub fn parse_range(text: &str) -> Result<(AddressRange, usize), EditorError> {
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut first = None;
    let mut second = None;
    let mut semicolon = false;
    // A leading address, if the next byte can open one.
    if opens_address(bytes, pos) {
        let (address, next) = parse_address(text, pos)?;
        first = Some(address);
        pos = next;
    }
    // Optional separator plus second address.
    if pos < bytes.len() && (bytes[pos] == b',' || bytes[pos] == b';') {
        semicolon = bytes[pos] == b';';
        pos += 1;
        if opens_address(bytes, pos) {
            let (address, next) = parse_address(text, pos)?;
            second = Some(address);
            pos = next;
        }
    }
    // Trailing offsets adjust the last address named (`1,2+3` names
    // lines 1 through 5: the `+3` belongs to the second address).
    while pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
        let (extra, next) = parse_offset(text, pos)?;
        if let Some(second) = second.as_mut() {
            second.offset = second.offset.saturating_add(extra);
        } else if let Some(first) = first.as_mut() {
            first.offset = first.offset.saturating_add(extra);
        } else {
            // No address yet (`+3p`): base is the current line.
            first = Some(Address {
                base: Base::Current,
                offset: extra,
            });
        }
        pos = next;
    }
    Ok((
        AddressRange {
            first,
            second,
            semicolon,
        },
        pos,
    ))
}

/// True when `text[pos]` can open an address expression.
fn opens_address(bytes: &[u8], pos: usize) -> bool {
    matches!(
        bytes.get(pos),
        Some(b'.' | b'$' | b'\'' | b'/' | b'?' | b'+' | b'-' | b'0'..=b'9')
    )
}

/// Parse one address: a base plus its chained offsets (`.-2+3`).
fn parse_address(text: &str, mut pos: usize) -> Result<(Address, usize), EditorError> {
    let bytes = text.as_bytes();
    let base = match *bytes.get(pos).ok_or(EditorError::InvalidArgument)? {
        b'.' => {
            pos += 1;
            Base::Current
        }
        b'$' => {
            pos += 1;
            Base::Last
        }
        b'\'' => {
            pos += 1;
            let mark = *bytes.get(pos).ok_or(EditorError::InvalidArgument)?;
            if !mark.is_ascii_lowercase() {
                return Err(EditorError::InvalidArgument);
            }
            pos += 1;
            Base::Mark(mark - b'a')
        }
        b'/' => {
            pos = skip_pattern(bytes, pos + 1, b'/')?;
            Base::SearchForward
        }
        b'?' => {
            pos = skip_pattern(bytes, pos + 1, b'?')?;
            Base::SearchBackward
        }
        b'0'..=b'9' => {
            let start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            Base::Number(parse_u32(&text[start..pos])?)
        }
        b'+' | b'-' => Base::Current,
        _ => return Err(EditorError::InvalidArgument),
    };
    let mut offset: i32 = 0;
    while pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
        let (extra, next) = parse_offset(text, pos)?;
        offset = offset.saturating_add(extra);
        pos = next;
    }
    Ok((Address { base, offset }, pos))
}

/// Parse one `+n`/`-n` offset (bare signs mean one step).
fn parse_offset(text: &str, mut pos: usize) -> Result<(i32, usize), EditorError> {
    let bytes = text.as_bytes();
    let sign = if bytes[pos] == b'+' { 1 } else { -1 };
    pos += 1;
    let start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if start == pos {
        Ok((sign, pos))
    } else {
        let magnitude = parse_u32(&text[start..pos])?;
        if magnitude > i32::MAX as u32 {
            return Err(EditorError::InvalidArgument);
        }
        Ok((sign * magnitude as i32, pos))
    }
}

/// Skip a `/pattern/` or `?pattern?` body (backslash escapes the closer),
/// returning the position past it. The pattern text itself is the search
/// crate's business; only its extent matters here.
fn skip_pattern(bytes: &[u8], mut pos: usize, closer: u8) -> Result<usize, EditorError> {
    loop {
        if pos >= bytes.len() {
            return Err(EditorError::InvalidArgument);
        }
        if bytes[pos] == b'\\' {
            pos += 2;
            continue;
        }
        if bytes[pos] == closer {
            return Ok(pos + 1);
        }
        pos += 1;
    }
}

fn parse_u32(text: &str) -> Result<u32, EditorError> {
    if text.is_empty() {
        return Err(EditorError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(EditorError::InvalidArgument)?;
    }
    Ok(value)
}

/// Evaluation context: buffer size, current line, and the mark table.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    /// How many lines the buffer holds.
    pub line_count: usize,
    /// The current line (1 based; 0 when the buffer is empty).
    pub current: usize,
    /// Mark table: `marks[i]` is the line carrying mark `a + i`, if any.
    pub marks: [Option<usize>; MAX_MARKS],
}

impl Context {
    /// An empty buffer context.
    pub fn empty() -> Self {
        Context {
            line_count: 0,
            current: 0,
            marks: [None; MAX_MARKS],
        }
    }
}

/// Evaluate one address to a line number: resolve the base, add the
/// offset, range check the sum.
pub fn evaluate(address: Address, context: &Context) -> Result<usize, EditorError> {
    let base = match address.base {
        Base::Current => context.current,
        Base::Last => context.line_count,
        Base::Number(n) => {
            if n == 0 {
                return Err(EditorError::InvalidArgument);
            }
            n as usize
        }
        Base::Mark(slot) => context.marks[slot as usize].ok_or(EditorError::InvalidArgument)?,
        // Search addresses need the search crate plus buffer access; the
        // parser accepts them, evaluation reports "not wired yet" through
        // the same error channel (loud, not silent).
        Base::SearchForward | Base::SearchBackward => {
            return Err(EditorError::InvalidArgument)
        }
    };
    let line = base as i64 + address.offset as i64;
    if line < 1 || line > context.line_count as i64 {
        return Err(EditorError::InvalidArgument);
    }
    Ok(line as usize)
}

/// Evaluate a range to `(from, to)`, applying the `ed` defaulting rules:
///
/// - No addresses: the command default (passed in as `default`).
/// - One address: that line twice (single line commands).
/// - `,b` (leading comma): lines 1 through `b`.
/// - `a,b`: both evaluated; reversed ranges are rejected here (matching
///   `check_addr_range`: the caller, not the parser, owns the complaint).
/// - `;` moves current to the first address for the second evaluation.
pub fn evaluate_range(
    range: &AddressRange,
    context: &Context,
    default: (usize, usize),
) -> Result<(usize, usize), EditorError> {
    match (range.first, range.second) {
        (None, None) => Ok(default),
        (Some(first), None) => {
            let line = evaluate(first, context)?;
            Ok((line, line))
        }
        (None, Some(second)) => {
            let to = evaluate(second, context)?;
            if to == 0 || to > context.line_count {
                return Err(EditorError::InvalidArgument);
            }
            Ok((1, to))
        }
        (Some(first), Some(second)) => {
            let context = if range.semicolon {
                let line = evaluate(first, context)?;
                Context { current: line, ..*context }
            } else {
                *context
            };
            let from = evaluate(first, &context)?;
            let to = evaluate(second, &context)?;
            if from > to {
                return Err(EditorError::InvalidArgument);
            }
            Ok((from, to))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            line_count: 10,
            current: 4,
            marks: {
                let mut marks = [None; MAX_MARKS];
                marks[0] = Some(7);
                marks
            },
        }
    }

    fn address(base: Base, offset: i32) -> Address {
        Address { base, offset }
    }

    #[test]
    fn test_bare_addresses() {
        let context = context();
        assert_eq!(evaluate(address(Base::Current, 0), &context), Ok(4));
        assert_eq!(evaluate(address(Base::Last, 0), &context), Ok(10));
        assert_eq!(evaluate(address(Base::Number(3), 0), &context), Ok(3));
        assert_eq!(evaluate(address(Base::Mark(0), 0), &context), Ok(7));
        assert_eq!(
            evaluate(address(Base::Mark(1), 0), &context),
            Err(EditorError::InvalidArgument)
        );
        assert_eq!(
            evaluate(address(Base::Number(0), 0), &context),
            Err(EditorError::InvalidArgument)
        );
        assert_eq!(
            evaluate(address(Base::Number(11), 0), &context),
            Err(EditorError::InvalidArgument)
        );
        // Offsets apply on top of the base: `.+2` from line 4 is line 6.
        assert_eq!(evaluate(address(Base::Current, 2), &context), Ok(6));
        assert_eq!(
            evaluate(address(Base::Current, -4), &context),
            Err(EditorError::InvalidArgument)
        );
    }

    #[test]
    fn test_range_parsing() {
        let (range, used) = parse_range("1,5p").unwrap();
        assert_eq!(
            range.first,
            Some(address(Base::Number(1), 0))
        );
        assert_eq!(
            range.second,
            Some(address(Base::Number(5), 0))
        );
        assert!(!range.semicolon);
        assert_eq!(used, 3);
        let (range, _) = parse_range("$-3;$").unwrap();
        assert_eq!(range.first, Some(address(Base::Last, -3)));
        assert_eq!(range.second, Some(address(Base::Last, 0)));
        assert!(range.semicolon);
    }

    #[test]
    fn test_comma_defaults() {
        // `,5` means lines 1 through 5; `,` alone leaves both sides empty
        // for the command default.
        let context = context();
        let (range, _) = parse_range(",5").unwrap();
        assert_eq!((range.first, range.second), (None, Some(address(Base::Number(5), 0))));
        assert_eq!(evaluate_range(&range, &context, (4, 4)), Ok((1, 5)));
        let (range, _) = parse_range(",").unwrap();
        assert_eq!((range.first, range.second), (None, None));
    }

    #[test]
    fn test_trailing_offsets() {
        // `1,2+3` names lines 1 through 5: the offset joins the second
        // address, it never stands alone.
        let (range, _) = parse_range("1,2+3p").unwrap();
        assert_eq!(
            range.second,
            Some(address(Base::Number(2), 3))
        );
        let context = context();
        assert_eq!(evaluate_range(&range, &context, (4, 4)), Ok((1, 5)));
    }

    #[test]
    fn test_chained_offsets() {
        // `.-2+3` is one address: current with a net +1 offset.
        let (range, _) = parse_range(".-2+3p").unwrap();
        assert_eq!(
            range.first,
            Some(address(Base::Current, 1))
        );
    }

    #[test]
    fn test_search_shapes_accepted() {
        let (range, used) = parse_range("/err/p").unwrap();
        assert_eq!(
            range.first,
            Some(address(Base::SearchForward, 0))
        );
        assert_eq!(used, 5);
        let (range, _) = parse_range("?main?d").unwrap();
        assert_eq!(
            range.first,
            Some(address(Base::SearchBackward, 0))
        );
    }

    #[test]
    fn test_range_evaluation() {
        let context = context();
        let (range, _) = parse_range("2,5").unwrap();
        assert_eq!(evaluate_range(&range, &context, (4, 4)), Ok((2, 5)));
        let (range, _) = parse_range("3").unwrap();
        assert_eq!(evaluate_range(&range, &context, (4, 4)), Ok((3, 3)));
        let (range, _) = parse_range("").unwrap();
        assert_eq!(evaluate_range(&range, &context, (4, 4)), Ok((4, 4)));
    }

    #[test]
    fn test_reversed_range_rejected() {
        let context = context();
        let (range, _) = parse_range("5,2").unwrap();
        assert_eq!(
            evaluate_range(&range, &context, (4, 4)),
            Err(EditorError::InvalidArgument)
        );
    }

    #[test]
    fn test_semicolon_moves_current() {
        // `4;+2`: current becomes 4, then +2 names line 6.
        let context = context();
        let (range, _) = parse_range("4;+2").unwrap();
        assert_eq!(evaluate_range(&range, &context, (4, 4)), Ok((4, 6)));
    }
}
