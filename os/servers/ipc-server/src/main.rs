//! Minix IPC server binary.
//!
//! Entry point for the IPC server process: build the server, run startup
//! registration, enter the event loop (never returns).
//!
//! C: `main` — main.c:216-284. Document `01-ipc-init-main.md`.

// In test builds, use the system allocator (the production allocator needs
// kernel-provided memory, unavailable under the test harness).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
    // The production transport (kernel `sef_receive_status` / `ipc_sendnb`)
    // lands with the kernel IPC bindings; until then the binary cannot run
    // outside tests. Keep the entry point as the wiring list so landing it
    // is a small, reviewable change (same deferral as the VM server's early
    // transport stub).
    #[cfg(not(test))]
    {
        // Wiring list (lands with the kernel transport):
        //   1. build the kernel transport,
        //   2. `IpcServer::new(transport, StubHandler)` (05-08 replace the stub),
        //   3. `server.init()` (startup registration — main.c:223-224),
        //   4. `server.run()` (main loop — main.c:227-280).
        panic!("IPC server production transport not landed yet (see server.rs)");
    }
}
