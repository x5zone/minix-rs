//! The lance driver binary.
//!
//! C: `netdriver_task` (`lance.c:172-177`, framework loop at
//! `netdriver.c:969-993`) — SEF local startup, then the
//! receive-classify-dispatch loop. The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary names the
//! data-store key and builds the card face.
//!
//! Boot-table seam: the lance driver is not yet a boot module — the
//! manifest entry and the freestanding dual entry ride the boot-table
//! registration (one-line/one-stage lines). Register traffic (probe,
//! init block, interrupt and tick sources) stays in the card seam the
//! service module documents; the pump answers the whole NDEV protocol
//! around the modeled face.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("lance: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C publishes `drv.net.` plus the process's own label
    // (`netdriver_announce`, `netdriver.c:58-75`).
    let mut runtime = DriverRuntime::new(transport, "drv.net.lance");
    let mut service = minix_driver_lance::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
