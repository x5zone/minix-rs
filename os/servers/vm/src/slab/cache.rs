//! Slab 缓存实现
//!
//! 提供固定大小对象的分配和释放功能。

use super::{MockPageAllocator, SlabStats, PAGE_SIZE};
use std::ptr::NonNull;

/// Slab 头信息
///
/// 每个 Slab 页面开头的元数据。
#[allow(dead_code)]
struct SlabHeader {
    /// 下一个 slab 指针
    next: Option<NonNull<SlabHeader>>,
    /// 已使用对象数
    nused: u16,
    /// 对象大小
    object_size: u16,
    /// 对象数量
    object_count: u16,
    /// 使用位图（每个位表示一个对象是否被使用）
    /// 最大支持 512 个对象（4096 / 8）
    use_bits: [u64; 8], // 512 bits
}

/// Slab 缓存
///
/// 管理特定大小对象的分配和释放。
/// 每个 SlabCache 只分配固定大小的对象。
///
/// # 示例
///
/// ```rust
/// use minix_vm::slab::SlabCache;
///
/// let mut cache = SlabCache::new(64);
/// let ptr = cache.allocate().expect("allocation failed");
/// unsafe { cache.free(ptr); }
/// ```
pub struct SlabCache {
    /// 对象大小
    object_size: usize,
    /// 每个 slab 中的对象数量
    objects_per_slab: usize,
    /// 第一个 slab
    slabs: Option<NonNull<SlabHeader>>,
    /// 统计信息
    stats: SlabStats,
    /// 页分配器
    allocator: MockPageAllocator,
}

// SlabCache 可以安全地跨线程传递（如果 stats 是原子的）
unsafe impl Send for SlabCache {}
unsafe impl Sync for SlabCache {}

impl SlabCache {
    /// 创建新的 Slab 缓存
    ///
    /// # 参数
    /// - `object_size`: 对象大小（字节），必须是 2 的幂次且 >= 8
    ///
    /// # Panics
    ///
    /// 如果 object_size 不是 2 的幂次或小于 8，会 panic。
    pub fn new(object_size: usize) -> Self {
        assert!(object_size >= 8, "object size must be at least 8");
        assert!(
            object_size.is_power_of_two(),
            "object size must be power of two"
        );
        assert!(
            object_size <= PAGE_SIZE / 2,
            "object size too large for slab"
        );

        // 计算每个 slab 能容纳的对象数量
        // 减去 header 大小，然后除以对象大小
        let header_size = std::mem::size_of::<SlabHeader>();
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

    /// 创建带有限制的 Slab 缓存
    ///
    /// # 参数
    /// - `object_size`: 对象大小
    /// - `max_pages`: 最大页数限制
    pub fn with_limit(object_size: usize, max_pages: usize) -> Self {
        let mut cache = Self::new(object_size);
        cache.allocator = MockPageAllocator::new(max_pages);
        cache
    }

    /// 分配一个对象
    ///
    /// 返回指向对象的指针，如果分配失败返回 None。
    ///
    /// # 返回值
    /// - `Some(*mut u8)`: 分配成功，返回对象指针
    /// - `None`: 分配失败（内存不足）
    pub fn allocate(&mut self) -> Option<*mut u8> {
        // 尝试从现有 slab 分配
        if let Some(ptr) = self.alloc_from_existing() {
            self.stats.record_alloc(true); // 快速路径
            return Some(ptr);
        }

        // 需要分配新 slab
        if self.alloc_new_slab().is_none() {
            self.stats.record_failure();
            return None;
        }

        // 从新 slab 分配
        let ptr = self.alloc_from_existing()?;
        self.stats.record_alloc(false); // 慢速路径
        Some(ptr)
    }

    /// 从现有 slab 分配
    fn alloc_from_existing(&mut self) -> Option<*mut u8> {
        let mut current = self.slabs?;

        loop {
            unsafe {
                let header = current.as_mut();

                // 尝试在这个 slab 中分配
                if let Some(index) = self.find_free_slot(header) {
                    // 标记为已使用
                    self.set_bit(header, index, true);
                    header.nused += 1;

                    // 计算对象地址
                    let header_size = std::mem::size_of::<SlabHeader>();
                    let obj_addr = current.as_ptr() as usize + header_size + index * self.object_size;
                    return Some(obj_addr as *mut u8);
                }

                // 尝试下一个 slab
                match header.next {
                    Some(next) => current = next,
                    None => return None,
                }
            }
        }
    }

    /// 分配新的 slab
    fn alloc_new_slab(&mut self) -> Option<()> {
        unsafe {
            let page = self.allocator.alloc_page()?;
            self.stats.record_page_alloc(1);

            // 初始化 slab header
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

    /// 查找空闲槽位
    fn find_free_slot(&self, header: &SlabHeader) -> Option<usize> {
        for (chunk_idx, chunk) in header.use_bits.iter().enumerate() {
            if *chunk != u64::MAX {
                // 这个 chunk 有空闲位
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

    /// 设置位图
    fn set_bit(&self, header: &mut SlabHeader, index: usize, used: bool) {
        let chunk_idx = index / 64;
        let bit_idx = index % 64;

        if used {
            header.use_bits[chunk_idx] |= 1 << bit_idx;
        } else {
            header.use_bits[chunk_idx] &= !(1 << bit_idx);
        }
    }

    /// 释放一个对象
    ///
    /// # 参数
    /// - `ptr`: 对象指针（必须是通过 `allocate` 分配的）
    ///
    /// # Safety
    ///
    /// - `ptr` 必须是通过本缓存的 `allocate` 分配的
    /// - `ptr` 不能被重复释放
    /// - `ptr` 不能是空指针
    pub unsafe fn free(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }

        // 找到包含这个指针的 slab
        let slab = unsafe { self.find_slab_containing(ptr) };
        if slab.is_none() {
            panic!("attempt to free invalid pointer: {:p}", ptr);
        }

        let header = slab.unwrap().as_ptr() as *mut SlabHeader;
        let header_size = std::mem::size_of::<SlabHeader>();
        let slab_start = header as usize;
        let offset = ptr as usize - slab_start - header_size;
        let index = offset / self.object_size;

        // 检查索引是否有效
        assert!(index < self.objects_per_slab, "invalid object index");

        // 检查是否已释放（双释放检测）
        let chunk_idx = index / 64;
        let bit_idx = index % 64;
        let bit = unsafe { ((*header).use_bits[chunk_idx] >> bit_idx) & 1 };
        if bit == 0 {
            panic!("double free detected at {:p}", ptr);
        }

        // 标记为未使用
        unsafe {
            self.set_bit(&mut *header, index, false);
            (*header).nused -= 1;
        }

        self.stats.record_free();

        // 如果 slab 为空，考虑释放它（简化版：不释放，保持缓存）
    }

    /// 查找包含指定指针的 slab
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

    /// 获取统计信息引用
    pub fn stats(&self) -> &SlabStats {
        &self.stats
    }

    /// 获取对象大小
    pub fn object_size(&self) -> usize {
        self.object_size
    }

    /// 获取当前 slab 数量
    pub fn slab_count(&self) -> usize {
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

    /// 批量释放对象
    ///
    /// # Safety
    ///
    /// 所有指针必须是通过本缓存的 `allocate` 分配的。
    pub unsafe fn free_batch(&mut self, ptrs: &[*mut u8]) {
        for ptr in ptrs {
            unsafe { self.free(*ptr) };
        }
    }
}

impl Drop for SlabCache {
    fn drop(&mut self) {
        // 释放所有 slab
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

    #[test]
    fn test_basic_alloc_free() {
        let mut cache = SlabCache::new(64);

        let ptr = cache.allocate().expect("allocation failed");
        assert!(!ptr.is_null());

        unsafe {
            cache.free(ptr);
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_multiple_allocations() {
        let mut cache = SlabCache::new(32);
        let mut ptrs = Vec::new();

        // 分配多个对象
        for _ in 0..100 {
            let ptr = cache.allocate().expect("allocation failed");
            ptrs.push(ptr);
        }

        assert_eq!(cache.stats().active_allocations(), 100);

        // 释放所有
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

            unsafe {
                cache.free(ptr);
            }
        }
    }

    #[test]
    fn test_alloc_until_exhausted() {
        let mut cache = SlabCache::with_limit(64, 2); // 最多 2 页

        let mut count = 0;
        while let Some(ptr) = cache.allocate() {
            count += 1;
            if count > 1000 {
                break; // 防止无限循环
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
            cache.free(ptr); // 应该 panic
        }
    }

    #[test]
    #[should_panic(expected = "invalid pointer")]
    fn test_free_invalid_pointer() {
        let mut cache = SlabCache::new(64);
        let invalid_ptr = 0x12345678 as *mut u8;

        unsafe {
            cache.free(invalid_ptr);
        }
    }

    #[test]
    fn test_free_null_pointer() {
        let mut cache = SlabCache::new(64);

        unsafe {
            cache.free(std::ptr::null_mut()); // 应该安全地返回
        }
    }

    #[test]
    fn test_batch_free() {
        let mut cache = SlabCache::new(64);
        let ptrs: Vec<_> = (0..100)
            .map(|_| cache.allocate().unwrap())
            .collect();

        unsafe {
            cache.free_batch(&ptrs);
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }
}
