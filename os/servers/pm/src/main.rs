//! Minix-RS PM 入口。
//!
//! C 对应: `minix3/minix/servers/pm/main.c:49`（main）——启动链细节见
//! 01-pm-init-main.md，主循环分发见 04-ipc-dispatch.md。

use minix_pm::init::{BootParams, PmServer};

fn main() {
    // C: main.c:49-56 — main() → sef_local_startup() → sef_startup()
    // → sef_cb_init_fresh()。本树 C 的 PM 启动只与 VFS 同步
    //（VFS_PM_INIT，main.c:220-236），无 RS_INIT 握手；此处直接构造
    // 服务器并执行 init_fresh 等价初始化。
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

    // C: main.c:59-110 — 主循环（分发细节归 04）。
    server.run();
}
