//! The readclock driver binary.
//!
//! C: `main()` (`readclock.c:41-131`) — the service loop over the RTCDEV
//! protocol family: gate, fetch/store the caller's `struct tm`, answer
//! the general reply. The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary names the service label and
//! builds the device.
//!
//! Boot-table seam: the readclock driver is not yet a boot module — the
//! manifest entry, the freestanding dual entry, and the minix-rt link
//! reference ride the boot-table registration (one-line/one-stage lines).
//! The chip behind the face is [`minix_driver_readclock::clock::NullClock`]
//! until an architecture clock lands; every operation then answers `EIO`
//! honestly instead of fabricating a time.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("readclock: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // The boot configuration's service name (`minix3/etc/system.conf:257`
    // lists `readclock.drv`); C relies on the boot-time label, and this
    // announce publishes the same key through the shared runtime.
    let mut runtime = DriverRuntime::new(transport, "readclock.drv");
    let mut service = minix_driver_readclock::init();
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. C turns
    // the same receive failure into a panic; main returns and the process
    // exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
