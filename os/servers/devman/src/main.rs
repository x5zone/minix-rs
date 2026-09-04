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
        use minix_devman::{RootStat, ServerConfig};

        // C: main.c:77-80 — fill in the hooks (wired inside devman_default).
        // C: main.c:82-86 — describe the root inode.
        let root = RootStat::devman_root();
        // C: main.c:89 — run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE).
        let _config = ServerConfig::devman_default(root);

        // The VTreeFS event loop (02) takes over here; until it lands,
        // park the service instead of exiting (RS expects a live endpoint).
        // 02 STATUS: the framework data plane is done (`vtreefs::VTreeFs`
        // + injected `Transport`, fully tested). What remains is the
        // *production transport* (kernel IPC via `minix-sys`, still
        // `todo!()` — calling it would panic, so park, don't call).
        // P1-6 is narrowed to transport wiring; see 02 §4.4.
        // NOTE: replaced by `VTreeFs::run(MinixTransport)` in transport step.
        loop {
            core::hint::spin_loop();
        }
    }
}
