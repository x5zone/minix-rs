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

use crate::arch::boot_arch::BootArch;
use crate::paging::{PageFlags, PageTableError, Paging};
use minix_types::{PhysBytes, VirBytes};

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

/// One page-size capability answer: the size plus whether this platform can
/// use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSupport {
    size: PageSize,
    supported: bool,
}

impl PageSupport {
    pub const fn new(size: PageSize, supported: bool) -> Self {
        Self { size, supported }
    }

    pub const fn size(&self) -> PageSize {
        self.size
    }

    pub const fn supported(&self) -> bool {
        self.supported
    }
}

/// System-level page-size facts for one boot.
///
/// This snapshot answers "what does this platform support", never "what this
/// boot chose". Planning consumes it and produces policies; the snapshot
/// itself carries no alignment or mapping decision. It is not a permanent
/// capability cache — runtime callers still query
/// [`HugePages::supports_huge_page`] directly.
pub struct BootPageSupport<A: BootArch> {
    base_page_size: PageSize,
    phys_addr_limit: u64,
    preferred: PageSupport,
    fallback: PageSupport,
    _arch: core::marker::PhantomData<fn() -> A>,
}

impl<A: BootArch> Copy for BootPageSupport<A> {}

impl<A: BootArch> Clone for BootPageSupport<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: BootArch> core::fmt::Debug for BootPageSupport<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BootPageSupport")
            .field("base_page_size", &self.base_page_size)
            .field("phys_addr_limit", &self.phys_addr_limit)
            .field("preferred", &self.preferred)
            .field("fallback", &self.fallback)
            .finish()
    }
}

impl<A: BootArch> BootPageSupport<A> {
    /// Test-only constructor for controlled capability injection.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn from_parts(
        base_page_size: PageSize,
        phys_addr_limit: u64,
        preferred: PageSupport,
        fallback: PageSupport,
    ) -> Self {
        Self {
            base_page_size,
            phys_addr_limit,
            preferred,
            fallback,
            _arch: core::marker::PhantomData,
        }
    }

    pub const fn base_page_size(&self) -> PageSize {
        self.base_page_size
    }

    pub const fn phys_addr_limit(&self) -> u64 {
        self.phys_addr_limit
    }

    pub const fn preferred(&self) -> PageSupport {
        self.preferred
    }

    pub const fn fallback(&self) -> PageSupport {
        self.fallback
    }

    /// Check the static page-size configuration behind this snapshot.
    ///
    /// Planning calls this again even for injected snapshots, so a test
    /// cannot smuggle an impossible ordering (fallback below the base page,
    /// preferred below the fallback, a size outside the architecture's
    /// declared set) into the policy layer.
    pub fn validate(&self) -> Result<(), PageSupportError> {
        let base = PageSize::try_from_usize(A::Paging::PAGE_SIZE)
            .ok_or(PageSupportError::InvalidBasePageSize)?;
        if self.base_page_size != base {
            return Err(PageSupportError::InvalidBasePageSize);
        }
        if self.phys_addr_limit == 0 {
            return Err(PageSupportError::InvalidPhysAddrLimit);
        }
        if self.fallback.size() != A::Paging::FALLBACK_HUGE_PAGE_SIZE
            || !A::Paging::HUGE_PAGE_SIZES.contains(&self.fallback.size())
            || self.fallback.size() <= base
        {
            return Err(PageSupportError::InvalidFallback);
        }
        if self.preferred.size() != A::Paging::HUGE_PAGE_SIZE
            || !A::Paging::HUGE_PAGE_SIZES.contains(&self.preferred.size())
            || self.preferred.size() <= self.fallback.size()
        {
            return Err(PageSupportError::InvalidPreferred);
        }
        Ok(())
    }
}

/// Capability-fact errors. They describe whether the observed boot facts are
/// self-consistent; they carry no mapping-planning semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSupportError {
    /// `Paging::PAGE_SIZE` is not a legal `PageSize`, or disagrees with the
    /// snapshot.
    InvalidBasePageSize,
    /// No trustworthy platform physical-address bound, or the bound is zero.
    InvalidPhysAddrLimit,
    /// The fallback size is not the architecture constant, is outside the
    /// declared size set, or is not larger than the base page.
    InvalidFallback,
    /// The preferred size is not the architecture constant, is outside the
    /// declared size set, or is not larger than the fallback.
    InvalidPreferred,
}

impl core::fmt::Display for PageSupportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidBasePageSize => write!(f, "invalid base page size"),
            Self::InvalidPhysAddrLimit => write!(f, "invalid physical address limit"),
            Self::InvalidFallback => write!(f, "invalid fallback huge page size"),
            Self::InvalidPreferred => write!(f, "invalid preferred huge page size"),
        }
    }
}

/// Collect this boot's capability facts.
///
/// Reads the target translation configuration's static sizes, the
/// platform-level physical-address bound, and the hardware capability answers.
/// It does not read the kernel image layout, does not check alignment, and
/// does not choose a page size.
pub fn detect_boot_page_support<A: BootArch>() -> Result<BootPageSupport<A>, PageSupportError> {
    let base_page_size = PageSize::try_from_usize(A::Paging::PAGE_SIZE)
        .ok_or(PageSupportError::InvalidBasePageSize)?;
    let phys_addr_limit = A::Paging::phys_addr_limit()
        .map_err(|_| PageSupportError::InvalidPhysAddrLimit)?;
    let support = BootPageSupport {
        base_page_size,
        phys_addr_limit,
        preferred: PageSupport::new(
            A::Paging::HUGE_PAGE_SIZE,
            A::Paging::supports_huge_page(A::Paging::HUGE_PAGE_SIZE),
        ),
        fallback: PageSupport::new(
            A::Paging::FALLBACK_HUGE_PAGE_SIZE,
            A::Paging::supports_huge_page(A::Paging::FALLBACK_HUGE_PAGE_SIZE),
        ),
        _arch: core::marker::PhantomData,
    };
    support.validate()?;
    Ok(support)
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

    #[cfg(feature = "runtime-window")]
    #[test]
    fn detect_reports_the_mock_platform_facts() {
        use super::{detect_boot_page_support, BootPageSupport};
        use crate::arch::boot_arch::MockBootArch;

        let support: BootPageSupport<MockBootArch> =
            detect_boot_page_support::<MockBootArch>().unwrap();
        assert_eq!(support.base_page_size(), PageSize::KIB_4);
        assert_eq!(support.phys_addr_limit(), 1u64 << 52);
        assert_eq!(support.preferred().size(), PageSize::GIB_1);
        assert!(support.preferred().supported());
        assert_eq!(support.fallback().size(), PageSize::MIB_2);
        assert!(support.fallback().supported());
        assert_eq!(support.validate(), Ok(()));
    }

    #[cfg(feature = "runtime-window")]
    #[test]
    fn validate_rejects_impossible_static_configurations() {
        use super::{BootPageSupport, PageSupport, PageSupportError};
        use crate::arch::boot_arch::MockBootArch;

        let base = PageSize::KIB_4;
        let fallback = PageSupport::new(PageSize::MIB_2, true);
        let preferred = PageSupport::new(PageSize::GIB_1, true);

        let zero_limit =
            BootPageSupport::<MockBootArch>::from_parts(base, 0, preferred, fallback);
        assert_eq!(
            zero_limit.validate(),
            Err(PageSupportError::InvalidPhysAddrLimit)
        );

        let wrong_base = BootPageSupport::<MockBootArch>::from_parts(
            PageSize::MIB_2,
            1 << 52,
            preferred,
            fallback,
        );
        assert_eq!(
            wrong_base.validate(),
            Err(PageSupportError::InvalidBasePageSize)
        );

        let swapped = BootPageSupport::<MockBootArch>::from_parts(
            base,
            1 << 52,
            PageSupport::new(PageSize::MIB_2, true),
            PageSupport::new(PageSize::MIB_2, true),
        );
        assert_eq!(swapped.validate(), Err(PageSupportError::InvalidPreferred));

        let tiny_fallback = BootPageSupport::<MockBootArch>::from_parts(
            base,
            1 << 52,
            preferred,
            PageSupport::new(PageSize::KIB_4, true),
        );
        assert_eq!(
            tiny_fallback.validate(),
            Err(PageSupportError::InvalidFallback)
        );
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
