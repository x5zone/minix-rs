//! Minix-RS cal — the doing half over `minix_doctools::cal`.
//!
//! Ground truth: `minix3/usr.bin/cal/cal.c` (`main` 的参数解析与
//! `render_month`/`render_year` 的年版式）。决定半（栅格与渲染）在库内；
//! 本程序只翻命令行、取当前时刻、把渲染结果写到标准输出。
//!
//! 声明性留白（10-doc-man-tools.md §5）：`-j`（儒略日）、`-3`/`-h`/
//! `-r`/`-A`/`-B`/`-C`/`-d`/`-R` 等面回答 "option not wired"；月份只收
//! 数字（C 的月名解析面未移植）。

use minix_doctools::cal::{render_month, render_year, Reckoning};
use minix_sys::write;

const STDOUT: i32 = 1;
const STDERR: i32 = 2;

fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

fn fail(message: &str) -> ! {
    let _ = write(STDERR, message.as_bytes());
    let _ = write(STDERR, b"\n");
    terminate(1)
}

/// 宿主取当前 UTC 时刻（`cal` 无参时的当前月）；no_std 目标换时钟系统
/// 调用，接缝与 argv 同批切换。
fn now_epoch_days_civil() -> (i32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
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

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut operands: Vec<&str> = Vec::new();
    for arg in &args[1..] {
        if let Some(rest) = arg.strip_prefix('-') {
            if rest.is_empty() {
                fail("option not wired");
            }
            for c in rest.chars() {
                match c {
                    'y' => {}
                    // 其余 C 旗标（-3hjrA B C d R）逐个留白。
                    _ => fail("option not wired"),
                }
            }
        } else {
            operands.push(arg);
        }
    }

    enum View {
        Month(i32, u32),
        Year(i32),
    }
    let view = match operands.as_slice() {
        [] => {
            let (y, m) = now_epoch_days_civil();
            View::Month(y, m)
        }
        [one] => {
            let y = match (*one).parse::<i32>() {
                Ok(y @ 1..=9999) => y,
                _ => fail("bad number"),
            };
            View::Year(y)
        }
        [month, year] => {
            let m: u32 = match (*month).parse() {
                Ok(v @ 1..=12) => v,
                _ => fail("bad month"),
            };
            let y = match (*year).parse::<i32>() {
                Ok(y @ 1..=9999) => y,
                _ => fail("bad number"),
            };
            View::Month(y, m)
        }
        _ => fail("usage: cal [-yj] [[month] year]"),
    };

    let mut out = [0u8; 4096];
    let rendered = match view {
        View::Month(y, m) => render_month(y, m, Reckoning::Gregorian, &mut out),
        View::Year(y) => render_year(y, Reckoning::Gregorian, &mut out),
    };
    match rendered {
        Ok(used) => {
            let _ = write(STDOUT, &out[..used]);
        }
        Err(_) => fail("render buffer too small"),
    }
    terminate(0);
}
