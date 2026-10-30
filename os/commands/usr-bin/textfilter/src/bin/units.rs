//! Minix-RS units — conversion program.
//!
//! Ground truth: `minix3/usr.bin/units/units.c`, usage
//! `units [-Llqv] [-f filename] [[count] from-unit to-unit]`. The
//! deciding half (units-file parsing, expression reduction,
//! conformability, answer rendering, the `-l`/`-L` listings) is the
//! library's `units` module; this program runs the option ring, loads
//! the database, and drives the interactive prompt face.
//!
//! The front door has three exit classes, all with exit code 3, each with its
//! own sentence owner. The libc `getopt` line (unknown letter, or the
//! value-taking `-f` missing its value — the optstring at `units.c:817` is
//! `"lLvqf:"`, only `f` takes a value, so the missing-value shape has just `f`
//! reachable in this arm) comes from the single source
//! [`minix_textfilter::GetoptReport`] and is always followed by the whole usage
//! block, because `units.c:843-845` writes `default: usage()`. `-v` prints the
//! two version lines then usage, and those two lines go to **standard error**
//! (`units.c:836-839` is `fprintf(stderr, …)`). A wrong operand count gets only
//! the usage block, with no libc line. The usage block is nine lines (two
//! `fprintf` calls: the first line carries its own leading blank line, and so
//! does the description paragraph); the `units` in it is a literal written into
//! the sentence and does not follow `getprogname()` — only the libc line does.
//!
//! The two file sentences belong to two different arms: a `-f` file that cannot
//! be opened is the three-part `err(1, "Unable to open units file '%s'", …)`
//! (with `strerror`), while the default file
//! `/usr/share/misc/units.lib` (`minix3/usr.bin/units/pathnames.h`) not being
//! found is the two-part `errx(1, "Can't find units file '%s'", …)` (without).
//! The operand-count check runs **before** reading the database
//! (`units.c:849-857`), so "bad file plus one operand" reports usage rather than
//! an open failure.
//!
//! Both argv and interactive lines enter the chain as bytes: option letters,
//! unit names inside operands, the `-l` listing and the echo slot of the
//! `Unknown unit` sentence all return the original bytes; only the `-f` file
//! name's two legs (open and echo) still pass through the library's `&str`
//! door, a registered limitation.
//!
//! Every interactive line goes through `bin_support::read_line_bounded`, one leg
//! for the host and one for the target: this is the tenth fix of the
//! "thin-shell raw `read` cannot see input on the host" family.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::units::{
    PRECISION, PRECISION_LIST_EXPAND, UnitType, UnitsArgError, UnitsTable, add_unit,
    complete_reduce, list_units, parse_units_args, read_units, render_answer, show_answer,
    units_error_line,
};

/// C's `_PATH_UNITSLIB` (`minix3/usr.bin/units/pathnames.h`).
const UNITSFILE: &str = "/usr/share/misc/units.lib";
/// C's `fgets(havestr, 80, stdin)`: `havestr` is `char[81]`, so at most 79 bytes
/// are taken per call and the 80th byte is reserved for the next newline or the
/// next data character; hence on truncation the newline **stays in the stream**.
const LINE_LIMIT: usize = 79;

fn run() -> ! {
    // argv comes back as bytes: C's `crt0` treats `argv` as a byte array and
    // `getopt` takes letters by stepping bytes, so a `ff` after the dash must
    // still be that same `ff` in the message (the byte seam settled in this
    // round).
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program-name leg must be handed to the `&str` doors of `warn_line` and
    // `units_error_line` (a registered limitation), so it folds lossy once here
    // — the same established rule as the other arms.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let prog = prog.as_str();
    let args: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let parsed = match parse_units_args(&args) {
        Ok(parsed) => parsed,
        Err(UnitsArgError::Getopt(report)) => {
            // Go byte-wise: the letter slot of the libc line is one raw byte
            // (`getopt.c:79`'s `optopt = *place++` takes one byte at a time,
            // `:103` writes it out verbatim with `%c`). Measured against the
            // reference binary `tmp/units-319/units`, `units -é` prints
            // `unknown option -- ` + `c3` + newline, whereas rendering through
            // `char` would add a second byte (U+00C3 is `c3 83` in UTF-8).
            support::warn(&report.line_bytes(prog));
            usage();
        }
        Err(UnitsArgError::Version) => {
            support::warn(VERSION.as_bytes());
            usage();
        }
        Err(UnitsArgError::Usage) => usage(),
    };

    let text = load_units_file(prog, parsed.userfile);
    let table = read_units(&split_file_lines(&text));

    if parsed.list {
        let prec = if parsed.listexpand {
            PRECISION_LIST_EXPAND
        } else {
            PRECISION
        };
        let (listing, errors) = list_units(&table, parsed.listexpand, prec);
        support::emit(&listing);
        support::terminate(if errors > 0 { 1 } else { 0 });
    }

    match parsed.operands.len() {
        2 | 3 => convert_operands(prog, &table, &parsed.operands),
        _ => interactive(prog, &table, parsed.quiet),
    }
}

/// The two-or-three operand path (`units.c:855-875`). With three operands the
/// first two are joined with a space into the have side; the ring does not
/// permute, so in `units -f <file> m sec -l` the `-l` is the **third operand**
/// rather than an option.
///
/// Operands arrive as the raw argv bytes (`support::args_bytes()`); invalid
/// UTF-8 travels down the chain just the same.
fn convert_operands(prog: &str, table: &UnitsTable, operands: &[&[u8]]) -> ! {
    let have_expr: Vec<u8> = if operands.len() == 3 {
        let mut joined = operands[0].to_vec();
        joined.push(b' ');
        joined.extend_from_slice(operands[1]);
        joined
    } else {
        operands[0].to_vec()
    };
    let have = build_unit(prog, table, &have_expr);
    let want = build_unit(prog, table, operands[operands.len() - 1]);
    // C ignores the return values of `addunit`/`completereduce` and still emits
    // an answer block from the half-built unit, exiting 0 (`units.c:864-875`);
    // when these two arms return `Err` there is no half-built unit to show, so
    // the messages match C and the exit code matches C, with only the answer
    // block line missing — a registered limitation.
    if let (Some(have), Some(want)) = (have, want) {
        support::emit(&render_answer(&show_answer(&have, &want), PRECISION));
    }
    support::terminate(0);
}

/// The no-operand path: paired `You have: `/`You want: ` prompts.
fn interactive(prog: &str, table: &UnitsTable, quiet: bool) -> ! {
    if !quiet {
        support::emit(
            format!(
                "{} units, {} prefixes\n\n",
                table.units.len(),
                table.prefixes.len()
            )
            .as_bytes(),
        );
    }
    loop {
        let have = read_unit(prog, table, quiet, b"You have: ");
        let want = read_unit(prog, table, quiet, b"You want: ");
        support::emit(&render_answer(&show_answer(&have, &want), PRECISION));
    }
}

/// C's `do { prompt; fgets; } while (addunit || completereduce)`
/// (`units.c:876-893`): a line that is not understood **re-asks the same cell**
/// rather than moving to the opposite cell. At EOF, a non-`-q` run emits one
/// newline first and then exits 0.
///
/// The bytes that line yields enter the unit-name chain unchanged: C never
/// decodes after `fgets`, and both `addunit`→`lookupunit`'s `strcmp` and
/// `mywarnx`'s `'%s'` work on bytes.
fn read_unit(prog: &str, table: &UnitsTable, quiet: bool, prompt: &[u8]) -> UnitType {
    loop {
        if !quiet {
            support::emit(prompt);
        }
        let Some(bytes) = support::read_line_bounded(LINE_LIMIT) else {
            if !quiet {
                support::emit(b"\n");
            }
            support::terminate(0);
        };
        let mut unit = match add_unit(&bytes, false) {
            Ok(unit) => unit,
            Err(error) => {
                report(prog, &error);
                continue;
            }
        };
        match complete_reduce(table, &mut unit) {
            Ok(()) => return unit,
            Err(error) => report(prog, &error),
        }
    }
}

fn build_unit(prog: &str, table: &UnitsTable, expr: &[u8]) -> Option<UnitType> {
    let mut unit = match add_unit(expr, false) {
        Ok(unit) => unit,
        Err(error) => {
            report(prog, &error);
            return None;
        }
    };
    match complete_reduce(table, &mut unit) {
        Ok(()) => Some(unit),
        Err(error) => {
            report(prog, &error);
            None
        }
    }
}

/// The `mywarnx` sentences of the derivation phase (`units.c:109-124`). In
/// non-list mode `errprefix` is `NULL`, and the whole sentence goes through
/// `vwarnx` to standard error.
fn report(prog: &str, error: &minix_textfilter::units::UnitsError) {
    if let Some(line) = units_error_line(prog, error) {
        support::warn(&line);
    }
}

/// Read the dictionary file. The `-f` and default-file failure sentences belong
/// to the `err` and `errx` arms respectively; see the comparison in the module
/// header.
fn load_units_file(prog: &str, path: Option<&[u8]>) -> Vec<u8> {
    match path {
        Some(raw) => {
            // Both legs of the `-f` file name (open and echo) still pass through
            // the library's `&str` door: `minix_sys::open` and
            // `minix_types::errno::err_line` both take `&str` (a registered
            // limitation), so fold lossy once here until the library switches
            // to the byte form.
            let name = String::from_utf8_lossy(raw);
            match support::read_file(&name) {
                Ok(bytes) => bytes,
                Err(code) => {
                    support::warn_line(prog, &format!("Unable to open units file '{name}'"), code);
                    support::terminate(1);
                }
            }
        }
        None => match support::read_file(UNITSFILE) {
            Ok(bytes) => bytes,
            // C first searches `PATH` directory by directory here
            // (`units.c:146-176`); that segment is a registered limitation.
            // Neither shape lands on a directory testable on this host, and
            // this sentence is the final failure.
            Err(_) => {
                support::warn(format!("{prog}: Can't find units file '{UNITSFILE}'\n").as_bytes());
                support::terminate(1);
            }
        },
    }
}

/// The dictionary's newline split works on bytes (C's
/// `fgets(line, 79, unitfile)` never decodes). The old form decoded the whole
/// file as UTF-8 first and treated the **entire** dictionary as an empty string
/// when that failed, so one raw byte could make every unit disappear — the
/// registered limitation that this slice removed along with `read_units`' switch
/// to byte keys.
fn split_file_lines(bytes: &[u8]) -> Vec<&[u8]> {
    // `split` yields one extra empty item after the file's trailing newline;
    // `read_units` skips empty names uniformly, which is equivalent to C's
    // `fgets` not producing another line at EOF.
    bytes.split(|&byte| byte == b'\n').collect()
}

/// The `usage()` of `units.c:793-807`: two `fprintf` calls making nine lines,
/// exit code 3.
///
/// `concat!` is used rather than byte strings with backslash line continuations:
/// a continuation eats the **leading whitespace** of the next line, and that is
/// exactly how the four spaces of indentation got lost (the first run of this
/// gate caught this very spot).
const USAGE: &str = concat!(
    "\nunits [-Llqv] [-f filename] [[count] from-unit to-unit]\n",
    "\n    -f specify units file\n",
    "    -L list units in standardized base units\n",
    "    -l list units\n",
    "    -q suppress prompting (quiet)\n",
    "    -v print version number\n",
);

/// The two version lines of `units.c:836-839`, from the same
/// `fprintf(stderr, …)`.
const VERSION: &str = concat!(
    "\n  units version 1.0  Copyright (c) 1993 by Adrian Mariano\n",
    "                    This program may be freely distributed\n",
);

fn usage() -> ! {
    support::warn(USAGE.as_bytes());
    support::terminate(3);
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
