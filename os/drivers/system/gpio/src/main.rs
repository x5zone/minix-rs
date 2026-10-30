//! The GPIO driver binary.
//!
//! C: `main` fills the virtual-tree hooks and runs the filesystem
//! (`gpio.c:272-289`, `run_vtreefs`). The VFS message loop, the RS birth
//! handshake, and the request decode live in `minix-fs-rt`; this binary
//! contributes the server value (the pin tree and its hooks) and the
//! signal action.
//!
//! Board table seam: C publishes pins only on BBXM and BB boards, after
//! reading the machine identity and programming pad multiplexing. Neither
//! the board identification nor the pad configuration is wired here, so
//! the binary serves the empty root C builds on every other board.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, and an unreferenced rlib would be dropped whole,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain
// `_start` has already completed rt initialization before main (`lib.rs` init
// contract: called automatically in no_std mode, idempotent), so it must not
// and need not be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

extern crate alloc;

use alloc::boxed::Box;

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// The entry point splits into two forms by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host/test side keeps `()` — since
// rustc 1.94, Termination is no longer implemented for i32.
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        let server = minix_driver_gpio::init();
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(minix_driver_gpio::signal_action),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            Err(e) => panic!("gpio: startup failed: {e}"),
        }
    }
}
