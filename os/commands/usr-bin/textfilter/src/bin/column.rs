//! Minix-RS column — columnate lines into a table.
//!
//! Ground truth: `minix3/usr.bin/column/column.c` (NetBSD). The
//! deciding half (entry collection, the two columnate layouts, and the
//! `-t` table alignment) is the library's `column` module; this program
//! reads stdin or file operands, lays the entries out, and prints. With
//! file operands each is opened in turn and fed through one shared entry
//! pool (`column.c:114-123`); an unopenable operand warns and only sets
//! the exit status, and the run continues to the next file. A bare `-` is
//! a literal file name (`fopen`), not a stdin alias. The terminal width
//! comes from `-c` (the C queries the window size, which belongs to the
//! terminal face).
//!
//! The entry pool works on **bytes**: `input` in `column.c` stores the `fgetln`
//! slice into the pool unchanged (`:270-297`), layout widths are measured with
//! `strlen` (`:243`), so a non-UTF-8 byte both enters the pool and occupies one
//! cell; the last column in table mode goes out through `printf("%s\n", ...)`
//! (`:252`) unchanged — no padding, no trimming step.
//!
//! There are four diagnostic faces, each with its own owner of the sentence: the
//! two libc `getopt` lines (out-of-set letter, value-taking option missing its
//! value) come from the single source [`minix_textfilter::GetoptReport`] and are
//! always followed by one usage line; `warn(3)`'s `Cannot open \`...'` has the
//! three-part form "program: label: strerror sentence"; `warnx(3)`'s
//! `line too long` has only two parts (`vwarnx` appends no strerror), and it
//! prints one sentence for each dropped remnant line.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::column::{columnate, parse_column_args, scan_entries};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program-name leg goes through the `&str` mouth of `warn_line` and
    // `GetoptReport::line_bytes` (a known libs gap), so the arm folds once here;
    // operands and the `-s` value are not folded along.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C's option loop sends `'?'` and everything out-of-set to `default: usage()`
    // (`column.c:107-109`), and libc prints a line itself before returning `'?'`
    // (`getopt.c:101-104`, `:132-136`). That line goes through the byte form
    // `line_bytes`: libc's letter slot is one bare byte (`optopt = *place++` at
    // `getopt.c:79` advances one byte at a time, and `:103` writes it out with
    // `%c`); reference measurement `tmp/column-314/column` shows `column -é`
    // printing one byte `c3`, while `char` rendering would add one.
    let (options, operands) = match parse_column_args(&args[1..]) {
        Ok(a) => (a.options, a.operands),
        Err(report) => {
            support::warn(&report.line_bytes(&prog));
            usage(&prog);
        }
    };

    // `column.c:114-123`: no operands means read stdin; otherwise open each file
    // in turn into one shared entry pool, where an unopenable file only `warn(3)`s
    // and sets the exit code to 1 (`eval = 1`), and the run continues with the
    // next file.
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let mut rval = 0;
    if operands.is_empty() {
        collect(&mut entries, &mut rval, &support::read_stdin(), &prog);
    } else {
        for operand in &operands {
            // The open leg still requires `&str` (a known libs gap), so fold once;
            // the message echoes this folded name — the difference from the real
            // source is in the open leg, not in how the echo is written.
            let path = String::from_utf8_lossy(operand);
            match support::read_file(&path) {
                Ok(bytes) => collect(&mut entries, &mut rval, &bytes, &prog),
                Err(code) => {
                    support::warn_line(&prog, &format!("Cannot open `{path}'"), code);
                    rval = 1;
                }
            }
        }
    }
    // `column.c:125-126`: an empty entry pool returns directly by `eval` without
    // entering layout — layout's `maxlength` is the alignment value computed for
    // zero entries, a different shape from C's "never entered these functions".
    if entries.is_empty() {
        support::terminate(rval);
    }
    for line in columnate(&entries, &options) {
        support::emit(&line);
        support::emit(b"\n");
    }
    support::terminate(rval);
}

/// Collects one source's bytes into the entry pool: split on newlines, then
/// keep the non-blank entries (the C `input`/`add` behaviour). A tail
/// line without its newline is dropped by the C with one
/// `warnx("line too long")` per occurrence, and each such line also sets the
/// exit status (`column.c:277-283`).
///
/// A line is bytes, not text: C's `input` stores the whole `fgetln` slice into the
/// pool unchanged and never decodes. The old `from_utf8(bytes).unwrap_or("")` let
/// one bad byte wipe **the whole file's** entries (measurement in
/// `tmp/column-435/ab.log`: the reference prints `a\xffb\tz`, this wrapper returned
/// empty).
fn collect(entries: &mut Vec<Vec<u8>>, rval: &mut i32, bytes: &[u8], prog: &str) {
    let lines: Vec<&[u8]> = bytes.split(|&byte| byte == b'\n').collect();
    let pool = scan_entries(&lines);
    entries.extend(pool.entries);
    for _ in 0..pool.dropped_tail {
        support::warn(format!("{prog}: line too long\n").as_bytes());
        *rval = 1;
    }
}

/// C's `usage()` (`column.c:302-307`): one capitalised `Usage` line, the program
/// name from `getprogname()`, and `exit(1)`.
fn usage(prog: &str) -> ! {
    support::warn(format!("Usage: {prog} [-tx] [-c columns] [-s sep] [file ...]\n").as_bytes());
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
