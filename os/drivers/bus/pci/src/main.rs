//! The PCI bus driver binary.
//!
//! C: `main` → `chardriver_task(&driver)` (`main.c:721-738`) plus the
//! typed `BUSC_PCI_*` dispatcher reached through the same receive loop.
//! The loop shell and the RS birth handshake live in the shared driver
//! runtime; this binary names the data-store key and builds the service.
//!
//! Boot-table seam: the PCI driver is not yet a boot module — the
//! manifest entry and the freestanding dual entry ride the boot-table
//! registration (one-line/one-stage lines). The configuration-space
//! backend rides the port-IO/chipset seam; until it lands the probe
//! refuses to start, so the service never claims an empty bus.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("pci: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_pci::init();

    // A driver whose configuration space cannot answer must not enter the
    // loop; returning here is the same fate RS observes from a failed SEF
    // startup.
    if service.probe().is_err() {
        return;
    }
    // C enumerates at startup; the bus list comes from the bridge walk
    // this crate does not model yet, so the service walks bus zero.
    service.scan(&[0]);

    let mut runtime = DriverRuntime::new(transport, "drv.chr.pci");
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
