//! The TPS65217 PMIC driver binary.
//!
//! C: `main` parses the bus and address from the environment, runs the
//! SEF startup (bus lookup, reservation, chip check, interrupt setup,
//! power-off enable — `tps65217.c:267-320`), then the notification-only
//! loop (`tps65217.c:332-395`). The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary names the
//! data-store key and builds the service.
//!
//! Registered seams: the environment plumbing (C reads `bus` and
//! `address` from `-args`) is fixed to the documented defaults
//! `BUS_NUMBER` and `DEVICE_ADDRESS`; the bus-restart subscription rides
//! the data-store client seam.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("tps65217: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_tps65217::init();

    // C's fresh-start init: a driver without a bus, a reservation, or a
    // recognized chip must not enter the loop.
    if service.start(&mut transport).is_err() {
        return;
    }

    // C publishes the i2c driver key (`i2cdriver_announce`,
    // `drv.i2c.<name>`); there is no character-driver side.
    let mut runtime = DriverRuntime::new(transport, "drv.i2c.tps65217");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a log line and keeps looping; main
    // returns and the process exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
