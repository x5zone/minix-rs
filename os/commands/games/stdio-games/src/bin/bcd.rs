//! Minix-RS bcd — punched-card display.
//!
//! Ground truth: `minix3/games/bcd/bcd.c` (NetBSD 1.17), `main` at `:129-149`.
//! Each argument becomes one card (`:139-141`); with no arguments the input is read
//! with `fgets(cardline, sizeof cardline, stdin)` where `cardline` is `char [80]`
//! (`:132`, `:142-144`) — and `fgets` reserves one byte for the terminator, so a
//! physical line longer than **79** characters arrives as a 79-character chunk followed
//! by the remainder, each chunk rendered as its own card. There is no option parsing at
//! all: `bcd -x` renders `-x` as text (`:139` tests only the count). Exit status 0.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::bcd::render_card;
use support::LineReader;

/// Usable bytes of C's `char cardline[80]` under `fgets` (`bcd.c:132,142`).
const CARDLINE: usize = 79;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.len() > 1 {
        for arg in &argv[1..] {
            card(arg.as_bytes());
        }
    } else {
        // C's `cardline[80]` fed to `fgets` hands out at most 79 characters per call,
        // so the reader's line capacity is 79 — 80 here would move one character from the
        // start of the second card to the end of the first.
        let mut input = [0u8; CARDLINE];
        let mut reader = LineReader::new(support::stdin_chunk, &mut input);
        // Read failure ends the stream the way a NULL from C `fgets` does.
        while reader.next_line().unwrap_or(false) {
            card(reader.line());
        }
    }
    support::terminate(0);
}

fn card(text: &[u8]) {
    let mut out = [0u8; 800];
    match render_card(text, &mut out) {
        Ok(len) => support::emit(&out[..len]),
        Err(_) => support::terminate(1),
    }
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
