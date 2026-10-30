//! The SD/MMC block driver binary.
//!
//! C: `main` runs the SEF startup (host selection, the slot scan, the
//! announcements — `mmcblk.c:600-...`), then the block loop
//! (`mmcblk.c:96-130`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary builds the service.
//!
//! The host/card layer is the registered seam; until its production host
//! lands, the binary runs with the unbound host, whose card operations are
//! refused honestly rather than serving invented blocks.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no symbol from rt, and an unreferenced rlib is dropped whole,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// keep semantics of `extern crate` in panic-halt mode). The birth chain
// `_start` has already completed rt initialization before main (`lib.rs`
// init contract: called automatically and idempotently in no_std mode), so
// there is no need — and no reason — to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits by target: the none side satisfies the crt0
// Consumer contract (name plus Rust ABI plus `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); host and test sides keep `()`, because
// since rustc 1.94 Termination is no longer implemented for i32.
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
        .unwrap_or_else(|r| panic!("mmc: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let face = minix_driver_mmc::face::MmcFace::new(minix_driver_mmc::UnboundHost);
    let mut service = minix_driver_mmc::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.mmc");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
