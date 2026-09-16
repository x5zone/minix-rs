//! Minix-RS bcd — punched-card display.
//!
//! Ground truth: `minix3/games/bcd/bcd.c` (NetBSD 1.17), main at lines 129
//! to 149: each argument is rendered as a card, or with no arguments every
//! stdin line (`cardline[80]`, line 132) is rendered; exit 0. The card
//! layout is the library's `render_card`.

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::bcd::render_card;
use minix_sys::read;
use support::LineReader;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() > 1 {
        for arg in &argv[1..] {
            card(arg.as_bytes());
        }
    } else {
        let mut input = [0u8; 80];
        let mut reader = LineReader::new(
            |chunk| read(support::STDIN, chunk).map_err(|_| ()),
            &mut input,
        );
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
