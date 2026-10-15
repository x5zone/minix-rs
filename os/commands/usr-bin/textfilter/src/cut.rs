//! Field list parsing and column selection behind `cut`.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (option string `b:c:d:f:sn` at
//! line 88). Three selection modes share one list grammar (comma separated
//! numbers and `low-high` ranges, 1 based):
//!
//! - `-b`: byte positions.
//! - `-c`: character positions (identical to bytes here; multibyte
//!   awareness is a documented follow-up, matching the engine's byte
//!   orientation).
//! - `-f`: delimiter separated fields with a custom delimiter (`-d`),
//!   suppressing lines without the delimiter unless `-s` is off (default
//!   prints them whole).

use crate::{GetoptReport, TextError};
use alloc::vec::Vec;

/// Maximum ranges in one field list.
pub const MAX_RANGES: usize = 16;

/// One selected interval, 1 based and inclusive on both ends.
/// `high == u32::MAX` means "to the end of the line" (`3-`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// First selected position, 1 based.
    pub low: u32,
    /// Last selected position, inclusive.
    pub high: u32,
}

/// 一份列表里两个端点之外还会招到的两种错，对位 C `get_list()` 的两句 `errx`。
///
/// C 只有这两个名字（`cut.c:196`、`:198`），而且两句都不带值：那句里没有 `%s`，
/// 所以调用方不能把出错的那一段拼回句子。区别只在：残留字符算前一句
///（`if (*p) errx(...)`，`cut.c:195-196`），算出一个零算后一句
///（`if (!stop || !start)`，`:197-198`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListError {
    /// 这一项读不完（残留字符、空项、非数字、溢出）。
    IllegalValue,
    /// 这一项里出现了零（孤立一个 `0`、`0-5`、`3-0`）。
    IncludesZero,
}

/// Parse a `cut` list (`1,3-5,8-`) into up to 16 ranges, sorted by `low`
/// with overlaps kept (overlapping output is deduplicated at selection
/// time, matching the C tool printing each byte once).
pub fn parse_list(text: &str) -> Result<([Range; MAX_RANGES], usize), ListError> {
    if text.is_empty() {
        return Err(ListError::IllegalValue);
    }
    let mut ranges: [Range; MAX_RANGES] = [Range { low: 1, high: 0 }; MAX_RANGES];
    let mut count = 0;
    for part in text.split(',') {
        if count >= MAX_RANGES {
            // C 不封顶：`get_list` 用 `ecalloc` 按 `ALLOC_CHUNK` 长大（`cut.c:161-164`、
            // `:200-208`）。本树的 16 项上限是已登记的声明性限制，它借用 C 的
            // 「非法列表」那一句报出去，差异记在 07 篇的 cut 行。
            return Err(ListError::IllegalValue);
        }
        ranges[count] = parse_part(part)?;
        count += 1;
    }
    // Insertion sort by low (lists are tiny).
    for i in 1..count {
        let mut j = i;
        while j > 0 && ranges[j].low < ranges[j - 1].low {
            ranges.swap(j, j - 1);
            j -= 1;
        }
    }
    Ok((ranges, count))
}

fn parse_part(part: &str) -> Result<Range, ListError> {
    if part.is_empty() {
        return Err(ListError::IllegalValue);
    }
    match part.split_once('-') {
        None => {
            let n = parse_number(part)?;
            Ok(Range { low: n, high: n })
        }
        Some((low_text, high_text)) => {
            if low_text.is_empty() {
                // `-N`: from the start through N.
                Ok(Range {
                    low: 1,
                    high: parse_number(high_text)?,
                })
            } else if high_text.is_empty() {
                // `N-`: from N to the end of the line.
                Ok(Range {
                    low: parse_number(low_text)?,
                    high: u32::MAX,
                })
            } else {
                let low = parse_number(low_text)?;
                let high = parse_number(high_text)?;
                if low > high {
                    // C 这里不报错（它「比 POSIX 草案宽」，`:171-174` 的注释），
                    // 只是选出空集合；本树先按已登记的差异拒，差异记在 07 篇。
                    return Err(ListError::IllegalValue);
                }
                Ok(Range { low, high })
            }
        }
    }
}

fn parse_number(text: &str) -> Result<u32, ListError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ListError::IllegalValue);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(ListError::IllegalValue)?;
    }
    if value == 0 {
        return Err(ListError::IncludesZero);
    }
    Ok(value)
}

/// Select byte ranges from `line` (1 based positions), writing into `out`.
///
/// Returns the used byte count. Overlapping ranges print each byte once.
pub fn select_bytes(
    line: &str,
    ranges: &[Range],
    out: &mut [u8],
) -> Result<usize, TextError> {
    let bytes = line.as_bytes();
    let mut written = 0;
    let mut next = 1u32;
    for range in ranges {
        let low = range.low.max(next);
        let high = range.high.min(bytes.len() as u32);
        if low <= high {
            for index in (low - 1)..high {
                if written >= out.len() {
                    return Err(TextError::TooLong);
                }
                out[written] = bytes[index as usize];
                written += 1;
            }
            next = high + 1;
        }
    }
    Ok(written)
}

/// Select delimiter separated fields from `line`.
///
/// Fields are 1 based; `ranges` reuse the same grammar. Lines without the
/// delimiter print whole unless `suppress` (`-s`) is set. The delimiter is
/// one byte.
pub fn select_fields(
    line: &str,
    ranges: &[Range],
    delimiter: u8,
    suppress: bool,
    out: &mut [u8],
) -> Result<usize, TextError> {
    if !line.as_bytes().contains(&delimiter) {
        if suppress {
            return Ok(0);
        }
        if line.len() > out.len() {
            return Err(TextError::TooLong);
        }
        out[..line.len()].copy_from_slice(line.as_bytes());
        return Ok(line.len());
    }
    let bytes = line.as_bytes();
    let mut written = 0;
    let mut field: u32 = 1;
    let mut start = 0;
    let mut first = true;
    let mut pos = 0;
    while pos <= bytes.len() {
        let end = pos == bytes.len() || bytes[pos] == delimiter;
        if end {
            if in_ranges(ranges, field) {
                if !first {
                    if written >= out.len() {
                        return Err(TextError::TooLong);
                    }
                    out[written] = delimiter;
                    written += 1;
                }
                first = false;
                for byte in &bytes[start..pos] {
                    if written >= out.len() {
                        return Err(TextError::TooLong);
                    }
                    out[written] = *byte;
                    written += 1;
                }
            }
            field += 1;
            start = pos + 1;
        }
        pos += 1;
    }
    Ok(written)
}

fn in_ranges(ranges: &[Range], field: u32) -> bool {
    ranges.iter().any(|range| range.low <= field && field <= range.high)
}

/// Parsed `cut` command line: the byte (`-b`/`-c`) and field (`-f`) list
/// selectors, the `-d` delimiter, the `-s` suppress flag, and file operands
/// (a bare `-` names standard input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutArgs<'a> {
    pub bytes_list: Option<&'a str>,
    pub fields_list: Option<&'a str>,
    pub delimiter: u8,
    /// `-d` 是否真被给过。C 把它单独记在 `dflag` 里，而 `cut.c:124` 的那道
    /// post-check 问的是旗标而不是值：给了字节列表又给 `-d` 就是用法错。
    pub delimiter_given: bool,
    pub suppress: bool,
    /// `-s` 是否真被给过（`sflag`，同一道 post-check 的另一半）。
    pub suppress_given: bool,
    pub operands: Vec<&'a str>,
}

/// `cut` 参数错。每个臂对应 C 的一条真出路（调用者负责渲染），而不是「都是用法错」：
/// - [`CutError::Usage`]：C 的四道 post-check（`cut.c:120-128`），全部直走 `usage()`，
///   **前面没有 getopt 那一行**。
/// - [`CutError::Getopt`]：集外字母与缺值，getopt 先印一行再走 usage。
/// - [`CutError::IllegalList`] / [`CutError::ZeroInList`]：C `get_list()` 的两句 `errx`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutError {
    Usage,
    Getopt(GetoptReport),
    IllegalList,
    ZeroInList,
}

/// 把列表解析的错归到 C 的那两句上。壳在取范围时也会走一次（那里已不可达，
/// 但句子只住在这里与 `arg_error`，不重复写）。
impl From<ListError> for CutError {
    fn from(error: ListError) -> Self {
        match error {
            ListError::IllegalValue => CutError::IllegalList,
            ListError::IncludesZero => CutError::ZeroInList,
        }
    }
}

/// Fold `cut`'s argv tail faithfully to BSD `getopt("b:c:d:f:sn")` (`cut.c:88`):
/// `-b`/`-c`/`-f`/`-d` take a value attached (`-f1-3`, `-d:`) or as the next
/// word; `-s`/`-n` are flags (so `-sf1-3` is `-s` then `-f 1-3`, and `-sn`/`-ns`
/// both set suppress); `--` ends option scanning; any other dash letter is the
/// libc report, never a leaked operand. `-b`/`-c` set the byte list (they
/// coincide here — the selection is byte-grained).
///
/// 两个时刻不能合并：列表的**合法性**在选项环里就地查（C 的 `get_list(optarg)` 就在
/// `case 'b'`/`case 'c'`/`case 'f'` 里，`cut.c:90-108`），所以一个非法列表抢在后面的
/// 集外字母之前报错；而**选哪一类**的冲突到环外才查（`:120-128`），所以
/// `cut -b 1 -f 2 -x` 报的是 `-x` 而不是冲突。
pub fn parse_cut_args<'a>(args: &'a [&'a str]) -> Result<CutArgs<'a>, CutError> {
    let mut bytes_list: Option<&'a str> = None;
    let mut fields_list: Option<&'a str> = None;
    let mut delimiter: u8 = b'\t';
    let mut delimiter_given = false;
    let mut suppress = false;
    let mut suppress_given = false;
    let mut operands: Vec<&'a str> = Vec::new();

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'b' | b'c' | b'f' => {
                        let letter = bytes[pos];
                        let value = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            match args.get(index) {
                                Some(v) => *v,
                                // C：optstring 不以 `:` 开头，所以缺值时 getopt 先印
                                // 一句再返回 `'?'`（`getopt.c:132-136`）。
                                None => {
                                    return Err(CutError::Getopt(GetoptReport::MissingValue(
                                        letter as char,
                                    )));
                                }
                            }
                        };
                        // C 的 `get_list(optarg)` 就在这个 case 里跑（`cut.c:91/96/105`），
                        // 所以下面那两道 post-check 之前先有可能报错。
                        parse_list(value).map_err(CutError::from)?;
                        if letter == b'f' {
                            fields_list = Some(value);
                        } else {
                            bytes_list = Some(value);
                        }
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    b'd' => {
                        let value = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            match args.get(index) {
                                Some(v) => *v,
                                None => {
                                    return Err(CutError::Getopt(GetoptReport::MissingValue('d')));
                                }
                            }
                        };
                        // C `cut.c:101-102` 只有一句 `dchar = *optarg`：取首字节，
                        // 多了不用也不报错，空串就取到那个 `\0`。
                        delimiter = value.bytes().next().unwrap_or(0);
                        delimiter_given = true;
                        pos = bytes.len();
                    }
                    b's' => {
                        suppress = true;
                        suppress_given = true;
                        pos += 1;
                    }
                    // `-n`: accepted and ignored for compatibility
                    // (`cut.c:112-113` `case 'n': break`), never a leaked operand.
                    b'n' => pos += 1,
                    _ => {
                        return Err(CutError::Getopt(GetoptReport::UnknownOption(
                            bytes[pos] as char,
                        )));
                    }
                }
            }
            index += 1;
            continue;
        }
        operands.push(arg);
        index += 1;
    }
    // C `cut.c:120-128` 的四道 post-check，四支都走同一个 `usage()`（没有 getopt 前置行）：
    //   if (fflag) { if (cflag || bflag) usage(); }
    //   else if ((!cflag && !bflag) || dflag || sflag) usage();
    //   else if (bflag && cflag) usage();
    // 本实现里 `-b` 与 `-c` 同义（选择按字节粒度），所以第三道不可达：
    // 它需要的「bflag 与 cflag 各自成立」在本树没法表达，差异记在 07 篇的 cut 行。
    if fields_list.is_some() {
        if bytes_list.is_some() {
            return Err(CutError::Usage);
        }
    } else if bytes_list.is_none() || delimiter_given || suppress_given {
        return Err(CutError::Usage);
    }
    Ok(CutArgs {
        bytes_list,
        fields_list,
        delimiter,
        delimiter_given,
        suppress,
        suppress_given,
        operands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_of(line: &str, list: &str) -> String {
        let (ranges, count) = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_bytes(line, &ranges[..count], &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    fn fields_of(line: &str, list: &str, delimiter: u8, suppress: bool) -> String {
        let (ranges, count) = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_fields(line, &ranges[..count], delimiter, suppress, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_byte_selection() {
        assert_eq!(bytes_of("hello", "2-4"), "ell");
        assert_eq!(bytes_of("hello", "1,5"), "ho");
        assert_eq!(bytes_of("hello", "3-"), "llo");
        assert_eq!(bytes_of("hello", "-2"), "he");
    }

    #[test]
    fn test_overlaps_print_once() {
        assert_eq!(bytes_of("hello", "1-3,2-4"), "hell");
    }

    #[test]
    fn test_field_selection() {
        assert_eq!(fields_of("a:b:c", "2", b':', false), "b");
        assert_eq!(fields_of("a:b:c", "1,3", b':', false), "a:c");
        assert_eq!(fields_of("a:b:c", "2-", b':', false), "b:c");
    }

    #[test]
    fn test_missing_delimiter_policy() {
        assert_eq!(fields_of("abc", "1", b':', false), "abc");
        assert_eq!(fields_of("abc", "1", b':', true), "");
    }

    #[test]
    fn test_bad_lists_rejected() {
        assert!(parse_list("").is_err());
        assert!(parse_list("0").is_err());
        assert!(parse_list("5-2").is_err());
        assert!(parse_list("1,,2").is_err());
    }

    #[test]
    fn test_parse_cut_args_faithful_getopt() {
        // Defaults: a field list with attached and next-word values.
        let a = parse_cut_args(&["-f", "1-3", "file"]).unwrap();
        assert_eq!(a.fields_list, Some("1-3"));
        assert_eq!(a.operands, vec!["file"]);
        assert_eq!(a.delimiter, b'\t');
        // Attached list value (regression vs whole-token leak).
        assert_eq!(
            parse_cut_args(&["-f1-3", "file"]).unwrap().fields_list,
            Some("1-3")
        );
        // KEY regression: a flag clustered with a value option (`-sf1-3`).
        let s = parse_cut_args(&["-sf1-3", "file"]).unwrap();
        assert!(s.suppress);
        assert_eq!(s.fields_list, Some("1-3"));
        // `-sn` and `-ns` both set suppress (`-n` is the compat no-op).
        assert!(parse_cut_args(&["-sn", "-f1"]).unwrap().suppress);
        assert!(parse_cut_args(&["-ns", "-f1"]).unwrap().suppress);
        // `-d` attached and separate; `-b`/`-c` set the byte list.
        assert_eq!(parse_cut_args(&["-d:", "-f1"]).unwrap().delimiter, b':');
        assert_eq!(parse_cut_args(&["-d", ",", "-f1"]).unwrap().delimiter, b',');
        assert_eq!(
            parse_cut_args(&["-c1-2", "file"]).unwrap().bytes_list,
            Some("1-2")
        );
        // `--` ends scanning; bare `-` is an operand.
        assert_eq!(
            parse_cut_args(&["-f1", "--", "-x"]).unwrap().operands,
            vec!["-x"]
        );
        assert_eq!(parse_cut_args(&["-f1", "-"]).unwrap().operands, vec!["-"]);
        // 集外字母与缺值带的是 libc 那一行（句子单源在 `GetoptReport`）。
        assert_eq!(
            parse_cut_args(&["-x", "file"]).err(),
            Some(CutError::Getopt(GetoptReport::UnknownOption('x')))
        );
        assert_eq!(
            parse_cut_args(&["-f"]).err(),
            Some(CutError::Getopt(GetoptReport::MissingValue('f')))
        );
        assert_eq!(
            parse_cut_args(&["-d"]).err(),
            Some(CutError::Getopt(GetoptReport::MissingValue('d')))
        );
        // 两类列表并存是**环外**的 post-check（`cut.c:120-122`），报的是裸 usage：
        // 前面没有 libc 那一行。旧写法把这个当环内的 `Conflict` 抢先报，所以
        // `cut -b 1 -f 2 -x` 会报冲突而不是报 `-x`。
        assert_eq!(parse_cut_args(&["-b1", "-f1"]).err(), Some(CutError::Usage));
        assert_eq!(parse_cut_args(&["-f1", "-b1"]).err(), Some(CutError::Usage));
        // `-d`/`-s` 只能配 `-f`（`cut.c:124` 的 `dflag || sflag`）：给字节列表
        // 再给它们也是用法错。旧写法两条都静默接受。
        assert_eq!(parse_cut_args(&["-c1", "-d,"]).err(), Some(CutError::Usage));
        assert_eq!(parse_cut_args(&["-c1", "-s"]).err(), Some(CutError::Usage));
        // 一个列表也没有 → `(!cflag && !bflag)` 那一支。
        assert_eq!(parse_cut_args(&["-s"]).err(), Some(CutError::Usage));
        // C 的 `dchar = *optarg` 只取首字节，多了不用也不报错（`cut.c:101-102`）；
        // 旧写法把多字节当成一个专门的错。
        assert_eq!(parse_cut_args(&["-dab", "-f1"]).unwrap().delimiter, b'a');
        assert_eq!(parse_cut_args(&["-d", "", "-f1"]).unwrap().delimiter, 0);
        // 列表合法性在选项环里就地查（C 的 `get_list` 就坐在那个 case 里），所以
        // 非法列表抢在后面的集外字母之前报错。
        assert_eq!(
            parse_cut_args(&["-f", "0", "-x"]).err(),
            Some(CutError::ZeroInList)
        );
        assert_eq!(
            parse_cut_args(&["-f", "a", "-x"]).err(),
            Some(CutError::IllegalList)
        );
    }
}
