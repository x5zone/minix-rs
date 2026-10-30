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
//! registered corner. File access goes through `bin_support`'s twin face
//! (standard input via `read_stdin`, operand files via `read_file`, `-o` via
//! `write_file`). The hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`).
//!
//! The front door has three classes of exits, each with its own sentence owner, and
//! **this command's usage exits 2, not 1** (`unifdef.c:373-379`'s `exit(2)`):
//! (1) an off-charset letter and a missing value for a value-taking letter both print
//! one libc line first (`getopt.c:101-104`, `:132-136`), always followed by usage;
//! (2) when `-i`'s following letter is not `D`/`U`, or a `-D`/`-U` symbol name does not
//! fit `addsym`'s shape, C prints **only the usage line** with no sentence of its own;
//! (3) after the loop, it first checks "no symbol at all and no `-s`" — one `warnx`
//! line plus usage, two lines, and this branch precedes the operand check; more than
//! one operand yields `unifdef: can only do one file`, and an operand file that cannot
//! be opened uses `err(2, "can't open %s", …)`'s three-part shape. The usage line's
//! program name is **hard-coded in the string** (`unifdef.c:375-377` does not call
//! `getprogname()`), whereas the two libc lines and the `err`/`warnx` prefixes follow
//! `getprogname()` — two lines from different origins in one command.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::unifdef::{Unifdef, UnifdefArgError, UnifdefError, parse_unifdef_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // Option loop (`"i:D:U:I:o:cdeklst"` at `unifdef.c:273`). The two libc lines are
    // always followed by usage, whereas for a bad `-i` letter or a symbol name that
    // does not fit `addsym`'s shape, C prints only the usage line.
    //
    // The letter slot goes through `line_bytes`: `src/unifdef.rs` already takes it as
    // bytes (`char::from(bytes[pos])`), and a non-ASCII letter routed through `line`
    // would be UTF-8 encoded into two bytes. The reference
    // `tmp/unifdef-318/unifdef_ref` measures both `-é` and `-cé` as
    // `unifdef: unknown option -- ` + a single `c3` + newline then usage, with the
    // mechanism at `getopt.c:79`'s `optopt = *place++` and `:103`'s `%c`.
    let (options, operands) = match parse_unifdef_args(&args[1..]) {
        Ok(parsed) => (parsed.options, parsed.operands),
        Err(UnifdefArgError::Getopt(report)) => {
            support::warn(&report.line_bytes(prog));
            usage();
        }
        Err(UnifdefArgError::Usage) => usage(),
        Err(UnifdefArgError::TooManySymbols) => {
            support::warn(format!("{prog}: too many symbols\n").as_bytes());
            support::terminate(2);
        }
    };

    // `unifdef.c:327-330`: when the symbol table is empty and `-s` was not given,
    // print one `warnx` line then usage; this branch precedes the operand check, so
    // `unifdef -- -z` reports it rather than "can only do one file".
    if options.symbols.is_empty() && !options.symlist {
        support::warn(format!("{prog}: must -D or -U at least one symbol\n").as_bytes());
        usage();
    }
    // `unifdef.c:331-332`: more than one operand yields a single `errx(2, "can only do one file")` line.
    if operands.len() > 1 {
        support::warn(format!("{prog}: can only do one file\n").as_bytes());
        support::terminate(2);
    }

    // `unifdef.c:333-341`: only one operand that is not a bare `-` opens a file; a
    // bare `-` reads stdin just like no operand. The failure sentence is
    // `err(2, "can't open %s", filename)`'s three-part shape (program name, operand
    // text, `strerror` sentence), and the exit code is 2, not 1.
    let input = match operands.first().copied() {
        None | Some("-") => support::read_stdin(),
        Some(path) => match support::read_file(path) {
            Ok(bytes) => bytes,
            Err(code) => {
                support::warn_line(prog, &format!("can't open {path}"), code);
                support::terminate(2);
            }
        },
    };
    let text = String::from_utf8_lossy(&input).into_owned();
    // `-o`'s value is taken by the loop before the operand check
    // (`unifdef.c:311-312`); copy it here first, because once `options` is handed to
    // the decisive half as a whole it is out of reach.
    let output_path = options.output.clone();

    match Unifdef::new(options).run(&text) {
        Ok(outcome) => {
            match output_path.as_deref() {
                Some(path) => {
                    // C's sentence is `err(2, "can't open %s", ofilename)`'s
                    // three-part shape; this layer's `write_file` has no errno channel
                    // (`Result<(), ()>`), so only two parts can be printed here; the
                    // missing part is registered as `UNIFDEF-O-ERRNO-LEG`.
                    if support::write_file(path, &outcome.output).is_err() {
                        support::warn(format!("{prog}: can't open {path}\n").as_bytes());
                        support::terminate(2);
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
