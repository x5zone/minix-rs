//! Minix-RS unexpand — spaces to tabs.
//!
//! Ground truth: `minix3/usr.bin/unexpand/unexpand.c` (NetBSD). The
//! deciding half (the tabify walk, `-t` stop lists, the `-a`
//! whole-line form) is the library's `unexpand` module. With no operand the
//! program tabifies standard input; with operands it tabifies each file in turn
//! (C `unexpand.c:127-135`'s `do … while (argc > 0)`: each file name is
//! `freopen`ed onto stdin, the walk runs `tabify` line by line, and the line
//! cursor resets per file); a file that cannot be opened is fatal
//! `err(EXIT_FAILURE, "Cannot open `%s'", …)`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::unexpand::{
    StopRejection, UnexpandArgs, UnexpandError, UnexpandOptions, parse_unexpand_args, unexpand,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program name is folded once (the two `&str` entry points of
    // `line_bytes` and `err_line`); the two tabstop message value slots and the
    // `Cannot open` file-name slot each have their own byte-assembled leg.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let UnexpandArgs { options, operands } = match parse_unexpand_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(error) => arg_error(&prog, error),
    };
    // After the option loop and before reading input, C dumps the tab-stop table
    // to stderr (`unexpand.c:124-125`:
    // `for (i = 0; i < nstops; i++) fprintf(stderr, "%lu %zu\n", i, tabstops[i])`)
    // — a leftover debug line in the ground truth, but it is part of the external
    // behaviour, and a reference binary was measured for every shape (`-t 4` ->
    // `0 4`; `-t 4,8` -> `0 4` plus `1 8`), so this shell reproduces it instead of
    // "fixing" C.
    for (index, stop) in options.stops.iter().enumerate() {
        support::warn(format!("{index} {stop}\n").as_bytes());
    }

    // The read seam goes through `bin_support`'s twin face: this file used to call
    // bare `minix_sys::open`/`read`, so **no input could be read on the host** (the
    // sixth case in the family: the read legs of `nl`/`fold`/`rev`/`colrm` and the
    // write leg of `split`), which is why its output had never been tested at the
    // process level.
    if operands.is_empty() {
        emit_tabified(&support::read_stdin(), &options);
        support::terminate(0);
    }
    for operand in &operands {
        // A failed C `freopen(argv[0], "r", stdin)` is
        // `err(EXIT_FAILURE, "Cannot open `%s'")` (`unexpand.c:129-130`): program
        // name, backquoted file name, and `strerror` sentence, all three parts (the
        // old code was missing the third), exit 1.  The file-name slot holds argv's
        // **raw bytes**, so this message is assembled here; the copy handed to
        // `read_file` goes through the `&str` entry point, and the fold happens
        // only at that moment, leaving the message untouched.
        let shown = String::from_utf8_lossy(operand);
        match support::read_file(&shown) {
            Ok(input) => emit_tabified(&input, &options),
            Err(code) => {
                let mut line = Vec::new();
                line.extend_from_slice(prog.as_bytes());
                line.extend_from_slice(b": Cannot open `");
                line.extend_from_slice(operand);
                line.extend_from_slice(b"': ");
                line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
                line.push(b'\n');
                support::warn(&line);
                support::terminate(1);
            }
        }
    }
    support::terminate(0)
}

/// Tabifies one collected stream and writes it out.
///
/// C emits everything through `putchar`, and the newline at end of line is a byte
/// from the input (`fgetln` counts it in the length), so **no newline is added
/// when the last line has none** (a reference binary: input `no trailing newline`
/// still comes out as one line without a newline).  The old code appended a
/// newline to every line, which lengthened the last line out of nowhere.
fn emit_tabified(input: &[u8], options: &UnexpandOptions) {
    let text = String::from_utf8_lossy(input);
    let lines = unexpand(text.as_ref(), options);
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        out.extend_from_slice(line.as_bytes());
        let last = index + 1 == lines.len();
        // A newline is omitted only for "the last line and the input does not end
        // with one"; otherwise the input's newline is written out as C does.
        if !last || input.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    support::emit(&out);
}

/// The message and exit code of each exit branch, each following C.
fn arg_error(prog: &str, error: UnexpandError<'_>) -> ! {
    match error {
        UnexpandError::Usage => usage(prog),
        // Unknown letter and missing value: libc's line comes first
        // (`getopt.c:101-104`, `:132-136`) and C's `default: usage()` after it
        // (`unexpand.c:116-118`) — two lines.
        UnexpandError::UnknownOption(letter) => {
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(prog));
            usage(prog)
        }
        UnexpandError::MissingValue(letter) => {
            support::warn(&GetoptReport::MissingValue(letter).line_bytes(prog));
            usage(prog)
        }
        UnexpandError::BadStops(StopRejection::Invalid(piece)) => {
            // C `errx(EXIT_FAILURE, "Invalid tabstop `%s'", tab)`: a two-part form
            // with no strerror suffix, where `%s` prints the **raw bytes** of the
            // piece split out by `strsep`.
            support::warn(
                &[
                    format!("{prog}: Invalid tabstop `").as_bytes(),
                    piece,
                    b"'\n".as_slice(),
                ]
                .concat(),
            );
            support::terminate(1);
        }
        UnexpandError::BadStops(StopRejection::NotIncreasing { spec, previous }) => {
            // The second message of the same family (`unexpand.c:108-112`): the
            // spec slot carries raw bytes and the trailing one is a **number**.
            support::warn(
                &[
                    format!("{prog}: Bad tabstop spec `").as_bytes(),
                    spec,
                    format!("', must be greater than the previous `{previous}'\n").as_bytes(),
                ]
                .concat(),
            );
            support::terminate(1);
        }
    }
}

/// C's `usage()` (`unexpand.c:69-73`): **capitalised** `Usage:`, the program name
/// from `getprogname()`, a `[file ...]` tail, exit 1.  The old code used lowercase
/// `usage:`, `[-t tablist]`, and was missing `[file ...]`.
fn usage(prog: &str) -> ! {
    support::warn(format!("Usage: {prog} [-a] [-t tabstop] [file ...]\n").as_bytes());
    support::terminate(1);
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
