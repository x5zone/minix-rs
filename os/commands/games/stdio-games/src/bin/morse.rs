//! Minix-RS morse — international morse code encoder and decoder.
//!
//! Ground truth: `minix3/games/morse/morse.c`, usage `morse [-ds] [string ...]`
//! (`:140`). The encode face is **one line per input byte**: `morse()` looks the byte
//! up and hands the code to `show()`, which spells each symbol as `dit`/`daw` (or,
//! under `-s`, prints the symbol run itself) and then a newline (`:186-196`, `:253-261`).
//! A whitespace byte is not skipped but rendered as the empty code, and an operand
//! boundary adds one more such line; the message as a whole is closed by `SK`
//! (`show("...-.-")`) exactly once. Bytes the tables do not list print nothing at all.
//! Decode (`-d`) turns code groups back into letters; C's stream-shaped decoder
//! (double blanks, the ten-symbol overrun that prints `x`) is not the shape here yet
//! and is registered in the stage ledger.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::morse::{
    BLANK_CODE, MAX_SHOW_LINE, SK_CODE, StaticMorse, code_for_byte, decode_group, show_line,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut decode = false;
    let mut short = false;
    let mut first = 1;
    // BSD getopt stops at the first non-option argument, and an argument that is just
    // `-` is such an argument rather than an option run; `--` ends the scan and is
    // itself consumed (`morse.c:130-144`).
    while first < argv.len() {
        let arg = argv[first].as_str();
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        if arg == "--" {
            first += 1;
            break;
        }
        for byte in arg.as_bytes()[1..].iter() {
            match byte {
                b'd' => decode = true,
                b's' => short = true,
                // C's `default:` answers with the usage line and exit 1, and `-s` never
                // reaches `decode()`, so `-d` combined with `-s` stays inert.
                _ => usage(),
            }
        }
        first += 1;
    }
    let rest: Vec<&str> = argv[first..].iter().map(String::as_str).collect();
    if decode {
        for line in text_lines(&rest) {
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
        encode(&rest, short);
    }
    support::terminate(0);
}

/// C's encode loop and its closing mark (`morse.c:186-196`).
///
/// With operands, every byte of every argument goes through `morse()` and each
/// argument is closed by `show("")`. Without them, standard input is consumed a byte at
/// a time — no line grouping, because C never looks at line structure here and a
/// newline is simply another separator byte. Either way `show("...-.-")` ends the run.
fn encode(strings: &[&str], short: bool) {
    if strings.is_empty() {
        for byte in support::read_stdin() {
            emit_code(code_for_byte(byte), short);
        }
    } else {
        for text in strings {
            for byte in text.as_bytes() {
                emit_code(code_for_byte(*byte), short);
            }
            emit_code(Some(BLANK_CODE), short);
        }
    }
    emit_code(Some(SK_CODE), short);
}

/// One `show()` call: an unlisted byte contributes nothing, everything else one line.
fn emit_code(code: Option<&str>, short: bool) {
    let Some(code) = code else { return };
    let mut out = [0u8; MAX_SHOW_LINE];
    if let Ok(len) = show_line(code, short, &mut out) {
        support::emit(&out[..len]);
    }
}

/// The strings when given, otherwise standard input split into lines.
fn text_lines(strings: &[&str]) -> Vec<String> {
    if !strings.is_empty() {
        return strings.iter().map(|s| s.to_string()).collect();
    }
    let text = String::from_utf8_lossy(&support::read_stdin()).into_owned();
    let mut lines: Vec<String> = text.split('\n').map(|piece| piece.to_string()).collect();
    // A trailing newline terminates the last line rather than opening an empty one.
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}

fn usage() -> ! {
    // C writes the line to standard error and leaves with status 1 (`morse.c:138-141`).
    support::warn(b"usage: morse [-ds] [string ...]\n");
    support::terminate(1);
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
