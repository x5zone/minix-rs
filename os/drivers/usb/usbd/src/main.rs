//! The USBD (USB host daemon) binary.
//!
//! C: `main` runs the SEF startup (`base/usbd.c`: the controller bring-up
//! and the device scan) and then serves the five `USB_RQ_*` requests
//! (`usb_server.c:731-758`). The loop shell and the RS birth handshake
//! live in the shared driver runtime; this binary builds the service.
//!
//! The controller seam is the registered piece: with the unbound
//! controller in place the daemon answers its protocol and refuses every
//! request block honestly rather than reporting a completion that never
//! happened.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib would be dropped whole,
// taking the `_start`/`#[panic_handler]`/`#[global_allocator]` trio with it
// (the `extern crate` keep-alive idiom of panic-halt mode). The birth chain
// `_start` has already completed rt initialization before main (the `lib.rs`
// init contract: under no_std, `_start` calls it automatically and
// idempotently), so there is no need — and no reason — to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_driver_usbd::hcd::HcdLeg;

// The entry point takes two shapes by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test side stays `()` —
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
        .unwrap_or_else(|r| panic!("usbd: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usbd::init(minix_driver_usbd::hcd::UnboundHcd);
    // C's fresh-start init: a controller that cannot come up must not
    // serve (the daemon's own `hcd_init` gate).
    if service.hcd_mut().init().is_err() {
        return;
    }

    // C announces the daemon key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.usbd");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
