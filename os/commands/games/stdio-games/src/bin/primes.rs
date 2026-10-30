//! Minix-RS primes — prime generation over an interval.
//!
//! Ground truth: `minix3/games/primes/primes.c`, `primes [-dh] [start [stop]]`.
//! The library caps the interval at `MAX_BOUND` (1,000,000 — the trial
//! division engine is built for short lived runs, not the C streaming
//! sieve), so with no arguments the interval is 2 to the cap instead of the
//! C default near 2^32; one argument starts there, two bound it.
//!
//! `stop` is an **exclusive** bound — C says so in its own header
//! (`primes.c:49`, "Print primes >= start and < stop") and the reference
//! process agrees: `primes 5 13` stops at 11, `primes 2 2` prints nothing.
//! The shell expresses that by lowering the sieve's inclusive upper bound by
//! one; `sieve` itself stays the generic inclusive primitive.
//!
//! The diagnostic face is wired to C leg by leg (`primes.c:133-172`): the
//! negative check and the two conversion kinds are different lines in C, and
//! which family each belongs to is not interchangeable —
//! `err(1, "%s", argv[i])` when `strtoumax` set `errno` (`_strtol.h:69` `EINVAL`,
//! `:157`/`:172` `ERANGE`) prints `primes: <string>: <sentence>`, while
//! `errx(1, "%s: illegal numeric format.")` for trailing junk
//! (`primes.c:140`/`:147`/`:160`) and the size and ordering refusals
//! (`:149`, `:172`) print a phrase and no sentence. `usage()` here is a
//! hardcoded literal (`:339-343`) — unlike `rm`/`cp`/`mv`, this one does not
//! carry `getprogname()`.
//!
//! One leg of C's own remains unwired and is declared rather than hidden: option
//! *permutation* — C's getopt moves an option letter found after an operand back into
//! the option scan, which this shell's single-pass parser does not (a separate
//! registered gap). The zero-operand path (`read_num_buf`, `:176-205`) and the
//! base-0 spellings (`0x`/octal/leading blanks/leading `+`) are wired.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::primes::{
    BoundError, MAX_BOUND, NumVerdict, TABLE_LAST_PRIME, judge_number_line, over_cap, parse_bound,
    previous_prime, sieve,
};
use minix_sys::Errno;
use support::terminate;

/// C's `errx(1, …)` family: program name, phrase, no sentence suffix, exit 1 after
/// writing.
fn errx(prog: &str, phrase: &str) -> ! {
    support::warn(format!("{prog}: {phrase}\n").as_bytes());
    terminate(1)
}

/// C's `err(1, "%s", argv[i])`: the same shape, but suffixed with the number
/// `strtoumax` left behind. Only one thing can set errno in this command — `ERANGE`
/// (`_strtol.h:157`/`:172`). "Not a single digit converted" does not set one
/// (`_strtol.h:69`'s `EINVAL` is the invalid-base branch, unreachable because base
/// is always 0), so that shape lands on
/// `errx`'s "illegal numeric format" and not here. An earlier revision conflated
/// the two; this one separates them.
fn err_erange(prog: &str, word: &str) -> ! {
    support::warn_line(prog, word, Errno::from_i32(ERANGE));
    terminate(1)
}

/// C's `err(1, "stdin")`: the same shape, but the number comes from
/// `ferror(stdin)` rather than from parsing (`primes.c:187-190`). The only
/// difference from the helper above is where the number comes from; the report
/// shape matches C.
fn err_errno_code(prog: &str, label: &str, code: Errno) -> ! {
    support::warn_line(prog, label, code);
    terminate(1)
}

/// `ERANGE` from `minix3/sys/sys/errno.h` (value 34, "numerical result out of
/// range"). `Errno` has no associated constant for this name, so follow the
/// precedent of `rm.rs` for `ETXTBSY` and of this file for `MAX_BOUND`: define
/// the value on the shell side and convert.
const ERANGE: i32 = 34;

/// `primes.c:339-343`: a hard-coded one-line usage; no `getprogname()`.
fn usage() -> ! {
    support::warn(b"usage: primes [-dh] [start [stop]]\n");
    terminate(1)
}

/// Split one bound into C's three legs: negative sign, failed conversion,
/// trailing characters.
fn bound_of(prog: &str, word: &str) -> u64 {
    if word.starts_with('-') {
        // `primes.c:133`/`:155`: `argv[i][0] == '-'` is rejected first; strtoumax
        // never runs. Note that C scans options first, so only a hyphen that lands
        // among the operands takes this branch.
        errx(prog, "negative numbers aren't permitted.");
    }
    match parse_bound(word) {
        Ok(value) => value,
        // `primes.c:140`/`:147`/`:160`: `*p != '\0'` — trailing characters and
        // "not a single digit converted" both take this branch (the ground truth
        // leaves `endptr` in place). The trailing period is C's own.
        Err(BoundError::TrailingJunk) => errx(prog, &format!("{word}: illegal numeric format.")),
        // `primes.c:138`/`:145`: a sentence is suffixed only when `errno` was really
        // set (ERANGE).
        Err(BoundError::OutOfRange) => err_erange(prog, word),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C does not call `setprogname` either; `getprogname()` sets that piece up from
    // `argv[0]` (crt0-common.c:156-165), and `err`/`errx` both start with it.
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // `getopt(argc, argv, "dh")` at `primes.c:105-117`: `-d` and `-h` are both
    // counters (`dflag++`/`hflag++`), so `-d`, `-h`, `-dh`, `-hd`, `-dd` and
    // `-d -h` all count;
    // an unrecognized letter takes `case '?': default: usage()`. A bare `-` is an
    // operand, not an option, in getopt, so only strings with letters are
    // consumed here. Option permutation (C's getopt also pulls options found after
    // an operand back into the scan) is a separate registered gap and is not
    // expanded here.
    let mut dflag = false;
    let mut hflag = false;
    let mut cursor = 1usize;
    while let Some(word) = args.get(cursor) {
        let Some(letters) = word.strip_prefix('-').filter(|letters| !letters.is_empty()) else {
            break;
        };
        for letter in letters.bytes() {
            match letter {
                b'd' => dflag = true,
                b'h' => hflag = true,
                _ => usage(),
            }
        }
        cursor += 1;
    }
    let operands = &args[cursor..];

    if operands.len() > 2 {
        // `primes.c:167-168`: `default: usage()`.
        usage();
    }

    let (lo, hi) = match operands {
        [] => {
            // C's `case 0: start = read_num_buf();` (`primes.c:165-166`): read the
            // starting value from standard input line by line. All four leg predicates
            // live in `judge_number_line` on the decision side (pure-testable); this
            // branch only does the reading and writing.
            // Buffer size: C uses `buf[LINE_MAX]`, this tree uses 256 (the same
            // fixed-line convention as the other shells in the package); when one
            // line exceeds it this layer splits the read in two (unlike C's single
            // read; registered).
            let mut input = [0u8; 256];
            let mut reader = support::LineReader::new(support::stdin_chunk, &mut input);
            let start = loop {
                // The two endings are not the same in C: `fgets` returning NULL **and**
                // `ferror(stdin)` is an error, taking `err(1, "stdin")` (`:187-190`);
                // merely reaching the end is `exit(0)` with no output.
                let has_line = match reader.next_line() {
                    Ok(has_line) => has_line,
                    Err(code) => err_errno_code(prog, "stdin", code),
                };
                if !has_line {
                    terminate(0);
                }
                // The report fields below use the whole `buf` (including the trailing
                // newline), so C's two lines break into two lines here.
                let buf = core::str::from_utf8(reader.line()).unwrap_or("");
                match judge_number_line(buf) {
                    NumVerdict::SkipLine => continue,
                    NumVerdict::Negative => errx(prog, "negative numbers aren't permitted."),
                    NumVerdict::RangeError => err_erange(prog, buf),
                    NumVerdict::TrailingJunk(whole) => {
                        errx(prog, &format!("{whole}: illegal numeric format."))
                    }
                    NumVerdict::Start(value) => break value,
                }
            };
            if over_cap(start) {
                // C does not check the start value (it only checks `stop > SPSPMAX`,
                // `:149`); this tree's sieve cannot take an over-cap value, so it is
                // blocked here with the same report shape as the two operand branches —
                // the difference between this tree's cap and C's default upper bound is
                // declared.
                errx(
                    prog,
                    &format!("{start}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            (start as u32, MAX_BOUND)
        }
        [start] => {
            let lo = bound_of(prog, start);
            if over_cap(lo) {
                // C checks the cap only for `stop` (`:149`); a `start` over this tree's
                // cap can only be blocked here, because the sieve below cannot take
                // it — same report shape; the difference is registered in document 22.
                errx(
                    prog,
                    &format!("{start}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            (lo as u32, MAX_BOUND)
        }
        [start, stop] => {
            let lo = bound_of(prog, start);
            let hi = bound_of(prog, stop);
            if over_cap(hi) {
                // `primes.c:149`: only stop has this branch; the parentheses hold the
                // cap value.
                errx(
                    prog,
                    &format!("{stop}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            if over_cap(lo) {
                errx(
                    prog,
                    &format!("{start}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            if lo > hi {
                // `primes.c:171-172`: C compares the **original** two values, so
                // `lo == hi` is legal (no error, just no output — the decrement below
                // turns it into an empty interval).
                errx(prog, "start value must be less than stop value.");
            }
            // C's upper bound is exclusive (the header comment at `:49` plus the
            // measured reference process) while `sieve`'s is inclusive, so decrement
            // by 1 here; with `stop == 0` it saturates at 0, which together with
            // `lo > hi` yields empty output.
            (lo as u32, (hi as u32).saturating_sub(1))
        }
        // Unreachable: operands beyond the three arms were already `usage()`'d above
        // (the compiler needs this arm to consider the match complete).
        _ => usage(),
    };
    // Largest prime count below MAX_BOUND is pi(1_000_000) = 78,498.
    let mut found = vec![0u32; 78_498];
    let count = match sieve(lo, hi, &mut found) {
        Ok(count) => count,
        Err(_) => terminate(1),
    };
    // C's two printing paths answer "does the first prime have a predecessor"
    // differently (shape and measurements in `TABLE_LAST_PRIME`): when the start
    // falls inside the table range the difference column prints unconditionally
    // (`primes.c:256-258`), while the off-table sieve path keeps only the
    // `prev != 0` guard, so the first prime has no column (`:328`).
    let uses_table = (lo as u64) <= TABLE_LAST_PRIME as u64;
    let mut printed: Option<u32> = None;
    let mut out = [0u8; 12];
    for &prime in &found[..count] {
        // `-h` changes only the **prime's own** base; the difference column in C is
        // always decimal `PRIu64` (`:257`/`:329`). The reference process shows the
        // distinction: `primes -dh 113 130` prints `71 (4)` and `7f (14)` — the
        // primes are hexadecimal (113→`71`, 127→`7f`), while the difference prints
        // `14`, not `e`.
        let len = if hflag {
            support::utox(prime, &mut out)
        } else {
            support::utoa(prime, &mut out)
        };
        support::emit(&out[..len]);
        if dflag {
            let difference = match printed {
                Some(before) => Some(prime - before),
                None if uses_table => Some(prime - previous_prime(prime)),
                None => None,
            };
            if let Some(difference) = difference {
                support::emit(b" (");
                let dlen = support::utoa(difference, &mut out);
                support::emit(&out[..dlen]);
                support::emit(b")");
            }
        }
        support::emit(b"\n");
        printed = Some(prime);
    }
    terminate(0);
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
