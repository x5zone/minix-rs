//! The filter (checksum/mirror proxy) driver binary.
//!
//! C: `main` parses the option table (`main.c:38-...`: checksumming, the
//! on-disk layout, the mirror pair, retries/restarts/timeout, chunk size),
//! resolves the two lower drivers by label, then runs the block loop
//! (`blockdriver_task`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary builds the service.
//!
//! The middle layer (the checksum engine, the mirror policy, and the
//! asynchronous lower-driver management) is the registered seam; until its
//! production leg lands, the binary runs with the unbound leg, whose every
//! transfer is refused honestly rather than serving unchecked data.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code
// itself references none of rt's symbols, so an unreferenced rlib would be
// dropped wholesale and `_start`/`#[panic_handler]`/`#[global_allocator]`
// would vanish with it (the `extern crate` retention semantics of the
// panic-halt pattern). The birth chain `_start` has already completed rt
// initialization before main (`lib.rs` init contract: in no_std mode `_start`
// calls it automatically and it is idempotent), so there is no need — and no
// reason — to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits into two forms by target: the none side satisfies
// the crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test sides keep `()` —
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
        .unwrap_or_else(|r| panic!("filter: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let face = minix_driver_filter::face::FilterFace::new(minix_driver_filter::UnboundLeg);
    let mut service = minix_driver_filter::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.filter");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
