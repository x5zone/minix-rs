//! The TI1225 CardBus controller driver binary.
//!
//! C: `main` runs the SEF startup (probe, bridge setup, socket masks,
//! one socket pass — `ti1225.c:87-240`), then loops printing every
//! message (`ti1225.c:46-64`). The birth handshake and the loop live in
//! the shared driver runtime; this binary builds the service.
//!
//! The runtime label is unused: C's startup announces nothing (this
//! driver has no data-store key and no client protocol), which is why
//! the loop is entered through the announce-free serve path.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("ti1225: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_ti1225::init();

    // C's fresh-start init: a machine without the controller must not
    // enter the loop.
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "ti1225");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a panic.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
