//! Minix-RS uniq — adjacent duplicate filter.
//!
//! Ground truth: `minix3/usr.bin/uniq/uniq.c` (NetBSD), `uniq [-c | -d |
//! -u] [-f fields] [-s chars] [input [output]]`. The run grouping,
//! mode selection, and count formatting are the library's (`Uniq`,
//! `UniqMode`, `Run::format`, which prints the count prefix without the
//! trailing newline — the doing half adds it). Input is stdin, split into
//! newline-stripped lines before the run handler sees them. The field-skip
//! comparison (`-f fields`, `-s chars`, C `skip()`) is the library's
//! `comparison_key`: lines are compared on the skipped key but the full line is
//! reported. NetBSD `uniq` has no `-i` (its getopt string is `"-cdf:s:u"`), so
//! case-folding is not offered. The input operand (read a file instead of stdin)
//! is wired; the second operand (an output file, a create-and-write path) is
//! reported as not modelled rather than truncating a file, so use a shell
//! redirect.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::uniq::{Uniq, UniqMode};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let (mode, skip_fields, skip_chars, operands) = match parse_uniq_args(&argv) {
        Ok(parsed) => parsed,
        Err(_) => usage(),
    };

    // C `switch (argc) { 0: stdin/stdout; 1: file/stdout; 2: file/file;
    // default: usage }`. The output-file form (a create-and-write path) is not
    // modelled; the input file and stdout are.
    if operands.len() > 2 {
        usage();
    }
    if operands.len() == 2 {
        support::warn(b"uniq: the output-file operand is not modelled; redirect with the shell\n");
        support::terminate(1);
    }
    let input = match operands.first() {
        None => support::read_stdin(),
        Some(path) => read_file(path),
    };
    let lines = support::lines_of(&input);
    let mut handler = Uniq::new_with_skip(mode, skip_fields, skip_chars);
    let mut out = [0u8; 4096];
    for line in &lines {
        if let Some(run) = handler.push(line) {
            report(&mut handler, &run, &mut out);
        }
    }
    if let Some(run) = handler.finish() {
        report(&mut handler, &run, &mut out);
    }
    support::terminate(0);
}

fn report(handler: &mut Uniq<'_>, run: &minix_textfilter::uniq::Run<'_>, out: &mut [u8]) {
    if !handler.keep(run) {
        return;
    }
    match handler.format(run, out) {
        Ok(len) => {
            support::emit(&out[..len]);
            support::emit(b"\n");
        }
        Err(_) => support::terminate(1),
    }
}

/// Read an input file to the end; an unopenable file is fatal (C `file()`
/// `err`s), matching the input-only path.
fn read_file(path: &str) -> Vec<u8> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("uniq: {path}\n").as_bytes());
            support::terminate(1);
        }
    };
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => data.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    //[SD19-gate] SD-19: close 后 fd 不再使用
    #[allow(clippy::let_underscore_must_use)]
    let _ = close(fd);
    data
}

fn usage() -> ! {
    support::warn(b"usage: uniq [-c | -d | -u] [-f fields] [-s chars] [input [output]]\n");
    support::terminate(1);
}

/// Parse `uniq`'s argv the way NetBSD `getopt(argc, argv, "-cdf:s:u")` does
/// (uniq.c:77): boolean mode flags `-c`/`-d`/`-u` (mutually exclusive, matching
/// the C composition guard), value flags `-f`/`-s` taking the rest of the token
/// or the next argv (rejecting a non-numeric or negative value), `--` ends
/// options, and everything else is an operand. Returns `(mode, fields, chars,
/// operands)`; `Err` on a bad or unknown option (the caller prints usage).
fn parse_uniq_args(argv: &[String]) -> Result<(UniqMode, usize, usize, Vec<String>), ()> {
    let mut mode = UniqMode::All;
    let mut mode_seen = false;
    let mut fields = 0usize;
    let mut chars = 0usize;
    let mut operands: Vec<String> = Vec::new();
    let mut i = 1usize;
    while i < argv.len() {
        let tok = argv[i].as_str();
        if tok == "--" {
            operands.extend(argv[i + 1..].iter().cloned());
            return Ok((mode, fields, chars, operands));
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
                    let next = match bytes[j] {
                        b'c' => UniqMode::Counted,
                        b'd' => UniqMode::Repeated,
                        _ => UniqMode::Unique,
                    };
                    if mode_seen && mode != next {
                        return Err(());
                    }
                    mode = next;
                    mode_seen = true;
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
                        argv.get(i).cloned().unwrap_or_default()
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
    Ok((mode, fields, chars, operands))
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
        let (mode, f, c, ops) = parse_uniq_args(&argv(&["-f", "2", "-s", "1", "in"])).unwrap();
        assert_eq!(mode, UniqMode::All);
        assert_eq!((f, c), (2, 1));
        assert_eq!(ops, strs(&["in"]));
    }

    #[test]
    fn parse_attached_value_and_mode() {
        let (mode, f, _c, _ops) = parse_uniq_args(&argv(&["-c", "-f3"])).unwrap();
        assert_eq!(mode, UniqMode::Counted);
        assert_eq!(f, 3);
    }

    #[test]
    fn parse_rejects_composed_modes() {
        assert!(parse_uniq_args(&argv(&["-d", "-u"])).is_err());
    }

    #[test]
    fn parse_rejects_bad_value_and_unknown_option() {
        assert!(parse_uniq_args(&argv(&["-f", "x"])).is_err()); // non-numeric
        assert!(parse_uniq_args(&argv(&["-f", "-1"])).is_err()); // negative
        assert!(parse_uniq_args(&argv(&["-i"])).is_err()); // not a NetBSD uniq flag
    }

    #[test]
    fn parse_double_dash_collects_operands() {
        let (_m, _f, _c, ops) = parse_uniq_args(&argv(&["-c", "--", "-x", "y"])).unwrap();
        assert_eq!(ops, strs(&["-x", "y"]));
    }
}
