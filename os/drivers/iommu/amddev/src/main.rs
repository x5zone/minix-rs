//! The AMD DEV (IOMMU) driver binary.
//!
//! C: `main` runs the SEF startup (find the device, init domain and map,
//! write the control value — `amddev.c:118-170`), then the loop that
//! reports exceptions and answers `IOMMU_MAP` requests
//! (`amddev.c:69-98`). The birth handshake and the loop live in the
//! shared driver runtime; this binary builds the service.
//!
//! The runtime label is unused: C's startup announces nothing (this
//! driver has no data-store key), which is why the loop is entered
//! through the announce-free serve path.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("amddev: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_amddev::init();

    // C's fresh-start init: a machine without the AMD DEV must not enter
    // the loop.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "amddev");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a panic.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
