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

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let (flags, skip_fields, skip_chars, operands) = match parse_uniq_args(&argv) {
        Ok(parsed) => parsed,
        Err(_) => usage(),
    };

    // C `switch (argc) { 0: stdin/stdout; 1: file/stdout; 2: file/file;
    // default: usage }`.
    if operands.len() > 2 {
        usage();
    }
    // `case 2` opens the input handle first (`ifp = file(argv[0], "r")`), then
    // creates the output (`ofp = file(argv[1], "w")`, which truncates at that
    // moment), and reads the input only afterwards. Keeping that order is what
    // makes `uniq file file` produce an empty report, as C does, and keeps an
    // unopenable input from creating an output file.
    let source = match operands.first() {
        None => None,
        Some(path) => match support::open_input(path) {
            Ok(source) => Some(source),
            Err(()) => {
                support::warn(format!("uniq: {path}\n").as_bytes());
                support::terminate(1);
            }
        },
    };
    let sink = match operands.get(1) {
        None => None,
        Some(path) => match support::create_sink(path) {
            Ok(sink) => Some(sink),
            Err(()) => {
                support::warn(format!("uniq: {path}\n").as_bytes());
                support::terminate(1);
            }
        },
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
        if let Some(run) = handler.push(line) {
            report(&mut handler, &run, &mut out, &mut body);
        }
    }
    if let Some(run) = handler.finish() {
        report(&mut handler, &run, &mut out, &mut body);
    }
    // The whole report is assembled first because the input is already read to
    // the end; either destination gets the same bytes C's stdio would flush.
    match sink {
        Some(sink) => {
            if let Err(()) = sink.finish(&body) {
                support::warn(format!("uniq: {}\n", operands[1]).as_bytes());
                support::terminate(1);
            }
        }
        None => support::emit(&body),
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
            body.push(b'\n');
        }
        Err(_) => support::terminate(1),
    }
}

fn usage() -> ! {
    support::warn(b"usage: uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n");
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
fn obsolete_args(argv: &[String]) -> Vec<String> {
    let mut out: Vec<String> = argv.to_vec();
    for index in 1..argv.len() {
        let token = argv[index].as_str();
        let bytes = token.as_bytes();
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
        let letter = if first == Some(b'+') { 's' } else { 'f' };
        out[index] = format!("-{letter}{}", &token[1..]);
    }
    out
}

/// Parse `uniq`'s argv the way NetBSD `getopt(argc, argv, "-cdf:s:u")` does
/// (`uniq.c`): the three mode flags `-c`/`-d`/`-u` are independent bits (C keeps
/// `cflag`/`dflag`/`uflag` and composes them in `show()`, so `-c -d` and even
/// `-d -u` are legal rather than a clash), value flags `-f`/`-s` take the rest of
/// the token or the next argv (rejecting a non-numeric value), `--` ends options,
/// and everything else is an operand. Returns `(flags, fields, chars, operands)`;
/// `Err` on a bad or unknown option (the caller prints usage).
fn parse_uniq_args(argv: &[String]) -> Result<(UniqFlags, usize, usize, Vec<String>), ()> {
    // C runs `obsolete(argv)` before `getopt`, so the folding happens first here
    // too and every later step sees the rewritten words.
    let scanned = obsolete_args(argv);
    let mut flags = UniqFlags::default();
    let mut fields = 0usize;
    let mut chars = 0usize;
    let mut operands: Vec<String> = Vec::new();
    let mut i = 1usize;
    while i < scanned.len() {
        let tok = scanned[i].as_str();
        if tok == "--" {
            operands.extend(scanned[i + 1..].iter().cloned());
            return Ok((flags, fields, chars, operands));
        }
        if tok.len() <= 1 || !tok.starts_with('-') {
            operands.push(String::from(tok));
            i += 1;
            continue;
        }
        let bytes = tok.as_bytes();
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
                    let is_field = bytes[j] == b'f';
                    j += 1;
                    let value = if j < bytes.len() {
                        let v = String::from(&tok[j..]);
                        j = bytes.len();
                        v
                    } else {
                        i += 1;
                        scanned.get(i).cloned().unwrap_or_default()
                    };
                    let n: usize = value.parse().map_err(|_| ())?;
                    if is_field {
                        fields = n;
                    } else {
                        chars = n;
                    }
                }
                _ => return Err(()),
            }
        }
        i += 1;
    }
    Ok((flags, fields, chars, operands))
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

    fn argv(words: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        v.push(String::from("uniq"));
        v.extend(words.iter().map(|w| String::from(*w)));
        v
    }

    fn strs(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| String::from(*w)).collect()
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
        assert!(parse_uniq_args(&argv(&["-f", "x"])).is_err()); // non-numeric
        assert!(parse_uniq_args(&argv(&["-f", "-1"])).is_err()); // negative
        assert!(parse_uniq_args(&argv(&["-i"])).is_err()); // not a NetBSD uniq flag
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
