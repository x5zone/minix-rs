//! Paging extension traits
//!
//! Optional page table feature extensions, not supported by all architectures:
//! - `PagingWithId`: TLB process identification (PCID/ASID)
//! - `HugePages`: huge page support
//!
//! # Future extensions
//!
//! The following features will be added to this module when actual demand arises:
//!
//! - **`clear_dirty()`**: Page replacement algorithms (Clock/LRU) need to periodically
//!   clear the dirty bit to detect pages that have been re-written. Currently this can
//!   be done via `update_flags()`, but a dedicated method could use atomic RMW
//!   operations on x86-64 (e.g., `LOCK CMPXCHG16B`), avoiding loss of hardware
//!   updates during the read-modify-write cycle.

use minix_types::{PhysBytes, VirBytes};
use crate::paging::{PageFlags, PageTableError, Paging};

/// TLB process identification support (optional trait)
///
/// Tags TLB entries with a process identifier, avoiding a full TLB flush
/// on context switch and significantly reducing TLB miss overhead.
///
/// **Note**: This trait does not define ASID/PCID lifecycle semantics or
/// reuse strategies. ASID allocation/recycling policy, TLB shootdown
/// consistency maintenance, and generation counter mechanisms are the
/// responsibility of the upper VM manager. This trait only provides
/// low-level hardware operation primitives.
pub trait PagingWithId: Paging {
    type AddressSpaceId: Copy + Eq + core::fmt::Debug + Send;

    /// Allocate an Address Space ID (ASID/PCID).
    ///
    /// The `&self` receiver allows the implementation to decide the allocation
    /// strategy: it may use a global pool, a per-CPU pool, or a per-page-table
    /// pool. Minix3 does not implement PCID/ASID management, so there is no
    /// direct C counterpart. The allocation strategy is an implementation detail
    /// left to each architecture.
    fn alloc_asid(&self) -> Result<Self::AddressSpaceId, PageTableError>;

    /// Free a previously allocated ASID/PCID.
    ///
    /// The `&self` receiver mirrors `alloc_asid` so that the same allocation
    /// context is used for both operations.
    fn free_asid(&self, id: Self::AddressSpaceId);

    /// Activate this page table with the given ASID on the current CPU.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The page table is fully initialized (kernel mappings present)
    /// - The ASID was allocated via `alloc_asid()` and has not been freed
    /// - On SMP systems, proper TLB shootdown is performed if needed
    unsafe fn switch_with_asid(&self, id: Self::AddressSpaceId);

    /// Flush TLB entries for the given ASID.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - This is called in a valid MMU context (a page table is active)
    /// - The ASID is valid and currently in use
    unsafe fn flush_tlb_asid(&self, id: Self::AddressSpaceId);

    /// Flush the TLB entry for a single virtual address within the given ASID.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - `vaddr` falls within the currently active page table's valid range
    /// - The ASID is valid and currently in use
    unsafe fn flush_tlb_addr_asid(&self, vaddr: VirBytes, id: Self::AddressSpaceId);
}

/// Huge page support (optional trait)
///
/// Abstracts the MMU's huge page capabilities. Each architecture provides
/// the sizes it supports, fallback sizes for when the preferred size is
/// unavailable, and the PTE flags for huge page entries.
///
/// This trait is separate from `DirectMapArch` because huge page support
/// is an MMU hardware parameter, not an address space layout decision.
pub trait HugePages: Paging {
    /// Supported huge page sizes (in bytes), sorted largest first.
    const HUGE_PAGE_SIZES: &'static [usize];

    /// Preferred huge page size for Direct Map (bytes).
    const HUGE_PAGE_SIZE: u64;

    /// Page table level shift for the preferred huge page.
    const HUGE_PAGE_SHIFT: u32;

    /// Fallback huge page size when the preferred size is not available.
    const FALLBACK_HUGE_PAGE_SIZE: u64;

    /// Hardware-specific bit that identifies a PTE as a huge-page entry
    /// (x86_64: PS bit `1 << 7`; ARM64/RISC-V: 0 — no extra bit needed).
    const PTE_HUGE_IDENTIFIER_BIT: u64;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    fn supports_huge_page(size: usize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }

    /// Whether 1GB huge pages are supported by the current CPU.
    ///
    /// x86-64 requires a CPUID check (`CPUID.80000001H:EDX.GBPAGES`);
    /// ARM64 and RISC-V always support it.
    fn supports_1gb_page() -> bool {
        true
    }
}
