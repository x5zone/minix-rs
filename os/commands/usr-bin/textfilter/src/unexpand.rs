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
/// unexpand.c:93-115), each stop parses as a base-zero number and must
/// be strictly greater than the previous. `None` marks any violation.
/// C's two bad shapes for the `-t` stop list each get their own sentence; never
/// flatten them into one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRejection<'a> {
    /// `unexpand.c:101-103`: `strtoul` leaves trailing characters, or overflows
    /// to `ULONG_MAX` (`errx(EXIT_FAILURE, "Invalid tabstop `%s'", tab)`). It
    /// carries that stop's **original bytes**—what C hands to `%s` is exactly
    /// the word `strsep` cut out.
    Invalid(&'a [u8]),
    /// `unexpand.c:108-112`: not greater than the previous stop
    /// (`errx(... "Bad tabstop spec `%s', must be greater than the previous `%zu'", ...)`).
    NotIncreasing { spec: &'a [u8], previous: usize },
}

/// Splits the stop list the way C's `strsep(&optarg, ", \t")` does: commas,
/// spaces, and tabs are all separators, empty fields are skipped (so both
/// `-t ""` and `-t ,` mean "no stop at all", with C keeping `nstops` at 0 and
/// behaving as if `-t` were absent), and each piece is parsed in **base 0**
/// (`0x` is hexadecimal, a `0` prefix is octal, `unexpand.c:100`'s
/// `strtoul(tab, &ep, 0)`; the reference build measures `-t 010` as 8 and
/// `-t 0x10` as 16).
pub fn parse_stops(spec: &[u8]) -> Result<Vec<usize>, StopRejection<'_>> {
    let mut stops: Vec<usize> = Vec::new();
    for piece in spec.split(|byte| matches!(byte, b',' | b' ' | b'\t')) {
        if piece.is_empty() {
            continue;
        }
        let value = match base_zero(piece) {
            Some(value) => value,
            None => return Err(StopRejection::Invalid(piece)),
        };
        // "Not greater than the previous stop" is only checked when there is a
        // previous stop, and equality is not "greater" either (the reference
        // build rejects `-t 4,4` alike).
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

/// C's `strtoul(piece, &ep, 0)` plus "reject if `*ep` is nonzero" and "reject
/// when `ERANGE` returned `ULONG_MAX`". The base-0 prefix rules follow C: `0x`/`0X`
/// is hexadecimal, a leading `0` is octal, everything else decimal. Overflow is
/// caught with `checked_*` (both this shell and C are 64-bit unsigned wide);
/// falling back to `None` is C's Invalid.
fn base_zero(piece: &[u8]) -> Option<usize> {
    let (radix, rest) = if piece.len() > 1 && piece[0] == b'0' {
        match piece[1] {
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
    for &byte in rest {
        let digit = char::from(byte).to_digit(radix as u32)?;
        value = value.checked_mul(radix)?.checked_add(digit as usize)?;
    }
    Some(value)
}

/// Converts blank runs of the line into tabs.
///
/// Faithful to `tabify` (unexpand.c:148-214): a display column (`dcol`)
/// walks the input — spaces add one, tabs quantize to the next stop —
/// and each non-blank byte flushes the accumulated gap as "tabs then spaces":
/// C does this in the Output-tabs and Output-spaces arms at
/// `unexpand.c:167-205`, and what triggers it must be **a non-blank byte**.
fn flush_gap(out: &mut String, ocol: &mut usize, dcol: usize, stops: &[usize], limit: usize) {
    if stops.is_empty() {
        while (*ocol + DEFAULT_STOP) / DEFAULT_STOP <= dcol / DEFAULT_STOP && dcol - *ocol >= 2 {
            out.push('\t');
            *ocol = (1 + *ocol / DEFAULT_STOP) * DEFAULT_STOP;
        }
        while *ocol < dcol {
            out.push(' ');
            *ocol += 1;
        }
        return;
    }
    let mut n = 0usize;
    while n < stops.len() && stops[n] <= *ocol {
        n += 1;
    }
    while n < stops.len() && stops[n] <= dcol && *ocol < dcol && *ocol < limit {
        out.push('\t');
        *ocol = stops[n];
        n += 1;
    }
    while *ocol < dcol && *ocol < limit {
        out.push(' ');
        *ocol += 1;
    }
}

/// columns) followed by spaces. A backspace pulls both columns back.
/// Without `-a` the first non-blank byte ends the tabification and the
/// rest of the line passes through verbatim.
pub fn unexpand_line(line: &str, options: &UnexpandOptions, terminated: bool) -> String {
    let bytes = line.as_bytes();
    // C subtracts a `size_t` here (`unexpand.c:146`); with a stop of 0 it wraps
    // to the maximum instead of stopping, so `wrapping_sub` is C's step; a bare
    // subtraction traps first in a host debug build and `-t 0` would crash.
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
            if options.stops.is_empty() {
                dcol = (1 + dcol / DEFAULT_STOP) * DEFAULT_STOP;
                cursor += 1;
                continue;
            }
            let mut n = 0usize;
            while n < options.stops.len() && options.stops[n].wrapping_sub(1) < dcol {
                n += 1;
            }
            if n + 1 < options.stops.len() && options.stops[n].wrapping_sub(1) < limit {
                dcol = options.stops[n];
                cursor += 1;
                continue;
            }
            // When the next stop is out of reach C does not `continue`: the test
            // at `unexpand.c:160-165` fails and control falls into the "flush the
            // stop, then `putchar(*p)`" arm below (`:195`), so the tab is handed
            // back as-is and both columns advance by one. This must fall into the
            // same arm here, not skip the tab as an already-consumed blank.
        }

        // Non-blank byte: flush the gap as tabs then spaces.
        flush_gap(&mut out, &mut ocol, dcol, &options.stops, limit);

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
            // (unexpand.c:207-213).
            while cursor < bytes.len() {
                out.push(bytes[cursor] as char);
                cursor += 1;
            }
            break;
        }
    }
    // The line ends with a newline: that newline is the non-blank byte that ends
    // the blank run, and C flushes at that moment. When the input has no trailing
    // newline, `tabify` walks off the buffer and returns directly
    // (`unexpand.c:214-215`), flushing not one byte.
    if terminated {
        flush_gap(&mut out, &mut ocol, dcol, &options.stops, limit);
    }
    out
}

/// Converts every line of the collected input.
pub fn unexpand(input: &str, options: &UnexpandOptions) -> Vec<String> {
    let text = alloc::str::from_utf8(input.as_bytes()).unwrap_or("");
    let mut pieces: Vec<&str> = text.split('\n').collect();
    if input.as_bytes().ends_with(b"\n") {
        pieces.pop();
    }
    let last = pieces.len().saturating_sub(1);
    let tail_terminated = crate::window::final_line_terminated(input.as_bytes());
    pieces
        .iter()
        .enumerate()
        .map(|(index, line)| unexpand_line(line, options, index != last || tail_terminated))
        .collect()
}

/// `unexpand`'s parsed argv tail: the merge options and the file operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpandArgs<'a> {
    /// `-a` / `-t` options (which columns to tabify).
    pub options: UnexpandOptions,
    /// File operands in order. Bytes, because C hands `argv[0]` to `freopen`
    /// (`unexpand.c:129`) and prints it inside `Cannot open \`%s'` (`:130`).
    pub operands: Vec<&'a [u8]>,
}

/// `unexpand` argument-parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnexpandError<'a> {
    /// C's own `usage()` line: `-a` and `-t` conflict (`unexpand.c:89-95`'s
    /// `if (nstops) usage()` and `if (all) usage()`).
    Usage,
    /// Out-of-set letter: libc prints one line first (`getopt.c:101-104`), then
    /// C's `default: usage()` appends a second line.
    UnknownOption(char),
    /// No value after `-t`: libc prints `option requires an argument -- t`
    /// (`getopt.c:132-136`), then `default: usage()` adds the second line.
    MissingValue(char),
    /// One bad stop in `-t`'s stop list; hands back whichever of C's two
    /// sentences applies.
    BadStops(StopRejection<'a>),
}

/// Splits `unexpand`'s argument tail into [`UnexpandArgs`], following C's
/// `getopt(argc, argv, "at:")` (`unexpand.c:86`). Three things must be read
/// letter by letter through getopt's cluster pointer rather than by whole-string
/// comparison: ① `-a` and `-t` are mutually exclusive, and whichever arrives
/// second calls `usage()` (`unexpand.c:88-95`), so `-at4` is usage and not a
/// combined form; ② for an out-of-set letter libc prints one line first
/// (`getopt.c:101-104`), handing back the **first** character after `-`
/// (measured: `-Za` reports only `Z`); ③ `-t`'s value may be attached to this
/// token (`-t4,8`) or be a separate argument (`-t 4,8`), and when that argument
/// is absent libc prints `option requires an argument -- t`
/// (`getopt.c:132-136`) instead of going straight to usage.
///
/// No reordering: as soon as a non-option token appears, `getopt.c:74-78` returns
/// -1 on the spot, and that token and everything after it land in the
/// `argv += optind` tail, all treated as **file operands** (measured against the
/// reference build: in `unexpand f -a`, the `-a` is `freopen`ed as the second
/// file name and reported as `Cannot open `-a'`). `--` ends the option loop and
/// is not itself an operand (`getopt.c:81-85`); a bare `-` is an operand
/// (`getopt.c:87-92` does not advance `optind`, and C `unexpand.c:129` hands it
/// to `freopen`).
pub fn parse_unexpand_args<'a>(
    args: &'a [&'a [u8]],
) -> Result<UnexpandArgs<'a>, UnexpandError<'a>> {
    let mut options = UnexpandOptions::default();
    let mut operands: Vec<&'a [u8]> = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == b"--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg;
        if bytes.len() < 2 || bytes[0] != b'-' {
            operands.extend_from_slice(&args[index..]);
            break;
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'a' => {
                    // `-a` after the stop list is C's conflict → usage.
                    if !options.stops.is_empty() {
                        return Err(UnexpandError::Usage);
                    }
                    options.all = true;
                    pos += 1;
                }
                b't' => {
                    // `-t` after `-a` is the same conflict → usage.
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
                    // The loop itself walks by byte (`pos` is a byte index), and
                    // the letter handed back must be that byte too: `getopt.c:79`'s
                    // `optopt = *place++` takes one byte at a time and `:103`'s
                    // `%c` writes it out as-is, so `-é` (`c3 a9`) reports `c3`,
                    // not the whole character.
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

    /// The fixture argv: `std::env::args_os()` hands back bytes, so readable
    /// words are turned into byte vectors here; the value shapes that must assert
    /// bytes themselves (`é` and a bare `ff`) are written as literal bytes.
    fn argv<'a>(words: &[&'a str]) -> Vec<&'a [u8]> {
        words.iter().map(|word| word.as_bytes()).collect()
    }

    #[test]
    fn test_leading_blank_run_becomes_tabs_at_eight() {
        let options = UnexpandOptions::default();
        // Eight blanks land exactly on the stop: one tab. Four blanks
        // stay spaces — the tab loop requires crossing a stop boundary.
        assert_eq!(unexpand_line("        x", &options, true), "\tx");
        assert_eq!(unexpand_line("    x", &options, true), "    x");
    }

    #[test]
    fn test_two_space_gap_becomes_one_tab() {
        let options = UnexpandOptions::default();
        // Without `-a` the first non-blank byte ends the conversion, so
        // the interior blank run passes through untouched.
        assert_eq!(unexpand_line("ab        c", &options, true), "ab        c");
    }

    #[test]
    fn test_single_space_stays_a_space() {
        let options = UnexpandOptions::default();
        assert_eq!(unexpand_line("a b", &options, true), "a b");
    }

    #[test]
    fn test_without_a_only_leading_blanks_convert() {
        let options = UnexpandOptions::default();
        // Four leading blanks stay spaces (no stop crossed); everything
        // after the first non-blank passes verbatim.
        assert_eq!(
            unexpand_line("    ab        cd", &options, true),
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
        let result = unexpand_line("ab\x08\x08        c", &options, true);
        assert_eq!(result, "ab\x08\x08\tc");
    }

    #[test]
    fn test_stop_list_bounds_the_tab_region() {
        let options = UnexpandOptions {
            stops: vec![10, 20],
            all: false,
        };
        // Five leading blanks stay spaces: the first stop is 10.
        assert_eq!(unexpand_line("     x", &options, true), "     x");
        // Twelve blanks cross the 10 stop: one tab then two spaces.
        assert_eq!(unexpand_line("            x", &options, true), "\t  x");
    }

    /// The stop list's two sentences and base 0 pinned one by one: three kinds of
    /// separator, empty fields skipped, `0x`/leading `0` going hexadecimal and
    /// octal (the reference build measures `-t 010` as 8 and `-t 0x10` as 16),
    /// and the decreasing arm reporting **the later one** plus the previous value.
    #[test]
    fn the_stop_list_follows_strtoul_base_zero_and_two_sentences() {
        assert_eq!(parse_stops(b"1,2"), Ok(vec![1, 2]));
        assert_eq!(parse_stops(b"4 8"), Ok(vec![4, 8]), "空格也是分隔符");
        assert_eq!(parse_stops(b"4,\t8"), Ok(vec![4, 8]), "制表符也是分隔符");
        assert_eq!(parse_stops(b""), Ok(vec![]), "空表合法，行为同不给 -t");
        assert_eq!(parse_stops(b",,"), Ok(vec![]), "空字段跳过");
        assert_eq!(
            parse_stops(b"0"),
            Ok(vec![0]),
            "C 不挡零档（参照件印 `0 0`）"
        );
        assert_eq!(parse_stops(b"0x10"), Ok(vec![16]), "base 0 认十六进制");
        assert_eq!(parse_stops(b"010"), Ok(vec![8]), "前导零是八进制");
        // The first sentence: trailing characters, an out-of-range octal digit,
        // overflow.
        assert_eq!(parse_stops(b"x"), Err(StopRejection::Invalid(b"x")));
        assert_eq!(parse_stops(b"3x"), Err(StopRejection::Invalid(b"3x")));
        assert_eq!(parse_stops(b"08"), Err(StopRejection::Invalid(b"08")));
        assert_eq!(
            parse_stops(b"18446744073709551616"),
            Err(StopRejection::Invalid(b"18446744073709551616")),
            "溢出在 C 是 ERANGE 且回 ULONG_MAX，同判 Invalid"
        );
        // The second sentence: not greater than the previous stop; it reports the
        // later one and the previous value.
        assert_eq!(
            parse_stops(b"8,4"),
            Err(StopRejection::NotIncreasing {
                spec: b"4",
                previous: 8
            })
        );
        assert_eq!(
            parse_stops(b"4,4"),
            Err(StopRejection::NotIncreasing {
                spec: b"4",
                previous: 4
            }),
            "相等也不算「更大」"
        );
    }

    /// The option loop's four exits and its lack of reordering: an out-of-set
    /// letter reports only the first byte after the dash, a missing value is a
    /// different sentence, whichever of `-a`/`-t` arrives second triggers usage,
    /// and everything after the first non-option token becomes operands.
    #[test]
    fn the_option_ring_reports_libc_letters_and_stops_at_the_first_operand() {
        assert!(parse_unexpand_args(&argv(&["-a"])).unwrap().options.all);
        assert_eq!(
            parse_unexpand_args(&argv(&["-t", "4"]))
                .unwrap()
                .options
                .stops,
            vec![4]
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-t4,8"]))
                .unwrap()
                .options
                .stops,
            vec![4, 8],
            "附值形是 C 合法的（旧前门只整段比对，会把 `-t8 file` 判成 usage）"
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-at4"])),
            Err(UnexpandError::Usage)
        );
        // `-ta` is not "the two conflict" but "`-t` took the attached value `a'":
        // C's getopt, on seeing `t:`, hands back the rest of the same string as
        // optarg (`getopt.c:117-118`), so this takes the
        // `Invalid tabstop `a'` path (the reference build measures the same
        // sentence).
        assert_eq!(
            parse_unexpand_args(&argv(&["-ta"])),
            Err(UnexpandError::BadStops(StopRejection::Invalid(b"a")))
        );
        // Exclusivity also covers separate spellings: `-t` after `-a`, or `-a`
        // after the stop list.
        assert_eq!(
            parse_unexpand_args(&argv(&["-a", "-t", "8"])),
            Err(UnexpandError::Usage)
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-t", "8", "-a"])),
            Err(UnexpandError::Usage)
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-z", "f"])),
            Err(UnexpandError::UnknownOption('z'))
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-Za"])),
            Err(UnexpandError::UnknownOption('Z')),
            "聚簇只报第一个字母"
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["--x"])),
            Err(UnexpandError::UnknownOption('-'))
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-t"])),
            Err(UnexpandError::MissingValue('t')),
            "缺值是 libc 的另一句而不是 usage"
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-t", "8,4"])),
            Err(UnexpandError::BadStops(StopRejection::NotIncreasing {
                spec: b"4",
                previous: 8
            }))
        );
        // No reordering: the `-a` in `unexpand f -a` is the second file name (C
        // would freopen it).
        assert_eq!(
            parse_unexpand_args(&argv(&["f", "-a"])).unwrap().operands,
            vec![b"f".as_slice(), b"-a".as_slice()]
        );
        assert!(
            !parse_unexpand_args(&argv(&["f", "-a"]))
                .unwrap()
                .options
                .all,
            "操作数之后的 -a 不再被当选项读"
        );
        // `--` itself is not an operand; a bare `-` is.
        assert_eq!(
            parse_unexpand_args(&argv(&["--", "-a"])).unwrap().operands,
            vec![b"-a".as_slice()]
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-"])).unwrap().operands,
            vec![b"-".as_slice()]
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["--"])).unwrap().operands,
            Vec::<&[u8]>::new()
        );
    }

    /// The out-of-set letter handed back is the **first byte** after the dash, not
    /// the first character: `-\u{e9}` (`c3 a9`) is U+00C3 in the `char` domain (the
    /// single byte in the measured message of the reference build
    /// `tmp/unexpand-312/unexpand_ref` is `303`; see the same-shape gate in
    /// `tests/unexpand_face.rs`). This branch's loop already walks byte indices,
    /// so this gate pins "do not promote the byte to a character at hand-back
    /// time".
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_unexpand_args(&argv(&["-\u{e9}"])),
            Err(UnexpandError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // `a` is a legal, value-less letter in the option string, so the cluster
        // keeps consuming into the next byte.
        assert_eq!(
            parse_unexpand_args(&argv(&["-a\u{e9}"])),
            Err(UnexpandError::UnknownOption('\u{00c3}')),
            "-a\u{e9}"
        );
        // The loop stops at the first bad letter: `-Zé` reports only `Z`.
        assert_eq!(
            parse_unexpand_args(&argv(&["-Z\u{e9}"])),
            Err(UnexpandError::UnknownOption('Z'))
        );
        // The ASCII shape must not be disturbed by this change.
        assert_eq!(
            parse_unexpand_args(&argv(&["-q"])),
            Err(UnexpandError::UnknownOption('q'))
        );
        // The reverse: a value-taking letter swallows every byte after it as the
        // value, so the letter slot never gets a turn (the reference build prints
        // the same `Invalid tabstop `a\303\251'`).
        assert_eq!(
            parse_unexpand_args(&argv(&["-t\u{e9}"])),
            Err(UnexpandError::BadStops(StopRejection::Invalid(b"\xc3\xa9")))
        );
    }

    /// The value slots, separator string, and operands all hand back argv's
    /// **original bytes**, and the letter slot is the first byte after the dash.
    /// All three C sentences operate on bytes: `errx(EXIT_FAILURE, "Invalid
    /// tabstop `%s'", tab)` (`unexpand.c:101-103`) and `errx(... "Bad tabstop
    /// spec `%s', must be greater than the previous `%zu'", tab, ...)`
    /// (`:108-112`) print the word `strsep` cut out, while
    /// `err(EXIT_FAILURE, "Cannot open `%s'", argv[0])` (`:129-130`) prints the
    /// operand itself, and the separator test (`:96`'s
    /// `strsep(&optarg, ", \t")`) is a **byte set**. `getopt.c:79`'s
    /// `optopt = *place++` takes one byte at a time and `:103`'s `%c` writes it
    /// out as-is, so `-\xff` reports `ff`, not U+FFFD; `char::from(0xff)` is
    /// U+00FF, the same value that `%c` prints.
    #[test]
    fn the_stop_spec_and_the_operand_carry_the_argv_bytes() {
        assert_eq!(
            parse_stops(b"4\xff"),
            Err(StopRejection::Invalid(b"4\xff")),
            "坏档位的回声带的是那一支的原字节，包括那枚 ff"
        );
        assert_eq!(
            parse_stops(b"8,4\xff"),
            Err(StopRejection::Invalid(b"4\xff")),
            "回声带的是 `strsep` 切出来的那一支，不是整张 `-t` 值（参照件同形见 \
             `tmp/unexpand-470/measure.log` 的 `tabstop-list-mixed` 那一档）"
        );
        assert_eq!(
            parse_stops(b"8,4"),
            Err(StopRejection::NotIncreasing {
                spec: b"4",
                previous: 8
            }),
            "第二支的回声位也是字节——但注意这一臂**不可能**带非法字节：走到「不比前一支大」\
             之前那一支必须已经按 base 0 解析成数，而解析只认 ASCII 数字与 `0x`/八进制前缀。\
             所以这句的字节缝是「形状对得上」而不是「有可观察差」，与批次四百六十七那枚\
             无分辨力探针同一类"
        );
        assert_eq!(
            parse_unexpand_args(&[b"-\xff"]).err(),
            Some(UnexpandError::UnknownOption('\u{00ff}')),
            "短横后第一枚字节 ff 交回 U+00FF，不是 U+FFFD"
        );
        assert_eq!(
            parse_unexpand_args(&[b"-t\xff"]).err(),
            Some(UnexpandError::BadStops(StopRejection::Invalid(b"\xff"))),
            "带值字母收走的值位同样是原字节"
        );
        assert_eq!(
            parse_unexpand_args(&[b"in\xff", b"-a"]).unwrap().operands,
            vec![b"in\xff".as_slice(), b"-a".as_slice()],
            "操作数是 `freopen` 的参数，也是 `Cannot open` 那句话里的字"
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
            parse_unexpand_args(&argv(&["f"])).map(|a| (a.options, a.operands)),
            Ok((UnexpandOptions::default(), vec![b"f".as_slice()]))
        );
        // `-a` valueless, `-t` separate value.
        assert!(
            parse_unexpand_args(&argv(&["-a", "f"]))
                .map(|a| a.options.all)
                .unwrap()
        );
        assert_eq!(
            parse_unexpand_args(&argv(&["-t", "8", "f"])).map(|a| a.options.stops),
            Ok(vec![8])
        );
        // The regression: attached `-t8` is accepted (was a usage error).
        assert_eq!(
            parse_unexpand_args(&argv(&["-t8", "f"])).map(|a| (a.options.stops, a.operands)),
            Ok((vec![8], vec![b"f".as_slice()]))
        );
        // A lone `-` is an operand; `--` ends options.
        assert_eq!(
            parse_unexpand_args(&argv(&["--", "-a"])).map(|a| a.operands),
            Ok(vec![b"-a".as_slice()])
        );
    }

    #[test]
    fn a_zero_tabstop_wraps_its_limit_the_way_c_does() {
        // `unexpand.c:146`'s `limit = tabstops[nstops - 1] - 1` and the two
        // `tabstops[n] - 1` spots at `:157`/`:160` are all `size_t` subtractions:
        // with a stop of 0 they wrap to the maximum, and C neither stops nor
        // errors. The reference build measures both `-t 0` and `-t 0,8` as
        // **handing the whole line back unchanged, exit 0**
        // (`tmp/unexpand-425/ref-matrix.log`), so this shell must not crash on a
        // 0 stop either.
        assert_eq!(parse_stops(b"0"), Ok(vec![0]));
        assert_eq!(parse_stops(b"0,8"), Ok(vec![0, 8]));
        let zero = UnexpandOptions {
            stops: vec![0],
            all: false,
        };
        assert_eq!(unexpand_line("    abc", &zero, true), "    abc");
        assert_eq!(unexpand_line("  ab", &zero, true), "  ab");
        assert_eq!(unexpand_line("a\tb\tc", &zero, true), "a\tb\tc");
        let zero_then_eight = UnexpandOptions {
            stops: vec![0, 8],
            all: false,
        };
        // The reference build measures `-t 0,8` as `\tx` for eight blanks (stop
        // 8 still works as a tab exit), while `-t 0` has a single stop that is
        // the current column, so eight blanks are handed back unchanged
        // (`tmp/unexpand-425/tab-drop.log`).
        assert_eq!(unexpand_line("        x", &zero_then_eight, true), "\tx");
        assert_eq!(unexpand_line("        x", &zero, true), "        x");
        // This shape only exists to reach `wrapping_sub` in the two tab spots
        // (after `  ` the column is already 2, and stop 0 is the last stop): its
        // **output shape** belongs to another registered divergence (swallowing
        // the tab); this pass does not pin its value, only that it does not
        // crash.
        let _ = unexpand_line("  \tx", &zero_then_eight, true);
    }
    /// Reference reading `tmp/unexpand-426/ref-shapes.log`: when the next stop is
    /// out of reach C does not `continue`, and that tab falls to `putchar(*p)` in
    /// the same arm (`unexpand.c:160-165`'s test fails → `:195`).
    #[test]
    fn a_tab_beyond_the_last_stop_is_handed_back_like_c_does() {
        let eight = UnexpandOptions {
            stops: vec![8],
            all: false,
        };
        assert_eq!(unexpand_line("\ta", &eight, true), "\ta");
        assert_eq!(unexpand_line("  \tx", &eight, true), "  \tx");
        // Eight blanks first push the column to stop 8, then the tab cannot reach
        // the next stop, so it too is handed back unchanged: two tabs.
        assert_eq!(unexpand_line("        \ty", &eight, true), "\t\ty");
        assert_eq!(unexpand_line("   \t   \tw", &eight, true), "   \t   \tw");
        let four_then_eight = UnexpandOptions {
            stops: vec![4, 8],
            all: false,
        };
        // This shape's tab can reach the 1st stop (it stops at stop 0, which is
        // not the last stop), so it folds into the stop: the reference build
        // hands back `\tx`, the exact opposite of the "hand back unchanged"
        // shapes above.
        assert_eq!(unexpand_line("  \tx", &four_then_eight, true), "\tx");
        assert_eq!(unexpand_line("    \ty", &four_then_eight, true), "\t\ty");
        let zero = UnexpandOptions {
            stops: vec![0],
            all: false,
        };
        assert_eq!(unexpand_line("\ta", &zero, true), "\ta");
        assert_eq!(unexpand_line("  \tx", &zero, true), "  \tx");
    }
    /// The newline at the end of a line is the "non-blank byte that ends the blank
    /// run": a blank-only line **with its newline** must flush. Reference
    /// measurements from `tmp/unexpand-312/unexpand_ref`
    /// (`tmp/unexpand-431/eol.log`)—`-t 4,8` turns `    \n` into `\t\n`; `-t 8`
    /// turns `   \n` into `   \n` (with stop 8 out of reach only spaces remain,
    /// no folding into tabs). The gate above tests the same blank strings but
    /// without the newline in the input; the two shapes differ only in the
    /// terminator.
    #[test]
    fn a_blank_line_ending_in_a_newline_flushes_its_gap() {
        let four_then_eight = UnexpandOptions {
            stops: vec![4, 8],
            all: false,
        };
        assert_eq!(unexpand_line("    ", &four_then_eight, true), "\t");
        assert_eq!(unexpand_line("   ", &four_then_eight, true), "   ");
        let default = UnexpandOptions {
            stops: Vec::new(),
            all: true,
        };
        assert_eq!(unexpand_line("        ", &default, true), "\t");
    }

    /// The whole-input decision falls on the terminator, not the line index: a
    /// last line with a newline flushes, one without does not.
    #[test]
    fn the_last_line_flushes_only_when_the_input_terminated_it() {
        let four_then_eight = UnexpandOptions {
            stops: vec![4, 8],
            all: false,
        };
        assert_eq!(unexpand("    \n", &four_then_eight), vec!["\t".to_string()]);
        assert_eq!(unexpand("    ", &four_then_eight), vec!["".to_string()]);
        assert_eq!(
            unexpand("    \n    ", &four_then_eight),
            vec!["\t".to_string(), "".to_string()]
        );
    }

    /// Reference reading `tmp/unexpand-426/eol.log`: C flushes the gap only from a
    /// **non-blank byte**, and the loop `return`s on reaching the end of the
    /// buffer (`unexpand.c:214-215`), so a line of nothing but blanks yields **no
    /// bytes**. This function receives the line body with the newline stripped,
    /// so it matches the "last line without a newline" family; the half that
    /// carries a newline (the reference build emits a folded tab instead of an
    /// empty line) is a registered divergence, unchanged by this pass.
    #[test]
    fn a_blank_run_is_collapsed_only_by_a_non_blank_byte() {
        let eight = UnexpandOptions {
            stops: vec![8],
            all: false,
        };
        assert_eq!(unexpand_line("        ", &eight, false), "");
        assert_eq!(unexpand_line("        \ty", &eight, true), "\t\ty");
        let four_then_eight = UnexpandOptions {
            stops: vec![4, 8],
            all: false,
        };
        assert_eq!(unexpand_line("        ", &four_then_eight, false), "");
        assert_eq!(unexpand_line("    ", &four_then_eight, false), "");
        let default = UnexpandOptions {
            stops: Vec::new(),
            all: false,
        };
        assert_eq!(unexpand_line("        ", &default, false), "");
        // A gap shorter than one stop does not fold even on the "flushed by a
        // non-blank" arm: under two columns it stays as-is.
        assert_eq!(unexpand_line("  x", &default, true), "  x");
        assert_eq!(unexpand_line("   x", &default, true), "   x");
        assert_eq!(unexpand_line("", &default, true), "");
    }
}
