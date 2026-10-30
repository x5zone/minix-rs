//! Minix-RS comm — three-column merge of two sorted streams.
//!
//! Ground truth: `minix3/usr.bin/comm/comm.c` (NetBSD), main at lines
//! 66-164: `-1`/`-2`/`-3` suppress a column, `-f` folds the comparison,
//! column two carries one tab and column three two, both streams drain
//! at end of input, and the status is 0 (usage and an unopenable file
//! leave 1). Exactly two operands are required; each `-` names standard
//! input. The merge itself is the library's tested `comm` module.
//!
//! The diagnostic surface has only two exits, but they emit **different
//! numbers of lines**: for an unknown option letter libc prints one line
//! first (`getopt.c:101-104`) and then `comm.c:91-93`'s `default: usage()`
//! adds a second; when the operand count is not two (too many or too few
//! alike) only the single `usage()` line appears (`comm.c:101`). The optstring
//! `"123f"` has no letter that takes a value, so libc's missing-argument
//! message is unreachable on this branch — the reachable exits are decided by
//! the optstring, not padded out by analogy with sibling tools.
//! An unopenable file name is the three-part `err(1, "%s", name)` form and is
//! **fatal** (`comm.c:181-184`): unlike `column`/`rev`, which warn and
//! continue, `comm` exits 1 as soon as it cannot open the first file and never
//! even tries the second name.
//!
//! Command-line arguments arrive as **bytes** (`support::args_bytes`), so a
//! bare `ff` is still `ff` when it reaches the option loop, and the
//! option-letter report prints that very byte. Only two legs still require
//! `&str`: opening a file and the `err(1, "%s", name)` sentence (both in
//! `os/libs/` territory, registered as `ARGV-BYTE-PATH-IN-LIBS`), so the shell
//! keeps two in-arm conversions (one for the program name, one for the
//! operands) — the same established rule as every other branch.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::comm::{CommArgError, CommOptions, comm, parse_comm_args};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program-name leg goes to the `&str` name field of `warn_line` and
    // `line_bytes` (registered as `ARGV-BYTE-PATH-IN-LIBS`), so it is
    // converted once inside the arm; the two operands are not converted
    // along with it.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C's option loop has only one error branch: libc prints one line, then
    // `usage()` prints another (exit 1). The old form omitted libc's line and
    // flattened both failure kinds into a single hand-written usage line.
    let (options, files): (CommOptions, [&[u8]; 2]) = match parse_comm_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(CommArgError::UnknownOption(letter)) => {
            // The option-letter position uses `line_bytes`: retrieval in
            // `src/comm.rs` is byte-based, and the two sides must stay in the
            // same form for this to hold.
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(&prog));
            usage();
        }
        Err(CommArgError::NotTwoOperands) => usage(),
    };

    let lines1 = operand_lines(&prog, files[0]);
    let lines2 = operand_lines(&prog, files[1]);
    let a: Vec<&[u8]> = lines1.iter().map(Vec::as_slice).collect();
    let b: Vec<&[u8]> = lines2.iter().map(Vec::as_slice).collect();
    let merged = comm(&a, &b, &options);
    // Column prefixes are handed out as in C, **only to columns that are
    // actually printed** (`comm.c:105-110`: `p = tabs;` then
    // `if (flag1) col1 = *p++; if (flag2) col2 = *p++; if (flag3) col3 = *p;`).
    // So after column one is suppressed, the former column two gets `tabs[0]`
    // (no prefix) and the former column three gets `tabs[1]`, rather than
    // taking `tabs[column - 1]` by fixed column number — the old form did the
    // latter, so `comm -1` put two tabs in front of common lines.
    let tabs = ["", "\t", "\t\t"];
    let mut prefix = ["\t\t"; 4];
    let mut next = 0usize;
    for column in 1..=3u8 {
        let suppressed = match column {
            1 => options.suppress1,
            2 => options.suppress2,
            _ => options.suppress3,
        };
        if !suppressed {
            prefix[column as usize] = tabs[next];
            next += 1;
        }
    }
    for entry in &merged {
        let mut out = prefix[entry.column as usize].as_bytes().to_vec();
        out.extend_from_slice(entry.text);
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0)
}

/// Read one operand (`-` is standard input) and split it into newline-stripped
/// lines. An unopenable named file is fatal (C `file()` `err(1)`).
///
/// A last line without a newline is still a record (`getnextln` returns
/// `NULL` only on "EOF with no character read at all", `comm.c:211-214`) —
/// the opposite of `column`, which discards the residue as an overlong line.
///
/// File names arrive as bytes; both the file-opening leg and the
/// `err(1, "%s", name)` sentence need `&str` (registered as
/// `ARGV-BYTE-PATH-IN-LIBS`), so the conversion happens here once — the
/// reported name differs from the true source on that leg only, not in the
/// echoing form. The `-` test is a **byte** comparison and is unrelated to the
/// converted name.
fn operand_lines(prog: &str, name: &[u8]) -> Vec<Vec<u8>> {
    let path = String::from_utf8_lossy(name);
    let input = if name == b"-" {
        support::read_stdin()
    } else {
        match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // C does `err(1, "%s", name)`: program name, file name, and
                // `strerror` sentence on one line, then exit 1.
                support::warn_line(prog, &path, code);
                support::terminate(1);
            }
        }
    };
    split_lines(&input)
}

/// Split lines by byte: C's `getnextln` fills its buffer one `getc` at a time
/// (`comm.c:200-205`), so a non-UTF-8 byte is just a byte in that line. The
/// old form `from_utf8(input).unwrap_or("")` let one bad byte erase every
/// record in the file (measured against the reference: it printed three
/// columns where the old shell returned nothing).
fn split_lines(input: &[u8]) -> Vec<Vec<u8>> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut pieces: Vec<Vec<u8>> = input
        .split(|&byte| byte == b'\n')
        .map(<[u8]>::to_vec)
        .collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

/// C's `usage()` (`comm.c:195-199`): one line with the **program name
/// hardcoded in the string** (unlike `column`, which follows
/// `getprogname()`), then exit 1.
fn usage() -> ! {
    support::warn(b"usage: comm [-123f] file1 file2\n");
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
