//! Minix-RS scheduler — entry point.
//!
//! C: `main()` — `minix3/minix/servers/sched/main.c:22-96`.
//! See `rewrite-notes/06-stage-sched/01-sched-init-main.md`
//! (the startup order) and `02-sched-message-surface.md` (the loop's turn).

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
        use minix_sched::cpu::MachineTopology;
        use minix_sched::kernel_api::transport::{KernelApi, KernelIpcTransport, SysKernelApi};
        use minix_sched::sef::init_fresh;
        use minix_sched::server::SchedServer;

        // C: sef_local_startup() — main.c:111-121. The two init names are
        // `SchedInitKind::{Fresh, RestartStateful}`; restart needs no sched
        // code (libsef generic), so there is no registration table to build
        // here and the fresh path below runs unconditionally.

        // C: sys_getmachine(&machine) — main.c:130. A failed query is fatal,
        // exactly as C `panic`s (main.c:131) — never enter the loop
        // half-knowing the machine.
        let mut kernel = SysKernelApi::new();
        let ready = kernel
            .get_machine()
            .map(init_fresh)
            .unwrap_or_else(|errno| panic!("couldn't get machine info: {errno}"));

        // The ready token carries the machine; the server is born around it
        // (01 D3: no boot, no wait — the token is the order, not a formality).
        let mut server = SchedServer::new(MachineTopology {
            processors_count: ready.machine.processors_count,
            bsp_id: ready.machine.bsp_id,
        });

        // C: init_scheduling() — main.c:133, owned by 11-balance-queues.md:
        // five seconds times the clock rate, armed once. C panics on a
        // failed arm (schedule.c:340-341); the Err dies here, at the binary
        // layer, where every startup failure already dies (01 D2).
        server
            .init_scheduling(&mut kernel)
            .unwrap_or_else(|errno| panic!("sys_setalarm failed: {errno}"));

        // SD-11 T3b 第二迁安装点（首迁 DS 同法，`minix_rt::install_vm_supplier`）:
        // 把全局堆供给切到 VM_BRK 增长堆，初始断点 = 链接器镜像尾
        //（C `brksize.S` 以 `_end` 播 `_brksize` 的同一符号）。安装本身
        // 无 IPC；首次超出固定池的分配才真正发 VM_BRK。出生序上 SCHED
        // 晚于 VM（`rs/boot.rs` 装机表），安装时 VM 主循环已在收请求；
        // EINVAL（镜像尾为 0 = 链接缺陷）fail-fast。
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("sched: install_vm_supplier(镜像尾) 不得失败");

        // SD-11 T3b 真机取证探针（DS 同法）：8 字节 slab 请求强制走
        // `add_slab`→`supply_page`→`VM_BRK`，成功打一行串口标记——
        // VM_BRK 腿对更早出生服务的贯通证据；探针块随 free 回 slab
        // 自由表（VM 侧页不回收：收缩 brk 从不 unmap，同 C）。
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "sched: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            // [SD19-gate] 报告通道自坏时 park 兜底（panic.c:66）
            #[allow(clippy::let_underscore_must_use)]
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: sched-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C: main loop — main.c:35-96, owned by 02-sched-message-surface.md:
        // receive → dispatch → reply, forever (02's `run` adds the
        // transport-failure bound; see server.rs for the documented
        // deviation from C's panic-on-first-failure).
        server.run(&KernelIpcTransport::new(), &mut kernel);
    }
    #[cfg(test)]
    unreachable!("bin entry unused in test builds")
}
