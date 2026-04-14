//! 端点标识类型定义
//!
//! 提供进程端点（Endpoint）类型，用于 IPC 通信中的进程标识。
//!
//! # 槽位类型区分
//!
//! - `UserSlot`: 服务器本地进程表索引（0 ~ NR_PROCS-1），用于访问 mproc/fproc/vmproc
//! - `KernelSlot`: 内核进程表索引（0 ~ NR_TASKS+NR_PROCS-1），用于访问 kernel proc table
//!
//! 注意：内核任务（负数 slot）不在 mproc/fproc/vmproc 中，只能通过 KernelSlot 访问

// 从 com 模块导入共享常量
pub use super::com::MAX_NR_TASKS;

/// Endpoint 代数位移位数
///
/// 对应 Minix3 的 `_ENDPOINT_GENERATION_SHIFT`
pub const ENDPOINT_GENERATION_SHIFT: i32 = 15;

/// Endpoint 代数大小
///
/// 对应 Minix3 的 `_ENDPOINT_GENERATION_SIZE`
pub const ENDPOINT_GENERATION_SIZE: i32 = 1 << ENDPOINT_GENERATION_SHIFT;

/// Endpoint 槽位上限
///
/// 对应 Minix3 的 `_ENDPOINT_SLOT_TOP`
pub const ENDPOINT_SLOT_TOP: i32 = ENDPOINT_GENERATION_SIZE - (MAX_NR_TASKS as i32);

/// 端点标识
///
/// Minix3 中用于标识进程的唯一标识符，用于 IPC 通信。
/// 由两部分组成：进程槽位号（slot）和代数（generation）。
///
/// # 结构
/// - 低 15 位：进程槽位号（0 ~ NR_PROCS-1 为用户进程，负数为内核任务）
/// - 高位：代数（generation），每次槽位重用时递增
///
/// # 特殊端点
/// - `NONE`: 无效端点
/// - `ANY`: 任意进程
/// - `SELF`: 自身进程
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Endpoint(pub i32);

impl Endpoint {
    // 特殊端点
    pub const NONE: Endpoint = Endpoint(ENDPOINT_SLOT_TOP - 2); // 无效端点
    pub const ANY: Endpoint = Endpoint(ENDPOINT_SLOT_TOP - 1);  // 任意进程
    pub const SELF: Endpoint = Endpoint(ENDPOINT_SLOT_TOP - 3); // 自身进程

    // 内核任务 (-5 ~ -1)
    pub const ASYNCM: Endpoint = Endpoint(-5);   // 异步消息通知
    pub const IDLE: Endpoint = Endpoint(-4);     // 空闲任务
    pub const CLOCK: Endpoint = Endpoint(-3);    // 时钟任务
    pub const SYSTEM: Endpoint = Endpoint(-2);   // 系统任务
    pub const KERNEL: Endpoint = Endpoint(-1);   // 内核/硬件中断
    pub const HARDWARE: Endpoint = Self::KERNEL; // 硬件中断别名

    // 用户空间进程 (0 ~ 11)
    pub const PM: Endpoint = Endpoint(0);    // 进程管理器
    pub const VFS: Endpoint = Endpoint(1);   // 虚拟文件系统
    pub const RS: Endpoint = Endpoint(2);    // 重启服务器
    pub const MEM: Endpoint = Endpoint(3);   // 内存驱动
    pub const SCHED: Endpoint = Endpoint(4); // 调度器
    pub const TTY: Endpoint = Endpoint(5);   // TTY 驱动
    pub const DS: Endpoint = Endpoint(6);    // 数据存储服务
    pub const MIB: Endpoint = Endpoint(7);   // 管理信息库服务
    pub const VM: Endpoint = Endpoint(8);    // 虚拟内存管理器
    pub const PFS: Endpoint = Endpoint(9);   // Pipe 文件系统
    pub const MFS: Endpoint = Endpoint(10);  // Minix 根文件系统
    pub const INIT: Endpoint = Endpoint(11); // Init 进程

    /// 获取原始值（调试用）
    #[inline(always)]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// 从代数和槽位构造端点（`_ENDPOINT(g, p)`）
    #[inline(always)]
    pub const fn from_generation_slot(generation: i32, slot: i32) -> Self {
        Self((generation << ENDPOINT_GENERATION_SHIFT) + slot)
    }

    /// 提取槽位号（`_ENDPOINT_P(e)`）
    #[inline(always)]
    pub const fn slot(self) -> i32 {
        ((self.0 + MAX_NR_TASKS as i32) & (ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS as i32
    }

    /// 提取代数（`_ENDPOINT_G(e)`）
    #[inline(always)]
    pub const fn generation(self) -> i32 {
        (self.0 + MAX_NR_TASKS as i32) >> ENDPOINT_GENERATION_SHIFT
    }

    /// 是否为 NONE
    #[inline(always)]
    pub const fn is_none(self) -> bool {
        self.0 == Self::NONE.0
    }

    /// 是否为 ANY
    #[inline(always)]
    pub const fn is_any(self) -> bool {
        self.0 == Self::ANY.0
    }

    /// 是否为 SELF
    #[inline(always)]
    pub const fn is_self(self) -> bool {
        self.0 == Self::SELF.0
    }

    /// 是否有效（非 NONE/ANY/SELF）
    #[inline(always)]
    pub const fn is_valid(self) -> bool {
        self.0 != Self::NONE.0 && self.0 != Self::ANY.0 && self.0 != Self::SELF.0
    }

    /// 检查是否为内核任务（槽位号为负）
    #[inline(always)]
    pub const fn is_kernel_task(self) -> bool {
        self.slot() < 0
    }

    /// 是否为用户进程（槽位 >= 0）
    #[inline(always)]
    pub const fn is_user_proc(self) -> bool {
        self.slot() >= 0
    }

    /// 转为 UserSlot（用户进程）
    #[inline(always)]
    pub const fn to_user_slot(self) -> Option<UserSlot> {
        if self.is_user_proc() {
            Some(UserSlot(self.slot() as usize))
        } else {
            None
        }
    }

    /// 转为 KernelSlot
    #[inline(always)]
    pub const fn to_kernel_slot(self) -> KernelSlot {
        // 内核任务: slot 为负，位置是 MAX_NR_TASKS + slot（如 -1 -> MAX_NR_TASKS-1）
        // 用户进程: slot 为正，位置是 MAX_NR_TASKS + slot
        KernelSlot((MAX_NR_TASKS as i32 + self.slot()) as usize)
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::NONE
    }
}

/// 用户进程槽位索引（0 ~ NR_PROCS-1）
/// 用于访问 mproc/fproc/vmproc，**不含内核任务**
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserSlot(pub usize);

impl UserSlot {
    #[inline(always)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[inline(always)]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// 内核进程表槽位索引（0 ~ NR_TASKS+NR_PROCS-1）
/// 用于访问内核 proc_tab：0~NR_TASKS-1 是内核任务，NR_TASKS~ 是用户进程
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KernelSlot(pub usize);

impl KernelSlot {
    #[inline(always)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[inline(always)]
    pub const fn get(self) -> usize {
        self.0
    }

    /// 从 UserSlot 转换
    #[inline(always)]
    pub const fn from_user_slot(user_slot: UserSlot) -> Self {
        Self(user_slot.0 + MAX_NR_TASKS as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_slot_extraction() {
        // 测试槽位提取：endpoint = (generation << 15) + slot
        // 当 generation = 0 时，endpoint = slot
        let ep = Endpoint::from_generation_slot(0, 5);
        assert_eq!(ep.slot(), 5);
        assert_eq!(ep.generation(), 0);

        // 测试带 generation 的情况
        let ep = Endpoint::from_generation_slot(1, 5);
        assert_eq!(ep.slot(), 5);
        assert_eq!(ep.generation(), 1);
    }

    #[test]
    fn test_endpoint_special_values() {
        assert!(Endpoint::NONE.is_none());
        assert!(Endpoint::ANY.is_any());
        assert!(Endpoint::SELF.is_self());

        assert!(!Endpoint::NONE.is_valid());
        assert!(!Endpoint::ANY.is_valid());
        assert!(!Endpoint::SELF.is_valid());

        assert!(Endpoint::PM.is_valid());
    }

    #[test]
    fn test_endpoint_kernel_task() {
        // 内核任务的槽位号为负
        let kernel_ep = Endpoint(-5); // 槽位号 -5
        assert!(kernel_ep.is_kernel_task());
        assert!(!kernel_ep.is_user_proc());
    }

    #[test]
    fn test_endpoint_equality() {
        let e1 = Endpoint::from_generation_slot(1, 100);
        let e2 = Endpoint::from_generation_slot(1, 100);
        let e3 = Endpoint::from_generation_slot(2, 100);

        assert_eq!(e1, e2);
        assert_ne!(e1, e3);
    }

    #[test]
    fn test_negative_slot() {
        // 内核任务使用负数槽位
        let endpoint = Endpoint::from_generation_slot(1, -1);
        assert_eq!(endpoint.slot(), -1);
        assert_eq!(endpoint.generation(), 1);
    }

    #[test]
    fn test_user_slot() {
        let idx = UserSlot::new(5);
        assert_eq!(idx.get(), 5);
    }

    #[test]
    fn test_kernel_slot() {
        let idx = KernelSlot::new(10);
        assert_eq!(idx.get(), 10);
    }

    #[test]
    fn test_kernel_slot_from_user() {
        let user = UserSlot::new(5);
        let kernel = KernelSlot::from_user_slot(user);
        assert_eq!(kernel.get(), 5 + MAX_NR_TASKS as usize);
    }

    #[test]
    fn test_endpoint_to_user_slot() {
        // 用户进程的 endpoint 可以转换为 UserSlot
        let ep = Endpoint::from_generation_slot(1, 5);
        assert_eq!(ep.to_user_slot(), Some(UserSlot::new(5)));

        // 内核任务的 endpoint 不能转换为 UserSlot
        let kernel_ep = Endpoint::from_generation_slot(0, -1);
        assert_eq!(kernel_ep.to_user_slot(), None);
    }

    #[test]
    fn test_endpoint_to_kernel_slot() {
        // 用户进程的内核槽位
        let ep = Endpoint::from_generation_slot(1, 5);
        assert_eq!(ep.to_kernel_slot().get(), 5 + MAX_NR_TASKS as usize);

        // 内核任务的内核槽位
        let kernel_ep = Endpoint::from_generation_slot(0, -1);
        assert_eq!(kernel_ep.to_kernel_slot().get(), (MAX_NR_TASKS - 1) as usize);
    }
}
