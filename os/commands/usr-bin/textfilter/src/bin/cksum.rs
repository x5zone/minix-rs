//! Minix-RS cksum — checksums and byte counts.
//!
//! Ground truth: `minix3/usr.bin/cksum/` (NetBSD). The deciding half —
//! the POSIX CRC, the two historic sums, the block-rounding of the printed
//! length, and the `%lu %lld` output format (`print.c` `pcrc`/`psum1`/`psum2`)
//! — is the library's `cksum` module. This program reads each file operand (or
//! standard input when none) and prints `{crc} {length}` plus ` {name}` for a
//! file. The `-a` digest leg goes through the real body of `minix_textfilter::digest`
//! (all eight algorithms wired), and the report shapes are C's `MD5 (file) = <digest>`
//! and its `-n`/`-q` variants; the three self-printing legs `-s`/`-t`/`-x` are emitted
//! in argv order ahead of diagnostics (see `emit_action` for the criterion and the
//! readings); the `-c` check leg inside hash mode recomputes and compares line by line
//! as C's `do_check` does (see `check_files`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::cksum::{
    CheckLine, CksumAction, CksumError, CksumRequest, SumKind, cksum_crc, classify_check_line,
    csum1, csum2, leading_u32, parse_cksum_args_into, sum_display_length,
};
use minix_textfilter::digest::{self, Algo};

fn run() -> ! {
    // argv comes back as bytes: C's `crt0` hands over a byte array, and the option loop
    // at `cksum.c:193` passes `argv` to `getopt` verbatim, so the value leg and the
    // operand leg both receive those bytes.
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program-name leg has to go through `warn_line`'s `&str` opening (registered
    // `ARGV-BYTE-PATH-IN-LIBS`), so it is folded lossily once inside the arm, the same
    // established policy as every other branch.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let prog = prog.as_str();
    let args: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let mut actions: Vec<CksumAction> = Vec::new();
    let parsed = parse_cksum_args_into(&args, &mut actions);
    // C's three legs `-s`/`-t`/`-x` finish printing **inside** the getopt loop
    // (`cksum.c:255-274`), before later option errors can catch up: reference fixtures
    // e-1/e-2/e-3 all carry that line on stdout and still exit 1. So this pass runs
    // before the diagnostics and is not part of `hash_run`.
    for action in &actions {
        emit_action(action);
    }
    let request = match parsed {
        Ok(request) => request,
        Err(CksumError::IllegalOptionValue(flag)) => {
            // C: `warnx("illegal argument to -o option")` + `usage()` (`cksum.c:219`, `:243`).
            support::warn(format!("{prog}: illegal argument to -{flag} option\n").as_bytes());
            usage();
        }
        Err(CksumError::IllegalUseOfA) => {
            // C's first line of `case 'a'` (`cksum.c:195-199`): the two-part `warnx` form,
            // with no strerror suffix, followed by `usage()`. This shape is reachable only
            // once a hash has been selected, so it follows the state.
            support::warn(format!("{prog}: illegal use of -a option\n").as_bytes());
            usage();
        }
        Err(CksumError::MutuallyExclusiveWithSum(name)) => {
            // C's `warnx("%s mutually exclusive with sum", hash->hashname)`
            // (`cksum.c:228-234`): the sentence carries the **upper-case** name from the
            // table (`MD5`, not `md5`).
            support::warn(format!("{prog}: {name} mutually exclusive with sum\n").as_bytes());
            usage();
        }
        Err(CksumError::RequiresHash(flag)) => {
            // C's `requirehash` (`cksum.c:521-525`): `<flag> flag requires `-a algorithm'`
            // followed by usage. The old wording invented "requires a hash algorithm" and
            // skipped usage.
            support::warn(format!("{prog}: -{flag} flag requires `-a algorithm'\n").as_bytes());
            usage();
        }
        Err(CksumError::UnknownOption(flag)) => {
            // libc's sentence (`getopt.c:101-104`) is `unknown option`, not `illegal
            // option` — the latter belongs to the `pr` chain's `egetopt`, and the two must
            // not be mixed in one family. The letter position goes through `line_bytes`:
            // `getopt.c:79`'s `optopt = *place++` takes one byte at a time, so `-é`
            // (`c3 a9`) reports `c3`.
            support::warn(&GetoptReport::UnknownOption(flag).line_bytes(prog));
            usage();
        }
        Err(CksumError::MissingValue(flag)) => {
            // Option with a value missing its value: libc's other sentence (`getopt.c:132-136`), likewise followed by usage.
            support::warn(&GetoptReport::MissingValue(flag).line_bytes(prog));
            usage();
        }
    };

    if request.do_check {
        // C's whole `if (do_check)` block sits **before** the computing branch
        // (`cksum.c:283`), so `-a md5 -c file` takes the check leg; `-s`/`-t`/`-x` have
        // already printed their lines in the option-loop pass.
        // `-c` takes only the first operand as the check file (the rest are ignored),
        // mirroring C's `argv[0]`-or-stdin.
        support::terminate(check_files(
            prog,
            &request.operands,
            request.check_warn,
            request.hash,
        ));
    }

    if let Some(algo) = request.hash {
        // An algorithm is selected: the eight bodies and the three legs `-s`/`-t`/`-x`
        // are already wired to real readings (`src/digest.rs` and `emit_action` above);
        // only the digest body remains on this leg.
        support::terminate(hash_run(prog, &request, algo));
    }

    let mut status = 0;
    if request.operands.is_empty() {
        // C computes over stdin and prints the line with no filename.
        let input = support::read_stdin();
        emit_line(request.kind, &input, None);
        support::terminate(status);
    }
    for operand in request.operands.iter().copied() {
        // The read seam goes through `bin_support`'s pair of cfg twins instead of writing
        // `minix_sys::open`/`read` directly: the direct leg can never read on the host (a
        // host run of `printf 'abc' | cksum` yields `4294967295 0`, i.e. the CRC of zero
        // bytes), so whether "the three-part sentence is complete" cannot be compared on
        // the host at all — a report-shape gate would pin the sentence the placeholder
        // transport returns, not the kernel's.
        // This step also retires the registration `CKSUM-HOSTED-READ-SEAM` (the fix is
        // the same one already applied to the digest leg).
        // C's `warn("%s", fn)` (`cksum.c:491-495`) is `warn(3)`, not `warnx(3)`: program
        // name, operand, and `strerror` sentence are all required (the old wording wiped
        // the third part).
        // The file-name leg's two `&str` openings (open and the three-part report
        // sentence) live in the library, registered `ARGV-BYTE-PATH-IN-LIBS`; the file
        // name echoed in the digest line uses the raw bytes.
        let path = String::from_utf8_lossy(operand);
        let input = match support::read_file(&path) {
            Ok(input) => input,
            Err(code) => {
                support::warn_line(prog, &path, code);
                status = 1;
                continue;
            }
        };
        emit_line(request.kind, &input, Some(operand));
    }
    support::terminate(status)
}

/// C's `hash` path: the loop body of `cksum.c:474-491` plus `hash_digest_file`
/// (`:506-524`). With operands, print one digest line per file; without operands, read
/// standard input to the end — under `-p`, first copy the bytes verbatim to standard
/// output — then print the one digest line (`md5.c`'s `MD5Filter`; the digest line
/// always comes after the copy).
///
/// The three shapes are decided in C by `print_flags`, and `-q` beats `-n`
/// (`cksum.c:515-520` is `if QUIET … else if NORMAL … else …`):
///
/// * default `MD5 (file) = <digest>`;
/// * `-n` `<digest> file`;
/// * `-q` only `<digest>`.
///
/// This leg has no fallback left: the lines of the three self-printing legs
/// `-s`/`-t`/`-x` live in `emit_action`, the `-c` check leg inside hash mode lives in
/// `check_files`, and this shell's placeholder report (`cksum: -a crypto algorithms are
/// not modelled yet`, exit 2) has been deleted wholesale.
///
/// Reading files goes through `support::read_file`/`support::read_stdin`, the same pair
/// of cfg twins as the crc leg (the digest leg was switched over first, then the crc
/// leg). The seam that wrote `minix_sys::open` directly can neither open nor read on
/// the host (a run of `printf 'abc' | cksum` gives `4294967295 0`, i.e. zero bytes), so
/// a host gate could only pin the digest of empty input and the error number the
/// placeholder transport returns — that kind of gate is worse than none.
fn hash_run(prog: &str, request: &CksumRequest<'_>, algo: Algo) -> i32 {
    if request.operands.is_empty() {
        // C's `else if (hash && !nohashstdin)` (`cksum.c:489`): if any of `-s`/`-t`/`-x`
        // appeared, this leg is skipped entirely — their own lines were already emitted
        // by `emit_action` ahead of diagnostics (reference fixture s-10: `-a md5 -p -s abc`
        // prints only that one digest line, neither copying nor digesting standard input).
        if request.nohash_stdin {
            return 0;
        }
        // A read failure on C's side is "read to the end" (`MD5Filter`'s
        // `while ((len = fread(…)) > 0)` ignores `ferror`), and this shell's `read_stdin`
        // follows the same rule, so there is no error leg to report here.
        let input = support::read_stdin();
        if request.pipe {
            support::emit(&input);
        }
        support::emit(format!("{}\n", algo.hex_digest(&input)).as_bytes());
        return 0;
    }

    let mut status = 0;
    for operand in request.operands.iter().copied() {
        let path = String::from_utf8_lossy(operand);
        let input = match support::read_file(&path) {
            Ok(input) => input,
            Err(code) => {
                // When C's `filefunc` returns `NULL`, `hash_digest_file` returns 1 and
                // `main` therefore does `warn("%s", fn)` and sets the overall `rval` to 1
                // (`cksum.c:477-483`), **without printing a digest line**.
                // Reference fixture f-1: `cksum_ref: <path>: No such file or directory`;
                // the directory shape is `.: Is a directory` (f-3); the sentence comes from
                // the hosted `strerror` table, and this shell uses the same table
                // (`bin_support::warn_line` → `minix_sys::err_line`).
                support::warn_line(prog, &path, code);
                status = 1;
                continue;
            }
        };
        // All three shapes echo the operand's raw bytes as the file name: C hands out
        // `printf("…%s…", fn)`, and that `fn` is the byte string from argv; encoding is
        // never consulted.
        let digest = algo.hex_digest(&input);
        if request.print_quiet {
            support::emit(format!("{digest}\n").as_bytes());
        } else if request.print_normal {
            let mut line = format!("{digest} ").into_bytes();
            line.extend_from_slice(operand);
            line.push(b'\n');
            support::emit(&line);
        } else {
            let mut line = format!("{} (", algo.hashname()).into_bytes();
            line.extend_from_slice(operand);
            line.extend_from_slice(format!(") = {digest}\n").as_bytes());
            support::emit(&line);
        }
    }
    status
}

/// C's `MD5String` line (`minix3/usr.bin/cksum/md5.c:60-67`): `NAME ("<string>") = <digest>`.
/// The string is **echoed verbatim**, not escaped and with no extra punctuation
/// (reference fixtures s-2 with spaces and s-3 with the empty string both look like
/// this); this line is unaffected by `-n`/`-q`/`-p`, because the `printf` format string
/// is hardwired in `md5.c` and `print_flags` is read only on the `hash_digest_file` leg
/// (`cksum.c:506-524`).
fn string_line(algo: Algo, text: &[u8]) -> Vec<u8> {
    let mut line = format!("{} (\"", algo.hashname()).into_bytes();
    line.extend_from_slice(text);
    line.extend_from_slice(format!("\") = {}\n", algo.hex_digest(text)).as_bytes());
    line
}

/// Emit the bytes of one self-printing leg. How many lines each of the three legs prints
/// and what starts each line is decided in the three functions of `md5.c`, shared by all
/// eight algorithms (the comment on `digest::TEST_SUITE` records why).
fn emit_action(action: &CksumAction<'_>) {
    match action {
        CksumAction::String(algo, text) => support::emit(&string_line(*algo, text)),
        CksumAction::TestSuite(algo) => {
            support::emit(format!("{} test suite:\n", algo.hashname()).as_bytes());
            for text in digest::TEST_SUITE {
                support::emit(&string_line(*algo, text.as_bytes()));
            }
        }
        CksumAction::TimeTrial(algo) => {
            // C writes in two steps: first this sentence plus `fflush` (`md5.c:81-83`),
            // then the folding, and only when done the ` done` and the three lines. This
            // does the same in two steps rather than building one string — the `Time` and
            // `Speed` values can only be known after folding anyway.
            support::emit(
                format!(
                    "{} time trial.  Digesting {} {}-byte blocks ...",
                    algo.hashname(),
                    digest::TIME_TRIAL_BLOCK_LEN,
                    digest::TIME_TRIAL_BLOCK_COUNT
                )
                .as_bytes(),
            );
            let start = support::wall_seconds();
            let hex = algo.hex_digest(&digest::time_trial_data());
            let elapsed = support::wall_seconds() - start;
            let total = (digest::TIME_TRIAL_BLOCK_LEN * digest::TIME_TRIAL_BLOCK_COUNT) as i64;
            // C's `(endTime - startTime) != 0 ? (endTime - startTime) : 1`: anything that
            // finishes within a second counts as 1 second, so the reading for 1000×1000
            // bytes is always `Speed = 1000000 bytes/second`.
            let divisor = if elapsed != 0 { elapsed } else { 1 };
            support::emit(
                format!(
                    " done\nDigest = {hex}\nTime = {elapsed} seconds\nSpeed = {} bytes/second\n",
                    total / divisor
                )
                .as_bytes(),
            );
        }
    }
}

/// `cksum -c`: C's `do_check` (`cksum.c:283-465`) verifies the file line by line.
/// Three behaviors: a fully correct line says **nothing**; a failed check prints
/// `<file>: FAILED` (prefixed `(<NAME>) ` when hash mode is on, `:454`); any failed line
/// in the whole run (or a format failure) makes the exit status 1.
///
/// The `hash` state is **kept across lines**: when C meets the `NAME (file) = digest`
/// shape it points `hash` there (`hash = nhash`, `:363`), and from then on ordinary
/// lines in the same file without a `(` also take the digest branch — reference
/// fixtures h-1/h-3/h-4 all fail as a result, and h-4 never even passed `-a`: the
/// algorithm is recognized from the line prefix of the file's first line. So this
/// function carries `hash` through the loop instead of judging each line alone.
///
/// In the digest branch, when the file cannot be read, C takes the
/// `access(filename, R_OK) == 0` short circuit (`:424`) and on failure prints only
/// `(MD5) <file>: FAILED` — it does **not** warn, which forks from the crc branch's
/// `warn("%s", filename)` (`:432`) (reference fixtures c-5/h-13 confirm both). The crc
/// branch's warn on a failed read is printed, as in C, only under `-w`.
///
/// Each of the three format legs has its own sentence: `unknown hash: <whole line>`
/// (`:358`, only when the line prefix is unrecognized), `bogus format: <whole line>.
/// Skipping...` (`:325` and `:398`, same sentence), `no filename in <whole line>?
/// Skipping...` (`:381` and `:409`). All three print only under `-w` (C's
/// `if (check_warn)` wraps each), but **each records a failure** (`rval = 1`).
fn check_files(prog: &str, operands: &[&[u8]], check_warn: bool, selected: Option<Algo>) -> i32 {
    let from_operand = !operands.is_empty();
    // The whole check-file leg (open, the file name in the `Cannot read` sentence, and
    // the line-by-line parse of the file content) still goes through the text seam,
    // registered `CKSUM-CHECK-LEG-TEXTSEAM` for a separate round; this slice covers the
    // option loop and the digest echo.
    let path = String::from_utf8_lossy(operands.first().copied().unwrap_or(b""));
    let bytes = if from_operand {
        match support::read_file(&path) {
            Ok(bytes) => bytes,
            // A check file that cannot be opened is, in C, `err(1, "Cannot read %s",
            // argc>0?argv[0]:"stdin")` (`cksum.c:301-303`): `err(3)` = those three parts
            // plus `exit(1)`. Reference fixture c-9 shows
            // `cksum_ref: Cannot read <path>: No such file or directory`, exit 1, empty
            // stdout.
            Err(code) => {
                support::warn_line(prog, &format!("Cannot read {path}"), code);
                return 1;
            }
        }
    } else {
        support::read_stdin()
    };
    let Ok(text) = core::str::from_utf8(&bytes) else {
        return 1;
    };
    // C reads line by line with `fgets`, so the trailing newline terminates the last
    // line without producing a phantom empty line; strip exactly that one.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut hash = selected;
    let mut failed = false;
    if !body.is_empty() {
        for line in body.split('\n') {
            match classify_check_line(line, hash) {
                CheckLine::Hash {
                    algo: None,
                    name,
                    digest,
                } => {
                    // When C cannot recognize the line prefix it discards the whole line,
                    // reports `unknown hash` (under `-w`) and records a failure; it does
                    // **not** print a FAILED line (reference fixtures c-3/c-4: empty stdout).
                    let _ = (name, digest);
                    if check_warn {
                        support::warn(format!("cksum: unknown hash: {line}\n").as_bytes());
                    }
                    failed = true;
                }
                CheckLine::Hash {
                    algo: Some(algo),
                    name,
                    digest,
                } => {
                    hash = Some(algo);
                    if !verify_digest(algo, name, digest) {
                        support::emit(format!("({}) {name}: FAILED\n", algo.hashname()).as_bytes());
                        failed = true;
                    }
                }
                CheckLine::Malformed => {
                    if check_warn {
                        support::warn(
                            format!("cksum: bogus format: {line}. Skipping...\n").as_bytes(),
                        );
                    }
                    failed = true;
                }
                CheckLine::NoFilename => {
                    if check_warn {
                        support::warn(
                            format!("cksum: no filename in {line}? Skipping...\n").as_bytes(),
                        );
                    }
                    failed = true;
                }
                CheckLine::Token { token, name } => {
                    let ok = match hash {
                        // `hash` selected: compare the whole token string with the recomputed
                        // digest (the `strcmp` at `:424-426`); an unreadable file only counts as
                        // a failure and does not warn (reference fixture h-13).
                        Some(algo) => verify_digest(algo, name, token),
                        // crc branch: the first word is parsed as a leading decimal via
                        // `strtoul` (`:442`), and an unreadable file warns under `-w` (`:432`).
                        None => match support::read_file(name) {
                            // In the crc branch, C's warning for an unreadable file is
                            // `warn("%s", filename)` (`:432`), also three-part and only under
                            // `-w` (`if (check_warn)` wraps it). The same cell in the digest
                            // branch does **not** print this sentence (`access` short circuit,
                            // `:424`); that cell's gate is
                            // `an_unreadable_operand_in_a_digest_line_fails_without_warning`.
                            Err(code) => {
                                if check_warn {
                                    support::warn_line(prog, name, code);
                                }
                                false
                            }
                            Ok(data) => cksum_crc(&data).0 == leading_u32(token),
                        },
                    };
                    if !ok {
                        match hash {
                            Some(algo) => support::emit(
                                format!("({}) {name}: FAILED\n", algo.hashname()).as_bytes(),
                            ),
                            None => support::emit(format!("{name}: FAILED\n").as_bytes()),
                        }
                        failed = true;
                    }
                }
            }
        }
    }
    i32::from(failed)
}

/// Read the file `name` to the end, recompute the digest with `algo`, and compare it
/// verbatim with the `expected` recorded in the file. An unreadable file is a failure:
/// on C's side it is either blocked by `access` (`:424`) or `filefunc` returns `NULL`,
/// which it feeds straight to `strcmp`, so that line crashes on the spot (`:425`); this
/// shell records it as a failure instead of crashing along.
fn verify_digest(algo: Algo, name: &str, expected: &str) -> bool {
    match support::read_file(name) {
        Ok(data) => algo.hex_digest(&data) == expected,
        Err(_) => false,
    }
}

/// Compute the selected sum over `data` and print C's `"%lu %lld"` line (with a
/// trailing ` name` for a file operand), the length folded per `print.c`.
fn emit_line(kind: SumKind, data: &[u8], name: Option<&[u8]>) {
    let (value, bytes) = match kind {
        SumKind::Crc => cksum_crc(data),
        SumKind::Old1 => csum1(data),
        SumKind::Old2 => csum2(data),
    };
    let shown = sum_display_length(kind, bytes);
    match name {
        Some(n) => {
            let mut line = format!("{value} {shown} ").into_bytes();
            line.extend_from_slice(n);
            line.push(b'\n');
            support::emit(&line)
        }
        None => support::emit(format!("{value} {shown}\n").as_bytes()),
    }
}

/// C's `usage()` (`cksum.c:535-550`): nine lines — `cksum`/`sum` take two each (argument
/// string and file string on separate lines), and `md2`/`md4`/`md5`/`rmd160`/`sha1` take
/// one each. In C the names are **literals** (it passes the three strings
/// `sumargs`/`hashargs`/`fileargs`, not `getprogname()`), so this block's indentation
/// and line items must be copied verbatim. `exit(1)`.
fn usage() -> ! {
    support::warn(
        concat!(
            "usage: cksum [-n] [-a algorithm [-ptx] [-s string]] [-o 1|2]\n",
            "             [file ... | -c [-w] [sumfile]]\n",
            "       sum [-n] [-a algorithm [-ptx] [-s string]] [-o 1|2]\n",
            "           [file ... | -c [-w] [sumfile]]\n",
            "       md2 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       md4 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       md5 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       rmd160 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       sha1 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
        )
        .as_bytes(),
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
