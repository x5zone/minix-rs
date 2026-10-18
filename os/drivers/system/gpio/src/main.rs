//! The GPIO driver binary.
//!
//! C: `main` fills the virtual-tree hooks and runs the filesystem
//! (`gpio.c:272-289`, `run_vtreefs`). The VFS message loop, the RS birth
//! handshake, and the request decode live in `minix-fs-rt`; this binary
//! contributes the server value (the pin tree and its hooks) and the
//! signal action.
//!
//! Board table seam: C publishes pins only on BBXM and BB boards, after
//! reading the machine identity and programming pad multiplexing. Neither
//! the board identification nor the pad configuration is wired here, so
//! the binary serves the empty root C builds on every other board.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs` init 契约：no_std 模式 `_start` 自动调用、幂等），这里无须
// 也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

extern crate alloc;

use alloc::boxed::Box;

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

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
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        let server = minix_driver_gpio::init();
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(minix_driver_gpio::signal_action),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            Err(e) => panic!("gpio: startup failed: {e}"),
        }
    }
}
