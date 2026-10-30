//! The VBOX (VirtualBox guest) driver binary.
//!
//! C: `main` runs the SEF startup (`vbox_init`: find and reserve the
//! VMMDev device, register the interrupt, allocate the request page,
//! report the guest, and run the first time update — `vbox.c:88-140`),
//! then the loop that dispatches hardware and clock notifications plus
//! the VBOX requests (`vbox.c:180-193`). The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary finds the
//! device and builds the service.
//!
//! The runtime label is unused: C's startup announces nothing (the
//! `vbox` name in the C configuration is RS's own service label, not a
//! data-store publication), which is why the loop is entered through the
//! announce-free serve path.
//!
//! The PCI probe needs the PCI server's attribute passthrough, which is
//! that driver's own registered seam (it answers ENOSYS today), so on a
//! machine with the current PCI server this binary refuses to start
//! rather than fabricating a port. When the passthrough lands, the probe
//! and this binary need no change.
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
use minix_driver_rt::transport::DriverTransport;

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
        .unwrap_or_else(|r| panic!("vbox: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);

    // C `pci_init` resolves the PCI driver by its label.
    let Some(pci) = transport.lookup_label("pci") else {
        return;
    };
    // C's fresh-start init: without the device there is nothing to serve.
    let Ok(probed) =
        minix_driver_vbox::backdoor::probe_device(&minix_sys::ipc::DirectTrapTransport, pci)
    else {
        return;
    };

    let backdoor = minix_driver_vbox::backdoor::KernelBackdoor::new(who.endpoint, probed.port);
    let mut service = minix_driver_vbox::init(backdoor);
    // The guest report must reach the device (C panics otherwise).
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "vbox");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a panic.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
