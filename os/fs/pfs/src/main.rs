//! Pipe file server entry point.
//!
//! C: `main` — `minix3/minix/fs/pfs/pfs.c:422-451` (`env_setargs` +
//! `sef_local_startup` + `fsdriver_task(&pfs_table)`). The common runtime
//! shape lives in `minix-fs-rt`; this entry contributes the server value
//! (in-memory table, no block device — PFS never touches a disk) and the
//! two hooks.
//!
//! Deliberate stand-in, honest rather than fake: the signal hook ignores
//! notifications for now. The C handler's termination path needs the
//! signal-manager pull channel whose kernel wrappers are not wired yet;
//! the decision point stays in one hook. PFS has no block-device face at
//! all, so the E-FSBDEV stand-in does not apply here.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;


// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs:86` init 契约：no_std 模式 `_start` 自动调用、幂等），这里
// 无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// hooks 的 Box::new 裸用：none 侧由 alloc 供给（宿主/测试走 std prelude）。
#[cfg(all(not(test), target_os = "none"))]
extern crate alloc;

#[cfg(all(not(test), target_os = "none"))]
use alloc::boxed::Box;

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
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        let server = minix_fs_pfs::init();
        // Birth: the C fresh-install sequence builds the inode table inside
        // the server value (pfs.c init half); the callback confirms ready.
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(|_pending| false),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            Err(e) => panic!("pfs: startup failed: {e}"),
        }
    }
}
