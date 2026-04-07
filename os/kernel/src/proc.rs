//! Kernel process management module

use minix_ipc::Endpoint;

/// 进程结构
pub struct Process {
    pub endpoint: Endpoint,
    pub pid: u32,
}

/// 创建进程
pub fn create_process() -> Process {
    // TODO: 实现进程创建
    Process {
        endpoint: Endpoint::NONE,
        pid: 0,
    }
}

/// 复制进程（fork）
pub fn copy_process(proc: &Process) -> Process {
    // TODO: 实现进程复制
    Process {
        endpoint: Endpoint::NONE,
        pid: proc.pid + 1,
    }
}
