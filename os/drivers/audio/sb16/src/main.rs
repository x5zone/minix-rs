//! The SB16 (SoundBlaster 16) driver binary.
//!
//! C: `main` runs the SEF startup (`sef_cb_init_fresh`: the DSP probe and
//! reset, `drv_init`/`drv_init_hw` — `sb16.c`), then the audio framework's
//! task loop (`audio_fw.c:73-87`). The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary builds the
//! service.
//!
//! The DMA allocator is the registered seam (`E-DMABUF`): with the unbound
//! allocator in place the card comes up, answers its three file nodes, and
//! refuses every DMA open honestly rather than programming an address it
//! does not own. The mixer node (control-only) opens without DMA.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the binary's own
// code uses no rt symbol, so an unreferenced rlib is dropped wholesale and the
// three pieces `_start` / `#[panic_handler]` / `#[global_allocator]` vanish
// with it (the retaining semantics of `extern crate` in the panic-halt
// pattern). The birth chain's `_start` has already completed rt initialization
// before main (the `lib.rs` init contract: in no_std mode `_start` calls it
// automatically, idempotently); there is no need and no place to call it here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point is split in two by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host/test side keeps `()` — since rustc
// 1.94 Termination is no longer implemented for i32.
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("sb16: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_sb16::init();
    // C's fresh-start init: a card that cannot initialize must not serve.
    if service.init() < 0 {
        return;
    }

    // C announces the character key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.sb16");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
