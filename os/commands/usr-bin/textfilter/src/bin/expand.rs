//! Minix-RS expand — tabs to spaces.
//!
//! Ground truth: `minix3/usr.bin/expand/expand.c` (NetBSD). The deciding
//! half (stop lists, the four expansion rules, backspace and newline
//! column tracking) is the library's `expand` module; this program reads
//! stdin, expands, and prints. File operands wait for the gated
//! open-existing call; the obsolete `-N` stop form parses too.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::expand::{expand, parse_stops};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut stops: Vec<usize> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-t" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            stops = match parse_stops(&argv[index]) {
                Some(stops) => stops,
                None => {
                    support::warn(b"expand: bad tab stop spec\n");
                    support::terminate(1);
                }
            };
        } else if arg.starts_with('-') && arg.len() > 1 && arg[1..].bytes().all(|b| b.is_ascii_digit())
        {
            // Obsolete `-N` stop form (expand.c:69-73).
            stops = match parse_stops(&arg[1..]) {
                Some(stops) => stops,
                None => {
                    support::warn(b"expand: bad tab stop spec\n");
                    support::terminate(1);
                }
            };
        } else {
            support::warn(b"expand: file operands wait for the open call (edge E-CMDSYSFACE)\n");
            support::terminate(1);
        }
        index += 1;
    }

    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let text = std::str::from_utf8(&input).unwrap_or("");
    support::emit(expand(text, &stops).as_bytes());
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: expand [-t tablist]\n");
    support::terminate(1);
}
