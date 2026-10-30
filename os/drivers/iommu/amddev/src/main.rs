//! The AMD DEV (IOMMU) driver binary.
//!
//! C: `main` runs the SEF startup (find the device, init domain and map,
//! write the control value — `amddev.c:118-170`), then the loop that
//! reports exceptions and answers `IOMMU_MAP` requests
//! (`amddev.c:69-98`). The birth handshake and the loop live in the
//! shared driver runtime; this binary builds the service.
//!
//! The runtime label is unused: C's startup announces nothing (this
//! driver has no data-store key), which is why the loop is entered
//! through the announce-free serve path.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself touches no rt symbol, so an unreferenced rlib is discarded whole
// and `_start`/`#[panic_handler]`/`#[global_allocator]` disappear with it —
// this is the keep-alive meaning of `extern crate` in panic-halt style. The
// birth chain's `_start` already finished rt initialization before main
// (the `lib.rs` init contract: in no_std mode `_start` calls it
// automatically, idempotently), so there is neither need nor reason to call
// it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); host and test sides keep `()` — since
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
        .unwrap_or_else(|r| panic!("amddev: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_amddev::init();

    // C's fresh-start init: a machine without the AMD DEV must not enter
    // the loop.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "amddev");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a panic.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
