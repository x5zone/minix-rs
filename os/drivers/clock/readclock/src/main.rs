//! The readclock driver binary.
//!
//! C: `main()` (`readclock.c:41-131`) — the service loop over the RTCDEV
//! protocol family: gate, fetch/store the caller's `struct tm`, answer
//! the general reply. The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary names the service label and
//! builds the device.
//!
//! Boot-table seam: the readclock driver is not yet a boot module — the
//! manifest entry, the freestanding dual entry, and the minix-rt link
//! reference ride the boot-table registration (one-line/one-stage lines).
//! The chip behind the face is [`minix_driver_readclock::clock::NullClock`]
//! until an architecture clock lands; every operation then answers `EIO`
//! honestly instead of fabricating a time.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code
// itself uses no symbol from rt, and an unreferenced rlib is dropped whole,
// taking `_start`, `#[panic_handler]`, and `#[global_allocator]` with it (the
// `extern crate` retention semantics of the panic-halt pattern). The birth
// chain's `_start` already completed rt initialization before main (the
// `lib.rs` init contract: called automatically in no_std mode and idempotent),
// so there is no need—and no reason—to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`); the
// host and test sides keep `()`—since rustc 1.94, Termination is no longer
// implemented for i32.
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
        .unwrap_or_else(|r| panic!("readclock: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // The boot configuration's service name (`minix3/etc/system.conf:257`
    // lists `readclock.drv`); C relies on the boot-time label, and this
    // announce publishes the same key through the shared runtime.
    let mut runtime = DriverRuntime::new(transport, "readclock.drv");
    let mut service = minix_driver_readclock::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
