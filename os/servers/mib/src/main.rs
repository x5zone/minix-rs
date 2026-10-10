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

        // SD-11 U-5 迁移：把全局堆供给切到 VM_BRK 增长堆，初始断点 = 链接器
        // 镜像尾（`minix_rt::image_end`，C `brksize.S` 以 `_end` 播 `_brksize`
        // 的同一符号）。安装本身无 IPC（只切换分配器形态）；首次超出固定池的
        // 分配才真正发 VM_BRK。出生序上 MIB 晚于 VM（`rs/boot.rs` 装机表），
        // 安装时 VM 主循环已在收请求；EINVAL（镜像尾为 0 = 链接缺陷）fail-fast。
        // 与首迁 DS（T3）、二迁 SCHED（T3b）同法。
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("mib: install_vm_supplier(镜像尾) 不得失败");

        // SD-11 真机取证探针：安装后立即从 VM 腿取一页（8 字节 slab 请求触发
        // add_slab → supply_page → VM_BRK），失败即 fail-fast。成功打一行串口
        // 标记——VM_BRK 增长腿端到端贯通的 L1 证据；探针块随 free 回 slab 自由
        // 表（VM 侧页不回收：收缩 brk 从不 unmap，同 C）。
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "mib: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] 报告通道自坏时 park 兑底（panic.c:66）
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: mib-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C: main loop — main.c:443-488: receive → triage → dispatch →
        // reply, forever; a broken transport dies at the documented
        // 64-turn bound (server.rs, the SCHED/DS-shared deviation from
        // C's panic-on-first-failure).
        Server::new(server, ipc, kernel, services).run();
    }
}
