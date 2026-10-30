//! Minix-RS paste — merge lines of files.
//!
//! Ground truth: `minix3/usr.bin/paste/paste.c` (NetBSD), `paste [-s]
//! [-d list] file ...`. The ring is `paste.c:70`'s `getopt(argc, argv,
//! "d:s")`; the merge itself is the library's `join_parallel` /
//! `join_serial`, and every operand names an already-open stream (each
//! file opens once, all `-` operands share the single standard-input
//! stream, `paste.c:106-112`).
//!
//! The front door has **three sentence families and two exit codes**, all
//! carried over from C sentence by sentence:
//! (1) An out-of-set letter and a missing `-d` value each take "one libc line
//!     + **one** usage line" and exit **1** (`paste.c:80-82`'s
//!     `case '?': default: usage();` exits straight from inside the ring, so
//!     this branch's ring can produce **at most one** libc message — the exact
//!     opposite of `seq`'s accumulating shape; `jot` matches this branch).
//!     The program name in the usage line is **hard-coded in the string**
//!     (`paste.c:231`), while the libc line follows `getprogname()`, so the
//!     two lines of one branch come from different sources.
//! (2) `-d ''` goes through `errx(1, "no delimiters specified.")`: one line,
//!     program name included, **the sentence carries its own full stop**, no
//!     `strerror`, exit 1 (`paste.c:221-223`).
//! (3) An unopenable file name leaves through two different exits depending on
//!     the mode (the most valuable shape in this branch): parallel mode uses
//!     the three-part **fatal** form `err(1, "%s", argv[cnt])`
//!     (`paste.c:107-108`, the first unopenable name exits and later names are
//!     never even tried); serial mode uses the three-part **non-fatal** form
//!     `warn("%s", p)` (`paste.c:166-169`, skip this name and continue with the
//!     next, and the run still exits 0). Both branches share the sentence
//!     shape; the exit code and whether to continue are opposite.
//!
//! Operands have **no count check**: `paste` with no operands neither reads
//! standard input nor reports an error; it exits 0 with empty output
//! (`paste.c:88-92` hands `argc` straight to `parallel`, and with zero streams
//! the line loop never takes a step).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::paste::{PasteError, join_parallel, join_serial, parse_paste_args};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let parsed = match parse_paste_args(&words) {
        Ok(parsed) => parsed,
        Err(error) => fail(&prog, &error),
    };
    let options = parsed.options;
    let operands = parsed.operands;
    if operands.is_empty() {
        support::terminate(0);
    }

    // One column = one **already-open stream**: each file name opens once (so
    // `paste a.txt a.txt` is two cursors), while all `-` share the single
    // standard-input stream (a reference build's `paste - -` over three lines
    // prints two columns, each taking one line).
    let mut sources: Vec<Vec<Vec<u8>>> = Vec::new();
    let mut columns: Vec<usize> = Vec::new();
    let mut stdin_source: Option<usize> = None;
    for operand in &operands {
        if *operand == b"-" {
            let index = match stdin_source {
                Some(index) => index,
                None => {
                    sources.push(split_lines(&support::read_stdin()));
                    stdin_source = Some(sources.len() - 1);
                    sources.len() - 1
                }
            };
            columns.push(index);
            continue;
        }
        // Opening the file and the warning sentence need the text form:
        // `read_file` lowers to `minix_sys`'s `open(2)`, and `warn_line`'s
        // label is `&str`. A non-UTF-8 file name is therefore folded at this
        // sentence and the message prints the folded bytes — line content
        // never passes through this step.
        let path = String::from_utf8_lossy(operand).into_owned();
        match support::read_file(&path) {
            Ok(data) => {
                sources.push(split_lines(&data));
                columns.push(sources.len() - 1);
            }
            Err(errno) => {
                support::warn_line(&prog, &path, errno);
                if options.serial {
                    // The `sequential` branch is `warn` + `continue`
                    // (`paste.c:166-169`)
                    continue;
                }
                support::terminate(1);
            }
        }
    }

    if options.serial {
        // When the same stream is listed in several places, the second place
        // has no lines left to take (the cursor belongs to the stream, not to
        // the column)
        let mut taken: Vec<usize> = vec![0; sources.len()];
        for column in &columns {
            let source = *column;
            let rest = &sources[source][taken[source]..];
            taken[source] = sources[source].len();
            let lines: Vec<&[u8]> = rest.iter().map(Vec::as_slice).collect();
            support::emit(&join_serial(&lines, &options));
        }
    } else {
        let lines: Vec<Vec<&[u8]>> = sources
            .iter()
            .map(|source| source.iter().map(Vec::as_slice).collect())
            .collect();
        let slices: Vec<&[&[u8]]> = lines.iter().map(Vec::as_slice).collect();
        support::emit(&join_parallel(&columns, &slices, &options));
    }
    support::terminate(0)
}

/// Each of the three sentence families takes its own exit and exit code; the libc line comes back from the single [`minix_textfilter::GetoptReport`] source.
///
/// The letter slot uses `line_bytes` rather than `line`: `src/paste.rs`
/// already reads it as bytes (`token[k]` is a `u8`, then `char::from`), and a
/// non-ASCII letter through `line` would be UTF-8 encoded into two bytes. A
/// reference build measured `paste -é` and `paste -sé` both printing
/// `paste: unknown option -- ` + a single `c3` + newline; the mechanism is
/// `getopt.c:79`'s `optopt = *place++` and `:103`'s `%c`.
fn fail(prog: &str, error: &PasteError) -> ! {
    match error {
        PasteError::Usage { reports } => {
            for report in reports {
                support::warn(&report.line_bytes(prog));
            }
            support::warn(b"paste: [-s] [-d delimiters] file ...\n");
            support::terminate(1);
        }
        PasteError::NoDelimiters => {
            support::warn(format!("{prog}: no delimiters specified.\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// Split bytes into newline-stripped **byte** lines, dropping the empty piece after
/// a trailing newline (interior blank lines stay as empty cells) — C uses
/// `fgetln` to obtain the whole line including its newline and then subtracts
/// it (`paste.c:134-136`); a last line without a newline is still a line.
///
/// Lines are bytes, not text: C's line path never decodes, so a non-UTF-8 byte
/// is just one byte in that line. The old `from_utf8(input).unwrap_or("")` let a
/// single bad byte erase the lines of the **whole file** — a reference build
/// over `paste raw.txt raw.txt` (`raw.txt` = `a\xffb\nc\n`) prints
/// `a\xffb\ta\xffb\nc\tc\n`, while the old shell returned empty output.
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

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
