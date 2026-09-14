//! Minix-RS data store — entry point.
//!
//! C: `main()` — `minix3/minix/servers/ds/main.c:28-88`.
//! See `notes/rewrite/fork-syscall-rewrite/07-stage-ds/01-ds-init-main.md`.

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
        use minix_ds::server::{DsKernel, DsServer, SysIpc, SysKernel};
        use minix_sys::ipc::DirectTrapTransport;

        // C: env_setargs + sef_local_startup() — main.c:38-39. The fresh
        // anchor (`sef_cb_init_fresh`, store.c:254-282) resets both tables
        // and shadows the RS boot table; the rproctab *decode* (raw
        // `rprocpub` bytes → `BootService` list) is pinned at edge
        // E-RSWIRE, so the fresh run starts from the reset state — the
        // exact state `sef_cb_init_fresh` itself occupies after its reset
        // half, before the shadow loop.
        let mut server = DsServer::new();
        let mut ipc = SysIpc::new(DirectTrapTransport);
        let mut kernel = SysKernel;

        // C: main loop — main.c:42-86, owned by server.rs: receive →
        // triage → dispatch → reply, forever; a broken transport dies at
        // the documented 64-turn bound (server.rs, the SCHED-shared
        // deviation from C's panic-on-first-failure).
        server.run(&mut ipc, &mut kernel);
    }
}
