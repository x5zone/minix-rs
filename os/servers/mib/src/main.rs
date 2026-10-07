//! Minix-RS MIB — entry point.
//!
//! C: `main()` — `minix3/minix/servers/mib/main.c:433-492`.
//! See `rewrite-notes/10-stage-mib/01-mib-init-main.md`.

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

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字+Rust
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:40）；宿主/测试侧保持
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
        use minix_mib::server::{MibServer, Server, SysIpc};
        use minix_mib::transport::{SysServices, SysTransport};

        // C: mib_startup() — main.c:415-431. The fresh anchor builds the
        // static tree (`MibTree::init`, arena.rs) and resets the remote
        // slot table; the restart anchor rebuilds it (sef.rs).
        let server = MibServer::new();
        let ipc = SysIpc::default();
        let kernel = SysTransport;
        let services = SysServices::default();

        // C: main loop — main.c:443-488: receive → triage → dispatch →
        // reply, forever; a broken transport dies at the documented
        // 64-turn bound (server.rs, the SCHED/DS-shared deviation from
        // C's panic-on-first-failure).
        Server::new(server, ipc, kernel, services).run();
    }
}
