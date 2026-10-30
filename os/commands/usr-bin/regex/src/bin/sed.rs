//! Minix-RS sed — substitute scripts over stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/sed/` (NetBSD `main.c`/`process.c`). The
//! substitution engine is the library's (`parse_subst` + `apply`, which
//! mirror `process.c:418-422` including the empty-match advance). This
//! build carries the `s` command only, with its `p` flag and the `-n`
//! option interacting exactly as the C: without `-n` every result line
//! prints; with `-n` only lines whose `p` flag fired print. Chained
//! commands (`;`), addresses, and the other verbs are not modelled and are
//! rejected with an explicit message instead of being ignored. With file
//! operands each is opened in turn and its substituted lines stream to
//! standard output (`main.c` `mf_fgets`); an unopenable operand warns and
//! only sets the exit status, and the run continues to the next file. With
//! no operands sed reads stdin. In-place editing (`-i`/`-I`) needs the
//! create/rename surface and is not modelled; net output carries no
//! filename prefix, unlike `grep`.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_regex::pattern::{compile_basic, Pattern};
use minix_regex::sed::{apply, parse_subst, Subst};

/// Argument-splitting errors for the sed front end.
#[derive(PartialEq, Debug)]
enum SedArgError {
    /// An option other than `-n` was requested (this build only models `-n`).
    UnsupportedOption,
    /// No positional script was supplied.
    MissingScript,
}

/// Splits sed argv into `(quiet, script, file operands)`, mirroring the C:
/// `-n` sets quiet wherever it appears, the first positional is the script
/// and the rest are input files. Any other leading-dash option is rejected
/// (only `-n` is modelled here). NetBSD sed opens input files through
/// `fopen` (`main.c` `mf_fgets`), so a bare `-` is a literal file name, not
/// a stdin alias.
fn parse_sed_args<'a>(argv: &'a [&'a str]) -> Result<(bool, &'a str, Vec<&'a str>), SedArgError> {
    let mut quiet = false;
    let mut script: Option<&'a str> = None;
    let mut files: Vec<&'a str> = Vec::new();
    for &arg in argv {
        if arg == "-n" {
            quiet = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            return Err(SedArgError::UnsupportedOption);
        } else if script.is_none() {
            script = Some(arg);
        } else {
            files.push(arg);
        }
    }
    match script {
        Some(script) => Ok((quiet, script, files)),
        None => Err(SedArgError::MissingScript),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (quiet, script, files) = match parse_sed_args(&words[1..]) {
        Ok(parsed) => parsed,
        Err(SedArgError::UnsupportedOption) => {
            support::emit(b"sed: unsupported option\n");
            support::terminate(1);
        }
        Err(SedArgError::MissingScript) => {
            support::emit(b"usage: sed [-n] script [file ...]\n");
            support::terminate(1);
        }
    };
    // Only the substitute verb is modelled; anything else is a loud
    // failure, never a silent pass-through.
    if !script.starts_with('s') {
        support::emit(b"sed: only s (substitute) scripts are modelled (see 08 sec 4.4)\n");
        support::terminate(1);
    }
    let (subst, consumed) = match parse_subst(&script[1..]) {
        Ok(parsed) => parsed,
        Err(_) => {
            support::emit(b"sed: bad s command\n");
            support::terminate(1);
        }
    };
    if consumed != script.len() - 1 {
        support::emit(b"sed: chained commands are not modelled (see 08 sec 4.4)\n");
        support::terminate(1);
    }
    let pattern: Pattern = match compile_basic(subst.pattern_text) {
        Ok(pattern) => pattern,
        Err(_) => {
            support::emit(b"sed: bad s command\n");
            support::terminate(1);
        }
    };

    let mut scratch = vec![0u8; 4 * 1024 * 1024];
    let mut rval = 0;
    if files.is_empty() {
        let input = support::read_stdin();
        sed_lines(&input, quiet, &pattern, &subst, &mut scratch);
    } else {
        for file in &files {
            match read_file(file) {
                Ok(data) => sed_lines(&data, quiet, &pattern, &subst, &mut scratch),
                Err(()) => {
                    // C `mf_fgets`: an unopenable operand warns and flags the
                    // exit status but does not stop the run (main.c:449-452).
                    support::warn(format!("sed: {file}\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    support::terminate(rval);
}

/// Substitutes every line of one collected stream and emits the printed
/// lines. The print decision mirrors the C (`process.c`): without `-n` every
/// result line prints; with `-n` only lines whose `p` flag fired.
fn sed_lines(input: &[u8], quiet: bool, pattern: &Pattern, subst: &Subst<'_>, scratch: &mut [u8]) {
    for line in support::lines_of(input) {
        let (len, replaced) = apply(pattern, subst, line, scratch);
        let prints = !quiet || (replaced && subst.print);
        if prints {
            let mut result = scratch[..len].to_vec();
            result.push(b'\n');
            support::emit(&result);
        }
    }
}

/// Opens and drains one file operand. Unlike commands that abort on an
/// unreadable operand, sed warns, flags the exit status and moves on (C
/// `mf_fgets` `warn`/`rval = 1`/`continue`).
fn read_file(path: &str) -> Result<Vec<u8>, ()> {
    let fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => return Err(()),
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
    // [SD19-gate] the fd is no longer used after close
    #[allow(clippy::let_underscore_must_use)]
    let _ = close(fd);
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_only_has_no_files() {
        let (quiet, script, files) = parse_sed_args(&["s/a/b/"]).unwrap();
        assert!(!quiet);
        assert_eq!(script, "s/a/b/");
        assert!(files.is_empty());
    }

    #[test]
    fn dash_n_sets_quiet() {
        let (quiet, _, _) = parse_sed_args(&["-n", "s/a/b/"]).unwrap();
        assert!(quiet);
    }

    #[test]
    fn operands_after_the_script_are_files() {
        let (_, script, files) = parse_sed_args(&["s/a/b/", "f1", "f2"]).unwrap();
        assert_eq!(script, "s/a/b/");
        assert_eq!(files, ["f1", "f2"]);
    }

    #[test]
    fn flags_and_files_mix_in_any_order() {
        // `-n` is recognised wherever it appears; the first positional is the
        // script and the rest are files.
        let (quiet, script, files) = parse_sed_args(&["s/a/b/", "-n", "f1"]).unwrap();
        assert!(quiet);
        assert_eq!(script, "s/a/b/");
        assert_eq!(files, ["f1"]);
    }

    #[test]
    fn bare_dash_is_a_literal_file_operand() {
        // C opens files through fopen, so `-` is not a stdin alias here.
        let (_, _, files) = parse_sed_args(&["s/a/b/", "-"]).unwrap();
        assert_eq!(files, ["-"]);
    }

    #[test]
    fn unsupported_option_is_rejected() {
        assert_eq!(
            parse_sed_args(&["-i", "s/a/b/"]),
            Err(SedArgError::UnsupportedOption)
        );
    }

    #[test]
    fn missing_script_is_rejected() {
        assert_eq!(parse_sed_args(&[]), Err(SedArgError::MissingScript));
        assert_eq!(parse_sed_args(&["-n"]), Err(SedArgError::MissingScript));
    }
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
