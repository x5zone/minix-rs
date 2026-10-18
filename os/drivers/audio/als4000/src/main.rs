//! The ALS4000 driver binary.
//!
//! C: `main` runs the SEF startup (`sef_cb_init_fresh`: `pci_init`, the
//! device probe and reset, `drv_init`/`drv_init_hw` — `als4000.c`), then
//! the audio framework's task loop (`audio_fw.c:73-87`). The loop shell
//! and the RS birth handshake live in the shared driver runtime; this
//! binary finds the card and builds the service.
//!
//! The DMA allocator is the registered seam (`E-DMABUF`): with the unbound
//! allocator in place the card comes up, answers its three file nodes, and
//! refuses every DMA open honestly rather than programming an address it
//! does not own. The mixer node (control-only) opens without DMA.
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
use minix_driver_rt::transport::DriverTransport;

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
        .unwrap_or_else(|r| panic!("als4000: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);

    // C `pci_init` resolves the PCI driver by its label.
    let Some(pci) = transport.lookup_label("pci") else {
        return;
    };
    // C's fresh-start init: without the card there is nothing to serve.
    let Ok(probed) = minix_driver_als4000::probe::probe(&minix_sys::ipc::DirectTrapTransport, pci)
    else {
        return;
    };

    let mut service = minix_driver_als4000::init(probed.port);
    service.hooks_mut().set_irq(probed.irq);
    // C's fresh-start init: a card that cannot initialize must not serve.
    if service.init() < 0 {
        return;
    }

    // C announces the character key (`chardriver_announce`).
    let mut runtime = DriverRuntime::new(transport, "drv.chr.als4000");
    // `serve`: birth handshake then the receive loop forever; it returns
    // only when startup is refused or the transport dies.
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
