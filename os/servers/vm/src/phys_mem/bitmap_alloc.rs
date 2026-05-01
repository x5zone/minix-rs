use alloc::vec;
use alloc::vec::Vec;
use super::alloc_trait::PhysMemAlloc;
use super::stats::MemStats;
use super::types::{AllocError, AllocParams, PageAllocFlags, PhysAddr};
use super::{clicks_to_bytes, CLICK_SIZE, BootMemRegion};

const BITS_PER_CHUNK: usize = 64;
const PAGE_CACHE_MAX: usize = 10000;

pub(crate) struct BitmapAllocator {
    bitmap: Vec<u64>,
    total_pages: usize,
    free_pages: usize,
    page_cache: Vec<usize>,
    last_scan: Option<usize>,
    stats: MemStats,
    mem_low: usize,
    mem_high: usize,
}

impl BitmapAllocator {
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

        let max_page = mem_high / CLICK_SIZE + 1;
        let bitmap_chunks = (max_page + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK;

        let mut alloc = Self {
            bitmap: vec![0u64; bitmap_chunks],
            total_pages: 0,
            free_pages: 0,
            page_cache: Vec::with_capacity(PAGE_CACHE_MAX),
            last_scan: None,
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
            alloc.free_pages_internal(base_page, num_pages);
            alloc.total_pages += num_pages;
        }

        alloc
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

    pub(crate) fn memstats(&self) -> (usize, usize, usize) {
        let mut nodes = 0;
        let mut pages = 0;
        let mut largest = 0;
        let mut i = 0;
        let total = self.bitmap_len();

        while i < total {
            let mut size = 0;
            while i < total && self.page_is_free(i) {
                size += 1;
                i += 1;
            }
            if size == 0 {
                i += 1;
                continue;
            }
            nodes += 1;
            pages += size;
            if size > largest {
                largest = size;
            }
        }

        (nodes, pages, largest)
    }

    fn alloc_pages(&mut self, pages: usize, flags: PageAllocFlags) -> Option<usize> {
        let max_page = self.max_page_for_flags(flags);

        if !flags.intersects(PageAllocFlags::LOWER16MB | PageAllocFlags::LOWER1MB) {
            if pages == 1 {
                while !self.page_cache.is_empty() {
                    if let Some(idx) = self.page_cache.pop() {
                        if idx < self.bitmap_len() && self.page_is_free(idx) {
                            self.mark_allocated(idx, 1);
                            return Some(idx);
                        }
                    }
                }
            }
        }

        let start_scan = self.last_scan
            .filter(|&s| s < max_page)
            .unwrap_or(max_page);

        if let Some(mem) = self.find_bit(0, start_scan, pages) {
            self.last_scan = Some(mem);
            self.mark_allocated(mem, pages);
            return Some(mem);
        }

        if let Some(mem) = self.find_bit(0, max_page, pages) {
            self.last_scan = Some(mem);
            self.mark_allocated(mem, pages);
            return Some(mem);
        }

        None
    }

    fn find_bit(&self, low: usize, start_scan: usize, pages: usize) -> Option<usize> {
        let mut run_length = 0;
        let mut free_start = 0usize;
        let mut i = start_scan;

        loop {
            if !self.page_is_free(i) {
                run_length = 0;

                let chunk_idx = i / BITS_PER_CHUNK;
                if chunk_idx > 0 && self.bitmap[chunk_idx] == 0 {
                    let mut skip_to = chunk_idx;
                    while skip_to > 0 && self.bitmap[skip_to] == 0 {
                        skip_to -= 1;
                    }
                    if self.bitmap[skip_to] == 0 {
                        break;
                    }
                    i = skip_to * BITS_PER_CHUNK + BITS_PER_CHUNK - 1;
                    if i < low {
                        break;
                    }
                    continue;
                }

                if i == low {
                    break;
                }
                i -= 1;
                continue;
            }

            if run_length == 0 {
                free_start = i;
                run_length = 1;
            } else {
                free_start = i;
                run_length += 1;
            }

            if run_length == pages {
                return Some(free_start);
            }

            if i == low {
                break;
            }
            i -= 1;
        }

        None
    }

    fn free_pages_internal(&mut self, start_page: usize, num_pages: usize) {
        for i in start_page..start_page + num_pages {
            if i >= self.bitmap_len() {
                break;
            }
            let chunk = i / BITS_PER_CHUNK;
            let bit = i % BITS_PER_CHUNK;
            self.bitmap[chunk] |= 1u64 << bit;

            if self.page_cache.len() < PAGE_CACHE_MAX {
                self.page_cache.push(i);
            }
        }
        self.free_pages += num_pages;
    }



    fn mark_allocated(&mut self, start_page: usize, num_pages: usize) {
        for i in start_page..start_page + num_pages {
            let chunk = i / BITS_PER_CHUNK;
            let bit = i % BITS_PER_CHUNK;
            self.bitmap[chunk] &= !(1u64 << bit);
        }
        self.free_pages -= num_pages;
    }

    pub(crate) fn page_is_free(&self, page: usize) -> bool {
        if page >= self.bitmap_len() {
            return false;
        }
        let chunk = page / BITS_PER_CHUNK;
        let bit = page % BITS_PER_CHUNK;
        (self.bitmap[chunk] >> bit) & 1 == 1
    }

    fn bitmap_len(&self) -> usize {
        self.bitmap.len() * BITS_PER_CHUNK
    }

    fn max_page_for_flags(&self, flags: PageAllocFlags) -> usize {
        if flags.contains(PageAllocFlags::LOWER1MB) {
            (1024 * 1024) / CLICK_SIZE - 1
        } else if flags.contains(PageAllocFlags::LOWER16MB) {
            (16 * 1024 * 1024) / CLICK_SIZE - 1
        } else {
            self.bitmap_len().saturating_sub(1)
        }
    }

    fn cache_freepages(&mut self, _needed: usize) -> Option<usize> {
        // TODO(stage-3): 实现 cache_freepages，从 ReservedQueue 回收预留页。
        // Minix3 的 cache_freepages 从 reservedqueues 中回收页，当 alloc_pages
        // 失败时调用。需要先实现 ReservedQueue（对应 Minix3 的 reservedqueues）。
        None
    }
}

impl PhysMemAlloc for BitmapAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysAddr, AllocError> {
        if clicks == 0 {
            self.stats.record_failure();
            return Err(AllocError::OutOfMemory);
        }

        let params = AllocParams::compute(clicks, flags, self.total_pages);

        let mut mem = self.alloc_pages(params.alloc_clicks, flags);

        if mem.is_none() {
            if let Some(freed) = self.cache_freepages(params.alloc_clicks) {
                if freed > 0 {
                    mem = self.alloc_pages(params.alloc_clicks, flags);
                }
            }
        }

        let mem = mem.ok_or_else(|| {
            self.stats.record_failure();
            params.error_type()
        })?;

        let (result_page, leading) = params.aligned_result(mem);

        if leading > 0 {
            self.free_pages_internal(mem, leading);
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
        self.free_pages_internal(start_page, clicks);
        self.stats.record_free(clicks_to_bytes(clicks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_regions() -> Vec<BootMemRegion> {
        vec![
            BootMemRegion { base: 0x100000, size: 128 * 1024 * 1024 },
        ]
    }

    #[test]
    fn test_phys_addr_from_page_index() {
        let addr = PhysAddr::from_page_index(5);
        assert_eq!(addr.as_u64(), 5 * CLICK_SIZE as u64);
        assert_eq!(addr.page_index(), 5);
    }

    #[test]
    fn test_alloc_free_basic() {
        let mut alloc = BitmapAllocator::init(&make_test_regions());

        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        assert_eq!(addr.page_index() % 1, 0);

        alloc.free_mem(addr, 4);
    }

    #[test]
    fn test_alloc_zero_clicks() {
        let mut alloc = BitmapAllocator::init(&make_test_regions());
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn test_alloc_exhaustion() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let mut alloc = BitmapAllocator::init(&regions);

        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());

        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());

        alloc.free_mem(a.unwrap(), 4);

        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn test_free_and_realloc() {
        let regions = vec![BootMemRegion { base: 0, size: 20 * CLICK_SIZE }];
        let mut alloc = BitmapAllocator::init(&regions);

        let a = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 10);

        let c = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());

        alloc.free_mem(b, 10);
        alloc.free_mem(c, 10);
    }

    #[test]
    fn test_contiguous_allocation() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = BitmapAllocator::init(&regions);

        let a = alloc.alloc_mem(10, PageAllocFlags::CONTIG).unwrap();
        let start = a.page_index();
        for i in 0..10 {
            assert!(!alloc.page_is_free(start + i));
        }

        alloc.free_mem(a, 10);
    }

    #[test]
    fn test_lower16mb_flag() {
        let regions = vec![BootMemRegion { base: 0, size: 32 * 1024 * 1024 }];
        let mut alloc = BitmapAllocator::init(&regions);

        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);

        alloc.free_mem(addr, 1);
    }

    #[test]
    fn test_align64k_flag() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * 1024 * 1024 }];
        let mut alloc = BitmapAllocator::init(&regions);

        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);

        alloc.free_mem(addr, 1);
    }

    #[test]
    fn test_memstats() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = BitmapAllocator::init(&regions);

        let (nodes, pages, largest) = alloc.memstats();
        assert_eq!(nodes, 1);
        assert_eq!(pages, 100);
        assert_eq!(largest, 100);

        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        let (nodes, pages, largest) = alloc.memstats();
        assert!(pages < 100);

        alloc.free_mem(a, 50);
    }

    #[test]
    fn test_multiple_regions() {
        let regions = vec![
            BootMemRegion { base: 0x100000, size: 4 * 1024 * 1024 },
            BootMemRegion { base: 0x10000000, size: 8 * 1024 * 1024 },
        ];
        let mut alloc = BitmapAllocator::init(&regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn test_memory_pressure() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = BitmapAllocator::init(&regions);

        assert!(!alloc.is_under_pressure());

        let _a = alloc.alloc_mem(91, PageAllocFlags::empty()).unwrap();
        assert!(alloc.is_under_pressure());
    }

    #[test]
    fn test_page_cache_single_page() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = BitmapAllocator::init(&regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 1);

        let b = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        assert_eq!(b.page_index(), a.page_index());
    }
}
