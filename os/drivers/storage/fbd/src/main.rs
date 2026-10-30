//! The FBD (faulty block device) driver binary.
//!
//! C: `main` runs the SEF startup (`sef_cb_init_fresh`: parse `label` and
//! `minor`, resolve the lower driver's endpoint, allocate the staging
//! buffer, announce — `fbd.c:51-87`), then the block loop
//! (`blockdriver_task`, `fbd.c:118-130`). The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary binds the
//! lower driver and builds the service.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself uses no
// rt symbol, so an unreferenced rlib would be dropped wholesale and `_start`/
// `#[panic_handler]`/`#[global_allocator]` would vanish with it (the `extern crate`
// retention semantics of panic-halt mode). The birth chain's `_start` has already
// completed rt initialization before main (`lib.rs` init contract: `_start` calls it
// automatically in no_std mode, and it is idempotent), so it must not and need not be
// called here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_driver_rt::transport::DriverTransport;

// The entry comes in two forms by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs`); the host and
// test side stays `()` — since rustc 1.94, Termination is no longer implemented for i32.
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
        .unwrap_or_else(|r| panic!("fbd: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);

    // C resolves the lower driver by its data-store label
    // (`ds_retrieve_label_endpt`); the label comes from `-args`, which is
    // a registered seam, so the binary binds the documented default.
    let Some(downstream) = transport.lookup_label(minix_driver_fbd::DRIVER_LABEL) else {
        return;
    };
    let mut face = minix_driver_fbd::face::FbdFace::new(minix_driver_fbd::DRIVER_MINOR);
    face.set_downstream(downstream.0);
    let mut service = minix_driver_fbd::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.fbd");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
