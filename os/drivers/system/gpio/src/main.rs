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

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
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
