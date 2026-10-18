//! The USB mass-storage driver binary.
//!
//! C: `main` initializes DDEkit, registers the driver with the USB host
//! daemon (`ddekit_usb_init(&mass_storage_driver, ...)`,
//! `usb_storage.c:121-124`), and serves the block interface
//! (`bdr_transfer = mass_storage_transfer`). The loop shell and the RS
//! birth handshake live in the shared driver runtime; this binary builds
//! the service.
//!
//! The daemon seam is the registered piece: with the unbound seam in place
//! the driver registers nothing and refuses startup honestly rather than
//! serving a device it cannot reach.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("usb_storage: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usb_storage::init(minix_driver_usb_storage::usbd::UnboundUsbd);
    // C's fresh-start init: a driver that cannot reach the daemon must not
    // serve.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "drv.blk.usb_storage");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
