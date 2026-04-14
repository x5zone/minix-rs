#![no_std]
#![doc = include_str!("../README.md")]

//! # Minix3 核心协议类型库
//!
//! 本 crate 提供 Minix3 跨服务通讯必须知道的最小类型集合，
//! 相当于 C 语言里的 `include/minix/`。
//!
//! # 设计原则
//!
//! 1. **最小化**: 只放跨服务通讯必须知道的最小集合
//! 2. **稳定性**: 这些类型是各服务之间的"协议"，变更需要谨慎
//! 3. **无业务逻辑**: 不包含任何服务的私有实现细节
//!
//! # 核心类型
//!
//! - `Endpoint`: 端点标识（用于 IPC）
//! - `Pid`: 进程 ID
//! - `UserSlot`: 进程表索引
//! - `Uid`/`Gid`: 用户/组 ID
//! - `Message`: IPC 消息
//!
//! # 为什么不包含 MProc 等进程表结构？
//!
//! 根据 Minix3 微内核设计，各服务拥有私有的进程表：
//! - **PM/mproc**: 进程管理、信号、权限 → 在 `minix-pm` crate 中
//! - **VM/vmproc**: 虚拟内存、页表 → 在 `minix-vm` crate 中
//! - **VFS/fproc**: 文件描述符、目录 → 在 `minix-vfs` crate 中
//! - **Kernel/proc**: 调度、IPC、寄存器 → 在 `minix-kernel` crate 中
//!
//! 这样设计的原因：
//! 1. **职责隔离**: 各服务的进程表包含大量私有逻辑
//! 2. **不变量保护**: 状态转换逻辑绑定了服务内部复杂逻辑
//! 3. **微内核原则**: 遵循"知识最小化"原则

pub mod types;
pub mod ipc;

// 重新导出核心类型
pub use types::*;
pub use ipc::*;
