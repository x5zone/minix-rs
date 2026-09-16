//! Minix-RS primes — prime generation over an interval.
//!
//! Ground truth: `minix3/games/primes/primes.c`, `primes [start [stop]]`.
//! The library caps the interval at `MAX_BOUND` (1,000,000 — the trial
//! division engine is built for short lived runs, not the C streaming
//! sieve), so with no arguments the interval is 2 to the cap instead of the
//! C default near 2^32; one argument starts there, two bound it. Output is
//! one prime per line.

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::primes::{parse_bound, sieve, MAX_BOUND};
use support::terminate;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let (lo, hi) = match argv.len() {
        1 => (2, MAX_BOUND),
        2 => bound(&argv[1], MAX_BOUND),
        3 => {
            let lo = match parse_bound(&argv[1]) {
                Ok(value) => value,
                Err(_) => terminate(1),
            };
            bound(&argv[2], lo)
        }
        _ => terminate(1),
    };
    // Largest prime count below MAX_BOUND is pi(1_000_000) = 78,498.
    let mut found = vec![0u32; 78_498];
    let count = match sieve(lo, hi, &mut found) {
        Ok(count) => count,
        Err(_) => terminate(1),
    };
    let mut out = [0u8; 12];
    for &prime in &found[..count] {
        let len = support::utoa(prime, &mut out);
        support::emit(&out[..len]);
        support::emit(b"\n");
    }
    terminate(0);
}

fn bound(word: &str, default_hi: u32) -> (u32, u32) {
    let lo = match parse_bound(word) {
        Ok(value) => value,
        Err(_) => terminate(1),
    };
    (lo, default_hi)
}
