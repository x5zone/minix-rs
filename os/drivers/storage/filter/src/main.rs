//! The filter (checksum/mirror proxy) driver binary.
//!
//! C: `main` parses the option table (`main.c:38-...`: checksumming, the
//! on-disk layout, the mirror pair, retries/restarts/timeout, chunk size),
//! resolves the two lower drivers by label, then runs the block loop
//! (`blockdriver_task`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary builds the service.
//!
//! The middle layer (the checksum engine, the mirror policy, and the
//! asynchronous lower-driver management) is the registered seam; until its
//! production leg lands, the binary runs with the unbound leg, whose every
//! transfer is refused honestly rather than serving unchecked data.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("filter: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let face = minix_driver_filter::face::FilterFace::new(minix_driver_filter::UnboundLeg);
    let mut service = minix_driver_filter::init(face);

    // C announces the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.filter");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
