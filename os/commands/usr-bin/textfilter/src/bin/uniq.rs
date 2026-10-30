//! Minix-RS uniq — adjacent duplicate filter.
//!
//! Ground truth: `minix3/usr.bin/uniq/uniq.c` (NetBSD), `uniq [-cdu] [-f fields]
//! [-s chars] [input_file [output_file]]`. The run grouping, the `show()` flag
//! guard, and the count formatting are the library's (`Uniq`, `UniqFlags`,
//! `Run::format`, which prints the `%4d` count prefix without the
//! trailing newline — the doing half adds it). The three mode flags are
//! independent bits exactly as C keeps `cflag`/`dflag`/`uflag`, so `-c -d`,
//! `-c -u` and `-d -u` are all accepted and composed. The old-style skip forms
//! are folded the way C's `obsolete()` does it before `getopt`: `-2` means
//! `-f 2` (skip two fields) and `+3` means `-s 3` (skip three characters).
//! Input is stdin, split into
//! newline-stripped lines before the run handler sees them. The field-skip
//! comparison (`-f fields`, `-s chars`, C `skip()`) is the library's
//! `comparison_key`: lines are compared on the skipped key but the full line is
//! reported. NetBSD `uniq` has no `-i` (its getopt string is `"-cdf:s:u"`), so
//! case-folding is not offered. The input operand (read a file instead of stdin)
//! is wired; the second operand is written through C's `fopen(name, "w")` shape —
//! created if absent, truncated at open time even if the input then turns out to
//! be unreadable.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::uniq::{Uniq, UniqFlags};
use minix_textfilter::GetoptReport;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Every diagnostic line in C begins with `getprogname()` (`err(1, "%s", name)`
    // at `uniq.c:216-221`, `errx` at `:91-98`, usage at `:252-256`), so the command
    // name must not be hard-coded.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let (flags, skip_fields, skip_chars, operands) = match parse_uniq_args(&argv) {
        Ok(parsed) => parsed,
        // libc prints one line first; then `case '?'` leads to usage (uniq.c:102-104).
        Err(UniqArgError::Getopt(report)) => {
            // The option letter goes through `line_bytes`: it is fetched by byte
            // anyway (`bytes[j] as char`), and libc's letter slot is a single byte
            // (`optopt = *place++` at `getopt.c:79`, `%c` at `:103`).  A locally built
            // reference binary measured `-é`, `-cé` and `-ué` as
            // `uniq: unknown option -- ` + the single byte `c3` + newline, then usage
            // and exit 1.
            support::warn(&report.line_bytes(&prog));
            usage(&prog)
        }
        // C's two errx calls (`uniq.c:91`, `:97`): they go through `vwarnx`, append
        // no `errno` sentence, but **splice the bad value back into the message**
        // (there is a `%s` in it).
        Err(UniqArgError::IllegalField(value)) => {
            support::warn(&illegal_value(&prog, b"field", &value));
            support::terminate(1);
        }
        Err(UniqArgError::IllegalChar(value)) => {
            support::warn(&illegal_value(&prog, b"character", &value));
            support::terminate(1);
        }
    };

    // C `switch (argc) { 0: stdin/stdout; 1: file/stdout; 2: file/file;
    // default: usage }` — this arm is a **bare usage**, with no preceding libc line.
    if operands.len() > 2 {
        usage(&prog);
    }
    // `case 2` opens the input handle first (`ifp = file(argv[0], "r")`), then
    // creates the output (`ofp = file(argv[1], "w")`, which truncates at that
    // moment), and reads the input only afterwards. Keeping that order is what
    // makes `uniq file file` produce an empty report, as C does, and keeps an
    // unopenable input from creating an output file.
    let source = match operands.first() {
        None => None,
        // The operand is a path: raw non-UTF-8 bytes are folded lossily once at the
        // open boundary (the library's `&str` entry point); until then they stay
        // bytes.
        Some(raw) => {
            let path = String::from_utf8_lossy(raw);
            match support::open_input(&path) {
                Ok(source) => Some(source),
                // C's `file()` is `err(1, "%s", name)` (`uniq.c:216-221`): fatal, and
                // all three parts of that message are required.  The old code treated
                // `err(3)` as if it were `errx(3)`.
                Err(code) => {
                    support::warn_line(&prog, &path, code);
                    support::terminate(1);
                }
            }
        }
    };
    let sink = match operands.get(1) {
        None => None,
        Some(raw) => {
            let path = String::from_utf8_lossy(raw);
            match support::create_sink(&path) {
                Ok(sink) => Some(sink),
                Err(code) => {
                    support::warn_line(&prog, &path, code);
                    support::terminate(1);
                }
            }
        }
    };
    let input = match source {
        Some(source) => source.read_to_end(),
        None => support::read_stdin(),
    };
    let lines = support::lines_of(&input);
    let mut handler = Uniq::new_with_skip(flags, skip_fields, skip_chars);
    let mut body: Vec<u8> = Vec::new();
    let mut out = [0u8; 4096];
    for line in &lines {
        if let Some(run) = handler.push(*line) {
            report(&mut handler, &run, &mut out, &mut body);
        }
    }
    if let Some(run) = handler.finish() {
        report(&mut handler, &run, &mut out, &mut body);
    }
    // The whole report is assembled first because the input is already read to
    // the end; either destination gets the same bytes C's stdio would flush.
    //
    // C's output leg **never checks for write failures**: `file(argv[1], "w")` calls
    // `err(1, "%s", name)` only when the **open** fails
    // (`minix3/usr.bin/uniq/uniq.c:216-221`); afterwards the return values of
    // `fprintf` and `fputs` are ignored and the whole file contains no `ferror` or
    // `fflush`.  A reference binary behaves the same way: `uniq_ref < in >/dev/full`
    // prints nothing and **exits 0**.  Both shapes used to report a failed write as
    // exit 1 (the stdout shape said nothing at all), and the output-file shape was
    // worse still — it printed a two-part `uniq: <path>`, which is neither C's
    // message nor C's exit code.
    match sink {
        Some(sink) => {
            let _ = sink.finish(&body);
        }
        None => {
            let _ = support::emit_error(&body);
        }
    }
    support::terminate(0);
}

/// Append one kept run to the collected report: the library formats it (the
/// `%4d` count prefix with `-c`, no newline), and the newline C's `fprintf` gets
/// from the input line is added here because `lines_of` stripped it.
fn report(
    handler: &mut Uniq<'_>,
    run: &minix_textfilter::uniq::Run<'_>,
    out: &mut [u8],
    body: &mut Vec<u8>,
) {
    if !handler.keep(run) {
        return;
    }
    match handler.format(run, out) {
        Ok(len) => {
            body.extend_from_slice(&out[..len]);
            // C `uniq` prints the **first line** of a run, and that line carries the
            // newline state of the line itself: for `printf 'x\nx'` the printed line is
            // the first one, with its newline; the final `x` without a newline merely
            // joined the same run (a reference binary measured `x\ny`).
            body.extend_from_slice(run.line.eol());
        }
        Err(_) => support::terminate(1),
    }
}

/// C's two `errx` calls (`uniq.c:90-98`) are assembled byte by byte: the bad value
/// comes from argv's raw bytes, and the echoed slot must not be rewritten by any
/// character decoding (`uniq: illegal field skip value: <raw bytes>`).  The program
/// name leg still goes through the lossy text entry point shared by the whole
/// family.
fn illegal_value(prog: &str, kind: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(prog.as_bytes());
    out.extend_from_slice(b": illegal ");
    out.extend_from_slice(kind);
    out.extend_from_slice(b" skip value: ");
    out.extend_from_slice(value);
    out.extend_from_slice(b"\n");
    out
}

fn usage(prog: &str) -> ! {
    // Verbatim counterpart of `uniq.c:252-256` (one line joined with single spaces, exit 1).
    support::warn(
        format!("usage: {prog} [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n")
            .as_bytes(),
    );
    support::terminate(1);
}

/// C `obsolete()`'s pre-pass over `uniq`'s argv (`uniq.c`): an old-style
/// `-<digit>…` token means "skip that many fields" (`-2` → `-f2`) and `+<digit>…`
/// means "skip that many characters" (`+3` → `-s3`) — C builds the replacement
/// with `asprintf("-%c%s", ap[0] == '+' ? 's' : 'f', ap + 1)`.
///
/// The walk's stopping rule is C's, and it is not the one a `tail` reader would
/// guess: a token whose second char is not a digit is `continue`d over (an option
/// letter, known or not), while anything that is neither `-` nor `+` prefixed —
/// an operand — ends the whole pre-pass. So `-c -2` folds the `-2`, and `-f 2 -3`
/// does not: the walk stops at the value word `2`, leaving `-3` for the option
/// scanner to reject. Index 0 (the program name) is left alone, exactly as C's
/// `*++argv` loop does.
/// The shape the argument layer hands back: flags + the two skip counts + the
/// remaining operands (as raw argv bytes).  It has a name because spelling the
/// tuple out would trip clippy's type-complexity lint.
type Parsed = (UniqFlags, usize, usize, Vec<Vec<u8>>);

fn obsolete_args(argv: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = argv.to_vec();
    for index in 1..argv.len() {
        let bytes = argv[index].clone();
        let first = bytes.first().copied();
        if first != Some(b'-') && first != Some(b'+') {
            break;
        }
        if first == Some(b'-') && bytes.get(1) == Some(&b'-') {
            break;
        }
        let Some(second) = bytes.get(1).copied() else {
            continue;
        };
        if !second.is_ascii_digit() {
            continue;
        }
        let letter = if first == Some(b'+') { b's' } else { b'f' };
        let mut replaced: Vec<u8> = Vec::with_capacity(bytes.len());
        replaced.push(b'-');
        replaced.push(letter);
        replaced.extend_from_slice(&bytes[1..]);
        out[index] = replaced;
    }
    out
}

/// Parse `uniq`'s argv the way NetBSD `getopt(argc, argv, "-cdf:s:u")` does
/// (`uniq.c`): the three mode flags `-c`/`-d`/`-u` are independent bits (C keeps
/// `cflag`/`dflag`/`uflag` and composes them in `show()`, so `-c -d` and even
/// `-d -u` are legal rather than a clash), value flags `-f`/`-s` take the rest of
/// the token or the next argv, `--` ends options, and everything else is an
/// operand.
///
/// The three exits are not the same thing, so this no longer returns a `()`: an
/// unknown letter or a missing value produces libc's line, while a value that does
/// not convert to a legal skip count produces C's own `errx` (carrying the bad
/// value, without usage).
fn parse_uniq_args(argv: &[Vec<u8>]) -> Result<Parsed, UniqArgError> {
    // C runs `obsolete(argv)` before `getopt`, so the folding happens first here
    // too and every later step sees the rewritten words.
    let scanned = obsolete_args(argv);
    let mut flags = UniqFlags::default();
    let mut fields = 0usize;
    let mut chars = 0usize;
    let mut operands: Vec<Vec<u8>> = Vec::new();
    let mut i = 1usize;
    while i < scanned.len() {
        let tok: Vec<u8> = scanned[i].clone();
        if tok == b"--" {
            operands.extend(scanned[i + 1..].iter().cloned());
            return Ok((flags, fields, chars, operands));
        }
        if tok.len() <= 1 || tok[0] != b'-' {
            operands.push(tok.clone());
            i += 1;
            continue;
        }
        let bytes = tok.as_slice();
        let mut j = 1usize;
        while j < bytes.len() {
            match bytes[j] {
                b'c' | b'd' | b'u' => {
                    match bytes[j] {
                        b'c' => flags.count = true,
                        b'd' => flags.repeats_only = true,
                        _ => flags.uniques_only = true,
                    }
                    j += 1;
                }
                b'f' | b's' => {
                    let letter = bytes[j];
                    let is_field = letter == b'f';
                    j += 1;
                    let value = if j < bytes.len() {
                        let v: Vec<u8> = bytes[j..].to_vec();
                        j = bytes.len();
                        v
                    } else {
                        i += 1;
                        match scanned.get(i) {
                            Some(v) => v.clone(),
                            // C: the optstring does not start with `:`, so on a
                            // missing value getopt prints a line first and then
                            // returns `'?'` (`getopt.c:132-136`).
                            None => {
                                return Err(UniqArgError::Getopt(GetoptReport::MissingValue(
                                    letter as char,
                                )));
                            }
                        }
                    };
                    // C checks only two things: a negative conversion, or leftover
                    // bytes at `*p` (`uniq.c:90-98`).  The old code treated both as
                    // usage errors, getting the exit and the message wrong.
                    let n = skip_count(&value).ok_or(if is_field {
                        UniqArgError::IllegalField(value)
                    } else {
                        UniqArgError::IllegalChar(value)
                    })?;
                    if is_field {
                        fields = n;
                    } else {
                        chars = n;
                    }
                }
                _ => {
                    return Err(UniqArgError::Getopt(GetoptReport::UnknownOption(
                        bytes[j] as char,
                    )));
                }
            }
        }
        i += 1;
    }
    Ok((flags, fields, chars, operands))
}

/// The three argument-error shapes of `uniq` (corresponding to C's three real exits).
#[derive(Debug, Clone, PartialEq, Eq)]
enum UniqArgError {
    /// Unknown option letter or missing value: libc prints a line first, then usage.
    Getopt(GetoptReport),
    /// The `-f` skip count is invalid (C `errx "illegal field skip value: %s"`).
    IllegalField(Vec<u8>),
    /// The `-s` skip count is invalid (C `errx "illegal character skip value: %s"`).
    IllegalChar(Vec<u8>),
}

/// A model of C's `strtol(optarg, &p, 10)` plus `if (numfields < 0 || *p)`:
/// skip leading whitespace, accept one optional sign, read until a non-digit;
/// **when no digit is converted at all the value is 0** and `p` stays at the start
/// of the number, so the empty string is a legal zero (`-f ''` is not an error in
/// C), whereas for `-f -` `p` stops at that minus sign — that is the boundary
/// between the two shapes.  In C an overflow past `long` merely sets `errno` (uniq
/// does not check it), so this function saturates instead of erroring.
///
/// `None` corresponds to C's errx.
fn skip_count(value: &[u8]) -> Option<usize> {
    let bytes = value;
    let mut cursor = 0usize;
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let number_start = cursor;
    let negative = bytes.get(cursor) == Some(&b'-');
    if matches!(bytes.get(cursor), Some(b'-') | Some(b'+')) {
        cursor += 1;
    }
    let digits_from = cursor;
    let mut number = 0usize;
    while let Some(&byte) = bytes.get(cursor) {
        if !byte.is_ascii_digit() {
            break;
        }
        number = number
            .saturating_mul(10)
            .saturating_add(usize::from(byte - b'0'));
        cursor += 1;
    }
    let tail = if cursor > digits_from {
        cursor
    } else {
        // No digit converted: `strtol` puts `endptr` back at the start of the number
        // (sign included).
        number_start
    };
    if negative || tail < bytes.len() {
        return None;
    }
    Some(number)
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<Vec<u8>> {
        let mut v: Vec<Vec<u8>> = Vec::new();
        v.push(b"uniq".to_vec());
        v.extend(words.iter().map(|w| w.as_bytes().to_vec()));
        v
    }

    fn strs(words: &[&str]) -> Vec<Vec<u8>> {
        words.iter().map(|w| w.as_bytes().to_vec()).collect()
    }

    /// The two legs of the byte-shaped input: when `-f` is rejected the raw argv
    /// bytes come back, and when `obsolete()` folds an old-style `-2<ff>` the
    /// trailing raw byte must not be folded into a replacement character (C's
    /// `asprintf` assembles bytes).
    #[test]
    fn a_raw_byte_is_kept_by_the_ring_and_by_the_obsolete_fold() {
        assert_eq!(
            parse_uniq_args(&[b"uniq".to_vec(), b"-f".to_vec(), vec![0xffu8]]),
            Err(UniqArgError::IllegalField(vec![0xffu8])),
            "被拒的字段跳过值该交回原字节"
        );
        assert_eq!(
            obsolete_args(&[b"uniq".to_vec(), vec![b'-', b'2', 0xffu8]])[1],
            [b"-f2".as_slice(), &[0xffu8][..]].concat(),
            "旧式折叠只补字母，尾巴的裸字节原样带过去"
        );
    }

    #[test]
    fn parse_field_and_char_values() {
        let (flags, f, c, ops) = parse_uniq_args(&argv(&["-f", "2", "-s", "1", "in"])).unwrap();
        assert_eq!(flags, UniqFlags::default());
        assert_eq!((f, c), (2, 1));
        assert_eq!(ops, strs(&["in"]));
    }

    #[test]
    fn parse_attached_value_and_mode() {
        let (flags, f, _c, _ops) = parse_uniq_args(&argv(&["-c", "-f3"])).unwrap();
        assert!(flags.count);
        assert_eq!(f, 3);
    }

    #[test]
    fn parse_composes_mode_flags_as_c_does() {
        // C keeps three independent ints, so `-d -u` is not a clash: both bits
        // end up set, and it is `show()` that turns that combination into
        // "suppress every run". Clustered letters (`-cdu`) set all three too.
        let (flags, ..) = parse_uniq_args(&argv(&["-d", "-u"])).unwrap();
        assert!(flags.repeats_only && flags.uniques_only && !flags.count);
        let (clustered, ..) = parse_uniq_args(&argv(&["-cdu"])).unwrap();
        assert!(clustered.count && clustered.repeats_only && clustered.uniques_only);
    }

    #[test]
    fn parse_rejects_bad_value_and_unknown_option() {
        assert_eq!(
            parse_uniq_args(&argv(&["-f", "x"])),
            Err(UniqArgError::IllegalField(b"x".to_vec()))
        ); // C's errx, not usage
        assert_eq!(
            parse_uniq_args(&argv(&["-s", "x"])),
            Err(UniqArgError::IllegalChar(b"x".to_vec()))
        ); // each value gets its own message
           // Note the difference between the two shapes: the `-1` in `-f -1` is first
           // folded by `obsolete()` into `-f1`, so what optarg sees — and what is
           // spliced back into the message — is `-f1` (a host reference binary reads
           // the same: `uniq_ref: illegal field skip value: -f1`); exercising the
           // "converted a negative" branch requires the attached form `-f-1`.
        assert_eq!(
            parse_uniq_args(&argv(&["-f", "-1"])),
            Err(UniqArgError::IllegalField(b"-f1".to_vec()))
        );
        assert_eq!(
            parse_uniq_args(&argv(&["-f-1"])),
            Err(UniqArgError::IllegalField(b"-1".to_vec()))
        ); // negative -> C's `numfields < 0`
        assert_eq!(
            parse_uniq_args(&argv(&["-i"])),
            Err(UniqArgError::Getopt(GetoptReport::UnknownOption('i')))
        ); // not a NetBSD uniq flag
        assert_eq!(
            parse_uniq_args(&argv(&["-f"])),
            Err(UniqArgError::Getopt(GetoptReport::MissingValue('f')))
        ); // a missing value takes libc's line
           // The two strtol shapes: with no digit converted the value is 0 and `p`
           // stays at the start of the number, so the empty string is legal (a host
           // reference binary agrees: it proceeds to the read path like the no-option
           // baseline), while a lone minus sign is not.
        assert_eq!(
            parse_uniq_args(&argv(&["-f", ""])).unwrap().1,
            0,
            "`-f ''` 在 C 是合法的零"
        );
        assert_eq!(
            parse_uniq_args(&argv(&["-s", " -3"])),
            Err(UniqArgError::IllegalChar(b" -3".to_vec()))
        );
        assert_eq!(parse_uniq_args(&argv(&["-f", " 2"])).unwrap().1, 2); // leading whitespace
        assert_eq!(
            parse_uniq_args(&argv(&["-f", "5x"])),
            Err(UniqArgError::IllegalField(b"5x".to_vec()))
        ); // leftover
    }

    #[test]
    fn parse_obsolete_skip_forms() {
        // C `obsolete()` folds `-<digit>…` into `-f<digit>…` and `+<digit>…` into
        // `-s<digit>…` before `getopt` ever sees them.
        let (flags, f, c, ops) = parse_uniq_args(&argv(&["-2"])).unwrap();
        assert!(!flags.count);
        assert_eq!((f, c, ops), (2, 0, Vec::new()));
        let (_m, f, c, ops) = parse_uniq_args(&argv(&["-3", "in"])).unwrap();
        assert_eq!((f, c), (3, 0));
        assert_eq!(ops, strs(&["in"]));
        let (_m, f, c, _ops) = parse_uniq_args(&argv(&["+4"])).unwrap();
        assert_eq!((f, c), (0, 4));
        // The walk continues past option letters, so a mode flag in front does
        // not stop the fold.
        let (flags, f, _c, _ops) = parse_uniq_args(&argv(&["-c", "-2"])).unwrap();
        assert!(flags.count);
        assert_eq!(f, 2);
    }

    #[test]
    fn parse_obsolete_walk_stops_at_a_plain_word() {
        // C's pre-pass `return`s at the first token that is not `-`/`+` prefixed,
        // and a value word counts as one: in `-f 2 -3` the `2` ends the walk, so
        // `-3` is never folded into `-f3` and the option scanner rejects the
        // unknown letter `3`.
        assert!(parse_uniq_args(&argv(&["-f", "2", "-3"])).is_err());
        // Without the stop, the same three words would have folded `-3`.
        let (_m, f, _c, _ops) = parse_uniq_args(&argv(&["-f", "2"])).unwrap();
        assert_eq!(f, 2);
    }

    #[test]
    fn parse_double_dash_collects_operands() {
        let (_m, _f, _c, ops) = parse_uniq_args(&argv(&["-c", "--", "-x", "y"])).unwrap();
        assert_eq!(ops, strs(&["-x", "y"]));
    }

    /// The output operand goes through `create_sink`/`finish`: the bytes C's
    /// stdio would flush land in the file, and an existing longer file is
    /// truncated rather than overwritten in place.
    #[test]
    fn output_operand_is_created_and_truncated() {
        let mut path = std::env::temp_dir();
        path.push(format!("uniq-outfile-{}", std::process::id()));
        std::fs::write(&path, b"stale content that is longer than the report\n").unwrap();
        let sink = support::create_sink(path.to_str().unwrap()).unwrap();
        // Opening already truncates: the old bytes are gone before anything is
        // written, which is C's `fopen(..., "w")` at open time.
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        sink.finish(b"   2 a\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"   2 a\n");
        let sink = support::create_sink(path.to_str().unwrap()).unwrap();
        sink.finish(b"").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"");
        std::fs::remove_file(&path).unwrap();
    }

    /// An output operand that cannot be opened is fatal in C (`file()` calls
    /// `err(1, "%s", name)`); a directory name is the reachable case.
    #[test]
    fn output_operand_reports_an_unopenable_path() {
        let mut path = std::env::temp_dir();
        path.push("definitely-not-a-file-unique-dir-for-uniq-test");
        std::fs::create_dir_all(&path).unwrap();
        assert!(support::create_sink(path.to_str().unwrap()).is_err());
        std::fs::remove_dir(&path).unwrap();
    }
}
