//! PM 上下文结构体定义
//!
//! 这是 Minix3 `mp` 宏的 Rust 实现，包含 PM 私有的上下文管理逻辑。
//!
//! # Minix3 多进程表架构
//! Minix3 采用分布式进程表设计，共有 4 份进程表：
//! - **PM/mproc**: 进程管理、信号、权限（本模块）
//! - **VM/vmproc**: 虚拟内存、页表
//! - **VFS/fproc**: 文件描述符、目录
//! - **Kernel/proc**: 调度、IPC、寄存器保存
//!
//! # 设计决策
//! - 使用 `PmContext` 封装进程表和当前进程索引
//! - 把 C 的隐式全局状态 → Rust 的显式 capability
//! - 利用借用检查器作为"编译时的锁"
//!
//! # 为什么放在 PM crate 而不是 minix-types？
//!
//! 1. **职责隔离**: PM 上下文是 PM 私有的概念
//! 2. **不变量保护**: 上下文管理绑定了 PM 内部状态
//! 3. **微内核原则**: 其他服务不需要了解 PM 的上下文实现

use minix_types::UserSlot;
use crate::mproc::{ProcTable, Process, Privilege, Credentials};

/// PM 上下文
///
/// 封装进程表和当前进程，提供 do_fork 等系统调用的上下文
///
/// # 设计理念
///
/// 在 Minix3 C 代码中，`mp` 是一个全局宏：
/// ```c
/// #define mp (&mproc[who_p])
/// ```
///
/// 在 Rust 中，我们将其封装为 `PmContext`：
/// - 显式传递上下文，而不是隐式全局状态
/// - 利用借用检查器保证安全性
/// - 可测试（可以创建任意上下文）
///
/// # 借用检查器作为"编译时的锁"
///
/// 当你创建 `PmContext` 时，你锁定了对 `ProcTable` 的访问权限：
/// - 如果你创建的是 `&mut ProcTable`，Rust 会保证在整个 `PmContext` 生命周期内，
///   没有其他代码能偷偷摸摸地去读或写进程表
/// - 这就是编译时的锁
pub struct PmContext<'a> {
    /// 进程表引用
    pub table: &'a mut ProcTable,
    /// 当前进程索引
    pub current: usize,
}

impl<'a> PmContext<'a> {
    /// 创建新的 PM 上下文
    pub fn new(table: &'a mut ProcTable, current: usize) -> Self {
        Self { table, current }
    }
    
    /// 获取当前进程引用
    pub fn current_proc(&self) -> &Process {
        &self.table.procs[self.current]
    }
    
    /// 获取当前进程可变引用
    pub fn current_proc_mut(&mut self) -> &mut Process {
        &mut self.table.procs[self.current]
    }
    
    /// 获取进程引用
    pub fn get_proc(&self, index: usize) -> Option<&Process> {
        self.table.get(index)
    }
    
    /// 获取进程可变引用
    pub fn get_proc_mut(&mut self, index: usize) -> Option<&mut Process> {
        self.table.get_mut(index)
    }
    
    /// 检查当前进程是否是 root
    pub fn is_root(&self) -> bool {
        match &self.current_proc().resources.privilege {
            Privilege::User(creds) => creds.user.real == 0,
            Privilege::Kernel => true,
        }
    }
    
    /// 检查是否可以分配槽位
    pub fn can_alloc(&self) -> bool {
        self.table.can_alloc_for_user(self.is_root())
    }
    
    /// 获取父进程索引
    pub fn parent_index(&self) -> UserSlot {
        self.current_proc().state.guardianship.parent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::Lifecycle;
    
    #[test]
    fn test_pm_context_new() {
        let mut table = ProcTable::new();
        let ctx = PmContext::new(&mut table, 0);
        assert_eq!(ctx.current, 0);
    }
    
    #[test]
    fn test_current_proc() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        let ctx = PmContext::new(&mut table, 0);
        assert!(ctx.current_proc().is_in_use());
    }
}
