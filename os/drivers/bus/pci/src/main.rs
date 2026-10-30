//! The PCI bus driver binary.
//!
//! C: `main` → `chardriver_task(&driver)` (`main.c:721-738`) plus the
//! typed `BUSC_PCI_*` dispatcher reached through the same receive loop.
//! The loop shell and the RS birth handshake live in the shared driver
//! runtime; this binary names the data-store key and builds the service.
//!
//! Boot-table seam: the PCI driver is not yet a boot module — the
//! manifest entry and the freestanding dual entry ride the boot-table
//! registration (one-line/one-stage lines). The configuration-space
//! backend rides the port-IO/chipset seam; until it lands the probe
//! refuses to start, so the service never claims an empty bus.
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("pci: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_pci::init();

    // A driver whose configuration space cannot answer must not enter the
    // loop; returning here is the same fate RS observes from a failed SEF
    // startup.
    if service.probe().is_err() {
        return;
    }
    // C enumerates at startup; the bus list comes from the bridge walk
    // this crate does not model yet, so the service walks bus zero.
    service.scan(&[0]);

    let mut runtime = DriverRuntime::new(transport, "drv.chr.pci");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
