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

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_driver_rt::transport::DriverTransport;

fn main() {
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
