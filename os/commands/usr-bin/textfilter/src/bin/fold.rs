//! Minix-RS fold — wrap lines at a width.
//!
//! Ground truth: `minix3/usr.bin/fold/fold.c` (NetBSD). The deciding
//! half (the column rules for `\b`/`\r`/`\t`, the width break, the
//! last-space preference without `-s`) is the library's `fold` module.
//! With no operand the program folds standard input; with operands it folds
//! each file in turn (the column/word state restarting per file), and a file
//! it cannot open is fatal (C `err(1)`). The obsolete `-N` width form parses
//! too (`-w` is the modern spelling).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::fold::{fold_input, parse_fold_args, FoldArgs, FoldError, FoldOptions};
use minix_textfilter::GetoptReport;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program-name leg still renders through `&str`
    // (`GetoptReport::line_bytes` and `warn_line` both take `&str`), and the
    // library-side byte entry points are registered but untouched:
    // `ARGV-BYTE-PATH-IN-LIBS`. The option-letter and operand legs are raw
    // bytes from here on.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let FoldArgs { options, operands } = match parse_fold_args(&words[1..]) {
        Ok(parsed) => parsed,
        // The two errors getopt itself raises (an out-of-set letter, a
        // missing `-w` value): the libc line first, then `usage()`. The old
        // form printed only usage and lost the first line; the usage text and
        // exit code also disagreed with C at the time.
        //
        // The option letter goes through `line_bytes`, both halves being
        // byte-based: the argument seam hands back raw bytes, and
        // `src/fold.rs` takes a single byte (`bytes[pos] as char`), so `-é`
        // (`c3 a9`) prints just the one byte `c3` — the same shape as the
        // reference binary `tmp/fold-405/fold_ref` (the letter slot of the
        // libc line is one byte to begin with: `getopt.c:79`
        // `optopt = *place++`, `:103` `%c`). The missing-value branch takes
        // its letter from the option string `"0123456789bsw:"`, which is
        // already the ASCII `w`, so both entries share the same string.
        Err(FoldError::UnknownOption(flag)) => {
            support::warn(&GetoptReport::UnknownOption(flag).line_bytes(&prog));
            usage(&prog);
        }
        Err(FoldError::MissingValue(flag)) => {
            support::warn(&GetoptReport::MissingValue(flag).line_bytes(&prog));
            usage(&prog);
        }
        Err(FoldError::IllegalWidth) => {
            // C here is `errx(1, "illegal width value")`: **this line only**,
            // no usage appended.
            support::warn(format!("{prog}: illegal width value\n").as_bytes());
            support::terminate(1);
        }
    };

    // C `if (!*argv) fold(stdin); else for each argv freopen() … err(1) on
    // failure`: no operand folds stdin once; otherwise each file is folded in
    // turn (a fresh `fold_input` call restarts the column/word state), and an
    // unopenable file is fatal.
    if operands.is_empty() {
        fold_bytes(&support::read_stdin(), &options);
        support::terminate(0);
    }
    for operand in operands {
        // Both the open leg and the error-message leg take `&str`
        // (`os/libs/minix-sys/src/lib.rs:fn open`,
        // `os/libs/minix-types/src/types/errno.rs:fn err_line`), so a
        // byte-form operand can only be folded once here — the divergence is
        // registered in `ARGV-BYTE-PATH-IN-LIBS`.
        let path = String::from_utf8_lossy(operand).into_owned();
        // The read seam goes through the two-faced `bin_support` layer (host
        // `std::fs`, real machine open+read): this file previously read
        // through a bare `open`/`read` fd itself and **could read nothing on
        // the host** (the same defect addressed for `nl`), so the newly added
        // end-to-end gate exposed it at once: the output was empty.
        let input = match support::read_file(&path) {
            Ok(data) => data,
            Err(code) => {
                // C is `err(1, "%s", *argv)` (`fold.c:107-109`): program
                // name, operand, and the `strerror` sentence — all three
                // parts present. The old form printed only the first two and
                // lost the sentence.
                support::warn_line(&prog, &path, code);
                support::terminate(1);
            }
        };
        fold_bytes(&input, &options);
    }
    support::terminate(0)
}

/// Emit each folded line of one already-read input; the per-file restart of the
/// column/word state falls out of calling `fold_input` once per file.
fn fold_bytes(input: &[u8], options: &FoldOptions) {
    let text = String::from_utf8_lossy(input);
    for line in fold_input(&text, options) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
}

/// C's `usage()` (`fold.c:115-121`): one line, the program name following
/// `getprogname()`, `exit(1)`. The old form invented the line items
/// `[-b] [-s] [-w width | -width]` and dropped `[file ...]`.
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [-bs] [-w width] [file ...]\n").as_bytes());
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
