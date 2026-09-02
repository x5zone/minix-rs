//! Minix-RS PM 入口。
//!
//! C 对应: `minix3/minix/servers/pm/main.c:49`（main）——启动链细节见
//! 01-pm-init-main.md，主循环分发见 04-ipc-dispatch.md。

use minix_pm::init::{BootParams, PmServer};

fn main() {
    // C: main.c:49-56 — main() → sef_local_startup() → sef_startup()
    // → sef_cb_init_fresh()。RS_INIT 握手归主循环（04），此处直接构造
    // 服务器并执行 init_fresh 等价初始化。
    //
    // 占位参数：真实启动路径由 sys_getmonparams/sys_getimage 填充
    // （minix-sys 落地后替换 BootParams::placeholder()）。
    let params = BootParams::placeholder();

    let mut server = PmServer::new(params);

    // C: sef_cb_init_fresh — main.c:130-243。
    server.init();

    // C: main.c:59-110 — 主循环（分发细节归 04）。
    server.run();
}
