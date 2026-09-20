//! The tty driver binary.
//!
//! C: `main()` (`tty.c:146-238`) — SEF local startup (the birth handshake
//! with the restart server), then the receive-classify-dispatch loop. Both
//! halves come from the shared driver runtime; this binary only names the
//! data-store label and builds the device.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_types::Endpoint;

fn main() {
    // The self endpoint is assigned by the restart server during SEF
    // startup; until that assignment lands (real boot, edge E5) this is the
    // same `Endpoint::NONE` seam the input server carries.
    let self_endpoint = Endpoint::NONE;
    let transport = KernelTransport::new(self_endpoint);
    let mut runtime = DriverRuntime::new(transport, "drv.chr.tty");
    let mut service = minix_driver_tty::init();
    // A fresh start needs no extra preparation: the line table is built at
    // service construction. The runtime refuses the stateful kinds itself,
    // so this callback only ever sees the fresh kind.
    //
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. There is
    // nothing a driver that cannot serve can do next, so main returns and
    // the process exits (C turns the same receive failure into a panic).
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
