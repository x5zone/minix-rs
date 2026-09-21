//! Minix-RS cmp — byte comparison of two streams.
//!
//! Ground truth: `minix3/usr.bin/cmp/` (NetBSD). The deciding half — the
//! byte walk with the `-l` difference list and the EOF report — is the
//! library's `cmp` module, fully tested there. This program is
//! structurally wired: both operands are file names in the C, and file
//! opens wait for the gated open-existing call, so every invocation is
//! rejected with that message until the face lands (edge
//! E-CMDSYSFACE). Exit statuses match the C: 0 equal, 1 different, 2
//! usage or open trouble (`ERR_EXIT`).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;


fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut list_all = false;
    let mut silent = false;
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if arg == "-l" {
            list_all = true;
        } else if arg == "-s" {
            silent = true;
        } else if !arg.starts_with('-') || arg.len() == 1 {
            operands.push(arg);
        } else {
            usage();
        }
    }
    if list_all && silent {
        usage();
    }
    if operands.len() < 2 || operands.len() > 4 {
        usage();
    }
    // Both operands are file names; the open-existing call they need is
    // gated (edge E-CMDSYSFACE).
    support::warn(b"cmp: file operands wait for the open call (edge E-CMDSYSFACE)\n");
    support::terminate(2);
}

fn usage() -> ! {
    support::warn(b"usage: cmp [-l | -s] file1 file2 [skip1 [skip2]]\n");
    support::terminate(2);
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
