//! The es1371 (Ensoniq AudioPCI) driver binary.
//!
//! C: `main` runs the SEF startup (`sef_cb_init_fresh`: the PCI probe, the
//! IRQ policy, `drv_init`/`drv_init_hw` — `es1371.c`), then the audio
//! framework's task loop (`audio_fw.c:73-87`). The loop shell and the RS
//! birth handshake live in the shared driver runtime; this binary builds
//! the service.
//!
//! The DMA allocator is the registered seam (`E-DMABUF`): with the unbound
//! allocator in place the card comes up, answers its file interface, and
//! refuses every open honestly rather than programming an address it does
//! not own.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("es1371: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_es1371::init();
    // C's fresh-start init: a card that cannot initialize must not serve.
    if service.init() < 0 {
        return;
    }

    // C announces the character key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.es1371");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
