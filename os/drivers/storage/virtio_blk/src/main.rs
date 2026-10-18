//! The virtio_blk driver binary.
//!
//! C: `blockdriver_task` (`virtio_blk.c` service entry, framework loop at
//! `driver_st.c:52-89`) — SEF local startup, then the
//! receive-classify-dispatch loop. The loop shell and the RS birth
//! handshake live in the shared driver runtime, the pump in
//! `minix-blockdriver`; this binary names the data-store key and builds
//! the drive face.
//!
//! Boot-table seam: the driver is not yet a boot module — the manifest
//! entry and the freestanding dual entry ride the boot-table
//! registration. Queue setup, descriptor chains, and the device
//! configuration read stay in the card seam the crate documents; the
//! pump answers the whole block protocol around the modeled face.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap and feeds the
    // endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("virtio_blk: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C publishes `drv.blk.` plus the process's own label
    // (`blockdriver_announce`, `driver.c:93-130`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.virtio_blk");
    let mut service = minix_driver_virtio_blk::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
