//! Slab cache implementation for fixed-size object allocation.

use super::{MockPageAllocator, SlabStats, PAGE_SIZE};
use core::ptr::NonNull;

#[allow(dead_code)]
struct SlabHeader {
    next: Option<NonNull<SlabHeader>>,
    nused: u16,
    object_size: u16,
    object_count: u16,
    use_bits: [u64; 8],
}

pub(crate) struct SlabCache {
    object_size: usize,
    objects_per_slab: usize,
    slabs: Option<NonNull<SlabHeader>>,
    stats: SlabStats,
    allocator: MockPageAllocator,
}

unsafe impl Send for SlabCache {}
unsafe impl Sync for SlabCache {}

impl SlabCache {
    pub(crate) fn new(object_size: usize) -> Self {
        assert!(object_size >= 8, "object size must be at least 8");
        assert!(object_size.is_power_of_two(), "object size must be power of two");
        assert!(object_size <= PAGE_SIZE / 2, "object size too large for slab");

        let header_size = core::mem::size_of::<SlabHeader>();
        let available = PAGE_SIZE - header_size;
        let objects_per_slab = available / object_size;

        Self {
            object_size,
            objects_per_slab,
            slabs: None,
            stats: SlabStats::new(),
            allocator: MockPageAllocator::default(),
        }
    }

    pub(crate) fn with_limit(object_size: usize, max_pages: usize) -> Self {
        let mut cache = Self::new(object_size);
        cache.allocator = MockPageAllocator::new(max_pages);
        cache
    }

    pub(crate) fn allocate(&mut self) -> Option<*mut u8> {
        if let Some(ptr) = self.alloc_from_existing() {
            self.stats.record_alloc(true);
            return Some(ptr);
        }

        if self.alloc_new_slab().is_none() {
            self.stats.record_failure();
            return None;
        }

        let ptr = self.alloc_from_existing()?;
        self.stats.record_alloc(false);
        Some(ptr)
    }

    fn alloc_from_existing(&mut self) -> Option<*mut u8> {
        let mut current = self.slabs?;

        loop {
            unsafe {
                let header = current.as_mut();

                if let Some(index) = self.find_free_slot(header) {
                    self.set_bit(header, index, true);
                    header.nused += 1;

                    let header_size = core::mem::size_of::<SlabHeader>();
                    let obj_addr = current.as_ptr() as usize + header_size + index * self.object_size;
                    return Some(obj_addr as *mut u8);
                }

                match header.next {
                    Some(next) => current = next,
                    None => return None,
                }
            }
        }
    }

    fn alloc_new_slab(&mut self) -> Option<()> {
        unsafe {
            let page = self.allocator.alloc_page()?;
            self.stats.record_page_alloc(1);

            let header = page as *mut SlabHeader;
            (*header) = SlabHeader {
                next: self.slabs,
                nused: 0,
                object_size: self.object_size as u16,
                object_count: self.objects_per_slab as u16,
                use_bits: [0; 8],
            };

            self.slabs = Some(NonNull::new_unchecked(header));
            Some(())
        }
    }

    fn find_free_slot(&self, header: &SlabHeader) -> Option<usize> {
        for (chunk_idx, chunk) in header.use_bits.iter().enumerate() {
            if *chunk != u64::MAX {
                let bit_idx = chunk.trailing_ones() as usize;
                if bit_idx < 64 {
                    let index = chunk_idx * 64 + bit_idx;
                    if index < self.objects_per_slab {
                        return Some(index);
                    }
                }
            }
        }
        None
    }

    fn set_bit(&self, header: &mut SlabHeader, index: usize, used: bool) {
        let chunk_idx = index / 64;
        let bit_idx = index % 64;

        if used {
            header.use_bits[chunk_idx] |= 1 << bit_idx;
        } else {
            header.use_bits[chunk_idx] &= !(1 << bit_idx);
        }
    }

    pub(crate) unsafe fn free(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        let slab = unsafe { self.find_slab_containing(ptr) };
        if slab.is_none() {
            panic!("attempt to free invalid pointer: {:p}", ptr);
        }

        let header = slab.unwrap().as_ptr() as *mut SlabHeader;
        let header_size = core::mem::size_of::<SlabHeader>();
        let slab_start = header as usize;
        let offset = ptr as usize - slab_start - header_size;
        let index = offset / self.object_size;

        assert!(index < self.objects_per_slab, "invalid object index");

        let chunk_idx = index / 64;
        let bit_idx = index % 64;
        let bit = unsafe { ((*header).use_bits[chunk_idx] >> bit_idx) & 1 };
        if bit == 0 {
            panic!("double free detected at {:p}", ptr);
        }

        unsafe {
            self.set_bit(&mut *header, index, false);
            (*header).nused -= 1;
        }

        self.stats.record_free();
    }

    unsafe fn find_slab_containing(&self, ptr: *mut u8) -> Option<NonNull<SlabHeader>> {
        let mut current = self.slabs?;
        let ptr_addr = ptr as usize;

        loop {
            let header = current.as_ptr() as *mut SlabHeader;
            let slab_start = header as usize;
            let slab_end = slab_start + PAGE_SIZE;

            if ptr_addr >= slab_start && ptr_addr < slab_end {
                return Some(current);
            }

            let next = unsafe { (*header).next };
            match next {
                Some(n) => current = n,
                None => return None,
            }
        }
    }

    pub(crate) fn stats(&self) -> &SlabStats {
        &self.stats
    }

    pub(crate) fn object_size(&self) -> usize {
        self.object_size
    }

    pub(crate) fn slab_count(&self) -> usize {
        let mut count = 0;
        let mut current = self.slabs;

        while let Some(slab) = current {
            count += 1;
            unsafe {
                current = slab.as_ref().next;
            }
        }

        count
    }

    pub(crate) unsafe fn free_batch(&mut self, ptrs: &[*mut u8]) {
        for ptr in ptrs {
            unsafe { self.free(*ptr); }
        }
    }
}

impl Drop for SlabCache {
    fn drop(&mut self) {
        unsafe {
            let mut current = self.slabs;
            while let Some(slab) = current {
                let header = slab.as_ptr();
                current = (*header).next;
                self.allocator.free_page(header as *mut u8);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn test_basic_alloc_free() {
        let mut cache = SlabCache::new(64);
        let ptr = cache.allocate().expect("allocation failed");
        assert!(!ptr.is_null());
        unsafe { cache.free(ptr); }
        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_multiple_allocations() {
        let mut cache = SlabCache::new(32);
        let mut ptrs = Vec::new();

        for _ in 0..100 {
            let ptr = cache.allocate().expect("allocation failed");
            ptrs.push(ptr);
        }

        assert_eq!(cache.stats().active_allocations(), 100);

        unsafe {
            for ptr in ptrs {
                cache.free(ptr);
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_alloc_write_read() {
        let mut cache = SlabCache::new(64);
        let ptr = cache.allocate().expect("allocation failed") as *mut u64;
        unsafe {
            ptr.write(0xDEADBEEF);
            assert_eq!(ptr.read(), 0xDEADBEEF);
            cache.free(ptr as *mut u8);
        }
    }

    #[test]
    fn test_different_sizes() {
        let sizes = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

        for size in sizes {
            let mut cache = SlabCache::new(size);
            let ptr = cache.allocate().expect("allocation failed");
            assert!(!ptr.is_null());
            unsafe { cache.free(ptr); }
        }
    }

    #[test]
    fn test_alloc_until_exhausted() {
        let mut cache = SlabCache::with_limit(64, 2);

        let mut count = 0;
        while let Some(_ptr) = cache.allocate() {
            count += 1;
            if count > 1000 {
                break;
            }
        }

        assert!(count > 0, "should allocate at least one object");
        assert!(cache.stats().allocation_failures() > 0, "should have failures");
    }

    #[test]
    #[should_panic(expected = "double free")]
    fn test_double_free() {
        let mut cache = SlabCache::new(64);
        let ptr = cache.allocate().expect("allocation failed");
        unsafe {
            cache.free(ptr);
            cache.free(ptr);
        }
    }

    #[test]
    #[should_panic(expected = "invalid pointer")]
    fn test_free_invalid_pointer() {
        let mut cache = SlabCache::new(64);
        let invalid_ptr = 0x12345678 as *mut u8;
        unsafe { cache.free(invalid_ptr); }
    }

    #[test]
    fn test_free_null_pointer() {
        let mut cache = SlabCache::new(64);
        unsafe { cache.free(core::ptr::null_mut()); }
    }

    #[test]
    fn test_batch_free() {
        let mut cache = SlabCache::new(64);
        let ptrs: Vec<_> = (0..100).map(|_| cache.allocate().unwrap()).collect();
        unsafe { cache.free_batch(&ptrs); }
        assert_eq!(cache.stats().active_allocations(), 0);
    }
}
