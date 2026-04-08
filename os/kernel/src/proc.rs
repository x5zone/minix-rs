//! Kernel process management module
//!
//! 这是 Minix3 `proc` 结构体的 Rust 实现占位符。
//!
//! # Minix3 多进程表架构
//!
//! Minix3 采用分布式进程表设计，共有 4 份进程表：
//! - **Kernel/proc**: 调度、IPC、寄存器保存（本模块）
//! - **PM/mproc**: 进程管理、信号、权限 → 在 `minix-pm` crate 中
//! - **VM/vmproc**: 虚拟内存、页表 → 在 `minix-vm` crate 中
//! - **VFS/fproc**: 文件描述符、目录 → 在 `minix-vfs` crate 中
//!
//! 各进程表通过 `endpoint` 关联。

use minix_types::Endpoint;

/// Kernel 进程结构体（占位符）
///
/// TODO: 实现完整的 `struct proc` 对应
/// - `p_reg`: 寄存器保存
/// - `p_rts_flags`: 运行时状态标志
/// - `p_priority`: 优先级
/// - `p_endpoint`: 端点
#[derive(Debug, Clone, Default)]
pub struct KProcess {
    pub endpoint: Endpoint,
}

/// 创建进程
pub fn create_process() -> KProcess {
    KProcess::default()
}

/// 复制进程（fork）
pub fn copy_process(proc: &KProcess) -> KProcess {
    proc.clone()
}
