//! Stream merging for `paste`.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD):
//! `paste [-s] [-d list] file ...`. The loop is written as `paste.c:70`'s
//! `while ((ch = getopt(argc, argv, "d:s")) != -1)`, and `paste.c:80-82`'s
//! `case '?': default: usage();` **exits right inside the loop** — which makes it
//! the exact opposite of its sibling `seq` (which does `errflg++` and keeps
//! scanning): this branch's loop **can accumulate only one** libc report line —
//! `jot` sits in the same column (`jot.c:150-152` is `default: usage();`; the
//! reference measures both `-ZQ` and `-Z -Q` reporting only `Z`).
//!
//! The delimiter table is converted and counted in place by `tr()` (`paste.c:199-226`):
//! three escapes `\n`, `\t`, `\0`, and an **unrecognized escape drops the
//! backslash** (`default: *arg = ch;`, the opposite of `seq`'s `unescape`); an
//! empty table yields `errx(1, "no delimiters specified.")`. Each item in the
//! table is **one byte**; a zero-valued item still occupies a slot but prints
//! nothing (`if (ch) putchar(ch)`), so `-d '\0'` means "nothing between the two
//! columns", not "one fewer column".

use alloc::vec;
use alloc::vec::Vec;

/// One inter-column delimiter slot: `None` means the slot exists but prints
/// nothing (the `-d '\0'` item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteOptions {
    /// The delimiter table (by byte), converted and counted in the loop by
    /// [`tr_delims`]; without `-d` it is one tab (C's `dflt_delim`,
    /// `paste.c:61-63`).
    pub delimiters: Vec<u8>,
    /// `-s`: merge each operand's lines **in sequence** into one line instead of
    /// merging in parallel across operands.
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

/// What is on hand once the loop has run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteArgs<'a> {
    pub options: PasteOptions,
    /// The loop **does not reorder**: the first non-option token and every token
    /// after it are operands (`argc -= optind; argv += optind;` at
    /// `paste.c:85-86`).
    pub operands: Vec<&'a [u8]>,
}

/// Front-door errors, split into two branches per C's exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    /// An option letter outside the set, or `-d` missing its value: libc prints
    /// one line followed immediately by one usage line, exit 1
    /// (`paste.c:80-82`, `:229-233`). This branch's loop exits right here, so
    /// `reports` holds at most one entry.
    Usage { reports: Vec<crate::GetoptReport> },
    /// `-d ''`: `tr()` counts zero delimiters (`paste.c:221-223`),
    /// `errx(1, "no delimiters specified.")`; the sentence **carries its own
    /// period** and no `strerror`.
    NoDelimiters,
}

/// The option loop of `paste.c:70` plus the operand decision. `optstring` is
/// `"d:s"`: `d` takes a value, `s` is a flag.
///
/// Three points differ from the siblings, all measured against the reference:
/// ① the loop exits at the **first** bad letter (`usage()` is called directly
///    inside the `switch`), so `paste -sZ` prints one libc line plus one usage
///    line, and a bad letter after `-d ''` reports "no delimiters" rather than
///    the unknown letter — the **order** of the error exits is the order of the
///    scan;
/// ② operands have **no count check**: `paste` with no operand neither reads
///    standard input nor errors; it exits 0 printing nothing;
/// ③ the `-d` value may be attached to this token (`-d,`; in `-ds` the `s` is
///    the value, not a flag).
pub fn parse_paste_args<'a>(args: &[&'a [u8]]) -> Result<PasteArgs<'a>, PasteError> {
    const OPTSTRING: &[u8] = b"d:s";
    let mut options = PasteOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        let token = args[i];
        // `getopt.c:74-78`: the first character is not `-` (or the whole token is
        // a single `-`) ⇒ end the loop and do not advance `optind`;
        // `getopt.c:81-85`: `--` ends the loop and consumes itself.
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
                // `getopt.c:95-104` prints that line; C's `default: usage()` then
                // exits immediately
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
                // `tr(delim)` runs right inside `case 'd'` (`paste.c:73-75`), so
                // an empty table exits at this instant and the remaining letters
                // and operands never get their turn.
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

/// C's `tr()`: convert `\n`, `\t`, `\0` in place; an unrecognized escape
/// **drops the backslash** and keeps the character that follows; return the
/// **slot count** (a zero-valued item still counts as one slot). The empty
/// string ⇒ [`PasteError::NoDelimiters`].
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
        // A trailing backslash: C's `(ch = *p++)` reads that `\0` and **ends the
        // whole loop**, so the backslash itself never becomes an item (the loop
        // condition at `paste.c:204`).
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

/// The **byte** of the `gap`-th delimiter slot (the table cycles by slot). A
/// zero-valued slot and an empty table both return `None` — C's
/// `if (ch) putchar(ch)` (`paste.c:131-133`, `:148-150`, `:184-185`) is exactly
/// "occupy a slot and print nothing". Returns `u8` rather than `char`:
/// `putchar` takes that byte itself, and routing it through a `char` field into
/// a `String` would write a bare `ff` as the two bytes `c3 bf`.
fn delimiter_byte(delimiters: &[u8], gap: usize) -> Option<u8> {
    let byte = *delimiters.get(gap % delimiters.len())?;
    if byte == 0 { None } else { Some(byte) }
}

/// Serial mode (`-s`): merge one stream's lines into one line using the cycling
/// delimiters (`paste.c:177-191`).
///
/// The cursor here starts at the table's **first item** (unlike parallel mode,
/// see [`join_parallel`]), advances one slot per emitted delimiter, and returns
/// to the head after a full cycle; a zero-valued slot prints nothing but still
/// consumes a slot.
///
/// Lines and delimiters are all bytes: C's `sequential` takes lines with `fgets`
/// and emits line content with `printf("%s", buf)` (`paste.c:175-191`), decoding
/// nothing throughout, so a non-UTF-8 byte stays in its original place on that
/// line (a zero byte is a separate matter on that leg: `%s` truncates there,
/// registered elsewhere).
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

/// Parallel mode: each row takes one line from each **column** (`paste.c:111-140`).
///
/// `columns[k]` is the **already-open stream** that column `k` points at
/// (`fpp[cnt]`): the same stream may appear in several columns, and then they
/// **share one cursor** — the reference `paste - -` (both columns stdin) on
/// `p\nq\n` prints `p\tq\n` (one row, one line per column), whereas opening the
/// same file twice yields two cursors (`paste a.txt a.txt` prints `1\t1`,
/// measured to the same shape).
///
/// The cursor starts at the **NUL past the table's end** (`dp = delim + delimcnt`,
/// `paste.c:112`), so the slot padded before the first column of each row is a
/// NUL that prints nothing, and `delim[0]` is first used from the second column
/// on. More to the point, there are two padding rules: when some stream runs out
/// early, the following columns **pad a delimiter for the exhausted column too**
/// (`paste.c:127-133`'s "catch-up" loop walks by column number, not by how many
/// were actually emitted), and at end of line each remaining column gets one more
/// (`paste.c:144-150`). The reference `paste -d '+:' a.txt b.txt a.txt` (a has
/// three lines, b two) gives `3+:3` for the third row: the `+` is padded for the
/// exhausted b, and the `:` is the slot in front of the third column itself.
///
/// Lines and delimiters are bytes (as in [`join_serial`]): `parallel`'s `fgetln`
/// returns the whole line including its newline (`paste.c:121`), the trailing
/// newline is subtracted (`:138-139`), and the line is emitted at the **measured
/// length** (`printf("%.*s", (int)line_len, line)` at `:140`) — the length comes
/// from `fgetln`, not from an encoding calculation.
pub fn join_parallel(columns: &[usize], sources: &[&[&[u8]]], options: &PasteOptions) -> Vec<u8> {
    let slots = &options.delimiters;
    let count = slots.len();
    let mut out: Vec<u8> = Vec::new();
    let mut cursor: Vec<usize> = vec![0; sources.len()];
    loop {
        // C's `dp`: start "past the table's end", treat landing there as NUL and
        // wrap to the table head
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

/// An options-only peek (operands are produced by the front door
/// [`parse_paste_args`]).
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
        // Unknown letter: one libc line + usage, and **only one libc line**
        // (`usage()` exits inside the loop)
        assert_eq!(
            parse_paste_args(&["-Z".as_bytes(), "-h".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
        // A missing `-d` value takes libc's second report form
        assert_eq!(
            parse_paste_args(&["-d".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::MissingValue('d')]
            })
        );
        // A bad letter after good ones also reports only the bad one
        assert_eq!(
            parse_paste_args(&["-sZ".as_bytes(), "a.txt".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
    }

    #[test]
    fn test_error_order_follows_the_scan() {
        // `-d ''` hits `tr()` inside the loop ⇒ "no delimiters" is reported
        // first, the later unknown letter never gets its turn
        assert_eq!(
            parse_paste_args(&["-d".as_bytes(), "".as_bytes(), "-Z".as_bytes()]),
            Err(PasteError::NoDelimiters)
        );
        // Conversely, with the bad letter first, `-d` never gets its turn
        assert_eq!(
            parse_paste_args(&["-Z".as_bytes(), "-d".as_bytes(), "".as_bytes()]),
            Err(PasteError::Usage {
                reports: vec![crate::GetoptReport::UnknownOption('Z')]
            })
        );
    }

    #[test]
    fn test_operands_are_not_reordered() {
        // The first non-option token and everything after it are operands: in
        // `paste a.txt -s` the `-s` is the second file name
        let parsed = args(&["a.txt".as_bytes(), "-s".as_bytes()]);
        assert_eq!(parsed.operands, vec!["a.txt".as_bytes(), "-s".as_bytes()]);
        assert!(!parsed.options.serial);
        // `--` ends the loop and is not itself an operand
        let parsed = args(&["--".as_bytes(), "-s".as_bytes()]);
        assert_eq!(parsed.operands, vec!["-s".as_bytes()]);
        // A bare `-` is an operand (C treats it as standard input), and it does
        // **not** end the scan before the loop
        let parsed = args(&["-".as_bytes(), "b.txt".as_bytes()]);
        assert_eq!(parsed.operands, vec!["-".as_bytes(), "b.txt".as_bytes()]);
    }

    #[test]
    fn test_no_operand_is_not_an_error() {
        // C neither reads standard input nor errors on zero operands: exit 0
        // printing nothing (`paste.c:88-92` hands argc to parallel)
        let parsed = args(&[]);
        assert!(parsed.operands.is_empty());
        assert_eq!(parsed.operands.len(), 0);
        let parsed = args(&["--".as_bytes()]);
        assert!(parsed.operands.is_empty());
    }

    #[test]
    fn test_attached_values_and_clusters() {
        // The value of `-d,` is attached to this token
        let parsed = args(&["-d,".as_bytes(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![b',']);
        // `-ds`: the `s` is `d`'s value, not a flag
        let parsed = args(&["-ds".as_bytes(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![b's']);
        assert!(!parsed.options.serial);
        // When the value comes from the next token, `optind` must step over it
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
        // `\0` is one **slot** with value 0
        assert_eq!(tr_delims(b"\\0".as_slice()).unwrap(), vec![0]);
        // An unrecognized escape drops the backslash (the opposite of `seq`'s
        // unescape)
        assert_eq!(tr_delims(b"\\q".as_slice()).unwrap(), vec![b'q']);
        // `a\0b` is three slots: 'a', a zero-valued one, 'b'
        assert_eq!(tr_delims(b"a\\0b".as_slice()).unwrap(), vec![b'a', 0, b'b']);
        // A lone trailing backslash: the backslash never becomes a slot
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
        // A zero-valued slot still occupies a slot but prints nothing
        let options = PasteOptions {
            delimiters: vec![0],
            serial: true,
        };
        let three: [&[u8]; 3] = [b"a", b"b", b"c"];
        assert_eq!(join_serial(&three, &options), b"abc\n".as_slice());
        // Empty stream: C's sequential prints not a single character (not even a
        // newline)
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
        // Third row: b is exhausted ⇒ the `+` is padded for b, the `:` is the
        // slot in front of the third column (same shape in the reference)
        let c: [&[u8]; 3] = [b"x", b"y", b"z"];
        assert_eq!(
            join_parallel(&[0, 1, 2], &[&a[..], &b[..], &c[..]], &options),
            b"1+a:x\n2+b:y\n3+:z\n".as_slice()
        );
        // Two columns share one stream (the `paste - -` shape): there is only
        // one cursor
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
        // `-d '\0'`: nothing between the two columns, but the slot is occupied
        // (reference `1a\n2b\n3\n`)
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
        // With three slots `a\0b`, the first slot for both columns is 'a' ⇒
        // `1aa` (b's first line is itself an `a`)
        let options = PasteOptions {
            delimiters: vec![b'a', 0, b'b'],
            serial: false,
        };
        assert_eq!(
            join_parallel(&[0, 1], &[&a[..], &b[..]], &options),
            b"1aa\n2ab\n3a\n".as_slice()
        );
    }

    /// A delimiter slot emits one **byte**: `putchar` takes the table item
    /// itself (`paste.c:131-133`, `:148-150`, `:184-185`), so a bare `ff` goes
    /// out as `ff`. The old spelling `slots[dp] as char` promoted it to U+00FF,
    /// which lands in a `String` as the two bytes `c3 bf` — one byte more than C.
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

    /// Non-UTF-8 bytes in the line content stay the bytes of that line: the old
    /// shell spelling `from_utf8(input).unwrap_or("")` made every line of the
    /// whole file vanish (measured: the reference prints `a\xffb\ta\xffb`,
    /// while this shell returned empty).
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

    /// The letter position of the unknown-letter line is **that one byte** in
    /// argv: `optopt = *place++` at
    /// `minix3/lib/libc/stdlib/getopt.c:79` takes a byte, and
    /// `minix3/usr.bin/paste/paste.c:80-82`'s `default: usage()` exits on the
    /// spot, so a bare `ff` has only one letter position to report. Since the
    /// argument seam hands bytes through, this leg no longer passes through
    /// `from_utf8_lossy` — the old spelling folded `ff` into `ef bf bd`, which is
    /// why `ef` appeared in the report; the reference measures a single `ff` as
    /// well.
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

    /// The `-d` **value** is likewise a byte string: C's `tr()` scans that table
    /// slot by slot (`paste.c:199-226`), so a non-UTF-8 byte is just **one slot**
    /// in the table. The old spelling went through lossy decoding and turned one
    /// `ff` into the **three** bytes `ef bf bd`, so `paste -d\xff a.txt b.txt`
    /// yielded `1\xefa` (`ef` was used on that line, `bf` and `bd` fell to the
    /// next two lines), whereas the reference measures one `ff` per line.
    #[test]
    fn parse_paste_keeps_a_nonutf8_delimiter_byte_as_one_slot() {
        let attached: Vec<u8> = [b"-d".as_slice(), b"\xff".as_slice()].concat();
        let parsed = args(&[attached.as_slice(), "a.txt".as_bytes(), "b.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![0xff]);
        assert_eq!(
            parsed.operands,
            vec!["a.txt".as_bytes(), "b.txt".as_bytes()]
        );
        // Attached to ASCII it is **two slots**: one `ff`, one `+`
        let two: Vec<u8> = [b"-d".as_slice(), b"\xff".as_slice(), b"+".as_slice()].concat();
        let parsed = args(&[two.as_slice(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![0xff, b'+']);
        // Taking the value from the next token is byte-wise as well
        let separate: Vec<u8> = [b"\xff".as_slice(), b"+".as_slice()].concat();
        let parsed = args(&["-d".as_bytes(), separate.as_slice(), "a.txt".as_bytes()]);
        assert_eq!(parsed.options.delimiters, vec![0xff, b'+']);
    }
}
