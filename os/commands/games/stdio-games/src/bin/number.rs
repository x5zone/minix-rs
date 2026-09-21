//! Minix-RS number — numbers as English words.
//!
//! Ground truth: `minix3/games/number/number.c`: stdin lines in, each
//! number spoken in English words on its own line. The library covers zero
//! through 999,999 (`number_words` reports larger values as out of range,
//! matching the range the words tables model); malformed input is a
//! failure, exiting 1.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::words::number_words;
use minix_sys::read;
use support::LineReader;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.len() > 1 {
        support::emit(b"usage: number\n");
        support::terminate(1);
    }
    let mut input = [0u8; 64];
    let mut out = [0u8; 512];
    let mut reader = LineReader::new(
        |chunk| read(support::STDIN, chunk).map_err(|_| ()),
        &mut input,
    );
    while reader.next_line().unwrap_or(false) {
        let line = reader.line();
        let trimmed = match line.last() {
            Some(b'\n') => &line[..line.len() - 1],
            _ => line,
        };
        let word = core::str::from_utf8(trimmed).unwrap_or("");
        match word.parse::<u32>() {
            Ok(value) => match number_words(value, &mut out) {
                Ok(len) => {
                    support::emit(&out[..len]);
                    support::emit(b"\n");
                }
                Err(_) => support::terminate(1),
            },
            Err(_) => support::terminate(1),
        }
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
