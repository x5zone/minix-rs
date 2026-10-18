//! The VND (vnode disk) driver binary.
//!
//! C: `main` parses the instance number from the environment
//! (`vnd.c:583-590`), runs the SEF startup (`vnd_init` zeroes the state
//! and picks the instance, `vnd.c:540-565`), then the block loop
//! (`blockdriver_task`, `vnd.c:592-600`). The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary binds the
//! backing-file backend and builds the service.
//!
//! The backing file is configured later, by a `VNDIOCSET` from
//! `vndconfig(8)`: the unit starts unconfigured and answers ENXIO to
//! everything but the whole-disk minor until then (`vnd.c:66-105`).

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

fn main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap and feeds the
    // endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("vnd: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_vnd::init(minix_driver_vnd::backend::VfsBackend);

    // C publishes the block key (`blockdriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.blk.vnd");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies. C turns the same
    // receive failure into a log line and keeps looping; main returns and
    // the process exits.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
