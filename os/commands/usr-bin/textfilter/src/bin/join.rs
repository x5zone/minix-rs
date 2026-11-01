//! Minix-RS `join` — the doing half over `minix_textfilter::join`.
//!
//! Ground truth: `minix3/usr.bin/join/join.c`. The merge and field logic are
//! the library (`join_lines`/`JoinOptions`); this program parses the option
//! words, reads the two file operands (a `-` names standard input), feeds the
//! newline-stripped lines to the library, and emits the result.
//!
//! Wired: default join, `-t` (delimiter), `-1`/`-2` (per-file join field),
//! `-a 1`/`-a 2` (unmatched lines), `-o` (output field list, `0`/`F.N`), `-e`
//! (empty-field replacement), `-j field` (both join fields at once — the BSD
//! alias for `-1 field -2 field`) and `-v fileno` (that file's unpairable lines
//! only, with matched output suppressed). `-a` and `-v` are mutually exclusive
//! (`join.c:234-235`). Option values take either form the BSD `getopt`
//! (`join.c:133`) allows: attached (`-t,`, `-a1`, `-o0,1`) or as the following
//! argument (`-t ,`). The C `obsolete(argv)` pre-pass (`join.c:132`) is modelled:
//! legacy bare `-a` becomes the synthetic option letter `\01` (both files emit
//! unpaired lines) and `-j1`/`-j2` become `-1`/`-2`, with malformed `-j3`/`-j12`
//! refused before the option ring ever starts.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::join::{JoinError, join_lines, parse_join_args};

/// Split a buffer into newline-stripped lines, dropping the empty tail that
/// a newline-terminated file leaves.
fn line_refs(buf: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = buf.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }
    lines
}

/// Write the `<program name>: <sentence>` line. **No exit** — C's `warnx`
/// prints it and keeps going; only `errx` calls `exit`. Which of the two this
/// is belongs to the caller (the `fail` family exits, `warn_then_usage`
/// follows up with usage).
fn diagnose(prog: &str, message: &[u8]) {
    let mut line = Vec::with_capacity(prog.len() + message.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(message);
    line.push(b'\n');
    support::warn(&line);
}

/// C's `errx` family: program name (`getprogname()`), the phrase, no appended
/// sentence, exit 1 after writing. The phrase comes back as **bytes** — the
/// five arms `join.c:145`/`:156`/`:176`/`:190`/`:592` splice `optarg` or the
/// whole token into the sentence, and that span is argv's raw bytes; any
/// character decoding would change its length.
fn fail(prog: &str, message: &[u8]) -> ! {
    diagnose(prog, message);
    support::terminate(1)
}

/// Concatenate `prefix` and argv's raw bytes into one sentence (shared by
/// C's five `"... -- %s"` arms).
fn quoted(prefix: &str, value: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(prefix.len() + value.len());
    message.extend_from_slice(prefix.as_bytes());
    message.extend_from_slice(value);
    message
}

/// `join.c:630-637`: `fprintf(stderr, "usage: %s ...\n            ...\n", getprogname())` —
/// **two lines**, the name appears only on the first one (the twelve-space
/// indent on the second line continues it under column 12).
fn usage(prog: &str) -> ! {
    support::warn(
        format!("usage: {prog} [-a fileno | -v fileno] [-e string] [-j fileno field]\n").as_bytes(),
    );
    support::warn(b"            [-o list] [-t char] [-1 field] [-2 field] file1 file2\n");
    support::terminate(1)
}

/// The shape of C's `warnx` family here: first one line carrying the program
/// name, then those two usage lines, exit 1 (the eight arms such as
/// `join.c:141-142` are all `warnx(...)` immediately followed by `usage()`).
fn warn_then_usage(prog: &str, message: &[u8]) -> ! {
    diagnose(prog, message);
    usage(prog)
}

/// Render the [`JoinError`] the decision half hands back as C's own message
/// for that arm. **Three `warnx`+`usage()` arms, three `errx` arms (no usage),
/// one usage-only arm** — the branching itself (which letter, which value,
/// whether usage follows) lives in the variant payloads, so the shell never
/// invents wording: `join.c:141`/`:145`/`:152`/`:156`/`:171`/`:176`/`:186`/
/// `:190`/`:202`/`:217`/`:222` and `fieldarg`'s `:527-532` correspond arm by
/// arm.
fn join_error_fail(prog: &str, e: JoinError<'_>) -> ! {
    match e {
        // `join.c:236-237`: `if (argc != 2) usage();` — only the two usage
        // lines, no diagnostic line.
        JoinError::Usage => usage(prog),
        // Unknown option letter or a value-taking option missing its value:
        // libc prints its own line first (`getopt.c:101-104`, `:132-136`),
        // then falls into `join.c:226-228`'s `case '?'` → the same two usage
        // lines.
        JoinError::Getopt(report) => {
            // The letter slot goes through `line_bytes`: `src/join.rs` already
            // takes it as a byte (`b[1] as char`), and libc's line has a
            // single byte in the letter slot anyway (`optopt = *place++` at
            // `getopt.c:79`, `%c` at `:103`). A measured reference run of
            // `-é` prints `join: unknown option -- ` + one `c3` byte +
            // newline, then the two usage lines.
            support::warn(&report.line_bytes(prog));
            usage(prog)
        }
        // The sentence puts the option letter first (`"-1 option field number less than 1"`).
        JoinError::FieldLessThan1(opt) => {
            warn_then_usage(prog, format!("{opt} option field number less than 1").as_bytes())
        }
        JoinError::FileNumberNot12(opt) => {
            warn_then_usage(prog, format!("{opt} option file number not 1 or 2").as_bytes())
        }
        // These two arms write no letter in C; they put `optarg` after a pair
        // of dashes (`"illegal field number -- %s"`): the value span is argv's
        // raw bytes, so the sentence is assembled bytewise.
        JoinError::IllegalFieldNumber(value) => {
            warn_then_usage(prog, quoted("illegal field number -- ", value).as_slice())
        }
        JoinError::IllegalFileNumber(value) => {
            warn_then_usage(prog, quoted("illegal file number -- ", value).as_slice())
        }
        // The `-t` arm likewise writes no letter (C also writes only
        // `"illegal tab character specification"`).
        JoinError::BadTabChar => warn_then_usage(prog, b"illegal tab character specification"),
        // The three arms below go through `errx` in C: print and leave, no
        // usage.
        JoinError::MalformedOutputField => fail(prog, b"malformed -o option field"),
        JoinError::FieldNumbersAre1Based => fail(prog, b"field numbers are 1 based"),
        JoinError::AAndVMutuallyExclusive => fail(prog, b"-a and -v options mutually exclusive"),
        // The malformed legacy `-j` forms: C does
        // `errx(1, "illegal option -- %s", ap)` inside `obsolete()`
        // (`join.c:592`); the sentence carries the whole token (leading dash
        // and raw byte tail included) and **no usage**.
        JoinError::IllegalOptionToken(token) => {
            fail(prog, quoted("illegal option -- ", token).as_slice())
        }
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C's `join` does not call `setprogname`; `getprogname()` is still primed
    // from `argv[0]` by `minix3/lib/csu/common/crt0-common.c:156-165`, so all
    // three of its families (`errx`/`err`/`usage`) start with that span. The
    // program name is handed back as raw bytes; only the two `&str` mouths
    // below (opening a file, `err(1, "%s", name)`) and the two usage lines
    // need the text form, so the fold happens once.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));

    let (opts, files) = match parse_join_args(&args[1..]) {
        Ok(a) => (a.opts, a.files),
        Err(e) => join_error_fail(&prog, e),
    };

    // `join.c:250-251`: when both operands are `-` both `FILE*`s equal
    // `stdin`, and C takes `errx(1, "only one input file may be stdin")`
    // right away — this shell used to lack that arm and would read the same
    // input twice. C guards it before `slurp`; here it sits before both
    // reads. The `-` test is bytewise, so it does not depend on whether the
    // filename was ever folded.
    if files[0] == b"-" && files[1] == b"-" {
        fail(&prog, b"only one input file may be stdin");
    }

    // `join.c:241-249`: a `-` operand reads standard input, otherwise
    // `fopen`; failure is `err(1, "%s", *argv)` — the operand name and the
    // errno sentence come out together and the program ends there
    // (**fatal**: the second operand is never even tried). Opening the file
    // and that line both need the `&str` mouth, so the fold happens up front
    // (registered as `ARGV-BYTE-PATH-IN-LIBS`).
    let read_operand = |name: &[u8]| -> Vec<u8> {
        if name == b"-" {
            // C's `join` consults `ferror` **only for stdout**
            // (`minix3/usr.bin/join/join.c:457,488,498,508`); there is no
            // input-side check anywhere, so a failed read simply ends the input
            // and nothing is reported. The read still goes through the crate's
            // one standard-input seam (`support::read_stdin`, which is
            // `read_stdin_error` without the number) so the seam has a single
            // implementation — the number is dropped here **because C drops it**,
            // not because this shell forgot to look. (`JOIN-STDIN-LEG-BYPASSES-DUAL-SURFACE`,
            // registered in batch 463 with the opposite implication, corrected here.)
            return support::read_stdin();
        }
        let path = String::from_utf8_lossy(name);
        match support::read_file(&path) {
            Ok(bytes) => bytes,
            Err(code) => {
                support::warn_line(&prog, &path, code);
                support::terminate(1);
            }
        }
    };
    let b1 = read_operand(files[0]);
    let b2 = read_operand(files[1]);

    let out = join_lines(&line_refs(&b1), &line_refs(&b2), &opts);
    let mut rendered = Vec::new();
    for line in &out {
        rendered.extend_from_slice(line);
        rendered.push(b'\n');
    }
    support::emit(&rendered);
    support::terminate(0);
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
