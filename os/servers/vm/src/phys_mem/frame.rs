//! 物理页框所有权抽象
//!
//! 提供 `PhysFrame` 类型，利用 Rust 所有权系统确保物理页框的安全管理。

use super::{AllocFlags, PhysAddr, PhysMemAllocator, CLICK_SHIFT};
use std::cell::RefCell;
use std::mem;
use std::rc::{Rc, Weak};

/// 表示一个物理页框的所有权
///
/// 当 `PhysFrame` 被 drop 时，自动将页框归还给分配器。
/// 这防止了常见的内存管理错误：
/// - 重复释放（double free）
/// - 使用已释放的页框（use after free）
/// - 忘记释放（memory leak）
///
/// # 示例
///
/// ```rust
/// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags, PhysFrame};
/// use std::cell::RefCell;
/// use std::rc::Rc;
///
/// let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
///
/// {
///     let frame = PhysFrame::alloc(&allocator, AllocFlags::empty())
///         .expect("allocation failed");
///
///     println!("PFN: {}", frame.pfn());
///
///     // frame 离开作用域时自动释放
/// }
/// ```
#[derive(Debug)]
pub struct PhysFrame {
    /// 页框的物理地址
    addr: PhysAddr,
    /// 指向分配器的弱引用（避免循环引用）
    allocator: Weak<RefCell<PhysMemAllocator>>,
}

impl PhysFrame {
    /// 从分配器分配一个新的页框
    ///
    /// # 参数
    /// - `allocator`: 物理内存分配器的引用
    /// - `flags`: 分配标志
    ///
    /// # 返回值
    /// - `Some(PhysFrame)`: 分配成功
    /// - `None`: 分配失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags, PhysFrame};
    /// use std::cell::RefCell;
    /// use std::rc::Rc;
    ///
    /// let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
    /// let frame = PhysFrame::alloc(&allocator, AllocFlags::ZERO)
    ///     .expect("allocation failed");
    /// ```
    pub fn alloc(
        allocator: &Rc<RefCell<PhysMemAllocator>>,
        flags: AllocFlags,
    ) -> Option<Self> {
        let addr = allocator.borrow_mut().alloc(1, flags)?;
        Some(PhysFrame {
            addr,
            allocator: Rc::downgrade(allocator),
        })
    }

    /// 获取页框的物理地址
    ///
    /// # 返回值
    /// 页框的起始物理地址
    pub fn addr(&self) -> PhysAddr {
        self.addr
    }

    /// 获取页框号（PFN - Page Frame Number）
    ///
    /// PFN = 物理地址 >> CLICK_SHIFT
    ///
    /// # 返回值
    /// 页框号
    pub fn pfn(&self) -> u64 {
        self.addr.0 >> CLICK_SHIFT
    }

    /// 将页框转换为原始地址，消耗所有权但不释放
    ///
    /// 调用者负责在不再需要时手动释放页框。
    ///
    /// # 返回值
    /// 页框的物理地址
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags, PhysFrame};
    /// use std::cell::RefCell;
    /// use std::rc::Rc;
    ///
    /// let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
    ///
    /// let frame = PhysFrame::alloc(&allocator, AllocFlags::empty())
    ///     .expect("allocation failed");
    ///
    /// // 转换为原始地址，所有权转移给调用者
    /// let addr = frame.into_raw();
    ///
    /// // 稍后必须手动释放
    /// allocator.borrow_mut().free(addr, 1);
    /// ```
    pub fn into_raw(self) -> PhysAddr {
        let addr = self.addr;
        mem::forget(self); // 防止调用 drop
        addr
    }

    /// 从原始地址创建 `PhysFrame`
    ///
    /// # Safety
    /// 调用者必须确保：
    /// - `addr` 是一个有效的、已分配的物理页框地址
    /// - `addr` 当前没有被其他的 `PhysFrame` 或数据结构引用
    /// - `addr` 是由指定的 `allocator` 分配的
    ///
    /// # 参数
    /// - `addr`: 物理页框地址
    /// - `allocator`: 分配器的弱引用
    ///
    /// # 返回值
    /// 新的 `PhysFrame` 实例
    ///
    /// # 示例
    ///
    /// ```rust
    /// use minix_vm::phys_mem::{PhysMemAllocator, AllocFlags, PhysFrame};
    /// use std::cell::RefCell;
    /// use std::rc::Rc;
    ///
    /// let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
    ///
    /// // 使用原始分配
    /// let addr = allocator.borrow_mut().alloc(1, AllocFlags::empty())
    ///     .expect("allocation failed");
    ///
    /// // 不安全：包装为 PhysFrame
    /// let frame = unsafe {
    ///     PhysFrame::from_raw(addr, Rc::downgrade(&allocator))
    /// };
    ///
    /// // frame 离开作用域时自动释放
    /// ```
    pub unsafe fn from_raw(addr: PhysAddr, allocator: Weak<RefCell<PhysMemAllocator>>) -> Self {
        PhysFrame { addr, allocator }
    }
}

impl Drop for PhysFrame {
    /// 当 `PhysFrame` 离开作用域时，自动释放页框
    fn drop(&mut self) {
        if let Some(alloc_rc) = self.allocator.upgrade() {
            // 释放单个页框（1 click）
            alloc_rc.borrow_mut().free(self.addr, 1);
        }
        // 如果分配器已经被释放，无法归还内存
        // 在实际内核中，这应该是一个严重错误
    }
}

// PhysFrame 不实现 Clone，因为这会导致重复释放
// 如果需要共享所有权，应该使用 Rc<PhysFrame> 或 Arc<PhysFrame>

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 PhysFrame 基本分配和自动释放
    #[test]
    fn test_phys_frame_alloc_drop() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let initial_allocated = allocator.borrow().stats().total_allocated();

        {
            let frame = PhysFrame::alloc(&allocator, AllocFlags::empty())
                .expect("allocation should succeed");

            assert!(frame.addr().is_valid());
            assert_eq!(frame.pfn(), frame.addr().0 >> CLICK_SHIFT);

            // 统计应该显示已分配
            let current_allocated = allocator.borrow().stats().total_allocated();
            assert!(current_allocated > initial_allocated);
        }

        // frame 被 drop 后，内存应该被释放
        let final_allocated = allocator.borrow().stats().total_allocated();
        assert_eq!(final_allocated, initial_allocated);
    }

    /// 测试 into_raw 绕过自动释放
    #[test]
    fn test_phys_frame_into_raw() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let addr = {
            let frame = PhysFrame::alloc(&allocator, AllocFlags::empty())
                .expect("allocation should succeed");
            frame.into_raw() // 消耗所有权，不释放
        };

        // 此时内存仍未释放
        let current_allocated = allocator.borrow().stats().total_allocated();
        assert!(current_allocated > 0);

        // 手动释放
        allocator.borrow_mut().free(addr, 1);
    }

    /// 测试 from_raw 重新获得所有权
    #[test]
    fn test_phys_frame_from_raw() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        // 原始分配
        let addr = allocator
            .borrow_mut()
            .alloc(1, AllocFlags::empty())
            .expect("allocation should succeed");

        // 包装为 PhysFrame
        let frame = unsafe { PhysFrame::from_raw(addr, Rc::downgrade(&allocator)) };
        assert_eq!(frame.addr(), addr);

        // frame 离开作用域时自动释放
        drop(frame);

        // 内存应该被释放
        let current_allocated = allocator.borrow().stats().total_allocated();
        assert_eq!(current_allocated, 0);
    }

    /// 测试多个 PhysFrame 的分配和释放
    #[test]
    fn test_multiple_phys_frames() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let frames: Vec<_> = (0..10)
            .map(|_| PhysFrame::alloc(&allocator, AllocFlags::empty()).expect("alloc failed"))
            .collect();

        // 验证所有帧都有有效地址
        for (i, frame) in frames.iter().enumerate() {
            assert!(frame.addr().is_valid(), "frame {} should have valid addr", i);
        }

        // 统计应该显示已分配 10 页
        let current_allocated = allocator.borrow().stats().total_allocated();
        assert_eq!(current_allocated, 10 * 4096);

        // 显式 drop 所有帧
        drop(frames);

        // 所有内存应该被释放
        let final_allocated = allocator.borrow().stats().total_allocated();
        assert_eq!(final_allocated, 0);
    }

    /// 测试 ZERO 标志
    #[test]
    fn test_phys_frame_zero_flag() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let frame = PhysFrame::alloc(&allocator, AllocFlags::ZERO)
            .expect("allocation should succeed");

        assert!(frame.addr().is_valid());
        // ZERO 标志已设置，分配器应该清零内存
    }

    /// 测试分配器被释放后的行为
    #[test]
    fn test_allocator_dropped_before_frame() {
        let frame = {
            let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
            PhysFrame::alloc(&allocator, AllocFlags::empty()).expect("alloc failed")
        };

        // allocator 已经被释放，但 frame 仍然存在
        // 当 frame 被 drop 时，upgrade() 会失败，不会 panic
        drop(frame);
    }
}
