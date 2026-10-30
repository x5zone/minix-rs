//! Minix-RS `look` — the doing half over `minix_textfilter::look`.
//!
//! Ground truth: `minix3/minix/commands/look/look.c`. The comparison and the
//! `-d`/`-f` semantics live in the library (`look_matches`); this program parses
//! the flags exactly as C does, chooses the wordlist (a file operand, or the
//! default `/usr/lib/dict/words`), reads it, and prints every line that begins
//! with the search string.
//!
//! Wired: `look [-d] [-f] string [file]`. Note the C quirk that a bare `look
//! string` (no file operand, no dash flags) turns on both `-d` and `-f`
//! (`look.c:130`). A file that cannot be opened reports `look: Can't open <name>`
//! and exits 1, like C's `look()` (`look.c:92-95`). Exit status is 0. Declared
//! residual: the default dictionary is not installed in the shipped image, so the
//! no-file form reports "Can't open"; and C's non-ASCII abort (`nonascii()`) is
//! only reproduced for the search string, not for arbitrary wordlist bytes.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::look::{LookOptions, look_matches};

const DEFAULT_DICT: &str = "/usr/lib/dict/words";

fn warn_exit(message: &str) -> ! {
    let mut line = String::from("look: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1);
}

fn usage() -> ! {
    support::warn(b"Usage: look [-df] string [file]\n");
    support::terminate(1);
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let a: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();

    let mut dictionary = false;
    let mut fold = false;
    let mut argc = a.len();
    let mut base = 0usize;

    // C's `if (argc == 2) dflg = fflg = 1;` on the *original* argument count
    // (program name + a single string operand, no file).
    if argc == 2 {
        dictionary = true;
        fold = true;
    }

    // Consume leading `-`-flag groups exactly as C's getopt-free loop does.
    while argc > 1 {
        let first = match a.get(base + 1) {
            Some(s) => *s,
            None => break,
        };
        if first.first() != Some(&b'-') {
            break;
        }
        for &flag_byte in &first[1..] {
            match flag_byte {
                b'd' => dictionary = true,
                b'f' => fold = true,
                // An unknown flag's letter slot uses the raw bytes (C's test is on
                // bytes, in `look.c`'s flag loop).
                other => {
                    let mut msg: Vec<u8> = b"look: Bad flag: ".to_vec();
                    msg.push(other);
                    msg.push(b'\n');
                    support::warn(&msg);
                    support::terminate(1);
                }
            }
        }
        argc -= 1;
        base += 1;
    }

    // After flags, C expects the operands to reduce to `string [file]`.
    let (string, file): (&[u8], Option<&[u8]>) = match argc {
        3 => (a[base + 1], Some(a[base + 2])),
        2 => (a[base + 1], None),
        _ => usage(),
    };
    // The default dictionary is a constant path; the open call still takes `&str`.
    let path: &[u8] = file.unwrap_or(DEFAULT_DICT.as_bytes());

    if !string.is_ascii() {
        warn_exit("prefix string contains non-ASCII characters.");
    }

    // The open call takes `&str` (registered `ARGV-BYTE-PATH-IN-LIBS`), so the fold
    // happens only at this point; what the message reads out is the original bytes.
    let shown = String::from_utf8_lossy(path).into_owned();
    let data = match support::read_file(&shown) {
        Ok(d) => d,
        Err(_) => {
            let mut msg: Vec<u8> = b"look: Can't open ".to_vec();
            msg.extend_from_slice(path);
            msg.push(b'\n');
            support::warn(&msg);
            support::terminate(1);
        }
    };

    let mut lines: Vec<&[u8]> = data.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }

    let opts = LookOptions { dictionary, fold };
    let mut out: Vec<u8> = Vec::new();
    for line in look_matches(string, &lines, &opts) {
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    support::emit(&out);
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
