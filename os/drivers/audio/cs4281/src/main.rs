//! The CS4281 driver binary.
//!
//! C: `main` runs the SEF startup (`sef_cb_init_fresh`: `pci_init`, the
//! device probe, the BAR mapping, and `drv_init`/`drv_init_hw` —
//! `cs4281.c`), then
//! the audio framework's task loop (`audio_fw.c:73-87`). The loop shell
//! and the RS birth handshake live in the shared driver runtime; this
//! binary finds the card and builds the service.
//!
//! The DMA allocator is the registered seam (`E-DMABUF`): with the unbound
//! allocator in place the card comes up, answers its three file nodes, and
//! refuses every DMA open honestly rather than programming an address it
//! does not own. The mixer node (control-only) opens without DMA.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself
// uses none of rt's symbols, and an unreferenced rlib is dropped wholesale,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain's
// `_start` has already completed rt initialization before main (`lib.rs` init
// contract: in no_std mode `_start` calls it automatically, idempotently), so
// there is no need — and no business — to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_driver_rt::transport::DriverTransport;

// The entry point is split by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// `os/libs/minix-rt/src/crt0.rs`); the host and test sides keep `()` — since
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("cs4281: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);

    // C `pci_init` resolves the PCI driver by its label.
    let Some(pci) = transport.lookup_label("pci") else {
        return;
    };
    // C's fresh-start init: without the card there is nothing to serve.
    let Ok(probed) = minix_driver_cs4281::probe::probe(&minix_sys::ipc::DirectTrapTransport, pci)
    else {
        return;
    };
    // The register window is memory-mapped: fetch the first BAR and have
    // the VM server place it (`vm_map_phys(SELF, base, size)`).
    let Ok(bar) = minix_driver_cs4281::probe::get_bar(
        &minix_sys::ipc::DirectTrapTransport,
        pci,
        probed.index,
        minix_driver_cs4281::probe::PCI_BAR,
    ) else {
        return;
    };
    if bar.io {
        return;
    }
    let Ok(base) = minix_driver_cs4281::probe::map_bar(
        &minix_sys::ipc::DirectTrapTransport,
        bar.base,
        bar.size,
    ) else {
        return;
    };

    // SAFETY: `base` is the mapping the VM server just placed.
    let dma = minix_audiodriver::dma::VmDmaAlloc {
        self_endpoint: who.endpoint,
    };
    let mut service = unsafe { minix_driver_cs4281::init(base, dma) };
    service.hooks_mut().set_irq(probed.irq);
    // C's fresh-start init: a card that cannot initialize must not serve.
    if service.init() < 0 {
        return;
    }

    // C announces the character key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.cs4281");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
