//! Minix-RS `sync` — schedule all pending filesystem writes to disk.
//!
//! Ground truth: `minix3/bin/sync/sync.c` (a five-line `main`). It ignores all
//! operands (no `getopt`), calls `sync()`, and exits 0 — behaviour reproduced
//! here exactly. The deciding/testable logic is empty by design: like the
//! `true`/`false` wrappers, the command's whole contract is one syscall plus a
//! zero exit, so correctness is the faithful call, verified by the freestanding
//! build rather than a unit test.
//!
//! Real-machine effect is the writable-root gate (`C-57`/E5), as for every
//! disk-touching command; `minix_sys::sync` is the wired leg.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    // C ignores argv entirely: no operand parsing, no usage, always exit 0.
    minix_sys::sync();
    support::terminate(0);
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
