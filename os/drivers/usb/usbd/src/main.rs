//! The USBD (USB host daemon) binary.
//!
//! C: `main` runs the SEF startup (`base/usbd.c`: the controller bring-up
//! and the device scan) and then serves the five `USB_RQ_*` requests
//! (`usb_server.c:731-758`). The loop shell and the RS birth handshake
//! live in the shared driver runtime; this binary builds the service.
//!
//! The controller seam is the registered piece: with the unbound
//! controller in place the daemon answers its protocol and refuses every
//! request block honestly rather than reporting a completion that never
//! happened.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_driver_usbd::hcd::HcdLeg;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("usbd: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usbd::init(minix_driver_usbd::hcd::UnboundHcd);
    // C's fresh-start init: a controller that cannot come up must not
    // serve (the daemon's own `hcd_init` gate).
    if service.hcd_mut().init().is_err() {
        return;
    }

    // C announces the daemon key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.usbd");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
