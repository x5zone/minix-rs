//! The TI1225 CardBus controller driver binary.
//!
//! C: `main` runs the SEF startup (probe, bridge setup, socket masks,
//! one socket pass — `ti1225.c:87-240`), then loops printing every
//! message (`ti1225.c:46-64`). The birth handshake and the loop live in
//! the shared driver runtime; this binary builds the service.
//!
//! The runtime label is unused: C's startup announces nothing (this
//! driver has no data-store key and no client protocol), which is why
//! the loop is entered through the announce-free serve path.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code itself
// uses no rt symbol, and an unreferenced rlib is dropped entirely, taking
// `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the `extern crate`
// retention semantics of panic-halt mode). The birth chain's `_start` has already
// finished rt initialization before main (the `lib.rs` init contract: in no_std
// mode `_start` calls it automatically and idempotently), so it neither needs nor
// should be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`); host and
// test sides keep `()` — since rustc 1.94, Termination is no longer implemented
// for i32.
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("ti1225: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_ti1225::init();

    // C's fresh-start init: a machine without the controller must not
    // enter the loop.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "ti1225");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a panic.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
