//! Paging abstraction
//!
//! Defines the trait interface for page table management, including:
//! - Page table creation and destruction
//! - Page mapping and unmapping
//! - Page table switching
//! - Address translation

use minix_types::{PhysBytes, VirBytes};

bitflags::bitflags! {
    /// Page table entry flags
    ///
    /// OS-level semantic interface; each architecture's Paging implementation
    /// translates these into hardware PTE bit encodings internally.
    /// Uses bitflags (u16 backing) for memory efficiency and semantic clarity.
    ///
    /// Flags fall into two categories:
    ///
    /// **Status** (directly maps to hardware, consistent across architectures):
    /// - `PRESENT` / `WRITABLE` / `USER_ACCESSIBLE` / `ACCESSED` / `DIRTY`
    ///
    /// **Policy** (requires translation; some architectures use inverted logic):
    /// - `EXECUTABLE`: x86-64 NX bit (inverted), ARM64 PXN bit (inverted), RISC-V X bit (normal)
    /// - `GLOBAL`: x86-64 G bit (normal), ARM64 nG bit (inverted)
    /// - `WRITE_THROUGH` / `NO_CACHE`: cache policy, encoding varies greatly across architectures
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
    /// User-space read-only page: PTF_PRESENT | PTF_USER.
    /// Corresponds to Minix3 `PTF_PRESENT|PTF_USER` (no PTF_WRITE).
    pub const fn read_only() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }

    /// User-space read-write page: PTF_PRESENT | PTF_USER | PTF_WRITE.
    /// Corresponds to Minix3 `PTF_PRESENT|PTF_USER|PTF_WRITE`.
    pub const fn read_write() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::WRITABLE.bits() | Self::USER_ACCESSIBLE.bits()
        )
    }

    /// Kernel read-only page (global): PTF_PRESENT | PTF_GLOBAL.
    pub const fn kernel_read_only() -> Self {
        Self::from_bits_truncate(
            Self::PRESENT.bits() | Self::GLOBAL.bits()
        )
    }

    /// Kernel read-write page (global): PTF_PRESENT | PTF_WRITE | PTF_GLOBAL.
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

/// Page table error type
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

/// Page table management trait
///
/// Defines core page table operations; each architecture must implement this trait.
///
/// **Design choice**: The current trait uses "flat mapping" semantics — `map()` appears
/// as a single-step operation to the caller; on-demand allocation of intermediate page
/// tables (PDPT/PD/PT etc.) is handled internally by the implementation and not exposed
/// to the caller. This simplifies VM-layer usage, but means the caller cannot directly
/// control intermediate-level entries. If finer control is needed in the future (e.g.,
/// THP split/merge, migration entries), this trait can be extended or a new
/// `PagingLevel` trait can be introduced.
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

    /// Atomically replace an existing mapping or create a new one.
    ///
    /// Corresponds to Minix3's `pt_writemap()` with `WMF_OVERWRITE` flag,
    /// which is the default behavior in Minix3 — "overwrite mapping" is
    /// the norm, not the exception (region.c:285, pagetable.c:713,743).
    ///
    /// Unlike `map()` which returns `AlreadyMapped` if the virtual address
    /// is already mapped, `remap()` atomically replaces the old entry,
    /// avoiding the "no mapping" window that would exist between a manual
    /// `unmap()` + `map()` sequence.
    ///
    /// Returns the old physical address and flags if a mapping was replaced,
    /// or `None` if the virtual address was previously unmapped.
    fn remap(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<Option<(PhysBytes, PageFlags)>, PageTableError>;

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

    /// Map a range of consecutive virtual pages to consecutive physical pages.
    ///
    /// Provides a default implementation (calls `map()` per page); arch implementations
    /// may override to exploit hardware optimizations:
    /// - x86-64: single CR3 reload after batch mapping instead of multiple INVLPGs
    /// - ARM64: similar, can use TLBI range instructions
    ///
    /// Corresponds to Minix3 `pt_writemap()` batch mapping semantics.
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

    /// Unmap a range of consecutive virtual pages.
    ///
    /// Provides a default implementation (calls `unmap()` per page); arch implementations
    /// may override to exploit hardware optimizations, symmetric with `map_range`.
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
    // Minix3's pt_checkrange() has only one call site in the entire source,
    // wrapped in #if SANITYCHECKS (region.c:746-751, in map_pf()), making it
    // a debug-only assertion rather than a production API. It has no hardware
    // optimization opportunity (just a query() loop); the VM layer can
    // implement its own loop using query() when needed. Therefore it is not
    // included in the Paging trait, keeping the trait limited to operations
    // that hardware must provide semantics for.
    //
    // Original implementation preserved below for reference:
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



/// Mock paging implementation
///
/// Software-simulated implementation for user-space testing.
/// Does not operate on real hardware; maintains a mapping table in memory only.
///
/// **Thread model**: Not concurrency-safe. The `mappings` field uses `BTreeMap`
/// without locking, designed for single-threaded testing only (`#[cfg(test)]`).
/// Multi-threaded tests require external synchronization.
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

        fn remap(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
            -> Result<Option<(PhysBytes, PageFlags)>, PageTableError>
        {
            let v = vaddr.0;
            let p = paddr.0;

            if v % Self::PAGE_SIZE as u64 != 0 || p % Self::PAGE_SIZE as u64 != 0 {
                return Err(PageTableError::InvalidAddress);
            }

            let old = self.mappings.insert(v, (p, flags))
                .map(|(old_p, old_f)| (PhysBytes(old_p), old_f));
            Ok(old)
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
