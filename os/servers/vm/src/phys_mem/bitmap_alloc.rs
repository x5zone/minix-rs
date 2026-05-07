//! Bitmap-based physical memory allocator.
//!
//! This allocator uses a bitmap to track free/used pages, matching Minix3's
//! original implementation. Each bit represents one page (1 = free, 0 = used).

use super::alloc_trait::{PhysAllocator, PhysAllocatorStats, PhysMemStats};
use super::stats::MemStats;
use super::types::{AllocError, PageAllocFlags, PhysBytes};
use super::{CLICK_SIZE, BootMemRegion, EarlyHeap};

const BITS_PER_CHUNK: usize = 64;
const PAGE_CACHE_MAX: usize = 10000;

pub struct BitmapAllocator {
    bitmap: &'static mut [u64],
    total_pages: usize,
    free_pages: usize,
    page_cache: &'static mut [usize],
    page_cache_size: usize,
    stats: MemStats,
}

impl BitmapAllocator {
    pub fn init(early_heap: &mut EarlyHeap, regions: &[BootMemRegion]) -> Self {
        let (total_pages, _mem_low, _mem_high) = super::compute_memory_bounds(regions);

        let bitmap_chunks = (total_pages + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK;
        let bitmap = early_heap.alloc_slice::<u64>(bitmap_chunks);

        for chunk in bitmap.iter_mut() {
            *chunk = 0;
        }

        let page_cache = early_heap.alloc_slice::<usize>(PAGE_CACHE_MAX);

        let mut alloc = Self {
            bitmap,
            total_pages,
            free_pages: 0,
            page_cache,
            page_cache_size: 0,
            stats: MemStats::new(),
        };

        for region in regions {
            if region.size == 0 {
                continue;
            }
            region.validate();
            let base_page = region.base / CLICK_SIZE;
            let num_pages = region.size / CLICK_SIZE;
            alloc.free_pages_internal(base_page, num_pages);
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

    fn memstats_internal(&self) -> (usize, usize, usize) {
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

    fn alloc_pages(&mut self, pages: usize, max_page: usize, use_cache: bool) -> Option<usize> {
        if pages == 0 {
            return None;
        }

        if use_cache && pages == 1 {
            while self.page_cache_size > 0 {
                self.page_cache_size -= 1;
                let idx = self.page_cache[self.page_cache_size];
                if idx < self.bitmap_len() && self.page_is_free(idx) {
                    self.mark_allocated(idx, 1);
                    return Some(idx);
                }
            }
        }

        let start = max_page.saturating_sub(1).min(self.bitmap_len().saturating_sub(1));
        if let Some(mem) = self.find_bit(0, start, pages) {
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

                // Chunk skip optimization: if the current 64-bit chunk is entirely
                // allocated (all zeros), scan backwards to find the nearest non-empty
                // chunk. This avoids checking each page individually in fully-allocated
                // regions, which is common under memory pressure.
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
            if self.page_cache_size < PAGE_CACHE_MAX {
                self.page_cache[self.page_cache_size] = i;
                self.page_cache_size += 1;
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

    fn cache_freepages(&mut self, _needed: usize) -> usize {
        // TODO: In Minix3, cache_freepages() evicts pages from the VM file cache
        // (LRU list in cache.c) to free physical memory. This requires the VM
        // page cache subsystem which is not yet implemented. Returns 0 for now.
        0
    }
}

impl PhysAllocator for BitmapAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
        if clicks == 0 {
            self.stats.record_failure();
            return Err(AllocError::OutOfMemory);
        }

        let mut alloc_clicks = clicks;
        let mut align_clicks = 0usize;

        if flags.contains(PageAllocFlags::ALIGN64K) {
            align_clicks = (64 * 1024) / CLICK_SIZE;
            alloc_clicks += align_clicks;
        } else if flags.contains(PageAllocFlags::ALIGN16K) {
            align_clicks = (16 * 1024) / CLICK_SIZE;
            alloc_clicks += align_clicks;
        }

        let max_page = if flags.contains(PageAllocFlags::LOWER1MB) {
            (1 * 1024 * 1024) / CLICK_SIZE
        } else if flags.contains(PageAllocFlags::LOWER16MB) {
            (16 * 1024 * 1024) / CLICK_SIZE
        } else {
            self.total_pages
        };

        let use_cache = !super::is_low_mem_flag(flags);

        let mut page;
        loop {
            page = self.alloc_pages(alloc_clicks, max_page, use_cache);
            if page.is_some() {
                break;
            }
            let freed = self.cache_freepages(alloc_clicks);
            if freed == 0 {
                break;
            }
        }

        let page = match page {
            Some(p) => p,
            None => {
                self.stats.record_failure();
                return Err(super::oom_error(flags));
            }
        };

        if align_clicks > 0 {
            let offset = page % align_clicks;
            if offset > 0 {
                let excess = align_clicks - offset;
                self.free_pages_internal(page, excess);
                let aligned_page = page + excess;
                self.stats.record_alloc(clicks * CLICK_SIZE);
                return Ok(PhysBytes::from_page_index(aligned_page));
            }
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
        self.free_pages_internal(start_page, clicks);
        self.stats.record_free(clicks * CLICK_SIZE);
    }

    fn total_count(&self) -> usize {
        self.total_pages
    }

    fn reloc_array_count(&self) -> usize {
        2
    }

    fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
        match index {
            0 => (self.bitmap.as_ptr() as *const u8, self.bitmap.len(), core::mem::size_of::<u64>()),
            1 => (self.page_cache.as_ptr() as *const u8, self.page_cache.len(), core::mem::size_of::<usize>()),
            _ => (core::ptr::null(), 0, 0),
        }
    }

    fn update_relocated_arrays(&mut self, new_ptrs: &[*mut u8]) {
        unsafe {
            self.bitmap = core::slice::from_raw_parts_mut(
                new_ptrs[0] as *mut u64,
                self.bitmap.len(),
            );
            self.page_cache = core::slice::from_raw_parts_mut(
                new_ptrs[1] as *mut usize,
                self.page_cache.len(),
            );
        }
    }
}

impl PhysAllocatorStats for BitmapAllocator {
    fn memstats(&self) -> PhysMemStats {
        let (free_nodes, free_pages, largest_free) = self.memstats_internal();
        PhysMemStats { free_nodes, free_pages, largest_free }
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

    fn make_test_heap(regions: &[BootMemRegion]) -> (&'static mut [u8], EarlyHeap) {
        let (_, _, mem_high) = super::super::compute_memory_bounds(regions);
        let total_pages = mem_high / CLICK_SIZE + 1;
        let bitmap_chunks = (total_pages + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK;
        let heap_size = bitmap_chunks * 8 + PAGE_CACHE_MAX * core::mem::size_of::<usize>() + 1024;

        let buffer: &'static mut [u8] = {
            let boxed = alloc::boxed::Box::new([0u8; 1024 * 1024]);
            alloc::boxed::Box::leak(boxed)
        };

        let mut heap = EarlyHeap::new();
        heap.init(buffer.as_mut_ptr(), heap_size);
        (buffer, heap)
    }

    #[test]
    fn test_phys_addr_from_page_index() {
        let addr = PhysBytes::from_page_index(5);
        assert_eq!(addr.as_u64(), 5 * CLICK_SIZE as u64);
        assert_eq!(addr.page_index(), 5);
    }

    #[test]
    fn test_alloc_free_basic() {
        let regions = make_test_regions();
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        assert_eq!(addr.page_index() % 1, 0);

        alloc.free_mem(addr, 4);
    }

    #[test]
    fn test_alloc_zero_pages() {
        let regions = make_test_regions();
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn test_alloc_exhaustion() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

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
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 10);

        let c = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());

        alloc.free_mem(b, 10);
        alloc.free_mem(c, 10);
    }

    #[test]
    fn test_memstats() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let stats = alloc.memstats();
        assert_eq!(stats.free_nodes, 1);
        assert_eq!(stats.free_pages, 100);
        assert_eq!(stats.largest_free, 100);

        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        let stats = alloc.memstats();
        assert!(stats.free_pages < 100);

        alloc.free_mem(a, 50);
    }

    #[test]
    fn test_multiple_regions() {
        let regions = vec![
            BootMemRegion { base: 0x100000, size: 4 * 1024 * 1024 },
            BootMemRegion { base: 0x10000000, size: 8 * 1024 * 1024 },
        ];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn test_memory_pressure() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        assert!(!alloc.is_under_pressure());

        let _a = alloc.alloc_mem(91, PageAllocFlags::empty()).unwrap();
        assert!(alloc.is_under_pressure());
    }

    #[test]
    fn test_page_cache_single_page() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 1);
        alloc.free_mem(b, 1);

        let c = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), b.page_index());

        let d = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        assert_eq!(d.page_index(), a.page_index());
    }

    #[test]
    fn test_page_cache_not_used_with_lower_flags() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 1);

        assert!(alloc.page_cache_size > 0);

        let b = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB);
        assert!(b.is_ok());
    }

    #[test]
    fn test_page_cache_stale_entry_skipped() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        let c = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        let d = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 1);
        alloc.free_mem(b, 1);
        alloc.free_mem(c, 1);
        alloc.free_mem(d, 1);

        assert_eq!(alloc.page_cache_size, 4);

        let _e = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();

        let f = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(f.is_err());
    }

    #[test]
    fn test_low_mem_exhausted_error() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let _a = alloc.alloc_mem(100, PageAllocFlags::empty()).unwrap();

        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);

        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);
    }

    #[test]
    fn test_oom_error_type() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);

        let _a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();

        let err = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap_err();
        assert_eq!(err, AllocError::OutOfMemory);
    }

    #[test]
    fn test_cache_freepages_returns_zero() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let (_, mut heap) = make_test_heap(&regions);
        let mut alloc = BitmapAllocator::init(&mut heap, &regions);
        assert_eq!(alloc.cache_freepages(1), 0);
        assert_eq!(alloc.cache_freepages(100), 0);
    }
}
