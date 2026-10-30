//! The at_wini driver binary.
//!
//! C: `blockdriver_task` (`at_wini.c` service entry, framework loop at
//! `driver_st.c:52-89`) — SEF local startup, then the
//! receive-classify-dispatch loop. The loop shell and the RS birth
//! handshake live in the shared driver runtime, the pump in
//! `minix-blockdriver`; this binary names the data-store key and builds
//! the drive face.
//!
//! Boot-table seam: the driver is not yet a boot module — the manifest
//! entry and the freestanding dual entry ride the boot-table
//! registration. Controller reset, probe, IDENTIFY, IRQ14, and the data
//! movement stay in the card seam the crate documents (the machine tail
//! is tracked separately on the B board); the pump answers the whole
//! block protocol around the modeled face.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself
// touches no symbol from rt, and an unreferenced rlib is dropped wholesale,
// taking `_start`, `#[panic_handler]` and `#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt).  The birth chain's `_start`
// has already completed rt initialization before main (`lib.rs` init contract:
// in no_std mode `_start` calls it automatically and idempotently), so it must
// not and need not be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point is split by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`); the
// host/test side stays `()` — since rustc 1.94 nothing implements Termination
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
    // answer rides the sourceless direct trap and feeds the
    // endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("at_wini: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C publishes `drv.blk.` plus the process's own label
    // (`blockdriver_announce`, `driver.c:93-130`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.at_wini");
    let mut service = minix_driver_at_wini::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
