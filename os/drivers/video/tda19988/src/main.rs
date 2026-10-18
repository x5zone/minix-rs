//! The TDA19988 driver binary.
//!
//! C: `main` parses the bus/address pairs from the environment, runs the
//! SEF startup (two bus lookups and reservations — `tda19988.c:850-960`),
//! then `blockdriver_task(&tda19988_tab)` (`tda19988.c:1006`). The loop
//! shell and the RS birth handshake live in the shared driver runtime;
//! this binary names the data-store keys and builds the service.
//!
//! Registered seams: the environment plumbing (C reads `cec_bus`,
//! `cec_address`, `hdmi_bus`, and `hdmi_address` from `-args`) is fixed
//! to the documented defaults; the bus-restart subscriptions ride the
//! data-store client seam.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("tda19988: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_tda19988::init();

    // C's fresh-start init: a driver without both i2c interfaces must not
    // enter the loop.
    if service.start(&mut transport).is_err() {
        return;
    }

    // C publishes the block-driver key (`blockdriver_announce`) and the
    // i2c driver key (`i2cdriver_announce`, `drv.i2c.<name>`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.tda19988")
        .also_announce("drv.i2c.tda19988");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a log line and keeps looping; main
    // returns and the process exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
