//! The pckbd driver binary.
//!
//! C correspondence: `main()` (`pckbd.c:465-493`) announces the device type
//! through the input-driver library, then runs the receive-classify-dispatch
//! loop. Both halves come from the shared driver runtime; this binary names
//! the data-store key and builds the device.
//!
//! Announce seam: C's `inputdriver_announce` publishes the device-type mask
//! as a 32-bit value under `drv.inp.<label>` (`inputdriver.c:22-38`), which
//! differs from the character framework's endpoint-label publish that the
//! shared runtime's announce runs. The key spelling is faithful here; the
//! value-versus-endpoint payload of the announce is a transport detail that
//! rides on the input server's subscription reader (a closed edge3 concern),
//! so it is registered rather than faked.

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;
use minix_sys::inputdriver::announce_key;

fn main() {
    // 出生自证:C sef_startup 先 sys_whoami 填 sef_self_endpoint(sef.c:76-87,
    // 失败即 panic :81)。端点由内核引导表授予(引导期不经 RS——旧注失实);
    // 问询走无源 DirectKernelCallTransport,答案再喂带源的 IPC 传输
    //(tty NL6-A 同款)。
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("pckbd: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // C names the driver after its SEF label ("pckbd"); the shared
    // `announce_key` adds the input family's `drv.inp.` prefix.
    let key = announce_key("pckbd");
    let mut runtime = DriverRuntime::new(transport, key);
    let mut service = minix_driver_pckbd::init();
    // A fresh start needs no extra preparation: the state machines and
    // (empty) registration are built at service construction. The runtime
    // refuses the stateful kinds itself, so this callback only ever sees
    // the fresh kind.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
