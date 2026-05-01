use alloc::vec;
use alloc::vec::Vec;
use super::alloc_trait::PhysMemAlloc;
use super::stats::MemStats;
use super::types::{AllocError, AllocParams, PageAllocFlags, PhysAddr};
use super::{clicks_to_bytes, CLICK_SIZE, BootMemRegion};

const MAX_ORDER: usize = 30;

pub(crate) struct BuddyAllocator {
    total_pages: usize,
    free_pages: usize,
    max_order: usize,
    free_lists: Vec<Vec<usize>>,
    // page_order[i] 记录页 i 所属块的 order。仅对空闲块的首页有保证；
    // 已分配块的非首页可能残留旧值。try_merge 依赖此字段判断 buddy 是否可合并，
    // 因此只读取 buddy 页的 page_order（buddy 必须是空闲块首页才有正确值）。
    page_order: Vec<u8>,
    page_free: Vec<bool>,
    stats: MemStats,
    mem_low: usize,
    mem_high: usize,
}

impl BuddyAllocator {
    pub(crate) fn init(regions: &[BootMemRegion]) -> Self {
        let mut mem_low = usize::MAX;
        let mut mem_high = 0;

        for region in regions {
            if region.size == 0 {
                continue;
            }
            region.validate();
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
        let max_order = compute_max_order(total_pages);

        let free_lists: Vec<Vec<usize>> = (0..=max_order).map(|_| Vec::new()).collect();
        let page_order = vec![0u8; total_pages];
        let page_free = vec![false; total_pages];

        let mut alloc = Self {
            total_pages,
            free_pages: 0,
            max_order,
            free_lists,
            page_order,
            page_free,
            stats: MemStats::new(),
            mem_low,
            mem_high,
        };

        for region in regions {
            if region.size == 0 {
                continue;
            }
            let base_page = region.base / CLICK_SIZE;
            let num_pages = region.size / CLICK_SIZE;
            alloc.add_free_region(base_page, num_pages);
        }

        alloc
    }

    fn add_free_region(&mut self, start: usize, count: usize) {
        let mut pos = start;
        let end = start + count;

        while pos < end {
            let remaining = end - pos;
            let max_align_order = pos.trailing_zeros() as usize;
            let max_size_order = remaining.next_power_of_two().trailing_zeros() as usize;
            let order = max_align_order.min(max_size_order).min(self.max_order);
            let block_size = 1usize << order;

            if pos + block_size > end {
                let smaller = (end - pos).trailing_zeros() as usize;
                let smaller = smaller.min(order);
                if smaller == 0 {
                    self.page_free[pos] = true;
                    self.page_order[pos] = 0;
                    self.free_lists[0].push(pos);
                    self.free_pages += 1;
                    pos += 1;
                    continue;
                }
                let smaller_size = 1usize << smaller;
                self.free_block_internal(pos, smaller);
                pos += smaller_size;
                continue;
            }

            self.free_block_internal(pos, order);
            pos += block_size;
        }
    }

    fn free_block(&mut self, page: usize, order: usize) {
        let block_size = 1usize << order;
        for i in page..page + block_size {
            if i < self.total_pages {
                self.page_free[i] = true;
                self.page_order[i] = order as u8;
            }
        }
        self.free_lists[order].push(page);
        self.free_pages += block_size;
    }

    fn alloc_block(&mut self, order: usize) -> Option<usize> {
        if order > self.max_order {
            return None;
        }

        if !self.free_lists[order].is_empty() {
            let page = self.free_lists[order].pop().unwrap();
            let block_size = 1usize << order;
            for i in page..page + block_size {
                if i < self.total_pages {
                    self.page_free[i] = false;
                }
            }
            self.free_pages -= block_size;
            return Some(page);
        }

        if order < self.max_order {
            if let Some(block) = self.alloc_block(order + 1) {
                let buddy = block + (1usize << order);
                self.free_block(buddy, order);
                self.page_order[block] = order as u8;
                return Some(block);
            }
        }

        None
    }

    fn buddy_of(&self, page: usize, order: usize) -> usize {
        page ^ (1usize << order)
    }

    fn free_block_internal(&mut self, page: usize, order: usize) {
        let block_size = 1usize << order;
        for i in page..page + block_size {
            if i < self.total_pages {
                self.page_free[i] = true;
            }
        }
        self.page_order[page] = order as u8;
        self.free_pages += block_size;
        self.free_lists[order].push(page);
        self.try_merge(page, order);
    }

    fn try_merge(&mut self, page: usize, mut order: usize) {
        let mut page = page;
        while order < self.max_order {
            let buddy = self.buddy_of(page, order);

            if buddy >= self.total_pages {
                break;
            }

            if self.page_order[buddy] as usize != order {
                break;
            }

            let buddy_idx = self.free_lists[order].iter().position(|&p| p == buddy);
            if buddy_idx.is_none() {
                break;
            }
            let buddy_idx = buddy_idx.unwrap();
            self.free_lists[order].remove(buddy_idx);

            let merged = page.min(buddy);
            order += 1;
            self.page_order[merged] = order as u8;
            self.free_lists[order].push(merged);

            page = merged;
        }
    }

    fn order_for_pages(pages: usize) -> usize {
        if pages == 0 {
            return 0;
        }
        (pages - 1).next_power_of_two().trailing_zeros() as usize
    }

    pub(crate) fn stats(&self) -> &MemStats {
        &self.stats
    }

    pub(crate) fn total_memory(&self) -> usize {
        self.total_pages * CLICK_SIZE
    }

    pub(crate) fn free_memory(&self) -> usize {
        self.free_pages * CLICK_SIZE
    }

    pub(crate) fn is_under_pressure(&self) -> bool {
        self.free_pages * 10 < self.total_pages
    }

    pub(crate) fn largest_free(&self) -> usize {
        for order in (0..=self.max_order).rev() {
            if !self.free_lists[order].is_empty() {
                return 1usize << order;
            }
        }
        0
    }
}

impl PhysMemAlloc for BuddyAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysAddr, AllocError> {
        if clicks == 0 {
            self.stats.record_failure();
            return Err(AllocError::OutOfMemory);
        }

        let params = AllocParams::compute(clicks, flags, self.total_pages);

        let order = Self::order_for_pages(params.alloc_clicks);
        let block_size = 1usize << order;

        let page = self.alloc_block(order).ok_or_else(|| {
            self.stats.record_failure();
            params.error_type()
        })?;

        if page + block_size > params.max_page {
            self.free_block_internal(page, order);
            self.stats.record_failure();
            return Err(AllocError::LowMemoryExhausted);
        }

        let (result_page, leading) = params.aligned_result(page);

        if leading > 0 {
            self.add_free_region(page, leading);
        }

        let trailing_start = result_page + params.clicks;
        let trailing_count = (page + block_size).saturating_sub(trailing_start);
        if trailing_count > 0 {
            self.add_free_region(trailing_start, trailing_count);
        }

        // TODO(stage-3): 处理 PAF_CLEAR flag。Minix3 在 alloc_pages 末尾调用
        // sys_memset(NONE, 0, CLICK_SIZE*mem, VM_PAGE_SIZE*pages) 清零分配的页。
        // 当前阶段缺少 kernel IPC 基础设施，无法调用 sys_memset。
        // if flags.contains(PageAllocFlags::CLEAR) { ... }

        self.stats.record_alloc(clicks_to_bytes(params.clicks));
        Ok(PhysAddr::from_page_index(result_page))
    }

    fn free_mem(&mut self, base: PhysAddr, clicks: usize) {
        if clicks == 0 {
            return;
        }
        let start_page = base.page_index();
        self.add_free_region(start_page, clicks);
        self.stats.record_free(clicks_to_bytes(clicks));
    }
}

fn compute_max_order(total_pages: usize) -> usize {
    if total_pages == 0 {
        return 0;
    }
    let max_order = total_pages.next_power_of_two().trailing_zeros() as usize;
    max_order.min(MAX_ORDER)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_regions() -> Vec<BootMemRegion> {
        vec![BootMemRegion { base: 0, size: 128 * 1024 * 1024 }]
    }

    #[test]
    fn test_alloc_free_basic() {
        let mut alloc = BuddyAllocator::init(&make_test_regions());

        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn test_alloc_zero_clicks() {
        let mut alloc = BuddyAllocator::init(&make_test_regions());
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn test_alloc_exhaustion() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

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
        let mut alloc = BuddyAllocator::init(&regions);

        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 128);
        alloc.free_mem(b, 128);

        let c = alloc.alloc_mem(256, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn test_lower16mb_flag() {
        let regions = vec![BootMemRegion { base: 0, size: 32 * 1024 * 1024 }];
        let mut alloc = BuddyAllocator::init(&regions);

        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);

        alloc.free_mem(addr, 1);
    }

    #[test]
    fn test_largest_free() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

        assert!(alloc.largest_free() >= 256);

        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 128);
    }

    #[test]
    fn test_internal_fragmentation() {
        let regions = vec![BootMemRegion { base: 0, size: 16 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

        let a = alloc.alloc_mem(3, PageAllocFlags::empty()).unwrap();
        let start = a.page_index();

        assert!(start % 4 == 0);

        alloc.free_mem(a, 3);
    }

    #[test]
    fn test_multiple_allocations() {
        let regions = vec![BootMemRegion { base: 0, size: 64 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

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
        let mut alloc = BuddyAllocator::init(&regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn test_single_page() {
        let regions = vec![BootMemRegion { base: 0, size: CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

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
        let mut alloc = BuddyAllocator::init(&regions);

        let a = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(128, PageAllocFlags::empty()).unwrap();

        let page_a = a.page_index();
        let page_b = b.page_index();

        alloc.free_mem(a, 128);
        alloc.free_mem(b, 128);

        assert_eq!(alloc.largest_free(), 256);

        let c = alloc.alloc_mem(256, PageAllocFlags::empty());
        assert!(c.is_ok(), "should be able to re-allocate full 256 pages after merge chain");

        alloc.free_mem(c.unwrap(), 256);
    }

    #[test]
    fn test_merge_chain_high_to_low() {
        let regions = vec![BootMemRegion { base: 0, size: 16 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

        let a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        let c = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();

        let page_a = a.page_index();
        let page_b = b.page_index();
        let page_c = c.page_index();

        alloc.free_mem(b, 4);
        alloc.free_mem(a, 4);

        assert_eq!(alloc.largest_free(), 8);

        let d = alloc.alloc_mem(8, PageAllocFlags::empty());
        assert!(d.is_ok(), "should be able to allocate 8 pages after freeing a+b and merge chain");

        alloc.free_mem(c, 4);
        alloc.free_mem(d.unwrap(), 8);
    }

    #[test]
    fn test_excess_pages_reused() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        let page_a = a.page_index();
        assert!(page_a % 16 == 0, "should be 64K aligned");

        let free_before = alloc.free_memory();
        let total = alloc.total_memory();

        let mut addrs = Vec::new();
        while let Ok(addr) = alloc.alloc_mem(1, PageAllocFlags::empty()) {
            addrs.push(addr);
        }

        assert!(addrs.len() > 0, "should be able to allocate pages from excess region");
        assert_eq!(alloc.free_memory(), 0, "all memory should be allocated");

        for addr in addrs {
            alloc.free_mem(addr, 1);
        }
        alloc.free_mem(a, 1);
    }

    #[test]
    fn test_aligned_alloc_free_no_leak() {
        let regions = vec![BootMemRegion { base: 0, size: 256 * CLICK_SIZE }];
        let mut alloc = BuddyAllocator::init(&regions);
        let total = alloc.total_memory();
        let free_before = alloc.free_memory();

        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        alloc.free_mem(addr, 1);

        let free_after = alloc.free_memory();
        assert_eq!(free_before, free_after, "aligned alloc+free should not leak: before={}, after={}", free_before, free_after);

        let addr = alloc.alloc_mem(3, PageAllocFlags::ALIGN16K).unwrap();
        alloc.free_mem(addr, 3);

        let free_after = alloc.free_memory();
        assert_eq!(free_before, free_after, "aligned alloc+free (3 pages, ALIGN16K) should not leak");
    }
}
