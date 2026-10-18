//! The rtl8139 driver binary.
//!
//! C: `netdriver_task` (`rtl8139.c:118-123`, framework loop at
//! `netdriver.c:969-993`) — SEF local startup, then the
//! receive-classify-dispatch loop. The loop shell and the RS birth
//! handshake live in the shared driver runtime, the pump in
//! `minix-netdriver`; this binary names the data-store key and builds the
//! card face.
//!
//! Boot-table seam: the driver is not yet a boot module — the manifest
//! entry and the freestanding dual entry ride the boot-table
//! registration. Register traffic and the slot/ring cursors stay in the
//! card seam the crate documents; the pump answers the whole NDEV
//! protocol around the modeled face.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap and feeds the
    // endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("rtl8139: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C publishes `drv.net.` plus the process's own label
    // (`netdriver_announce`, `netdriver.c:58-75`).
    let mut runtime = DriverRuntime::new(transport, "drv.net.rtl8139");
    let mut service = minix_driver_rtl8139::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
