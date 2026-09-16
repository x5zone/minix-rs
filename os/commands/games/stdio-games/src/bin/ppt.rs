//! Minix-RS ppt — paper tape punch and reader.
//!
//! Ground truth: `minix3/games/ppt/ppt.c` (NetBSD 1.19), main at lines 65
//! to 127. Encode (default): the tape edge, then each byte of the argument
//! strings (blank between arguments) or of stdin. Decode (`-d`, no string
//! arguments allowed): stdin tape lines (`buf[132]`, line 68) resolve to
//! bytes until a line without a feed hole — after the tape has synced that
//! is the end, and the run ends with a newline when one is owed. Unknown
//! options print `usage: ppt [-d] [string ...]` and exit 1 (line 61).

#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::ppt::{decode_line, punch_byte, EDGE};
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
    let rest = &argv[first..];
    if decode {
        decode_tape(rest);
    } else {
        punch(rest);
    }
}

fn usage() -> ! {
    support::emit(b"usage: ppt [-d] [string ...]\n");
    support::terminate(1);
}

fn punch(strings: &[String]) {
    support::emit(EDGE);
    support::emit(b"\n");
    let mut piece = [0u8; 16];
    if strings.is_empty() {
        let mut chunk = [0u8; 256];
        loop {
            let read = read(support::STDIN, &mut chunk);
            let count = match read {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => break,
            };
            for &byte in &chunk[..count] {
                punch_one(byte, &mut piece);
            }
        }
    } else {
        for (index, string) in strings.iter().enumerate() {
            for &byte in string.as_bytes() {
                punch_one(byte, &mut piece);
            }
            if index + 1 < strings.len() {
                punch_one(b' ', &mut piece);
            }
        }
    }
    support::emit(EDGE);
    support::emit(b"\n");
    support::terminate(0);
}

fn punch_one(byte: u8, piece: &mut [u8]) {
    match punch_byte(byte, piece) {
        Ok(len) => support::emit(&piece[..len]),
        Err(_) => support::terminate(1),
    }
}

fn decode_tape(strings: &[String]) {
    if !strings.is_empty() {
        usage();
    }
    let mut input = [0u8; 132];
    let mut reader = LineReader::new(
        |chunk| read(support::STDIN, chunk).map_err(|_| ()),
        &mut input,
    );
    let mut synced = false;
    let mut needs_newline = false;
    loop {
        let has_line = reader.next_line().unwrap_or(false);
        if !has_line {
            break;
        }
        match decode_line(reader.line()) {
            Some(byte) => {
                synced = true;
                support::emit(&[byte]);
                needs_newline = byte != b'\n';
            }
            // A sync-less line before the tape starts is preamble.
            None if synced => {
                if needs_newline {
                    support::emit(b"\n");
                }
                support::terminate(0);
            }
            None => {}
        }
    }
    if needs_newline {
        support::emit(b"\n");
    }
    support::terminate(0);
}
