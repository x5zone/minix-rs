//! 分页机制抽象
//!
//! 定义页表管理的trait接口，包括：
//! - 页表创建和销毁
//! - 页映射和取消映射
//! - 页表切换
//! - 地址转换

use minix_types::{PhysBytes, VirBytes};

bitflags::bitflags! {
    /// 页表项标志位
    ///
    /// OS 层的语义接口，各架构 Paging 实现内部负责将其翻译为硬件 PTE 位编码。
    /// 使用 bitflags（u16 底层）兼顾内存效率和语义清晰。
    ///
    /// 标志分为两类：
    ///
    /// **状态类**（直接映射硬件，语义跨架构一致）：
    /// - `PRESENT` / `WRITABLE` / `USER_ACCESSIBLE` / `ACCESSED` / `DIRTY`
    ///
    /// **策略类**（需要翻译，部分架构为反逻辑）：
    /// - `EXECUTABLE`：x86-64 为 NX 位（反逻辑），ARM64 为 PXN 位（反逻辑），RISC-V 为 X 位（正逻辑）
    /// - `GLOBAL`：x86-64 为 G 位（正逻辑），ARM64 为 nG 位（反逻辑）
    /// - `WRITE_THROUGH` / `NO_CACHE`：缓存策略，各架构编码差异大
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PageFlags: u16 {
        const PRESENT         = 1 << 0;
        const WRITABLE        = 1 << 1;
        const USER_ACCESSIBLE = 1 << 2;
        const EXECUTABLE      = 1 << 3;
        const GLOBAL          = 1 << 4;
        const WRITE_THROUGH   = 1 << 5;
        const NO_CACHE        = 1 << 6;
        const ACCESSED        = 1 << 7;
        const DIRTY           = 1 << 8;
    }
}

impl PageFlags {
    pub const fn read_only() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }

    pub const fn read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }

    pub const fn kernel_read_only() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::GLOBAL.bits()
        )
    }

    pub const fn kernel_read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::GLOBAL.bits()
        )
    }

    /// Kernel code pages: present + executable + global, but NOT writable.
    ///
    /// Kernel code pages should be read-execute only (W^X principle).
    /// For writable executable pages (e.g., kernel module loading, ftrace),
    /// construct the flags manually.
    pub const fn kernel_executable() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::EXECUTABLE.bits() | Self::GLOBAL.bits()
        )
    }
}

/// 页表错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableError {
    InvalidAddress,
    AlreadyMapped,
    NotMapped,
    AllocationFailed,
    PermissionDenied,
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
///
/// **设计选择**：当前 trait 采用"扁平映射"语义——`map()` 在调用者看来
/// 是单步操作，中间页表（PDPT/PD/PT 等）的按需分配由实现内部处理，
/// 不暴露给调用者。这简化了 VM 层的使用，但意味着调用者无法直接控制
/// 中间层条目。若未来需要 THP split/merge、migration entry 等精细控制，
/// 可扩展此 trait 或引入新的 `PagingLevel` trait。
pub trait Paging {
    const PAGE_SIZE: usize;

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized;

    /// Destroy this page table and release all resources.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - This page table is not currently active on any CPU
    /// - The page table has been unbound from any process
    /// - All mappings have been properly unmapped, or caller accepts memory leak
    unsafe fn destroy(&mut self);

    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError>;

    fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags)
        -> Result<(), PageTableError>;

    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;

    fn root_paddr(&self) -> PhysBytes;

    /// Activate this page table on the current CPU.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The page table is fully initialized (kernel mappings present)
    /// - On SMP systems, proper TLB invalidation is performed after switch
    /// - No stale references to the old page table's mappings are in use
    unsafe fn switch(&self);

    /// Flush TLB entries associated with this page table.
    ///
    /// Without ASID, equivalent to a global TLB flush.
    /// With PCID/ASID, only flushes entries for the current address space.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - This is called in a valid MMU context (a page table is active)
    /// - Global flush affects all address spaces; use with care on SMP
    unsafe fn flush_tlb(&self);

    /// Flush the TLB entry for a single virtual address.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - `vaddr` falls within the currently active page table's valid range
    /// - This is called in a valid MMU context (a page table is active)
    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes);

    /// 批量映射连续虚拟页到连续物理页
    ///
    /// 提供默认实现（逐页调用 `map()`），arch 实现可覆盖以利用硬件优化：
    /// - x86-64：批量映射后单次 CR3 reload 替代多次 INVLPG，减少 TLB 刷新开销
    /// - ARM64：类似，可利用 TLBI range 指令
    ///
    /// 对应 Minix3 `pt_writemap()` 的批量映射语义。
    fn map_range(
        &mut self,
        vaddr_start: VirBytes,
        paddr_start: PhysBytes,
        pages: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        for i in 0..pages {
            let v = VirBytes(vaddr_start.0 + (i * Self::PAGE_SIZE) as u64);
            let p = PhysBytes(paddr_start.0 + (i * Self::PAGE_SIZE) as u64);
            self.map(v, p, flags)?;
        }
        Ok(())
    }

    /// 批量取消映射连续虚拟页
    ///
    /// 提供默认实现（逐页调用 `unmap()`），arch 实现可覆盖以利用硬件优化，
    /// 与 `map_range` 对称。
    fn unmap_range(
        &mut self,
        vaddr_start: VirBytes,
        pages: usize,
    ) -> Result<(), PageTableError> {
        for i in 0..pages {
            let v = VirBytes(vaddr_start.0 + (i * Self::PAGE_SIZE) as u64);
            self.unmap(v)?;
        }
        Ok(())
    }

    // REMOVED: check_range — 2026-05
    //
    // Minix3 原版 pt_checkrange() 在全源码中仅有一处调用，且被 #if SANITYCHECKS
    // 包裹（region.c:746-751，在 map_pf() 中），属于 debug-only 断言而非生产 API。
    // 该函数无硬件优化空间（仅是 query() 的循环），VM 层需要时可自行循环 query()
    // 实现。因此不纳入 Paging trait，保持 trait 只包含硬件必须提供语义的操作。
    //
    // 原实现保留如下供参考：
    //
    // fn check_range(
    //     &self,
    //     vaddr_start: VirBytes,
    //     pages: usize,
    //     require_writable: bool,
    // ) -> Result<(), PageTableError> {
    //     for i in 0..pages {
    //         let v = VirBytes(vaddr_start.0 + (i * Self::PAGE_SIZE) as u64);
    //         match self.query(v) {
    //             Some((_, flags)) => {
    //                 if require_writable && !flags.contains(PageFlags::WRITABLE) {
    //                     return Err(PageTableError::PermissionDenied);
    //                 }
    //             }
    //             None => return Err(PageTableError::NotMapped),
    //         }
    //     }
    //     Ok(())
    // }
}

/// Page table statistics.
///
/// Corresponds to Minix3's `memstats()`/`total_pages`/`free_pages` tracking
/// used by `printmemstats()` and `vm_info` sysctl. Fields will be populated
/// by a future `Paging::stats()` method.
#[derive(Debug, Clone, Copy, Default)]
pub struct PageTableStats {
    pub(crate) mapped_pages: usize,
    pub(crate) used_page_tables: usize,
    pub(crate) total_page_tables: usize,
}



/// Mock分页实现
///
/// 用于用户态测试的软件模拟实现。
/// 不操作真实硬件，仅在内存中维护映射表。
///
/// **线程模型**：非并发安全。`mappings` 字段使用 `BTreeMap` 且未加锁，
/// 仅设计用于单线程测试（`#[cfg(test)]`）。多线程测试需外部同步。
#[cfg(feature = "mock")]
pub mod mock {
    use super::*;
    use crate::paging_ext::PagingWithId;
    use alloc::collections::BTreeMap;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    pub struct MockPaging {
        id: usize,
        mappings: BTreeMap<u64, (u64, PageFlags)>,
        root_phys: u64,
    }

    static MOCK_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static ACTIVE_MOCK_TABLE: AtomicUsize = AtomicUsize::new(usize::MAX);

    const NO_ACTIVE_TABLE: usize = usize::MAX;

    impl MockPaging {
        pub(crate) fn new_mock() -> Result<Self, PageTableError> {
            let id = MOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(Self {
                id,
                mappings: BTreeMap::new(),
                root_phys: 0x1000 + (id as u64 * 0x1000),
            })
        }

        pub fn id(&self) -> usize {
            self.id
        }

        pub fn mappings(&self) -> &BTreeMap<u64, (u64, PageFlags)> {
            &self.mappings
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
            self.mappings.clear();
            if ACTIVE_MOCK_TABLE.load(Ordering::SeqCst) == self.id {
                ACTIVE_MOCK_TABLE.store(NO_ACTIVE_TABLE, Ordering::SeqCst);
            }
        }

        fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
            -> Result<(), PageTableError>
        {
            let v = vaddr.0;
            let p = paddr.0;

            if v % Self::PAGE_SIZE as u64 != 0 || p % Self::PAGE_SIZE as u64 != 0 {
                return Err(PageTableError::InvalidAddress);
            }

            if self.mappings.contains_key(&v) {
                return Err(PageTableError::AlreadyMapped);
            }

            self.mappings.insert(v, (p, flags));
            Ok(())
        }

        fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
            let v = vaddr.0;

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
            match self.mappings.get_mut(&vaddr.0) {
                Some((_, f)) => {
                    *f = flags;
                    Ok(())
                }
                None => Err(PageTableError::NotMapped),
            }
        }

        fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
            self.mappings.get(&vaddr.0).map(|(p, f)| (PhysBytes(*p), *f))
        }

        fn root_paddr(&self) -> PhysBytes {
            PhysBytes(self.root_phys)
        }

        unsafe fn switch(&self) {
            ACTIVE_MOCK_TABLE.store(self.id, Ordering::SeqCst);
        }

        unsafe fn flush_tlb(&self) {}

        unsafe fn flush_tlb_addr(&self, _vaddr: VirBytes) {}
    }

    impl crate::paging_ext::VmPagingExt for MockPaging {
        fn bind_to_process(&self, _endpoint: minix_types::Endpoint) -> Result<(), PageTableError> {
            Ok(())
        }

        fn map_kernel(&mut self) -> Result<(), PageTableError> {
            const MOCK_KERNEL_VBASE: u64 = 0xFFFF_8000_0000_0000;
            const MOCK_KERNEL_PBASE: u64 = 0x100_0000;
            const MOCK_KERNEL_PAGES: usize = 16;

            for i in 0..MOCK_KERNEL_PAGES {
                let vaddr = VirBytes(MOCK_KERNEL_VBASE + i as u64 * Self::PAGE_SIZE as u64);
                let paddr = PhysBytes(MOCK_KERNEL_PBASE + i as u64 * Self::PAGE_SIZE as u64);
                let flags = PageFlags::kernel_read_write();
                self.map(vaddr, paddr, flags)?;
            }
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MockAsid(u32);

    static MOCK_ASID_COUNTER: AtomicUsize = AtomicUsize::new(1);

    impl PagingWithId for MockPaging {
        type AddressSpaceId = MockAsid;

        fn alloc_asid(&self) -> Result<MockAsid, PageTableError> {
            let id = MOCK_ASID_COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(MockAsid(id as u32))
        }

        fn free_asid(&self, _id: MockAsid) {}

        unsafe fn switch_with_asid(&self, _id: MockAsid) {
            self.switch();
        }

        unsafe fn flush_tlb_asid(&self, _id: MockAsid) {}

        unsafe fn flush_tlb_addr_asid(&self, _vaddr: VirBytes, _id: MockAsid) {}
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

            assert!(pt.map(vaddr, paddr, flags).is_ok());

            let result = pt.query(vaddr);
            assert!(result.is_some());
            let (p, f) = result.unwrap();
            assert_eq!(p, paddr);
            assert!(f.contains(PageFlags::WRITABLE));

            let unmapped = pt.unmap(vaddr);
            assert!(unmapped.is_ok());
            assert_eq!(unmapped.unwrap(), paddr);

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
            assert_eq!(pt.unmap(VirBytes(0x1000)), Err(PageTableError::NotMapped));
        }

        #[test]
        fn test_mock_update_flags() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);

            pt.map(vaddr, paddr, PageFlags::read_write()).unwrap();
            pt.update_flags(vaddr, PageFlags::read_only()).unwrap();

            let (_, flags) = pt.query(vaddr).unwrap();
            assert!(!flags.contains(PageFlags::WRITABLE));
            assert!(flags.contains(PageFlags::PRESENT));
        }

        #[test]
        fn test_mock_switch() {
            let pt = MockPaging::new().unwrap();
            unsafe {
                pt.switch();
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

            let vaddr = VirBytes(0x1001);
            let paddr = PhysBytes(0x2000);
            assert_eq!(
                pt.map(vaddr, paddr, PageFlags::read_write()),
                Err(PageTableError::InvalidAddress)
            );
        }

        #[test]
        fn test_page_flags_presets() {
            let ro = PageFlags::read_only();
            assert!(ro.contains(PageFlags::PRESENT));
            assert!(!ro.contains(PageFlags::WRITABLE));
            assert!(ro.contains(PageFlags::USER_ACCESSIBLE));

            let rw = PageFlags::read_write();
            assert!(rw.contains(PageFlags::PRESENT));
            assert!(rw.contains(PageFlags::WRITABLE));
            assert!(rw.contains(PageFlags::USER_ACCESSIBLE));

            let kro = PageFlags::kernel_read_only();
            assert!(kro.contains(PageFlags::PRESENT));
            assert!(!kro.contains(PageFlags::WRITABLE));
            assert!(!kro.contains(PageFlags::USER_ACCESSIBLE));
            assert!(kro.contains(PageFlags::GLOBAL));

            let krw = PageFlags::kernel_read_write();
            assert!(krw.contains(PageFlags::PRESENT));
            assert!(krw.contains(PageFlags::WRITABLE));
            assert!(!krw.contains(PageFlags::USER_ACCESSIBLE));
            assert!(krw.contains(PageFlags::GLOBAL));
        }

        #[test]
        fn test_page_flags_combination() {
            let flags = PageFlags::read_write() | PageFlags::EXECUTABLE;
            assert!(flags.contains(PageFlags::EXECUTABLE));
            assert!(flags.contains(PageFlags::WRITABLE));

            let cow_flags = PageFlags::read_write() - PageFlags::WRITABLE;
            assert!(!cow_flags.contains(PageFlags::WRITABLE));
            assert!(cow_flags.contains(PageFlags::PRESENT));
        }

        #[test]
        fn test_page_flags_size() {
            assert_eq!(core::mem::size_of::<PageFlags>(), 2);
        }
    }
}
