//! Minix-RS factor — integer factorisation.
//!
//! Ground truth: `minix3/games/factor/factor.c`, usage `factor [value ...]`
//! (line 268): each argument is factored as `value: factor factor ...` (one
//! output line per value, the shape built by `render_factors`); with no
//! arguments every stdin line carries one value. A malformed value is a
//! failure (the C path is `errx`), exiting 1.

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::factor::{parse_value, render_factors, trial_divide};
use minix_sys::read;
use support::LineReader;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() > 1 {
        for arg in &argv[1..] {
            factor_word(arg);
        }
    } else {
        let mut input = [0u8; 80];
        let mut reader = LineReader::new(
            |chunk| read(support::STDIN, chunk).map_err(|_| ()),
            &mut input,
        );
        while reader.next_line().unwrap_or(false) {
            let line = reader.line();
            let trimmed: &[u8] = match line.last() {
                Some(b'\n') => &line[..line.len() - 1],
                _ => line,
            };
            if trimmed.is_empty() {
                continue;
            }
            let word = std::str::from_utf8(trimmed).unwrap_or("");
            factor_word(word);
        }
    }
    support::terminate(0);
}

fn factor_word(word: &str) {
    let value = match parse_value(word) {
        Ok(value) => value,
        Err(_) => support::terminate(1),
    };
    let mut factors = [0u64; 64];
    let count = match trial_divide(value, &mut factors) {
        Ok(count) => count,
        Err(_) => support::terminate(1),
    };
    let mut out = [0u8; 256];
    match render_factors(value, &factors[..count], &mut out) {
        Ok(len) => {
            support::emit(&out[..len]);
            support::emit(b"\n");
        }
        Err(_) => support::terminate(1),
    }
}
