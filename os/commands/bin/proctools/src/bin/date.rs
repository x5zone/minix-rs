//! Minix-RS `date` — the doing half over `minix_proctools::dates`.
//!
//! Ground truth: `minix3/bin/date/date.c`. Without a set operand `date`
//! reads the clock (`time`, here `minix_sys::gettimeofday`), breaks it down,
//! and prints `strftime` of `+%a %b %e %H:%M:%S %Z %Y` (the leading `+` is
//! the format-introducer convention, `date.c:142-146`, dropped from the
//! output at `date.c:166`).
//!
//! Wired here: default print, `-u` (whole thing in UTC — this layer is UTC
//! regardless, since there is no tz database, so it is a faithful no-op),
//! `-j` (interpret without setting: accepted; with no set operand it is a
//! no-op), `-r SECONDS` (report a given epoch second instead of the clock),
//! and a `+FORMAT` operand. `-d DATE` declines exactly as Minix's own
//! `__minix` build does (`date.c:100-103` always errors on it).
//!
//! Wired: the *set-the-time* path — a canonical operand
//! (`[[[[[[CC]yy]mm]dd]HH]MM[.SS]`, C `setthetime`, `date.c:185-331`)
//! composes over the current fields and calls `clock_settime(CLOCK_REALTIME)`
//! (the PM absolute-set call, `clock_settime.c:11-27`); `-j` interprets the
//! operand without setting. `-a` adjusts gradually through the same PM
//! call's gradual mode (`adjtime`, `date.c:346-350`); the wtmp/syslog
//! accounting lines stay a declared residual (no wtmp table in this tree).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_proctools::dates::{BadCanonicalTime, breakdown, format, set_the_time};
use minix_sys::write;

const STDERR: i32 = 2;
const DEFAULT_FMT: &str = "%a %b %e %H:%M:%S %Z %Y";

fn fail(message: &str) -> ! {
    let _ = write(STDERR, message.as_bytes());
    let _ = write(STDERR, b"\n");
    support::terminate(1)
}

/// True for a bare negative integer operand (`-123`), which is a value, not
/// a flag cluster. A lone `-` is neither (treated as an operand by C).
fn is_negative_number(arg: &str) -> bool {
    arg.len() > 1 && arg.as_bytes()[0] == b'-' && arg.as_bytes()[1..].iter().all(u8::is_ascii_digit)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut given_epoch: Option<i64> = None;
    let mut jflag = false;
    let mut aflag = false;
    let mut format_operand: Option<&str> = None;
    let mut set_operand: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        // A flag run begins with `-` but is not a negative number and not a
        // lone `-`.
        let is_option = arg.len() > 1 && arg.as_bytes()[0] == b'-' && !is_negative_number(arg);
        if is_option {
            let mut chars = arg[1..].chars();
            while let Some(c) = chars.next() {
                match c {
                    'u' | 'n' => {}
                    'j' => jflag = true,
                    'a' => aflag = true,
                    'r' => {
                        // `-r` takes the remaining cluster text or the next
                        // token as its value (getopt's `-rVAL` / `-r VAL`).
                        let tail: String = chars.as_str().to_string();
                        let word = if tail.is_empty() {
                            i += 1;
                            args.get(i).copied().unwrap_or("")
                        } else {
                            &tail
                        };
                        given_epoch = Some(match word.parse::<i64>() {
                            Ok(v) => v,
                            Err(_) => fail("date: -r needs an integer"),
                        });
                        chars = "".chars();
                    }
                    'd' => fail("date: Unrecognized date format"),
                    _ => fail("date: usage: [-ajnu] [-d val] [-r sec] [+format]"),
                }
            }
            i += 1;
            continue;
        }
        // Operands, in C's order: `+format` first, then the set value.
        if arg.starts_with('+') && format_operand.is_none() {
            format_operand = Some(&arg[1..]);
        } else if set_operand.is_none() {
            set_operand = Some(arg);
        }
        i += 1;
    }

    let epoch = match given_epoch {
        Some(v) => v,
        None => minix_sys::gettimeofday().map(|(secs, _)| secs).unwrap_or(0),
    };

    if let Some(operand) = set_operand {
        // C's `setthetime` composes over `localtime(&tval)` — the displayed
        // time source, which `-r` also steers (`date.c:206`).
        let new_time = match set_the_time(operand.as_bytes(), epoch) {
            Ok(v) => v,
            Err(BadCanonicalTime { message, caret }) => {
                // C's `badcanotime` (`date.c:174-182`): sentence, operand,
                // caret column, then usage — all to stderr, exit 1.
                let mut out: Vec<u8> = Vec::new();
                out.extend_from_slice(b"date: ");
                out.extend_from_slice(message.as_bytes());
                out.extend_from_slice(b" in canonical time\n");
                out.extend_from_slice(operand.as_bytes());
                out.push(b'\n');
                out.resize(out.len() + caret, b' ');
                out.push(b'^');
                out.push(b'\n');
                out.extend_from_slice(
                    b"Usage: date [-ajnu] [-d date] [-r seconds] [+format] [[[[[[CC]yy]mm]dd]HH]MM[.SS]]\n",
                );
                // The hosted stderr twin — the raw `write` leg is the
                // none-target face only.
                support::warn(&out);
                support::terminate(1);
            }
        };
        if jflag {
            // `-j`: interpret without setting (`date.c:330-333`).
            display(new_time, format_operand);
        }
        if aflag {
            // C's `-a` leg (`date.c:346-350`): the delta between the new
            // time and the current one goes through `adjtime`, and the
            // displayed time stays the unadjusted one — the correction
            // happens gradually in the manager.
            let delta = new_time - epoch;
            if let Err(code) = minix_sys::clock_adjust(delta, 0) {
                // C's `err(EXIT_FAILURE, "adjtime")` (`date.c:350`).
                let mut out: Vec<u8> = b"date: adjtime: ".to_vec();
                out.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
                out.push(b'\n');
                support::warn(&out);
                support::terminate(1);
            }
            display(epoch, format_operand);
        }
        // The absolute set: `clock_settime(CLOCK_REALTIME, {new_time, 0})`
        // (`date.c:351-354`; the PM call is the same one `settimeofday`
        // makes). The failure sentence is C's `err(EXIT_FAILURE,
        // "settimeofday")` (`date.c:355`).
        if let Err(code) = minix_sys::clock_settime(0, new_time as u64, 0) {
            let mut out: Vec<u8> = b"date: settimeofday: ".to_vec();
            out.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
            out.push(b'\n');
            support::warn(&out);
            support::terminate(1);
        }
        display(new_time, format_operand);
    }
    display(epoch, format_operand);
}

/// Break `epoch` down, format it, and print it — the tail C runs after the
/// set path has updated `tval` (`date.c:157-170`).
fn display(epoch: i64, format_operand: Option<&str>) -> ! {
    let fmt = format_operand.unwrap_or(DEFAULT_FMT);

    let time = breakdown(epoch);
    let mut buf: Vec<u8> = alloc::vec![0u8; 256];
    let used = match format(fmt, &time, &mut buf) {
        Ok(used) => used,
        Err(_) => fail("date: format too large"),
    };
    let mut line = buf[..used].to_vec();
    line.push(b'\n');
    support::emit(&line);
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
