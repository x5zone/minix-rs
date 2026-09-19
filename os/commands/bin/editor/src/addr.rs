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
    /// `/pattern/`: search forward from the line after current. The
    /// pattern is a byte span into the command line (still alive at
    /// evaluation time); an empty span is the empty pattern `//`, which
    /// reuses the previous one (C `get_compiled_pattern`'s `expr` cache,
    /// re.c:59).
    SearchForward(PatternRef),
    /// `?pattern?`: search backward from the line before current.
    SearchBackward(PatternRef),
}

/// 模式字节区间：指向命令行文本里的模式体。`Address` 保持 `Copy`，区间
/// 随命令行存活，求值时按引用取字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternRef {
    /// 模式体在命令行里的起点（`/` 或 `?` 之后）。
    pub start: u16,
    /// 模式体字节数（0 = 空模式，复用上一模式）。
    pub len: u16,
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
    if bytes.get(pos) == Some(&b'%') {
        // `%` as the first character names the whole buffer: the C
        // extractor turns it into first = 1, second = `$`
        // (`main.c:365-373`, the `case '%'` arm shares the `,`/`;` body
        // with `second_addr = 1; addr = addr_last`). Only offsets may
        // follow; a separator would re-open the range.
        pos += 1;
        first = Some(Address { base: Base::Number(1), offset: 0 });
        second = Some(Address { base: Base::Last, offset: 0 });
    } else if opens_address(bytes, pos) {
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
            let (span, next) = scan_pattern(bytes, pos + 1, b'/')?;
            pos = next;
            Base::SearchForward(span)
        }
        b'?' => {
            let (span, next) = scan_pattern(bytes, pos + 1, b'?')?;
            pos = next;
            Base::SearchBackward(span)
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

/// Scan a `/pattern/` or `?pattern?` body, returning the pattern span and
/// the position past it.
///
/// C `extract_pattern`（re.c:88-130）：扫描到未转义的定界符或行尾；`\\`
/// 跳过下一字节（转义后的定界符不终止），`[` 的平衡由 `parse_char_class`
/// 把守（未闭合 "unbalanced brackets"）；模式字节**原样保留**（转义交给
/// 正则引擎），行尾的结束定界符可省（C 是"是定界符才吃"，next_addr:356）。
pub fn scan_pattern(bytes: &[u8], pos: usize, closer: u8) -> Result<(PatternRef, usize), EditorError> {
    let start = pos;
    let mut at = pos;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => {
                at += 2;
                if at > bytes.len() {
                    // C 的 "trailing backslash (\\)"（re.c:110-113）。
                    return Err(EditorError::InvalidArgument);
                }
            }
            b'[' => {
                // `parse_char_class`（re.c:118-133）：`^` 与首个 `]` 可字面，
                // `[:`/`[.`/`[=` 三类名直到成对收尾；扫不到 `]` 即未闭合。
                let mut inner = at + 1;
                if bytes.get(inner) == Some(&b'^') {
                    inner += 1;
                }
                if bytes.get(inner) == Some(&b']') {
                    inner += 1;
                }
                while inner < bytes.len() && bytes[inner] != b']' {
                    if bytes[inner] == b'['
                        && matches!(bytes.get(inner + 1), Some(b':') | Some(b'.') | Some(b'='))
                    {
                        let d = bytes[inner + 1];
                        inner += 2;
                        while inner < bytes.len()
                            && !(bytes[inner] == b']' && bytes.get(inner.wrapping_sub(1)) == Some(&d))
                        {
                            inner += 1;
                        }
                    }
                    inner += 1;
                }
                if inner >= bytes.len() {
                    return Err(EditorError::InvalidArgument);
                }
                at = inner + 1;
            }
            c if c == closer => {
                return Ok((
                    PatternRef { start: start as u16, len: (at - start) as u16 },
                    at + 1,
                ));
            }
            _ => at += 1,
        }
    }
    Ok((PatternRef { start: start as u16, len: (at - start) as u16 }, at))
}

/// 取模式字节（区间由本模块的扫描产出，越界即编程错误——用断言拒绝）。
pub fn pattern_bytes(line: &str, span: PatternRef) -> &[u8] {
    let start = span.start as usize;
    let end = start + span.len as usize;
    &line.as_bytes()[start..end]
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
        // 搜索基需要缓冲与正则库：走 [`evaluate_with`]。
        Base::SearchForward(_) | Base::SearchBackward(_) => {
            return Err(EditorError::InvalidArgument)
        }
    };
    let line = base as i64 + address.offset as i64;
    if line < 1 || line > context.line_count as i64 {
        return Err(EditorError::InvalidArgument);
    }
    Ok(line as usize)
}

/// 一个地址的非搜索基（数字、标记、当前、末行）到行的解析。
fn plain_base(base: Base, context: &Context) -> Result<usize, EditorError> {
    match base {
        Base::Current => Ok(context.current),
        Base::Last => Ok(context.line_count),
        Base::Number(n) => {
            if n == 0 {
                return Err(EditorError::InvalidArgument);
            }
            Ok(n as usize)
        }
        Base::Mark(slot) => context.marks[slot as usize].ok_or(EditorError::InvalidArgument),
        Base::SearchForward(_) | Base::SearchBackward(_) => Err(EditorError::InvalidArgument),
    }
}

/// 搜索求值的对外缝：缓冲访问与匹配都归调用方（`exec` 持 store 与正则
/// 库），`addr` 只管"求到基行之后"的偏移与界检查。
///
/// `Ok(None)` = 整缓冲绕一圈无匹配——C `get_matching_node_addr` 的
/// "no match"（main.c:938）由调用方折成自己的错误消息；`Err` 是求值本身
/// 失败（空缓冲、坏模式）。
pub trait SearchProbe {
    fn find_line(&mut self, pattern: &[u8], forward: bool) -> Result<Option<usize>, EditorError>;
}

/// 拒绝搜索基的占位实现：纯数字调用方（测试、不求值的命令）用它保持
/// 旧签名——搜索基照旧走"invalid address"通道。
pub struct NoSearch;

impl SearchProbe for NoSearch {
    fn find_line(&mut self, _pattern: &[u8], _forward: bool) -> Result<Option<usize>, EditorError> {
        Err(EditorError::InvalidArgument)
    }
}

/// [`evaluate`] 的带搜索版：`line` 是模式区间所指的命令行原文。
pub fn evaluate_with(
    address: Address,
    context: &Context,
    line: &str,
    probe: &mut dyn SearchProbe,
) -> Result<usize, EditorError> {
    let base = match address.base {
        Base::SearchForward(span) => {
            probe.find_line(pattern_bytes(line, span), true)?.ok_or(EditorError::NoMatch)?
        }
        Base::SearchBackward(span) => {
            probe.find_line(pattern_bytes(line, span), false)?.ok_or(EditorError::NoMatch)?
        }
        other => plain_base(other, context)?,
    };
    let line_no = base as i64 + address.offset as i64;
    if line_no < 1 || line_no > context.line_count as i64 {
        return Err(EditorError::InvalidArgument);
    }
    Ok(line_no as usize)
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
    // 空行文本配 `NoSearch`：搜索基照旧被拒，纯数字路径与旧行为一致。
    evaluate_range_with(range, context, default, "", &mut NoSearch)
}

/// [`evaluate_range`] 的带搜索版：搜索地址经 `probe` 在缓冲上求值。
pub fn evaluate_range_with(
    range: &AddressRange,
    context: &Context,
    default: (usize, usize),
    line: &str,
    probe: &mut dyn SearchProbe,
) -> Result<(usize, usize), EditorError> {
    match (range.first, range.second) {
        (None, None) => Ok(default),
        (Some(first), None) => {
            let line_no = evaluate_with(first, context, line, probe)?;
            Ok((line_no, line_no))
        }
        (None, Some(second)) => {
            let to = evaluate_with(second, context, line, probe)?;
            if to == 0 || to > context.line_count {
                return Err(EditorError::InvalidArgument);
            }
            Ok((1, to))
        }
        (Some(first), Some(second)) => {
            let context = if range.semicolon {
                let first_line = evaluate_with(first, context, line, probe)?;
                Context { current: first_line, ..*context }
            } else {
                *context
            };
            let from = evaluate_with(first, &context, line, probe)?;
            let to = evaluate_with(second, &context, line, probe)?;
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

    fn span(start: u16, len: u16) -> PatternRef {
        PatternRef { start, len }
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
    fn test_percent_names_whole_buffer() {
        // `%` as the first character is the whole buffer: the C extractor
        // turns it into first = 1, second = `$` (`main.c:365-373`).
        let (range, used) = parse_range("%n").unwrap();
        assert_eq!(range.first, Some(address(Base::Number(1), 0)));
        assert_eq!(range.second, Some(address(Base::Last, 0)));
        assert_eq!(used, 1);
        let mut context = context();
        context.line_count = 7;
        assert_eq!(evaluate_range(&range, &context, (1, 1)), Ok((1, 7)));
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
        assert_eq!(range.first, Some(address(Base::SearchForward(span(1, 3)), 0)));
        assert_eq!(pattern_bytes("/err/p", span(1, 3)), b"err");
        assert_eq!(used, 5);
        let (range, _) = parse_range("?main?d").unwrap();
        assert_eq!(range.first, Some(address(Base::SearchBackward(span(1, 4)), 0)));
        assert_eq!(pattern_bytes("?main?d", span(1, 4)), b"main");
    }

    #[test]
    fn test_search_pattern_edges() {
        // 空模式（`//`）记零长区间；转义后的定界符不终止；行尾省略结束
        // 定界符（C next_addr:356 是定界符才吃）；尾偏移仍归地址。
        let (range, _) = parse_range("//+1").unwrap();
        assert_eq!(range.first, Some(address(Base::SearchForward(span(1, 0)), 1)));
        let (range, _) = parse_range("/a\\/b/p").unwrap();
        assert_eq!(range.first, Some(address(Base::SearchForward(span(1, 4)), 0)));
        assert_eq!(pattern_bytes("/a\\/b/p", span(1, 4)), b"a\\/b");
        let (range, used) = parse_range("?x").unwrap();
        assert_eq!(range.first, Some(address(Base::SearchBackward(span(1, 1)), 0)));
        assert_eq!(used, 2);
        // 未闭合字符类（C re.c:120 "unbalanced brackets"）。
        assert_eq!(parse_range("/[ab/p").map(|_| ()), Err(EditorError::InvalidArgument));
        // 行尾孤立反斜杠（C re.c:110 "trailing backslash"）。
        assert_eq!(parse_range("/a\\").map(|_| ()), Err(EditorError::InvalidArgument));
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
