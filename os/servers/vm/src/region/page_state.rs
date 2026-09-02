//! Physical page state management (PFN index model).
//!
//! Global PageState array indexed by PFN, replacing PhysBlock + PhysRegion.
//! Corresponds to Minix3's `phys_block` + `pb.c`.
//!
//! PageFrames only tracks physical page refcounts and cache flags; it does not
//! handle physical page allocation/deallocation. Page allocation is done by the
//! buddy/bitmap allocator (see phys_mem module).

use alloc::vec;
use alloc::vec::Vec;
use minix_types::{PhysBytes, VirBytes};
use crate::memtype::MemType;

pub(crate) const PAGE_SIZE: u64 = 4096;

pub(crate) trait PfnAllocator {
    fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError>;
    fn free_pfn(&mut self, pfn: u32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PfnAllocError {
    OutOfMemory,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct PageFlags: u8 {
        const IN_CACHE   = 0x01;
        const PENDING_IO = 0x02;
        /// Page is CoW (Copy-on-Write): mapped read-only, write triggers page fault.
        /// Set by `prepare_cow()` when refcount > 1 after fork.
        /// Equivalent to Minix3's `~PT_W` flag applied during `map_copy_region()`.
        const COW        = 0x04;
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub(crate) struct PageState {
    /// Reference count for this physical page.
    /// u32 matches Minix3's `int` refcount, avoiding overflow on 64-bit systems
    /// where many processes may share the same page (e.g., via CoW fork).
    pub(crate) refcount: u32,
    pub(crate) flags: PageFlags,
    _padding: u8,
}

impl PageState {
    pub const fn new() -> Self {
        Self {
            refcount: 0,
            flags: PageFlags::empty(),
            _padding: 0,
        }
    }

    pub fn refcount(&self) -> u32 {
        self.refcount
    }

    // V10-P2-1: no callers yet (`is_cached` reads the field directly).
    #[allow(dead_code)]
    pub fn flags(&self) -> PageFlags {
        self.flags
    }

    pub fn is_cached(&self) -> bool {
        self.flags.contains(PageFlags::IN_CACHE)
    }
}

/// Per-page mapping slot state machine.
///
/// Replaces Minix3's `struct phys_region **physblocks` pointer array
/// (NULL = unmapped) with an explicit three-state representation:
///
/// - `Empty`: no mapping and no reservation — the default state of every slot
///   in a freshly created region.
/// - `Reserved`: lazy placeholder — the slot is reserved for a mapping that
///   will be materialized on demand (anonymous demand paging / CoW
///   preallocation). Carries `offset` + `memtype` so the placeholder is
///   self-describing, but has no backing frame yet (`pfn()` is `None`).
/// - `Mapped`: materialized mapping backed by a physical frame (`pfn`).
///
/// The three states are explicit at the type level. The previous design
/// encoded both `Empty` and `Reserved` as `pfn == PFN_NONE`, which made a
/// lazy placeholder indistinguishable from an unmapped slot and caused
/// `get_slot()` to hide reserved slots (todo P0-1, 13-region-mapping §3.6).
#[derive(Clone, Copy)]
pub(crate) enum PageSlot {
    Empty,
    Reserved {
        offset: VirBytes,
        memtype: Option<&'static dyn MemType>,
    },
    Mapped {
        pfn: u32,
        offset: VirBytes,
        memtype: Option<&'static dyn MemType>,
    },
}

impl PartialEq for PageSlot {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Reserved { offset: a, .. }, Self::Reserved { offset: b, .. }) => a == b,
            (
                Self::Mapped {
                    pfn: a_pfn,
                    offset: a_off,
                    ..
                },
                Self::Mapped {
                    pfn: b_pfn,
                    offset: b_off,
                    ..
                },
            ) => a_pfn == b_pfn && a_off == b_off,
            _ => false,
        }
    }
}

impl Eq for PageSlot {}

impl PageSlot {
    /// Construct a materialized slot backed by physical frame `pfn`.
    pub fn mapped(pfn: u32, offset: VirBytes, memtype: Option<&'static dyn MemType>) -> Self {
        Self::Mapped {
            pfn,
            offset,
            memtype,
        }
    }

    /// Construct a lazy placeholder slot (no backing frame yet).
    ///
    /// Consumed by `VirRegion::map_lazy()`: reserves the slot for a mapping
    /// that will be materialized on demand, carrying the region's default
    /// memtype so materialization knows which policy applies.
    /// `#[allow(dead_code)]` (ARCH A-13): the lazy family has no production
    /// caller yet — it is exercised by tests and reserved for the demand
    /// paging / CoW preallocation path (todo P0-1).
    #[allow(dead_code)]
    pub fn reserved(offset: VirBytes, memtype: Option<&'static dyn MemType>) -> Self {
        Self::Reserved { offset, memtype }
    }

    /// The backing frame of a materialized slot, if any.
    ///
    /// `None` for `Empty` and `Reserved` — the state machine guarantees a
    /// frame exists only for `Mapped`.
    pub fn pfn(&self) -> Option<u32> {
        match self {
            Self::Mapped { pfn, .. } => Some(*pfn),
            Self::Empty | Self::Reserved { .. } => None,
        }
    }

    /// Page offset within the region (0 for `Empty`).
    pub fn offset(&self) -> VirBytes {
        match self {
            Self::Empty => VirBytes(0),
            Self::Reserved { offset, .. } | Self::Mapped { offset, .. } => *offset,
        }
    }

    /// The mapping's memtype, if any (`Empty` has none).
    pub fn memtype(&self) -> Option<&'static dyn MemType> {
        match self {
            Self::Empty => None,
            Self::Reserved { memtype, .. } | Self::Mapped { memtype, .. } => *memtype,
        }
    }

    pub fn is_mapped(&self) -> bool {
        matches!(self, Self::Mapped { .. })
    }

    #[allow(dead_code)] // ARCH A-13: lazy family, see `reserved()`.
    pub fn is_reserved(&self) -> bool {
        matches!(self, Self::Reserved { .. })
    }

    #[allow(dead_code)] // ARCH A-13: lazy family, used by `get_slot_any()`.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Replace the memtype of a present slot (Mapped or Reserved); no-op on
    /// `Empty`.
    pub fn set_memtype(&mut self, memtype: Option<&'static dyn MemType>) {
        match self {
            Self::Mapped { memtype: mt, .. } | Self::Reserved { memtype: mt, .. } => *mt = memtype,
            Self::Empty => {}
        }
    }
}

impl core::fmt::Debug for PageSlot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("PageSlot::Empty"),
            Self::Reserved { offset, memtype } => f
                .debug_struct("PageSlot::Reserved")
                .field("offset", offset)
                .field("memtype", &memtype.map(|m| m.name()))
                .finish(),
            Self::Mapped {
                pfn,
                offset,
                memtype,
            } => f
                .debug_struct("PageSlot::Mapped")
                .field("pfn", pfn)
                .field("offset", offset)
                .field("memtype", &memtype.map(|m| m.name()))
                .finish(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PageFrames {
    states: Vec<PageState>,
    // V10-P2-1: read only by the test-only `total_pages()` accessor.
    #[cfg_attr(not(test), allow(dead_code))]
    total_pages: u32,
}

impl PageFrames {
    pub fn new(total_phys: PhysBytes) -> Self {
        // total_pages bounded by u32: max 4TB physical memory with 4KB pages
        let total_pages = (total_phys.0 / PAGE_SIZE) as u32;
        let states = vec![PageState::new(); total_pages as usize];
        Self { states, total_pages }
    }

    pub fn get(&self, pfn: u32) -> Option<&PageState> {
        self.states.get(pfn as usize)
    }

    pub fn get_mut(&mut self, pfn: u32) -> Option<&mut PageState> {
        self.states.get_mut(pfn as usize)
    }

    // V10-P2-1: test-only accessor (production reads the allocator's
    // `total_count()`); kept for layout-invariant assertions.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn total_pages(&self) -> u32 {
        self.total_pages
    }

    pub fn pfn_to_phys(&self, pfn: u32) -> PhysBytes {
        PhysBytes((pfn as u64) * PAGE_SIZE)
    }

    pub fn phys_to_pfn(&self, phys: PhysBytes) -> u32 {
        (phys.0 / PAGE_SIZE) as u32
    }

    pub fn addcache(&mut self, pfn: u32) {
        if let Some(state) = self.states.get_mut(pfn as usize)
            && !state.flags.contains(PageFlags::IN_CACHE) {
                state.flags.insert(PageFlags::IN_CACHE);
                state.refcount = state.refcount.saturating_add(1);
            }
    }

    pub fn rmcache(&mut self, pfn: u32) {
        if let Some(state) = self.states.get_mut(pfn as usize)
            && state.flags.contains(PageFlags::IN_CACHE) {
                state.flags.remove(PageFlags::IN_CACHE);
                if state.refcount > 0 {
                    state.refcount -= 1;
                }
            }
    }

    // verify_refcounts is implemented in sanity.rs module.
    // It traverses all processes' VirRegions via VmProcTable::for_each_active_region(),
    // counts per-PFN references, and compares with PageFrames.refcount.
    // Equivalent to Minix3's map_sanitycheck() (region.c:168-261).
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_frames_init() {
        let frames = PageFrames::new(PhysBytes(0x1_0000_0000));
        assert_eq!(frames.total_pages(), (0x1_0000_0000u64 / PAGE_SIZE) as u32);
        for i in 0..frames.total_pages() {
            let state = frames.get(i).unwrap();
            assert_eq!(state.refcount, 0);
            assert!(!state.flags.contains(PageFlags::IN_CACHE));
        }
    }

    #[test]
    fn test_pfn_to_phys() {
        let frames = PageFrames::new(PhysBytes(0x1_0000_0000));
        assert_eq!(frames.pfn_to_phys(0), PhysBytes(0));
        assert_eq!(frames.pfn_to_phys(1), PhysBytes(PAGE_SIZE));
        assert_eq!(frames.pfn_to_phys(256), PhysBytes(256 * PAGE_SIZE));
    }

    #[test]
    fn test_phys_to_pfn() {
        let frames = PageFrames::new(PhysBytes(0x1_0000_0000));
        assert_eq!(frames.phys_to_pfn(PhysBytes(0)), 0);
        assert_eq!(frames.phys_to_pfn(PhysBytes(PAGE_SIZE)), 1);
        assert_eq!(frames.phys_to_pfn(PhysBytes(0x1000_0000)), (0x1000_0000 / PAGE_SIZE) as u32);
    }

    #[test]
    fn test_page_slot() {
        let slot = PageSlot::mapped(5, VirBytes(0x5000), None);
        assert_eq!(slot.pfn(), Some(5));
        assert!(slot.is_mapped());
        assert!(!slot.is_reserved());

        let reserved = PageSlot::reserved(VirBytes(0x1000), None);
        assert!(reserved.is_reserved());
        assert!(!reserved.is_mapped());
        assert_eq!(reserved.pfn(), None);
        assert_eq!(reserved.offset(), VirBytes(0x1000));

        let empty = PageSlot::Empty;
        assert!(empty.is_empty());
        assert!(!empty.is_mapped());
        assert_eq!(empty.pfn(), None);
        assert_eq!(empty.offset(), VirBytes(0));
    }

    #[test]
    fn test_incache() {
        let mut frames = PageFrames::new(PhysBytes(PAGE_SIZE * 4));
        frames.addcache(0);
        assert_eq!(frames.get(0).unwrap().refcount, 1);
        assert!(frames.get(0).unwrap().flags.contains(PageFlags::IN_CACHE));

        frames.rmcache(0);
        assert_eq!(frames.get(0).unwrap().refcount, 0);
        assert!(!frames.get(0).unwrap().flags.contains(PageFlags::IN_CACHE));
    }

    #[test]
    fn test_refcount_operations() {
        let mut frames = PageFrames::new(PhysBytes(PAGE_SIZE * 4));
        assert_eq!(frames.get(0).unwrap().refcount, 0);

        frames.get_mut(0).unwrap().refcount = frames.get(0).unwrap().refcount.saturating_add(1);
        assert_eq!(frames.get(0).unwrap().refcount, 1);

        frames.get_mut(0).unwrap().refcount -= 1;
        assert_eq!(frames.get(0).unwrap().refcount, 0);
    }
}
