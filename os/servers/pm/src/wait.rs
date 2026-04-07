//! Wait 切片
//!
//! 实现 wait/waitpid 系统调用的核心逻辑

use minix_ipc::Endpoint;

/// 等待子进程
///
/// # Arguments
/// * `parent` - 父进程 endpoint
/// * `pid` - 指定子进程 PID，-1 表示任意子进程
/// * `options` - 等待选项
///
/// # Returns
/// * `Ok((pid, status))` - 子进程 PID 和退出状态
pub fn sys_wait(parent: Endpoint, pid: i32, options: u32) -> Result<(i32, i32), WaitError> {
    // TODO: 实现 wait 逻辑
    // 1. 查找符合条件的子进程
    // 2. 如果有 zombie 子进程，立即返回
    // 3. 否则阻塞父进程，等待子进程退出
    todo!("wait implementation")
}

#[derive(Debug)]
pub enum WaitError {
    NoChild,
    InvalidEndpoint,
    Interrupted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_basic() {
        // TODO: 基础 wait 测试
    }
}
