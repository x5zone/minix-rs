//! PM 进程表模块 (mproc)
//!
//! 这是 Minix3 `mproc` 结构体的 Rust 实现，包含 PM 私有的进程管理逻辑。
//!
//! # 架构说明
//!
//! 根据 Minix3 微内核设计，进程表分布在多个服务中：
//! - **PM/mproc**: 进程管理、信号、权限（本模块）
//! - **VM/vmproc**: 虚拟内存、页表
//! - **VFS/fproc**: 文件描述符、目录
//! - **Kernel/proc**: 调度、IPC、寄存器保存
//!
//! 各进程表通过 `endpoint` 关联。
//!
//! # 为什么放在 PM crate 而不是 minix-types？
//!
//! 1. **职责隔离**: MProc 包含大量仅 PM 关心的私有逻辑（信号处理、父子进程树等）
//! 2. **不变量保护**: 状态转换逻辑绑定了 PM 内部复杂逻辑，放在公共库会破坏不变量
//! 3. **微内核原则**: 遵循"知识最小化"原则，其他服务不需要了解 PM 的内部实现
//!
//! # 模块结构
//!
//! - `mproc`: PM 进程结构体定义
//! - `table`: PM 进程表管理
//! - `lifecycle`: 进程生命周期状态机
//! - `block`: 阻塞状态
//! - `wait`: 父进程等待状态
//! - `guardianship`: 监护关系（父进程/tracer）
//! - `trace`: 追踪状态
//! - `signal`: 信号处理状态
//! - `credentials`: 凭证
//! - `context`: PM 上下文
//! - `fork`: fork 实现

mod mproc;
mod table;
mod lifecycle;
mod block;
mod wait;
mod guardianship;
mod trace;
mod signal;
mod credentials;
mod context;
mod fork;

pub use mproc::*;
pub use table::*;
pub use lifecycle::*;
pub use block::*;
pub use wait::*;
pub use guardianship::*;
pub use trace::*;
pub use signal::*;
pub use credentials::*;
pub use context::*;
pub use fork::*;
