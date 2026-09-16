//! Minix-RS pig — pig latin filter.
//!
//! Ground truth: `minix3/games/pig/pig.c`, usage `pig` (line 133): stdin
//! lines in, the same text out with every word rotated into pig latin
//! (vowel rule at line 103). Word and separator runs are preserved as they
//! appear; an unreadable (non-UTF-8) word passes through untouched.

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::words::pig_word;
use minix_sys::read;
use support::LineReader;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() > 1 {
        support::emit(b"usage: pig\n");
        support::terminate(1);
    }
    let mut input = [0u8; 1024];
    let mut out = [0u8; 1024];
    let mut reader = LineReader::new(
        |chunk| read(support::STDIN, chunk).map_err(|_| ()),
        &mut input,
    );
    while reader.next_line().unwrap_or(false) {
        pig_line(reader.line(), &mut out);
    }
    support::terminate(0);
}

fn pig_line(line: &[u8], out: &mut [u8]) {
    let mut cursor = 0usize;
    while cursor < line.len() {
        let byte = line[cursor];
        if byte.is_ascii_alphanumeric() {
            let start = cursor;
            while cursor < line.len() && line[cursor].is_ascii_alphanumeric() {
                cursor += 1;
            }
            let word = &line[start..cursor];
            match std::str::from_utf8(word) {
                Ok(word) => match pig_word(word, out) {
                    Ok(len) => support::emit(&out[..len]),
                    Err(_) => support::emit(word.as_bytes()),
                },
                Err(_) => support::emit(word),
            }
        } else {
            support::emit(&[byte]);
            cursor += 1;
        }
    }
}
