//! The I2C bus driver binary.
//!
//! C: `main` parses the instance from the environment, probes the SoC
//! controller, announces, then `chardriver_task(&i2c_tab)`
//! (`i2c.c:430-521`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary names the data-store key and
//! builds the service.
//!
//! Registered seams: the instance comes from `BUS_INSTANCE` (labels
//! `i2c.1`); the RS service label `i2c.1` that clients resolve through
//! the data store rides the boot table (A/C), and the SoC controller
//! implementation rides the board-support seam.
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("i2c: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_i2c::init();

    // C's fresh-start init probes the controller; a board whose bus cannot
    // come up must not serve reservations.
    if service.start().is_err() {
        return;
    }

    // C publishes `drv.chr.` plus the process's own label (`i2c.1`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.i2c.1");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
