//! Minix-RS Process Manager (PM)
//!
//! 进程管理器，负责：
//! - 进程创建与销毁 (fork, exec, exit)
//! - 进程状态管理
//! - 信号处理 (signal)
//! - 等待子进程 (wait)
//!
//! # 架构说明
//!
//! 根据 Minix3 微内核设计，PM 拥有私有的进程表 (`mproc`)，
//! 与 VM、VFS、Kernel 的进程表通过 `endpoint` 关联。
//!
//! # 为什么 MProc 放在 PM crate 而不是 minix-types？
//!
//! 1. **职责隔离**: MProc 包含大量仅 PM 关心的私有逻辑（信号处理、父子进程树等）
//! 2. **不变量保护**: 状态转换逻辑绑定了 PM 内部复杂逻辑，放在公共库会破坏不变量
//! 3. **微内核原则**: 遵循"知识最小化"原则，其他服务不需要了解 PM 的内部实现
//!
//! # 模块结构
//!
//! - `mproc`: PM 进程表模块（私有）
//! - `fork`: fork 系统调用入口
//! - `exec`: exec 系统调用
//! - `exit`: exit 系统调用
//! - `signal`: 信号处理
//! - `wait`: 等待子进程

pub mod mproc;
pub mod fork;
pub mod exec;
pub mod exit;
pub mod signal;
pub mod wait;

pub use mproc::*;

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
