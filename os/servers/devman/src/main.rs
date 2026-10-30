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

        // C: main.c:77-89 — fill hooks + describe the root inode + `run_vtreefs`.
        // On the Rust side the hooks are already inlined into `Server`; the
        // root inode description is still `RootStat::devman_root()`.
        let root = RootStat::devman_root();
        let config = ServerConfig::devman_default(root);

        // A `run_vtreefs` initialization failure panics in C (vtreefs.c:16-33);
        // on the Rust side `Server::new` returns `Err(ENOMEM)`, and with no
        // recovery surface at startup it folds back into a panic (same as a
        // failed `IsServer::startup` in 01).
        let mut server = match minix_devman::server::Server::new(&config) {
            Ok(server) => server,
            Err(e) => panic!("devman: init failed: {e:?}"),
        };

        // Production transport: kernel IPC + SEF message reception + grant
        // read/write; the FS side follows VFS's fsdriver protocol
        // (transid/request number/reply shaping), and the DEVMAN side shares
        // that loop (C `fsdriver_task` + `fdr_other`).
        let mut transport = MinixTransport::new(SysKernel);
        server.run(&mut transport);

        // The loop returns only when message reception fails
        // (`Transport::next` yields None) — C panics here (fsdriver.c:92: a
        // broken transport has no recovery surface).
        panic!("devman: fsdriver loop ended (receive failed)");
    }
}
