//! Minix-RS fortune — the doing half over `minix_text_games::fortune`.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c` — the quip database is
//! split by lines, and the draw index is taken modulo the database length (the
//! decisive-half semantics of `pick_quip`). This shell reads the data file
//! (default `/usr/share/games/fortune`, an operand may name another), takes the
//! draw value from clock seconds (mixed with the process id on real hardware),
//! and prints the hit line.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::fortune::{pick_quip, SliceQuips};

const DEFAULT_DB: &str = "/usr/share/games/fortune";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let path = args.get(1).copied().unwrap_or(DEFAULT_DB);

    // An open failure is reported separately (C's `err(1, "Cannot open `%s'", ...)`);
    // a read failure is not reported: C loads with `fgets(...) != NULL`
    // (`fortune.c:268`, `:293`) and never checks `ferror`, so a broken read and
    // end-of-file are the same thing. The seam is `support::slurp_operand`'s twin
    // face: the real-hardware leg uses `minix_sys::{open, read, close}`, the host
    // leg uses `std::fs`. The sentences reuse the two this shell already has (not
    // yet aligned with C's `err(1, "Cannot open `%s'", ...)` shape; that debt is
    // recorded in `rewrite-notes/18-stage-commands/todo.md`); this round only makes
    // the two arms reachable, without changing the wording
    let image = match support::slurp_operand(path) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(code)) => {
            // C's path here is `err(1, "Cannot open `%s'", fp->path)`
            // (`fortune.c:1056`, plus `:1067` and `:1116` for the `.dat` legs with
            // the same sentence shape): a three-part form whose middle part is
            // "sentence + file name wrapped in backticks and a single quote" and
            // whose tail is the errno sentence, exit 1. This shell's file-name
            // access failure is exactly this path (in C the directory leg uses
            // `warn` (`:525`, `:705`) and does not exit; this shell does not walk
            // directories yet, so it is not in that shape)
            let prog = support::prog_name(&argv);
            support::warn_line(prog, &format!("Cannot open `{path}'"), code);
            support::terminate(1);
        }
    };

    let text = String::from_utf8_lossy(&image).into_owned();
    let quips: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let draw = (support::epoch_micros().unwrap_or(0) / 1_000_000) as u32;
    let db = SliceQuips::new(&quips);
    match pick_quip(&db, draw) {
        Ok(quip) => {
            // C's output leg does not check failures: after `fputs(line, stdout)`
            // (`minix3/games/fortune/fortune/fortune.c:272`) comes
            // `(void) fflush(stdout)` (`:274`), both return values dropped by
            // `(void)`, and the whole file has no `ferror(stdout)` check (the
            // `strfile/strfile.c:255` line in the same directory checks its own
            // written data file and belongs to another tool). => A failed write adds
            // no sentence and does not change the exit code: errno is taken but not
            // reported, and the exit code is still fixed by `terminate(0)` below,
            // corresponding to C's `exit(0)` (`fortune.c:377`)
            let _ = support::emit_error(format!("{quip}\n").as_bytes());
        }
        Err(_) => {
            support::warn(format!("fortune: {path}: empty database\n").as_bytes());
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
