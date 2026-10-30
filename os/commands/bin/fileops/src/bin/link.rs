//! Minix-RS `link` — the doing half over `minix_fileops::linkops`.
//!
//! Ground truth: `minix3/usr.sbin/link/link.c`. POSIX's `link` is `link(2)`
//! behind an argument-count check; the shape check lives in
//! `minix_fileops::linkops`, this program supplies the syscall and error leg
//! (`link.c:52-59`). A write-side command: the real-machine effect is the
//! writable-root gate (E5). The failure leg is C's `perror("link")`
//! (`link.c:57`), which in Minix3 prints `<the caller's string>: <strerror>` with no
//! program-name prefix (`minix3/lib/libc/stdio/perror.c`) — so the line is
//! `link: <sentence>`, and neither operand reaches the diagnostic. Note this is the single
//! `link(2)` utility (hard link, target must be an existing non-directory on a
//! filesystem that disallows linking directories); the richer BSD `ln` with its
//! `-s`/`-f`/`-n` behaviour is the separate `ln` command.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::linkops::link_operands;
use minix_sys::link;

fn usage() -> ! {
    support::warn(b"usage: link file1 file2\n");
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (old, new) = match link_operands(&args[1.min(args.len())..]) {
        Some(p) => p,
        None => usage(),
    };
    match link(old, new) {
        Ok(()) => support::terminate(0),
        // C's `perror("link")`: reports the errno sentence, not the operands.
        Err(code) => {
            support::perror_line("link", code);
            support::terminate(1);
        }
    }
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
