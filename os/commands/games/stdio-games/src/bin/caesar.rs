//! Minix-RS caesar — rotation cipher filter.
//!
//! Ground truth: `minix3/games/caesar/caesar.c`, main at lines 190 to 204:
//! `caesar [rotation]` decrypts stdin with the given rotation (one line at
//! a time through the rotation table); a bad rotation value is a failure
//! (`get_rotation`, lines 125 to 137, exits). The argument-free mode runs
//! `guess_and_rotate` — the frequency-analysis rotation cracker — whose
//! deciding half is not yet in the library, so this build reports the C
//! usage and exits 1 for that mode until it lands.

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::caesar::{parse_rotation, rotate_line};
use minix_sys::read;
use support::LineReader;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let rotation = match argv.len() {
        2 => match parse_rotation(&argv[1]) {
            Ok(rotation) => rotation,
            Err(_) => support::terminate(1),
        },
        _ => {
            support::emit(b"usage: caesar [rotation]\n");
            support::terminate(1);
        }
    };
    let mut input = [0u8; 1024];
    let mut out = [0u8; 1024];
    let mut reader = LineReader::new(
        |chunk| read(support::STDIN, chunk).map_err(|_| ()),
        &mut input,
    );
    while reader.next_line().unwrap_or(false) {
        let length = reader.line().len();
        match rotate_line(reader.line(), rotation, &mut out[..length]) {
            Ok(()) => support::emit(&out[..length]),
            Err(_) => support::terminate(1),
        }
    }
    support::terminate(0);
}
