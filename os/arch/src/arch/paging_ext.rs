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

/// A page size expressed in bytes.
///
/// A distinct type instead of a bare `usize`: huge-page sizes travel through
/// capability queries, planning, and `map_huge`, and a bare integer let a
/// byte count be confused with a page count or a shift. Construction only
/// accepts a power of two no smaller than 4 KiB, and every conversion back to
/// `usize` is fallible; there is no silently truncating `as_usize`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageSize(u64);

impl PageSize {
    pub const KIB_4: Self = Self(4096);
    pub const MIB_2: Self = Self(1 << 21);
    pub const GIB_1: Self = Self(1 << 30);

    /// The size in bytes.
    pub const fn bytes(self) -> u64 {
        self.0
    }

    /// Build a page size from a byte count. Only powers of two no smaller
    /// than 4 KiB are page sizes.
    pub const fn try_from_bytes(bytes: u64) -> Option<Self> {
        if bytes >= 4096 && bytes.is_power_of_two() {
            Some(Self(bytes))
        } else {
            None
        }
    }

    /// Build a page size from a `usize` byte count.
    pub const fn try_from_usize(bytes: usize) -> Option<Self> {
        // `usize` never exceeds `u64` on any supported target.
        Self::try_from_bytes(bytes as u64)
    }

    /// Convert back to `usize`, failing on targets where the value does not fit.
    pub const fn try_as_usize(self) -> Option<usize> {
        if self.0 <= usize::MAX as u64 {
            Some(self.0 as usize)
        } else {
            None
        }
    }
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
    /// Supported huge page sizes, sorted largest first.
    const HUGE_PAGE_SIZES: &'static [PageSize];

    /// Preferred huge page size for Direct Map.
    const HUGE_PAGE_SIZE: PageSize;

    /// Page table level shift for the preferred huge page.
    const HUGE_PAGE_SHIFT: u32;

    /// Fallback huge page size when the preferred size is not available.
    const FALLBACK_HUGE_PAGE_SIZE: PageSize;

    /// Hardware-specific bit that identifies a PTE as a huge-page entry
    /// (x86_64: PS bit `1 << 7`; ARM64/RISC-V: 0 — no extra bit needed).
    const PTE_HUGE_IDENTIFIER_BIT: u64;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: PageSize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    /// Whether this architecture/CPU supports the given huge page size.
    ///
    /// No default: the capability is hardware- and translation-mode-specific,
    /// so every implementation answers for itself (x86-64 additionally probes
    /// CPUID.80000001H:EDX.GBPAGES).
    fn supports_huge_page(size: PageSize) -> bool;
}

#[cfg(test)]
mod page_size_tests {
    use super::PageSize;

    #[test]
    fn named_sizes_have_their_byte_values() {
        assert_eq!(PageSize::KIB_4.bytes(), 4096);
        assert_eq!(PageSize::MIB_2.bytes(), 1 << 21);
        assert_eq!(PageSize::GIB_1.bytes(), 1 << 30);
    }

    #[test]
    fn construction_rejects_non_powers_of_two_and_sub_page_sizes() {
        assert_eq!(PageSize::try_from_bytes(0), None);
        assert_eq!(PageSize::try_from_bytes(3000), None);
        assert_eq!(PageSize::try_from_bytes(4095), None);
        assert_eq!(PageSize::try_from_bytes(6144), None);
        assert_eq!(PageSize::try_from_bytes(4096), Some(PageSize::KIB_4));
        assert_eq!(PageSize::try_from_bytes(8192), Some(PageSize(8192)));
    }

    #[test]
    fn usize_round_trip_is_checked_not_truncating() {
        for size in [PageSize::KIB_4, PageSize::MIB_2, PageSize::GIB_1] {
            assert_eq!(
                PageSize::try_from_usize(size.bytes() as usize),
                Some(size)
            );
            assert_eq!(size.try_as_usize(), Some(size.bytes() as usize));
        }
        assert_eq!(PageSize::try_from_usize(0), None);
    }
}
