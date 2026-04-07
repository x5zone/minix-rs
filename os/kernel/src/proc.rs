//! Kernel process management module

pub use minix_types::{Endpoint, Process};

/// 创建进程
pub fn create_process() -> Process {
    Process::default()
}

/// 复制进程（fork）
pub fn copy_process(proc: &Process) -> Process {
    let mut new_proc = proc.clone();
    new_proc.identity.id.pid = proc.identity.id.pid + 1;
    new_proc
}
