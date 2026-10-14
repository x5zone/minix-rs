//! Minix-RS `unlink` — the doing half over `minix_fileops::linkops`.
//!
//! Ground truth: `minix3/usr.sbin/unlink/unlink.c`. POSIX's `unlink` is
//! `unlink(2)` behind an argument-count check; the shape check lives in
//! `minix_fileops::linkops`, this program supplies the syscall and error leg
//! (`unlink.c:52-59`). A write-side command: real-machine effect is the
//! writable-root gate (`C-57`/E5). The failure leg is C's
//! `perror("unlink")` (`unlink.c:56`), and Minix3's `perror`
//! (`minix3/lib/libc/stdio/perror.c`) prints `<the caller's string>: <strerror>`
//! with **no program-name prefix** — so the line is `unlink: <sentence>` about the
//! errno, not `unlink: <path>`: the path never reaches C's diagnostic at all.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::linkops::unlink_operand;
use minix_sys::unlink;

fn usage() -> ! {
    support::warn(b"usage: unlink file\n");
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let path = match unlink_operand(&args[1.min(args.len())..]) {
        Some(p) => p,
        None => usage(),
    };
    match unlink(path) {
        Ok(()) => support::terminate(0),
        // C 的 `perror("unlink")`：报的是这个号的句子，操作数本身不出现在诊断里。
        Err(code) => {
            support::perror_line("unlink", code);
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
