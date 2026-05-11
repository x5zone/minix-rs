pub(crate) mod types;
pub(crate) mod alloc_trait;
pub(crate) mod bitmap_alloc;
pub(crate) mod segment_tree_alloc;
pub(crate) mod buddy_alloc;
pub(crate) mod stats;
#[cfg(test)]
pub(crate) mod allocator_tests;

#[allow(unused_imports)]
pub(crate) use alloc_trait::{PhysAllocator, PhysAllocatorStats, PhysMemStats};
pub(crate) use bitmap_alloc::BitmapAllocator;
#[allow(unused_imports)]
pub(crate) use buddy_alloc::BuddyAllocator;
#[allow(unused_imports)]
pub(crate) use segment_tree_alloc::SegmentTreeAllocator;
#[allow(unused_imports)]
pub(crate) use stats::MemStats;
pub(crate) use types::{AllocError, PageAllocFlags, PhysBytes};

#[cfg(feature = "buddy_alloc")]
pub(crate) type DefaultAllocator = BuddyAllocator;

#[cfg(all(feature = "segment_tree_alloc", not(feature = "buddy_alloc")))]
pub(crate) type DefaultAllocator = SegmentTreeAllocator;

#[cfg(all(feature = "bitmap_alloc", not(any(feature = "segment_tree_alloc", feature = "buddy_alloc"))))]
pub(crate) type DefaultAllocator = BitmapAllocator;

#[cfg(not(any(feature = "bitmap_alloc", feature = "segment_tree_alloc", feature = "buddy_alloc")))]
pub(crate) type DefaultAllocator = BitmapAllocator;

pub(crate) enum PhysAlloc {
    Bitmap(BitmapAllocator),
    #[cfg(feature = "buddy_alloc")]
    Buddy(BuddyAllocator),
    #[cfg(feature = "segment_tree_alloc")]
    SegmentTree(SegmentTreeAllocator),
}

impl PhysAllocator for PhysAlloc {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
        match self {
            PhysAlloc::Bitmap(b) => b.alloc_mem(clicks, flags),
            #[cfg(feature = "buddy_alloc")]
            PhysAlloc::Buddy(b) => b.alloc_mem(clicks, flags),
            #[cfg(feature = "segment_tree_alloc")]
            PhysAlloc::SegmentTree(s) => s.alloc_mem(clicks, flags),
        }
    }

    fn free_mem(&mut self, base: PhysBytes, clicks: usize) {
        match self {
            PhysAlloc::Bitmap(b) => b.free_mem(base, clicks),
            #[cfg(feature = "buddy_alloc")]
            PhysAlloc::Buddy(b) => b.free_mem(base, clicks),
            #[cfg(feature = "segment_tree_alloc")]
            PhysAlloc::SegmentTree(s) => s.free_mem(base, clicks),
        }
    }

    fn total_count(&self) -> usize {
        match self {
            PhysAlloc::Bitmap(b) => b.total_count(),
            #[cfg(feature = "buddy_alloc")]
            PhysAlloc::Buddy(b) => b.total_count(),
            #[cfg(feature = "segment_tree_alloc")]
            PhysAlloc::SegmentTree(s) => s.total_count(),
        }
    }

    fn reserve_pages(&mut self, base_page: usize, count: usize) {
        match self {
            PhysAlloc::Bitmap(b) => b.reserve_pages(base_page, count),
            #[cfg(feature = "buddy_alloc")]
            PhysAlloc::Buddy(b) => b.reserve_pages(base_page, count),
            #[cfg(feature = "segment_tree_alloc")]
            PhysAlloc::SegmentTree(s) => s.reserve_pages(base_page, count),
        }
    }
}

impl PhysAlloc {
    pub(crate) fn is_bitmap(&self) -> bool {
        matches!(self, PhysAlloc::Bitmap(_))
    }

    #[cfg(feature = "buddy_alloc")]
    pub(crate) fn as_buddy(&self) -> Option<&BuddyAllocator> {
        match self {
            PhysAlloc::Buddy(b) => Some(b),
            _ => None,
        }
    }

    #[cfg(feature = "buddy_alloc")]
    pub(crate) fn as_buddy_mut(&mut self) -> Option<&mut BuddyAllocator> {
        match self {
            PhysAlloc::Buddy(b) => Some(b),
            _ => None,
        }
    }

    pub(crate) fn as_bitmap_mut(&mut self) -> Option<&mut BitmapAllocator> {
        match self {
            PhysAlloc::Bitmap(b) => Some(b),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysAllocType {
    Bitmap,
    Buddy,
    SegmentTree,
}

impl PhysAllocType {
    pub fn metadata_size(&self, total_pages: usize) -> usize {
        let exact = self.metadata_size_exact(total_pages);
        (exact + CLICK_SIZE - 1) & !(CLICK_SIZE - 1)
    }

    pub fn metadata_size_exact(&self, total_pages: usize) -> usize {
        match self {
            PhysAllocType::Bitmap => {
                let bitmap_chunks = (total_pages + 63) / 64;
                bitmap_chunks * core::mem::size_of::<u64>()
                    + 10000 * core::mem::size_of::<usize>()
            }
            PhysAllocType::Buddy => {
                let max_order = if total_pages == 0 {
                    0
                } else {
                    total_pages.next_power_of_two().trailing_zeros() as usize
                };
                (max_order + 1) * core::mem::size_of::<u32>()
                    + total_pages * core::mem::size_of::<u32>()
                    + total_pages * core::mem::size_of::<u8>()
            }
            PhysAllocType::SegmentTree => {
                let offset = if total_pages == 0 {
                    1
                } else {
                    total_pages.next_power_of_two()
                };
                let tree_size = if total_pages > 0 { 2 * offset } else { 2 };
                tree_size * core::mem::size_of::<(usize, usize, usize, usize)>()
            }
        }
    }
}

pub(crate) const CLICK_SIZE: usize = 4096;
pub(crate) const CLICK_SHIFT: usize = 12;

#[inline]
pub(crate) const fn bytes_to_clicks(bytes: usize) -> usize {
    (bytes + CLICK_SIZE - 1) >> CLICK_SHIFT
}

#[inline]
pub(crate) const fn clicks_to_bytes(clicks: usize) -> usize {
    clicks << CLICK_SHIFT
}

#[inline]
pub(crate) const fn click_floor(addr: usize) -> usize {
    (addr >> CLICK_SHIFT) << CLICK_SHIFT
}

#[inline]
pub(crate) const fn click_ceil(addr: usize) -> usize {
    ((addr + CLICK_SIZE - 1) >> CLICK_SHIFT) << CLICK_SHIFT
}

#[derive(Debug, Clone, Copy)]
pub struct BootMemRegion {
    pub base: usize,
    pub size: usize,
}

impl BootMemRegion {
    pub(crate) fn validate(&self) {
        assert!(
            self.base % CLICK_SIZE == 0,
            "BootMemRegion base must be page-aligned, got {:#x}",
            self.base
        );
        assert!(
            self.size % CLICK_SIZE == 0,
            "BootMemRegion size must be page-aligned, got {:#x}",
            self.size
        );
    }
}

pub(super) fn compute_memory_bounds(regions: &[BootMemRegion]) -> (usize, usize, usize) {
    let mut mem_low = usize::MAX;
    let mut mem_high = 0;

    for region in regions {
        if region.size == 0 {
            continue;
        }
        let from = region.base;
        let to = region.base + region.size - 1;
        if from < mem_low {
            mem_low = from;
        }
        if to > mem_high {
            mem_high = to;
        }
    }

    if mem_low == usize::MAX {
        mem_low = 0;
    }

    let total_pages = if mem_high > 0 { mem_high / CLICK_SIZE + 1 } else { 0 };
    (total_pages, mem_low, mem_high)
}

pub(super) fn is_low_mem_flag(flags: PageAllocFlags) -> bool {
    flags.intersects(PageAllocFlags::LOWER1MB | PageAllocFlags::LOWER16MB)
}

pub(super) fn oom_error(flags: PageAllocFlags) -> AllocError {
    if is_low_mem_flag(flags) {
        AllocError::LowMemoryExhausted
    } else {
        AllocError::OutOfMemory
    }
}

#[cfg(test)]
mod metadata_size_tests {
    use super::*;

    #[test]
    fn test_bitmap_metadata_size() {
        let total_pages = 1024;
        let size = PhysAllocType::Bitmap.metadata_size_exact(total_pages);
        let bitmap_chunks = (total_pages + 63) / 64;
        let expected = bitmap_chunks * 8 + 10000 * core::mem::size_of::<usize>();
        assert_eq!(size, expected);
    }

    #[test]
    fn test_buddy_metadata_size() {
        let total_pages = 1024;
        let size = PhysAllocType::Buddy.metadata_size_exact(total_pages);
        let max_order = total_pages.next_power_of_two().trailing_zeros() as usize;
        let expected = (max_order + 1) * 4 + total_pages * 4 + total_pages * 1;
        assert_eq!(size, expected);
    }

    #[test]
    fn test_metadata_size_page_aligned() {
        let total_pages = 1024;
        for alloc_type in [PhysAllocType::Bitmap, PhysAllocType::Buddy, PhysAllocType::SegmentTree] {
            let aligned = alloc_type.metadata_size(total_pages);
            assert_eq!(aligned % CLICK_SIZE, 0, "{:?}: metadata_size not page-aligned", alloc_type);
            assert!(aligned >= alloc_type.metadata_size_exact(total_pages));
        }
    }

    #[test]
    fn test_metadata_size_zero_pages() {
        assert_eq!(PhysAllocType::Buddy.metadata_size_exact(0), 4);
        assert_eq!(PhysAllocType::SegmentTree.metadata_size_exact(0), 2 * core::mem::size_of::<(usize, usize, usize, usize)>());
        let bitmap_zero = PhysAllocType::Bitmap.metadata_size_exact(0);
        assert_eq!(bitmap_zero, 10000 * core::mem::size_of::<usize>());
    }
}
