//! The SD/MMC block driver binary.
//!
//! C: `main` runs the SEF startup (host selection, the slot scan, the
//! announcements — `mmcblk.c:600-...`), then the block loop
//! (`mmcblk.c:96-130`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary builds the service.
//!
//! The host/card layer is the registered seam; until its production host
//! lands, the binary runs with the unbound host, whose card operations are
//! refused honestly rather than serving invented blocks.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("mmc: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let face = minix_driver_mmc::face::MmcFace::new(minix_driver_mmc::UnboundHost);
    let mut service = minix_driver_mmc::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.mmc");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
