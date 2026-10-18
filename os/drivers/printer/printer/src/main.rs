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
