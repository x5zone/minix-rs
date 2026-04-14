//! 分页机制抽象
//!
//! 定义页表管理的trait接口，包括：
//! - 页表创建和销毁
//! - 页映射和取消映射
//! - 页表切换
//! - 地址转换

use minix_types::{PhysBytes, VirBytes};

/// 页表项标志位
///
/// 各架构的具体标志位可能不同，但核心语义一致：
/// - Present: 页是否存在
/// - Writable: 是否可写
/// - UserAccessible: 用户态是否可访问
/// - Executable: 是否可执行
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageFlags {
    /// 页存在
    pub present: bool,
    /// 可写
    pub writable: bool,
    /// 用户态可访问
    pub user_accessible: bool,
    /// 可执行
    pub executable: bool,
    /// 全局页（地址转换缓存不刷新）
    pub global: bool,
    /// 写透（Write-through）
    pub write_through: bool,
    /// 禁用缓存
    pub no_cache: bool,
    /// 已访问（硬件设置）
    pub accessed: bool,
    /// 已修改（硬件设置）
    pub dirty: bool,
}

impl PageFlags {
    /// 空标志
    pub const fn empty() -> Self {
        Self {
            present: false,
            writable: false,
            user_accessible: false,
            executable: false,
            global: false,
            write_through: false,
            no_cache: false,
            accessed: false,
            dirty: false,
        }
    }

    /// 只读页
    pub const fn read_only() -> Self {
        Self {
            present: true,
            writable: false,
            user_accessible: true,
            executable: false,
            global: false,
            write_through: false,
            no_cache: false,
            accessed: false,
            dirty: false,
        }
    }

    /// 可读写页
    pub const fn read_write() -> Self {
        Self {
            present: true,
            writable: true,
            user_accessible: true,
            executable: false,
            global: false,
            write_through: false,
            no_cache: false,
            accessed: false,
            dirty: false,
        }
    }
}

/// 页表错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableError {
    /// 无效地址
    InvalidAddress,
    /// 页已存在
    AlreadyMapped,
    /// 页不存在
    NotMapped,
    /// 分配失败
    AllocationFailed,
    /// 权限不足
    PermissionDenied,
    /// 架构不支持
    NotSupported,
}

impl core::fmt::Display for PageTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidAddress => write!(f, "Invalid address"),
            Self::AlreadyMapped => write!(f, "Page already mapped"),
            Self::NotMapped => write!(f, "Page not mapped"),
            Self::AllocationFailed => write!(f, "Allocation failed"),
            Self::PermissionDenied => write!(f, "Permission denied"),
            Self::NotSupported => write!(f, "Operation not supported"),
        }
    }
}

/// 页表管理trait
///
/// 定义页表的核心操作，各架构需要实现此trait。
/// 这是OS与硬件分页机制的抽象接口。
pub trait Paging {
    /// 页大小（通常为4KB）
    const PAGE_SIZE: usize;

    /// 创建新的页表
    ///
    /// # 返回值
    /// - `Ok(table_id)`: 页表标识符
    /// - `Err(_)`: 创建失败
    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized;

    /// 销毁页表
    ///
    /// # Safety
    /// 调用者必须确保此页表不再被使用（包括当前未激活此页表）
    unsafe fn destroy(&mut self);

    /// 映射虚拟地址到物理地址
    ///
    /// # 参数
    /// - `vaddr`: 虚拟地址（页对齐）
    /// - `paddr`: 物理地址（页对齐）
    /// - `flags`: 页标志
    ///
    /// # 返回值
    /// - `Ok(())`: 映射成功
    /// - `Err(AlreadyMapped)`: 该虚拟地址已映射
    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 取消映射虚拟地址
    ///
    /// # 参数
    /// - `vaddr`: 虚拟地址（页对齐）
    ///
    /// # 返回值
    /// - `Ok(paddr)`: 取消映射成功，返回原物理地址
    /// - `Err(NotMapped)`: 该虚拟地址未映射
    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError>;

    /// 更新页标志
    ///
    /// # 参数
    /// - `vaddr`: 虚拟地址
    /// - `flags`: 新标志
    ///
    /// # 返回值
    /// - `Ok(())`: 更新成功
    /// - `Err(NotMapped)`: 该虚拟地址未映射
    fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    /// 查询虚拟地址的映射信息
    ///
    /// # 参数
    /// - `vaddr`: 虚拟地址
    ///
    /// # 返回值
    /// - `Some((paddr, flags))`: 映射信息
    /// - `None`: 未映射
    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;

    /// 获取页表根物理地址（用于激活页表）
    ///
    /// # 返回值
    /// 页目录/页表的物理地址
    fn root_paddr(&self) -> PhysBytes;

    /// 激活此页表
    ///
    /// # Safety
    /// 这是特权操作，只能在内核态执行。
    /// 调用者必须确保新页表包含有效的内核映射。
    unsafe fn switch(&self);

    /// 刷新地址转换缓存（整个页表）
    ///
    /// # Safety
    /// 特权操作
    unsafe fn flush_tlb(&self);

    /// 刷新指定虚拟地址的地址转换缓存项
    ///
    /// # Safety
    /// 特权操作
    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes);
}

/// 页表统计信息
#[derive(Debug, Clone, Copy, Default)]
pub struct PageTableStats {
    /// 已映射页数
    pub mapped_pages: usize,
    /// 已使用页表数
    pub used_page_tables: usize,
    /// 总页表数
    pub total_page_tables: usize,
}

/// Mock分页实现
///
/// 用于用户态测试的软件模拟实现。
/// 不操作真实硬件，仅在内存中维护映射表。
#[cfg(feature = "mock")]
pub mod mock {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    /// Mock页表实现
    #[derive(Debug)]
    pub struct MockPaging {
        /// 页表ID
        id: usize,
        /// 虚拟地址到(物理地址, 标志)的映射
        mappings: BTreeMap<u64, (u64, PageFlags)>,
        /// 页表根物理地址（模拟）
        root_phys: u64,
    }

    /// 全局Mock页表ID计数器
    static mut MOCK_ID_COUNTER: usize = 0;

    /// 当前活动的Mock页表（模拟页表根寄存器）
    static mut ACTIVE_MOCK_TABLE: Option<usize> = None;

    impl MockPaging {
        /// 创建新的Mock页表
        pub fn new_mock() -> Result<Self, PageTableError> {
            unsafe {
                let id = MOCK_ID_COUNTER;
                MOCK_ID_COUNTER += 1;

                Ok(Self {
                    id,
                    mappings: BTreeMap::new(),
                    root_phys: 0x1000 + (id as u64 * 0x1000), // 模拟物理地址
                })
            }
        }

        /// 获取页表ID
        pub fn id(&self) -> usize {
            self.id
        }

        /// 获取所有映射（用于测试）
        pub fn mappings(&self) -> &BTreeMap<u64, (u64, PageFlags)> {
            &self.mappings
        }

        /// 对齐地址到页边界
        fn page_align(addr: u64) -> u64 {
            (addr + Self::PAGE_SIZE as u64 - 1) & !(Self::PAGE_SIZE as u64 - 1)
        }

        /// 向下对齐地址
        fn page_align_down(addr: u64) -> u64 {
            addr & !(Self::PAGE_SIZE as u64 - 1)
        }
    }

    impl Paging for MockPaging {
        const PAGE_SIZE: usize = 4096;

        fn new() -> Result<Self, PageTableError>
        where
            Self: Sized,
        {
            Self::new_mock()
        }

        unsafe fn destroy(&mut self) {
            // Mock实现：清空映射即可
            self.mappings.clear();

            // 如果当前活动页表是此表，清除它
            if ACTIVE_MOCK_TABLE == Some(self.id) {
                ACTIVE_MOCK_TABLE = None;
            }
        }

        fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
            -> Result<(), PageTableError>
        {
            let v = vaddr.0;
            let p = paddr.0;

            // 检查页对齐
            if v % Self::PAGE_SIZE as u64 != 0 || p % Self::PAGE_SIZE as u64 != 0 {
                return Err(PageTableError::InvalidAddress);
            }

            // 检查是否已映射
            if self.mappings.contains_key(&v) {
                return Err(PageTableError::AlreadyMapped);
            }

            self.mappings.insert(v, (p, flags));
            Ok(())
        }

        fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
            let v = vaddr.0;

            // 检查页对齐
            if v % Self::PAGE_SIZE as u64 != 0 {
                return Err(PageTableError::InvalidAddress);
            }

            match self.mappings.remove(&v) {
                Some((p, _)) => Ok(PhysBytes(p)),
                None => Err(PageTableError::NotMapped),
            }
        }

        fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags)
            -> Result<(), PageTableError>
        {
            let v = vaddr.0;

            match self.mappings.get_mut(&v) {
                Some((_, f)) => {
                    *f = flags;
                    Ok(())
                }
                None => Err(PageTableError::NotMapped),
            }
        }

        fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
            let v = vaddr.0;
            self.mappings.get(&v).map(|(p, f)| (PhysBytes(*p), *f))
        }

        fn root_paddr(&self) -> PhysBytes {
            PhysBytes(self.root_phys)
        }

        unsafe fn switch(&self) {
            // Mock实现：记录当前活动页表
            ACTIVE_MOCK_TABLE = Some(self.id);
        }

        unsafe fn flush_tlb(&self) {
            // Mock实现：无操作
        }

        unsafe fn flush_tlb_addr(&self, _vaddr: VirBytes) {
            // Mock实现：无操作
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_mock_paging_new() {
            let pt = MockPaging::new();
            assert!(pt.is_ok());
        }

        #[test]
        fn test_mock_map_unmap() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);
            let flags = PageFlags::read_write();

            // 映射
            assert!(pt.map(vaddr, paddr, flags).is_ok());

            // 查询
            let result = pt.query(vaddr);
            assert!(result.is_some());
            let (p, f) = result.unwrap();
            assert_eq!(p, paddr);
            assert!(f.writable);

            // 取消映射
            let unmapped = pt.unmap(vaddr);
            assert!(unmapped.is_ok());
            assert_eq!(unmapped.unwrap(), paddr);

            // 再次查询应失败
            assert!(pt.query(vaddr).is_none());
        }

        #[test]
        fn test_mock_double_map() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);
            let flags = PageFlags::read_write();

            assert!(pt.map(vaddr, paddr, flags).is_ok());
            assert_eq!(
                pt.map(vaddr, paddr, flags),
                Err(PageTableError::AlreadyMapped)
            );
        }

        #[test]
        fn test_mock_unmapped() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);

            assert_eq!(pt.unmap(vaddr), Err(PageTableError::NotMapped));
        }

        #[test]
        fn test_mock_update_flags() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);

            // 初始为可写
            pt.map(vaddr, paddr, PageFlags::read_write()).unwrap();

            // 更新为只读
            pt.update_flags(vaddr, PageFlags::read_only()).unwrap();

            // 验证
            let (_, flags) = pt.query(vaddr).unwrap();
            assert!(!flags.writable);
            assert!(flags.present);
        }

        #[test]
        fn test_mock_switch() {
            let pt = MockPaging::new().unwrap();
            let id = pt.id();

            unsafe {
                pt.switch();
                // 在Mock中无法直接验证，但不应panic
            }
        }

        #[test]
        fn test_mock_root_paddr() {
            let pt = MockPaging::new().unwrap();
            let root = pt.root_paddr();
            assert!(root.0 > 0);
        }

        #[test]
        fn test_mock_alignment_check() {
            let mut pt = MockPaging::new().unwrap();

            // 未对齐的地址应该失败
            let vaddr = VirBytes(0x1001);
            let paddr = PhysBytes(0x2000);
            assert_eq!(
                pt.map(vaddr, paddr, PageFlags::read_write()),
                Err(PageTableError::InvalidAddress)
            );
        }
    }
}
