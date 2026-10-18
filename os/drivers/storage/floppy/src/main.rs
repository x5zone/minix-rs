//! The floppy driver binary.
//!
//! C: `main` runs the SEF startup (option parse, IRQ policy, the DMA
//! buffer, the controller reset — `floppy.c:1270-...`), then the block
//! loop (`floppy_task`, `floppy.c:290-...`). The loop shell and the RS
//! birth handshake live in the shared driver runtime; this binary builds
//! the service.
//!
//! The FDC device layer is the registered seam; until its production leg
//! lands, the binary runs with the unbound leg, whose sector operations
//! are refused honestly rather than serving invented data.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("floppy: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let face = minix_driver_floppy::face::FloppyFace::new(minix_driver_floppy::UnboundFdc);
    let mut service = minix_driver_floppy::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.floppy");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
