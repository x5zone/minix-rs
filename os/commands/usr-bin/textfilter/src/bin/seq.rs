//! Minix-RS seq — number sequences.
//!
//! Ground truth: `minix3/usr.bin/seq/seq.c` (NetBSD). The deciding half
//! (operand shapes, the direction-following default increment, the
//! wrong-direction errors) is the library's `seq` module; this program
//! generates the numbers and prints them with the separator after each
//! one and the terminator after the last (seq.c:174-186). The floating
//! generation and `-f` formats of the C are declared unsupported by the
//! engine (the same float-rendering adjudication as printf's).
//!
//! The front door has **three sentence families and two exit statuses**,
//! each carried over from C sentence by sentence:
//! ① The `errflg` family exits 1 in the shape "zero or more libc lines plus
//!    **one** usage line" -- `-h`, and an operand count outside 1..3, produce
//!    only the usage line (`h` is in the optstring, so libc has nothing to
//!    say; C's `case 'h'` has no `break` and falls straight into
//!    `default: errflg++`, `seq.c:120-122`), while an out-of-set letter and a
//!    value-taking letter missing its value each print one libc line first
//!    (the loop **does not stop at the first bad letter**, so `seq -+- 3`
//!    yields two libc lines). The usage line follows `getprogname()`
//!    (`seq.c:133-136`).
//! ② The command's own `errx(1, ...)` family exits 1, each **one line** with
//!    no usage attached: `zero increment`/`zero decrement` (the name follows
//!    `first < last`), `needs positive increment`, `needs negative decrement`,
//!    `invalid format string: \`%q'` (with the original text; the backtick
//!    plus single quote is C's verbatim wording).
//! ③ The operand family goes through `e_atof()` and exits **2**: trailing junk
//!    gives `invalid floating point argument: 3x` (`errx`), while overflow or
//!    underflow gives `1e310: Numerical result out of range` (`err`'s
//!    three-part shape, with `strerror`). C parses operands in the order
//!    `last`→`first`→`incr`, so `seq 3 -w` blames `-w`.
//!
//! The command line arrives here as **bytes** ([`bin_support::args_bytes`]):
//! the values of `-s`/`-t` are exactly what gets printed, and the value of
//! `-f` plus the operands are exactly what lands in the message, so any of
//! these three passing through a text seam would alter the output and echo
//! bytes (measured against the reference binary: `seq -s<ff> 1 3` prints
//! `1\xff2\xff3\xff`, while the folded shape prints the three bytes
//! `ef bf bd`; the same `-\xff` is three letters each reporting a line in the
//! folded shape but one letter per line in the byte shape). The only place
//! still folded once is the operand slot of the `OutOfRange` line -- that
//! sentence borrows the library-side `strerror`, whose mouth takes `&str`;
//! the debt is registered as `ARGV-BYTE-PATH-IN-LIBS`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{Errno, ERANGE};
use minix_textfilter::seq::{generate, parse, render_output, SeqError};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let options = match parse(&words) {
        Ok(options) => options,
        Err(error) => fail(&prog, &error),
    };
    let values = match generate(&options) {
        Ok(values) => values,
        Err(error) => fail(&prog, &error),
    };
    support::emit(&render_output(&values, &options));
    support::terminate(0);
}

/// Each of the three families takes its own exit and status; the libc lines
/// come back from the single source in [`minix_textfilter::GetoptReport`].
fn fail(prog: &str, error: &SeqError) -> ! {
    match error {
        SeqError::Usage { reports } => {
            for report in reports {
                // The letter slot goes through `line_bytes`: `getopt.c:79`'s
                // `optopt = *place++` takes one byte at a time, and `seq`'s
                // loop is **accumulating** (`errflg` accumulates instead of
                // stopping at the first bad letter, `seq.c:101-113`), so as
                // many bytes as there are on the command line, that many lines
                // come back, one bare byte each.
                support::warn(&report.line_bytes(prog));
            }
            support::warn(
                format!(
                    "usage: {prog} [-w] [-f format] [-s string] [-t string] [first [incr]] last\n"
                )
                .as_bytes(),
            );
            support::terminate(1);
        }
        SeqError::InvalidNumber(word) => {
            support::warn(&sentence(
                prog,
                b"invalid floating point argument: ",
                word,
                b"",
            ));
            support::terminate(2);
        }
        SeqError::OutOfRange(word) => {
            // `err(2, "%s", num)`'s three-part shape carries `strerror`; that
            // line can only borrow the library-side `&str` mouth
            let word = String::from_utf8_lossy(word);
            support::warn_line(prog, &word, Errno::from_i32(ERANGE));
            support::terminate(2);
        }
        SeqError::ZeroIncrement => give_up(prog, "zero increment"),
        SeqError::ZeroDecrement => give_up(prog, "zero decrement"),
        SeqError::NeedsPositiveIncrement => give_up(prog, "needs positive increment"),
        SeqError::NeedsNegativeDecrement => give_up(prog, "needs negative decrement"),
        SeqError::BadFormat(fmt) => {
            support::warn(&sentence(prog, b"invalid format string: `", fmt, b"'"));
            support::terminate(1);
        }
        SeqError::BadFormatPlain => give_up(prog, "invalid format string"),
        // The same adjudication as the printf family: once the specifier
        // itself is accepted, rendering floats via `%g`/`%.2f` is not wired up
        // yet. C prints numbers here as usual; this shell returns one line of
        // self-confessed explanation and exits 1, registered as
        // SEQ-FLOAT-RENDER.
        SeqError::FloatNotModelled => {
            support::warn(format!("{prog}: -f formats wait for the float renderer\n").as_bytes());
            support::terminate(1);
        }
    }
}

/// `errx(1, ...)`: one line, with the program name, no usage, exit 1.
fn give_up(prog: &str, message: &str) -> ! {
    support::warn(format!("{prog}: {message}\n").as_bytes());
    support::terminate(1);
}

/// The family "program name + sentence + **raw bytes** + trailing characters +
/// newline": the sentence and the trailing characters are ASCII, and the only
/// non-ASCII part is the value handed back by `%s`, so it must be assembled
/// byte-wise rather than through `format!`'s `{}`.
fn sentence(prog: &str, lead: &[u8], word: &[u8], trail: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(prog.as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(lead);
    out.extend_from_slice(word);
    out.extend_from_slice(trail);
    out.push(b'\n');
    out
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
