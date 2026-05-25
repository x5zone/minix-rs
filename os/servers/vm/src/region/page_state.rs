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

pub const PAGE_SIZE: u64 = 4096;

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
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub(crate) struct PageState {
    pub(crate) refcount: u16,
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

    pub fn refcount(&self) -> u16 {
        self.refcount
    }

    pub fn flags(&self) -> PageFlags {
        self.flags
    }

    pub fn is_cached(&self) -> bool {
        self.flags.contains(PageFlags::IN_CACHE)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PageSlot {
    pub(crate) pfn: u32,
    pub(crate) offset: VirBytes,
    pub(crate) memtype: Option<&'static dyn MemType>,
}

pub const PFN_NONE: u32 = u32::MAX;

impl PartialEq for PageSlot {
    fn eq(&self, other: &Self) -> bool {
        self.pfn == other.pfn && self.offset == other.offset
    }
}

impl Eq for PageSlot {}

impl PageSlot {
    pub fn new(pfn: u32, offset: VirBytes, memtype: Option<&'static dyn MemType>) -> Self {
        Self { pfn, offset, memtype }
    }

    pub fn pfn(&self) -> u32 {
        self.pfn
    }

    pub fn offset(&self) -> VirBytes {
        self.offset
    }

    pub fn memtype(&self) -> Option<&'static dyn MemType> {
        self.memtype
    }

    pub fn is_mapped(&self) -> bool {
        self.pfn != PFN_NONE
    }
}

impl core::fmt::Debug for PageSlot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageSlot")
            .field("pfn", &self.pfn)
            .field("offset", &self.offset)
            .field("memtype", &self.memtype.map(|m| m.name()))
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct PageFrames {
    states: Vec<PageState>,
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
        if let Some(state) = self.states.get_mut(pfn as usize) {
            if !state.flags.contains(PageFlags::IN_CACHE) {
                state.flags.insert(PageFlags::IN_CACHE);
                state.refcount = state.refcount.saturating_add(1);
            }
        }
    }

    pub fn rmcache(&mut self, pfn: u32) {
        if let Some(state) = self.states.get_mut(pfn as usize) {
            if state.flags.contains(PageFlags::IN_CACHE) {
                state.flags.remove(PageFlags::IN_CACHE);
                if state.refcount > 0 {
                    state.refcount -= 1;
                }
            }
        }
    }

    // TODO: Implement verify_refcounts (documented in 10-phys-pagestate.md Ch3§3.3.5).
    // This function should traverse all processes' VirRegions, count per-PFN references,
    // and compare with PageFrames.refcount. Equivalent to Minix3's map_sanitycheck().
    // Cannot be implemented in page_state.rs because it needs VmProcTable (vmproc module).
    // Suggested location: a top-level function in vm_server.rs or a dedicated sanity module,
    // with signature like:
    //   fn verify_refcounts(frames: &PageFrames, table: &VmProcTable) -> Result<(), Vec<(u32, u16, u16)>>
    // Returns Err with (pfn, expected, actual) for each mismatch.
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
        let slot = PageSlot::new(5, VirBytes(0x5000), None);
        assert_eq!(slot.pfn, 5);
        assert!(slot.is_mapped());

        let empty = PageSlot::new(PFN_NONE, VirBytes(0), None);
        assert!(!empty.is_mapped());
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
