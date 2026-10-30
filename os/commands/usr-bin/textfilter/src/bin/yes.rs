//! Minix-RS yes — repeated affirmation.
//!
//! Ground truth: `minix3/usr.bin/yes/yes.c` (NetBSD), main at lines 42-56:
//! the operand defaults to "y" and the program prints it with a newline
//! forever; the loop only ends when the write fails, and the C then
//! leaves with `EXIT_FAILURE`. There is no deciding half — the doing
//! half is the whole command.
//!
//! This branch's argument is **not** a file name or a flag; it is the content
//! that gets repeated: `yes.c:52` stores `argv[1]` as a `const char *`, and
//! `puts` emits it byte for byte plus a newline. So a bare `ff` on the command
//! line must appear verbatim in every line; folding it to `ef bf bd` first
//! would change the output content itself.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // `yes = (argc > 1) ? argv[1] : "y"` at `yes.c:52` takes the **bytes** of
    // that argv string, and `puts` also writes only bytes: the argument is the
    // output content, and folding it once changes the very bytes being repeated.
    let word: &[u8] = argv.get(1).map_or(b"y", Vec::as_slice);
    let mut line = word.to_vec();
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
