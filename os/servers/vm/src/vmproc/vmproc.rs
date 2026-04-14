//! VM 进程结构体
//!
//! 提供虚拟内存管理器中单个进程的元数据结构。

use minix_types::{Endpoint, UserSlot, VirBytes};
use super::VmFlags;
use crate::region::RegionAvl;

/// ACL 权限索引
///
/// VM 私有的 ACL 机制。`vm_acl` 字段和 `acl_mask[][]` 表仅在 VM 内部使用，
/// 其他服务（包括 RS）有自己的权限控制机制（如 RS 的 pci_acl）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AclIndex(pub i32);

impl AclIndex {
    /// 创建新 ACL 索引
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// 获取值
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// 页表引用（Mock 版本）
///
/// 使用 minix-arch crate 的 Paging trait 抽象，不直接操作硬件。
/// 当前为 Mock 实现，用于用户态测试。
#[derive(Debug, Clone)]
pub struct PageTableRef {
    /// 页表根物理地址（用于激活页表）
    pub root_phys: u64,
    /// 页表条目数（统计用）
    pub entry_count: usize,
}

impl Default for PageTableRef {
    fn default() -> Self {
        Self {
            root_phys: 0,
            entry_count: 0,
        }
    }
}

impl PageTableRef {
    /// 创建新的空页表引用
    pub fn new() -> Self {
        Self::default()
    }
}

// 保留旧名称作为类型别名，便于迁移
pub type PageTable = PageTableRef;

/// 启动镜像信息
///
/// 仅对启动时进程（boot time process）有效。
/// 对应 Minix3: [`type.h#L148-154`](../../../../minix3/minix/include/minix/type.h#L148-L154)
#[derive(Debug, Clone)]
pub struct BootImage {
    /// 进程号
    pub proc_nr: i32,
    /// 进程名称
    pub proc_name: [u8; 16],
    /// 端点号
    pub endpoint: Endpoint,
    /// 内存起始地址
    pub start_addr: u64,
    /// 内存长度
    pub len: u64,
}

impl BootImage {
    /// 创建新的启动镜像信息
    pub const fn new(proc_nr: i32, endpoint: Endpoint) -> Self {
        Self {
            proc_nr,
            proc_name: [0; 16],
            endpoint,
            start_addr: 0,
            len: 0,
        }
    }
}

/// VM 进程结构体
///
/// 设计原则：
/// - 所有字段始终存在（无 Option）
/// - 用 flags 表达状态
/// - 允许"暂时不一致"（如 fork 中间态）
///
/// # 为什么不用 Option<Endpoint>？
///
/// - fork 时 endpoint 还没生成，但这是"极短暂的中间态"
/// - 用 `Option` 会让 100% 的代码都处理 `None` 分支
/// - 正确做法：`endpoint: Endpoint` + `flags.contains(IN_USE)` 判断有效性
///
/// # 对应 Minix3 源码
///
/// [`vmproc.h`](../../../../minix3/minix/servers/vm/vmproc.h) 中的 `struct vmproc`
#[derive(Debug)]
pub struct VmProc {
    // === 标识层 ===
    /// 进程表槽位索引
    pub slot: UserSlot,

    /// 端点标识
    pub endpoint: Endpoint,

    /// 进程状态标志
    pub flags: VmFlags,

    /// ACL 权限索引
    pub acl: AclIndex,

    /// 启动镜像信息（仅对启动时进程有效）
    ///
    /// TODO：代码组织或许应该在别的地方
    /// 对应 Minix3: [`vmproc.h#L18`](../../../../minix3/minix/servers/vm/vmproc.h#L18)
    pub vm_boot: Option<BootImage>,

    // === 内存层 ===
    /// 页表
    pub page_table: PageTable,

    /// 内存区域 AVL 树
    ///
    /// 管理进程的虚拟地址空间区域。
    /// 对应 Minix3: `vm_regions_avl`
    pub regions: RegionAvl,

    /// 区域顶部地址
    pub region_top: VirBytes,

    // === 资源限制 ===
    /// 当前内存使用量
    pub total: VirBytes,

    /// 最大内存使用量
    pub total_max: VirBytes,

    // === 统计数据 ===
    /// 次要页错误数
    pub minor_fault: u64,

    /// 主要页错误数
    pub major_fault: u64,

    // === 调试统计（条件编译）===
    /// 字节复制计数（仅当 VMSTATS 启用时）
    ///
    /// 对应 Minix3: [`vmproc.h#L25-28`](../../../../minix3/minix/servers/vm/vmproc.h#L25-L28)
    #[cfg(feature = "vmstats")]
    pub byte_copies: u64,
}

impl VmProc {
    /// 创建新的空进程槽位
    ///
    /// 返回一个未初始化的进程结构体，调用者需要设置正确的值。
    pub fn empty(slot: UserSlot) -> Self {
        Self {
            slot,
            endpoint: Endpoint::NONE,
            flags: VmFlags::empty(),
            acl: AclIndex::default(),
            vm_boot: None,
            page_table: PageTable::new(),
            regions: RegionAvl::new(),
            region_top: VirBytes::default(),
            total: VirBytes::default(),
            total_max: VirBytes::default(),
            minor_fault: 0,
            major_fault: 0,
            #[cfg(feature = "vmstats")]
            byte_copies: 0,
        }
    }

    /// 检查进程是否在使用中
    #[inline]
    pub fn is_in_use(&self) -> bool {
        self.flags.contains(VmFlags::IN_USE)
    }

    /// 检查进程是否正在退出
    #[inline]
    pub fn is_exiting(&self) -> bool {
        self.flags.contains(VmFlags::EXITING)
    }

    /// 检查是否为 VM 实例
    #[inline]
    pub fn is_vm_instance(&self) -> bool {
        self.flags.contains(VmFlags::VM_INSTANCE)
    }

    /// Debug invariant（运行时检查）
    ///
    /// 替代 enum 的方式：运行时保证，而不是类型系统强制
    #[cfg(debug_assertions)]
    pub fn check(&self) {
        if self.flags.contains(VmFlags::IN_USE) {
            debug_assert!(!self.endpoint.is_none(), "IN_USE but endpoint is NONE");
        }
    }
}

impl Default for VmProc {
    fn default() -> Self {
        Self::empty(UserSlot::new(0))
    }
}

/// 从 endpoint 提取 slot 并验证是否匹配
///
/// 这是一个辅助函数，仅检查 endpoint 的 slot 部分是否与给定的 slot 匹配。
/// 注意：这不是完整的 `vm_isokendpt` 实现，完整的验证需要使用 `VmProcTable::vm_isokendpt()`。
///
/// # 参数
/// - `endpoint`: 要检查的端点
/// - `slot`: 预期的槽位号
///
/// # 返回值
/// - `true`: endpoint 的 slot 部分与给定的 slot 匹配
/// - `false`: 不匹配或 endpoint 无效
///
/// # 示例
/// ```
/// use minix_vm::vmproc::check_endpoint_slot;
/// use minix_types::{Endpoint, UserSlot};
///
/// let slot = UserSlot::new(5);
/// let endpoint = Endpoint::from_generation_slot(1, 5);
///
/// assert!(check_endpoint_slot(endpoint, slot));
/// assert!(!check_endpoint_slot(endpoint, UserSlot::new(3)));
/// ```
pub fn check_endpoint_slot(endpoint: Endpoint, slot: UserSlot) -> bool {
    // 特殊端点（NONE/ANY/SELF）不参与验证
    if !endpoint.is_valid() {
        return false;
    }
    
    // 提取 endpoint 中的 slot 部分并与给定的 slot 比较
    endpoint.slot() as usize == slot.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmproc_empty() {
        let proc = VmProc::empty(UserSlot::new(5));
        assert_eq!(proc.slot.get(), 5);
        assert!(proc.endpoint.is_none());
        assert!(!proc.is_in_use());
        assert!(!proc.is_exiting());
    }

    #[test]
    fn test_vmproc_flags() {
        let mut proc = VmProc::empty(UserSlot::new(0));
        assert!(!proc.is_in_use());

        proc.flags |= VmFlags::IN_USE;
        assert!(proc.is_in_use());

        proc.flags |= VmFlags::EXITING;
        assert!(proc.is_exiting());
    }

    #[test]
    fn test_vmproc_endpoint() {
        let mut proc = VmProc::empty(UserSlot::new(0));
        proc.endpoint = Endpoint::PM;
        proc.flags |= VmFlags::IN_USE;

        assert!(proc.endpoint.is_valid());
        assert!(proc.is_in_use());
    }

    #[test]
    fn test_acl_index() {
        let acl = AclIndex::new(42);
        assert_eq!(acl.get(), 42);
    }

    #[test]
    fn test_page_table() {
        let pt = PageTable::new();
        assert_eq!(pt.root_phys, 0);
        assert_eq!(pt.entry_count, 0);
    }

    // === check_endpoint_slot 测试 ===

    #[test]
    fn test_check_endpoint_slot_matching() {
        let slot = UserSlot::new(5);
        let endpoint = Endpoint::from_generation_slot(1, 5);

        assert!(check_endpoint_slot(endpoint, slot));
    }

    #[test]
    fn test_check_endpoint_slot_mismatch() {
        let slot = UserSlot::new(5);
        let endpoint = Endpoint::from_generation_slot(1, 3); // 不同的 slot

        assert!(!check_endpoint_slot(endpoint, slot));
    }

    #[test]
    fn test_check_endpoint_slot_invalid_endpoint() {
        // NONE, ANY, SELF 应该返回 false
        assert!(!check_endpoint_slot(Endpoint::NONE, UserSlot::new(0)));
        assert!(!check_endpoint_slot(Endpoint::ANY, UserSlot::new(0)));
        assert!(!check_endpoint_slot(Endpoint::SELF, UserSlot::new(0)));
    }

    #[test]
    fn test_check_endpoint_slot_with_generation() {
        // 不同 generation 但相同 slot 应该匹配
        let slot = UserSlot::new(10);
        let endpoint = Endpoint::from_generation_slot(5, 10);

        assert!(check_endpoint_slot(endpoint, slot));
    }

    // === 边界条件测试 ===

    #[test]
    fn test_slot_endpoint_consistency() {
        // 验证 slot 与 endpoint 的一致性
        let slot = UserSlot::new(5);
        let mut proc = VmProc::empty(slot);
        proc.endpoint = Endpoint::from_generation_slot(1, 5);
        proc.flags |= VmFlags::IN_USE;

        // 验证一致性
        assert!(check_endpoint_slot(proc.endpoint, proc.slot));
    }

    #[test]
    fn test_slot_endpoint_inconsistency() {
        // slot 与 endpoint 不匹配的情况
        let mut proc = VmProc::empty(UserSlot::new(5));
        proc.endpoint = Endpoint::from_generation_slot(1, 3); // slot 3，不是 5
        proc.flags |= VmFlags::IN_USE;

        // 应该不匹配
        assert!(!check_endpoint_slot(proc.endpoint, proc.slot));
    }

    #[test]
    fn test_vmproc_memory_limit() {
        let mut proc = VmProc::empty(UserSlot::new(1));
        proc.total_max = VirBytes(1024 * 1024); // 1MB 限制
        proc.total = VirBytes(512 * 1024); // 当前使用 512KB

        assert!(proc.total.0 <= proc.total_max.0);
    }

    #[test]
    fn test_vmproc_stats() {
        let mut proc = VmProc::empty(UserSlot::new(0));

        // 初始统计为 0
        assert_eq!(proc.minor_fault, 0);
        assert_eq!(proc.major_fault, 0);

        // 模拟页错误
        proc.minor_fault += 1;
        proc.major_fault += 1;

        assert_eq!(proc.minor_fault, 1);
        assert_eq!(proc.major_fault, 1);
    }

    #[cfg(feature = "vmstats")]
    #[test]
    fn test_vmproc_byte_copies() {
        let mut proc = VmProc::empty(UserSlot::new(0));
        proc.byte_copies = 1000;

        assert_eq!(proc.byte_copies, 1000);
    }
}
