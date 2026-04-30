//! Mock physical memory allocator for user-space testing.

use alloc::alloc::{alloc, dealloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct MockPageAllocator {
    allocated_pages: AtomicUsize,
    max_pages: usize,
}

impl MockPageAllocator {
    pub(crate) fn new(max_pages: usize) -> Self {
        Self {
            allocated_pages: AtomicUsize::new(0),
            max_pages,
        }
    }

    pub(crate) unsafe fn alloc_page(&self) -> Option<*mut u8> {
        if self.max_pages > 0 {
            let current = self.allocated_pages.fetch_add(1, Ordering::SeqCst);
            if current >= self.max_pages {
                self.allocated_pages.fetch_sub(1, Ordering::SeqCst);
                return None;
            }
        } else {
            self.allocated_pages.fetch_add(1, Ordering::SeqCst);
        }

        let layout = Layout::from_size_align(4096, 4096).ok()?;
        let ptr = unsafe { alloc(layout) };

        if ptr.is_null() {
            self.allocated_pages.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(ptr)
        }
    }

    pub(crate) unsafe fn free_page(&self, ptr: *mut u8) {
        if !ptr.is_null() {
            let layout = Layout::from_size_align(4096, 4096).unwrap();
            unsafe { dealloc(ptr, layout) };
            self.allocated_pages.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(crate) fn allocated_count(&self) -> usize {
        self.allocated_pages.load(Ordering::Relaxed)
    }

    pub(crate) unsafe fn alloc_pages(&self, count: usize) -> Option<*mut u8> {
        if count == 0 {
            return None;
        }

        if self.max_pages > 0 {
            let current = self.allocated_pages.fetch_add(count, Ordering::SeqCst);
            if current + count > self.max_pages {
                self.allocated_pages.fetch_sub(count, Ordering::SeqCst);
                return None;
            }
        } else {
            self.allocated_pages.fetch_add(count, Ordering::SeqCst);
        }

        let layout = Layout::from_size_align(4096 * count, 4096).ok()?;
        let ptr = unsafe { alloc(layout) };

        if ptr.is_null() {
            self.allocated_pages.fetch_sub(count, Ordering::SeqCst);
            None
        } else {
            Some(ptr)
        }
    }

    pub(crate) unsafe fn free_pages(&self, ptr: *mut u8, count: usize) {
        if !ptr.is_null() && count > 0 {
            let layout = Layout::from_size_align(4096 * count, 4096).unwrap();
            unsafe { dealloc(ptr, layout) };
            self.allocated_pages.fetch_sub(count, Ordering::SeqCst);
        }
    }
}

impl Default for MockPageAllocator {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_free_page() {
        let allocator = MockPageAllocator::new(10);

        unsafe {
            let ptr = allocator.alloc_page().expect("allocation failed");
            assert!(!ptr.is_null());
            assert_eq!(allocator.allocated_count(), 1);

            allocator.free_page(ptr);
            assert_eq!(allocator.allocated_count(), 0);
        }
    }

    #[test]
    fn test_max_pages_limit() {
        let allocator = MockPageAllocator::new(2);

        unsafe {
            let ptr1 = allocator.alloc_page().expect("allocation failed");
            let ptr2 = allocator.alloc_page().expect("allocation failed");
            assert_eq!(allocator.allocated_count(), 2);

            let ptr3 = allocator.alloc_page();
            assert!(ptr3.is_none());

            allocator.free_page(ptr1);
            allocator.free_page(ptr2);
        }
    }

    #[test]
    fn test_alloc_multiple_pages() {
        let allocator = MockPageAllocator::new(10);

        unsafe {
            let ptr = allocator.alloc_pages(3).expect("allocation failed");
            assert!(!ptr.is_null());
            assert_eq!(allocator.allocated_count(), 3);

            allocator.free_pages(ptr, 3);
            assert_eq!(allocator.allocated_count(), 0);
        }
    }
}
