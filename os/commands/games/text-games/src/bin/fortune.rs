//! Minix-RS fortune — the doing half over `minix_text_games::fortune`.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c` — the quip database is
//! split by line and the draw index is taken modulo the database length (the
//! deciding half of `pick_quip`). This shell reads the data file (default
//! `/usr/share/games/fortune`, or the operand when given), draws from the clock's
//! seconds (the real machine also mixes in the process id), and prints the line
//! it hit.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::fortune::{SliceQuips, pick_quip};

const DEFAULT_DB: &str = "/usr/share/games/fortune";

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let path: &[u8] = args.get(1).copied().unwrap_or(DEFAULT_DB.as_bytes());

    // An open failure is reported on its own (C's `err(1, "Cannot open `%s'", ...)`);
    // a read failure is not: C loads with `fgets(...) != NULL` (`fortune.c:268`,
    // `:293`) and never checks `ferror`, so a broken read and end of file are the
    // same thing. The seam is the twin face of `support::slurp_operand`: the
    // real-machine leg lands on `minix_sys::{open, read, close}` and the host leg
    // on `std::fs`. The sentences reuse the two this shell already has (not yet
    // squared with C's `err(1, "Cannot open `%s'", ...)` shape; that debt is
    // recorded in `rewrite-notes/18-stage-commands/todo.md`). Only the two arms are
    // made reachable here, not reworded. The open entry wants `&str` (registered
    // under `ARGV-BYTE-PATH-IN-LIBS`), so the fold happens only at this point; the
    // message still reports the raw bytes.
    let shown = String::from_utf8_lossy(path).into_owned();
    let image = match support::slurp_operand(&shown) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(code)) => {
            // This C path is `err(1, "Cannot open `%s'", fp->path)`
            // (`fortune.c:1056`, with two more `.dat` legs of the same sentence shape
            // at `:1067` and `:1116`): a three-part form whose middle is the sentence
            // plus the file name wrapped in a backtick and a single quote (raw
            // bytes), whose tail is the sentence for the errno, and whose exit
            // status is 1. A file-name access failure in this shell is exactly this
            // path (the directory leg goes through `warn` in C at `:525` and `:705`
            // and does not leave; this shell does not walk directories yet and is
            // not in that form). The `err_line` label entry takes `&str`, so the
            // line is put together here.
            let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
            let mut line: Vec<u8> = prog.as_bytes().to_vec();
            line.extend_from_slice(b": Cannot open `");
            line.extend_from_slice(path);
            line.extend_from_slice(b"': ");
            line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
            line.push(b'\n');
            support::warn(&line);
            support::terminate(1);
        }
    };

    let text = String::from_utf8_lossy(&image).into_owned();
    let quips: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let draw = (support::epoch_micros().unwrap_or(0) / 1_000_000) as u32;
    let db = SliceQuips::new(&quips);
    match pick_quip(&db, draw) {
        Ok(quip) => {
            // C's output leg does not check for failure: `fputs(line, stdout)`
            // (`minix3/games/fortune/fortune/fortune.c:272`) is followed by
            // `(void) fflush(stdout)` (`:274`), the return value is dropped by
            // `(void)`, and the whole file has no `ferror(stdout)` test (the one in
            // `strfile/strfile.c:255` checks the data file that tool writes, a
            // different program). So a failed write adds no line and changes no exit
            // status: the error is taken and dropped, and the status is still set by
            // `terminate(0)` below, matching C's `exit(0)` (`fortune.c:377`).
            let _ = support::emit_error(format!("{quip}\n").as_bytes());
        }
        Err(_) => {
            let mut line: Vec<u8> = b"fortune: ".to_vec();
            line.extend_from_slice(path);
            line.extend_from_slice(b": empty database\n");
            support::warn(&line);
            support::terminate(1);
        }
    }
    support::terminate(0)
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
