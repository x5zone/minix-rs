//! Field list parsing and column selection behind `cut`.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (option string `b:c:d:f:sn` at
//! line 88). Three selection modes share one list grammar: numbers and
//! `low-high` ranges, 1 based, separated by any of comma, space and tab
//! (`strtok(list, ", \t")`, `cut.c:175`), with empty items collapsed and no
//! cap on the item count:
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
    /// 这一项读不完（残留字符、非数字）。
    IllegalValue,
    /// 这一项里出现了零（孤立一个 `0`、`0-5`、`5-0`）。
    IncludesZero,
}

/// Parse a `cut` list (`1,3-5,8-`) into ranges, sorted by `low` with overlaps
/// kept (overlapping output is deduplicated at selection time, matching the C
/// tool printing each byte once).
///
/// 四个形状是本轮（批次二百八十六）按 C 重定的，旧写法把它们都拒了：
/// **分隔符集是 `,`、空格、制表符三个**，而且空项被折叠（`cut.c:175` 用的是
/// `strtok(list, ", \t")`），所以 `1,,2` 与 `1 2` 都是两项；**空列表不是错**
/// （`strtok` 直接交 NULL，循环体一次也跑不到，于是一个位置也不选）；
/// `5-2` 不查大小关系（`:209-210` 只是 `start++ <= stop` 地置位，倒序就一个也不置）；
/// 项数也不封顶（`:163-164` 初次分配、`:199-206` 按 `ALLOC_CHUNK = _POSIX2_LINE_MAX`
/// 长大，`:153`）。`-3-5` 那类的 autostart/autostop 是跨项、跨行的全局量
///（`:183-184`、`:191-192`、`:213-219`），本树只建模到单项可解析、单区间可选择的
/// 程度，剩下的记为下一项。
pub fn parse_list(text: &str) -> Result<Vec<Range>, ListError> {
    let mut ranges: Vec<Range> = Vec::new();
    for token in text.split([' ', '\t', ',']) {
        if token.is_empty() {
            // strtok 的空项折叠：`1,,2` 与 ` 1 ` 都不报错。
            continue;
        }
        ranges.push(parse_token(token)?);
    }
    // 插入排序语义不变（C 的 positions 位图天然有序）；列表本来就小。
    ranges.sort_by_key(|range| range.low);
    Ok(ranges)
}

/// 一项，逐字对位 `cut.c:176-198` 的那三段：前置一个 `-` 是 C 的 setautostart 形，
/// `N-` 是靠 autostop 打到行尾。C 在 `*p == '-'` 上写了**两个**判据（`:186`、`:189`），
/// 第二个测的是同一个位置——只有 `isdigit(p[1])` 成立、`strtol` 真的跑了，指针才前进
///（`:187-188`）；否则指针原地不动，于是第二个判据又命中同一个 `-` 并 `++p`（`:190`）。
/// 把这个双层结构压成「跨过 `-` 再读一个数」就会反：`5--` 在 C 是残留字符（
/// `illegal list value`），压平后却会读成零那句；`1-2-` 在 C 是**接受**的（autostop
/// 那一支把行尾整段照打），压平后会误报残留。leftover 与零两句的先后也不许换：
/// 残留先判（`:195-196`），零后判（`:197-198`），所以 `0-` 走零那句、`x` 走残留那句。
fn parse_token(token: &str) -> Result<Range, ListError> {
    let bytes = token.as_bytes();
    let mut cursor = 0usize;
    let auto_start = bytes.first() == Some(&b'-');
    if auto_start {
        cursor = 1;
    }
    // `if (isdigit((unsigned char)*p)) start = stop = strtol(p, &p, 10)`（`:181-182`）：
    // 一个数字也没有时两个端点都留在 0，指针也不动。
    let (start, next) = scan_number(token, cursor);
    cursor = next;
    let mut stop = start;
    let mut open_ended = false;
    if bytes.get(cursor) == Some(&b'-') {
        let after = cursor + 1;
        // 里层第一判据 `if (isdigit(p[1])) stop = strtol(p + 1, &p, 10)`（`:187-188`）。
        // 注意它**无条件覆盖** stop，包括把它写成 0（`5-0` 因此落进零那句）。
        let (value, scanned) = scan_number(token, after);
        if scanned > after {
            stop = value;
            cursor = scanned;
        }
        // 里层第二判据 `if (*p == '-') { ++p; … }`（`:189-193`）：`strtol` 没跑时它命中
        // 的还是同一个 `-`。走到行尾就是 C 的 autostop，本树用 high 到顶来表示「打到行尾」
        // 的效果（`x_cut.c:80-82` 那句「把 maxval 之后的原样打出」等价于此）。
        if bytes.get(cursor) == Some(&b'-') {
            cursor += 1;
            if cursor == bytes.len() {
                open_ended = true;
            }
        }
    }
    if cursor != bytes.len() {
        return Err(ListError::IllegalValue);
    }
    if start == 0 || stop == 0 {
        return Err(ListError::IncludesZero);
    }
    Ok(Range {
        low: if auto_start { 1 } else { start },
        high: if open_ended { u32::MAX } else { stop },
    })
}

/// C 那句 `strtol(p, &p, 10)`（`:182`、`:188`）：一个数字也没转出来时值为 0 且
/// 指针不动（对应 C 的 `if (isdigit(…))` 不成立）。转得出来但超出 `u32` 时本树
/// 饱和到最大值，而 C 会拿到 `LONG_MAX` 并把 `positions` 重开到那个尺寸（`:199-206`）
/// 直到分配失败——那一形不建模，饱和与 C 同为「不报句子」。
fn scan_number(token: &str, from: usize) -> (u32, usize) {
    let bytes = token.as_bytes();
    let mut cursor = from;
    let mut value: u32 = 0;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(bytes[cursor] - b'0'));
        cursor += 1;
    }
    (value, cursor)
}

/// Select byte ranges from `line` (1 based positions), writing into `out`.
///
/// Returns the used byte count. Overlapping ranges print each byte once.
pub fn select_bytes(
    line: &[u8],
    ranges: &[Range],
    out: &mut [u8],
) -> Result<usize, TextError> {
    let bytes = line;
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
/// Fields are 1 based; `ranges` reuse the same grammar. The delimiter is one
/// byte. The return says **how much of this line exists at all**: `Some(len)`
/// is the selected bytes, and the caller terminates them as it terminates any
/// output line; `None` means this line has no delimiter and `-s` is set, so C
/// skips it whole — `cut.c:248` only hands the line to `fwrite` when `!sflag`,
/// and `:253-254`'s `if (!isdelim) continue;` then jumps past the
/// `putchar('\n')` at `:279`, so the line contributes **zero bytes**, not an
/// empty line. A line
/// that does carry the delimiter but selects nothing is the opposite case and
/// stays `Some(0)` (参照件对 `-f 9 -s` 的 `a\tb\tc` 交回一枚换行).
pub fn select_fields(
    line: &[u8],
    ranges: &[Range],
    delimiter: u8,
    suppress: bool,
    out: &mut [u8],
) -> Result<Option<usize>, TextError> {
    if !line.contains(&delimiter) {
        if suppress {
            return Ok(None);
        }
        if line.len() > out.len() {
            return Err(TextError::TooLong);
        }
        out[..line.len()].copy_from_slice(line);
        return Ok(Some(line.len()));
    }
    let bytes = line;
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
    Ok(Some(written))
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
        let ranges = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_bytes(line.as_bytes(), &ranges, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    /// 取列的腿数的是字节：C 把 `-b` 单独编成一份 `b_cut`，它的取放宏就是 `getc`/`putchar`
    /// （`minix3/usr.bin/cut/x_cut.c:45-50`；同一文件的 `c_cut` 腿才是 `getwc`/`putwchar`，
    /// `:52-57`）。所以一枚解不出 UTF-8 的字节与任何别的字节一样占一格，也必须原样交回。
    #[test]
    fn a_byte_that_is_not_utf8_occupies_one_position() {
        let ranges = [Range { low: 2, high: 2 }];
        let mut out = [0u8; 8];
        let len = select_bytes(b"a\xffb", &ranges, &mut out).unwrap();
        assert_eq!(&out[..len], b"\xff");
        // 行里没有分隔符且 `-s` 关着：整行搬运，那枚字节照原样带着走。
        let mut copied = [0u8; 8];
        let whole = select_fields(b"a\xffb", &ranges, b'\t', false, &mut copied)
            .unwrap()
            .expect("整行搬运，不是跳过这一行");
        assert_eq!(&copied[..whole], b"a\xffb");
    }

    /// `None` 交回的是「这一行整行没写」，与空串（写了、但选不出字段）不是同一件事。
    fn fields_of(line: &str, list: &str, delimiter: u8, suppress: bool) -> Option<String> {
        let ranges = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_fields(line.as_bytes(), &ranges, delimiter, suppress, &mut out).unwrap()?;
        Some(String::from_utf8_lossy(&out[..len]).into_owned())
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
        assert_eq!(fields_of("a:b:c", "2", b':', false).as_deref(), Some("b"));
        assert_eq!(
            fields_of("a:b:c", "1,3", b':', false).as_deref(),
            Some("a:c")
        );
        assert_eq!(
            fields_of("a:b:c", "2-", b':', false).as_deref(),
            Some("b:c")
        );
    }

    #[test]
    fn test_missing_delimiter_policy() {
        assert_eq!(fields_of("abc", "1", b':', false).as_deref(), Some("abc"));
        // `-s` 且这一行没有分隔符：整行跳过，一个字节都不交（`cut.c:248`/`:253-254`）。
        assert_eq!(fields_of("abc", "1", b':', true), None);
    }

    /// 参照件读数 `tmp/cut-428/ref-s.log`：`-s` 吞的是「没有分隔符的那一行」，
    /// 「有分隔符而选不出字段」那一行仍交一枚换行——两形在库里必须是两个值。
    #[test]
    fn a_suppressed_line_is_absent_rather_than_empty() {
        let ranges = parse_list("9").unwrap();
        let mut out = [0u8; 16];
        assert_eq!(
            select_fields(b"a\tb\tc", &ranges, b'\t', true, &mut out).unwrap(),
            Some(0),
            "第 9 档选空：这一行写了，只是写不出字段"
        );
        assert_eq!(
            select_fields(b"alone", &ranges, b'\t', true, &mut out).unwrap(),
            None,
            "没有分隔符：这一行整行不存在"
        );
        assert_eq!(
            select_fields(b"alone", &ranges, b'\t', false, &mut out).unwrap(),
            Some(5),
            "不带 `-s` 时原行照抄"
        );
    }

    #[test]
    fn test_bad_lists_rejected() {
        // 只有两类形报错，而且各对应 C 自己那一句：零（`:197-198`）与残留（`:195-196`）。
        assert_eq!(parse_list("0").err(), Some(ListError::IncludesZero));
        assert_eq!(parse_list("a").err(), Some(ListError::IllegalValue));
        assert_eq!(parse_list("1-x").err(), Some(ListError::IllegalValue));
        assert_eq!(parse_list("1-2-3").err(), Some(ListError::IllegalValue));
        // 孤一个减号：start 与 stop 都是零（C 的 setautostart 不产生数字），走零那句。
        assert_eq!(parse_list("-").err(), Some(ListError::IncludesZero));
    }

    /// 列表语法的四形按 C 的 `get_list` 定形（批次二百八十六）：这四形在旧写法里
    /// 都被拒，而 C 接受。
    #[test]
    fn list_grammar_matches_the_strtok_rule() {
        // ① 空列表不是错：`strtok` 直接交 NULL（`cut.c:175`），一个位置也不选。
        assert_eq!(parse_list("").unwrap().len(), 0);
        assert_eq!(bytes_of("abc", ""), "");
        // ② 空项折叠与空白分隔：`cut.c:175` 的分隔符集是 `,`、空格、制表符。
        assert_eq!(bytes_of("abcde", "1,,2"), "ab");
        assert_eq!(bytes_of("abcde", "1 2"), "ab");
        assert_eq!(bytes_of("abcde", "1\t3"), "ac");
        // ③ 倒序区间不查大小关系，只是选不中任何东西（`:209-210` 那个
        // `start++ <= stop` 的循环一次也不跑）。
        assert_eq!(bytes_of("abcde", "5-2"), "");
        // ④ 项数不封顶（C 按 `ALLOC_CHUNK` 长大）：20 项照接。
        let many = (1..=20)
            .map(|n| n.to_string())
            .collect::<Vec<String>>()
            .join(",");
        assert_eq!(parse_list(&many).unwrap().len(), 20);
    }

    /// C 在 `*p == '-'` 上的两个判据（`:186`、`:189`）不能被压平成一件事：
    /// `strtol` 没跑时指针不动，第二个判据又命中同一个 `-`。这两形的报错句子
    /// 在压平写法下正好互换。
    #[test]
    fn the_two_dash_tests_are_not_merged() {
        // `5--`：第一个判据的 `isdigit(p[1])` 不成立（`:187`），于是第二个判据跨过
        // 索引 1 的 `-` 后停在索引 2 的 `-` 上 → 残留那句（`:195-196`）。
        assert_eq!(parse_list("5--").err(), Some(ListError::IllegalValue));
        // `1-2-` 却是 C 接受的：跨过尾部那个 `-` 正好走到行尾（`:189-193`），
        // autostop 那一支把 maxval 之后的内容原样打出（`x_cut.c:80-82`），
        // 等价于从 1 打到行尾。
        assert_eq!(bytes_of("abcde", "1-2-"), "abcde");
        // `0-` 也是接受的形，但走零那句：残留判据在前、零判据在后（`:195-198`）。
        assert_eq!(parse_list("0-").err(), Some(ListError::IncludesZero));
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
