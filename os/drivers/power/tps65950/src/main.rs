//! The TPS65950 PMIC/RTC driver binary.
//!
//! C: `main` parses the bus and base address from the environment, runs
//! the SEF startup (bus lookup, four reservations, chip check, RTC start
//! — `tps65950.c:166-222`), then the RTCDEV loop
//! (`tps65950.c:243-352`). The loop shell and the RS birth handshake
//! live in the shared driver runtime; this binary names the data-store
//! key and builds the service.
//!
//! Registered seams: the environment plumbing (C reads `bus` and
//! `address` from `-args`) is fixed to the documented defaults
//! `BUS_NUMBER` and the fixed address table; the bus-restart
//! subscription rides the data-store client seam.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib is dropped wholesale and
// `_start`/`#[panic_handler]`/`#[global_allocator]` disappear with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain's
// `_start` has already completed rt initialization before main (`lib.rs` init
// contract: in no_std mode `_start` calls it automatically, idempotently), so
// there is no need -- and no call -- for a second one here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits into two shapes by target: on none it satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); host and test builds keep `()` -- since
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("tps65950: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_tps65950::init();

    // C's fresh-start init: a driver without a bus, a reservation, or a
    // recognized chip must not enter the loop.
    if service.start(&mut transport).is_err() {
        return;
    }

    // C publishes the i2c driver key (`i2cdriver_announce`,
    // `drv.i2c.<name>`); there is no character-driver side.
    let mut runtime = DriverRuntime::new(transport, "drv.i2c.tps65950");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a log line and keeps looping; main
    // returns and the process exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
