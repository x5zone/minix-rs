//! Minix-RS tail — last lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/tail/tail.c` (NetBSD), `tail -n count`
//! with a default of ten. The ring capacity is fixed at 32 (the library's
//! `TailWindow`), so counts above 32 are rejected as out of range — a
//! declared cap; the byte mode (`-c`) and the reverse-offset form
//! (`tail -n +5`) are not modelled yet. File operands wait for the gated
//! open-existing call.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::window::{LineWindow, TailWindow};
use minix_sys::Fd;

fn run() -> ! {
    let argv: Vec<String> = support::args();
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
                support::emit(b"tail: file operands wait for the open call (edge E-CMDSYSFACE)\n");
                support::terminate(1);
            }
        }
        index += 1;
    }

    let mut window = match TailWindow::new(count) {
        Ok(window) => window,
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


#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
