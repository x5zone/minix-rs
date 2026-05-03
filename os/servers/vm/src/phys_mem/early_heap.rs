//! Early heap allocator for VM initialization.
//!
//! This module provides a simple bump allocator used during VM initialization
//! to allocate memory for physical allocator metadata before the permanent heap
//! is available.
//!
//! # Design
//!
//! - **Bump allocator**: Only allocates, never frees
//! - **No circular dependency**: Physical allocator does not manage early heap pages
//! - **One-time use**: After initialization, early heap is not used for further allocation
//! - **Type-safe**: Provides `alloc_slice<T>()` for typed allocation

use core::ptr::NonNull;

const PAGE_SIZE: usize = 4096;

pub struct EarlyHeap {
    start: NonNull<u8>,
    current: NonNull<u8>,
    end: NonNull<u8>,
}

impl EarlyHeap {
    pub const fn new() -> Self {
        Self {
            start: NonNull::dangling(),
            current: NonNull::dangling(),
            end: NonNull::dangling(),
        }
    }

    pub fn init(&mut self, start: *mut u8, size: usize) {
        assert!(size > 0, "early heap size must be positive");
        assert!(!start.is_null(), "early heap start must not be null");
        
        self.start = unsafe { NonNull::new_unchecked(start) };
        self.current = self.start;
        self.end = unsafe { NonNull::new_unchecked(start.add(size)) };
    }

    pub fn alloc_slice<T>(&mut self, count: usize) -> &'static mut [T] {
        if count == 0 {
            return &mut [];
        }

        let size = count * core::mem::size_of::<T>();
        let align = core::mem::align_of::<T>();

        let ptr = self.alloc_aligned(size, align);
        
        unsafe {
            core::slice::from_raw_parts_mut(ptr as *mut T, count)
        }
    }

    pub fn alloc_bytes(&mut self, size: usize, align: usize) -> *mut u8 {
        if size == 0 {
            return self.current.as_ptr();
        }
        self.alloc_aligned(size, align)
    }

    fn alloc_aligned(&mut self, size: usize, align: usize) -> *mut u8 {
        let current = self.current.as_ptr() as usize;
        let aligned = (current + align - 1) & !(align - 1);
        let new_current = aligned + size;

        if new_current > self.end.as_ptr() as usize {
            panic!(
                "early heap exhausted: need {} bytes at {:#x}, but end is at {:#x}",
                size,
                aligned,
                self.end.as_ptr() as usize
            );
        }

        self.current = unsafe { NonNull::new_unchecked(new_current as *mut u8) };
        aligned as *mut u8
    }

    pub fn used(&self) -> usize {
        self.current.as_ptr() as usize - self.start.as_ptr() as usize
    }

    pub fn total(&self) -> usize {
        self.end.as_ptr() as usize - self.start.as_ptr() as usize
    }

    pub fn remaining(&self) -> usize {
        self.end.as_ptr() as usize - self.current.as_ptr() as usize
    }

    pub fn is_initialized(&self) -> bool {
        !self.start.as_ptr().is_null() 
            && self.start.as_ptr() != NonNull::dangling().as_ptr()
    }
}

// SAFETY: EarlyHeap is only used during VM initialization on a single thread.
// The VM server runs as a single-threaded process. The raw pointers inside
// EarlyHeap are never accessed concurrently.
unsafe impl Send for EarlyHeap {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_slice() {
        let mut heap = EarlyHeap::new();
        let mut buffer = [0u8; 1024];
        heap.init(buffer.as_mut_ptr(), buffer.len());

        let slice: &mut [u64] = heap.alloc_slice(10);
        assert_eq!(slice.len(), 10);
    }

    #[test]
    fn test_alloc_alignment() {
        let mut heap = EarlyHeap::new();
        let mut buffer = [0u8; 1024];
        heap.init(buffer.as_mut_ptr(), buffer.len());

        let ptr1 = heap.alloc_bytes(1, 1);
        let ptr2 = heap.alloc_bytes(1, 8);
        
        assert_eq!(ptr2 as usize % 8, 0);
    }

    #[test]
    fn test_used_remaining() {
        let mut heap = EarlyHeap::new();
        let mut buffer = [0u8; 1024];
        heap.init(buffer.as_mut_ptr(), buffer.len());

        assert_eq!(heap.used(), 0);
        assert_eq!(heap.remaining(), 1024);

        heap.alloc_bytes(100, 1);
        assert_eq!(heap.used(), 100);
        assert_eq!(heap.remaining(), 924);
    }

    #[test]
    #[should_panic(expected = "early heap exhausted")]
    fn test_exhaustion() {
        let mut heap = EarlyHeap::new();
        let mut buffer = [0u8; 64];
        heap.init(buffer.as_mut_ptr(), buffer.len());

        heap.alloc_bytes(128, 1);
    }
}
