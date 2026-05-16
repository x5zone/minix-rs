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
        const GUARD_PAGE      = 1 << 9;
    }
}

impl core::fmt::Display for PageFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut first = true;
        let mut flag = |name: &str, present: bool| -> core::fmt::Result {
            if present {
                if !first { write!(f, "|")?; }
                first = false;
                write!(f, "{}", name)?;
            }
            Ok(())
        };
        flag("P", self.contains(Self::PRESENT))?;
        flag("W", self.contains(Self::WRITABLE))?;
        flag("U", self.contains(Self::USER_ACCESSIBLE))?;
        flag("X", self.contains(Self::EXECUTABLE))?;
        flag("G", self.contains(Self::GLOBAL))?;
        flag("WT", self.contains(Self::WRITE_THROUGH))?;
        flag("NC", self.contains(Self::NO_CACHE))?;
        flag("A", self.contains(Self::ACCESSED))?;
        flag("D", self.contains(Self::DIRTY))?;
        flag("GUARD", self.contains(Self::GUARD_PAGE))?;
        if first { write!(f, "0")?; }
        Ok(())
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

    /// Replace an existing mapping or create a new one in a single operation.
    ///
    /// Corresponds to Minix3's `pt_writemap()` with `WMF_OVERWRITE` flag,
    /// which is the default behavior in Minix3 — "overwrite mapping" is
    /// the norm, not the exception (region.c:285, pagetable.c:713,743).
    ///
    /// Unlike `map()` which returns `AlreadyMapped` if the virtual address
    /// is already mapped, `remap()` replaces the old entry in one step,
    /// avoiding the "no mapping" window that would exist between a manual
    /// `unmap()` + `map()` sequence.
    ///
    /// **Atomicity note**: In the single-threaded event loop model, "atomic"
    /// means "single logical operation" — no intermediate state visible to
    /// the caller. At the hardware level, x86-64 PTE writes are 8-byte
    /// naturally aligned and thus atomic per Intel SDM Vol3 §4.10.4. On
    /// ARM64, a single PTE write is also atomic. Therefore `remap()` is
    /// both logically and hardware-atomic in our single-threaded model.
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
    /// Provides a default implementation with Minix3-style all-or-nothing semantics:
    /// first validates that none of the target addresses are already mapped (via `query()`),
    /// then writes all mappings. If validation fails, no PTEs are written.
    ///
    /// This two-phase approach mirrors Minix3's `pt_writemap` which calls
    /// `pt_ptalloc_in_range` first to ensure all second-level page tables exist
    /// before writing any PTEs — if allocation fails, no partial mappings are left.
    ///
    /// Arch implementations may override to merge validation and writing into a
    /// single pass (e.g., x86-64 can check and write PTEs in one traversal), but
    /// must preserve the all-or-nothing guarantee: on failure, no partial mappings remain.
    ///
    /// Corresponds to Minix3 `pt_writemap()` batch mapping semantics.
    fn map_range(
        &mut self,
        vaddr_start: VirBytes,
        paddr_start: PhysBytes,
        pages: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        // Phase 1: Pre-validate — ensure no address in the range is already mapped
        for i in 0..pages {
            let offset = (i as u64)
                .checked_mul(Self::PAGE_SIZE as u64)
                .ok_or(PageTableError::InvalidAddress)?;
            let v = VirBytes(vaddr_start.0.checked_add(offset)
                .ok_or(PageTableError::InvalidAddress)?);
            if self.query(v).is_some() {
                return Err(PageTableError::AlreadyMapped);
            }
        }

        // Phase 2: Execute — all addresses are free, write mappings
        for i in 0..pages {
            let offset = (i as u64)
                .checked_mul(Self::PAGE_SIZE as u64)
                .ok_or(PageTableError::InvalidAddress)?;
            let v = VirBytes(vaddr_start.0.checked_add(offset)
                .ok_or(PageTableError::InvalidAddress)?);
            let p = PhysBytes(paddr_start.0.checked_add(offset)
                .ok_or(PageTableError::InvalidAddress)?);
            // map() should not fail after validation, but propagate if it does
            // (e.g., AllocationFailed from intermediate page table allocation)
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
            let offset = (i as u64)
                .checked_mul(Self::PAGE_SIZE as u64)
                .ok_or(PageTableError::InvalidAddress)?;
            let v = VirBytes(vaddr_start.0.checked_add(offset)
                .ok_or(PageTableError::InvalidAddress)?);
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

/// Copy page table entries from `src` to `dst` in the given virtual address range.
///
/// This is a cross-page-table operation — it reads existing mappings from one page table
/// and replicates them in another. It is NOT a hardware mechanism; it is a composition
/// of `query()` + `map()` on the `Paging` trait.
///
/// Corresponds to Minix3's:
/// - `pt_map_in_range(src, dst, start, end)` for partial range copies (VM live update)
/// - `pt_copy(dst, src)` for full user-space copies (fork) — call with
///   `start = 0, end = VM_USER_TOP`
///
/// # Errors
///
/// Returns `AlreadyMapped` if `dst` already has a mapping at any address in the range.
/// Skips addresses where `src` has no mapping (matches Minix3's behavior: absent
/// PDEs/PTEs are silently skipped).
///
/// # Design note
///
/// Minix3's `pt_copy` uses `memcpy` to copy entire page table pages (4KB, 1024 PTEs).
/// This optimization is x86-32 specific (PTE = u32, page = 4KB). On x86-64, PTE = u64
/// and the page table structure differs. The `query()`+`map()` approach is architecture-
/// independent and correct for all implementations. If performance becomes critical,
/// arch-specific implementations can provide a batch optimization.
pub fn clone_range<P: Paging>(
    src: &P,
    dst: &mut P,
    start: VirBytes,
    end: VirBytes,
) -> Result<(), PageTableError> {
    let page_size = P::PAGE_SIZE as u64;

    // Validate alignment
    if start.0 % page_size != 0 || end.0 % page_size != 0 {
        return Err(PageTableError::InvalidAddress);
    }

    let mut vaddr = start.0;
    while vaddr < end.0 {
        if let Some((paddr, flags)) = src.query(VirBytes(vaddr)) {
            dst.map(VirBytes(vaddr), paddr, flags)?;
        }
        // If src has no mapping at vaddr, skip (matches Minix3's behavior:
        // pt_map_in_range skips absent PDEs/PTEs)

        vaddr = vaddr.checked_add(page_size).ok_or(PageTableError::InvalidAddress)?;
    }

    Ok(())
}

/// Bind a page table to a process in the kernel.
///
/// Corresponds to Minix3's `pt_bind()` step 5 — notifies the kernel
/// of the process's page table root address via `sys_vmctl_set_addrspace`.
///
/// This is a VM policy operation, not a hardware mechanism. All architectures
/// perform the same kernel IPC call, so this is a plain function, not a trait method.
///
/// Minix3's `pt_bind()` also wrote the page directory physical address into
/// `pagedir_mappings` (steps 1-4). Under Direct Map, the kernel can access any
/// page directory via `kernel_phys_to_virt(cr3_phys)`, so those steps are eliminated.
pub fn bind_to_process(
    root_paddr: PhysBytes,
    endpoint: minix_types::Endpoint,
) -> Result<(), PageTableError> {
    // TODO: call sys_vmctl_set_addrspace(endpoint, root_paddr)
    // Currently a no-op until kernel IPC is implemented.
    let _ = root_paddr;
    let _ = endpoint;
    Ok(())
}

/// Map kernel address space into a page table.
///
/// Corresponds to Minix3's `pt_mapkernel()`. Establishes three mappings:
/// 1. Kernel code segment (executable in real impl)
/// 2. Kernel data segment (non-executable in real impl)
/// 3. Kernel direct map (all physical memory, supervisor-only in real impl)
///
/// This is NOT arch-specific — it composes `Paging::map()` operations.
/// Address layout comes from `DirectMapArch`; physical addresses from boot_info.
/// Therefore this is a generic function, not a trait method.
///
/// After `map_kernel()` completes, the kernel direct map PTEs are never modified
/// (read-only invariant). This makes the Global bit (G=1) safe — TLB entries
/// survive CR3 switches because the content never changes.
pub fn map_kernel<P: Paging>(
    pt: &mut P,
    kernel_text_vbase: u64,
    kernel_text_pbase: u64,
    kernel_text_pages: usize,
    kernel_data_pages: usize,
    dm_vbase: u64,
    dm_pages: usize,
) -> Result<(), PageTableError> {
    let page_size = P::PAGE_SIZE as u64;

    for i in 0..kernel_text_pages {
        let vaddr = VirBytes(kernel_text_vbase + i as u64 * page_size);
        let paddr = PhysBytes(kernel_text_pbase + i as u64 * page_size);
        pt.map(vaddr, paddr, PageFlags::kernel_read_write())?;
    }

    let data_vbase = kernel_text_vbase + kernel_text_pages as u64 * page_size;
    let data_pbase = kernel_text_pbase + kernel_text_pages as u64 * page_size;
    for i in 0..kernel_data_pages {
        let vaddr = VirBytes(data_vbase + i as u64 * page_size);
        let paddr = PhysBytes(data_pbase + i as u64 * page_size);
        pt.map(vaddr, paddr, PageFlags::kernel_read_write())?;
    }

    for i in 0..dm_pages {
        let vaddr = VirBytes(dm_vbase + i as u64 * page_size);
        let paddr = PhysBytes(i as u64 * page_size);
        pt.map(vaddr, paddr, PageFlags::kernel_read_write())?;
    }

    Ok(())
}

/// Initialize the page table subsystem for the VM process.
///
/// Corresponds to Minix3's `pt_init()` but drastically simplified:
/// - **Eliminated**: spare page pool (Direct Map provides VA access),
///   `kern_mappings` / `pagedir_mappings` initialization (Direct Map replaces),
///   dynamic rebuild (no static-to-dynamic transition needed)
/// - **Retained but changed**: CPU feature detection → `HugePages::supports_1gb_page()`;
///   VM page table setup → `map_kernel()` + direct map extension + `bind_to_process()`
///
/// # Phase 1: Huge page capability confirmation
///
/// Determines whether to use 1GB or 2MB huge pages for Direct Map extension.
/// On x86-64, this requires CPUID check; fallback to 2MB is handled by
/// `HugePages` trait implementation.
///
/// # Phase 2: VM page table setup (based on kernel-provided initial page table)
///
/// The kernel creates a minimal initial page table for VM (4 pages, 1GB direct map)
/// before VM starts running. This function extends that page table:
/// 1. `map_kernel()` — kernel code/data + kernel direct map
/// 2. VM direct map extension — if physical memory > 1GB
/// 3. `bind_to_process()` — register page table with the kernel
///
/// # Phase 3: (Future) SMP page table synchronization
///
/// # Type parameters
///
/// - `P`: Paging implementation (must also support HugePages)
/// - `D`: DirectMapArch for address layout constants
///
/// # Arguments
///
/// - `pt`: The VM process's page table (created from kernel-provided initial page table)
/// - `total_phys_bytes`: Total physical memory size in bytes (for direct map extension)
/// - `endpoint`: VM process endpoint (for `bind_to_process()`)
pub fn paging_init<P, D>(
    pt: &mut P,
    total_phys_bytes: u64,
    endpoint: minix_types::Endpoint,
) -> Result<(), PageTableError>
where
    P: crate::paging_ext::HugePages,
    D: crate::direct_map::DirectMapArch,
{
    // Phase 1: Huge page capability confirmation
    let use_1gb = P::supports_1gb_page();
    let huge_page_size = if use_1gb {
        P::HUGE_PAGE_SIZE
    } else {
        P::FALLBACK_HUGE_PAGE_SIZE
    };
    let _ = huge_page_size; // Used in Phase 2 extension

    // Phase 2: VM page table setup

    // Step 2a: Map kernel address space (kernel code/data + kernel direct map)
    // Address layout constants from DirectMapArch trait (architecture-specific)
    // Kernel segment physical addresses from boot_info (not yet passed to this function)
    // For now, use mock layout constants. In real implementation, boot_info provides:
    // - kernel text start physical address
    // - kernel text size in pages
    // - kernel data size in pages
    //
    // TODO: Pass kernel layout from boot_info when available
    const MOCK_KERNEL_TEXT_VBASE: u64 = 0xFFFF_FFFF_8000_0000;
    const MOCK_KERNEL_TEXT_PBASE: u64 = 0x100_0000;
    const MOCK_KERNEL_TEXT_PAGES: usize = 8;
    const MOCK_KERNEL_DATA_PAGES: usize = 8;
    let dm_vbase = D::KERNEL_DIRECT_MAP_BASE;
    let dm_pages = 4; // Only map a few sentinel pages in mock

    map_kernel(
        pt,
        MOCK_KERNEL_TEXT_VBASE,
        MOCK_KERNEL_TEXT_PBASE,
        MOCK_KERNEL_TEXT_PAGES,
        MOCK_KERNEL_DATA_PAGES,
        dm_vbase,
        dm_pages,
    )?;

    // Step 2b: Extend VM direct map if physical memory exceeds 1GB
    // The kernel-provided initial page table has 1GB direct map starting at
    // VM_DIRECT_MAP_BASE. If total physical memory exceeds 1GB, we need to
    // map additional 1GB (or 2MB fallback) entries.
    //
    // TODO: Implement direct map extension when phys_mem > 1GB.
    //       Algorithm:
    //       for each additional 1GB segment beyond the initial one:
    //         let paddr = PhysBytes(segment_index * 0x4000_0000);
    //         let vaddr = D::vm_phys_to_virt(paddr);
    //         pt.map_huge(vaddr, paddr, huge_page_size as usize,
    //                     PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER_ACCESSIBLE)?;
    let initial_dm_size: u64 = 1 << 30; // 1GB
    if total_phys_bytes > initial_dm_size {
        // TODO: Extend direct map beyond initial 1GB
        // Requires HugePages::map_huge() support in the implementation
    }

    // Step 2c: Bind page table to VM process
    bind_to_process(pt.root_paddr(), endpoint)?;

    Ok(())
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

    impl crate::paging_ext::HugePages for MockPaging {
        const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21]; // 1GB, 2MB
        const HUGE_PAGE_SIZE: u64 = 1 << 30;       // 1GB preferred
        const HUGE_PAGE_SHIFT: u32 = 30;           // 1GB shift
        const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21; // 2MB fallback
        const PTE_HUGE_FLAGS: u64 = 0;             // Mock: no hardware PTE flags

        fn map_huge(
            &mut self,
            vaddr: VirBytes,
            paddr: PhysBytes,
            size: usize,
            flags: PageFlags,
        ) -> Result<(), PageTableError> {
            // Mock: just map as regular pages within the huge page range
            let page_size = Self::PAGE_SIZE as u64;
            let pages = size / Self::PAGE_SIZE;
            self.map_range(vaddr, paddr, pages, flags)
        }

        fn supports_1gb_page() -> bool {
            true // Mock always supports 1GB pages
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
        use crate::paging_ext::{PagingWithId, HugePages};

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

        #[test]
        fn test_mock_remap_new() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr2 = PhysBytes(0x3000);
            let flags = PageFlags::read_write();

            let result = pt.remap(vaddr, paddr2, flags).unwrap();
            assert!(result.is_none());
            assert_eq!(pt.query(vaddr), Some((paddr2, flags)));
        }

        #[test]
        fn test_mock_remap_replace() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr1 = PhysBytes(0x2000);
            let paddr2 = PhysBytes(0x3000);
            let flags1 = PageFlags::read_write();
            let flags2 = PageFlags::read_only();

            pt.map(vaddr, paddr1, flags1).unwrap();
            let result = pt.remap(vaddr, paddr2, flags2).unwrap();
            assert_eq!(result, Some((paddr1, flags1)));
            assert_eq!(pt.query(vaddr), Some((paddr2, flags2)));
        }

        #[test]
        fn test_mock_map_range() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);
            let flags = PageFlags::read_write();
            let pages = 4;

            pt.map_range(vaddr, paddr, pages, flags).unwrap();

            for i in 0..pages {
                let offset = (i as u64) * (MockPaging::PAGE_SIZE as u64);
                let v = VirBytes(vaddr.0 + offset);
                let p = PhysBytes(paddr.0 + offset);
                assert_eq!(pt.query(v), Some((p, flags)));
            }
        }

        #[test]
        fn test_mock_unmap_range() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);
            let flags = PageFlags::read_write();
            let pages = 3;

            pt.map_range(vaddr, paddr, pages, flags).unwrap();
            pt.unmap_range(vaddr, pages).unwrap();

            for i in 0..pages {
                let offset = (i as u64) * (MockPaging::PAGE_SIZE as u64);
                let v = VirBytes(vaddr.0 + offset);
                assert!(pt.query(v).is_none());
            }
        }

        #[test]
        fn test_mock_map_range_all_or_nothing() {
            // Verify all-or-nothing semantics: if any address in the range is
            // already mapped, map_range returns AlreadyMapped and leaves no
            // partial mappings behind.
            let mut pt = MockPaging::new().unwrap();
            let flags = PageFlags::read_write();

            // Pre-map page 2 of a 4-page range
            let conflict_vaddr = VirBytes(0x3000);
            pt.map(conflict_vaddr, PhysBytes(0xF000), flags).unwrap();

            // Attempt map_range covering pages 0-3 (0x1000-0x4000)
            let result = pt.map_range(
                VirBytes(0x1000), PhysBytes(0x2000), 4, flags,
            );
            assert_eq!(result, Err(PageTableError::AlreadyMapped));

            // Verify no partial mappings were created (pages 0,1,3 should be unmapped)
            assert!(pt.query(VirBytes(0x1000)).is_none());
            assert!(pt.query(VirBytes(0x2000)).is_none());
            assert!(pt.query(VirBytes(0x4000)).is_none());

            // The pre-existing mapping at page 2 should be untouched
            assert_eq!(
                pt.query(conflict_vaddr),
                Some((PhysBytes(0xF000), flags))
            );
        }

        #[test]
        fn test_page_flags_display() {
            assert_eq!(format!("{}", PageFlags::read_write()), "P|W|U");
            assert_eq!(format!("{}", PageFlags::read_only()), "P|U");
            assert_eq!(format!("{}", PageFlags::kernel_read_write()), "P|W|G");
            assert_eq!(format!("{}", PageFlags::empty()), "0");
        }

        #[test]
        fn test_mock_map_kernel() {
            let mut pt = MockPaging::new().unwrap();

            const MOCK_KERNEL_TEXT_VBASE: u64 = 0xFFFF_FFFF_8000_0000;
            const MOCK_KERNEL_TEXT_PBASE: u64 = 0x100_0000;
            const MOCK_KERNEL_TEXT_PAGES: usize = 8;
            const MOCK_KERNEL_DATA_PAGES: usize = 8;
            const MOCK_DM_VBASE: u64 = 0xFFFF_8000_0000_0000;
            const MOCK_DM_SENTINEL_PAGES: usize = 4;

            super::super::map_kernel(
                &mut pt,
                MOCK_KERNEL_TEXT_VBASE,
                MOCK_KERNEL_TEXT_PBASE,
                MOCK_KERNEL_TEXT_PAGES,
                MOCK_KERNEL_DATA_PAGES,
                MOCK_DM_VBASE,
                MOCK_DM_SENTINEL_PAGES,
            ).unwrap();

            // Segment 1: Kernel code segment (8 pages at MOCK_KERNEL_TEXT_VBASE)
            const PAGE_SIZE: u64 = MockPaging::PAGE_SIZE as u64;

            for i in 0..8 {
                let vaddr = VirBytes(MOCK_KERNEL_TEXT_VBASE + i * PAGE_SIZE);
                let result = pt.query(vaddr);
                assert!(result.is_some(), "kernel code page {i} not mapped");
                let (paddr, flags) = result.unwrap();
                assert_eq!(paddr, PhysBytes(MOCK_KERNEL_TEXT_PBASE + i * PAGE_SIZE));
                assert_eq!(flags, PageFlags::kernel_read_write());
            }

            // Segment 1: Kernel data segment (8 pages after code segment)
            const MOCK_KERNEL_DATA_VBASE: u64 = MOCK_KERNEL_TEXT_VBASE + 8 * PAGE_SIZE;
            const MOCK_KERNEL_DATA_PBASE: u64 = MOCK_KERNEL_TEXT_PBASE + 8 * PAGE_SIZE;

            for i in 0..8 {
                let vaddr = VirBytes(MOCK_KERNEL_DATA_VBASE + i * PAGE_SIZE);
                let result = pt.query(vaddr);
                assert!(result.is_some(), "kernel data page {i} not mapped");
                let (paddr, flags) = result.unwrap();
                assert_eq!(paddr, PhysBytes(MOCK_KERNEL_DATA_PBASE + i * PAGE_SIZE));
                assert_eq!(flags, PageFlags::kernel_read_write());
            }

            // Segment 2: Kernel direct map sentinel (4 pages at KERNEL_DIRECT_MAP_BASE)
            const MOCK_DM_PBASE: u64 = 0;

            for i in 0..4 {
                let vaddr = VirBytes(MOCK_DM_VBASE + i * PAGE_SIZE);
                let result = pt.query(vaddr);
                assert!(result.is_some(), "kernel DM sentinel page {i} not mapped");
                let (paddr, flags) = result.unwrap();
                assert_eq!(paddr, PhysBytes(MOCK_DM_PBASE + i * PAGE_SIZE));
                assert_eq!(flags, PageFlags::kernel_read_write());
            }

            // Verify gap between code segment and data segment has no unmapped overlap
            // (In this mock, code and data are contiguous, so no gap to test)
            // Verify after last data page is unmapped
            let unmapped = VirBytes(MOCK_KERNEL_DATA_VBASE + 8 * PAGE_SIZE);
            assert!(pt.query(unmapped).is_none());
        }

        #[test]
        fn test_mock_alloc_asid_monotonic() {
            let pt = MockPaging::new().unwrap();
            let id1 = pt.alloc_asid().unwrap();
            let id2 = pt.alloc_asid().unwrap();
            let id3 = pt.alloc_asid().unwrap();

            assert_ne!(id1, id2);
            assert_ne!(id2, id3);
            assert_ne!(id1, id3);

            pt.free_asid(id2);
            let id4 = pt.alloc_asid().unwrap();
            assert_ne!(id4, id1);
            assert_ne!(id4, id3);
        }

        #[test]
        fn test_mock_switch_with_asid() {
            let pt = MockPaging::new().unwrap();
            let asid = pt.alloc_asid().unwrap();

            unsafe { pt.switch_with_asid(asid); }
        }

        #[test]
        fn test_mock_huge_pages_capability() {
            // Mock always reports 1GB support
            assert!(MockPaging::supports_1gb_page());
            assert!(MockPaging::supports_huge_page(1 << 30));
            assert!(MockPaging::supports_huge_page(1 << 21));
            assert!(!MockPaging::supports_huge_page(1 << 12)); // 4KB is not huge
        }

        #[test]
        fn test_mock_map_huge() {
            let mut pt = MockPaging::new().unwrap();
            let vaddr = VirBytes(0x200_0000); // 32MB
            let paddr = PhysBytes(0x400_0000);
            let size = 1 << 21; // 2MB huge page
            let flags = PageFlags::read_write();

            pt.map_huge(vaddr, paddr, size, flags).unwrap();

            // Mock maps as regular pages; verify first page is accessible
            let result = pt.query(vaddr);
            assert!(result.is_some());
            let (p, f) = result.unwrap();
            assert_eq!(p, paddr);
            assert_eq!(f, flags);
        }

        #[test]
        fn test_paging_init_basic() {
            use crate::direct_map::MockDirectMap;

            let mut pt = MockPaging::new().unwrap();
            let endpoint = minix_types::Endpoint(1);

            // paging_init with small physical memory (< 1GB, no extension needed)
            let result = super::super::super::paging::paging_init::<
                MockPaging, MockDirectMap,
            >(&mut pt, 512 * 1024 * 1024, endpoint); // 512MB

            assert!(result.is_ok());

            // Verify kernel code/data mappings were established by map_kernel() segment 1
            const MOCK_KERNEL_TEXT_VBASE: u64 = 0xFFFF_FFFF_8000_0000;
            let first_kernel_page = VirBytes(MOCK_KERNEL_TEXT_VBASE);
            assert!(pt.query(first_kernel_page).is_some());

            // Verify kernel direct map sentinel was established by map_kernel() segment 2
            const MOCK_DM_VBASE: u64 = crate::direct_map::MockDirectMap::KERNEL_DIRECT_MAP_BASE;
            // DirectMapArch trait must be in scope for associated const access
            use crate::direct_map::DirectMapArch as _;
            let first_dm_page = VirBytes(MOCK_DM_VBASE);
            assert!(pt.query(first_dm_page).is_some());
        }
    }
}
