//! Minix-RS cal — the doing half over `minix_doctools::cal`.
//!
//! Ground truth: `minix3/usr.bin/cal/cal.c` (`main` 的参数解析与
//! `render_month`/`render_year` 的年版式）。决定半（栅格与渲染）在库内；
//! 本程序只翻命令行、取当前时刻、把渲染结果写到标准输出。
//!
//! 声明性留白（10-doc-man-tools.md §5）：`-3`/`-h`/`-r`/`-A`/`-B`/`-C`/
//! `-d`/`-R` 等面回答 "option not wired"；月份只收数字（C 的月名解析面未
//! 移植）。`-y`（强制年视图）与 `-j`（年积日）已接。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_doctools::cal::{DayStyle, Reckoning, render_month, render_year};
use minix_sys::write;

const STDOUT: i32 = 1;
const STDERR: i32 = 2;

fn fail(message: &str) -> ! {
    let _ = write(STDERR, message.as_bytes());
    let _ = write(STDERR, b"\n");
    support::terminate(1)
}

/// 当前 UTC 时刻（`cal` 无参时的当前月）：宿主读宿主钟，目标读内核
/// `SYS_TIMES` 面（`support::epoch_micros` 双形）。
fn now_epoch_days_civil() -> (i32, u32) {
    let secs = (support::epoch_micros().unwrap_or(0) / 1_000_000) as i64;
    let days = secs.div_euclid(86_400);
    // Hinnant civil_from_days（与 12 篇 stamp.rs 同式）。
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
    Year(i32),
}

/// Decide the face from operands, the `-y` force-year flag, and the current
/// `(year, month)`. Mirrors `cal.c` argc switch (`cal.c:283-322`): `-y` only
/// changes the no-operand case (`cal.c:312` `if (!yflag) month = ...`, else
/// yearly); explicit operands already fix the face.
fn choose_view(operands: &[&str], force_year: bool, now: (i32, u32)) -> Result<View, &'static str> {
    match operands {
        [] => Ok(if force_year {
            View::Year(now.0)
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
                Ok(y @ 1..=9999) => Ok(View::Month(y, m)),
                _ => Err("bad number"),
            }
        }
        _ => Err("usage: cal [-yj] [[month] year]"),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut operands: Vec<&str> = Vec::new();
    let mut style = DayStyle::Month;
    let mut force_year = false;
    for arg in &args[1..] {
        if let Some(rest) = arg.strip_prefix('-') {
            if rest.is_empty() {
                fail("option not wired");
            }
            for c in rest.chars() {
                match c {
                    'y' => force_year = true,
                    'j' => style = DayStyle::Julian,
                    // 其余 C 旗标（-3h rA B C d R）逐个留白。
                    _ => fail("option not wired"),
                }
            }
        } else {
            operands.push(arg);
        }
    }

    let view = match choose_view(&operands, force_year, now_epoch_days_civil()) {
        Ok(v) => v,
        Err(msg) => fail(msg),
    };

    let mut out = [0u8; 4096];
    let rendered = match view {
        View::Month(y, m) => render_month(y, m, Reckoning::Gregorian, style, &mut out),
        View::Year(y) => render_year(y, Reckoning::Gregorian, style, &mut out),
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
            choose_view(&[], false, (2026, 10)),
            Ok(View::Month(2026, 10))
        );
    }

    #[test]
    fn no_operand_with_y_is_current_year() {
        // 旧体把 `-y` 当静默 no-op，仍走月视图——这行钉住与 C 一致：无操作数时 -y 拉年。
        assert_eq!(choose_view(&[], true, (2026, 10)), Ok(View::Year(2026)));
    }

    #[test]
    fn single_operand_is_year_regardless_of_y() {
        assert_eq!(
            choose_view(&["2025"], true, (2026, 10)),
            Ok(View::Year(2025))
        );
    }

    #[test]
    fn two_operands_is_month() {
        assert_eq!(
            choose_view(&["9", "2025"], false, (2026, 10)),
            Ok(View::Month(2025, 9))
        );
    }

    #[test]
    fn bad_values_and_overload_rejected() {
        assert_eq!(choose_view(&["0"], false, (2026, 10)), Err("bad number"));
        assert_eq!(
            choose_view(&["13", "2025"], false, (2026, 10)),
            Err("bad month")
        );
        assert_eq!(
            choose_view(&["9", "2025", "x"], false, (2026, 10)),
            Err("usage: cal [-yj] [[month] year]")
        );
    }
}
