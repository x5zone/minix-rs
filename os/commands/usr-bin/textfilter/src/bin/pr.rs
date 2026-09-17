//! Minix-RS pr — paginated output.
//!
//! Ground truth: `minix3/usr.bin/pr/pr.c` (NetBSD) with the constants of
//! `pr.h` (66-line pages, 5-line header and trailer). The deciding half
//! — page assembly, the header line, body padding — is the library's
//! `pr` module; this program reads stdin, paginates, and prints. The
//! time field of the header line comes from the host clock here (the C
//! renders the current date and time); the multi-column forms
//! (`-column`, `-a`, `-m`), double spacing (`-d`), page offsets (`-o`),
//! the formfeed (`-F`), and the starting page (`+first`) are later
//! batches. File operands wait for the gated open-existing call.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::pr::{paginate, PrOptions};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut options = PrOptions::default();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-t" {
            options.no_header = true;
        } else if arg == "-h" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            options.header = argv[index].clone();
        } else if arg == "-l" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match argv[index].parse::<usize>() {
                // pr.c:1875-1878: the page must carry header and trailer.
                Ok(length) if length > 10 => options.page_len = length,
                _ => usage(),
            }
        } else if arg == "+" {
            // `+first_page` and multi-column forms are later batches.
            support::warn(b"pr: +page and -column forms are not modelled yet\n");
            support::terminate(2);
        } else {
            support::warn(b"pr: file operands wait for the open call (edge E-CMDSYSFACE)\n");
            support::terminate(2);
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
    let lines = split_lines(&input);
    for page in paginate(&lines, &options) {
        for line in page.lines() {
            let mut out = line.as_bytes().to_vec();
            out.push(b'\n');
            support::emit(&out);
        }
    }
    support::terminate(0);
}

fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = std::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

fn usage() -> ! {
    support::warn(b"usage: pr [-t] [-h header] [-l lines]\n");
    support::terminate(2);
}
