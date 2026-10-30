//! Minix-RS cal — the doing half over `minix_doctools::cal`.
//!
//! Ground truth: `minix3/usr.bin/cal/cal.c` (`main`'s argument parsing and the
//! year layout of `render_month`/`render_year`).  The deciding half (grid and
//! rendering) lives in the library; this program only translates the command
//! line, reads the current time, and writes the rendered result to standard
//! output.
//!
//! Declared gaps (10-doc-man-tools.md §5): `-r`/`-h`/`-A`/`-B`/`-C`/`-R` and
//! similar faces answer "option not wired"; months are accepted as numbers only
//! (C's month-name parsing was not ported, and C itself only accepts numbers
//! too).  `-y` (force the year view), `-j` (day of year), `-3` (previous/current/
//! next month triple) and `-d` (week start day 0..6) are wired.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_doctools::cal::{DayStyle, Reckoning, render_month, render_triple, render_year};
use minix_sys::write;

const STDOUT: i32 = 1;
const STDERR: i32 = 2;

fn fail(message: &str) -> ! {
    let _ = write(STDERR, message.as_bytes());
    let _ = write(STDERR, b"\n");
    support::terminate(1)
}

/// The current UTC instant (the current month when `cal` has no operands): on the
/// host it reads the host clock, on the target the kernel `SYS_TIMES` face
/// (`support::epoch_micros` has both forms).
fn now_epoch_days_civil() -> (i32, u32) {
    let secs = (support::epoch_micros().unwrap_or(0) / 1_000_000) as i64;
    let days = secs.div_euclid(86_400);
    // Hinnant civil_from_days (the same formula as the library's stamp.rs).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    if m <= 2 {
        y += 1;
    }
    (y as i32, m)
}

/// Which calendar face to print. Extracted as a pure function (with `now`
/// injected) so the `-y` semantics are host-testable without a live clock.
#[derive(Debug, PartialEq, Eq)]
enum View {
    Month(i32, u32),
    Triple(i32, u32),
    Year(i32),
}

/// Decide the face from operands, the `-y`/`-3` flags, and the current
/// `(year, month)`. Mirrors `cal.c` argc switch + `monthrange` gating
/// (`cal.c:283-322`, `cal.c:340` before/after): `-y` and `-3` only reshape
/// the month-based faces; an explicit year-only operand is yearly regardless
/// (C resets `before`/`after` when `month == 0`), and `-y` outranks `-3` on
/// the no-operand path.
fn choose_view(
    operands: &[&str],
    force_year: bool,
    force_triple: bool,
    now: (i32, u32),
) -> Result<View, &'static str> {
    match operands {
        [] => Ok(if force_year {
            View::Year(now.0)
        } else if force_triple {
            View::Triple(now.0, now.1)
        } else {
            View::Month(now.0, now.1)
        }),
        [one] => match (*one).parse::<i32>() {
            Ok(y @ 1..=9999) => Ok(View::Year(y)),
            _ => Err("bad number"),
        },
        [month, year] => {
            let m: u32 = match (*month).parse() {
                Ok(v @ 1..=12) => v,
                _ => return Err("bad month"),
            };
            match (*year).parse::<i32>() {
                Ok(y @ 1..=9999) => Ok(if force_triple {
                    View::Triple(y, m)
                } else {
                    View::Month(y, m)
                }),
                _ => Err("bad number"),
            }
        }
        _ => Err("usage: cal [-3jy] [[month] year]"),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut operands: Vec<&str> = Vec::new();
    let mut style = DayStyle::Month;
    let mut force_year = false;
    let mut force_triple = false;
    let mut week_start: usize = 0;
    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        if arg.len() > 1 && arg.starts_with('-') {
            let rest: Vec<char> = arg.chars().skip(1).collect();
            let mut k = 0usize;
            while k < rest.len() {
                match rest[k] {
                    'y' => force_year = true,
                    'j' => style = DayStyle::Julian,
                    '3' => force_triple = true,
                    // -d value (0..=6): attached to this token (-d1) or taken from
                    // the next word (-d 1), matching C getopt("...d:...", cal.c:247)
                    // and getnum.
                    'd' => {
                        k += 1;
                        let raw: String = if k < rest.len() {
                            rest[k..].iter().collect()
                        } else {
                            i += 1;
                            args.get(i).copied().unwrap_or("").into()
                        };
                        match raw.parse::<i32>() {
                            Ok(v @ 0..=6) => week_start = v as usize,
                            _ => fail("illegal day of week value: use 0-6"),
                        }
                        k = rest.len(); // -d consumes its value and ends this token scan
                    }
                    // The remaining C flags (-h r A B C R) each stay unwired.
                    _ => fail("option not wired"),
                }
                k += 1;
            }
        } else {
            operands.push(arg);
        }
        i += 1;
    }

    let view = match choose_view(&operands, force_year, force_triple, now_epoch_days_civil()) {
        Ok(v) => v,
        Err(msg) => fail(msg),
    };

    let mut out = [0u8; 4096];
    let rendered = match view {
        View::Month(y, m) => render_month(y, m, Reckoning::Gregorian, style, week_start, &mut out),
        View::Triple(y, m) => {
            render_triple(y, m, Reckoning::Gregorian, style, week_start, &mut out)
        }
        View::Year(y) => render_year(y, Reckoning::Gregorian, style, week_start, &mut out),
    };
    match rendered {
        Ok(used) => {
            let _ = write(STDOUT, &out[..used]);
        }
        Err(_) => fail("render buffer too small"),
    }
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

#[cfg(test)]
mod view_tests {
    use super::*;

    #[test]
    fn no_operand_defaults_to_current_month() {
        assert_eq!(
            choose_view(&[], false, false, (2026, 10)),
            Ok(View::Month(2026, 10))
        );
    }

    #[test]
    fn no_operand_with_y_is_current_year() {
        // The old body treated `-y` as a silent no-op and still used the month
        // view; this pins the C behaviour: with no operands, -y pulls the year.
        assert_eq!(
            choose_view(&[], true, false, (2026, 10)),
            Ok(View::Year(2026))
        );
    }

    #[test]
    fn single_operand_is_year_regardless_of_y() {
        assert_eq!(
            choose_view(&["2025"], true, false, (2026, 10)),
            Ok(View::Year(2025))
        );
    }

    #[test]
    fn two_operands_is_month() {
        assert_eq!(
            choose_view(&["9", "2025"], false, false, (2026, 10)),
            Ok(View::Month(2025, 9))
        );
    }

    #[test]
    fn minus_three_reshapes_month_faces_to_triple() {
        // No operands + -3 -> the current month triple; two operands + -3 -> the triple for that month.
        assert_eq!(
            choose_view(&[], false, true, (2026, 10)),
            Ok(View::Triple(2026, 10))
        );
        assert_eq!(
            choose_view(&["9", "2025"], false, true, (2026, 10)),
            Ok(View::Triple(2025, 9))
        );
    }

    #[test]
    fn minus_three_ignored_on_year_only_and_y_outranks() {
        // A single operand (a year) is unaffected by -3 (C goes back to yearly when
        // month==0); -y outranks -3.
        assert_eq!(
            choose_view(&["2025"], false, true, (2026, 10)),
            Ok(View::Year(2025))
        );
        assert_eq!(
            choose_view(&[], true, true, (2026, 10)),
            Ok(View::Year(2026))
        );
    }

    #[test]
    fn bad_values_and_overload_rejected() {
        assert_eq!(
            choose_view(&["0"], false, false, (2026, 10)),
            Err("bad number")
        );
        assert_eq!(
            choose_view(&["13", "2025"], false, false, (2026, 10)),
            Err("bad month")
        );
        assert_eq!(
            choose_view(&["9", "2025", "x"], false, false, (2026, 10)),
            Err("usage: cal [-3jy] [[month] year]")
        );
    }
}
