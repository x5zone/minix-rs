//! The printer driver binary.
//!
//! C: `main` runs `chardriver_task(&printer_tab)`
//! (`printer.c:109-116`) — SEF local startup, the fresh-start probe, then
//! the receive-classify-dispatch loop. The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary names the
//! data-store key and builds the service.
//!
//! Boot-table seam: the printer driver is not yet a boot module — the
//! manifest entry and the freestanding dual entry ride the boot-table
//! registration (one-line/one-stage lines). Port traffic rides the
//! printer crate's hardware seam; until the port backend lands the seam
//! reports no parallel port and the driver honestly does not start.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("printer: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_printer::init();

    // C's fresh-start init probes the parallel port and reports `ENODEV`
    // when there is none (`printer.c:145-150`). A driver that cannot be
    // born must not enter the loop; returning here is the same fate RS
    // observes from a failed SEF startup.
    if service.probe().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "drv.chr.printer");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
