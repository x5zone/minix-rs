//! Minix-RS MIB — entry point.
//!
//! C: `main()` — `minix3/minix/servers/mib/main.c:433-492`.
//! See `notes/rewrite/fork-syscall-rewrite/10-stage-mib/01-mib-init-main.md`.

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
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
        let services = SysServices;

        // C: main loop — main.c:443-488: receive → triage → dispatch →
        // reply, forever; a broken transport dies at the documented
        // 64-turn bound (server.rs, the SCHED/DS-shared deviation from
        // C's panic-on-first-failure).
        Server::new(server, ipc, kernel, services).run();
    }
}
