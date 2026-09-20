//! The tty driver binary.
//!
//! C: `main()` (`tty.c:146-238`) — SEF local startup (the birth handshake
//! with the restart server), then the receive-classify-dispatch loop. Both
//! halves come from the shared driver runtime; this binary only names the
//! data-store label and builds the device.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs:86` init 契约：no_std 模式 `_start` 自动调用、幂等），这里
// 无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字+Rust
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:38）；宿主/测试侧保持
// `()`——rustc 1.94 起 Termination 不再为 i32 实现，宿主 i32 main 即
// E0277（docker minix-ci:1.94 实测）。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main 原地消费返回 ()；0 即进程退出码（crt0 Consumer contract）。
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // 出生自证:C sef_startup 先 sys_whoami 填 sef_self_endpoint(sef.c:76-87,
    // 失败即 panic :81)。端点是内核引导过程按 BOOT_MODULE_PROC_NRS 授予的
    //(引导期不经 RS——旧注"由 RS 分配"系失实);应答读调用者自身内核槽
    //(misc.rs getinfo_whoami),与消息 m_source 无关,故问询走无源的
    // DirectKernelCallTransport(driver-rt safecopy 同款),答案再喂给
    // 带源的 IPC 传输。
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("tty: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut runtime = DriverRuntime::new(transport, "drv.chr.tty");
    let mut service = minix_driver_tty::init();
    // A fresh start needs no extra preparation: the line table is built at
    // service construction. The runtime refuses the stateful kinds itself,
    // so this callback only ever sees the fresh kind.
    //
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. There is
    // nothing a driver that cannot serve can do next, so main returns and
    // the process exits (C turns the same receive failure into a panic).
    // serve 只在启动被拒或传输死亡时返回（彼时 C 侧同款接收失败即
    // panic）；退出码 0 由 none 入口补（crt0 Consumer contract）。
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
