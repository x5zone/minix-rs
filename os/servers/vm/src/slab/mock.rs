//! Mock 物理内存分配器
//!
//! 用户态测试用的物理内存模拟，不涉及真实硬件。
//! 所有内存操作都在用户态堆上完成。

use std::alloc::{alloc, dealloc, Layout};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Mock 物理页分配器
///
/// 模拟物理内存分配，实际使用用户态堆内存。
/// 用于测试 Slab 分配器而无需真实硬件。
pub struct MockPageAllocator {
    /// 已分配的页数
    allocated_pages: AtomicUsize,
    /// 最大允许分配的页数（用于测试内存耗尽场景）
    max_pages: usize,
}

impl MockPageAllocator {
    /// 创建新的 Mock 分配器
    ///
    /// # 参数
    /// - `max_pages`: 最大允许分配的页数，0 表示无限制
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::slab::MockPageAllocator;
    ///
    /// let allocator = MockPageAllocator::new(100);
    /// ```
    pub fn new(max_pages: usize) -> Self {
        Self {
            allocated_pages: AtomicUsize::new(0),
            max_pages,
        }
    }

    /// 分配一页内存
    ///
    /// 返回指向 4KB 对齐内存的指针。
    /// 如果超过 max_pages 限制，返回 None。
    ///
    /// # Safety
    ///
    /// 返回的指针必须在使用后通过 `free_page` 释放。
    pub unsafe fn alloc_page(&self) -> Option<*mut u8> {
        // 检查限制
        if self.max_pages > 0 {
            let current = self.allocated_pages.fetch_add(1, Ordering::SeqCst);
            if current >= self.max_pages {
                self.allocated_pages.fetch_sub(1, Ordering::SeqCst);
                return None;
            }
        } else {
            self.allocated_pages.fetch_add(1, Ordering::SeqCst);
        }

        // 分配 4KB 对齐内存
        let layout = Layout::from_size_align(4096, 4096).ok()?;
        let ptr = unsafe { alloc(layout) };

        if ptr.is_null() {
            self.allocated_pages.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(ptr)
        }
    }

    /// 释放一页内存
    ///
    /// # Safety
    ///
    /// - `ptr` 必须是通过 `alloc_page` 分配的
    /// - `ptr` 不能被重复释放
    pub unsafe fn free_page(&self, ptr: *mut u8) {
        if !ptr.is_null() {
            let layout = Layout::from_size_align(4096, 4096).unwrap();
            unsafe { dealloc(ptr, layout) };
            self.allocated_pages.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// 获取当前已分配的页数
    pub fn allocated_count(&self) -> usize {
        self.allocated_pages.load(Ordering::Relaxed)
    }

    /// 分配多页内存
    ///
    /// 返回连续的多页内存指针。
    ///
    /// # Safety
    ///
    /// 返回的指针必须通过 `free_pages` 释放。
    pub unsafe fn alloc_pages(&self, count: usize) -> Option<*mut u8> {
        if count == 0 {
            return None;
        }

        // 检查限制
        if self.max_pages > 0 {
            let current = self.allocated_pages.fetch_add(count, Ordering::SeqCst);
            if current + count > self.max_pages {
                self.allocated_pages.fetch_sub(count, Ordering::SeqCst);
                return None;
            }
        } else {
            self.allocated_pages.fetch_add(count, Ordering::SeqCst);
        }

        // 分配连续内存
        let layout = Layout::from_size_align(4096 * count, 4096).ok()?;
        let ptr = unsafe { alloc(layout) };

        if ptr.is_null() {
            self.allocated_pages.fetch_sub(count, Ordering::SeqCst);
            None
        } else {
            Some(ptr)
        }
    }

    /// 释放多页内存
    ///
    /// # Safety
    ///
    /// - `ptr` 必须是通过 `alloc_pages` 分配的
    /// - `count` 必须与分配时相同
    pub unsafe fn free_pages(&self, ptr: *mut u8, count: usize) {
        if !ptr.is_null() && count > 0 {
            let layout = Layout::from_size_align(4096 * count, 4096).unwrap();
            unsafe { dealloc(ptr, layout) };
            self.allocated_pages.fetch_sub(count, Ordering::SeqCst);
        }
    }
}

impl Default for MockPageAllocator {
    fn default() -> Self {
        Self::new(0) // 默认无限制
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

            // 第三次分配应该失败
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
