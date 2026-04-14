//! 页表管理模块（已弃用）
//!
//! ⚠️ **注意**: 此模块已被 `minix-arch` crate 替代。
//! 新的代码应该使用 `minix_arch::paging::Paging` trait。
//!
//! 保留此模块仅用于向后兼容，将在后续版本中移除。
//!
//! 对应 Minix3: `pt.h`, `pagetable.c`

use crate::phys_mem::PhysAddr;
use minix_types::VirBytes;

/// 页大小：4KB
pub const PAGE_SIZE: usize = 4096;

/// 页表项数量：1024
pub const PT_ENTRIES: usize = 1024;

/// 页目录项数量：1024
pub const PD_ENTRIES: usize = 1024;

/// 页表偏移位数：12
pub const PAGE_SHIFT: usize = 12;

/// 页目录索引偏移：22
pub const PD_SHIFT: usize = 22;

/// 页表索引掩码：0x3FF
pub const PT_MASK: usize = 0x3FF;

/// 页表项标志位
///
/// 对应分页机制的标志位定义。
/// 注意：具体标志位的含义由硬件架构决定，此处为通用定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtFlags(u32);

impl PtFlags {
    /// 页存在位
    pub const PRESENT: u32 = 0x001;
    /// 读写权限位
    pub const WRITE: u32 = 0x002;
    /// 用户模式访问位
    pub const USER: u32 = 0x004;
    /// 写透位
    pub const PWT: u32 = 0x008;
    /// 缓存禁用位
    pub const PCD: u32 = 0x010;
    /// 访问位
    pub const ACCESSED: u32 = 0x020;
    /// 脏页位
    pub const DIRTY: u32 = 0x040;
    /// 4MB 大页位（页目录项）
    pub const BIGPAGE: u32 = 0x080;
    /// 全局页位
    pub const GLOBAL: u32 = 0x100;

    /// 创建空标志
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 创建包含指定标志的 PtFlags
    pub const fn new(flags: u32) -> Self {
        Self(flags)
    }

    /// 检查是否包含指定标志
    pub fn contains(&self, flags: u32) -> bool {
        (self.0 & flags) == flags
    }

    /// 添加标志
    pub fn insert(&mut self, flags: u32) {
        self.0 |= flags;
    }

    /// 移除标志
    pub fn remove(&mut self, flags: u32) {
        self.0 &= !flags;
    }

    /// 获取原始值
    pub fn bits(&self) -> u32 {
        self.0
    }
}

impl Default for PtFlags {
    fn default() -> Self {
        Self::empty()
    }
}

/// 页表结构
///
/// 对应 Minix3: `struct pt_t`
#[derive(Debug)]
pub struct PageTable {
    /// 页目录虚拟地址
    pub dir: *mut u32,
    /// 页目录物理地址（用于激活页表）
    pub dir_phys: PhysAddr,
    /// 页表虚拟地址数组
    ///
    /// 每个元素指向一个页表的虚拟地址。
    /// 如果为 null，表示该页目录项未分配页表。
    pub tables: [*mut u32; PD_ENTRIES],
    /// 虚拟地址分配提示
    ///
    /// 查找虚拟地址空间空洞时的起始位置。
    /// 这是一个优化提示，不是必须使用的值。
    pub vartop: VirBytes,
}

// PageTable 包含原始指针，不是 Send/Sync
// 注意：在 stable Rust 中不能显式 impl !Send/!Sync
// 但包含原始指针会自动使类型 !Send + !Sync

impl PageTable {
    /// 创建新的页表结构
    ///
    /// # Safety
    ///
    /// 调用者必须确保 dir 和 dir_phys 指向有效的页目录。
    pub unsafe fn new(dir: *mut u32, dir_phys: PhysAddr) -> Self {
        Self {
            dir,
            dir_phys,
            tables: [core::ptr::null_mut(); PD_ENTRIES],
            vartop: VirBytes(0),
        }
    }

    /// 获取页目录项索引
    ///
    /// 从虚拟地址提取高10位作为页目录索引
    pub fn pde_index(vaddr: VirBytes) -> usize {
        (vaddr.0 as usize) >> PD_SHIFT
    }

    /// 获取页表项索引
    ///
    /// 从虚拟地址提取中10位作为页表索引
    pub fn pte_index(vaddr: VirBytes) -> usize {
        ((vaddr.0 as usize) >> PAGE_SHIFT) & PT_MASK
    }

    /// 获取页内偏移
    ///
    /// 从虚拟地址提取低12位作为页内偏移
    pub fn page_offset(vaddr: VirBytes) -> usize {
        (vaddr.0 as usize) & (PAGE_SIZE - 1)
    }

    /// 将物理地址转换为页框号
    pub fn phys_to_pfn(phys: PhysAddr) -> u32 {
        (phys.0 >> PAGE_SHIFT) as u32
    }

    /// 将页框号转换为物理地址
    pub fn pfn_to_phys(pfn: u32) -> PhysAddr {
        PhysAddr((pfn as u64) << PAGE_SHIFT)
    }

    /// 创建页表项值
    ///
    /// 将物理页框号和标志组合成页表项值
    pub fn make_pte(phys: PhysAddr, flags: PtFlags) -> u32 {
        (Self::phys_to_pfn(phys) << PAGE_SHIFT) | (flags.bits() & 0xFFF)
    }

    /// 创建页目录项值
    ///
    /// 将页表物理地址和标志组合成页目录项值
    pub fn make_pde(pt_phys: PhysAddr, flags: PtFlags) -> u32 {
        (Self::phys_to_pfn(pt_phys) << PAGE_SHIFT) | (flags.bits() & 0xFFF)
    }

    /// 从页表项提取物理地址
    pub fn pte_phys(pte: u32) -> PhysAddr {
        PhysAddr(((pte as u64) & !0xFFF) as u64)
    }

    /// 从页表项提取标志
    pub fn pte_flags(pte: u32) -> PtFlags {
        PtFlags(pte & 0xFFF)
    }

    /// 检查页表项是否有效（存在位设置）
    pub fn is_present(pte: u32) -> bool {
        (pte & PtFlags::PRESENT) != 0
    }

    /// 对齐地址到页边界
    pub fn page_align(addr: VirBytes) -> VirBytes {
        VirBytes((addr.0 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1))
    }

    /// 向下对齐地址到页边界
    pub fn page_align_down(addr: VirBytes) -> VirBytes {
        VirBytes(addr.0 & !(PAGE_SIZE as u64 - 1))
    }
}

/// 页表操作错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableError {
    /// 无效地址
    InvalidAddress,
    /// 页表未映射
    NotMapped,
    /// 权限不足
    PermissionDenied,
    /// 分配失败
    AllocationFailed,
}

impl core::fmt::Display for PageTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidAddress => write!(f, "Invalid address"),
            Self::NotMapped => write!(f, "Page not mapped"),
            Self::PermissionDenied => write!(f, "Permission denied"),
            Self::AllocationFailed => write!(f, "Allocation failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_constants() {
        assert_eq!(PAGE_SIZE, 4096);
        assert_eq!(PT_ENTRIES, 1024);
        assert_eq!(PD_ENTRIES, 1024);
        assert_eq!(1 << PAGE_SHIFT, PAGE_SIZE);
    }

    #[test]
    fn test_pt_flags() {
        let mut flags = PtFlags::empty();
        assert!(!flags.contains(PtFlags::PRESENT));
        
        flags.insert(PtFlags::PRESENT | PtFlags::WRITE);
        assert!(flags.contains(PtFlags::PRESENT));
        assert!(flags.contains(PtFlags::WRITE));
        assert!(!flags.contains(PtFlags::USER));
        
        flags.remove(PtFlags::WRITE);
        assert!(!flags.contains(PtFlags::WRITE));
    }

    #[test]
    fn test_address_calculation() {
        // 测试地址索引计算
        let vaddr = VirBytes(0x12345678);
        
        assert_eq!(PageTable::pde_index(vaddr), 0x48);  // 0x12345678 >> 22 = 0x48
        assert_eq!(PageTable::pte_index(vaddr), 0x345); // (0x12345678 >> 12) & 0x3FF = 0x345
        assert_eq!(PageTable::page_offset(vaddr), 0x678); // 0x12345678 & 0xFFF = 0x678
    }

    #[test]
    fn test_pte_operations() {
        let phys = PhysAddr(0x12345000);
        let flags = PtFlags::new(PtFlags::PRESENT | PtFlags::WRITE);
        
        let pte = PageTable::make_pte(phys, flags);
        assert!(PageTable::is_present(pte));
        assert!(PageTable::pte_flags(pte).contains(PtFlags::WRITE));
        assert_eq!(PageTable::pte_phys(pte), phys);
    }

    #[test]
    fn test_page_align() {
        assert_eq!(PageTable::page_align(VirBytes(0x1234)), VirBytes(0x2000));
        assert_eq!(PageTable::page_align(VirBytes(0x1000)), VirBytes(0x1000));
        assert_eq!(PageTable::page_align_down(VirBytes(0x1234)), VirBytes(0x1000));
    }
}
