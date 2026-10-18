//! The VBOX (VirtualBox guest) driver binary.
//!
//! C: `main` runs the SEF startup (`vbox_init`: find and reserve the
//! VMMDev device, register the interrupt, allocate the request page,
//! report the guest, and run the first time update — `vbox.c:88-140`),
//! then the loop that dispatches hardware and clock notifications plus
//! the VBOX requests (`vbox.c:180-193`). The loop shell and the RS birth
//! handshake live in the shared driver runtime; this binary finds the
//! device and builds the service.
//!
//! The runtime label is unused: C's startup announces nothing (the
//! `vbox` name in the C configuration is RS's own service label, not a
//! data-store publication), which is why the loop is entered through the
//! announce-free serve path.
//!
//! The PCI probe needs the PCI server's attribute passthrough, which is
//! that driver's own registered seam (it answers ENOSYS today), so on a
//! machine with the current PCI server this binary refuses to start
//! rather than fabricating a port. When the passthrough lands, the probe
//! and this binary need no change.
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
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`); the
    // answer rides the sourceless direct trap (the memory/tty/pckbd form)
    // and feeds the endpoint-carrying IPC transport built next.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("vbox: sys_whoami failed: {r}"));
    let mut transport = KernelTransport::new(who.endpoint);

    // C `pci_init` resolves the PCI driver by its label.
    let Some(pci) = transport.lookup_label("pci") else {
        return;
    };
    // C's fresh-start init: without the device there is nothing to serve.
    let Ok(probed) =
        minix_driver_vbox::backdoor::probe_device(&minix_sys::ipc::DirectTrapTransport, pci)
    else {
        return;
    };

    let backdoor = minix_driver_vbox::backdoor::KernelBackdoor::new(who.endpoint, probed.port);
    let mut service = minix_driver_vbox::init(backdoor);
    // The guest report must reach the device (C panics otherwise).
    if service.start().is_err() {
        return;
    }

    let mut runtime = DriverRuntime::new(transport, "vbox");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a panic.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
