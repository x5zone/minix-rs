//! Minix-RS sed — scripts of substitutions over stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/sed/` (NetBSD `compile.c` for reading the
//! script, `process.c` for running it). The deciding half is the library's:
//! `parse_script` walks the compile loop (commands separated by `;` or a line
//! end, blanks and comments included), and `apply` mirrors the substitution
//! execution including the empty-match advance that keeps `s/x*/-/g` from
//! looping forever. This build carries `s` (with its `p` flag), `d`, `p` and `=`
//! over one line at a time, in list order, each carrying the pattern space to the
//! next — the print rules are C's: without `-n` every surviving line prints once
//! at the end of the cycle (`process.c:269-270`), `s///p` prints again on top of
//! that, `d` ends the cycle and takes that automatic print away, and `=` prints
//! the line number unconditionally. Addresses and the remaining verbs are not
//! modelled and are rejected with a message on standard error rather than
//! silently dropped. With file operands each is read in turn and the results
//! stream to standard output (`main.c` `mf_fgets` on the non-in-place path); an
//! unopenable operand warns, sets the exit status, and the run continues to the
//! next. The line number runs across operands because C clears it only when
//! editing in place (`main.c:441-443`). With no operands sed reads stdin, and a
//! bare `-` is an operand name, not a stdin alias. In-place editing (`-i`/`-I`)
//! needs the create/rename surface and is not modelled; net output carries no
//! filename prefix, unlike `grep`. One script carries at most
//! [`MAX_SCRIPT_COMMANDS`] commands, because this crate's deciding half keeps to
//! fixed arrays where C links the compiled commands as it goes.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_regex::pattern::{Pattern, compile_basic};
use minix_regex::sed::{
    Command, MAX_SCRIPT_COMMANDS, ParseFailure, PatternChoice, ScriptError, Scripted, SubstError,
    apply, auto_print, first_undefined_backref, max_backref, parse_script, pattern_choice,
    script_quiet_directive, step,
};

/// Argument-splitting errors for the sed front end.
#[derive(PartialEq, Debug)]
enum SedArgError {
    /// An option other than `-n` was requested (this build only models `-n`).
    UnsupportedOption,
    /// No positional script was supplied.
    MissingScript,
}

/// Splits sed argv into `(quiet, script, file operands)`, mirroring the C:
/// `-n` sets quiet wherever it appears, the first positional is the script
/// and the rest are input files. Any other leading-dash option is rejected
/// (only `-n` is modelled here). NetBSD sed opens input files through
/// `fopen` (`main.c` `mf_fgets`), so a bare `-` is a literal file name, not
/// a stdin alias.
fn parse_sed_args<'a>(argv: &'a [&'a str]) -> Result<(bool, &'a str, Vec<&'a str>), SedArgError> {
    let mut quiet = false;
    let mut script: Option<&'a str> = None;
    let mut files: Vec<&'a str> = Vec::new();
    for &arg in argv {
        if arg == "-n" {
            quiet = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            return Err(SedArgError::UnsupportedOption);
        } else if script.is_none() {
            script = Some(arg);
        } else {
            files.push(arg);
        }
    }
    match script {
        Some(script) => Ok((quiet, script, files)),
        None => Err(SedArgError::MissingScript),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (quiet, script, files) = match parse_sed_args(&words[1..]) {
        Ok(parsed) => parsed,
        Err(SedArgError::UnsupportedOption) => {
            support::emit(b"sed: unsupported option\n");
            support::terminate(1);
        }
        Err(SedArgError::MissingScript) => {
            support::emit(b"usage: sed [-n] script [file ...]\n");
            support::terminate(1);
        }
    };
    // The whole script is turned into commands before a single line is read,
    // the way C's `main.c:207` runs `compile()` once and only then `process()`.
    // An address, a verb beyond `s`/`d`/`p`/`=`, or trailing text after a command
    // is a loud failure, never a silent pass-through.
    let mut slots = [Scripted {
        command: Command::Delete,
        line: 1,
    }; MAX_SCRIPT_COMMANDS];
    let count = match parse_script(script, &mut slots) {
        Ok(count) => count,
        Err(failure) => match failure.error {
            // Two shapes have no C sentence to borrow: this build's own command
            // cap, and the addresses and verbs C carries but this shell does not.
            ScriptError::NotModelled => not_modelled(),
            ScriptError::Subst(SubstError::TooLong) => bad_script(),
            error => compile_error(script, ParseFailure { error, ..failure }),
        },
    };
    // `-n` from the command line, or the `#n` line C honours at the start of a
    // script unit (`main.c:286-287`).
    let quiet = quiet || script_quiet_directive(script);
    // Every `s` compiles its expression once here; C does the same inside
    // `compile_subst`, and a chain would otherwise recompile per line per
    // command. An empty pattern is not compiled at all — it names the last
    // expression the script recorded, and with none recorded yet C stops with
    // `first RE may not be empty` (`process.c:668-671`).
    //
    // A replacement asking for more groups than the expression holds is refused
    // in two places with the same sentence, and the two are not interchangeable:
    // with its own expression C tests each digit as it reads the replacement
    // (`compile.c:523-528`), while with a reused one the expression is not known
    // until the command runs, so C tests `s->maxbref` at execution
    // (`process.c:384-391`) — which is why a `d` placed before it silences the
    // complaint for the whole run. That is also why the compile pass below only
    // compiles what a command brings with it: which expression an empty pattern
    // will land on is a fact about the run, not about the script text.
    let mut patterns: Vec<Pattern> = Vec::new();
    let mut program: Vec<Instruction<'_>> = Vec::new();
    for scripted in slots[..count].iter().copied() {
        let own = match scripted.command {
            Command::Subst(subst) => {
                if subst.pattern_text.is_empty() {
                    None
                } else {
                    let pattern = match compile_basic(subst.pattern_text) {
                        Ok(pattern) => pattern,
                        Err(_) => bad_script(),
                    };
                    if let Some(digit) =
                        first_undefined_backref(subst.replacement, pattern.group_count)
                    {
                        backref_error(script, scripted.line, digit);
                    }
                    patterns.push(pattern);
                    Some(patterns.len() - 1)
                }
            }
            _ => None,
        };
        program.push(Instruction {
            command: scripted.command,
            own,
            line: scripted.line,
        });
    }

    let mut scratch = vec![0u8; 4 * 1024 * 1024];
    let mut number = 0u64;
    // C's `defpreg` is a global: it survives lines **and** operands.
    let mut defpreg: Option<usize> = None;
    let mut rval = 0;
    if files.is_empty() {
        let input = support::read_stdin();
        sed_lines(
            script,
            &input,
            quiet,
            &program,
            &patterns,
            &mut scratch,
            &mut number,
            &mut defpreg,
        );
    } else {
        for file in &files {
            match support::read_file(file) {
                Ok(data) => {
                    // The line number is **not** reset between operands: C clears
                    // `linenum` only on the in-place path (`main.c:441-444`), so an
                    // operand list read to standard output keeps counting.
                    sed_lines(
                        script,
                        &data,
                        quiet,
                        &program,
                        &patterns,
                        &mut scratch,
                        &mut number,
                        &mut defpreg,
                    );
                }
                Err(()) => {
                    // C `mf_fgets`: an unopenable operand warns and flags the
                    // exit status but does not stop the run (main.c:449-452).
                    support::warn(format!("sed: {file}\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    support::terminate(rval);
}

/// One command plus the expression it brings with it. Patterns live in their own
/// list because two commands can name the **same** one, so the cycle loop never
/// touches the regex compiler and never copies a pattern. An `s` whose pattern is
/// empty brings nothing: which expression it runs is settled at execution by
/// [`pattern_choice`], because C's `defpreg` is written each time a command
/// actually runs (`process.c:668-671`).
struct Instruction<'a> {
    command: Command<'a>,
    own: Option<usize>,
    /// The script line this command came from, which C restores before printing
    /// its execution-time complaint (`process.c:387`).
    line: u64,
}

/// Substitutes every line of one collected stream and emits what the cycle
/// decides to print. The loop is C's `process()` inner walk (`process.c:100-346`):
/// commands run in order, each carrying the pattern space forward, `d` ends the
/// cycle and suppresses the automatic print, and the line number runs across the
/// whole operand list because only the in-place path clears it.
fn sed_lines(
    script: &str,
    input: &[u8],
    quiet: bool,
    program: &[Instruction<'_>],
    patterns: &[Pattern],
    scratch: &mut [u8],
    number: &mut u64,
    defpreg: &mut Option<usize>,
) {
    let mut space: Vec<u8> = Vec::new();
    for line in support::lines_of(input) {
        *number += 1;
        space.clear();
        space.extend_from_slice(line.as_bytes());
        let mut deleted = false;
        for instruction in program {
            let replaced = match instruction.command {
                Command::Subst(subst) => {
                    let index = match pattern_choice(instruction.own, *defpreg) {
                        PatternChoice::Own(index) => index,
                        PatternChoice::Reuse(index) => {
                            // C reaches this test only while running the command,
                            // and it names `s->maxbref` rather than the first
                            // over-large digit (`process.c:384-391`).
                            let largest = max_backref(subst.replacement);
                            if largest > patterns[index].group_count {
                                backref_error(script, instruction.line, largest);
                            }
                            index
                        }
                        // An empty pattern with nothing recorded yet: C's
                        // `regexec_e` stops the run right there
                        // (`process.c:668-670`).
                        PatternChoice::NoPrevious => first_re_empty(),
                    };
                    *defpreg = Some(index);
                    let (len, replaced) = apply(&patterns[index], &subst, &space, scratch);
                    space.clear();
                    space.extend_from_slice(&scratch[..len]);
                    replaced
                }
                _ => false,
            };
            let step = step(&instruction.command, replaced);
            if step.print_number {
                support::emit(format!("{number}\n").as_bytes());
            }
            if step.print_line {
                emit_line(&space);
            }
            if step.ends_cycle {
                deleted = true;
                break;
            }
        }
        if auto_print(quiet, deleted) {
            emit_line(&space);
        }
    }
}

/// C's `OUT()`: the pattern space and one newline (`process.c:97`).
fn emit_line(space: &[u8]) {
    let mut out = Vec::with_capacity(space.len() + 1);
    out.extend_from_slice(space);
    out.push(b'\n');
    support::emit(&out);
}

/// C's `errx` arms for a malformed `s` command (`compile.c:322-331`: the
/// delimiter may not be a newline or backslash, and an unterminated pattern is
/// fatal), which this shell renders as one line. `errx` writes to standard error
/// and exits (`minix3/lib/libc/gen/verrx.c:57`), so these two rejections go there
/// too —
/// the old form printed them to standard output, which mixed a diagnostic into
/// the very stream a filter's data lives on.
fn bad_script() -> ! {
    support::warn(b"sed: bad s command\n");
    support::terminate(1)
}

/// C's `errx(1, "first RE may not be empty")` (`process.c:668-671`), for an empty
/// pattern with no expression recorded yet. That sentence carries no line number
/// or file name, so this shell's line is the same one `errx` would print: the
/// program name, then the text.
fn first_re_empty() -> ! {
    support::warn(b"sed: first RE may not be empty\n");
    support::terminate(1)
}

/// The rejection for what this build does not carry: an address prefix or a verb
/// outside `s`/`d`/`p`/`=`. C parses all of those, so there is no C sentence to
/// copy here; a silent pass-through would turn a script the kernel cannot run
/// into output that looks like it did.
fn not_modelled() -> ! {
    support::warn(b"sed: address or verb not modelled (see 08 sec 4.4)\n");
    support::terminate(1)
}

/// C's compile-time error line. `errx` prints `<program name>: `
/// (`minix3/lib/libc/gen/verrx.c:57`) and every `compile.c` call starts its
/// format with `"%lu: %s: "` — the script line and `fname` — before the sentence
/// (`compile.c:219`, `:264`, `:325`, `:331`, `:601`, `:636`). The letter slots
/// carry **bytes**, the way C's `%c` writes them: a non-ASCII byte goes out as
/// that one byte instead of being re-encoded.
fn compile_error(script: &str, failure: ParseFailure) -> ! {
    let mut line: Vec<u8> = errx_prefix(script, failure.line);
    match failure.error {
        ScriptError::Subst(SubstError::BadDelimiter) => line.extend_from_slice(
            b"substitute pattern can not be delimited by newline or backslash",
        ),
        ScriptError::Subst(SubstError::Unterminated) => {
            line.extend_from_slice(b"unterminated substitute pattern");
        }
        ScriptError::Subst(SubstError::UnescapedNewline) => {
            line.extend_from_slice(b"unescaped newline inside substitute pattern");
        }
        ScriptError::Subst(SubstError::BadFlag(byte)) => {
            line.extend_from_slice(b"bad flag in substitute command: '");
            line.push(byte);
            line.push(b'\'');
        }
        ScriptError::Subst(SubstError::NumberOverflow) => {
            line.extend_from_slice(b"overflow in the 'N' substitute flag");
        }
        ScriptError::ExtraChars(verb) => {
            line.extend_from_slice(b"extra characters at the end of ");
            line.push(verb);
            line.extend_from_slice(b" command");
        }
        ScriptError::InvalidCode(byte) => {
            line.extend_from_slice(b"invalid command code ");
            line.push(byte);
        }
        // Routed elsewhere; naming them keeps the split from silently swallowing
        // a new arm.
        ScriptError::NotModelled | ScriptError::Subst(SubstError::TooLong) => bad_script(),
    }
    line.push(b'\n');
    support::warn(&line);
    support::terminate(1)
}

/// C's `\N not defined in the RE` line, shared by the two arms that print it.
///
/// The compile-time arm writes the digit it was reading (`compile.c:527-528`'s
/// `%c`); the execution-time one writes `s->maxbref` (`process.c:387-390`'s
/// `%u`). Both are one character because C's set is `1`-`9` (`compile.c:523`), so
/// the digit renders as `'0' + n` either way.
fn backref_error(script: &str, line: u64, ref_number: u8) -> ! {
    let mut message = errx_prefix(script, line);
    message.push(b'\\');
    message.push(b'0' + ref_number);
    message.extend_from_slice(b" not defined in the RE\n");
    support::warn(&message);
    support::terminate(1)
}

/// The `<程序名>: <脚本行号>: "<脚本>": ` that opens every C compile- and
/// execution-time complaint (`verrx.c:57` supplies the program name, `compile.c`
/// and `process.c` supply the line and `fname`).
fn errx_prefix(script: &str, line: u64) -> Vec<u8> {
    let mut message: Vec<u8> = b"sed: ".to_vec();
    message.extend_from_slice(format!("{line}: ").as_bytes());
    message.extend_from_slice(script_ident(script).as_bytes());
    message.extend_from_slice(b": ");
    message
}

/// The name C puts in that line: the script string in quotes, into a
/// `static char string_ident[30]` (`main.c:242`); when the quoted form does not
/// fit in 29 bytes, byte 24 onward becomes ` ..."` (`main.c:261-266`). The cut is
/// by bytes because C cuts bytes — one that would land inside a multibyte
/// character is walked back to a character boundary here, where C would have
/// printed the split bytes raw.
fn script_ident(script: &str) -> String {
    let quoted = format!("\"{script}\"");
    if quoted.len() < STRING_IDENT - 1 {
        return quoted;
    }
    let limit = STRING_IDENT - 6;
    let mut cut = limit.min(quoted.len());
    while cut > 0 && !quoted.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut ident = String::from(&quoted[..cut]);
    ident.push_str(" ...\"");
    ident
}

/// `sizeof(string_ident)` in `minix3/usr.bin/sed/main.c:242`.
const STRING_IDENT: usize = 30;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_only_has_no_files() {
        let (quiet, script, files) = parse_sed_args(&["s/a/b/"]).unwrap();
        assert!(!quiet);
        assert_eq!(script, "s/a/b/");
        assert!(files.is_empty());
    }

    #[test]
    fn dash_n_sets_quiet() {
        let (quiet, _, _) = parse_sed_args(&["-n", "s/a/b/"]).unwrap();
        assert!(quiet);
    }

    #[test]
    fn operands_after_the_script_are_files() {
        let (_, script, files) = parse_sed_args(&["s/a/b/", "f1", "f2"]).unwrap();
        assert_eq!(script, "s/a/b/");
        assert_eq!(files, ["f1", "f2"]);
    }

    #[test]
    fn flags_and_files_mix_in_any_order() {
        // `-n` is recognised wherever it appears; the first positional is the
        // script and the rest are files.
        let (quiet, script, files) = parse_sed_args(&["s/a/b/", "-n", "f1"]).unwrap();
        assert!(quiet);
        assert_eq!(script, "s/a/b/");
        assert_eq!(files, ["f1"]);
    }

    #[test]
    fn bare_dash_is_a_literal_file_operand() {
        // C opens files through fopen, so `-` is not a stdin alias here.
        let (_, _, files) = parse_sed_args(&["s/a/b/", "-"]).unwrap();
        assert_eq!(files, ["-"]);
    }

    #[test]
    fn unsupported_option_is_rejected() {
        assert_eq!(
            parse_sed_args(&["-i", "s/a/b/"]),
            Err(SedArgError::UnsupportedOption)
        );
    }

    #[test]
    fn missing_script_is_rejected() {
        assert_eq!(parse_sed_args(&[]), Err(SedArgError::MissingScript));
        assert_eq!(parse_sed_args(&["-n"]), Err(SedArgError::MissingScript));
    }
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
