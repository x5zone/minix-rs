//! Minix-RS head — first lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/head/head.c` (NetBSD), `head -n count`
//! with a default of ten. The window capacity is fixed at 32 (the library
//! keeps borrowed lines in a fixed array, `window.rs` `HeadWindow`), so
//! counts above 32 are rejected as out of range — a declared cap, the
//! same shape as the primes bound; the byte mode (`-c`) is not modelled
//! yet. File operands wait for the gated open-existing call.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::window::{HeadWindow, LineWindow};
use minix_sys::Fd;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut count: usize = 10;
    let mut index = 1;
    while index < argv.len() {
        match argv[index].as_str() {
            "-n" => {
                index += 1;
                if index >= argv.len() {
                    support::terminate(1);
                }
                match argv[index].parse::<usize>() {
                    Ok(value) => count = value,
                    Err(_) => support::terminate(1),
                }
            }
            _ => {
                // File operands need the gated open-existing call; the
                // obsolete bare-number syntax is not carried.
                support::emit(b"head: file operands wait for the open call (edge E-CMDSYSFACE)\n");
                support::terminate(1);
            }
        }
        index += 1;
    }

    let mut window = match HeadWindow::new(count) {
        Ok(window) => window,
        // Counts above the fixed 32-line capacity land here.
        Err(_) => support::terminate(1),
    };
    let input = support::read_stdin();
    for line in support::lines_of(&input) {
        window.push(line);
    }
    let mut survivors: Vec<&str> = vec![""; count.min(32)];
    let drained = window.drain(&mut survivors);
    for line in &survivors[..drained] {
        let mut out = line.as_bytes().to_vec();
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
}

// Standard input descriptor retained for the no_std sweep (the collection
// helper owns the read today).
const _: Fd = 0;
