//! The ES1370 (AudioPCI '97) driver binary.
//!
//! C: `main` runs the SEF startup (`sef_cb_init_fresh`: `pci_init`, the
//! device detection, `drv_init`/`drv_init_hw` — `es1370.c`), then the
//! audio framework's task loop (`audio_fw.c:73-87`). The loop shell and
//! the RS birth handshake live in the shared driver runtime; this binary
//! finds the card and builds the service.
//!
//! The DMA allocator is the registered seam (`E-DMABUF`): with the unbound
//! allocator in place the card comes up, answers its four file nodes, and
//! refuses every DMA open honestly rather than programming an address it
//! does not own. The mixer node (control-only) opens without DMA.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds the link reference to minix-rt: the bin code
// itself uses no rt symbol, so an unreferenced rlib would be dropped whole,
// taking the `_start`/`#[panic_handler]`/`#[global_allocator]` trio with it
// (the `extern crate` keep-alive idiom of panic-halt mode). The birth chain
// `_start` has already completed rt initialization before main (the `lib.rs`
// init contract: under no_std, `_start` calls it automatically and
// idempotently), so there is no need — and no reason — to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_driver_rt::transport::DriverTransport;

// The entry point takes two shapes by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test side stays `()` —
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("es1370: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);

    // C `pci_init` resolves the PCI driver by its label.
    let Some(pci) = transport.lookup_label("pci") else {
        return;
    };
    // C's fresh-start init: without the card there is nothing to serve.
    let Ok(probed) = minix_driver_es1370::probe::probe(&minix_sys::ipc::DirectTrapTransport, pci)
    else {
        return;
    };

    let mut service = minix_driver_es1370::init(probed.port);
    service.hooks_mut().set_irq(probed.irq);
    // C's fresh-start init: a card that cannot initialize must not serve.
    if service.init() < 0 {
        return;
    }

    // C announces the character key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.es1370");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
