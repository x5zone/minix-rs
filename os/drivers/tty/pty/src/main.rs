//! The pty driver binary.
//!
//! C: `pty_main` (`pty.c:380-460` area) — SEF local startup, then the
//! receive-classify-dispatch loop over the master minors. The loop shell
//! and the RS birth handshake live in the shared driver runtime; this
//! binary names the data-store label and builds the pair table.
//!
//! Boot-table seam: the pty driver is not yet a boot module — the
//! manifest entry, the freestanding dual entry, and the minix-rt link
//! reference ride the boot-table registration (one-line/one-stage lines).
//! The slave-side integration (the tty line sharing the pairs) and the
//! grant byte copies ride the registered cross-crate seams.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code
// itself uses none of rt's symbols, and an unreferenced rlib is dropped whole,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` keeps-the-rlib semantics of panic-halt mode). The birth
// chain's `_start` has already completed rt initialization before main (the
// `lib.rs` init contract: in no_std mode `_start` calls it automatically and
// it is idempotent), so calling it here is neither needed nor right.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits into two forms by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the hosted and test sides keep
// `()` — since rustc 1.94, Termination is no longer implemented for i32.
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
        .unwrap_or_else(|r| panic!("pty: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut runtime = DriverRuntime::new(transport, "drv.chr.pty");
    let mut service = minix_driver_pty::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
