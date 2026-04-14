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

use minix_types::{Pid, Endpoint, UserSlot, NR_PROCS, LAST_FEW, Clock, Uid, Gid};
use crate::mproc::{PmContext, Process, Lifecycle, Privilege, Credentials, ProcessIdentity, ProcessId, ProcessState, BlockState, WaitState, Guardianship, TraceState, ProcessResources, ProcessIpc, ProcTable, NR_ITIMERS, RemainingFlags};

/// PM -> VM: Fork 请求消息
#[derive(Debug, Clone, Copy)]
pub struct VmForkRequest {
    /// 父进程Endpoint
    pub parent_endpoint: Endpoint,
    /// 子进程槽位索引
    pub child_index: usize,
}

/// VM -> PM: Fork 响应消息
#[derive(Debug, Clone, Copy)]
pub struct VmForkResponse {
    /// 子进程Endpoint
    pub child_endpoint: Endpoint,
    /// 是否成功
    pub success: bool,
}

/// PM -> VFS: Fork 请求消息
#[derive(Debug, Clone, Copy)]
pub struct VfsPmForkRequest {
    /// 子进程Endpoint
    pub child_endpoint: Endpoint,
    /// 父进程Endpoint
    pub parent_endpoint: Endpoint,
    /// 子进程PID
    pub child_pid: Pid,
    /// 真实UID
    pub real_uid: Uid,
    /// 真实GID
    pub real_gid: Gid,
}

/// VFS -> PM: Fork 响应消息
#[derive(Debug, Clone, Copy)]
pub struct VfsPmForkResponse {
    /// 子进程Endpoint
    pub child_endpoint: Endpoint,
    /// 是否成功
    pub success: bool,
}

/// fork 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkError {
    /// 进程表已满
    TableFull,
    /// 非 root 用户槽位不足（LAST_FEW 保留）
    ReservedForRoot,
    /// 系统资源不足
    ResourceExhausted,
    /// VM调用失败
    VmError,
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
            Self::VmError => 12,          // ENOMEM
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
        // 直接使用槽位中已有的带generation的endpoint（release_slot时已更新）
        let child_endpoint = self.table.procs[child_index].endpoint();
        
        Ok(ForkResult {
            child_index,
            child_pid,
            child_endpoint,
        })
    }
    
    /// 生成子进程 PID
    ///
    /// 使用 PidGenerator 生成唯一的 PID。
    ///
    /// # Minix3 映射
    ///
    /// 对应 Minix3 的 `get_free_pid()` 函数：
    /// ```c
    /// pid_t get_free_pid()
    /// {
    ///   static pid_t next_pid = INIT_PID + 1;
    ///   register struct mproc *rmp;
    ///   int t;
    ///
    ///   do {
    ///     t = 0;
    ///     next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
    ///     for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
    ///       if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
    ///         t = 1;
    ///         break;
    ///       }
    ///   } while (t);
    ///
    ///   return(next_pid);
    /// }
    /// ```
    ///
    /// # 复杂度
    ///
    /// - 期望: O(1) (因为冲突概率极低，约 0.8%)
    /// - 最坏: O(N) (极罕见)
    fn generate_child_pid(&self) -> Pid {
        self.table.pid_generator.get_free_pid(self.table)
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
        
        let child = Process::fork_from(&parent, child_index, child_pid, child_endpoint, self.current);
        
        self.table.procs[child_index] = child;
    }
}

/// 获取当前时钟滴答数
///
/// 对应 Minix3 的 `getticks()` 函数
///
/// # TODO
/// 当前返回 0，后续需要实现真正的时钟获取：
/// - 通过 IPC 向 CLOCK 任务请求时间
/// - 或使用内核提供的时钟接口
fn getticks() -> Clock {
    0
}

impl Process {
    /// Fork 语义：从父进程创建子进程
    ///
    /// 策略：显式构造（Explicit Construction）
    /// 优势：编译器强制检查新增字段，无隐式行为
    ///
    /// # Minix3 映射
    /// | Minix3 行为 | Rust 行为 |
    /// |------------|----------|
    /// | `*rmc = *rmp` | 显式复制每个字段 |
    /// | `rmc->mp_pid = next_pid` | `identity.id.pid = child_pid` |
    /// | `rmc->mp_flags &= ~TRACE_EXIT` | `trace.stopped = false` |
    /// | `rmc->mp_child_utime = 0` | `resources.child_utime = 0` |
    /// | `rmc->mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` | `flags` 只保留 TAINTED |
    /// | `rmc->mp_started = getticks()` | `started = getticks()` |
    /// | 特权进程 scheduler | `Endpoint::RS` |
    pub fn fork_from(
        parent: &Process, 
        child_index: usize, 
        child_pid: Pid, 
        child_endpoint: Endpoint,
        parent_index: usize,
    ) -> Self {
        
        // --- 1. IDENTITY：继承 + 覆盖 ---
        // 语义：我是谁（PID变了，其他继承）
        let identity = ProcessIdentity {
            id: ProcessId { 
                index: UserSlot::new(child_index), // 显式传入新索引
                pid: child_pid,                     // 显式传入新 PID
            },
            endpoint: child_endpoint,
            // 继承：进程组和名字（值拷贝，安全）
            procgrp: parent.identity.procgrp, 
            name: parent.identity.name,
        };

        // --- 2. STATE：重置 + 关系 ---
        // 语义：我的状态（我是新进程，我是谁的孩子）
        let state = ProcessState {
            lifecycle: Lifecycle::Running,      // 刚出生的进程总是就绪/运行态
            block: BlockState::default(),       // 重置阻塞状态
            wait: WaitState::default(),         // 重置等待状态
            guardianship: Guardianship::Normal { 
                parent: UserSlot::new(parent_index), // 认祖归宗：父进程索引
            },
            trace: TraceState::default(),       // 清除追踪器（除非 TO_TRACEFORK）
        };

        // --- 3. RESOURCES：混合策略 ---
        // 语义：我的资源（权限继承，统计清零）
        let resources = ProcessResources {
            // --- 深度继承区 ---
            // ⚠️ 确保是 Deep Clone，不是浅拷贝
            privilege: parent.resources.privilege.clone(),
            signals: parent.resources.signals.clone(),
            
            // --- 重置区 ---
            child_utime: 0,
            child_stime: 0,
            started: getticks(),  // ⚠️ 确保时钟源正确
            timer: None,
            intervals: [0; NR_ITIMERS],
            nice: parent.resources.nice,
            
            // --- 特权逻辑区 ---
            // ⚠️ 这里不封装进 Resources，因为依赖了外部的 Endpoint::RS
            scheduler: if parent.resources.privilege.is_kernel() {
                Endpoint::RS  // 特权进程强制绑定 RS
            } else {
                parent.resources.scheduler  // 普通进程继承
            },
            
            // --- 标志位过滤区 ---
            flags: {
                let mut flags = RemainingFlags::empty();
                // 继承 TAINTED 和 DELAY_CALL 标志，对应 Minix3 逻辑：
                // rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED)
                if parent.resources.flags.contains(RemainingFlags::TAINTED) {
                    flags |= RemainingFlags::TAINTED;
                }
                if parent.resources.flags.contains(RemainingFlags::DELAY_CALL) {
                    flags |= RemainingFlags::DELAY_CALL;
                }
                flags
            },
        };

        // --- 4. IPC：默认 ---
        let ipc = ProcessIpc::default();

        Self { identity, state, resources, ipc }
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
        assert_eq!(child.state.guardianship.parent(), UserSlot::new(0));
    }
    
    #[test]
    fn test_fork_error_to_errno() {
        assert_eq!(ForkError::TableFull.to_errno(), 11);
        assert_eq!(ForkError::ReservedForRoot.to_errno(), 11);
        assert_eq!(ForkError::ResourceExhausted.to_errno(), 12);
        assert_eq!(ForkError::InternalError.to_errno(), 22);
    }
    
    #[test]
    fn test_fork_child_index_correct() {
        let parent = Process::new(0, 100);
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.identity.id.index, UserSlot::new(5));
        assert_eq!(child.identity.id.pid, 200);
        assert_eq!(child.identity.endpoint, Endpoint(50));
    }
    
    #[test]
    fn test_fork_inherited_fields() {
        let mut parent = Process::new(0, 100);
        parent.identity.procgrp = 500;
        parent.resources.nice = 10;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.identity.procgrp, 500);
        assert_eq!(child.resources.nice, 10);
    }
    
    #[test]
    fn test_fork_cleared_fields() {
        let mut parent = Process::new(0, 100);
        parent.resources.child_utime = 1000;
        parent.resources.child_stime = 2000;
        parent.resources.intervals = [100, 200, 300];
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.resources.child_utime, 0);
        assert_eq!(child.resources.child_stime, 0);
        assert_eq!(child.resources.intervals, [0; NR_ITIMERS]);
    }
    
    #[test]
    fn test_fork_flags_inheritance() {
        let mut parent = Process::new(0, 100);
        parent.resources.flags = RemainingFlags::TAINTED | RemainingFlags::DELAY_CALL | RemainingFlags::ALARM_ON | RemainingFlags::NEW_PARENT;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert!(child.resources.flags.contains(RemainingFlags::TAINTED));
        assert!(child.resources.flags.contains(RemainingFlags::DELAY_CALL));
        assert!(!child.resources.flags.contains(RemainingFlags::ALARM_ON));
        assert!(!child.resources.flags.contains(RemainingFlags::NEW_PARENT));
    }
    
    #[test]
    fn test_fork_flags_no_tainted() {
        let mut parent = Process::new(0, 100);
        parent.resources.flags = RemainingFlags::ALARM_ON | RemainingFlags::NEW_PARENT;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert!(child.resources.flags.is_empty());
    }
    
    #[test]
    fn test_fork_privilege_scheduler() {
        let mut parent = Process::new(0, 100);
        parent.resources.privilege = Privilege::Kernel;
        parent.resources.scheduler = Endpoint::NONE;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.resources.scheduler, Endpoint::RS);
    }
    
    #[test]
    fn test_fork_normal_scheduler() {
        let mut parent = Process::new(0, 100);
        parent.resources.privilege = Privilege::User(Credentials::new(1000, 100));
        parent.resources.scheduler = Endpoint::PM;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert_eq!(child.resources.scheduler, Endpoint::PM);
    }
    
    #[test]
    fn test_fork_parent_relationship() {
        let parent = Process::new(10, 100);
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 10);
        
        assert_eq!(child.state.guardianship.parent(), UserSlot::new(10));
    }
    
    #[test]
    fn test_fork_ipc_reset() {
        let mut parent = Process::new(0, 100);
        parent.ipc.reply = Some(minix_ipc::Message::default());
        parent.ipc.event_subscriber = Some(UserSlot::new(5));
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint(50), 0);
        
        assert!(child.ipc.reply.is_none());
        assert!(child.ipc.event_subscriber.is_none());
    }
}
