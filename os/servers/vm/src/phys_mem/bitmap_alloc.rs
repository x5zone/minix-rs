use super::alloc_trait::{PhysAllocator, PhysMemStats};
use super::stats::MemStats;
use super::types::{AllocError, PageAllocFlags, AlignedPhysBytes};
use super::{BumpBuf, CLICK_SIZE, BootMemRegion, METADATA_ALIGN_PADDING};

const BITS_PER_CHUNK: usize = 64;
/// Maximum number of entries in the single-page free cache.
const PAGE_CACHE_MAX: usize = 10000;

pub struct BitmapAllocator {
    bitmap: &'static mut [u64],
    total_pages: usize,
    free_pages: usize,
    page_cache: &'static mut [usize],
    page_cache_size: usize,
    stats: MemStats,
    meta_phys_base: u64,
    meta_pages: usize,
}

impl BitmapAllocator {
    pub fn init(metadata: &mut [u8], total_pages: usize, free_regions: &[BootMemRegion], meta_phys_base: u64, meta_pages: usize) -> Self {
        let bitmap_chunks = (total_pages + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK;
        let mut buf = BumpBuf::new(metadata);

        let bitmap = buf.alloc_slice::<u64>(bitmap_chunks);
        for chunk in bitmap.iter_mut() {
            *chunk = 0;
        }

        let page_cache = buf.alloc_slice::<usize>(PAGE_CACHE_MAX);

        let mut alloc = Self {
            bitmap,
            total_pages,
            free_pages: 0,
            page_cache,
            page_cache_size: 0,
            stats: MemStats::new(),
            meta_phys_base,
            meta_pages,
        };

        for region in free_regions {
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

    pub fn metadata_size(total_pages: usize) -> usize {
        let bitmap_chunks = (total_pages + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK;
        let bitmap_bytes = bitmap_chunks * core::mem::size_of::<u64>();
        let cache_bytes = PAGE_CACHE_MAX * core::mem::size_of::<usize>();
        bitmap_bytes + cache_bytes + METADATA_ALIGN_PADDING
    }

    pub fn total_memory(&self) -> usize {
        self.total_pages * CLICK_SIZE
    }

    pub fn free_memory(&self) -> usize {
        self.free_pages * CLICK_SIZE
    }

    #[cfg(test)]
    pub fn new_for_test(total_pages: usize) -> Self {
        let base = 0;
        let size = total_pages * CLICK_SIZE;
        let regions = [BootMemRegion { base, size }];
        let meta_size = Self::metadata_size(total_pages);
        let v: alloc::vec::Vec<u8> = alloc::vec![0u8; meta_size.max(1024 * 1024)];
        let metadata = alloc::boxed::Box::leak(v.into_boxed_slice());
        Self::init(&mut metadata[..meta_size], total_pages, &regions, 0, 0)
    }

    pub fn metadata_pa_range(&self) -> (u64, usize) {
        (self.meta_phys_base, self.meta_pages)
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
            debug_assert!(!self.page_is_free(i), "double free at page {i}");
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
        debug_assert!(self.free_pages >= num_pages, "mark_allocated underflow: free={}, allocating={}", self.free_pages, num_pages);
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
        0
    }
}

impl PhysAllocator for BitmapAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<AlignedPhysBytes, AllocError> {
        if clicks == 0 {
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

        // Handle alignment: we over-allocated to guarantee an aligned boundary
        // exists within the block. Free the unused prefix or suffix.
        if align_clicks > 0 {
            let offset = page % align_clicks;
            if offset > 0 {
                // Page is not aligned; free the excess prefix before the aligned boundary.
                let excess = align_clicks - offset;
                self.free_pages_internal(page, excess);
                let aligned_page = page + excess;
                self.stats.record_alloc(clicks * CLICK_SIZE);
                return Ok(AlignedPhysBytes::from_page_index(aligned_page));
            } else {
                // Page is already aligned; free the unused suffix after the requested clicks.
                self.free_pages_internal(page + clicks, align_clicks);
            }
        }

        if flags.contains(PageAllocFlags::CLEAR) {
            let virt = crate::direct_map::vm_phys_to_virt(AlignedPhysBytes::from_page_index(page));
            unsafe {
                let ptr = virt.0 as *mut u64;
                let words = clicks * CLICK_SIZE / 8;
                for i in 0..words {
                    core::ptr::write_volatile(ptr.add(i), 0);
                }
            }
        }

        self.stats.record_alloc(clicks * CLICK_SIZE);
        Ok(AlignedPhysBytes::from_page_index(page))
    }

    fn free_mem(&mut self, base: AlignedPhysBytes, clicks: usize) {
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

    fn reserve_pages(&mut self, base_page: usize, count: usize) {
        let mut reserved = 0usize;
        for i in base_page..base_page + count {
            if i >= self.bitmap_len() {
                break;
            }
            if self.page_is_free(i) {
                let chunk = i / BITS_PER_CHUNK;
                let bit = i % BITS_PER_CHUNK;
                self.bitmap[chunk] &= !(1u64 << bit);
                self.free_pages -= 1;
                reserved += 1;
            }
        }
        if reserved > 0 {
            self.stats.record_alloc(reserved * CLICK_SIZE);
        }
    }

    fn available_regions(&self, callback: &mut dyn FnMut(usize, usize)) {
        let mut i = 0;
        let total = self.bitmap_len();
        while i < total {
            if !self.page_is_free(i) {
                i += 1;
                continue;
            }
            let start = i;
            while i < total && self.page_is_free(i) {
                i += 1;
            }
            callback(start, i - start);
        }
    }
}

impl BitmapAllocator {
    pub fn memstats(&self) -> PhysMemStats {
        let (free_nodes, free_pages, largest_free) = self.memstats_internal();
        PhysMemStats { free_nodes, free_pages, largest_free }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::PhysAlloc;

    fn make_test_regions() -> Vec<BootMemRegion> {
        vec![
            BootMemRegion { base: 0x100000, size: 128 * 1024 * 1024 },
        ]
    }

    fn make_test_metadata(total_pages: usize) -> &'static mut [u8] {
        let size = BitmapAllocator::metadata_size(total_pages);
        let v: alloc::vec::Vec<u8> = alloc::vec![0u8; 1024 * 1024];
        let buf = alloc::boxed::Box::leak(v.into_boxed_slice());
        &mut buf[..size]
    }

    fn total_pages_from_regions(regions: &[BootMemRegion]) -> usize {
        let (tp, _, _) = super::super::compute_memory_bounds(regions);
        tp
    }

    #[test]
    fn test_phys_addr_from_page_index() {
        let addr = AlignedPhysBytes::from_page_index(5);
        assert_eq!(addr.as_u64(), 5 * CLICK_SIZE as u64);
        assert_eq!(addr.page_index(), 5);
    }

    #[test]
    fn test_alloc_free_basic() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        assert_eq!(addr.page_index() % 1, 0);

        alloc.free_mem(addr, 4);
    }

    #[test]
    fn test_alloc_zero_pages() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn test_alloc_exhaustion() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

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
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

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
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

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
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn test_memory_pressure() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        assert!(!alloc.is_under_pressure());

        let _a = alloc.alloc_mem(91, PageAllocFlags::empty()).unwrap();
        assert!(alloc.is_under_pressure());
    }

    #[test]
    fn test_page_cache_single_page() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

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
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(a, 1);

        assert!(alloc.page_cache_size > 0);

        let b = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB);
        assert!(b.is_ok());
    }

    #[test]
    fn test_page_cache_stale_entry_skipped() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

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
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        let _a = alloc.alloc_mem(100, PageAllocFlags::empty()).unwrap();

        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);

        let err = alloc.alloc_mem(1, PageAllocFlags::LOWER1MB).unwrap_err();
        assert_eq!(err, AllocError::LowMemoryExhausted);
    }

    #[test]
    fn test_oom_error_type() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        let _a = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();

        let err = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap_err();
        assert_eq!(err, AllocError::OutOfMemory);
    }

    #[test]
    fn test_cache_freepages_returns_zero() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);
        assert_eq!(alloc.cache_freepages(1), 0);
        assert_eq!(alloc.cache_freepages(100), 0);
    }

    #[test]
    fn test_reserve_pages() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        assert_eq!(alloc.free_pages, 100);

        alloc.reserve_pages(10, 5);
        assert_eq!(alloc.free_pages, 95);
        for i in 10..15 {
            assert!(!alloc.page_is_free(i));
        }

        let addr = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        assert!(addr.page_index() < 10 || addr.page_index() >= 15);
        alloc.free_mem(addr, 1);
    }

    #[test]
    fn test_available_regions_init() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0x100000, 5);

        let addr1 = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        let addr2 = alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr1, 1);
        alloc.free_mem(addr2, 1);
        let free_before = alloc.free_pages;

        let mut free_regions: alloc::vec::Vec<BootMemRegion> = alloc::vec![];
        alloc.available_regions(&mut |base_page, num_pages| {
            free_regions.push(BootMemRegion {
                base: base_page * CLICK_SIZE,
                size: num_pages * CLICK_SIZE,
            });
        });

        let meta_size = BitmapAllocator::metadata_size(tp);
        let new_buf: alloc::vec::Vec<u8> = alloc::vec![0u8; meta_size + CLICK_SIZE];
        let new_buf_leaked = alloc::boxed::Box::leak(new_buf.into_boxed_slice());
        let new_alloc = BitmapAllocator::init(&mut new_buf_leaked[..meta_size], tp, &free_regions, 0, 0);

        assert_eq!(new_alloc.free_pages, free_before);
        assert_eq!(new_alloc.metadata_pa_range(), (0, 0));

        let mut new_alloc = new_alloc;
        let addr3 = new_alloc.alloc_mem(1, PageAllocFlags::empty()).unwrap();
        new_alloc.free_mem(addr3, 1);

        let addr4 = new_alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        new_alloc.free_mem(addr4, 10);
    }

    #[test]
    fn test_available_regions_basic() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let mut alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);

        let addr = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let alloc_start = addr.page_index();

        let mut free_regions: alloc::vec::Vec<(usize, usize)> = alloc::vec![];
        alloc.available_regions(&mut |base, count| {
            free_regions.push((base, count));
        });

        assert!(!free_regions.is_empty());
        let total_free: usize = free_regions.iter().map(|(_, c)| *c).sum();
        assert_eq!(total_free, alloc.free_pages);

        for (base, _) in &free_regions {
            assert!(*base < alloc_start || *base >= alloc_start + 10);
        }

        alloc.free_mem(addr, 10);
    }

    #[test]
    fn test_metadata_pa_range() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let alloc = BitmapAllocator::init(metadata, tp, &regions, 0x100000, 5);
        let (pa_base, pa_pages) = alloc.metadata_pa_range();
        assert_eq!(pa_base, 0x100000);
        assert_eq!(pa_pages, 5);
    }

    #[test]
    fn test_metadata_pa_range_default() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);
        let (pa_base, pa_pages) = alloc.metadata_pa_range();
        assert_eq!(pa_base, 0);
        assert_eq!(pa_pages, 0);
    }

    #[test]
    fn test_available_regions_init_clears_pa_range() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let alloc = BitmapAllocator::init(metadata, tp, &regions, 0x100000, 5);

        let mut free_regions: alloc::vec::Vec<BootMemRegion> = alloc::vec![];
        alloc.available_regions(&mut |base_page, num_pages| {
            free_regions.push(BootMemRegion {
                base: base_page * CLICK_SIZE,
                size: num_pages * CLICK_SIZE,
            });
        });

        let meta_size = BitmapAllocator::metadata_size(tp);
        let new_buf: alloc::vec::Vec<u8> = alloc::vec![0u8; meta_size + CLICK_SIZE];
        let new_buf_leaked = alloc::boxed::Box::leak(new_buf.into_boxed_slice());
        let new_alloc = BitmapAllocator::init(&mut new_buf_leaked[..meta_size], tp, &free_regions, 0, 0);

        let (pa_base, pa_pages) = new_alloc.metadata_pa_range();
        assert_eq!(pa_base, 0);
        assert_eq!(pa_pages, 0);
    }

    #[test]
    fn test_physalloc_bitmap_access() {
        let regions = make_test_regions();
        let tp = total_pages_from_regions(&regions);
        let metadata = make_test_metadata(tp);
        let alloc = BitmapAllocator::init(metadata, tp, &regions, 0, 0);
        let phys = PhysAlloc::Bitmap(alloc);

        assert!(phys.as_bitmap().is_some());
        let mut phys = phys;
        assert!(phys.as_bitmap_mut().is_some());
    }
}
