//! Minix-RS Information Server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/is/main.c:31-71`.
//! See `notes/rewrite/fork-syscall-rewrite/08-stage-is/01-is-init-main.md`.

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
        use minix_is::{IsServer, UnimplementedFkeyCtl, sef::UnimplementedTransport};

        // Production transports (sef_receive/ipc_send/_taskcall wiring) land
        // with the minix-sef/minix-sys implementation (01-is-init-main.md
        // §3 D2 + 02-is-fkey-contract.md §3 D3, forward references). Until
        // then, fail closed instead of running a loop that can never receive.
        let mut server = IsServer::new(UnimplementedTransport, UnimplementedFkeyCtl);

        // C: sef_local_startup() + boot anchor — main.c:40-41,94-102.
        // Boot failure is fatal (C panics on startup paths); do not enter
        // the main loop on an incomplete boot.
        match server.startup() {
            Ok(_) => server.run(),
            Err(e) => panic!("IS boot failed: {e:?}"),
        }
    }
}
