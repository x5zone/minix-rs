//! Line folding for `fold`.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The display
//! column advances by `new_column_position` (fold.c:209-232): backspace
//! pulls one back, carriage return resets to zero, tab rounds up to the
//! next multiple of eight, and every other character adds one (byte
//! counts with `-b`); when the column passes the width the buffered
//! segment flushes, and only `-s` (split-words) first looks back for the
//! last space in the buffer and drops that space from the carried tail
//! (fold.c:152-176) — without it the break is a hard one wherever the
//! column lands, so a space stays at the end of the line it filled.
//! A newline flushes the buffer (fold.c:140-149). The
//! default width is 80 (`DEFLINEWIDTH`, fold.c:56); `-w` sets it.
//! Multibyte characters each occupy one column in this slice (the C
//! counts wide characters the same way without `-b`).

use alloc::string::String;
use alloc::vec::Vec;

/// Fold options: the width and the two modifier flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldOptions {
    /// `-w` width (the break happens when the column passes it).
    pub width: usize,
    /// `-b`: every byte advances the column by one (no `\b`/`\r`/`\t`
    /// column rules).
    pub count_bytes: bool,
    /// `-s`: break without looking for the last space.
    pub split_words: bool,
}

impl FoldOptions {
    /// Default width 80, no modifiers.
    pub fn new(width: usize) -> Self {
        FoldOptions {
            width,
            ..FoldOptions::default()
        }
    }
}

/// The display column after one character (C: `new_column_position`,
/// fold.c:209-232).
fn advance_column(column: usize, character: char, count_bytes: bool) -> usize {
    if count_bytes {
        return column + 1;
    }
    match character {
        '\u{8}' => column.saturating_sub(1),
        '\r' => 0,
        '\t' => (column + 8) & !7,
        _ => column + 1,
    }
}

/// Folds one line into output lines no wider than `options.width`.
///
/// Faithful to `fold`'s per-character walk (fold.c:119-200): the buffer
/// accumulates characters while the column stays within the width; when
/// it passes, the buffer flushes — breaking after the last space unless
/// [`FoldOptions::split_words`] — and the carried tail replays for its
/// new column before the current character joins it. A newline flushes
/// the buffer and resets.
pub fn fold_line(line: &str, options: &FoldOptions) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buffer: Vec<char> = Vec::new();
    let mut column = 0usize;
    for character in line.chars() {
        if character == '\n' {
            out.push(buffer.iter().collect());
            buffer.clear();
            column = 0;
            continue;
        }
        column = advance_column(column, character, options.count_bytes);
        if column > options.width {
            // `fold.c:152-176`：回找缓冲里最后一个空格是 `-s` 独有的腿；不开 `-s` 时列位一过宽度
            // 就把整块缓冲吐出，空格留在它落到的那一行。找到空格那一支把空格本身丢掉
            // （`last_space++` 之后才 `wmemmove`），续行的列位由剩下的字符重算。
            let last_space = if options.split_words {
                buffer.iter().rposition(|&ch| ch == ' ')
            } else {
                None
            };
            match last_space {
                Some(space) => {
                    out.push(buffer[..space].iter().collect());
                    buffer.drain(..=space);
                    column = 0;
                    for &carried in &buffer {
                        column = advance_column(column, carried, options.count_bytes);
                    }
                }
                None => {
                    out.push(buffer.iter().collect());
                    buffer.clear();
                    column = 0;
                }
            }
            column = advance_column(column, character, options.count_bytes);
        }
        buffer.push(character);
    }
    if !buffer.is_empty() {
        out.push(buffer.iter().collect());
    }
    out
}

/// Folds the whole input (line by line; the C folds the stream, whose
/// newlines flush the same buffer).
pub fn fold_input(input: &str, options: &FoldOptions) -> Vec<String> {
    let mut out = Vec::new();
    for line in input.split('\n') {
        out.extend(fold_line(line, options));
    }
    if out.last().is_some_and(|last| last.is_empty()) {
        // An input ending in a newline flushes an empty buffer: the
        // trailing empty fold line is the newline itself, not padding.
        out.pop();
    }
    out
}

/// `fold`'s parsed argv tail: the fold options and the file operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldArgs<'a> {
    /// `-w` width (default 80), `-b`, `-s`.
    pub options: FoldOptions,
    /// File operands in order.
    pub operands: Vec<&'a [u8]>,
}

/// `fold` argument-parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldError {
    /// 选项字母不在 `0-9`/`b`/`s`/`w` 里：libc `getopt` 先印一行
    /// （`minix3/lib/libc/stdlib/getopt.c:101-104`），再由 `default: usage()` 接上那一行 usage
    /// （`fold.c:99-101`、`:115-121`）——所以旧写法只打 usage 是把首行弄丢了。
    UnknownOption(char),
    /// `-w` 后面什么都没有（本 token 没有剩余、下一词也没有）：同一族的另一句
    /// （`getopt.c:132-136`），也接 usage。旧写法把它当成「非法字母」同句报出。
    MissingValue(char),
    /// A `-w`/legacy width that is not a positive integer (C `atoi <= 0` →
    /// `errx(1, "illegal width value")`, `fold.c:84-86`).
    IllegalWidth,
}

/// Split `fold`'s argv tail into [`FoldArgs`], faithful to C's
/// `getopt(argc, argv, "0123456789bsw:")` (`fold.c:76`): the valueless `-b` and
/// `-s` cluster in one token (`-bs`, `-bw10`), `-w` takes a value attached
/// (`-w80`) or as the following argument (`-w 80`), a leading all-digit token is
/// the obsolete `-N` width form (`fold.c:91-98`), `--` ends option scanning, a
/// lone `-` is an operand, and any other letter is [`FoldError::UnknownOption`]（带值选项
/// 缺值则走 [`FoldError::MissingValue`]）。A width that is not a positive integer is
/// [`FoldError::IllegalWidth`] — the old front door warned and *kept* the default instead of
/// rejecting, diverging from C's `errx`. The old bin matched only the whole tokens
/// `-b`/`-s`/`-w`, so the C-legal attached form `fold -w80 file` fell through to a usage error.
pub fn parse_fold_args<'a>(args: &[&'a [u8]]) -> Result<FoldArgs<'a>, FoldError> {
    let mut options = FoldOptions::new(80);
    let mut operands: Vec<&'a [u8]> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg;
        if bytes.len() > 1 && bytes[0] == b'-' {
            if bytes[1].is_ascii_digit() {
                // Obsolete `-N`: the whole token is the width (`fold.c:91-98`).
                options.width = parse_width(&arg[1..])?;
                index += 1;
                continue;
            }
            let mut pos = 1;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'b' => {
                        options.count_bytes = true;
                        pos += 1;
                    }
                    b's' => {
                        options.split_words = true;
                        pos += 1;
                    }
                    b'w' => {
                        let spec = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            if index >= args.len() {
                                return Err(FoldError::MissingValue('w'));
                            }
                            args[index]
                        };
                        options.width = parse_width(spec)?;
                        pos = bytes.len();
                    }
                    // 聚簇里出现数字：C 的选项串把 `0-9` 列为合法字母（遗留的 `-N` 宽度形），
                    // 所以它**不**报 unknown option；但 C 在那个 case 里取的是
                    // `argv[optind - 1]`（`fold.c:91-98`），结果取决于 getopt 内部的 optind 位置，
                    // 没参照件无法取证。本壳按与 `-N` 同一语义的读法处理：从该位置起的数字串作宽度。
                    // 这一形已作为**声明差**登记在 18 篇台账。
                    digit if digit.is_ascii_digit() => {
                        options.width = parse_width(&arg[pos..])?;
                        pos = bytes.len();
                    }
                    _ => return Err(FoldError::UnknownOption(bytes[pos] as char)),
                }
            }
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    Ok(FoldArgs { options, operands })
}

/// A width is a positive integer; `0`, a non-integer, or overflow is rejected
/// exactly as C's `atoi(optarg) <= 0` guard (`fold.c:84-86`).
fn parse_width(spec: &[u8]) -> Result<usize, FoldError> {
    // C 的 `atoi`：一串十进制数字、空串与非数字都归 0，随后 `<= 0` 报 `illegal width value`。
    if spec.is_empty() || !spec.iter().all(u8::is_ascii_digit) {
        return Err(FoldError::IllegalWidth);
    }
    let mut width: usize = 0;
    for digit in spec {
        width = width
            .saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'));
    }
    if width == 0 {
        return Err(FoldError::IllegalWidth);
    }
    Ok(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GetoptReport;

    /// 字节形入口的测试适配器：夹具仍写文本，只在进解析口前折一次。
    fn argv_of<'w>(list: &[&'w str]) -> Vec<&'w [u8]> {
        list.iter().map(|word| word.as_bytes()).collect()
    }

    #[test]
    fn test_short_lines_pass_through() {
        let options = FoldOptions::new(80);
        assert_eq!(fold_input("hello world\n", &options), vec!["hello world"]);
    }

    #[test]
    fn test_break_after_the_last_space() {
        // 参照件 `tmp/fold-405/fold_ref -s -w 10` 现量：`one two` / `three four`。
        // 空格被丢掉（C 把 `last_space` 加一之后才回移缓冲），所以续行不带前导空格。
        let options = FoldOptions {
            width: 10,
            split_words: true,
            ..FoldOptions::default()
        };
        assert_eq!(
            fold_input("one two three four\n", &options),
            vec!["one two", "three four"]
        );
    }

    #[test]
    fn without_split_words_the_break_is_a_hard_one_and_spaces_stay() {
        // 同一串输入不开 `-s`：`fold.c:152-176` 不回找空格，列位一过宽度就整块吐出，
        // 空格留在它填到的那一行末尾——参照件现量 `one two th` / `ree four`。
        let options = FoldOptions::new(10);
        assert_eq!(
            fold_input("one two three four\n", &options),
            vec!["one two th", "ree four"]
        );
    }

    #[test]
    fn split_words_drops_the_space_and_carries_the_rest_as_the_next_line() {
        // `-s -w 4` 对 `abcdefghij klmnop`（参照件现量同此六行）：空格被丢，
        // 剩下的字符留在缓冲里当下一行的开头，列位从它们重算——`ij` 之后是
        // `klmn`，不是带前导空格的 ` klmn`，也不是多一行空行。
        let options = FoldOptions {
            width: 4,
            split_words: true,
            ..FoldOptions::default()
        };
        assert_eq!(
            fold_input("abcdefghij klmnop\n", &options),
            vec!["abcd", "efgh", "ij", "klmn", "op"]
        );
    }

    #[test]
    fn test_no_space_breaks_hard() {
        let options = FoldOptions::new(4);
        assert_eq!(
            fold_input("abcdefghij\n", &options),
            vec!["abcd", "efgh", "ij"]
        );
    }

    #[test]
    fn test_split_words_breaks_anywhere() {
        let options = FoldOptions {
            width: 5,
            split_words: true,
            ..FoldOptions::default()
        };
        assert_eq!(fold_input("onetwo\n", &options), vec!["onetw", "o"]);
    }

    #[test]
    fn test_tab_counts_to_the_next_stop() {
        // One tab from column 0 lands at 8; the x at 9 stays within 10.
        let options = FoldOptions::new(10);
        assert_eq!(fold_input("\tx\n", &options), vec!["\tx"]);
        // Past width 10 the line folds after the tab.
        let options = FoldOptions::new(4);
        // The tab itself overflows: the empty buffer flushes as an empty
        // line (the C prints buf[0..0] plus the newline), then the tab
        // and the x land on the next line.
        assert_eq!(fold_input("\tx\n", &options), vec!["", "\t", "x"]);
    }

    #[test]
    fn test_count_bytes_counts_the_tab_as_one() {
        let options = FoldOptions {
            width: 4,
            count_bytes: true,
            ..FoldOptions::default()
        };
        assert_eq!(fold_input("\txyz\n", &options), vec!["\txyz"]);
    }

    #[test]
    fn test_empty_input_yields_nothing() {
        assert!(fold_input("", &FoldOptions::new(80)).is_empty());
    }

    #[test]
    fn test_parse_fold_default_and_separate_width() {
        use alloc::vec;
        // No options → width 80 default, one operand.
        assert_eq!(
            parse_fold_args(&argv_of(&["f"])),
            Ok(FoldArgs {
                options: FoldOptions::new(80),
                operands: vec![b"f".as_slice()]
            })
        );
        // `-w 20` value as the next token.
        assert_eq!(
            parse_fold_args(&argv_of(&["-w", "20", "f"])).map(|a| a.options.width),
            Ok(20)
        );
        // The regression: attached `-w20` is accepted (was a usage error).
        assert_eq!(
            parse_fold_args(&argv_of(&["-w20", "f"])).map(|a| (a.options.width, a.operands)),
            Ok((20, vec![b"f".as_slice()]))
        );
        // Obsolete leading all-digit `-40` sets the width.
        assert_eq!(
            parse_fold_args(&argv_of(&["-40", "f"])).map(|a| a.options.width),
            Ok(40)
        );
    }

    #[test]
    fn test_parse_fold_cluster_and_errors() {
        // `-b` and `-s` cluster; `-bw30` clusters `-b` then attached `-w30`.
        assert_eq!(
            parse_fold_args(&argv_of(&["-bs"]))
                .map(|a| (a.options.count_bytes, a.options.split_words)),
            Ok((true, true))
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-bw30"])).map(|a| (a.options.count_bytes, a.options.width)),
            Ok((true, 30))
        );
        // `--` ends options; a lone `-` is an operand.
        assert_eq!(
            parse_fold_args(&argv_of(&["--", "-w"])).map(|a| a.operands),
            Ok(vec![b"-w".as_slice()])
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-"])).map(|a| a.operands),
            Ok(vec![b"-".as_slice()])
        );
        // Unknown letter → usage; `-w` with no value → usage.
        assert_eq!(
            parse_fold_args(&argv_of(&["-z", "f"])),
            Err(FoldError::UnknownOption('z'))
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-w"])),
            Err(FoldError::MissingValue('w'))
        );
        // 聚簇里的数字不被当集外字母（C 的选项串包含 `0-9`）。
        assert_eq!(
            parse_fold_args(&argv_of(&["-b9"])).map(|a| (a.options.count_bytes, a.options.width)),
            Ok((true, 9))
        );
        // A non-positive / non-integer width → IllegalWidth (C errx).
        assert_eq!(
            parse_fold_args(&argv_of(&["-w", "0"])),
            Err(FoldError::IllegalWidth)
        );
        assert_eq!(
            parse_fold_args(&argv_of(&["-w", "abc"])),
            Err(FoldError::IllegalWidth)
        );
    }
    #[test]
    fn parse_fold_args_keeps_a_raw_ff_operand_as_its_own_bytes() {
        assert_eq!(
            parse_fold_args(&[&b"a\xffb"[..]]),
            Ok(FoldArgs {
                options: FoldOptions::new(80),
                operands: vec![b"a\xffb".as_slice()]
            }),
            "操作数带的是原字节，不是替换字符的三字节"
        );
    }

    #[test]
    fn parse_fold_args_names_a_raw_ff_option_letter_as_one_byte() {
        // 取法取一枚字节（`bytes[pos] as char`），渲染口 `line_bytes` 把它原样写回：
        // 这一对合起来才让字母位只有一枚 `ff`。
        assert_eq!(
            parse_fold_args(&[&b"-\xff"[..]]),
            Err(FoldError::UnknownOption('\u{ff}'))
        );
        assert_eq!(
            GetoptReport::UnknownOption('\u{ff}').line_bytes("fold"),
            b"fold: unknown option -- \xff\n".as_slice()
        );
    }
}
