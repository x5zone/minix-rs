//! Columnation for `column`.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). Input
//! becomes entries: a line that is entirely blank is dropped, a last line
//! without its newline is dropped with a warning, and every other entry is
//! the whole line — leading blanks included (the C `input`,
//! `column.c:261-297`). Three layouts follow the same widths: `print` (one
//! entry per line) when the longest entry reaches the terminal width,
//! `r_columnate` (fill down the columns, `column.c:170-190`) by default,
//! and `c_columnate` (fill across rows, `column.c:141-167`) with `-x`.
//! Both columnate modes pad with tabs up to `TABROUND` boundaries
//! (`TABROUND(l) = (l + 8) & ~7`, column.c:59) and advance `endcol` by
//! the rounded maximum length per column.
//!
//! 条目、字段、宽度全是**字节**：C 用 `fgetln` 一字节一字节读出、用 `strlen` 量长、
//! 用 `printf("%s")` 原样发出，整条链没有一次解码。所以一枚非 UTF-8 字节既不会让
//! 所属的记录消失，也不会让补齐的宽度多算一位。
//!
//! The terminal width comes from `-c` (default 80, column.c:68 — the C
//! queries the window size, which belongs to the terminal face). `-t`
//! selects the table form: entries split on the separator (default
//! `\t `, strtok run-collapsing semantics, `maketbl` column.c:223-247)
//! and each row prints its fields padded by `lens[col] - len[col] + 2`
//! spaces (column.c:249-251).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::GetoptReport;

/// Column options for one `column` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnOptions {
    /// Terminal width for layout decisions (`-c`; default 80).
    pub termwidth: usize,
    /// `-t`: table mode — split entries into fields on the separator.
    pub table: bool,
    /// `-x`: fill rows before columns.
    pub fill_rows_first: bool,
    /// `-s`: field separator for table mode (default space and tab).
    pub separator: String,
}

impl Default for ColumnOptions {
    fn default() -> Self {
        ColumnOptions {
            termwidth: 80,
            table: false,
            fill_rows_first: false,
            separator: "\t ".to_string(),
        }
    }
}

/// `column` argv 解析结果：选项 + 文件操数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnArgs<'a> {
    pub options: ColumnOptions,
    pub operands: Vec<&'a str>,
}

/// 把 `column` 的 argv 尾部按 BSD `getopt("c:s:tx")`（`column.c:93`）拆成 [`ColumnArgs`]。
/// `t`/`x` 是无值旗标（逐字聚簇，如 `-tx`/`-xt`）；`c`/`s` 带值，值附于字母后
/// （`-c100`/`-s,`）或取下一项（`-c 100`）。失败只可能是 libc `getopt` 那两形，
/// 所以错误载荷直接用 crate 根的 [`GetoptReport`]（句子单源，壳侧渲染）：
/// ① 集外字母 [`GetoptReport::UnknownOption`]（`getopt.c:95-104`，`--x` 的字母就是 `-`）；
/// ② 带值选项后面没有值 [`GetoptReport::MissingValue`]（`getopt.c:132-136`，
///    即 `column.c:107-109` 的 `default: usage()` 之前 libc 自己印的那一行）。
///
/// `--` 结束选项环而自身不是操作数（`getopt.c:81-85` 先 `++optind`）；裸 `-` 与任何
/// 非选项 token 一到就结束选项环**且不重排**（`getopt.c:74-78`、`:87-92`），它和它后面的
/// token 整个落进操作数——`column` 会把它们当档名去 `fopen`（旧写法扫完一个非选项 token
/// 还继续扫下一个，于是 `foo -t` 里的 `-t` 被偷偷当旗标吃掉，C 却是去开那个叫 `-t` 的档）。
///
/// `-c` 的宽度走 [`crate::atoi`]，与 `column.c:96` 的 `atoi(optarg)` 逐字同形：非数字、
/// 空串、负数都不是错。宽度非正时 C 的 `maxlength >= termwidth` 那一判（`:131`）在
/// 条目池非空时**永远成立**（最长条目至少 1 字节，取齐到制表位后是 8），于是布局退成
/// 每条一行；这里把负宽度并到 0 是为了让 `termwidth` 留一个 `usize`——两者在上面的
/// 判定下观察等价，而 `numcols` 的除法只在 `maxlength < termwidth` 时才走到，
/// 那时宽度必然为正。终端宽度本身（`ioctl(TIOCGWINSZ)` 与 `COLUMNS`）属终端面，不在本层。
pub fn parse_column_args<'a>(args: &'a [&'a str]) -> Result<ColumnArgs<'a>, GetoptReport> {
    let mut options = ColumnOptions::default();
    let mut index = 0usize;
    let operands = loop {
        let Some(&arg) = args.get(index) else {
            break Vec::new();
        };
        let bytes = arg.as_bytes();
        if arg == "--" {
            // `getopt.c:81-85`：`++optind` 之后才返回 -1，所以 `--` 自身不是操作数。
            break args[index + 1..].to_vec();
        }
        if bytes.len() < 2 || bytes[0] != b'-' {
            // `getopt.c:74-78`：非选项 token 一到就返回 -1，**不推进** `optind`，
            // 于是它自己连同后面的 token 全是操作数。
            break args[index..].to_vec();
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b't' => options.table = true,
                b'x' => options.fill_rows_first = true,
                ch @ (b'c' | b's') => {
                    // 值是「本串去掉 `-` 与字母之后的那一段」，没有剩余就取下一个 token
                    // （`getopt.c:118-141`）；两种取法都把本串吃到尽头。
                    let value = if pos + 1 < bytes.len() {
                        &arg[pos + 1..]
                    } else {
                        match args.get(index + 1) {
                            Some(next) => {
                                index += 1;
                                next
                            }
                            None => {
                                return Err(GetoptReport::MissingValue(ch as char));
                            }
                        }
                    };
                    if ch == b'c' {
                        options.termwidth = crate::atoi(value).max(0) as usize;
                    } else {
                        options.separator = value.to_string();
                    }
                    pos = bytes.len();
                }
                _ => {
                    // 报的是**这一处的第一个字节**：C 的 `optopt` 是 `char`，`%c` 打的就是
                    // 那一个字节。这里按字节取而不是切字符串，是因为聚簇游标 `pos` 是
                    // **字节**下标，非 ASCII 字母时它会落进多字节字符中间——`&arg[pos..]`
                    // 在那一刻不是字符边界，切串当场 panic（同族的 `col` 带着这一枚，已登记）。
                    return Err(GetoptReport::UnknownOption(char::from(bytes[pos])));
                }
            }
            pos += 1;
        }
        index += 1;
    };
    Ok(ColumnArgs { options, operands })
}

/// Splits one input line into table fields (strtok semantics: leading
/// and repeated separators collapse, empty fields dropped).
///
/// 分隔符集是**字节**集：C 交给 `strtok` 的是 `char *`，`strspn` 一个字节一个字节地比
/// （`maketbl` 的 `strtok(p, separator)`，`column.c:224-225`）。按字符判会放过一枚单独的
/// 续字节——条目现在按字节进池，那种字节是真能出现的。
fn split_fields<'a>(line: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    line.split(|byte| separator.contains(byte))
        .filter(|field| !field.is_empty())
        .collect()
}

/// 一份输入交回的东西：进池的条目，加上被 C 丢掉**且警告过**的未终止末行数。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EntryPool {
    pub entries: Vec<Vec<u8>>,
    /// 末行没有换行符的行数（只可能是 0 或 1）。C 的 `input` 先跳前导空白判空行，
    /// 再 `strchr(p, '\n')` 找不到换行就 `warnx("line too long")`、置 `eval = 1`
    /// 并**把这一行整个丢掉**（`column.c:277-283`）——所以「最后一行没回车」不是
    /// 无害的形状，它既少一条输出又改退出码。
    pub dropped_tail: usize,
}

/// Collects one source's lines into the entry pool: blank lines dropped, an
/// unterminated tail line dropped with a warning, and every kept entry the
/// **whole line** as read (the C `input` loop, `column.c:272-296`).
///
/// 跳前导空白只服务于两处判定——「整条是空白所以是空行」和「从这一段起找换行符」——
/// 入库的是 `buf` 本身，所以条目**留着它的前导空白**（`column.c:287` 的
/// `list[entries++] = buf`，而 `:285` 的 `len = p - buf` 也是从行首量的，
/// 于是 `maxlength` 把前导空白一并算进去）。参照件现量印证了这一点：
/// 输入 `  lead` 在 `-t` 之外的布局里原样打回那两个空格。
///
/// 入参是条目本身（字节，不是文本——C 的 `input` 一字节一字节读进 `buf`，
/// `column.c:272-296`）。按 `split(b'\n')` 的产物理解：**最后一个元素是那条没有换行符的末行**，
/// 输入以 `\n` 结束时它是空串，那时并没有未终止的行（`column.c:272` 的 `fgetln`
/// 只把带换行的记录交出来，最后一段是 EOF 截断的残行）。
#[must_use]
pub fn scan_entries(lines: &[&[u8]]) -> EntryPool {
    let mut pool = EntryPool::default();
    let Some((tail, records)) = lines.split_last() else {
        return pool;
    };
    let unterminated = !tail.is_empty();
    let mut pending = records.to_vec();
    if unterminated {
        pending.push(tail);
    }
    for (index, line) in pending.iter().enumerate() {
        // 前导空白一段段跳过去（本工具链的 `&[u8]` 没有 `trim_start_matches`，手动走）。
        // 哪些字节算空白照 C locale 的 `isspace` 表：空格、制表、换行、垂直制表、
        // 换页、回车六枚——只认前两枚会把一枚垂直制表符当条目收下。
        let mut blank = 0usize;
        while matches!(
            line.get(blank),
            Some(&byte) if matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
        ) {
            blank += 1;
        }
        let head = &line[blank..];
        // 空行判在换行判之前（C 先 `for (...; isspace) ;if (!*p) continue`，
        // 才 `strchr`）：末行整条是空白时它算空行，不算长行，一句警告都没有。
        if head.is_empty() {
            continue;
        }
        if unterminated && index + 1 == pending.len() {
            pool.dropped_tail += 1;
            continue;
        }
        pool.entries.push((*line).to_vec());
    }
    pool
}

/// Lays the entries out and returns the output lines.
///
/// Table mode (`-t`) aligns fields into padded columns; plain mode
/// columnates the whole entries: down the columns by default, across
/// rows with `-x`.
pub fn columnate(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    // main() rounds the longest entry up to the tab stop first, then
    // dispatches (column.c:128-137); a length that reaches the terminal
    // width degrades to one entry per line.
    let maxlength = tab_round(maxlen(entries));
    if options.table {
        return table_mode(entries, options);
    }
    if maxlength >= options.termwidth {
        return entries.to_vec();
    }
    if options.fill_rows_first {
        c_columnate(entries, options)
    } else {
        r_columnate(entries, options)
    }
}

/// Table mode (`-t`): split fields, size each column by its longest
/// member, and emit every row with a two-space gap after each field.
fn table_mode(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    let separator = options.separator.as_bytes();
    let rows: Vec<Vec<&[u8]>> = entries
        .iter()
        .map(|line| split_fields(line, separator))
        .collect();
    let numcols = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    let mut widths: Vec<usize> = alloc::vec![0; numcols];
    for row in &rows {
        for (index, field) in row.iter().enumerate() {
            if field.len() > widths[index] {
                widths[index] = field.len();
            }
        }
    }
    let mut out = Vec::new();
    for row in &rows {
        let mut line: Vec<u8> = Vec::new();
        let last = row.len().saturating_sub(1);
        for (index, field) in row.iter().enumerate() {
            line.extend_from_slice(field);
            // C 的最后一列走 `printf("%s\n", t->list[coloff])`（`column.c:252`）：
            // 既不补齐也没有「裁掉行尾」这一步。所以字段自带的尾随空白——回车这类
            // 不在分隔符集里的字符——原样留在行尾；旧写法补齐之后再 `trim_end`，
            // 把那一枚回车一并吃掉了。
            if index < last {
                line.extend(core::iter::repeat_n(b' ', widths[index] - field.len() + 2));
            }
        }
        out.push(line);
    }
    out
}

/// Fills entries across rows (`-x`): `numcols` per row with tabs up to
/// the `TABROUND` boundary of each column (`c_columnate`,
/// column.c:141-167).
fn c_columnate(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    // The C returns early when there are no entries (column.c:126-128);
    // without that guard the width divisions below would be zero.
    if entries.is_empty() {
        return Vec::new();
    }
    let maxlength = tab_round(maxlen(entries));
    let numcols = options.termwidth / maxlength;
    let mut out = Vec::new();
    let mut line: Vec<u8> = Vec::new();
    let mut chcnt = 0usize;
    let mut col = 0usize;
    let mut endcol = maxlength;
    for (index, entry) in entries.iter().enumerate() {
        line.extend_from_slice(entry);
        chcnt += entry.len();
        // C 的 `if (!--entries) break;` 排在补齐之前：最后一条之后**不补制表符**，
        // 也不决定这一行是否收尾（收尾由下面的 `if (chcnt)` 定）。
        if index + 1 == entries.len() {
            break;
        }
        col += 1;
        if col == numcols {
            chcnt = 0;
            col = 0;
            endcol = maxlength;
            out.push(line);
            line = Vec::new();
        } else {
            while tab_round(chcnt) <= endcol {
                line.push(b'\t');
                chcnt = tab_round(chcnt);
            }
            endcol += maxlength;
        }
    }
    // `if (chcnt) putchar('\n')`：整行刚好在上一步被收走时 `chcnt` 已清 0，
    // 所以这里不会多交出一行空行。
    if chcnt != 0 {
        out.push(line);
    }
    out
}

/// Fills entries down the columns: `numcols` across, rows first.
fn r_columnate(entries: &[Vec<u8>], options: &ColumnOptions) -> Vec<Vec<u8>> {
    // Same empty-input guard as `c_columnate` (column.c:126-128).
    if entries.is_empty() {
        return Vec::new();
    }
    let maxlength = tab_round(maxlen(entries));
    let numcols = options.termwidth / maxlength;
    let numrows = entries.len().div_ceil(numcols);
    let mut out = Vec::new();
    for row in 0..numrows {
        let mut line: Vec<u8> = Vec::new();
        let mut endcol = maxlength;
        let mut chcnt = 0usize;
        for col in 0..numcols {
            let index = row + col * numrows;
            let Some(entry) = entries.get(index) else {
                break;
            };
            line.extend_from_slice(entry);
            chcnt += entry.len();
            // 下一格越界就收行：C 的 `if ((base += numrows) >= entries) break;`
            // 也排在补齐之前，所以行尾不挂制表符。
            if index + numrows >= entries.len() {
                break;
            }
            while tab_round(chcnt) <= endcol {
                line.push(b'\t');
                chcnt = tab_round(chcnt);
            }
            endcol += maxlength;
        }
        out.push(line);
    }
    out
}

fn maxlen(entries: &[Vec<u8>]) -> usize {
    entries.iter().map(|e| e.len()).max().unwrap_or(0)
}

/// `TABROUND(l)` (column.c:59): `(l + TAB) & ~(TAB - 1)` — round up to
/// the next multiple of eight; an already-aligned length advances a
/// full stop (which is what terminates the columnate tab loops).
fn tab_round(length: usize) -> usize {
    (length + 8) & !7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_mode_splits_and_pads_by_max_column_width() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let entries = vec![b"a\tbb\tccc".to_vec(), b"dddd\te\tff".to_vec()];
        let out = columnate(&entries, &options);
        // Each row: fields padded to the per-column max (4) plus a
        // two-space gap; the trailing gap is trimmed.
        // Per-column widths [4, 2, 3]; each field is followed by
        // (width - len + 2) spaces, the trailing run trimmed.
        assert_eq!(out[0], *b"a     bb  ccc");
        assert_eq!(out[1], *b"dddd  e   ff");
    }

    #[test]
    fn a_blank_line_is_any_run_of_isspace_bytes_not_just_space_and_tab() {
        // C 的 `input` 先按 `isspace` 扫前导空白，扫到串尾就整条空行、静默丢弃，
        // 连「末行没有换行符」那一判都轮不到（`column.c:274-278` 的次序在此）。
        // 夹具写字面字节：垂直制表 `0b`、换页 `0c`、回车 `0d` 各一枚都该被丢。
        let pool = scan_entries(&[b"\x0b\x0c", b"\r", b""]);
        assert!(pool.entries.is_empty(), "got {:?}", pool.entries);
        assert_eq!(pool.dropped_tail, 0, "整条空白不是残行，不该计数");
        // 反面照旧：真内容之前的空白整段跳过，条目本身一个字节都不动。
        let pool = scan_entries(&[b"\x0bx", b"y\r", b""]);
        assert_eq!(pool.entries, [b"\x0bx".to_vec(), b"y\r".to_vec()]);
    }

    #[test]
    fn test_table_mode_ignores_blank_lines() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        // 末元素是空串：`split('\n')` 在输入以换行结束时留下的那一段，不是残行。
        let pool = scan_entries(&[b"", b"x\ty", b""]);
        let out = columnate(&pool.entries, &options);
        // 空行被丢掉（C `input` 的前导空白判），所以只剩一条补齐的行。
        assert_eq!(pool.dropped_tail, 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], *b"x  y");
    }

    /// 条目是字节串：一枚非 UTF-8 字节既不进池也不进布局宽度（宽度按 `strlen`，
    /// `column.c:243`）。旧写法在壳侧解一次 UTF-8，一枚坏字节让整个档**一条都不交**。
    #[test]
    fn a_byte_that_is_not_utf8_is_still_an_entry_and_its_layout_is_bytes() {
        let options = ColumnOptions::default();
        let pool = scan_entries(&[b"a\xffb", b"z", b""]);
        assert_eq!(pool.entries, vec![b"a\xffb".to_vec(), b"z".to_vec()]);
        assert_eq!(pool.dropped_tail, 0);
        assert_eq!(
            columnate(&pool.entries, &options),
            vec![b"a\xffb\tz".to_vec()]
        );
        // 表模式同样按字节补齐：`a\xffb` 是 3 字节，与 `dddd` 比时短 1 字节就补 1 空格。
        let table = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let pool = scan_entries(&[b"a\xffb\tq", b"z\tww", b""]);
        assert_eq!(
            columnate(&pool.entries, &table),
            vec![b"a\xffb  q".to_vec(), b"z    ww".to_vec()]
        );
    }

    /// 分隔符集是**字节**集（C 交给 `strtok` 的就是那张字节表）：`-s é` 的值是
    /// `c3 a9`，条目里一枚单独的 `a9` 就是分隔符。按字符判会把它当普通字节放过，
    /// 整条成一列。
    #[test]
    fn the_separator_is_a_set_of_bytes_not_a_set_of_characters() {
        let options = ColumnOptions {
            table: true,
            separator: "\u{e9}".to_string(),
            ..ColumnOptions::default()
        };
        let entries = vec![b"a\xa9b".to_vec()];
        assert_eq!(columnate(&entries, &options), vec![b"a  b".to_vec()]);
    }

    /// 表模式的最后一列原样发出（`column.c:252` 那句 `printf("%s\n", …)`）：
    /// 字段尾随的回车因此留在行尾。旧写法补齐后 `trim_end`，把它一并裁掉。
    #[test]
    fn a_trailing_carriage_return_survives_because_the_last_column_is_printed_raw() {
        let options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        let entries = vec![b"a\tb\r".to_vec(), b"zz\tq".to_vec()];
        assert_eq!(
            columnate(&entries, &options),
            vec![b"a   b\r".to_vec(), b"zz  q".to_vec()]
        );
    }

    #[test]
    fn test_table_mode_custom_separator() {
        let mut options = ColumnOptions {
            table: true,
            ..ColumnOptions::default()
        };
        options.separator = ":".to_string();
        let entries = vec![b"1:b:22".to_vec()];
        let out = columnate(&entries, &options);
        assert_eq!(out.len(), 1);
        assert!(out[0].starts_with(b"1  b  22"));
    }

    /// 入库的是整行：前导空白留在条目里（C 存的是 `buf` 而不是跳过空白后的 `p`），
    /// 只有「整条空白 = 空行」这一判用得到跳过空白后的那一段。
    #[test]
    fn test_entry_collection_keeps_the_line_and_drops_blank_lines() {
        let pool = scan_entries(&[b"   one", b"", b"  two", b""]);
        assert_eq!(pool.entries, vec![b"   one".to_vec(), b"  two".to_vec()]);
        assert_eq!(pool.dropped_tail, 0);
    }

    /// 末行没有换行符：C 判它「长行」，丢条目 + 记一句警告（参照件同受，
    /// `column.c:277-283`）。整条是空白的末行仍按空行处理，警告都没有。
    #[test]
    fn test_unterminated_tail_line_is_dropped_and_counted() {
        let pool = scan_entries(&[b"one", b"two"]);
        assert_eq!(pool.entries, vec![b"one".to_vec()]);
        assert_eq!(pool.dropped_tail, 1);
        let blank_tail = scan_entries(&[b"one", b"   "]);
        assert_eq!(blank_tail.entries, vec![b"one".to_vec()]);
        assert_eq!(blank_tail.dropped_tail, 0);
        let empty = scan_entries(&[b""]);
        assert!(empty.entries.is_empty());
        assert_eq!(empty.dropped_tail, 0);
    }

    #[test]
    fn parse_flags_cluster_and_attached_values() {
        // 关键回归：聚簇与附值不被前门当文件操数。
        let a = parse_column_args(&["-tx", "f"]).unwrap();
        assert!(a.options.table && a.options.fill_rows_first);
        assert_eq!(a.operands, vec!["f"]);
        let b = parse_column_args(&["-c100", "-s,", "f1", "f2"]).unwrap();
        assert_eq!(b.options.termwidth, 100);
        assert_eq!(b.options.separator, ",");
        assert_eq!(b.operands, vec!["f1", "f2"]);
        // 分离值也照常取下一项。
        let c = parse_column_args(&["-c", "120", "-t", "f"]).unwrap();
        assert_eq!(c.options.termwidth, 120);
        assert!(c.options.table);
    }

    #[test]
    fn parse_operand_discrimination_without_reordering() {
        // 裸 `-` 与非 dash token 都是操数；`--` 后全为操数（`--` 自身不是）。
        let a = parse_column_args(&["-", "f2"]).unwrap();
        assert_eq!(a.operands, vec!["-", "f2"]);
        let b = parse_column_args(&["-t", "--", "-c", "f"]).unwrap();
        assert!(b.options.table);
        assert_eq!(b.operands, vec!["-c", "f"]); // `--` 后的 `-c` 是文件名字串
        // 不重排：第一个非选项 token 一到，选项环当场结束，后面的 token 整个当操数。
        let c = parse_column_args(&["f", "-t"]).unwrap();
        assert_eq!(c.operands, vec!["f", "-t"]);
        assert!(!c.options.table);
    }

    /// `-c` 的值是 C 的 `atoi`：非数字、0、负数都不是错，只是宽度塌到「每条一行」。
    #[test]
    fn parse_width_is_atoi_not_a_strict_number() {
        for (argv, width) in [
            (vec!["-c", "0"], 0usize),
            (vec!["-c", "abc"], 0),
            (vec!["-c", "-5"], 0),
            (vec!["-c", "20x"], 20),
            (vec!["-c", " 30"], 30),
        ] {
            assert_eq!(parse_column_args(&argv).unwrap().options.termwidth, width);
        }
    }

    #[test]
    fn parse_error_faces_carry_the_libc_letters() {
        assert_eq!(
            parse_column_args(&["-z", "f"]).err(),
            Some(GetoptReport::UnknownOption('z'))
        ); // 集外
        assert_eq!(
            parse_column_args(&["-tZ"]).err(),
            Some(GetoptReport::UnknownOption('Z'))
        ); // 聚簇里的集外字母
        assert_eq!(
            parse_column_args(&["--x"]).err(),
            Some(GetoptReport::UnknownOption('-'))
        ); // `--x` 的字母就是 `-`（getopt.c:95-104）
        assert_eq!(
            parse_column_args(&["-c"]).err(),
            Some(GetoptReport::MissingValue('c'))
        ); // 缺值
        assert_eq!(
            parse_column_args(&["-t", "-s"]).err(),
            Some(GetoptReport::MissingValue('s'))
        );
    }

    /// 聚簇游标走的是**字节**下标，所以非 ASCII 字母时它落进多字节字符中间：
    /// 报的字母取那一个字节，而切字符串会当场 panic（`&arg[pos..]` 不在字符边界上）。
    #[test]
    fn a_non_ascii_cluster_letter_reports_a_byte_without_panicking() {
        // `-té`：`t` 是合法旗标，游标进到 `é` 的首字节 0xC3，报的就是那一字节。
        assert_eq!(
            parse_column_args(&["-té"]).err(),
            Some(GetoptReport::UnknownOption('\u{c3}'))
        );
        // 游标落在续字节上也不 panic。
        assert_eq!(
            parse_column_args(&["-txé"]).err(),
            Some(GetoptReport::UnknownOption('\u{c3}'))
        );
        // 带值字母后面的多字节值是合法 optarg（值从字符边界上切起）。
        assert_eq!(
            parse_column_args(&["-sé", "f"]).unwrap().options.separator,
            "é"
        );
    }
}
