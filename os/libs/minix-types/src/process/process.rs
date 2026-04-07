//! 进程结构体定义
//!
//! 这是 Minix3 `mproc` 结构体的 Rust 重写版本
//!
//! # 分层设计
//! 采用方案三的分层抽象，将进程字段分为四层：
//! 1. 身份信息：PID、端点、进程组、名称
//! 2. 状态机：生命周期、阻塞、等待、监护、追踪
//! 3. 资源：权限、信号、定时器、调度、时间统计
//! 4. IPC：消息回复、事件订阅

use crate::types::{Pid, Endpoint, ProcIndex, Clock, VirBytes};
use crate::process::*;
use crate::ipc::Message;

/// 进程名最大长度
pub const PROC_NAME_LEN: usize = 16;

/// 定时器数量
pub const NR_ITIMERS: usize = 3;

// ============================================================================
// 第一层：身份信息
// ============================================================================

/// 进程标识
///
/// 包含进程表索引和 PID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId {
    /// 进程表索引
    pub index: ProcIndex,
    /// 进程 ID
    pub pid: Pid,
}

/// 身份信息
///
/// 进程的基本标识信息
#[derive(Debug, Clone)]
pub struct ProcessIdentity {
    /// 进程标识（索引 + PID）
    pub id: ProcessId,
    /// 端点标识（用于 IPC）
    pub endpoint: Endpoint,
    /// 进程组 ID
    pub procgrp: Pid,
    /// 进程名
    pub name: [u8; PROC_NAME_LEN],
}

impl Default for ProcessIdentity {
    fn default() -> Self {
        Self {
            id: ProcessId {
                index: ProcIndex::new(0),
                pid: 0,
            },
            endpoint: Endpoint::default(),
            procgrp: 0,
            name: [0; PROC_NAME_LEN],
        }
    }
}

// ============================================================================
// 第二层：状态机
// ============================================================================

/// 状态机
///
/// 进程的所有状态信息
#[derive(Debug, Clone)]
pub struct ProcessState {
    /// 生命周期状态（互斥）
    pub lifecycle: Lifecycle,
    /// 阻塞状态（可与生命周期组合）
    pub block: BlockState,
    /// 父进程等待状态（⚠️ 放在父进程）
    pub wait: WaitState,
    /// 监护关系
    pub guardianship: Guardianship,
    /// 追踪状态
    pub trace: TraceState,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::default(),
            block: BlockState::default(),
            wait: WaitState::default(),
            guardianship: Guardianship::default(),
            trace: TraceState::default(),
        }
    }
}

// ============================================================================
// 第三层：资源
// ============================================================================

/// Minix 定时器
#[derive(Debug, Clone, Copy)]
pub struct MinixTimer {
    /// 过期时间
    pub expire_time: Clock,
    /// 重载时间（用于周期性定时器）
    pub reload_time: Clock,
}

/// 特权级别
///
/// 对应 Minix3 的 `PRIV_PROC` flag
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Privilege {
    /// 普通用户进程
    User(Credentials),
    /// 系统进程（PRIV_PROC）
    ///
    /// 系统进程有特殊权限，退出时不需要等待 VFS
    Kernel,
}

impl Default for Privilege {
    fn default() -> Self {
        Self::User(Credentials::default())
    }
}

impl Privilege {
    /// 检查是否是系统进程
    pub fn is_kernel(&self) -> bool {
        matches!(self, Self::Kernel)
    }
    
    /// 获取权限凭证
    ///
    /// 如果是系统进程，返回 `None`
    pub fn credentials(&self) -> Option<&Credentials> {
        match self {
            Self::User(creds) => Some(creds),
            Self::Kernel => None,
        }
    }
}

bitflags::bitflags! {
    /// 剩余标志位
    ///
    /// 这些标志位尚未归类到具体的状态机中
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RemainingFlags: u32 {
        /// 定时器已启动
        const ALARM_ON = 0x00010;
        /// 父进程已变更
        const NEW_PARENT = 0x00800;
        /// 部分执行
        const PARTIAL_EXEC = 0x04000;
        /// 污染标记
        const TAINTED = 0x40000;
    }
}

/// 资源
///
/// 进程的资源管理信息
#[derive(Debug, Clone)]
pub struct ProcessResources {
    /// 特权级别
    pub privilege: Privilege,
    /// 信号处理状态
    pub signals: SignalState,
    /// 子进程用户时间累计
    pub child_utime: Clock,
    /// 子进程系统时间累计
    pub child_stime: Clock,
    /// 进程启动时间
    pub started: Clock,
    /// 进程定时器
    pub timer: Option<MinixTimer>,
    /// 间隔定时器
    pub intervals: [Clock; NR_ITIMERS],
    /// nice 值
    pub nice: i32,
    /// 调度器端点
    pub scheduler: Endpoint,
    /// 未归类的标志位
    pub flags: RemainingFlags,
}

impl Default for ProcessResources {
    fn default() -> Self {
        Self {
            privilege: Privilege::default(),
            signals: SignalState::default(),
            child_utime: 0,
            child_stime: 0,
            started: 0,
            timer: None,
            intervals: [0; NR_ITIMERS],
            nice: 0,
            scheduler: Endpoint::default(),
            flags: RemainingFlags::empty(),
        }
    }
}

// ============================================================================
// 第四层：IPC
// ============================================================================

/// IPC 上下文
///
/// 进程间通信相关信息
#[derive(Debug, Clone)]
pub struct ProcessIpc {
    /// IPC 回复消息（延迟加载）
    pub reply: Option<Message>,
    /// 事件订阅者
    pub event_subscriber: Option<ProcIndex>,
    /// 栈帧地址
    pub frame_addr: VirBytes,
    /// 栈帧长度
    pub frame_len: usize,
}

impl Default for ProcessIpc {
    fn default() -> Self {
        Self {
            reply: None,
            event_subscriber: None,
            frame_addr: 0,
            frame_len: 0,
        }
    }
}

// ============================================================================
// 主结构体
// ============================================================================

/// 进程结构体
///
/// 这是 Minix3 `mproc` 结构体的 Rust 重写版本
///
/// # 分层设计
/// 采用方案三的分层抽象，提高代码组织性：
///
/// ```text
/// Process
/// ├── identity: ProcessIdentity   // 身份信息
/// ├── state: ProcessState         // 状态机
/// ├── resources: ProcessResources // 资源
/// └── ipc: ProcessIpc             // IPC 上下文
/// ```
///
/// # Minix3 映射
/// | Minix3 字段 | Rust 字段 |
/// |------------|-----------|
/// | mp_pid, mp_endpoint | identity.id, identity.endpoint |
/// | mp_flags (状态相关) | state.lifecycle, state.block |
/// | mp_realuid, mp_effuid | resources.privilege |
/// | mp_reply | ipc.reply |
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Process {
    /// 身份信息
    pub identity: ProcessIdentity,
    /// 状态机
    pub state: ProcessState,
    /// 资源
    pub resources: ProcessResources,
    /// IPC 上下文
    pub ipc: ProcessIpc,
}

impl Default for Process {
    fn default() -> Self {
        Self {
            identity: ProcessIdentity::default(),
            state: ProcessState::default(),
            resources: ProcessResources::default(),
            ipc: ProcessIpc::default(),
        }
    }
}

impl Process {
    /// 创建新进程
    ///
    /// # 参数
    /// - `index`: 进程表索引
    /// - `pid`: 进程 ID
    pub fn new(index: usize, pid: Pid) -> Self {
        Self {
            identity: ProcessIdentity {
                id: ProcessId {
                    index: ProcIndex::new(index),
                    pid,
                },
                ..Default::default()
            },
            ..Self::default()
        }
    }
    
    /// 检查槽位是否在使用中
    pub fn is_in_use(&self) -> bool {
        self.state.lifecycle.is_in_use()
    }
    
    /// 获取进程 PID
    pub fn pid(&self) -> Pid {
        self.identity.id.pid
    }
    
    /// 获取进程索引
    pub fn index(&self) -> ProcIndex {
        self.identity.id.index
    }
    
    /// 获取端点
    pub fn endpoint(&self) -> Endpoint {
        self.identity.endpoint
    }
    
    /// 获取父进程索引
    pub fn parent(&self) -> ProcIndex {
        self.state.guardianship.parent()
    }
    
    /// 获取追踪者索引
    pub fn tracer(&self) -> Option<ProcIndex> {
        self.state.guardianship.tracer()
    }
    
    /// 检查是否是系统进程
    pub fn is_kernel_process(&self) -> bool {
        self.resources.privilege.is_kernel()
    }
    
    /// 检查是否是僵尸进程
    pub fn is_zombie(&self) -> bool {
        self.state.lifecycle.is_zombie()
    }
    
    /// 检查是否正在退出
    pub fn is_exiting(&self) -> bool {
        self.state.lifecycle.is_exiting()
    }
    
    /// 检查是否被停止
    pub fn is_stopped(&self) -> bool {
        self.state.block.stopped || self.state.trace.stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process_default() {
        let proc = Process::default();
        assert!(!proc.is_in_use());
        assert_eq!(proc.pid(), 0);
        assert!(!proc.is_zombie());
        assert!(!proc.is_exiting());
    }
    
    #[test]
    fn test_process_new() {
        let proc = Process::new(5, 100);
        assert_eq!(proc.index(), ProcIndex::new(5));
        assert_eq!(proc.pid(), 100);
        assert!(!proc.is_in_use());
    }
    
    #[test]
    fn test_process_lifecycle() {
        let mut proc = Process::new(0, 1);
        proc.state.lifecycle = Lifecycle::Running;
        assert!(proc.is_in_use());
        assert!(!proc.is_zombie());
        
        proc.state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 9 };
        assert!(proc.is_exiting());
        
        proc.state.lifecycle = Lifecycle::Zombie { exit_code: 42, sig_status: 0 };
        assert!(proc.is_zombie());
    }
    
    #[test]
    fn test_process_guardianship() {
        let mut proc = Process::default();
        proc.state.guardianship = Guardianship::Normal { parent: ProcIndex::new(10) };
        assert_eq!(proc.parent(), ProcIndex::new(10));
        assert!(proc.tracer().is_none());
        
        proc.state.guardianship = Guardianship::Traced {
            parent: ProcIndex::new(10),
            tracer: ProcIndex::new(5),
            trace_exit: false,
            trace_options: TraceOptions::empty(),
        };
        assert_eq!(proc.tracer(), Some(ProcIndex::new(5)));
    }
    
    #[test]
    fn test_process_stopped() {
        let mut proc = Process::default();
        assert!(!proc.is_stopped());
        
        proc.state.block.stopped = true;
        assert!(proc.is_stopped());
        
        proc.state.block.stopped = false;
        proc.state.trace.stopped = true;
        assert!(proc.is_stopped());
    }
    
    #[test]
    fn test_process_privilege() {
        let mut proc = Process::default();
        assert!(!proc.is_kernel_process());
        
        proc.resources.privilege = Privilege::Kernel;
        assert!(proc.is_kernel_process());
    }
    
    #[test]
    fn test_layered_structure() {
        let proc = Process::default();
        
        assert_eq!(proc.identity.id.pid, proc.pid());
        assert_eq!(proc.identity.id.index, proc.index());
        assert_eq!(proc.identity.endpoint, proc.endpoint());
    }
}
