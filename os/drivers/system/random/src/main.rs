//! The random driver binary.
//!
//! C: `main()` (`main.c:120-268` plus `random.c`) — SEF local startup,
//! then the receive-classify-dispatch loop over the single `/dev/random`
//! minor. The loop shell and the RS birth handshake live in the shared
//! driver runtime; this binary names the data-store label and builds the
//! production face (AES-256 keystream, SHA-256 pools).
//!
//! Boot-table seam: the random driver is not yet a boot module — the
//! manifest entry, the freestanding dual entry, and the minix-rt link
//! reference ride the boot-table registration (one-line/one-stage lines).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib would be dropped whole
// and `_start`/`#[panic_handler]`/`#[global_allocator]` would vanish with it
// (the `extern crate` retention semantics of the panic-halt pattern). The
// birth chain's `_start` has already completed rt initialization before
// `main` (the `lib.rs` init contract: in no_std form `_start` calls it
// automatically, idempotently), so it is neither needed nor proper to call it
// again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry splits two ways by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`);
// the host and test side keeps `()` — since rustc 1.94, Termination is no
// longer implemented for i32.
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
        .unwrap_or_else(|r| panic!("random: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut runtime = DriverRuntime::new(transport, "drv.chr.random");
    let mut service = minix_driver_random::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
