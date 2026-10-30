//! The USB hub driver binary.
//!
//! C: `main` initializes DDEkit and registers the driver with the USB host
//! daemon (`ddekit_usb_init(&usb_driver, ...)`, `usb_hub.c:219-330`), then
//! the hub thread runs the polling rounds. The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary builds the
//! service.
//!
//! The daemon seam is the registered piece: with the unbound seam in place
//! the driver registers nothing and refuses startup honestly rather than
//! polling a hub it cannot reach.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbols, so an unreferenced rlib would be dropped
// wholesale and the trio `_start`/`#[panic_handler]`/`#[global_allocator]`
// would vanish with it (the `extern crate` retention semantics of
// panic-halt). The birth chain already ran rt initialization before main
// (`lib.rs` init contract: `_start` calls it automatically and idempotently
// in no_std mode), so there is no need — and no reason — to call it again.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry splits by target into two forms: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test side keeps `()` —
// since rustc 1.94, Termination is no longer implemented for i32.
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
        .unwrap_or_else(|r| panic!("usb_hub: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usb_hub::init(minix_driver_usb_hub::usbd::UnboundUsbd);
    // C's fresh-start init: a hub driver that cannot reach the daemon must
    // not serve.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "drv.chr.usb_hub");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
