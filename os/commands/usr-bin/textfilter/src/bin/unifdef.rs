//! Minix-RS unifdef — selectively remove C preprocessor conditionals.
//!
//! Ground truth: `minix3/usr.bin/unifdef/unifdef.c`, usage
//! `unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym]
//! [-iDsym[=val]] [-iUsym] ... [file]`. The deciding half (the ten-
//! state #if machine and expression evaluator) is the library's
//! `unifdef` module; this program parses the command line, reads the
//! input (stdin or one file), and reports the exit status: 0 when the
//! input was already clean, 1 when lines were dropped, 2 on trouble.
//! `-d` (debugging) and `-e` (obfuscated-directive tolerance) are
//! accepted and ignored — the DODGY table columns they gate are a
//! registered corner. File access goes through `bin_support`'s twins (standard input
//! through `read_stdin`, operand files through `read_file`, `-o` through `write_file`).
//! The hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`).
//!
//! The front door has three kinds of exits, each with its own owner, and this branch's
//! usage exits with 2 rather than 1 (`unifdef.c:373-379` has `exit(2)`):
//! ① an unknown letter and a value-taking letter missing its value are both preceded by
//! a libc line (`getopt.c:101-104`, `:132-136`), always followed by usage; ② when `-i`
//! is followed by a letter other than `D`/`U`, or a `-D`/`-U` symbol name does not fit
//! `addsym`'s shape, C prints only the usage line, with no sentence of its own; ③ after
//! the loop it first checks "no symbol at all and no `-s`" — one `warnx` line plus usage,
//! two lines, and this branch comes before the operand test; more than one operand is
//! one `unifdef: can only do one file` line, and an operand that cannot be opened is the
//! three-part `err(2, "can't open %s", ...)` form. The program name in the usage line is
//! hard-coded in the string (`unifdef.c:375-377` has no `getprogname()`), while the libc
//! lines and the `err`/`warnx` prefixes follow `getprogname()` — two lines in one command
//! come from different places.
//!
//! The command line reaches the library as bytes (`args_bytes`), so the operand
//! name, the `-o` name and each `-D`/`-U` symbol name and value are the exact
//! argv bytes — C echoes all three through `%s` (`unifdef.c:337`, `:365`, `:509`)
//! and matches symbol names with `strlcmp` (`:951`). Two legs stay on the text
//! side and are registered: the program-name token still folds once on its way
//! into `line_bytes`/`err_line` (`ARGV-BYTE-PATH-IN-LIBS`), and the file's own
//! line content folds once before the machine sees it, so a raw byte in a kept
//! line comes back as the replacement character instead of the byte
//! (`UNIFDEF-LINE-CONTENT-TEXTSEAM`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::unifdef::{Unifdef, UnifdefArgError, UnifdefError, parse_unifdef_args};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program name folds once here (the two `&str` mouths of `line_bytes` and
    // `err_line`, registered `ARGV-BYTE-PATH-IN-LIBS`); operand names and the `-o` name
    // have their own byte-built legs and do not pass through this point.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // The option loop (`unifdef.c:273`'s `"i:D:U:I:o:cdeklst"`). The two libc lines are
    // always followed by usage, whereas for a broken letter after `-i` and a symbol name
    // that does not fit `addsym`'s shape, C prints only the usage line.
    //
    // The letter slot goes through `line_bytes`: `src/unifdef.rs` already takes bytes
    // (`char::from(bytes[pos])`), and a non-ASCII letter would be UTF-8 encoded into two
    // bytes by `line`. C `unifdef` prints, for both `-é` and `-cé`,
    // `unifdef: unknown option -- ` + a single `c3` + newline followed by usage; the
    // mechanism is `optopt = *place++` at `getopt.c:79` and `%c` at `:103`.
    let (options, operands) = match parse_unifdef_args(&args[1..]) {
        Ok(parsed) => (parsed.options, parsed.operands),
        Err(UnifdefArgError::Getopt(report)) => {
            support::warn(&report.line_bytes(&prog));
            usage();
        }
        Err(UnifdefArgError::Usage) => usage(),
        Err(UnifdefArgError::TooManySymbols) => {
            support::warn(format!("{prog}: too many symbols\n").as_bytes());
            support::terminate(2);
        }
    };

    // `unifdef.c:327-330`: with an empty symbol table and no `-s`, one `warnx` line
    // comes first and then usage; this branch precedes the operand test, so
    // `unifdef -- -z` reports it rather than "can only do one file".
    if options.symbols.is_empty() && !options.symlist {
        support::warn(format!("{prog}: must -D or -U at least one symbol\n").as_bytes());
        usage();
    }
    // `unifdef.c:331-332`: more than one operand is a single
    // `errx(2, "can only do one file")` line.
    if operands.len() > 1 {
        support::warn(format!("{prog}: can only do one file\n").as_bytes());
        support::terminate(2);
    }

    // `unifdef.c:333-341`: only one operand that is not a bare `-` opens a file; a bare
    // `-` and no operand both read standard input. The unopenable sentence is the
    // three-part `err(2, "can't open %s", filename)` form (program name, operand text,
    // `strerror` sentence), and the exit code is 2 rather than 1.
    let input = match operands.first().copied() {
        // `unifdef.c:333`'s `strcmp(*argv, "-") != 0`: a bare `-` is compared as that one
        // byte.
        None | Some(b"-") => support::read_stdin(),
        Some(path) => {
            // The `&str` mouth of the open call (registered `ARGV-BYTE-PATH-IN-LIBS`):
            // only the name handed to `fopen` is folded, while the name slot of
            // `err(2, "can't open %s", filename)` keeps the original bytes.
            let shown = String::from_utf8_lossy(path);
            match support::read_file(&shown) {
                Ok(bytes) => bytes,
                Err(code) => err_can_open(&prog, path, code),
            }
        }
    };
    let text = String::from_utf8_lossy(&input).into_owned();
    // The `-o` value is taken by the loop before the operand test
    // (`unifdef.c:311-312`); copy it here because `options` is no longer reachable once
    // it is handed to the deciding half as a whole.
    let output_path = options.output.clone();

    match Unifdef::new(options).run(&text) {
        Ok(outcome) => {
            match output_path.as_deref() {
                Some(path) => {
                    // C's sentence is the three-part
                    // `err(2, "can't open %s", ofilename)` form; this layer's `write_file`
                    // has no errno channel (`Result<(), ()>`), so only two parts can be
                    // printed; the missing part is registered as `UNIFDEF-O-ERRNO-LEG`.
                    let shown = String::from_utf8_lossy(path);
                    if support::write_file(&shown, &outcome.output).is_err() {
                        err_can_open(&prog, path, minix_sys::Errno::from_i32(0));
                    }
                }
                None => support::emit(&outcome.output),
            }
            support::terminate(outcome.exitstat);
        }
        Err(err) => {
            support::warn(error_text(&err).as_bytes());
            support::terminate(2);
        }
    }
}

fn error_text(err: &UnifdefError) -> String {
    match err {
        UnifdefError::EofInComment => "unifdef: EOF in comment\n".to_string(),
        UnifdefError::InappropriateElif {
            line,
            if_line,
            depth,
        } => {
            if *depth == 0 {
                format!("unifdef: {}: Inappropriate #elif\n", line)
            } else {
                format!(
                    "unifdef: {}: Inappropriate #elif (#if line {} depth {})\n",
                    line, if_line, depth
                )
            }
        }
        UnifdefError::InappropriateElse { line } => {
            format!("unifdef: {}: Inappropriate #else\n", line)
        }
        UnifdefError::InappropriateEndif { line } => {
            format!("unifdef: {}: Inappropriate #endif\n", line)
        }
        UnifdefError::PrematureEof => "unifdef: Premature EOF\n".to_string(),
        UnifdefError::ObfuscatedControlLine { line } => {
            format!("unifdef: {}: Obfuscated preprocessor control line\n", line)
        }
        UnifdefError::TooDeep { line } => {
            format!("unifdef: {}: Too many levels of nesting\n", line)
        }
    }
}

/// Renders C's `err(2, "can't open %s", ...)` sentence by bytes:
/// `prog: can't open <name>: <sentence>`, then exit 2. The name is the file name argv
/// supplied (both the input operand and the `-o` value use it); going through
/// `support::warn_line` -> `minix_sys::err_line(&str)` would fold a bare byte into
/// U+FFFD and no longer match C byte for byte (the `split` and `tail` work measured this
/// shape). The branch with `code` 0 is this layer's honest reading: `write_file` has no
/// errno channel, while C prints the `strerror(0)` sentence there; the difference is
/// registered as `UNIFDEF-O-ERRNO-LEG`.
fn err_can_open(prog: &str, name: &[u8], code: minix_sys::Errno) -> ! {
    let mut line = Vec::new();
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": can't open ");
    line.extend_from_slice(name);
    line.extend_from_slice(b": ");
    line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
    line.push(b'\n');
    support::warn(&line);
    support::terminate(2)
}

fn usage() -> ! {
    support::warn(
        b"usage: unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym] [-iDsym[=val]] [-iUsym] ... [file]\n",
    );
    support::terminate(2);
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
