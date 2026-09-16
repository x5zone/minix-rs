//! Minix-RS morse — international morse code encoder and decoder.
//!
//! Ground truth: `minix3/games/morse/morse.c`, usage
//! `morse [-ds] [string ...]` (line 140). Encode (default): each argument,
//! or each stdin line, becomes its code groups on one line (blanks between
//! words become `/` through `encode_word`). Decode (`-d`): each blank
//! separated group resolves to one letter. Groups with no code are skipped
//! (the C decoder stays silent on them); the `-s` table dump of the C
//! program is not modelled yet and reports the usage.

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::morse::{decode_group, encode_word, StaticMorse};
use minix_sys::read;
use support::LineReader;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut decode = false;
    let mut first = 1;
    // BSD getopt stops at the first non-option argument.
    while first < argv.len() && argv[first].starts_with('-') && argv[first].len() > 1 {
        match argv[first].as_str() {
            "-d" => decode = true,
            _ => usage(),
        }
        first += 1;
    }
    let rest: Vec<&str> = argv[first..].iter().map(String::as_str).collect();
    if decode {
        let mut buffer = [0u8; 512];
        for line in text_lines(&rest, &mut buffer) {
            for group in line.split_whitespace() {
                if let Ok(letter) = decode_group(&StaticMorse, group) {
                    let mut out = [0u8; 4];
                    let len = letter.len_utf8();
                    letter.encode_utf8(&mut out);
                    support::emit(&out[..len]);
                }
            }
            support::emit(b"\n");
        }
    } else {
        let mut buffer = [0u8; 512];
        for line in text_lines(&rest, &mut buffer) {
            let mut out = [0u8; 1024];
            match encode_word(&StaticMorse, line.as_str(), &mut out) {
                Ok(len) => {
                    support::emit(&out[..len]);
                    support::emit(b"\n");
                }
                // Unknown characters stay silent, then the line ends.
                Err(_) => support::emit(b"\n"),
            }
        }
    }
    support::terminate(0);
}

/// The strings when given, otherwise stdin split into lines.
fn text_lines(strings: &[&str], input: &mut [u8]) -> Vec<String> {
    if !strings.is_empty() {
        return strings.iter().map(|s| s.to_string()).collect();
    }
    let mut reader = LineReader::new(
        |chunk| read(support::STDIN, chunk).map_err(|_| ()),
        input,
    );
    let mut lines = Vec::new();
    while reader.next_line().unwrap_or(false) {
        let line = reader.line();
        let trimmed = match line.last() {
            Some(b'\n') => &line[..line.len() - 1],
            _ => line,
        };
        lines.push(String::from_utf8_lossy(trimmed).into_owned());
    }
    lines
}

fn usage() -> ! {
    support::emit(b"usage: morse [-ds] [string ...]\n");
    support::terminate(1);
}
