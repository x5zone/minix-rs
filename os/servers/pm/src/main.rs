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
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:38）；宿主/测试侧保持
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

    // C: main.c:59-110 — 主循环（分发细节归 04）。run() 发散
    //（init.rs:396 `-> !`），尾表达式即 real_main 的函数尾。
    server.run()
}
