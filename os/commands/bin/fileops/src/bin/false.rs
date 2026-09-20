//! Minix-RS false — failing no-op.
//!
//! Ground truth: `minix3/usr.bin/false/false.c` (NetBSD): operands are
//! ignored, the exit status is 1. POSIX likewise specifies that `false`
//! shall do nothing unsuccessfully.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

/// The whole program body; both `main` forms call it and it never
/// returns. The two-form entry contract is documented once in `echo.rs`,
/// this crate's template binary.
fn run() -> ! {
    support::terminate(1)
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
