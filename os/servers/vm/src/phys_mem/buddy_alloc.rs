//! Buddy system physical memory allocator.
//!
//! This allocator uses the buddy system algorithm for O(log n) allocation
//! and automatic coalescing of free blocks. Uses SoA (Structure of Arrays)
//! design for better cache efficiency.

use super::alloc_trait::{PhysAllocator, PhysAllocatorStats, PhysMemStats};
use super::stats::MemStats;
use super::types::{AllocError, PageAllocFlags, PhysBytes};
use super::{CLICK_SIZE, BootMemRegion, EarlyHeap};

const FLAG_ALLOCATED: u8 = 0x80;
const ORDER_MASK: u8 = 0x7F;
const ORDER_INVALID: u8 = 0xFF;
// MAX_ORDER=30 limits addressable memory to 2^30 * 4KB = 4TB.
// To support more, increase to 31 (8TB) or 32 (16TB, requires u33 indices).
const MAX_ORDER: usize = 30;
const FREE_LIST_SENTINEL: u32 = u32::MAX;
// u32 page indices limit total pages to 2^32-1, i.e. 2^32 * 4KB = 16TB.

pub struct BuddyAllocator {
    free_list_heads: &'static mut [u32],
    page_next: &'static mut [u32],
    page_orders: &'static mut [u8],
    total_pages: usize,
    max_order: usize,
    free_pages: usize,
    stats: MemStats,
}

impl BuddyAllocator {
    pub fn init(early_heap: &mut EarlyHeap, regions: &[BootMemRegion]) -> Self {
        let (total_pages, _mem_low, _mem_high) = super::compute_memory_bounds(regions);
        let max_order = compute_max_order(total_pages);

        let free_list_heads = early_heap.alloc_slice::<u32>(max_order + 1);
        let page_next = early_heap.alloc_slice::<u32>(total_pages);
        let page_orders = early_heap.alloc_slice::<u8>(total_pages);

        for head in free_list_heads.iter_mut() {
            *head = FREE_LIST_SENTINEL;
        }
        for next in page_next.iter_mut() {
            *next = FREE_LIST_SENTINEL;
        }
        for order in page_orders.iter_mut() {
            *order = ORDER_INVALID;
        }

        let mut alloc = Self {
            free_list_heads,
            page_next,
            page_orders,
            total_pages,
            max_order,
            free_pages: 0,
            stats: MemStats::new(),
        };

        for region in regions {
            if region.size == 0 {
                continue;
            }
            region.validate();
            let base_page = region.base / CLICK_SIZE;
            let num_pages = region.size / CLICK_SIZE;
            alloc.add_free_region(base_page, num_pages);
        }

        alloc
    }

    pub fn total_memory(&self) -> usize {
        self.total_pages * CLICK_SIZE
    }

    pub fn free_memory(&self) -> usize {
        self.free_pages * CLICK_SIZE
    }

    pub fn is_under_pressure(&self) -> bool {
        self.free_pages * 10 < self.total_pages
    }

    pub(crate) fn largest_free(&self) -> usize {
        for order in (0..=self.max_order).rev() {
            if self.free_list_heads[order] != FREE_LIST_SENTINEL {
                return 1usize << order;
            }
        }
        0
    }

    fn add_free_region(&mut self, start: usize, count: usize) {
        let mut pos = start;
        let end = start + count;

        while pos < end {
            let remaining = end - pos;
            let max_align_order = pos.trailing_zeros() as usize;
            let max_size_order = if remaining == 0 {
                0
            } else {
                remaining.ilog2() as usize
            };
            let order = max_align_order.min(max_size_order).min(self.max_order);
            let block_size = 1usize << order;

            if pos + block_size > end {
                let gap = end - pos;
                let fallback_order = if gap <= 1 {
                    0
                } else {
                    gap.ilog2() as usize
                };
                let fallback_order = fallback_order.min(order);
                if fallback_order == 0 {
                    self.free_block_internal(pos, 0);
                    pos += 1;
                    continue;
                }
                let fallback_size = 1usize << fallback_order;
                self.free_block_internal(pos, fallback_order);
                pos += fallback_size;
                continue;
            }

            self.free_block_internal(pos, order);
            pos += block_size;
        }
    }

    fn free_block_internal(&mut self, page: usize, order: usize) {
        let block_size = 1usize << order;
        self.page_orders[page] = order as u8;
        self.free_pages += block_size;
        self.push_free(order, page);
        self.try_merge(page, order);
    }

    fn try_merge(&mut self, page: usize, mut order: usize) {
        let mut page = page;
        while order < self.max_order {
            let buddy = Self::buddy_of(page, order);

            if buddy >= self.total_pages {
                break;
            }

            if self.page_orders[buddy] == ORDER_INVALID {
                break;
            }

            if (self.page_orders[buddy] & FLAG_ALLOCATED) != 0 {
                break;
            }

            if (self.page_orders[buddy] & ORDER_MASK) != order as u8 {
                break;
            }

            if !self.remove_from_free_list(order, buddy) {
                break;
            }

            if !self.remove_from_free_list(order, page) {
                self.push_free(order, buddy);
                break;
            }

            self.page_orders[page] = ORDER_INVALID;
            self.page_orders[buddy] = ORDER_INVALID;
            let merged = page.min(buddy);
            order += 1;
            self.page_orders[merged] = order as u8;
            self.push_free(order, merged);

            page = merged;
        }
    }

    fn alloc_block(&mut self, order: usize) -> Option<usize> {
        if order > self.max_order {
            return None;
        }

        if let Some(page) = self.pop_free(order) {
            let block_size = 1usize << order;
            self.page_orders[page] = (order as u8) | FLAG_ALLOCATED;
            self.free_pages -= block_size;
            return Some(page);
        }

        if order < self.max_order {
            if let Some(block) = self.alloc_block(order + 1) {
                let buddy = block + (1usize << order);
                let buddy_size = 1usize << order;
                self.page_orders[buddy] = order as u8;
                self.push_free(order, buddy);
                self.free_pages += buddy_size;
                self.page_orders[block] = (order as u8) | FLAG_ALLOCATED;
                return Some(block);
            }
        }

        None
    }

    fn buddy_of(page: usize, order: usize) -> usize {
        page ^ (1usize << order)
    }

    fn push_free(&mut self, order: usize, page: usize) {
        let head = self.free_list_heads[order];
        self.page_next[page] = head;
        self.free_list_heads[order] = page as u32;
    }

    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let head = self.free_list_heads[order];
        if head == FREE_LIST_SENTINEL {
            return None;
        }
        let page = head as usize;
        self.free_list_heads[order] = self.page_next[page];
        self.page_next[page] = FREE_LIST_SENTINEL;
        Some(page)
    }

    fn remove_from_free_list(&mut self, order: usize, target: usize) -> bool {
        let head = self.free_list_heads[order];

        if head == FREE_LIST_SENTINEL {
            return false;
        }

        if head as usize == target {
            self.free_list_heads[order] = self.page_next[target];
            self.page_next[target] = FREE_LIST_SENTINEL;
            return true;
        }

        let max_chain_len = self.total_pages / (1usize << order).max(1);
        let mut prev = head as usize;
        let mut visited = 0;
        loop {
            let next = self.page_next[prev];
            if next == FREE_LIST_SENTINEL {
                return false;
            }
            if next as usize == target {
                self.page_next[prev] = self.page_next[target];
                self.page_next[target] = FREE_LIST_SENTINEL;
                return true;
            }
            prev = next as usize;
            visited += 1;
            if visited > max_chain_len {
                return false;
            }
        }
    }

    fn order_for_pages(pages: usize) -> usize {
        if pages == 0 {
            return 0;
        }
        (pages - 1).next_power_of_two().trailing_zeros() as usize
    }
}

impl PhysAllocator for BuddyAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
        if clicks == 0 {
            self.stats.record_failure();
            return Err(AllocError::OutOfMemory);
        }

        let align_clicks = if flags.contains(PageAllocFlags::ALIGN64K) {
            (64 * 1024) / CLICK_SIZE
        } else if flags.contains(PageAllocFlags::ALIGN16K) {
            (16 * 1024) / CLICK_SIZE
        } else {
            0
        };

        let max_page = if flags.contains(PageAllocFlags::LOWER1MB) {
            (1 * 1024 * 1024) / CLICK_SIZE
        } else if flags.contains(PageAllocFlags::LOWER16MB) {
            (16 * 1024 * 1024) / CLICK_SIZE
        } else {
            self.total_pages
        };

        let size_order = Self::order_for_pages(clicks);
        let align_order = if align_clicks > 0 {
            align_clicks.trailing_zeros() as usize
        } else {
            0
        };
        let order = size_order.max(align_order);

        let page = match self.alloc_block(order) {
            Some(p) => p,
            None => {
                self.stats.record_failure();
                return Err(super::oom_error(flags));
            }
        };

        let block_size = 1usize << order;

        if page + block_size > max_page {
            self.add_free_region(page, block_size);
            self.stats.record_failure();
            return Err(AllocError::LowMemoryExhausted);
        }

        if block_size > clicks {
            self.add_free_region(page + clicks, block_size - clicks);
        }

        if flags.contains(PageAllocFlags::CLEAR) {
            // TODO: requires kernel IPC (sys_memset), implement after kernel interface
        }

        self.stats.record_alloc(clicks * CLICK_SIZE);
        Ok(PhysBytes::from_page_index(page))
    }

    fn free_mem(&mut self, base: PhysBytes, clicks: usize) {
        if clicks == 0 {
            return;
        }
        let start_page = base.page_index();
        self.add_free_region(start_page, clicks);
        self.stats.record_free(clicks * CLICK_SIZE);
    }

    fn total_count(&self) -> usize {
        self.total_pages
    }
}

impl PhysAllocatorStats for BuddyAllocator {
    fn memstats(&self) -> PhysMemStats {
        let largest_free = self.largest_free();
        PhysMemStats {
            free_nodes: 0, // Buddy system does not track free node count; use free_pages instead
            free_pages: self.free_pages,
            largest_free,
        }
    }
}

fn compute_max_order(total_pages: usize) -> usize {
    if total_pages == 0 {
        return 0;
    }
    total_pages.next_power_of_two().trailing_zeros() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_regions() -> Vec<BootMemRegion> {
        vec![BootMemRegion { base: 0, size: 128 * 1024 * 1024 }]
    }

    fn make_test_heap(regions: &[BootMemRegion]) -> (&'static mut [u8], EarlyHeap) {
        let (total_pages, _, _) = super::super::compute_memory_bounds(regions);
        let max_order = compute_max_order(total_pages);

        let heads_size = (max_order + 1) * 4;
        let next_size = total_pages * 4;
        let orders_size = total_pages;
        let heap_size = heads_size + next_size + orders_size + 1024;

        let buffer: &'static mut [u8] = {
            let mut v = alloc::vec::Vec::with_capacity(2 * 1024 * 1024);
            v.resize(2 * 1024 * 1024, 0u8);
            let boxed = v.into_boxed_slice();
            alloc::boxed::Box::leak(boxed)
        };

        let mut heap = EarlyHeap::new();
        heap.init(buffer.as_mut_ptr(), heap_size);
        (buffer, heap)
    }

    #[test]
    fn test_alloc_free_basic() {
        let regions = make_test_regions();
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn test_alloc_zero_pages() {
        let regions = make_test_regions();
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn test_alloc_exhaustion() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());

        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());

        alloc.free_mem(a.unwrap(), 4);

        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn test_buddy_merge() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 128);
        alloc.free_mem(b, 128);

        let c = alloc.alloc_mem(256, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn test_largest_free() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        assert!(alloc.largest_free() >= 256);

        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 128);
    }

    #[test]
    fn test_internal_fragmentation() {
        let regions = vec![BootMemRegion { base: 0, size: 16 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(3, PageAllocFlags::empty()).unwrap();
        let start = a.page_index();

        assert!(start % 4 == 0);

        alloc.free_mem(a, 3);
    }

    #[test]
    fn test_multiple_allocations() {
        let regions = vec![BootMemRegion { base: 0, size: 64 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let mut addrs = Vec::new();
        for _ in 0..4 {
            let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
            addrs.push(addr);
        }

        for addr in addrs {
            alloc.free_mem(addr, 4);
        }
    }

    #[test]
    fn test_multiple_regions() {
        let regions = vec![
            BootMemRegion { base: 0x100000, size: 4 * 1024 * 1024 },
            BootMemRegion { base: 0x10000000, size: 8 * 1024 * 1024 },
        ];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn test_single_page() {
        let regions = vec![BootMemRegion { base: 0, size: CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());

        alloc.free_mem(a.unwrap(), 1);

        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn test_merge_chain() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 128);
        alloc.free_mem(b, 128);

        assert_eq!(alloc.largest_free(), 256);

        let c = alloc.alloc_mem(256, PageAllocFlags::empty());
        assert!(c.is_ok(), "should be able to re-allocate full 256 pages after merge chain");

        alloc.free_mem(c.unwrap(), 256);
    }

    #[test]
    fn test_low_mem_exhausted_error() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let _a = alloc.alloc_mem(256, PageAllocFlags::empty()).unwrap();

        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);
    }

    #[test]
    fn test_oom_error_type() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let _a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();

        let err = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap_err();
        assert_eq!(err, AllocError::OutOfMemory);
    }

    #[test]
    fn test_align64k_no_leak() {
        let regions = vec![BootMemRegion { base: 0, size: 1024 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let initial_free = alloc.free_pages;

        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);

        alloc.free_mem(addr, 1);

        assert_eq!(alloc.free_pages, initial_free, "free_pages should return to initial after alloc+free with ALIGN64K");
    }

    #[test]
    fn test_align16k_no_leak() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let initial_free = alloc.free_pages;

        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN16K).unwrap();
        assert_eq!(addr.as_usize() % (16 * 1024), 0);

        alloc.free_mem(addr, 1);

        assert_eq!(alloc.free_pages, initial_free, "free_pages should return to initial after alloc+free with ALIGN16K");
    }

    #[test]
    fn test_align_naturally_satisfied() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BuddyAllocator::init(&mut heap, &regions);

        let addr = alloc.alloc_mem(16, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);

        let initial_free = alloc.free_pages;
        alloc.free_mem(addr, 16);
        assert_eq!(alloc.free_pages, initial_free + 16);
    }
}
