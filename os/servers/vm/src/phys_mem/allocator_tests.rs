use alloc::vec;
use alloc::vec::Vec;
use super::alloc_trait::PhysAllocator;
use super::bitmap_alloc::BitmapAllocator;
use super::buddy_alloc::BuddyAllocator;
#[cfg(feature = "segment_tree_alloc")]
use super::segment_tree_alloc::SegmentTreeAllocator;
use super::types::{AllocError, PageAllocFlags, PhysBytes};
use super::{CLICK_SIZE, BootMemRegion};

fn make_regions(size_mb: usize) -> Vec<BootMemRegion> {
    vec![BootMemRegion { base: 0, size: size_mb * 1024 * 1024 }]
}

fn make_small_regions(pages: usize) -> Vec<BootMemRegion> {
    vec![BootMemRegion { base: 0, size: pages * CLICK_SIZE }]
}

fn make_multi_regions() -> Vec<BootMemRegion> {
    vec![
        BootMemRegion { base: 0x100000, size: 4 * 1024 * 1024 },
        BootMemRegion { base: 0x10000000, size: 8 * 1024 * 1024 },
    ]
}

fn make_gap_regions() -> Vec<BootMemRegion> {
    vec![
        BootMemRegion { base: 0, size: 4 * CLICK_SIZE },
        BootMemRegion { base: 100 * CLICK_SIZE, size: 4 * CLICK_SIZE },
    ]
}

fn make_metadata() -> &'static mut [u8] {
    let v: Vec<u8> = alloc::vec![0u8; 16 * 1024 * 1024];
    alloc::boxed::Box::leak(v.into_boxed_slice())
}

mod basic {
    use super::*;

    #[test]
    fn bitmap_alloc_free_basic() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_regions(64));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_alloc_free_basic() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_regions(64));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn buddy_alloc_free_basic() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_regions(64));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn bitmap_alloc_zero() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_regions(64));
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_alloc_zero() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_regions(64));
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn buddy_alloc_zero() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_regions(64));
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn bitmap_single_page() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(1));
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());
        alloc.free_mem(a.unwrap(), 1);
        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_single_page() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(1));
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());
        alloc.free_mem(a.unwrap(), 1);
        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn buddy_single_page() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_small_regions(1));
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());
        alloc.free_mem(a.unwrap(), 1);
        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }
}

mod exhaustion {
    use super::*;

    #[test]
    fn bitmap_exhaustion() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(4));
        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());
        alloc.free_mem(a.unwrap(), 4);
        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_exhaustion() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(4));
        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());
        alloc.free_mem(a.unwrap(), 4);
        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn buddy_exhaustion() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_small_regions(4));
        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());
        alloc.free_mem(a.unwrap(), 4);
        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn bitmap_alloc_all_then_free_all() {
        let metadata = make_metadata();
        let total = 64;
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(total));
        let mut addrs = Vec::new();
        for _ in 0..total {
            let addr = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
            addrs.push(addr);
        }
        assert!(alloc.alloc_mem(1, PageAllocFlags::empty()).is_err());
        for addr in addrs {
            alloc.free_mem(addr, 1);
        }
        let again = alloc.alloc_mem(total, PageAllocFlags::empty());
        assert!(again.is_ok());
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_alloc_all_then_free_all() {
        let metadata = make_metadata();
        let total = 64;
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(total));
        let mut addrs = Vec::new();
        for _ in 0..total {
            let addr = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
            addrs.push(addr);
        }
        assert!(alloc.alloc_mem(1, PageAllocFlags::empty()).is_err());
        for addr in addrs {
            alloc.free_mem(addr, 1);
        }
        let again = alloc.alloc_mem(total, PageAllocFlags::empty());
        assert!(again.is_ok());
    }
}

mod free_realloc {
    use super::*;

    #[test]
    fn bitmap_free_realloc() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(20));
        let a = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 10);
        let c = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());
        alloc.free_mem(b, 10);
        alloc.free_mem(c, 10);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_free_realloc() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(20));
        let a = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 10);
        let c = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());
        alloc.free_mem(b, 10);
        alloc.free_mem(c, 10);
    }

    #[test]
    fn buddy_free_realloc() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_small_regions(64));
        let a = alloc.alloc_mem(16, PageAllocFlags::empty()).unwrap();
        let _b = alloc.alloc_mem(16, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 16);
        let c = alloc.alloc_mem(16, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());
    }

    #[test]
    fn bitmap_merge_on_free() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(100));
        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 50);
        alloc.free_mem(b, 50);
        let c = alloc.alloc_mem(100, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_merge_on_free() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(100));
        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 50);
        alloc.free_mem(b, 50);
        let c = alloc.alloc_mem(100, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn buddy_merge_on_free() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_small_regions(256));
        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 128);
        alloc.free_mem(b, 128);
        let c = alloc.alloc_mem(256, PageAllocFlags::empty());
        assert!(c.is_ok());
    }
}

mod flags {
    use super::*;

    #[test]
    fn bitmap_lower16mb() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_regions(32));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_lower16mb() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_regions(32));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_lower16mb() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_regions(32));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn bitmap_align64k() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_regions(4));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_align64k() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_regions(4));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_align64k() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_regions(4));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn bitmap_align16k() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_regions(1));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_align16k() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_regions(1));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_align16k() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_regions(1));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn bitmap_contig() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(100));
        let a = alloc.alloc_mem(10, PageAllocFlags::CONTIG).unwrap();
        let start = a.page_index();
        for i in 0..10 {
            assert!(!alloc.page_is_free(start + i));
        }
        alloc.free_mem(a, 10);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_contig() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(100));
        let a = alloc.alloc_mem(10, PageAllocFlags::CONTIG).unwrap();
        let start = a.page_index();
        for i in 0..10 {
            assert!(!alloc.page_is_free(start + i));
        }
        alloc.free_mem(a, 10);
    }

    #[test]
    fn bitmap_lower1mb() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_regions(2));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap();
        assert!(addr.as_usize() < 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_lower1mb() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_regions(2));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap();
        assert!(addr.as_usize() < 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_lower1mb() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_regions(2));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap();
        assert!(addr.as_usize() < 1024 * 1024);
        alloc.free_mem(addr, 1);
    }
}

mod fragmentation {
    use super::*;

    #[test]
    fn bitmap_fragmentation() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_small_regions(100));
        let mut ptrs: Vec<Option<(PhysBytes, usize)>> = Vec::new();
        for size in 1..=10usize {
            if let Ok(addr) = alloc.alloc_mem(size, PageAllocFlags::empty()) {
                ptrs.push(Some((addr, size)));
            }
        }
        for i in (0..ptrs.len()).step_by(2) {
            if let Some((addr, size)) = ptrs[i] {
                alloc.free_mem(addr, size);
                ptrs[i] = None;
            }
        }
        if let Ok(addr) = alloc.alloc_mem(5, PageAllocFlags::CONTIG) {
            alloc.free_mem(addr, 5);
        }
        for opt in ptrs {
            if let Some((addr, size)) = opt {
                alloc.free_mem(addr, size);
            }
        }
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_fragmentation() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(100));
        let mut ptrs: Vec<Option<(PhysBytes, usize)>> = Vec::new();
        for size in 1..=10usize {
            if let Ok(addr) = alloc.alloc_mem(size, PageAllocFlags::empty()) {
                ptrs.push(Some((addr, size)));
            }
        }
        for i in (0..ptrs.len()).step_by(2) {
            if let Some((addr, size)) = ptrs[i] {
                alloc.free_mem(addr, size);
                ptrs[i] = None;
            }
        }
        if let Ok(addr) = alloc.alloc_mem(5, PageAllocFlags::CONTIG) {
            alloc.free_mem(addr, 5);
        }
        for opt in ptrs {
            if let Some((addr, size)) = opt {
                alloc.free_mem(addr, size);
            }
        }
    }

    #[test]
    fn buddy_fragmentation() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_small_regions(128));
        let mut ptrs: Vec<Option<(PhysBytes, usize)>> = Vec::new();
        for size in [1usize, 2, 4, 8, 16, 32].iter() {
            if let Ok(addr) = alloc.alloc_mem(*size, PageAllocFlags::empty()) {
                ptrs.push(Some((addr, *size)));
            }
        }
        for i in (0..ptrs.len()).step_by(2) {
            if let Some((addr, size)) = ptrs[i] {
                alloc.free_mem(addr, size);
                ptrs[i] = None;
            }
        }
        for opt in ptrs {
            if let Some((addr, size)) = opt {
                alloc.free_mem(addr, size);
            }
        }
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_largest_free_tracking() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_small_regions(100));
        assert_eq!(alloc.largest_free(), 100);

        let a = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        let c = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        let d = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 25);
        alloc.free_mem(c, 25);
        assert_eq!(alloc.largest_free(), 25);

        alloc.free_mem(b, 25);
        assert_eq!(alloc.largest_free(), 75);

        alloc.free_mem(d, 25);
        assert_eq!(alloc.largest_free(), 100);
    }
}

mod multi_region {
    use super::*;

    #[test]
    fn bitmap_multi_region() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_multi_regions());
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_multi_region() {
        let metadata = make_metadata();
        let mut alloc = SegmentTreeAllocator::init(metadata, &make_multi_regions());
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn buddy_multi_region() {
        let metadata = make_metadata();
        let mut alloc = BuddyAllocator::init(metadata, &make_multi_regions());
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn bitmap_gap_regions() {
        let metadata = make_metadata();
        let mut alloc = BitmapAllocator::init(metadata, &make_gap_regions());
        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());
    }
}
