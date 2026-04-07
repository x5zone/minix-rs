//! Signal 切片
//!
//! 实现 signal 系统调用的核心逻辑

use minix_ipc::Endpoint;

/// 发送信号
///
/// # Arguments
/// * `target` - 目标进程 endpoint
/// * `sig` - 信号编号
pub fn sys_kill(target: Endpoint, sig: i32) -> Result<(), SignalError> {
    // TODO: 实现 kill 逻辑
    // 1. 验证权限
    // 2. 将信号加入目标进程的 pending 集合
    // 3. 如果目标进程在睡眠，唤醒它
    todo!("kill implementation")
}

/// 设置信号处理函数
///
/// # Arguments
/// * `proc` - 进程 endpoint
/// * `sig` - 信号编号
/// * `handler` - 处理函数
pub fn sys_sigaction(
    proc: Endpoint,
    sig: i32,
    handler: SignalHandler,
) -> Result<(), SignalError> {
    // TODO: 实现 sigaction 逻辑
    todo!("sigaction implementation")
}

pub enum SignalHandler {
    Default,
    Ignore,
    Custom(usize), // 函数指针地址
}

#[derive(Debug)]
pub enum SignalError {
    InvalidSignal,
    InvalidEndpoint,
    NoPerm,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_basic() {
        // TODO: 基础 signal 测试
    }
}
