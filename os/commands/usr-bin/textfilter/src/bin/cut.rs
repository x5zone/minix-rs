//! Minix-RS cut — select bytes or fields per line.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (NetBSD), `cut -b list |
//! -c list | -f list [-d delim] [-s] [file ...]`. The list grammar and the
//! byte/field selection are the library's (`parse_list`,
//! `select_bytes`, `select_fields`); `-b` and `-c` coincide here because
//! the selection is byte-grained (the C distinction matters only for
//! multibyte locales, which this build does not carry). With no operand lines
//! come from stdin; each `-` operand reads stdin and each file operand is
//! opened, and a file that cannot be opened warns and is skipped (the exit
//! status stays nonzero, C `cut.c:138-142`). A malformed list or a missing
//! selection exits 1; a delimiter must be one byte.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::cut::{CutError, parse_cut_args, parse_list, select_bytes, select_fields};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Every C diagnostic starts with `getprogname()` (err/errx go through
    // `vwarn`/`vwarnx`). The program-name leg goes to the `&str` name field
    // of `warn_line` and `line_bytes` (registered under
    // `ARGV-BYTE-PATH-IN-LIBS`), so it is folded once here; the list values,
    // delimiter, and operands are not folded along with it.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // argv folding lives in the deciding half (`parse_cut_args`): BSD
    // `getopt("b:c:d:f:sn")` with attached-or-next list/delimiter values and
    // flag clustering. The old inline whole-token matcher mishandled a flag
    // clustered with a value option (`cut -sf1-3` silently dropped the field
    // list) and `-ns` (dropped the `-s` suppress).
    let parsed = match parse_cut_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e, &prog),
    };
    let bytes_list = parsed.bytes_list;
    let fields_list = parsed.fields_list;
    let delimiter = parsed.delimiter;
    let suppress = parsed.suppress;
    let operands = parsed.operands;

    let selecting_bytes = bytes_list.is_some();
    // `parse_cut_args` already parsed the list inside its option loop (C's
    // `get_list` sits in that same case), so all that remains here is which
    // one to take. The four post-checks have also already run on that half.
    let list_text = match (bytes_list, fields_list) {
        (Some(list), None) | (None, Some(list)) => list,
        _ => usage(),
    };
    let ranges = match parse_list(list_text) {
        Ok(parsed) => parsed,
        // Unreachable: the parser above has already run over the same
        // string. If this is ever reached, report C's sentence rather than
        // exiting 1 silently — the old form exited 1 silently without saying
        // a word.
        Err(error) => arg_error(CutError::from(error), &prog),
    };

    let process = |input: &[u8]| {
        for line in support::lines_of(input) {
            // `cut` is the branch in C that **normalizes the newline back
            // in**: `cut.c` writes one `\n` after each processed line, and
            // the reference binary `tmp/cut-406/cut_ref` returns three
            // newline-terminated lines even for input with no trailing
            // newline (reading `alp\nbet\ngam\n`). So the line is taken as
            // normalized text rather than in its original `eol` form — not
            // an oversight: C in this branch does exactly this.
            let line = line.bytes();
            let mut out = vec![0u8; line.len() + 1];
            let selection = if selecting_bytes {
                // The byte leg has no "skip the whole line" notion: `-s`
                // only coexists with `-f` (`cut.c:124`), so that side always
                // returns a line to write.
                select_bytes(line, &ranges, &mut out).map(Some)
            } else {
                select_fields(line, &ranges, delimiter, suppress, &mut out)
            };
            match selection {
                Ok(Some(len)) => {
                    support::emit(&out[..len]);
                    support::emit(b"\n");
                }
                // `-s` and this line has no delimiter: C does not even print
                // a newline — the test at `cut.c:248` fails, and `:253-254`
                // skip the `putchar('\n')` at `:279`.
                Ok(None) => {}
                Err(_) => support::terminate(1),
            }
        }
    };

    // C `if (*argv) for each operand fcn(...) else fcn(stdin)`: `-` and the
    // empty operand list both read stdin; a file is opened per operand, and an
    // unopenable file warns and is skipped (exit status stays nonzero).
    let mut status = 0;
    if operands.is_empty() {
        process(&support::read_stdin());
    }
    for operand in &operands {
        if *operand == b"-" {
            process(&support::read_stdin());
            continue;
        }
        // The file name arrives as bytes; the open leg and the three-part
        // `warn("%s", *argv)` sentence both need `&str` (registered under
        // `ARGV-BYTE-PATH-IN-LIBS`), so it is folded once here — any
        // divergence from ground truth lies on that leg, not in the echo
        // form. The `-` test compares bytes (the check above) and is
        // independent of the folded name.
        let path = String::from_utf8_lossy(operand);
        // `support::read_file` rather than a bare `minix_sys::open`: the
        // latter has no host counterpart at this crate's command layer, so
        // running the host binary directly fails to open any operand (tail hit
        // the same pit on the same leg, and that gap was then entered as
        // "verbatim reconciliation does not apply to tail"). With the
        // counterpart wired back in, `cut_face.rs` can pin this leg's error
        // path.
        let input = match support::read_file(&path) {
            Ok(input) => input,
            // C's `warn("%s", *argv)` at `cut.c:138-142`: program name,
            // operand, and `strerror` sentence — all three components are
            // required (the old form used `warn(3)` as if it were
            // `warnx(3)`).
            Err(code) => {
                support::warn_line(&prog, &path, code);
                status = 1;
                continue;
            }
        };
        process(&input);
    }
    support::terminate(status)
}

/// Map a [`CutError`] to the exact C path (message and exit code), then terminate.
fn arg_error(e: CutError, prog: &str) -> ! {
    match e {
        CutError::Usage => usage(),
        // The libc line comes first; usage's three lines follow.
        //
        // The letter slot goes through `line_bytes`: `src/cut.rs` already
        // works byte-wise (`bytes[pos] as char`), and the letter slot of the
        // libc line is one byte to begin with (`getopt.c:79`
        // `optopt = *place++`, `:103` `%c`). Measured against the reference
        // binary `tmp/cut-406/cut_ref`, both `-é` and `-cé` print
        // `cut: unknown option -- ` + a single `c3` + newline, then the three
        // tab-aligned usage lines, exiting 1.
        CutError::Getopt(report) => {
            support::warn(&report.line_bytes(prog));
            usage()
        }
        // C's two errx calls (`cut.c:196`, `:198`): they go through
        // `vwarnx`, so no `errno` sentence is appended, and neither splices
        // the offending value back in.
        CutError::IllegalList => {
            support::warn(format!("{prog}: [-bcf] list: illegal list value\n").as_bytes());
            support::terminate(1);
        }
        CutError::ZeroInList => {
            support::warn(format!("{prog}: [-bcf] list: values may not include zero\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// C's `usage()` (`cut.c:290-296`): **three lines**, a tab after `usage:`,
/// each of the last two lines starting with a tab, and no program-name
/// placeholder (C hardcodes `cut`). All of it goes to standard error — the
/// old form used `support::emit`, sending diagnostics to standard output,
/// which is the wrong channel. Exits 1.
fn usage() -> ! {
    support::warn(
        b"usage:\tcut -b list [-n] [file ...]\n\tcut -c list [file ...]\n\tcut -f list [-d string] [-s] [file ...]\n",
    );
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
