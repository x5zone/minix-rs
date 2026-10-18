//! The USBD (USB host daemon) binary.
//!
//! C: `main` runs the SEF startup (`base/usbd.c`: the controller bring-up
//! and the device scan) and then serves the five `USB_RQ_*` requests
//! (`usb_server.c:731-758`). The loop shell and the RS birth handshake
//! live in the shared driver runtime; this binary builds the service.
//!
//! The controller seam is the registered piece: with the unbound
//! controller in place the daemon answers its protocol and refuses every
//! request block honestly rather than reporting a completion that never
//! happened.
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
use minix_driver_usbd::hcd::HcdLeg;

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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("usbd: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_usbd::init(minix_driver_usbd::hcd::UnboundHcd);
    // C's fresh-start init: a controller that cannot come up must not
    // serve (the daemon's own `hcd_init` gate).
    if service.hcd_mut().init().is_err() {
        return;
    }

    // C announces the daemon key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.usbd");
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
