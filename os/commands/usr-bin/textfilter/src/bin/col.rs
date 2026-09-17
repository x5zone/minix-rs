//! Minix-RS col — filter reverse line feeds from input.
//!
//! Ground truth: `minix3/usr.bin/col/col.c` (NetBSD), usage
//! `col [-bfpx] [-l nline]` with no operands. The deciding half (the
//! line-assembly state machine and flush rules) is the library's
//! `col` module; this program parses the flags and runs the filter
//! over stdin. The hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`): argv via
//! `std::env::args`, termination via the host runtime.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::col::{filter, ColOptions};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut options = ColOptions::default();
    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let mut chars = arg.chars();
        if chars.next() == Some('-') && arg.len() > 1 {
            for c in chars {
                match c {
                    'b' => options.no_backspaces = true,
                    'f' => options.fine = true,
                    'h' => options.compress_spaces = true,
                    'p' => options.pass_unknown_seqs = true,
                    'x' => options.compress_spaces = false,
                    'l' => {
                        i += 1;
                        let value = argv
                            .get(i)
                            .and_then(|v| v.parse::<i32>().ok())
                            .filter(|v| *v > 0);
                        match value {
                            Some(v) => options.max_bufd_lines = v,
                            None => {
                                support::warn(b"col: bad -l argument.\n");
                                support::terminate(1);
                            }
                        }
                    }
                    _ => usage(),
                }
            }
        } else {
            // The C takes no operands: usage.
            usage();
        }
        i += 1;
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
    let out = filter(&input, &options);
    if out.warned {
        support::warn(b"col: warning: can't back up -- line already flushed\n");
    }
    support::emit(&out.bytes);
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: col [-bfpx] [-l nline]\n");
    support::terminate(1);
}
