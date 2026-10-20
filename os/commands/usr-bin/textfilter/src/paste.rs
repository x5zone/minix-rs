//! Stream merging for `paste`.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD):
//! `paste [-s] [-d list] file ...`. 环写作 `paste.c:70` 的
//! `while ((ch = getopt(argc, argv, "d:s")) != -1)`，而 `paste.c:80-82` 的
//! `case '?': default: usage();` 是**在环里直接退出**的——这一点让它与同族的
//! `seq`（`errflg++` 之后环继续扫）正相反：本支的环**只可能累积一行** libc 报文——`jot` 也在这
//! 一列里（`jot.c:150-152` 是 `default: usage();`，参照件现量 `-ZQ` 与 `-Z -Q` 都只报 `Z`）。
//!
//! 分隔符表由 `tr()`（`paste.c:199-226`）就地转换并计数：`\n`、`\t`、`\0` 三个转义，
//! **认不出的转义把反斜杠丢掉**（`default: *arg = ch;`，与 `seq` 的 `unescape` 相反），
//! 表为空则 `errx(1, "no delimiters specified.")`。表里的每一项是**一个字节**，
//! 值为 0 的那项照样占一个位但什么都不打（`if (ch) putchar(ch)`），所以
//! `-d '\0'` 是「两列之间不留任何东西」而不是「少一列」。

use alloc::vec;
use alloc::vec::Vec;

/// 一列之间的分隔符位：`None` 表示这一位存在但打不出东西（`-d '\0'` 那一项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteOptions {
    /// 分隔符表（按字节），环内由 [`tr_delims`] 转换并计数；没给 `-d` 时是
    /// 一个制表符（C 的 `dflt_delim`，`paste.c:61-63`）。
    pub delimiters: Vec<u8>,
    /// `-s`：把每个操作数的行**顺序**并成一行，而不是跨操作数并行合并。
    pub serial: bool,
}

impl Default for PasteOptions {
    fn default() -> Self {
        Self {
            delimiters: vec![b'\t'],
            serial: false,
        }
    }
}

/// 环跑完之后手上有的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteArgs<'a> {
    pub options: PasteOptions,
    /// 环**不重排**：第一个不是选项的 token 及其后所有 token 都是操作数
    /// （`paste.c:85-86` 的 `argc -= optind; argv += optind;`）。
    pub operands: Vec<&'a [u8]>,
}

/// 前门的错，按 C 的出口分两支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    /// 集外字母或 `-d` 缺值：libc 先印一行，紧跟一行 usage，退 1（`paste.c:80-82`、`:229-233`）。
    /// 本支的环在这里就退出，所以 `reports` 至多一条。
    Usage { reports: Vec<crate::GetoptReport> },
    /// `-d ''`：`tr()` 数到零个分隔符（`paste.c:221-223`），`errx(1, "no delimiters specified.")`，
    /// 句子**自带句号**且不带 `strerror`。
    NoDelimiters,
}

/// `paste.c:70` 的选项环 + 操作数判定。`optstring` 是 `"d:s"`：`d` 带值，`s` 是旗标。
///
/// 三处与同族不同，都经参照件现量（`tmp/paste-321/measure.log`）：
/// ① 环在**第一个**坏字母处就退出（`usage()` 在 `switch` 里被直接调用），于是
///    `paste -sZ` 只有一行 libc 加一行 usage，`-d ''` 之后跟着坏字母时报的是「没有分隔符」
///    而不是集外字母——错误出口的**先后**就是扫描的先后；
/// ② 操作数**没有个数判**：`paste` 不带操作数既不读标准输入也不报错，退 0 打空；
/// ③ `-d` 的值可以紧贴本 token（`-d,`、`-ds` 的 `s` 是值而不是旗标）。
pub fn parse_paste_args<'a>(args: &[&'a [u8]]) -> Result<PasteArgs<'a>, PasteError> {
    const OPTSTRING: &[u8] = b"d:s";
    let mut options = PasteOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        let token = args[i];
        // `getopt.c:74-78`：首字符不是 `-`（或整个 token 就是一个 `-`）⇒ 收环且不推进 `optind`；
        // `getopt.c:81-85`：`--` 结束环并吃掉自身。
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
            let Some(slot) = OPTSTRING.iter().position(|&o| o == letter && o != b':') else {
                // `getopt.c:95-104` 印那一行，随后 C 的 `default: usage()` 立刻退出
                return Err(PasteError::Usage {
                    reports: vec![crate::GetoptReport::UnknownOption(char::from(letter))],
                });
            };
            if OPTSTRING.get(slot + 1) == Some(&b':') {
                let value = if k < token.len() {
                    &args[i][k..]
                } else {
                    i += 1;
                    match args.get(i) {
                        Some(next) => *next,
                        None => {
                            return Err(PasteError::Usage {
                                reports: vec![crate::GetoptReport::MissingValue(char::from(
                                    letter,
                                ))],
                            });
                        }
                    }
                };
                // `tr(delim)` 就在 `case 'd'` 里跑（`paste.c:73-75`），所以空表在这一刻就退出，
                // 后面的字母与操作数都轮不到。
                options.delimiters = tr_delims(value)?;
                i += 1;
                break;
            }
            if letter == b's' {
                options.serial = true;
            }
            if k >= token.len() {
                i += 1;
            }
        }
    }
    Ok(PasteArgs {
        options,
        operands: args[i..].to_vec(),
    })
}

/// C 的 `tr()`：就地转换 `\n`、`\t`、`\0`，认不出的转义**丢掉反斜杠**留下后随字符，
/// 返回**位数**（值 0 的那项照样计一位）。空串 ⇒ [`PasteError::NoDelimiters`]。
pub fn tr_delims(list: &[u8]) -> Result<Vec<u8>, PasteError> {
    let bytes = list;
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        // 串尾的反斜杠：C 的 `(ch = *p++)` 读到那个 `\0` 就**结束整个循环**，
        // 反斜杠自己不成一项（`paste.c:204` 的循环条件）。
        let Some(&esc) = bytes.get(i) else { break };
        i += 1;
        out.push(match esc {
            b'n' => b'\n',
            b't' => b'\t',
            b'0' => 0,
            other => other,
        });
    }
    if out.is_empty() {
        return Err(PasteError::NoDelimiters);
    }
    Ok(out)
}

/// 第 `gap` 个分隔符位的**字节**（表按位循环）。值为 0 的那位与空表都返回 `None`——
/// C 的 `if (ch) putchar(ch)`（`paste.c:131-133`、`:148-150`、`:184-185`）就是
/// 「占一位而不打任何东西」。交回 `u8` 而不是 `char`：`putchar` 收的是那一字节本身，
/// 经 `char` 域再落进 `String` 会把裸 `ff` 写成 `c3 bf` 两字节。
fn delimiter_byte(delimiters: &[u8], gap: usize) -> Option<u8> {
    let byte = *delimiters.get(gap % delimiters.len())?;
    if byte == 0 { None } else { Some(byte) }
}

/// 顺序模式（`-s`）：把一个流的行用循环的分隔符并成一行（`paste.c:177-191`）。
///
/// 这里的游标从表的**第一项**开始（与并行模式不同，见 [`join_parallel`]），
/// 且每发出一个分隔符就前进一位、走满一轮回到表头；值为 0 的位不打印但仍然占位。
///
/// 行与分隔符都是字节：C 的 `sequential` 用 `fgets` 拿行、`printf("%s", buf)` 发行内容
/// （`paste.c:175-191`），全程不解码，所以一个非 UTF-8 字节照样留在它那一行的原位
/// （值为 0 的字节是那一条腿的另一件事：`%s` 在那里截断，另册登记）。
pub fn join_serial(lines: &[&[u8]], options: &PasteOptions) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.extend(delimiter_byte(&options.delimiters, index - 1));
        }
        out.extend_from_slice(line);
    }
    if !lines.is_empty() {
        out.push(b'\n');
    }
    out
}

/// 并行模式：每行从每**列**各取一行（`paste.c:111-140`）。
///
/// `columns[k]` 是第 `k` 列指向的**已打开的流**（`fpp[cnt]`）：同一个流可以出现在多列上，
/// 此时它们**共用一个游标**——参照件 `paste - -`（两列都是 stdin）对 `p\nq\n` 打的是
/// `p\tq\n`（一行两列各取一行），而两次 `open` 同一个档名是两个游标
/// （`paste a.txt a.txt` 打 `1\t1`，实测同形）。
///
/// 游标的起点是**表尾那个 NUL**（`dp = delim + delimcnt`，`paste.c:112`），所以每行第一列
/// 之前补的那一位是打不出东西的 NUL，从第二列起才用到 `delim[0]`。更要紧的是两处补齐：
/// 某条流提前见底时，后面的列会**为见底的那一列也补一个分隔符**（`paste.c:127-133` 的
/// 「追平」环按列号而不是按实际发出数走），行尾再给剩下的每一列各补一个
/// （`paste.c:144-150`）。参照件 `paste -d '+:' a.txt b.txt a.txt`（a 三行、b 两行）
/// 的第三行是 `3+:3`：`+` 是给见底的 b 补的，`:` 才是第三列自己前面的那一位。
///
/// 行与分隔符都是字节（同 [`join_serial`]）：`parallel` 的 `fgetln` 拿到含换行的整行
/// （`paste.c:121`），减掉末尾那枚换行（`:138-139`），再按**量出来的长度**发出
/// （`:140` 的 `printf("%.*s", (int)line_len, line)`）——长度由 `fgetln` 交回，不由编码算出。
pub fn join_parallel(columns: &[usize], sources: &[&[&[u8]]], options: &PasteOptions) -> Vec<u8> {
    let slots = &options.delimiters;
    let count = slots.len();
    let mut out: Vec<u8> = Vec::new();
    let mut cursor: Vec<usize> = vec![0; sources.len()];
    loop {
        // C 的 `dp`：从「表尾之后」开始，读到那里就当作 NUL 并回卷到表头
        let mut dp = count;
        let mut output = 0usize;
        let mut cnt = 0usize;
        while cnt < columns.len() {
            let source = columns[cnt];
            if cursor[source] >= sources[source].len() {
                cnt += 1;
                continue;
            }
            let line = sources[source][cursor[source]];
            cursor[source] += 1;
            loop {
                if dp >= count {
                    dp = 0;
                } else {
                    if slots[dp] != 0 {
                        out.push(slots[dp]);
                    }
                    dp += 1;
                }
                if dp >= count {
                    dp = 0;
                }
                output += 1;
                if output > cnt {
                    break;
                }
            }
            out.extend_from_slice(line);
            cnt += 1;
        }
        if output == 0 {
            break;
        }
        while output < columns.len() {
            if dp >= count {
                dp = 0;
            }
            if slots[dp] != 0 {
                out.push(slots[dp]);
            }
            dp += 1;
            output += 1;
        }
        out.push(b'\n');
    }
    out
}

/// 只要选项的一瞥（操作数由前门 [`parse_paste_args`] 给出）。
pub fn parse_args(args: &[&[u8]]) -> Result<PasteOptions, PasteError> {
    Ok(parse_paste_args(args)?.options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(list: &[&'a [u8]]) -> PasteArgs<'a> {
        parse_paste_args(list).expect("the front door should accept")
    }

    #[test]
    fn test_ring_only_says_once() {
        // 集外字母：libc 一行 + usage，且**只有一行 libc**（`usage()` 在环里就退出）
        assert_eq!(
            parse_paste_args(&["-Z".as_bytes(), "-h".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
        // `-d` 缺值走的是 libc 的第二种报文
        assert_eq!(
            parse_paste_args(&["-d".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::MissingValue('d')]
            })
        );
        // 坏字母排在好字母之后也一样只报坏的那个
        assert_eq!(
            parse_paste_args(&["-sZ".as_bytes(), "a.txt".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
    }

    #[test]
    fn test_error_order_follows_the_scan() {
        // `-d ''` 在环内就撞上 `tr()` ⇒ 「没有分隔符」先报，后面的集外字母轮不到
        assert_eq!(
            parse_paste_args(&["-d".as_bytes(), "".as_bytes(), "-Z".as_bytes()]),
            Err(PasteError::NoDelimiters)
        );
        // 反过来，坏字母在前时就轮不到 `-d`
        assert_eq!(
            parse_paste_args(&["-Z".as_bytes(), "-d".as_bytes(), "".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
    }

    #[test]
    fn test_operands_are_not_reordered() {
        // 第一个非选项 token 及其后一切全是操作数：`paste a.txt -s` 的 `-s` 是第二个档名
        let parsed = args(&["a.txt".as_bytes(), "-s".as_bytes()]);
        assert_eq!(parsed.operands, vec!["a.txt".as_bytes(), "-s".as_bytes()]);
        assert!(!parsed.options.serial);
        // `--` 结束环且自身不是操作数
        let parsed = args(&["--".as_bytes(), "-s".as_bytes()]);
        assert_eq!(parsed.operands, vec!["-s".as_bytes()]);
        // 裸 `-` 是操作数（C 拿它当标准输入），且它**不**结束环之前的扫描
        let parsed = args(&["-".as_bytes(), "b.txt".as_bytes()]);
        assert_eq!(parsed.operands, vec!["-".as_bytes(), "b.txt".as_bytes()]);
    }

    #[test]
    fn test_no_operand_is_not_an_error() {
        // C 对零个操作数既不读标准输入也不报错：退 0 打空（`paste.c:88-92` 把 argc 交给 parallel）
        let parsed = args(&[]);
        assert!(parsed.operands.is_empty());
        assert_eq!(parsed.operands.len(), 0);
        let parsed = args(&["--".as_bytes()]);
        assert!(parsed.operands.is_empty());
    }

    #[test]
    fn test_attached_values_and_clusters() {
        // `-d,` 的值紧贴本 token
        let parsed = args(&["-d,".as_bytes(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![b',']);
        // `-ds`：`s` 是 `d` 的值，不是旗标
        let parsed = args(&["-ds".as_bytes(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![b's']);
        assert!(!parsed.options.serial);
        // 值取自下一个 token 时，`optind` 要越过它
        let parsed = args(&[
            "-s".as_bytes(),
            "-d".as_bytes(),
            "+:".as_bytes(),
            "a.txt".as_bytes(),
            "b.txt".as_bytes(),
        ]);
        assert!(parsed.options.serial);
        assert_eq!(parsed.options.delimiters, vec![b'+', b':']);
        assert_eq!(
            parsed.operands,
            vec!["a.txt".as_bytes(), "b.txt".as_bytes()]
        );
    }

    #[test]
    fn test_tr_matches_the_c_table() {
        assert_eq!(tr_delims(b"\\n".as_slice()).unwrap(), vec![b'\n']);
        assert_eq!(tr_delims(b"\\t".as_slice()).unwrap(), vec![b'\t']);
        // `\0` 是一**位**，值为 0
        assert_eq!(tr_delims(b"\\0".as_slice()).unwrap(), vec![0]);
        // 认不出的转义丢掉反斜杠（与 `seq` 的 unescape 相反）
        assert_eq!(tr_delims(b"\\q".as_slice()).unwrap(), vec![b'q']);
        // `a\0b` 是三位：'a'、值为 0 的一位、'b'
        assert_eq!(tr_delims(b"a\\0b".as_slice()).unwrap(), vec![b'a', 0, b'b']);
        // 串尾孤零零一个反斜杠：反斜杠不成位
        assert_eq!(tr_delims(b"x\\".as_slice()).unwrap(), vec![b'x']);
        assert_eq!(tr_delims(b"".as_slice()), Err(PasteError::NoDelimiters));
        assert_eq!(tr_delims(b"\\".as_slice()), Err(PasteError::NoDelimiters));
    }

    #[test]
    fn test_serial_cycles_from_the_first_slot() {
        let options = PasteOptions {
            delimiters: vec![b'+', b':'],
            serial: true,
        };
        let four: [&[u8]; 4] = [b"a", b"b", b"c", b"d"];
        assert_eq!(join_serial(&four, &options), b"a+b:c+d\n".as_slice());
        // 值为 0 的一位照样占位但什么都不打
        let options = PasteOptions {
            delimiters: vec![0],
            serial: true,
        };
        let three: [&[u8]; 3] = [b"a", b"b", b"c"];
        assert_eq!(join_serial(&three, &options), b"abc\n".as_slice());
        // 空流：C 的 sequential 一个字符都不打（连换行也没有）
        let options = PasteOptions::default();
        assert_eq!(join_serial(&[], &options), b"".as_slice());
    }

    #[test]
    fn test_parallel_catches_up_for_a_finished_stream() {
        let a: [&[u8]; 3] = [b"1", b"2", b"3"];
        let b: [&[u8]; 2] = [b"a", b"b"];
        let options = PasteOptions {
            delimiters: vec![b'+', b':'],
            serial: false,
        };
        // 第三行：b 见底 ⇒ `+` 是给 b 补的，`:` 才是第三列前面那一位（参照件同形）
        let c: [&[u8]; 3] = [b"x", b"y", b"z"];
        assert_eq!(
            join_parallel(&[0, 1, 2], &[&a[..], &b[..], &c[..]], &options),
            b"1+a:x\n2+b:y\n3+:z\n".as_slice()
        );
        // 两列共用一个流（`paste - -` 的形状）：游标只有一个
        assert_eq!(
            join_parallel(&[0, 0], &[&a[..]], &options),
            b"1+2\n3+\n".as_slice()
        );
    }

    #[test]
    fn test_parallel_default_is_one_tab() {
        let options = PasteOptions::default();
        let a: [&[u8]; 3] = [b"1", b"2", b"3"];
        let b: [&[u8]; 2] = [b"a", b"b"];
        assert_eq!(
            join_parallel(&[0, 1], &[&a[..], &b[..]], &options),
            b"1\ta\n2\tb\n3\t\n".as_slice()
        );
    }

    #[test]
    fn test_nul_slot_still_counts_as_a_gap() {
        // `-d '\0'`：两列之间不留任何东西，但占一位（参照件 `1a\n2b\n3\n`）
        let options = PasteOptions {
            delimiters: vec![0],
            serial: false,
        };
        let a: [&[u8]; 3] = [b"1", b"2", b"3"];
        let b: [&[u8]; 2] = [b"a", b"b"];
        assert_eq!(
            join_parallel(&[0, 1], &[&a[..], &b[..]], &options),
            b"1a\n2b\n3\n".as_slice()
        );
        // `a\0b` 三位时，两列的第一位是 'a' ⇒ `1aa`（b 的首行本身就是一个 `a`）
        let options = PasteOptions {
            delimiters: vec![b'a', 0, b'b'],
            serial: false,
        };
        assert_eq!(
            join_parallel(&[0, 1], &[&a[..], &b[..]], &options),
            b"1aa\n2ab\n3a\n".as_slice()
        );
    }

    /// 分隔符位发出的是一**字节**：`putchar` 收下表项本身（`paste.c:131-133`、`:148-150`、`:184-185`），
    /// 裸 `ff` 写出去就是 `ff`。旧写法 `slots[dp] as char` 把它升成 U+00FF，落进 `String`
    /// 变成 `c3 bf` 两字节——比 C 多一字节。
    #[test]
    fn a_delimiter_slot_is_one_byte_not_a_codepoint() {
        let left: [&[u8]; 1] = [b"p"];
        let right: [&[u8]; 1] = [b"q"];
        let options = PasteOptions {
            delimiters: vec![0xff],
            serial: false,
        };
        assert_eq!(
            join_parallel(&[0, 1], &[&left[..], &right[..]], &options),
            b"p\xffq\n".as_slice()
        );
        let both: [&[u8]; 2] = [b"p", b"q"];
        let options = PasteOptions {
            delimiters: vec![0xff],
            serial: true,
        };
        assert_eq!(join_serial(&both, &options), b"p\xffq\n".as_slice());
    }

    /// 行内容里的非 UTF-8 字节照常是那一行的字节：旧写法在壳里
    /// `from_utf8(input).unwrap_or("")` 让整个档的行一起消失（现量见
    /// `tmp/paste-comm-434/ab.log`：参照件打 `a\xffb\ta\xffb`，本壳交回空）。
    #[test]
    fn a_byte_that_is_not_utf8_is_still_part_of_the_line() {
        let options = PasteOptions::default();
        let lines: [&[u8]; 2] = [b"a\xffb", b"c"];
        assert_eq!(join_serial(&lines, &options), b"a\xffb\tc\n".as_slice());
        let left: [&[u8]; 2] = [b"a\xffb", b"c"];
        let right: [&[u8]; 1] = [b"x"];
        assert_eq!(
            join_parallel(&[0, 1], &[&left[..], &right[..]], &options),
            b"a\xffb\tx\nc\t\n".as_slice()
        );
    }

    #[test]
    fn test_parse_keeps_the_options_only_view() {
        assert_eq!(
            parse_args(&["-s".as_bytes(), "-d".as_bytes(), ",".as_bytes()]),
            Ok(PasteOptions {
                delimiters: vec![b','],
                serial: true
            })
        );
        assert_eq!(
            parse_args(&["-d".as_bytes(), "".as_bytes()]),
            Err(PasteError::NoDelimiters)
        );
    }

    /// 集外字母那一行的字母位是 argv 里的**那一枚字节**：`minix3/lib/libc/stdlib/getopt.c:79`
    /// 的 `optopt = *place++` 取的是字节，`minix3/usr.bin/paste/paste.c:80-82` 的
    /// `default: usage()` 就地退出，所以一枚裸 `ff` 只有一个字母位可报。参数缝按字节交回之后
    /// 这一条腿不再经过 `from_utf8_lossy`——旧写法把 `ff` 折叠成 `ef bf bd`，报文里因此是 `ef`；
    /// 参照件 `tmp/paste-321/paste_ref` 现量同为单个 `ff`（`tmp/argv-441/measure-ref.log`）。
    #[test]
    fn parse_paste_names_a_raw_ff_option_letter_by_that_byte() {
        let flag: Vec<u8> = [b"-".as_slice(), b"\xff".as_slice()].concat();
        assert_eq!(
            parse_paste_args(&[flag.as_slice()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('\u{00ff}')]
            })
        );
        assert_eq!(
            crate::GetoptReport::UnknownOption('\u{00ff}').line_bytes("paste"),
            [
                b"paste: unknown option -- ".as_slice(),
                b"\xff".as_slice(),
                b"\n".as_slice()
            ]
            .concat()
        );
    }

    /// `-d` 的**值**同样是一串字节：C 的 `tr()` 按位扫过那张表（`paste.c:199-226`），
    /// 非 UTF-8 字节只是表里的**一位**。旧写法经 lossy 把一枚 `ff` 变成 `ef bf bd` **三位**，
    /// 于是 `paste -d\xff a.txt b.txt` 交出 `1\xefa`（`ef` 用在这一行，`bf`、`bd` 轮到后面
    /// 两行），参照件现量是每行一枚 `ff`。
    #[test]
    fn parse_paste_keeps_a_nonutf8_delimiter_byte_as_one_slot() {
        let attached: Vec<u8> = [b"-d".as_slice(), b"\xff".as_slice()].concat();
        let parsed = args(&[attached.as_slice(), "a.txt".as_bytes(), "b.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![0xff]);
        assert_eq!(
            parsed.operands,
            vec!["a.txt".as_bytes(), "b.txt".as_bytes()]
        );
        // 紧贴 ASCII 时是**两位**：一枚 `ff`、一个 `+`
        let two: Vec<u8> = [b"-d".as_slice(), b"\xff".as_slice(), b"+".as_slice()].concat();
        let parsed = args(&[two.as_slice(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![0xff, b'+']);
        // 值取自下一个 token 时同样按字节
        let separate: Vec<u8> = [b"\xff".as_slice(), b"+".as_slice()].concat();
        let parsed = args(&["-d".as_bytes(), separate.as_slice(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![0xff, b'+']);
    }
}
