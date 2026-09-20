//! Minix-RS echo — the doing half over `minix_fileops::echo`.
//!
//! Ground truth: `minix3/bin/echo/echo.c` (NetBSD 1.19). The deciding half
//! (leading `-n`, single-space separation, trailing newline) lives in the
//! library; this program gathers argv, writes each piece to standard
//! output through `minix_sys::write`, and exits 0, or 1 when a write
//! failed (echo.c:77-78's `ferror` check). The output channel follows the
//! stage contract (99-global-concepts.md §1): no stdio library, no message
//! construction.
//!
//! The hosted-versus-target split is carried by the two seams shared with
//! every binary in this crate (`../bin_support.rs`): argv via the host
//! runtime or the `minix-rt` birth-chain descriptor, termination via the
//! host runtime or `minix_sys::exit`. This file adds the third piece the
//! freestanding image needs at the crate root: the `no_std`/`no_main`
//! gate with the two `main` forms over one diverging `run` (the init
//! entry precedent, `os/commands/sbin/init/src/main.rs` — rustc 1.94
//! requires a hosted `main` to return `()`, while the `crt0` consumer
//! contract resolves the symbol `main` returning `i32`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::echo::echo_emit;
use minix_sys::{write, Fd};

/// Standard output, POSIX `STDOUT_FILENO`.
const STDOUT: Fd = 1;

/// The whole program body; both `main` forms call it and it never
/// returns (every path ends in [`support::terminate`]).
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let delivered = echo_emit(&args, |piece: &[u8]| write(STDOUT, piece).is_ok());
    if delivered {
        support::terminate(0);
    } else {
        support::terminate(1);
    }
}

/// Target entry: the `crt0` birth chain resolves the symbol `main` by
/// name (consumer contract, `minix-rt/src/crt0.rs`) and its stage-6
/// `exit(main())` reads the `i32` slot — `run` diverges, so the slot is
/// never produced, but the ABI shape must match the contract.
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

/// Hosted entry: std builds go through rustc's start glue, whose `main`
/// must return a `Termination` type — `i32` is not one (rustc 1.94,
/// E0277), `()` is.
#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
