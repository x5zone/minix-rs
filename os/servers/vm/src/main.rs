//! Minix VM Server
//!
//! 虚拟内存管理器服务进程。

use minix_vm::VmProcTable;

/// VM 服务全局状态
struct VmServer {
    proc_table: VmProcTable,
}

impl VmServer {
    fn new() -> Self {
        Self {
            proc_table: VmProcTable::new(),
        }
    }

    fn init(&mut self) {
        // TODO: 初始化 VM 服务
        // - 初始化进程表
        // - 注册 IPC 端点
        // - 初始化内存管理
    }

    fn run(&mut self) {
        // TODO: 主循环
        // - 接收 IPC 消息
        // - 处理内存管理请求
        loop {
            // 暂时空转
        }
    }
}

fn main() {
    let mut server = VmServer::new();
    server.init();
    server.run();
}
