//! Tab restoration for `unexpand`.
//!
//! Ground truth: `minix3/usr.bin/unexpand/unexpand.c` (NetBSD). The
//! `tabify` routine (lines 148-205) walks each line tracking a display
//! column (`dcol`: what the input text occupies, where tabs quantize to
//! the next stop) and an output column (`ocol`: what has been emitted);
//! blank runs convert to tabs once the gap reaches two columns, the
//! `-t` stop list bounds the tab region (the last stop minus one is the
//! conversion limit, line 161), and a backspace pulls both columns
//! back. Without `-a` only the leading blank run tabifies — the rest of
//! the line passes through verbatim (the `if (!all || dcol >= limit)`
//! tail, lines 199-205); with `-a` every later blank run converts too.

use alloc::string::String;
use alloc::vec::Vec;

/// The default tab stop when no `-t` list is given.
const DEFAULT_STOP: usize = 8;

/// Unexpand options: the stop list (empty = every eight columns) and
/// the `-a` flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnexpandOptions {
    /// Tab stop columns, strictly increasing (empty = every 8).
    pub stops: Vec<usize>,
    /// `-a`: tabify blank runs everywhere, not only at line starts.
    pub all: bool,
}

/// Parses a `-t` stop list: the C splits on `", \t"` (`strsep`,
/// unexpand.c:91-106), each stop parses as a base-zero number and must
/// be strictly greater than the previous. `None` marks any violation.
/// C 对 `-t` 档位表的两种坏法，各有各的句子，不许抹平成一句。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRejection<'a> {
    /// `unexpand.c:101-104`：`strtoul` 剩下尾巴字符，或溢出到 `ULONG_MAX`
    /// （`errx(EXIT_FAILURE, "Invalid tabstop `%s'", tab)`）。
    Invalid(&'a str),
    /// `unexpand.c:110-114`：不比前一支大
    /// （`errx(... "Bad tabstop spec `%s', must be greater than the previous `%zu'", ...)`）。
    NotIncreasing { spec: &'a str, previous: usize },
}

/// 按 C 的 `strsep(&optarg, ", \t")` 切档位表：逗号、空格、制表符都是分隔符，
/// 空字段跳过（所以 `-t ""` 与 `-t ,` 都是「一支都没有」，C 那边 `nstops` 保持 0，
/// 行为与不给 `-t` 相同），每支按 **base 0** 解析（`0x` 是十六进制、`0` 前缀是八进制，
/// `unexpand.c:98` 的 `strtoul(tab, &ep, 0)`；参照件现量 `-t 010` 得 8、`-t 0x10` 得 16）。
pub fn parse_stops(spec: &str) -> Result<Vec<usize>, StopRejection<'_>> {
    let mut stops: Vec<usize> = Vec::new();
    for piece in spec.split([',', ' ', '\t']) {
        if piece.is_empty() {
            continue;
        }
        let value = match base_zero(piece) {
            Some(value) => value,
            None => return Err(StopRejection::Invalid(piece)),
        };
        // 「不比前一支大」只在有前一支时才判，且相等也不算「更大」（参照件 `-t 4,4' 同拒）。
        if let Some(&last) = stops.last().filter(|&&previous| previous >= value) {
            return Err(StopRejection::NotIncreasing {
                spec: piece,
                previous: last,
            });
        }
        stops.push(value);
    }
    Ok(stops)
}

/// C 的 `strtoul(piece, &ep, 0)` 加上「`*ep` 非零就拒」与「`ERANGE` 且回 `ULONG_MAX` 就拒」。
/// base 0 的前缀规则照 C：`0x`/`0X` 走十六进制，前导 `0` 走八进制，其余十进制。
/// 溢出用 `checked_*` 挡（本壳与 C 都是 64 位无符号宽），落回 `None` 即 C 的 Invalid。
fn base_zero(piece: &str) -> Option<usize> {
    let bytes = piece.as_bytes();
    let (radix, rest) = if bytes.len() > 1 && bytes[0] == b'0' {
        match bytes[1] {
            b'x' | b'X' => (16usize, &piece[2..]),
            _ => (8usize, &piece[1..]),
        }
    } else {
        (10usize, piece)
    };
    if rest.is_empty() {
        return None;
    }
    let mut value: usize = 0;
    for byte in rest.bytes() {
        let digit = (byte as char).to_digit(radix as u32)?;
        value = value.checked_mul(radix)?.checked_add(digit as usize)?;
    }
    Some(value)
}

/// Converts blank runs of the line into tabs.
///
/// Faithful to `tabify` (unexpand.c:148-205): a display column (`dcol`)
/// walks the input — spaces add one, tabs quantize to the next stop —
/// and each non-blank byte flushes the accumulated gap as tabs (only
/// where a stop boundary is crossed and the gap is at least two
/// columns) followed by spaces. A backspace pulls both columns back.
/// Without `-a` the first non-blank byte ends the tabification and the
/// rest of the line passes through verbatim.
pub fn unexpand_line(line: &str, options: &UnexpandOptions) -> String {
    let bytes = line.as_bytes();
    // C 这里减的是 `size_t`（`unexpand.c:146`），档位为 0 时它回绕成最大值而不是停下，
    // 所以 `wrapping_sub` 才是 C 的那一步；裸减法在宿主调试构建里先触 trap，`-t 0` 就崩了。
    let limit = options
        .stops
        .last()
        .map_or(usize::MAX, |stop| stop.wrapping_sub(1));
    let mut out = String::new();
    let mut dcol = 0usize;
    let mut ocol = 0usize;
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b' ' {
            dcol += 1;
            cursor += 1;
            continue;
        }
        if byte == b'\t' {
            // C 量不到下一档时会走 fall-through，把输入里这个制表符原样 `putchar`；
            // 本壳在这一支把它吞掉，那是**另一处已登记的偏离**（`UNEXPAND-LAST-STOP-TAB-DROPPED`，
            // 读数 `tmp/unexpand-425/tab-drop.log`），与本轮的「0 这一档不许崩」不是同一件事。
            if options.stops.is_empty() {
                dcol = (1 + dcol / DEFAULT_STOP) * DEFAULT_STOP;
            } else {
                let mut n = 0usize;
                while n < options.stops.len() && options.stops[n].wrapping_sub(1) < dcol {
                    n += 1;
                }
                if n + 1 < options.stops.len() && options.stops[n].wrapping_sub(1) < limit {
                    dcol = options.stops[n];
                }
            }
            cursor += 1;
            continue;
        }

        // Non-blank byte: flush the gap as tabs then spaces.
        if options.stops.is_empty() {
            while (ocol + DEFAULT_STOP) / DEFAULT_STOP <= dcol / DEFAULT_STOP && dcol - ocol >= 2 {
                out.push('\t');
                ocol = (1 + ocol / DEFAULT_STOP) * DEFAULT_STOP;
            }
            while ocol < dcol {
                out.push(' ');
                ocol += 1;
            }
        } else {
            let mut n = 0usize;
            while n < options.stops.len() && options.stops[n] <= ocol {
                n += 1;
            }
            while n < options.stops.len() && options.stops[n] <= dcol && ocol < dcol && ocol < limit
            {
                out.push('\t');
                ocol = options.stops[n];
                n += 1;
            }
            while ocol < dcol && ocol < limit {
                out.push(' ');
                ocol += 1;
            }
        }

        out.push(byte as char);
        if byte == 0x08 {
            if ocol > 0 {
                ocol -= 1;
            }
            if dcol > 0 {
                dcol -= 1;
            }
        } else {
            ocol += 1;
            dcol += 1;
        }
        cursor += 1;
        if !options.all {
            // Without `-a` the first non-blank byte ends the
            // tabification; the remainder passes through verbatim
            // (unexpand.c:199-205).
            while cursor < bytes.len() {
                out.push(bytes[cursor] as char);
                cursor += 1;
            }
            break;
        }
    }
    out
}

/// Converts every line of the collected input.
pub fn unexpand(input: &str, options: &UnexpandOptions) -> Vec<String> {
    let text = alloc::str::from_utf8(input.as_bytes()).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.as_bytes().ends_with(b"\n") {
        pieces.pop();
    }
    pieces
        .iter()
        .map(|line| unexpand_line(line, options))
        .collect()
}

/// `unexpand`'s parsed argv tail: the merge options and the file operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpandArgs<'a> {
    /// `-a` / `-t` options (which columns to tabify).
    pub options: UnexpandOptions,
    /// File operands in order.
    pub operands: Vec<&'a str>,
}

/// `unexpand` argument-parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnexpandError<'a> {
    /// C 的 `usage()` 自己那一行：`-a` 与 `-t` 冲突（`unexpand.c:87-95` 的
    /// `if (nstops) usage()` 与 `if (all) usage()`）。
    Usage,
    /// 集外字母：libc 先印一行（`getopt.c:101-104`），C 的 `default: usage()` 再接一行。
    UnknownOption(char),
    /// `-t` 后面没有值：libc 印 `option requires an argument -- t`（`getopt.c:132-136`），
    /// 再由 `default: usage()` 接第二行。
    MissingValue(char),
    /// `-t` 的档位表里有一支坏档位，交回 C 那两种句子的哪一种。
    BadStops(StopRejection<'a>),
}

/// 把 `unexpand` 的参数尾巴拆成 [`UnexpandArgs`]，照 C 的 `getopt(argc, argv, "at:")`
/// （`unexpand.c:86`）。三件事必须逐字母按 getopt 的聚簇指针来读而不是整段比对：
/// ① `-a` 与 `-t` 互斥，两支谁后到谁就 `usage()`（`unexpand.c:87-95`），所以 `-at4`
/// 是 usage 而不是组合形；② 集外字母先由 libc 印一行（`getopt.c:101-104`），
/// 交回的是 `-` 后面的**第一个**字符（量过：`-Za` 只报 `Z`）；③ `-t` 的值可以附在
/// 本 token（`-t4,8`）也可以另起一段（`-t 4,8`），后面没有那一段时 libc 印
/// `option requires an argument -- t`（`getopt.c:132-136`）而不是直接 usage。
///
/// 不重排：非选项 token 一到，`getopt.c:74-78` 当场返回 -1，它和它后面的 token 整个
/// 落进 `argv += optind` 的那一段，全当**文件操作数**（参照件量过：`unexpand f -a`
/// 里的 `-a` 是被当第二个档名去 `freopen` 的，报 `Cannot open `-a'`）。
/// `--` 结束选项环且自身不算操作数（`getopt.c:81-85`），裸 `-` 是操作数
/// （`getopt.c:87-92` 不推进 `optind`，而 C `unexpand.c:124` 会拿它去 `freopen`）。
pub fn parse_unexpand_args<'a>(args: &'a [&'a str]) -> Result<UnexpandArgs<'a>, UnexpandError<'a>> {
    let mut options = UnexpandOptions::default();
    let mut operands: Vec<&'a str> = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            operands.extend_from_slice(&args[index..]);
            break;
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'a' => {
                    // `-a` 在档位表之后是 C 的冲突 → usage。
                    if !options.stops.is_empty() {
                        return Err(UnexpandError::Usage);
                    }
                    options.all = true;
                    pos += 1;
                }
                b't' => {
                    // `-t` 在 `-a` 之后是同一支冲突 → usage。
                    if options.all {
                        return Err(UnexpandError::Usage);
                    }
                    let spec = if pos + 1 >= bytes.len() {
                        index += 1;
                        match args.get(index) {
                            Some(value) => *value,
                            None => return Err(UnexpandError::MissingValue('t')),
                        }
                    } else {
                        &arg[pos + 1..]
                    };
                    options.stops = parse_stops(spec).map_err(UnexpandError::BadStops)?;
                    pos = bytes.len();
                }
                _ => {
                    // 环本身按字节走（`pos` 是字节下标），交回的字母也必须是那一枚字节：
                    // `getopt.c:79` 的 `optopt = *place++` 一次进一个字节、`:103` 的 `%c`
                    // 原样写出，所以 `-é`（`c3 a9`）报的是 `c3` 而不是整个字符。
                    return Err(UnexpandError::UnknownOption(char::from(bytes[pos])));
                }
            }
        }
        index += 1;
    }
    Ok(UnexpandArgs { options, operands })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leading_blank_run_becomes_tabs_at_eight() {
        let options = UnexpandOptions::default();
        // Eight blanks land exactly on the stop: one tab. Four blanks
        // stay spaces — the tab loop requires crossing a stop boundary.
        assert_eq!(unexpand_line("        x", &options), "\tx");
        assert_eq!(unexpand_line("    x", &options), "    x");
    }

    #[test]
    fn test_two_space_gap_becomes_one_tab() {
        let options = UnexpandOptions::default();
        // Without `-a` the first non-blank byte ends the conversion, so
        // the interior blank run passes through untouched.
        assert_eq!(unexpand_line("ab        c", &options), "ab        c");
    }

    #[test]
    fn test_single_space_stays_a_space() {
        let options = UnexpandOptions::default();
        assert_eq!(unexpand_line("a b", &options), "a b");
    }

    #[test]
    fn test_without_a_only_leading_blanks_convert() {
        let options = UnexpandOptions::default();
        // Four leading blanks stay spaces (no stop crossed); everything
        // after the first non-blank passes verbatim.
        assert_eq!(
            unexpand_line("    ab        cd", &options),
            "    ab        cd"
        );
    }

    #[test]
    fn test_backspace_pulls_both_columns() {
        // The backspace bytes pass through; the columns pull back so the
        // following blank run starts from a smaller column.
        let options = UnexpandOptions::default();
        // The backspaces pull the column from 2 back to 0; the eight
        // following blanks then reach the stop 8 and become one tab.
        let mut options = options;
        options.all = true;
        let result = unexpand_line("ab\x08\x08        c", &options);
        assert_eq!(result, "ab\x08\x08\tc");
    }

    #[test]
    fn test_stop_list_bounds_the_tab_region() {
        let options = UnexpandOptions {
            stops: vec![10, 20],
            all: false,
        };
        // Five leading blanks stay spaces: the first stop is 10.
        assert_eq!(unexpand_line("     x", &options), "     x");
        // Twelve blanks cross the 10 stop: one tab then two spaces.
        assert_eq!(unexpand_line("            x", &options), "\t  x");
    }

    /// 档位表的两支句子与 base 0 逐个钉：分隔符三种、空字段跳过、`0x`/前导 `0` 走
    /// 十六进制与八进制（参照件现量 `-t 010` 得 8、`-t 0x10` 得 16），
    /// 递减那支报的是**后写的那一支**与前一支的值。
    #[test]
    fn the_stop_list_follows_strtoul_base_zero_and_two_sentences() {
        assert_eq!(parse_stops("1,2"), Ok(vec![1, 2]));
        assert_eq!(parse_stops("4 8"), Ok(vec![4, 8]), "空格也是分隔符");
        assert_eq!(parse_stops("4,\t8"), Ok(vec![4, 8]), "制表符也是分隔符");
        assert_eq!(parse_stops(""), Ok(vec![]), "空表合法，行为同不给 -t");
        assert_eq!(parse_stops(",,"), Ok(vec![]), "空字段跳过");
        assert_eq!(
            parse_stops("0"),
            Ok(vec![0]),
            "C 不挡零档（参照件印 `0 0`）"
        );
        assert_eq!(parse_stops("0x10"), Ok(vec![16]), "base 0 认十六进制");
        assert_eq!(parse_stops("010"), Ok(vec![8]), "前导零是八进制");
        // 第一支句子：尾巴有字符、八进制位越界、溢出。
        assert_eq!(parse_stops("x"), Err(StopRejection::Invalid("x")));
        assert_eq!(parse_stops("3x"), Err(StopRejection::Invalid("3x")));
        assert_eq!(parse_stops("08"), Err(StopRejection::Invalid("08")));
        assert_eq!(
            parse_stops("18446744073709551616"),
            Err(StopRejection::Invalid("18446744073709551616")),
            "溢出在 C 是 ERANGE 且回 ULONG_MAX，同判 Invalid"
        );
        // 第二支句子：不比前一支大，报的是后写的那一支与前一支的值。
        assert_eq!(
            parse_stops("8,4"),
            Err(StopRejection::NotIncreasing {
                spec: "4",
                previous: 8
            })
        );
        assert_eq!(
            parse_stops("4,4"),
            Err(StopRejection::NotIncreasing {
                spec: "4",
                previous: 4
            }),
            "相等也不算「更大」"
        );
    }

    /// 选项环的四支出口与不重排：集外字母只报短横后的第一个字节、缺值是另一句、
    /// `-a` 与 `-t` 谁后到谁触 usage、非选项 token 一到后面的整个当操作数。
    #[test]
    fn the_option_ring_reports_libc_letters_and_stops_at_the_first_operand() {
        assert!(parse_unexpand_args(&["-a"]).unwrap().options.all);
        assert_eq!(
            parse_unexpand_args(&["-t", "4"]).unwrap().options.stops,
            vec![4]
        );
        assert_eq!(
            parse_unexpand_args(&["-t4,8"]).unwrap().options.stops,
            vec![4, 8],
            "附值形是 C 合法的（旧前门只整段比对，会把 `-t8 file` 判成 usage）"
        );
        assert_eq!(parse_unexpand_args(&["-at4"]), Err(UnexpandError::Usage));
        // `-ta` 不是「两支冲突」而是「`-t` 取到附值 `a'」：C 的 getopt 一见 `t:` 就把
        // 同一串里剩下的 `a` 当 optarg 交回（`getopt.c:120-122`），于是走
        // `Invalid tabstop `a'`（参照件现量同一句）。
        assert_eq!(
            parse_unexpand_args(&["-ta"]),
            Err(UnexpandError::BadStops(StopRejection::Invalid("a")))
        );
        // 互斥也管分开的写法：`-a` 之后再来 `-t`，或档位表之后再来 `-a`。
        assert_eq!(
            parse_unexpand_args(&["-a", "-t", "8"]),
            Err(UnexpandError::Usage)
        );
        assert_eq!(
            parse_unexpand_args(&["-t", "8", "-a"]),
            Err(UnexpandError::Usage)
        );
        assert_eq!(
            parse_unexpand_args(&["-z", "f"]),
            Err(UnexpandError::UnknownOption('z'))
        );
        assert_eq!(
            parse_unexpand_args(&["-Za"]),
            Err(UnexpandError::UnknownOption('Z')),
            "聚簇只报第一个字母"
        );
        assert_eq!(
            parse_unexpand_args(&["--x"]),
            Err(UnexpandError::UnknownOption('-'))
        );
        assert_eq!(
            parse_unexpand_args(&["-t"]),
            Err(UnexpandError::MissingValue('t')),
            "缺值是 libc 的另一句而不是 usage"
        );
        assert_eq!(
            parse_unexpand_args(&["-t", "8,4"]),
            Err(UnexpandError::BadStops(StopRejection::NotIncreasing {
                spec: "4",
                previous: 8
            }))
        );
        // 不重排：`unexpand f -a` 里的 `-a` 是第二支档名（C 会去 freopen 它）。
        assert_eq!(
            parse_unexpand_args(&["f", "-a"]).unwrap().operands,
            vec!["f", "-a"]
        );
        assert!(
            !parse_unexpand_args(&["f", "-a"]).unwrap().options.all,
            "操作数之后的 -a 不再被当选项读"
        );
        // `--` 自身不是操作数，裸 `-` 是。
        assert_eq!(
            parse_unexpand_args(&["--", "-a"]).unwrap().operands,
            vec!["-a"]
        );
        assert_eq!(parse_unexpand_args(&["-"]).unwrap().operands, vec!["-"]);
        assert_eq!(
            parse_unexpand_args(&["--"]).unwrap().operands,
            Vec::<&str>::new()
        );
    }

    /// 集外字母交回的是短横后的**第一个字节**而不是第一个字符：`-\u{e9}`（`c3 a9`）在
    /// `char` 域里是 U+00C3（参照件 `tmp/unexpand-312/unexpand_ref` 现量报文里那一个字节
    /// 就是 `303`，见 `tests/unexpand_face.rs` 的同形门）。本支的环本就按字节下标走，
    /// 所以这一枚钉的是「交回的那一刻不要把字节升格成字符」。
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_unexpand_args(&["-\u{e9}"]),
            Err(UnexpandError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // `a` 是选项串里合法且不带值的字母，聚簇继续吃到下一字节。
        assert_eq!(
            parse_unexpand_args(&["-a\u{e9}"]),
            Err(UnexpandError::UnknownOption('\u{00c3}')),
            "-a\u{e9}"
        );
        // 环停在第一个坏字母上：`-Zé` 只报 `Z`。
        assert_eq!(
            parse_unexpand_args(&["-Z\u{e9}"]),
            Err(UnexpandError::UnknownOption('Z'))
        );
        // ASCII 那一形不许被这次改动波及。
        assert_eq!(
            parse_unexpand_args(&["-q"]),
            Err(UnexpandError::UnknownOption('q'))
        );
        // 反面：带值字母把其后的字节整个当值收走，轮不到字母位（参照件同句
        // `Invalid tabstop `a\303\251'`）。
        assert_eq!(
            parse_unexpand_args(&["-t\u{e9}"]),
            Err(UnexpandError::BadStops(StopRejection::Invalid("é")))
        );
    }

    #[test]
    fn test_every_line_converts() {
        let options = UnexpandOptions::default();
        assert_eq!(
            unexpand("        a\n        b\n", &options),
            vec!["\ta".to_string(), "\tb".to_string()]
        );
    }

    #[test]
    fn test_parse_unexpand_flags_and_attach() {
        use alloc::vec;
        assert_eq!(
            parse_unexpand_args(&["f"]).map(|a| (a.options, a.operands)),
            Ok((UnexpandOptions::default(), vec!["f"]))
        );
        // `-a` valueless, `-t` separate value.
        assert!(
            parse_unexpand_args(&["-a", "f"])
                .map(|a| a.options.all)
                .unwrap()
        );
        assert_eq!(
            parse_unexpand_args(&["-t", "8", "f"]).map(|a| a.options.stops),
            Ok(vec![8])
        );
        // The regression: attached `-t8` is accepted (was a usage error).
        assert_eq!(
            parse_unexpand_args(&["-t8", "f"]).map(|a| (a.options.stops, a.operands)),
            Ok((vec![8], vec!["f"]))
        );
        // A lone `-` is an operand; `--` ends options.
        assert_eq!(
            parse_unexpand_args(&["--", "-a"]).map(|a| a.operands),
            Ok(vec!["-a"])
        );
    }

    #[test]
    fn a_zero_tabstop_wraps_its_limit_the_way_c_does() {
        // `unexpand.c:146` 的 `limit = tabstops[nstops - 1] - 1` 与 `:157`/`:160`
        // 那两处 `tabstops[n] - 1` 都是 `size_t` 的减法：档位给 0 时它们回绕成最大值，
        // C 不停下也不报错。参照件现量 `-t 0` 与 `-t 0,8` 都是**整行原样交回、退 0**
        // （`tmp/unexpand-425/ref-matrix.log`），所以本壳也不许在 0 这一档上崩掉。
        assert_eq!(parse_stops("0"), Ok(vec![0]));
        assert_eq!(parse_stops("0,8"), Ok(vec![0, 8]));
        let zero = UnexpandOptions {
            stops: vec![0],
            all: false,
        };
        assert_eq!(unexpand_line("    abc", &zero), "    abc");
        assert_eq!(unexpand_line("  ab", &zero), "  ab");
        assert_eq!(unexpand_line("a\tb\tc", &zero), "a\tb\tc");
        let zero_then_eight = UnexpandOptions {
            stops: vec![0, 8],
            all: false,
        };
        // 参照件现量 `-t 0,8` 对八空格是 `	x`（第 1 档 8 仍可用作 tab 出口），
        // 而 `-t 0` 只有一档且它就是当前列，八空格原样交回（`tmp/unexpand-425/tab-drop.log`）。
        assert_eq!(unexpand_line("        x", &zero_then_eight), "\tx");
        assert_eq!(unexpand_line("        x", &zero), "        x");
        // 这一支只为走进制表符那两处的 `wrapping_sub`（`  ` 之后已是第 2 列，档位 0 是最后一档）：
        // 它的**输出形状**属另一处已登记的偏离（吞掉制表符），本轮不钉它的值，只要求不崩。
        let _ = unexpand_line("  \tx", &zero_then_eight);
    }
}
