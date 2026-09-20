//! The memory driver binary.
//!
//! C: `main()` (`memory.c:99-108`) — SEF local startup (the birth handshake
//! with the restart server), then the receive loop that routes every message
//! to the block or character framework. Both halves of the shell come from
//! the shared driver runtime; this binary names the two data-store labels
//! (the memory driver announces on both families, `memory.c:160-161`) and
//! builds the device.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_types::Endpoint;

fn main() {
    // The self endpoint is assigned by the restart server during SEF
    // startup; until that assignment lands (real boot, edge E5) this is the
    // same `Endpoint::NONE` seam the tty binary and input server carry.
    let self_endpoint = Endpoint::NONE;
    let transport = KernelTransport::new(self_endpoint);
    // A dual-framework driver publishes both family keys at announce: the
    // character label first, then the block label (C runs
    // `chardriver_announce` and `blockdriver_announce` back to back in
    // `sef_cb_init_fresh`).
    let mut runtime =
        DriverRuntime::new(transport, "drv.chr.memory").also_announce("drv.blk.memory");
    let mut service = minix_driver_memory::init();
    // A fresh start needs no extra preparation: both faces and their tables
    // are built at service construction. The runtime refuses the stateful
    // kinds (a restart must preserve RAM-disk contents, which the birth
    // handshake does not yet model) itself, so this callback only ever sees
    // the fresh kind.
    //
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. There is
    // nothing a driver that cannot serve can do next, so main returns and
    // the process exits (C turns the same receive failure into a panic).
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
