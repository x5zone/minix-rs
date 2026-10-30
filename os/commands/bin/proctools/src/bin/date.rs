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
//! Declared residual: the *set-the-time* path (`date operand` needs
//! `settimeofday`/`adjtime` plus wtmp/syslog accounting, `date.c:337-357`)
//! has no client wrapper yet, so an operand that would mutate the clock
//! reports an error and exits 1 rather than pretending to set the time.
//! Tracked in 12-process-tools.md §5 and the real-machine half of the
//! command face in the coordination ledger.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_proctools::dates::{breakdown, format};
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
    arg.len() > 1
        && arg.as_bytes()[0] == b'-'
        && arg.as_bytes()[1..].iter().all(u8::is_ascii_digit)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut given_epoch: Option<i64> = None;
    let mut format_operand: Option<&str> = None;
    let mut set_operand: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        // A flag run begins with `-` but is not a negative number and not a
        // lone `-`.
        let is_option =
            arg.len() > 1 && arg.as_bytes()[0] == b'-' && !is_negative_number(arg);
        if is_option {
            let mut chars = arg[1..].chars();
            while let Some(c) = chars.next() {
                match c {
                    'u' | 'j' | 'n' | 'a' => {}
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

    // A set operand mutates the clock — needs settimeofday (no wrapper yet).
    if set_operand.is_some() {
        fail("date: setting the clock is not wired (settimeofday pending)");
    }

    let epoch = match given_epoch {
        Some(v) => v,
        None => minix_sys::gettimeofday().map(|(secs, _)| secs).unwrap_or(0),
    };
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
