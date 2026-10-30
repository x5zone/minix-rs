//! Tab expansion for `expand`.
//!
//! Ground truth: `minix3/usr.bin/expand/expand.c` (NetBSD). Tab stops
//! come from `-t a,b,c` (1 to 256 per stop, strictly increasing, at most
//! eight stops — `getstops`, lines 143-166) with the obsolete `-N` form
//! as its one-stop special case (lines 69-73). Expansion rules
//! (lines 96-127): no stops — pad to the next multiple of eight; one
//! stop — pad to the next multiple of that stop; several — pad to the
//! next stop beyond the current column, or one space past the last
//! stop. A backspace passes through and pulls the column back (never
//! below zero); a newline resets the column; every other byte advances
//! it. File operands (`freopen`, line 88) wait for the gated
//! open-existing call.

use alloc::string::String;
use alloc::vec::Vec;

/// Expansion options: the tab stop list (empty = every eight columns).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExpandOptions {
    /// Tab stop columns, strictly increasing (empty = every 8).
    pub stops: Vec<usize>,
}

/// Parses a `-t` stop list: comma- or space-separated, each 1 to 256,
/// strictly increasing, at most eight stops (`getstops`, expand.c:143-166).
/// `None` marks any violation.
/// Parses a `-t` stop list: comma- or space-separated, each stop 1 to
/// 256, strictly increasing, at most eight stops (`getstops`,
/// expand.c:143-166; a trailing separator leaves an empty piece the C
/// rejects as stop zero). `None` marks any violation.
pub fn parse_stops(spec: &[u8]) -> Option<Vec<usize>> {
    let mut stops: Vec<usize> = Vec::new();
    // 分隔符与 C 同为逗号与空格；空字段照跳（expand.c 的 getstops 同形）。
    for piece in spec.split(|&b| b == b',' || b == b' ') {
        let bytes: &[u8] = piece;
        let mut value = 0usize;
        let mut digits = 0usize;
        for &byte in bytes {
            if !byte.is_ascii_digit() {
                break;
            }
            value = value * 10 + (byte - b'0') as usize;
            digits += 1;
        }
        if digits != bytes.len() || value == 0 || value > 256 {
            return None;
        }
        if !stops.is_empty() && value <= stops[stops.len() - 1] {
            return None;
        }
        if stops.len() == 8 {
            return None;
        }
        stops.push(value);
    }
    if stops.is_empty() { None } else { Some(stops) }
}

/// Expands tabs to spaces over the whole input.
pub fn expand(input: &str, stops: &[usize]) -> String {
    let mut out = Vec::new();
    let mut column = 0usize;
    for &byte in input.as_bytes() {
        match byte {
            b'\t' => {
                if stops.is_empty() {
                    loop {
                        out.push(b' ');
                        column += 1;
                        if column & 7 == 0 {
                            break;
                        }
                    }
                } else if stops.len() == 1 {
                    let stop = stops[0];
                    loop {
                        out.push(b' ');
                        column += 1;
                        if (column - 1) % stop == stop - 1 {
                            break;
                        }
                    }
                } else {
                    let next = stops.iter().find(|&&stop| stop > column);
                    match next {
                        Some(&stop) => {
                            while column < stop {
                                out.push(b' ');
                                column += 1;
                            }
                        }
                        None => {
                            out.push(b' ');
                            column += 1;
                        }
                    }
                }
            }
            b'\x08' => {
                if column > 0 {
                    column -= 1;
                }
                out.push(b'\x08');
            }
            b'\n' => {
                out.push(b'\n');
                column = 0;
            }
            _ => {
                out.push(byte);
                column += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Tab stops + file operands parsed from `expand`'s argv tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandArgs<'a> {
    /// Tab stop columns from `-t`/obsolete `-N`; empty means the default 8.
    pub stops: Vec<usize>,
    /// File operands in order——按 argv 的原字节交回（A2 参数缝半）。
    pub operands: Vec<&'a [u8]>,
}

/// `expand` argument-parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandError {
    /// An option letter other than `-t` (C `getopt` default → usage,
    /// `expand.c:81-83`).
    Usage,
    /// `-t`/`-N` with no value, or a stop list `getstops`/`parse_stops` rejects
    /// (`expand.c:79`).
    IllegalStops,
}

/// Split `expand`'s argv tail into [`ExpandArgs`], faithful to C. The obsolete
/// leading run `while (argv[1][0] == '-' && isdigit(argv[1][1]))`
/// (`expand.c:70-73`) consumes a run of `-<digits>` tokens, each handed to
/// `getstops`, stopping at the first non-match. Then `getopt(argc, argv, "t:")`
/// (`expand.c:76`) applies: `-t` takes a value either attached (`-t8`) or as the
/// following argument (`-t 8`); a lone `-` and any non-dash token are file
/// operands; `--` ends option scanning (BSD getopt), after which everything is
/// an operand; any other option letter is [`ExpandError::Usage`]. The old front
/// door matched only the whole token `-t`, so the C-legal attached form
/// `expand -t8 file` fell through to a usage error — this parser accepts it. A
/// stop list `parse_stops` rejects, or a `-t`/`-N` with no value, is
/// [`ExpandError::IllegalStops`].
pub fn parse_expand_args<'a>(args: &'a [&'a [u8]]) -> Result<ExpandArgs<'a>, ExpandError> {
    let mut stops: Vec<usize> = Vec::new();
    let mut operands: Vec<&'a [u8]> = Vec::new();
    let mut index = 0;
    // Obsolete leading `-<digit>` run (expand.c:70-73), consumed before getopt.
    while index < args.len() {
        let token: &[u8] = args[index];
        if token.len() >= 2 && token[0] == b'-' && token[1].is_ascii_digit() {
            let spec = &token[1..];
            stops = parse_stops(spec).ok_or(ExpandError::IllegalStops)?;
            index += 1;
        } else {
            break;
        }
    }
    // getopt("t:").
    while index < args.len() {
        let arg: &[u8] = args[index];
        if arg == b"--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        if arg.len() > 1 && arg[0] == b'-' {
            if arg[1] != b't' {
                return Err(ExpandError::Usage);
            }
            let spec: &[u8] = if arg.len() > 2 {
                &arg[2..]
            } else {
                index += 1;
                if index >= args.len() {
                    return Err(ExpandError::IllegalStops);
                }
                args[index]
            };
            stops = parse_stops(spec).ok_or(ExpandError::IllegalStops)?;
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    Ok(ExpandArgs { stops, operands })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_eight_column_stops() {
        assert_eq!(expand("a\tb", &[]), "a       b");
        assert_eq!(expand("\t", &[]), "        ");
        // The C's do-while emits at least one space even on a stop, then
        // pads to the next multiple of eight (expand.c:101-106).
        assert_eq!(expand("12345678\t", &[]), "12345678        ");
    }

    #[test]
    fn test_single_stop_multiples() {
        let stops = [4];
        assert_eq!(expand("a\tb", &stops), "a   b");
        assert_eq!(expand("abcd\t", &stops), "abcd    ");
    }

    #[test]
    fn test_stop_list_picks_next_beyond_column() {
        let stops = [1, 5, 20];
        assert_eq!(expand("ab\t", &stops), "ab   ");
        // Past the last stop: a single space.
        assert_eq!(
            expand("aaaaaaaaaaaaaaaaaaaa\t", &stops),
            "aaaaaaaaaaaaaaaaaaaa "
        );
    }

    #[test]
    fn test_backspace_pulls_the_column_and_passes_through() {
        // The backspace bytes pass through untouched (expand.c:128-133);
        // only the column tracking pulls back.
        assert_eq!(expand("abc\x08\x08x", &[4]), "abc\u{8}\u{8}x".to_string());
        // A tab right after sees the pulled-back column: x sits at
        // column 2, the tab pads to the stop at 4.
        assert_eq!(
            expand("abc\x08\x08x\t", &[4]),
            "abc\u{8}\u{8}x  ".to_string()
        );
    }

    #[test]
    fn test_newline_resets_the_column() {
        let stops = [4];
        assert_eq!(expand("ab\nc\td", &stops), "ab\nc   d");
    }

    #[test]
    fn test_stop_list_validation() {
        assert!(parse_stops(b"1,4,8").is_some());
        assert!(parse_stops(b"4,2").is_none());
        assert!(parse_stops(b"0").is_none());
        assert!(parse_stops(b"257").is_none());
    }

    #[test]
    fn test_parse_expand_attached_and_separate_stops() {
        use alloc::vec;
        // No options → default stops, one operand.
        assert_eq!(
            parse_expand_args(&[b"f"]),
            Ok(ExpandArgs {
                stops: Vec::new(),
                operands: vec![b"f"]
            })
        );
        // `-t 8` value as the next token.
        assert_eq!(
            parse_expand_args(&[b"-t", b"8", b"f"]).map(|a| (a.stops, a.operands)),
            Ok((vec![8usize], vec![b"f".as_slice()]))
        );
        // The regression: the attached form `-t8` is accepted, not a usage error.
        assert_eq!(
            parse_expand_args(&[b"-t8", b"f"]).map(|a| (a.stops, a.operands)),
            Ok((vec![8usize], vec![b"f".as_slice()]))
        );
        // `-t` takes a list, not just one column.
        assert_eq!(
            parse_expand_args(&[b"-t", b"1,4,8"]).map(|a| a.stops),
            Ok(vec![1, 4, 8])
        );
    }

    #[test]
    fn test_parse_expand_obsolete_and_errors() {
        use alloc::vec;
        // Obsolete leading `-N` form (expand.c:70) sets the stops.
        assert_eq!(
            parse_expand_args(&[b"-8", b"f"]).map(|a| (a.stops, a.operands)),
            Ok((vec![8usize], vec![b"f".as_slice()]))
        );
        // A lone `-` is a file operand, not an option.
        assert_eq!(
            parse_expand_args(&[b"-"]).map(|a| a.operands),
            Ok(vec![b"-".as_slice()])
        );
        // `--` ends options; the dash token after it is an operand.
        assert_eq!(
            parse_expand_args(&[b"--", b"-t"]).map(|a| a.operands),
            Ok(vec![b"-t".as_slice()])
        );
        // An unknown option letter → usage.
        assert_eq!(parse_expand_args(&[b"-z", b"f"]), Err(ExpandError::Usage));
        // `-t` with a stop list parse_stops rejects → IllegalStops.
        assert_eq!(
            parse_expand_args(&[b"-t", b"0"]),
            Err(ExpandError::IllegalStops)
        );
        // `-t` with no following value → IllegalStops.
        assert_eq!(parse_expand_args(&[b"-t"]), Err(ExpandError::IllegalStops));
    }
}
