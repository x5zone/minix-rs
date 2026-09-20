//! Minix-RS yes — repeated affirmation.
//!
//! Ground truth: `minix3/usr.bin/yes/yes.c` (NetBSD), main at lines 42-56:
//! the operand defaults to "y" and the program prints it with a newline
//! forever; the loop only ends when the write fails, and the C then
//! leaves with `EXIT_FAILURE`. There is no deciding half — the doing
//! half is the whole command.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let word: &str = argv.get(1).map_or("y", |s| s.as_str());
    let mut line = word.as_bytes().to_vec();
    line.push(b'\n');
    // The write channel keeps refusing on the host until the transport
    // lands; a refusing write is the C's loop-exit (`puts < 0`) and
    // leaves status 1.
    while support::write_ok(&line) {
        continue;
    }
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
