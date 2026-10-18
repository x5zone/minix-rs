//! The USB hub driver binary.
//!
//! C: `main` initializes DDEkit and registers the driver with the USB host
//! daemon (`ddekit_usb_init(&usb_driver, ...)`, `usb_hub.c:219-330`), then
//! the hub thread runs the polling rounds. The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary builds the
//! service.
//!
//! The daemon seam is the registered piece: with the unbound seam in place
//! the driver registers nothing and refuses startup honestly rather than
//! polling a hub it cannot reach.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("usb_hub: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usb_hub::init(minix_driver_usb_hub::usbd::UnboundUsbd);
    // C's fresh-start init: a hub driver that cannot reach the daemon must
    // not serve.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "drv.chr.usb_hub");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
