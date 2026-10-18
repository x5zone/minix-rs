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
