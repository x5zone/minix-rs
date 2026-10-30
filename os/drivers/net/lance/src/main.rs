//! The lance driver binary.
//!
//! C: `netdriver_task` (`lance.c:172-177`, framework loop at
//! `netdriver.c:969-993`) — SEF local startup, then the
//! receive-classify-dispatch loop. The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary names the
//! data-store key and builds the card face.
//!
//! Boot-table seam: the lance driver is not yet a boot module — the
//! manifest entry and the freestanding dual entry ride the boot-table
//! registration (one-line/one-stage lines). Register traffic (probe,
//! init block, interrupt and tick sources) stays in the card seam the
//! service module documents; the pump answers the whole NDEV protocol
//! around the modeled face.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbols, so an unreferenced rlib would be dropped
// wholesale and the trio `_start`/`#[panic_handler]`/`#[global_allocator]`
// would vanish with it (the `extern crate` retention semantics of
// panic-halt). The birth chain already ran rt initialization before main
// (`lib.rs` init contract: `_start` calls it automatically and idempotently
// in no_std mode), so there is no need — and no reason — to call it again.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry splits by target into two forms: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test side keeps `()` —
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("lance: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C publishes `drv.net.` plus the process's own label
    // (`netdriver_announce`, `netdriver.c:58-75`).
    let mut runtime = DriverRuntime::new(transport, "drv.net.lance");
    let mut service = minix_driver_lance::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
