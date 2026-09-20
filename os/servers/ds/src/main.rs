//! Minix-RS data store — entry point.
//!
//! C: `main()` — `minix3/minix/servers/ds/main.c:28-88`.
//! See `notes/rewrite/fork-syscall-rewrite/07-stage-ds/01-ds-init-main.md`.

// 按目标分形态（init 先例，`os/commands/sbin/init/src/main.rs:27-35`）：
// 宿主与测试构建保持 std 形态——rustc 对 hosted target 一律传 Scrt1.o
// （其 `_start` 需要 `main` + `__libc_start_main`），no_std + no_main 在
// 宿主链接必断；`target_os = "none"` 侧才是 freestanding 真身（boot 装
// 机模块）。`#[unsafe(no_mangle)]` 满足 minix-rt crt0 出生链按名解
// `main` 的 Consumer contract（`os/libs/minix-rt/src/crt0.rs:40`：
// 名字 + Rust ABI + `-> i32` 三要素，返回值经 stage-6 exit 原样成为
// 进程退出码）。
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号（init 不同——它 main 里直接调 `minix_rt::init()`），未引用的
// rlib 会被整体丢弃，`_start`/`#[panic_handler]`/`#[global_allocator]`
// 三件随之消失（panic-halt 模式的 `extern crate` 保留语义）。出生链
// `_start` 在 main 前已自动完成 rt 初始化（`lib.rs:86` init 契约：
// no_std 模式 `_start` 自动调用、幂等），这里无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字+Rust
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:40）；宿主/测试侧保持
// `()`——rustc 1.94 起 Termination 不再为 i32 实现，宿主 i32 main 即
// E0277（docker minix-ci:1.94 实测）。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() -> ! {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        use minix_ds::server::{DsServer, SysIpc, SysKernel};
        use minix_sys::ipc::DirectTrapTransport;

        // C: env_setargs + sef_local_startup() — main.c:38-39. The fresh
        // anchor (`sef_cb_init_fresh`, store.c:254-282) resets both tables
        // and shadows the RS boot table; the rproctab *decode* (raw
        // `rprocpub` bytes → `BootService` list) is pinned at edge
        // E-RSWIRE, so the fresh run starts from the reset state — the
        // exact state `sef_cb_init_fresh` itself occupies after its reset
        // half, before the shadow loop.
        let mut server = DsServer::new();
        let mut ipc = SysIpc::new(DirectTrapTransport);
        let mut kernel = SysKernel;

        // C: main loop — main.c:42-86, owned by server.rs: receive →
        // triage → dispatch → reply, forever; a broken transport dies at
        // the documented 64-turn bound (server.rs, the SCHED-shared
        // deviation from C's panic-on-first-failure).
        server.run(&mut ipc, &mut kernel);
    }
    // 测试臂编译排除后本函数体为空，以 unreachable 保持 `-> !` 形状
    //（测试壳直接驱动库，不进本入口）。
    #[cfg(test)]
    unreachable!("bin entry unused in test builds")
}
