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
    #[cfg(not(test))]
    {
        // Four-step wiring (main.c:216-284):
        use minix_ipc_server::boundary::{SysBoundary, SysEventLoopTransport};
        use minix_ipc_server::server::IpcServer;
        use minix_ipc_server::service::IpcService;

        // 1. Production boundary (trap-backend wrapper) + kernel event-loop transport.
        let boundary = SysBoundary::new();
        let transport = SysEventLoopTransport::new();
        // 2. Service assembly (decision layer + boundary).
        let server = IpcServer::new(transport, IpcService::new(boundary));
        // 3. Startup registration (main.c:223-224; SEF callback registration comes with the SEF library wiring).
        server.init();
        // 4. Event loop (main.c:227-280; never returns).
        server.run();
    }
}
