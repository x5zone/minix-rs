//! 进程标识类型定义
//!
//! 提供进程 ID、端点、进程索引等核心类型

use core::fmt;

/// 最大进程数
///
/// 对应 Minix3 的 `NR_PROCS`
pub const NR_PROCS: usize = 256;

/// 保留给 root 的槽位数
///
/// 对应 Minix3 的 `LAST_FEW`
pub const LAST_FEW: usize = 5;

/// 进程 ID（32 位有符号整数）
///
/// 对应 C 的 `pid_t`，在 32 位和 64 位系统中都是 4 字节
pub type Pid = i32;

/// 端点标识
///
/// Minix3 中用于标识进程的唯一标识符，用于 IPC 通信
///
/// # 预定义端点
/// - `NONE(0)`: 无效端点
/// - `KERNEL(1)`: 内核端点
/// - `PM(2)`: 进程管理器端点
/// - `VFS(3)`: 虚拟文件系统端点
/// - `VM(4)`: 虚拟内存管理器端点
/// - `RS(5)`: 重启服务器端点
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Endpoint(pub i32);

impl Endpoint {
    /// 无效端点
    pub const NONE: Endpoint = Endpoint(0);
    /// 内核端点
    pub const KERNEL: Endpoint = Endpoint(1);
    /// 进程管理器端点
    pub const PM: Endpoint = Endpoint(2);
    /// 虚拟文件系统端点
    pub const VFS: Endpoint = Endpoint(3);
    /// 虚拟内存管理器端点
    pub const VM: Endpoint = Endpoint(4);
    /// 重启服务器端点
    pub const RS: Endpoint = Endpoint(5);
    
    /// 创建新端点
    pub const fn new(value: i32) -> Self {
        Self(value)
    }
    
    /// 获取端点值
    pub const fn get(self) -> i32 {
        self.0
    }
    
    /// 从进程 ID 创建端点
    pub fn process(pid: u32) -> Self {
        Endpoint(pid as i32)
    }
    
    /// 获取进程 ID
    pub fn pid(&self) -> u32 {
        self.0 as u32
    }
    
    /// 检查端点是否有效
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Endpoint({})", self.0)
    }
}

/// 进程表索引
///
/// 用于在进程表中索引进程槽位，是一个透明包装的 `usize`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ProcIndex(pub usize);

impl ProcIndex {
    /// 创建新进程索引
    pub const fn new(index: usize) -> Self {
        Self(index)
    }
    
    /// 获取索引值
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for ProcIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProcIndex({})", self.0)
    }
}

/// 无效追踪者索引
///
/// 表示进程没有被追踪
pub const NO_TRACER: ProcIndex = ProcIndex(usize::MAX);

/// init 进程索引
pub const INIT_PROC_NR: ProcIndex = ProcIndex(0);
