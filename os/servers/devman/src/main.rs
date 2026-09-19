//! Minix DEVMAN Server.
//!
//! Device manager service process.
//! Entry point for the devman server binary (doc 01-devm-init-main).
//!
//! C: `minix3/minix/servers/devman/main.c:70-91` — fill hooks, describe the
//! root inode, call `run_vtreefs`. The VTreeFS main loop itself (`02`) and
//! the device tree build (`04`) land in later modules; this binary wires
//! the 01-level defaults together.

// In test builds, use the system allocator.
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
    #[cfg(not(test))]
    {
        use minix_devman::ipc::minix::{MinixTransport, SysKernel};
        use minix_devman::{RootStat, ServerConfig};

        // C: main.c:77-89 — 填 hooks + 描述根 inode + `run_vtreefs`。
        // Rust 侧 hooks 已内联进 `Server`（DM-P1-2/P1-5），根 inode 描述
        // 仍是 `RootStat::devman_root()`。
        let root = RootStat::devman_root();
        let config = ServerConfig::devman_default(root);

        // C: `run_vtreefs` 的初始化失败在 C 里 panic（vtreefs.c:16-33）；
        // Rust 侧 `Server::new` 回 `Err(ENOMEM)`，启动期无恢复面，折回
        // panic（与 01 的 `IsServer::startup` 失败同款）。
        let mut server = match minix_devman::server::Server::new(&config) {
            Ok(server) => server,
            Err(e) => panic!("devman: init failed: {e:?}"),
        };

        // 生产传输：内核 IPC + SEF 收包 + grant 读写；FS 面按 VFS 的
        // fsdriver 协议（transid/请求号/回复成型），DEVMAN 面同循环
        // （C `fsdriver_task` + `fdr_other`，DM-P1-2）。
        let mut transport = MinixTransport::new(SysKernel);
        server.run(&mut transport);

        // 循环只在收包失败时返回（`Transport::next` 给 None）——C 在这里
        // panic（fsdriver.c:92：传输损坏无恢复面）。
        panic!("devman: fsdriver loop ended (receive failed)");
    }
}
