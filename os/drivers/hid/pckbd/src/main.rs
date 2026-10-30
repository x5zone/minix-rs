//! The pckbd driver binary.
//!
//! C correspondence: `main()` (`pckbd.c:465-493`) announces the device type
//! through the input-driver library, then runs the receive-classify-dispatch
//! loop. Both halves come from the shared driver runtime; this binary names
//! the data-store key and builds the device.
//!
//! Announce seam: C's `inputdriver_announce` publishes the device-type mask
//! as a 32-bit value under `drv.inp.<label>` (`inputdriver.c:22-38`), which
//! differs from the character framework's endpoint-label publish that the
//! shared runtime's announce runs. The key spelling is faithful here; the
//! value-versus-endpoint payload of the announce is a transport detail that
//! rides on the input server's subscription reader (closed elsewhere), so it
//! is registered rather than faked.
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
use minix_sys::inputdriver::announce_key;

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
    // Birth self-identification: C's sef_startup calls sys_whoami first to
    // fill sef_self_endpoint (sef.c:76-87, panic on failure at :81). The
    // endpoint is granted by the kernel boot table (the bootstrap path does
    // not go through RS); the query rides the sourceless
    // DirectKernelCallTransport, and the answer then feeds the
    // endpoint-carrying IPC transport (the same form as tty).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("pckbd: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C names the driver after its SEF label ("pckbd"); the shared
    // `announce_key` adds the input family's `drv.inp.` prefix.
    let key = announce_key("pckbd");
    let mut runtime = DriverRuntime::new(transport, key);
    let mut service = minix_driver_pckbd::init();
    // A fresh start needs no extra preparation: the state machines and
    // (empty) registration are built at service construction. The runtime
    // refuses the stateful kinds itself, so this callback only ever sees
    // the fresh kind.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
