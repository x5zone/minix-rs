//! The log driver binary.
//!
//! C: `main()` (`log.c:41-66`) — announce through the data store, then run
//! the receive-classify-dispatch loop. Both halves come from the shared
//! driver runtime; this binary only names the data-store label and builds
//! the device.
//!
//! Boot-table seam: the log driver is not yet a boot module — the manifest
//! entry, the freestanding dual entry, and the minix-rt link reference ride
//! the boot-table registration (one-line/one-stage lines). This is the
//! production wiring that registration will carry.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("log: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut runtime = DriverRuntime::new(transport, "drv.chr.log");
    let mut service = minix_driver_log::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
