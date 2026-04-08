//! fork 系统调用实现
//!
//! 这是 Minix3 `do_fork` 函数的 Rust 实现，包含 PM 私有的 fork 逻辑。
//!
//! # Minix3 多进程表架构
//! Minix3 采用分布式进程表设计，共有 4 份进程表：
//! - **PM/mproc**: 进程管理、信号、权限（本模块）
//! - **VM/vmproc**: 虚拟内存、页表
//! - **VFS/fproc**: 文件描述符、目录
//! - **Kernel/proc**: 调度、IPC、寄存器保存
//!
//! # 阶段划分
//! - 阶段 2a：参数检查与槽位分配（本文件）
//! - 阶段 2b：VM fork 调用
//! - 阶段 2c：进程结构初始化
//!
//! # 为什么放在 PM crate 而不是 minix-types？
//!
//! 1. **职责隔离**: fork 是 PM 的核心系统调用
//! 2. **不变量保护**: fork 逻辑绑定了 PM 内部状态
//! 3. **微内核原则**: 其他服务不需要了解 PM 的 fork 实现

use minix_types::{Pid, Endpoint, ProcIndex, NR_PROCS, LAST_FEW};
use crate::mproc::{PmContext, Process, Lifecycle, Privilege, Credentials, ProcessIdentity, ProcessId, ProcessState, BlockState, WaitState, Guardianship, TraceState, ProcessResources, ProcessIpc, ProcTable};

/// fork 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkError {
    /// 进程表已满
    TableFull,
    /// 非 root 用户槽位不足（LAST_FEW 保留）
    ReservedForRoot,
    /// 系统资源不足
    ResourceExhausted,
    /// 内部错误（不应该发生）
    InternalError,
}

impl ForkError {
    /// 转换为错误码
    ///
    /// 对应 Minix3 的 `errno` 值
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::TableFull => 11,        // EAGAIN
            Self::ReservedForRoot => 11,  // EAGAIN
            Self::ResourceExhausted => 12, // ENOMEM
            Self::InternalError => 22,    // EINVAL
        }
    }
}

/// fork 结果
#[derive(Debug, Clone, Copy)]
pub struct ForkResult {
    /// 子进程索引
    pub child_index: usize,
    /// 子进程 PID
    pub child_pid: Pid,
    /// 子进程 Endpoint
    pub child_endpoint: Endpoint,
}

impl<'a> PmContext<'a> {
    /// fork 系统调用（前半部分）
    ///
    /// 对应 Minix3 的 `do_fork` 函数开头部分：
    /// ```c
    /// int do_fork(void) {
    ///   register struct mproc *rmp;   // 父进程指针
    ///   register struct mproc *rmc;   // 子进程指针
    ///   static unsigned int next_child = 0;
    ///   int n = 0;
    ///
    ///   rmp = mp;  // 当前进程
    ///   
    ///   // 1. 检查进程表是否已满
    ///   if ((procs_in_use == NR_PROCS) ||
    ///       (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
    ///     printf("PM: warning, process table is full!\n");
    ///     return(EAGAIN);
    ///   }
    ///
    ///   // 2. 查找空闲槽位
    ///   do {
    ///     next_child = (next_child+1) % NR_PROCS;
    ///     n++;
    ///   } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
    ///   
    ///   if(n > NR_PROCS)
    ///     panic("do_fork can't find child slot");
    ///   
    ///   // ... 后续处理
    /// }
    /// ```
    ///
    /// # 返回值
    /// - `Ok(ForkResult)`: 成功分配槽位
    /// - `Err(ForkError)`: 分配失败
    pub fn do_fork_prepare(&mut self) -> Result<ForkResult, ForkError> {
        if self.table.is_full() {
            return Err(ForkError::TableFull);
        }
        
        if !self.can_alloc() {
            return Err(ForkError::ReservedForRoot);
        }
        
        let child_index = self.table.alloc_slot().ok_or(ForkError::TableFull)?;
        
        let child_pid = self.generate_child_pid();
        let child_endpoint = ProcTable::calculate_endpoint(child_index);
        
        Ok(ForkResult {
            child_index,
            child_pid,
            child_endpoint,
        })
    }
    
    /// 生成子进程 PID
    ///
    /// 简化实现：使用索引 + 时间戳
    fn generate_child_pid(&self) -> Pid {
        let parent = self.current_proc();
        let base = parent.pid().max(1);
        base + self.table.count() as i32 + 1
    }
    
    /// 从父进程创建子进程
    ///
    /// 对应 Minix3 的进程结构复制：
    /// ```c
    /// *rmc = *rmp;  // 复制整个结构体
    /// ```
    ///
    /// 但我们使用显式的 `fork_from` 方法，强迫检查每一个字段
    pub fn fork_child_from_parent(&mut self, child_index: usize, child_pid: Pid, child_endpoint: Endpoint) {
        let parent = self.current_proc().clone();
        
        let child = Process::fork_from(&parent, child_pid, child_endpoint, self.current);
        
        self.table.procs[child_index] = child;
    }
}

impl Process {
    /// 从父进程创建子进程
    ///
    /// 强迫检查每一个字段：哪些该留，哪些该清
    ///
    /// # Minix3 映射
    /// | Minix3 行为 | Rust 行为 |
    /// |------------|----------|
    /// | `*rmc = *rmp` | 显式复制每个字段 |
    /// | `rmc->mp_pid = next_pid` | `identity.id.pid = child_pid` |
    /// | `rmc->mp_flags &= ~TRACE_EXIT` | `trace.stopped = false` |
    /// | `rmc->mp_child_utime = 0` | `resources.child_utime = 0` |
    pub fn fork_from(parent: &Process, child_pid: Pid, child_endpoint: Endpoint, parent_index: usize) -> Self {
        Self {
            identity: ProcessIdentity {
                id: ProcessId {
                    index: parent.identity.id.index,
                    pid: child_pid,
                },
                endpoint: child_endpoint,
                procgrp: parent.identity.procgrp,
                name: parent.identity.name,
            },
            state: ProcessState {
                lifecycle: Lifecycle::Running,
                block: BlockState::default(),
                wait: WaitState::default(),
                guardianship: Guardianship::Normal { 
                    parent: ProcIndex::new(parent_index) 
                },
                trace: TraceState::default(),
            },
            resources: ProcessResources {
                privilege: parent.resources.privilege.clone(),
                signals: parent.resources.signals.clone(),
                child_utime: 0,
                child_stime: 0,
                started: parent.resources.started,
                timer: None,
                intervals: parent.resources.intervals,
                nice: parent.resources.nice,
                scheduler: parent.resources.scheduler,
                flags: parent.resources.flags,
            },
            ipc: ProcessIpc::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::boxed::Box;
    
    fn create_test_context() -> PmContext<'static> {
        let table = Box::leak(Box::new(ProcTable::new()));
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        PmContext::new(table, 0)
    }
    
    #[test]
    fn test_fork_prepare() {
        let mut ctx = create_test_context();
        let result = ctx.do_fork_prepare().unwrap();
        assert!(result.child_index < NR_PROCS);
        assert!(result.child_pid > 0);
    }
    
    #[test]
    fn test_fork_table_full() {
        let mut ctx = create_test_context();
        
        for i in 0..NR_PROCS {
            ctx.table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        ctx.table.procs_in_use.set(NR_PROCS);
        
        let result = ctx.do_fork_prepare();
        assert!(matches!(result, Err(ForkError::TableFull)));
    }
    
    #[test]
    fn test_fork_reserved_for_root() {
        let table = Box::leak(Box::new(ProcTable::new()));
        
        for i in 0..(NR_PROCS - LAST_FEW) {
            table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        table.procs_in_use.set(NR_PROCS - LAST_FEW);
        
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        
        let mut ctx = PmContext::new(table, 0);
        
        let result = ctx.do_fork_prepare();
        assert!(matches!(result, Err(ForkError::ReservedForRoot)));
    }
    
    #[test]
    fn test_fork_child_from_parent() {
        let mut ctx = create_test_context();
        
        let fork_result = ctx.do_fork_prepare().unwrap();
        ctx.fork_child_from_parent(
            fork_result.child_index,
            fork_result.child_pid,
            fork_result.child_endpoint,
        );
        
        let child = ctx.table.get(fork_result.child_index).unwrap();
        assert!(child.is_in_use());
        assert_eq!(child.pid(), fork_result.child_pid);
        assert_eq!(child.state.guardianship.parent(), ProcIndex::new(0));
    }
    
    #[test]
    fn test_fork_error_to_errno() {
        assert_eq!(ForkError::TableFull.to_errno(), 11);
        assert_eq!(ForkError::ReservedForRoot.to_errno(), 11);
        assert_eq!(ForkError::ResourceExhausted.to_errno(), 12);
        assert_eq!(ForkError::InternalError.to_errno(), 22);
    }
}
