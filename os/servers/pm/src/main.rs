//! Minix-RS PM 入口。
//!
//! C 对应: `minix3/minix/servers/pm/main.c:49`（main）——启动链细节见
//! 01-pm-init-main.md，主循环分发见 04-ipc-dispatch.md。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs:86` init 契约：no_std 模式 `_start` 自动调用、幂等），这里
// 无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_pm::init::{BootParams, PmServer};

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
    // C: main.c:49-56 — main() → sef_local_startup() → sef_startup()
    // → 阻塞等 RS 的 RS_INIT（sef.c:127-141）→ sef_cb_init_fresh
    //（main.c:130-243，内含与 VFS 的 VFS_PM_INIT 同步 main.c:220-236）
    // → process_init 尾部回 RS_INIT+result（sef_init.c:113-117）。
    // 本树把 fresh 体放在构造后的 server.init()；出生请求的接收与
    // 应答由 run() 循环内的出生臂（E-BIRTHFACE，NS1）承接。
    //
    // 真实启动获取（D-02 消费半 / S42 ②）：C main.c:167-176/238 的
    // sys_getmonparams + sys_getimage + sys_hz。任一失败即停车——
    // C 对应 panic（"get monitor params failed"/"couldn't get image
    // table"），半真数据不得进启动契约。
    let params = BootParams::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|e| panic!("PM boot: kernel getinfo failed: errno {e}"));

    let mut server = PmServer::new(params);

    // C: sef_cb_init_fresh — main.c:130-243。
    server.init();

    // SD-11 U-5 迁移（第六台）：PM 是进程表/fork/exec 工作集的服务（src/ 内
    // 82 处 Vec/Box，链接 alloc-global）。把全局堆供给从焊死固定池切到 VM_BRK
    // 增长堆。安装点在 init()（fresh 体已建表，仍在固定池）之后、`run()` 主
    // 循环之前——此刻 VM（boot 序第 2 位，早于 PM 第 3 位）主循环已在收请求，
    // `install_vm_supplier` 内的 VM_BRK 直连有应答；EINVAL fail-fast；与
    // DS(T3)/SCHED(T3b)/MIB/VFS/PFS 同法，整块 #[cfg(target_os="none")] 不影响宿主测。
    #[cfg(target_os = "none")]
    minix_rt::install_vm_supplier(minix_rt::image_end())
        .expect("pm: install_vm_supplier(镜像尾) 不得失败");

    #[cfg(target_os = "none")]
    {
        let probe = minix_rt::alloc(8);
        assert!(!probe.is_null(), "pm: vm-brk probe 分配失败");
        minix_rt::free(probe);
        use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
        #[allow(clippy::let_underscore_must_use)] // [SD19-gate] 报告通道自坏时 park 兑底（panic.c:66）
        let _ = sys_diagctl_write(
            &DirectKernelCallTransport,
            "nk4a: pm-vmbrk probe ok pages=1 via=VM_BRK\n",
        );
    }

    // C: main.c:59-110 — 主循环（分发细节归 04）。run() 发散
    //（init.rs:398 `-> !`），尾表达式即 real_main 的函数尾。
    server.run()
}
