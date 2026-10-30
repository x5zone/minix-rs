//! The USB mass-storage driver binary.
//!
//! C: `main` initializes DDEkit, registers the driver with the USB host
//! daemon (`ddekit_usb_init(&mass_storage_driver, ...)`,
//! `usb_storage.c:121-124`), and serves the block interface
//! (`bdr_transfer = mass_storage_transfer`). The loop shell and the RS
//! birth handshake live in the shared driver runtime; this binary builds
//! the service.
//!
//! The daemon seam is the registered piece: with the unbound seam in place
//! the driver registers nothing and refuses startup honestly rather than
//! serving a device it cannot reach.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself
// uses none of rt's symbols, and an unreferenced rlib is dropped wholesale,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain's
// `_start` has already completed rt initialization before main (`lib.rs` init
// contract: in no_std mode `_start` calls it automatically, idempotently), so
// there is no need — and no business — to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point is split by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test sides keep `()` — since
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("usb_storage: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usb_storage::init(minix_driver_usb_storage::usbd::UnboundUsbd);
    // C's fresh-start init: a driver that cannot reach the daemon must not
    // serve.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "drv.blk.usb_storage");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
