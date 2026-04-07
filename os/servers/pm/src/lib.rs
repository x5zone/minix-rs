//! Minix-RS Process Manager (PM)
//!
//! 进程管理器，负责：
//! - 进程创建与销毁 (fork, exec, exit)
//! - 进程状态管理
//! - 信号处理 (signal)
//! - 等待子进程 (wait)

pub mod fork;
pub mod exec;
pub mod exit;
pub mod signal;
pub mod wait;

/// PM 初始化
pub fn init() {
    // TODO: 初始化进程表
}

/// PM 主循环
pub fn run() -> ! {
    loop {
        // TODO: 处理消息
    }
}
