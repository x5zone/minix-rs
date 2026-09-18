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
        // S26 四步接线(main.c:216-284):
        use minix_ipc_server::boundary::{SysBoundary, SysEventLoopTransport};
        use minix_ipc_server::server::IpcServer;
        use minix_ipc_server::service::IpcService;

        // 1. 生产边界(trap 后端包装)+ 内核事件循环传输。
        let boundary = SysBoundary::new();
        let transport = SysEventLoopTransport::new();
        // 2. 服务装配(判定层 + 边界)。
        let server = IpcServer::new(transport, IpcService::new(boundary));
        // 3. 启动登记(main.c:223-224;SEF 回调注册随 SEF 库接线)。
        server.init();
        // 4. 事件循环(main.c:227-280;永不返回)。
        server.run();
    }
}
