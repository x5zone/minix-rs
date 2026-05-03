use alloc::vec;
use alloc::vec::Vec;
use super::alloc_trait::PhysAllocator;
use super::bitmap_alloc::BitmapAllocator;
use super::buddy_alloc::BuddyAllocator;
use super::early_heap::EarlyHeap;
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

fn make_heap() -> (&'static mut [u8], EarlyHeap) {
    let buffer: &'static mut [u8] = {
        let mut v = alloc::vec::Vec::with_capacity(16 * 1024 * 1024);
        v.resize(16 * 1024 * 1024, 0u8);
        let boxed = v.into_boxed_slice();
        alloc::boxed::Box::leak(boxed)
    };
    let mut heap = EarlyHeap::new();
    heap.init(buffer.as_mut_ptr(), 16 * 1024 * 1024);
    (buffer, heap)
}

mod basic {
    use super::*;

    #[test]
    fn bitmap_alloc_free_basic() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_regions(64));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_alloc_free_basic() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_regions(64));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn buddy_alloc_free_basic() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_regions(64));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn bitmap_alloc_zero() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_regions(64));
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_alloc_zero() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_regions(64));
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn buddy_alloc_zero() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_regions(64));
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn bitmap_single_page() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(1));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(1));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(1));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(4));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(4));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(4));
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
        let (_, mut heap) = make_heap();
        let total = 64;
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(total));
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
        let (_, mut heap) = make_heap();
        let total = 64;
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(total));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(20));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(20));
        let a = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 10);
        let c = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());
        alloc.free_mem(b, 10);
        alloc.free_mem(c, 10);
    }

    // NOTE: This test assumes LIFO (last-in-first-out) allocation behavior
    // from the buddy free list. If the allocator strategy changes to FIFO
    // or best-fit, this assertion may break and should be updated.
    #[test]
    fn buddy_free_realloc() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(64));
        let a = alloc.alloc_mem(16, PageAllocFlags::empty()).unwrap();
        let _b = alloc.alloc_mem(16, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 16);
        let c = alloc.alloc_mem(16, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());
    }

    #[test]
    fn bitmap_merge_on_free() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(100));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(100));
        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 50);
        alloc.free_mem(b, 50);
        let c = alloc.alloc_mem(100, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn buddy_merge_on_free() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(256));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_regions(32));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_lower16mb() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_regions(32));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_lower16mb() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_regions(32));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn bitmap_align64k() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_regions(4));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_align64k() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_regions(4));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_align64k() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_regions(4));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn bitmap_align16k() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_regions(1));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_align16k() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_regions(1));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_align16k() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_regions(1));
        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn bitmap_contig() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(100));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(100));
        let a = alloc.alloc_mem(10, PageAllocFlags::CONTIG).unwrap();
        let start = a.page_index();
        for i in 0..10 {
            assert!(!alloc.page_is_free(start + i));
        }
        alloc.free_mem(a, 10);
    }

    #[test]
    fn bitmap_lower1mb() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_regions(2));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap();
        assert!(addr.as_usize() < 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_lower1mb() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_regions(2));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap();
        assert!(addr.as_usize() < 1024 * 1024);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn buddy_lower1mb() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_regions(2));
        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap();
        assert!(addr.as_usize() < 1024 * 1024);
        alloc.free_mem(addr, 1);
    }
}

mod fragmentation {
    use super::*;

    #[test]
    fn bitmap_fragmentation() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(100));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(100));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(128));
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
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(100));
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
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_multi_regions());
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_multi_region() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_multi_regions());
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn buddy_multi_region() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_multi_regions());
        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());
        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn bitmap_gap_regions() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_gap_regions());
        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(b.is_ok());
        alloc.free_mem(a.unwrap(), 4);
        alloc.free_mem(b.unwrap(), 4);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_gap_regions() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_gap_regions());
        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(b.is_ok());
        alloc.free_mem(a.unwrap(), 4);
        alloc.free_mem(b.unwrap(), 4);
    }
}

mod stress {
    use super::*;

    fn stress_test<A: PhysAllocator>(alloc: &mut A, _total_pages: usize) {
        let mut allocations: Vec<Option<(PhysBytes, usize)>> = (0..50).map(|_| None).collect();
        let mut seed: u64 = 12345;

        for _ in 0..500 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let idx = (seed % 50) as usize;

            if allocations[idx].is_none() {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let clicks = ((seed % 5) + 1) as usize;
                if let Ok(addr) = alloc.alloc_mem(clicks, PageAllocFlags::empty()) {
                    allocations[idx] = Some((addr, clicks));
                }
            } else {
                let (addr, clicks) = allocations[idx].unwrap();
                alloc.free_mem(addr, clicks);
                allocations[idx] = None;
            }
        }

        for opt in allocations {
            if let Some((addr, clicks)) = opt {
                alloc.free_mem(addr, clicks);
            }
        }
    }

    #[test]
    fn bitmap_stress() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(200));
        stress_test(&mut alloc, 200);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_stress() {
        let (_, mut heap) = make_heap();
        let mut alloc = SegmentTreeAllocator::init(&mut heap, &make_small_regions(200));
        stress_test(&mut alloc, 200);
    }

    #[test]
    fn buddy_stress() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(512));
        stress_test(&mut alloc, 512);
    }
}

mod trait_object {
    use super::*;

    #[test]
    fn bitmap_as_dyn() {
        let (_, mut heap) = make_heap();
        let mut alloc: &mut dyn PhysAllocator = &mut BitmapAllocator::init(&mut heap, &make_regions(4));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn segment_tree_as_dyn() {
        let (_, mut heap) = make_heap();
        let mut alloc: &mut dyn PhysAllocator = &mut SegmentTreeAllocator::init(&mut heap, &make_regions(4));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn buddy_as_dyn() {
        let (_, mut heap) = make_heap();
        let mut alloc: &mut dyn PhysAllocator = &mut BuddyAllocator::init(&mut heap, &make_regions(4));
        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    #[cfg(feature = "segment_tree_alloc")]
    fn swap_implementation() {
        fn use_allocator(alloc: &mut dyn PhysAllocator) -> PhysBytes {
            alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap()
        }

        let (_, mut heap) = make_heap();
        let mut bitmap = BitmapAllocator::init(&mut heap, &make_regions(4));
        let addr1 = use_allocator(&mut bitmap);
        bitmap.free_mem(addr1, 1);

        let (_, mut heap) = make_heap();
        let mut segtree = SegmentTreeAllocator::init(&mut heap, &make_regions(4));
        let addr2 = use_allocator(&mut segtree);
        segtree.free_mem(addr2, 1);

        let (_, mut heap) = make_heap();
        let mut buddy = BuddyAllocator::init(&mut heap, &make_regions(4));
        let addr3 = use_allocator(&mut buddy);
        buddy.free_mem(addr3, 1);
    }
}

mod buddy_internal_frag {
    use super::*;

    #[test]
    fn buddy_allocates_power_of_two() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(16));
        let a = alloc.alloc_mem(3, PageAllocFlags::empty()).unwrap();
        assert_eq!(a.page_index() % 4, 0);
        alloc.free_mem(a, 3);
    }

    #[test]
    fn buddy_internal_frag_non_power2() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(6));
        let a = alloc.alloc_mem(2, PageAllocFlags::empty());
        assert!(a.is_ok());
        let b = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(b.is_ok(), "6 pages = 4+2 after merge, so 4-page alloc should succeed");
        alloc.free_mem(a.unwrap(), 2);
        alloc.free_mem(b.unwrap(), 4);
    }
}

mod error_types {
    use super::*;

    #[test]
    fn bitmap_oom_is_out_of_memory() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(4));
        let _a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        let err = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap_err();
        assert_eq!(err, AllocError::OutOfMemory);
    }

    #[test]
    fn bitmap_low_mem_exhausted() {
        let (_, mut heap) = make_heap();
        let mut alloc = BitmapAllocator::init(&mut heap, &make_small_regions(4));
        let _a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);
    }

    #[test]
    fn buddy_oom_is_out_of_memory() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(4));
        let _a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        let err = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap_err();
        assert_eq!(err, AllocError::OutOfMemory);
    }

    #[test]
    fn buddy_low_mem_exhausted() {
        let (_, mut heap) = make_heap();
        let mut alloc = BuddyAllocator::init(&mut heap, &make_small_regions(256));
        let _a = alloc.alloc_mem(256, PageAllocFlags::empty()).unwrap();
        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);
    }
}
