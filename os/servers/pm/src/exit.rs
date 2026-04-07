//! Exit 切片
//!
//! 实现 exit 系统调用的核心逻辑

use minix_ipc::Endpoint;

/// 进程退出
///
/// # Arguments
/// * `proc` - 进程 endpoint
/// * `status` - 退出状态
pub fn sys_exit(proc: Endpoint, status: i32) -> Result<(), ExitError> {
    // TODO: 实现 exit 逻辑
    // 1. 释放资源
    // 2. 通知父进程（通过 signal 或 wait）
    // 3. 标记为 zombie（如果有父进程等待）
    // 4. 调度其他进程
    todo!("exit implementation")
}

#[derive(Debug)]
pub enum ExitError {
    InvalidEndpoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_basic() {
        // TODO: 基础 exit 测试
    }
}
