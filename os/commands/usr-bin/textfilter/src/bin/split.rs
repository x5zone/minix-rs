//! Minix-RS split — chunk computation for line splits.
//!
//! Ground truth: `minix3/usr.bin/split/split.c` (NetBSD): chunks of
//! 1000 lines (`-l` moves it) named by a two-letter suffix `aa`, `ab`,
//! ... The deciding half (chunk computation and suffix enumeration) is
//! the library's `split` module. The doing half's file writes wait for
//! the gated create face on the host, so this build prints each chunk
//! under a `==> prefix <==` banner instead of writing files — the chunk
//! boundaries and suffixes are what the deciding half owns.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::split::{chunks, suffix, DEFAULT_LINES};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut lines_per_chunk = DEFAULT_LINES;
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-l" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match argv[index].parse::<usize>() {
                Ok(0) | Err(_) => usage(),
                Ok(count) => lines_per_chunk = count,
            }
        } else if arg.starts_with('-') && arg.len() > 1 {
            usage();
        } else {
            support::warn(b"split: file operands wait for the open call (edge E-CMDSYSFACE)\n");
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
    let text = std::str::from_utf8(&input).unwrap_or("");
    let owned: Vec<String> = text
        .split('\n')
        .map(|line| line.to_string())
        .collect();
    let mut lines: Vec<String> = owned;
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    for (index, chunk_lines) in chunks(&lines, lines_per_chunk).iter().enumerate() {
        let suffix = match suffix(index) {
            Some(suffix) => suffix,
            None => {
                support::warn(b"split: suffixes exhausted\n");
                support::terminate(2);
            }
        };
        let banner = format!("==> {} <==\n", suffix);
        support::warn(banner.as_bytes());
        for line in chunk_lines {
            let mut out: Vec<u8> = line.as_bytes().to_vec();
            out.push(b'\n');
            support::emit(&out);
        }
    }
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: split [-l lines]\n");
    support::terminate(2);
}
