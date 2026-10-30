//! The floppy driver binary.
//!
//! C: `main` runs the SEF startup (option parse, IRQ policy, the DMA
//! buffer, the controller reset — `floppy.c:1270-...`), then the block
//! loop (`floppy_task`, `floppy.c:290-...`). The loop shell and the RS
//! birth handshake live in the shared driver runtime; this binary builds
//! the service.
//!
//! The FDC device layer is the registered seam; until its production leg
//! lands, the binary runs with the unbound leg, whose sector operations
//! are refused honestly rather than serving invented data.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no symbol from rt, so an unreferenced rlib would be dropped
// whole, taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it
// (the `extern crate` retention semantics of panic-halt mode). On the
// birth chain `_start` has already completed rt initialization before
// main (`lib.rs` init contract: in no_std mode `_start` calls it
// automatically, and idempotently), so calling it here is neither needed
// nor appropriate.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point is split in two per target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test side keeps `()` —
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
        .unwrap_or_else(|r| panic!("floppy: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let face = minix_driver_floppy::face::FloppyFace::new(minix_driver_floppy::UnboundFdc);
    let mut service = minix_driver_floppy::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.floppy");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
