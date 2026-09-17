//! Minix-RS unexpand — spaces to tabs.
//!
//! Ground truth: `minix3/usr.bin/unexpand/unexpand.c` (NetBSD). The
//! deciding half (the tabify walk, `-t` stop lists, the `-a`
//! whole-line form) is the library's `unexpand` module; this program
//! reads stdin, tabifies, and prints. File operands wait for the gated
//! open-existing call.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::unexpand::{parse_stops, unexpand, UnexpandOptions};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut options = UnexpandOptions::default();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-a" {
            options.all = true;
        } else if arg == "-t" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match parse_stops(&argv[index]) {
                Some(stops) => options.stops = stops,
                None => {
                    support::warn(b"unexpand: bad tab stop spec\n");
                    support::terminate(1);
                }
            }
        } else if arg.starts_with('-') && arg.len() > 1 {
            usage();
        } else {
            support::warn(b"unexpand: file operands wait for the open call (edge E-CMDSYSFACE)\n");
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
    for line in unexpand(text, &options) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: unexpand [-a] [-t tablist]\n");
    support::terminate(1);
}
