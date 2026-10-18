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
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs` init 契约：no_std 模式 `_start` 自动调用、幂等），这里无须
// 也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字 + Rust
// ABI + `-> i32`，`os/libs/minix-rt/src/crt0.rs`）；宿主与测试侧保持
// `()`——rustc 1.94 起 Termination 不再为 i32 实现。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
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
