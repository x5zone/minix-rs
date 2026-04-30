//! Physical frame ownership abstraction.
//!
//! Provides `PhysFrame` type that leverages Rust's ownership system for safe physical frame management.

use super::{AllocFlags, PhysAddr, PhysMemAllocator, CLICK_SHIFT};
use alloc::rc::{Rc, Weak};
use core::cell::RefCell;
use core::mem;

/// Represents ownership of a physical frame.
///
/// When `PhysFrame` is dropped, the frame is automatically returned to the allocator.
/// This prevents common memory management errors:
/// - Double free
/// - Use after free
/// - Memory leak
#[derive(Debug)]
pub(crate) struct PhysFrame {
    addr: PhysAddr,
    allocator: Weak<RefCell<PhysMemAllocator>>,
}

impl PhysFrame {
    /// Allocates a new frame from the allocator.
    ///
    /// Returns `Some(PhysFrame)` on success, `None` on failure.
    pub(crate) fn alloc(
        allocator: &Rc<RefCell<PhysMemAllocator>>,
        flags: AllocFlags,
    ) -> Option<Self> {
        let addr = allocator.borrow_mut().alloc(1, flags)?;
        Some(PhysFrame {
            addr,
            allocator: Rc::downgrade(allocator),
        })
    }

    /// Returns the physical address of the frame.
    pub(crate) fn addr(&self) -> PhysAddr {
        self.addr
    }

    /// Returns the Page Frame Number (PFN = physical address >> CLICK_SHIFT).
    pub(crate) fn pfn(&self) -> u64 {
        self.addr.0 >> CLICK_SHIFT
    }

    /// Converts frame to raw address, consuming ownership without freeing.
    ///
    /// Caller is responsible for manually freeing the frame later.
    pub(crate) fn into_raw(self) -> PhysAddr {
        let addr = self.addr;
        mem::forget(self);
        addr
    }

    /// Creates a `PhysFrame` from a raw address.
    ///
    /// # Safety
    /// Caller must ensure:
    /// - `addr` is a valid, allocated physical frame address
    /// - `addr` is not referenced by another `PhysFrame` or data structure
    /// - `addr` was allocated by the specified `allocator`
    pub(crate) unsafe fn from_raw(addr: PhysAddr, allocator: Weak<RefCell<PhysMemAllocator>>) -> Self {
        PhysFrame { addr, allocator }
    }
}

impl Drop for PhysFrame {
    fn drop(&mut self) {
        if let Some(alloc_rc) = self.allocator.upgrade() {
            alloc_rc.borrow_mut().free(self.addr, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phys_frame_alloc_drop() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
        let initial_allocated = allocator.borrow().stats().total_allocated();

        {
            let frame = PhysFrame::alloc(&allocator, AllocFlags::empty()).expect("allocation failed");
            assert!(frame.addr().is_valid());
            assert_eq!(frame.pfn(), frame.addr().0 >> CLICK_SHIFT);
            assert!(allocator.borrow().stats().total_allocated() > initial_allocated);
        }

        assert_eq!(allocator.borrow().stats().total_allocated(), initial_allocated);
    }

    #[test]
    fn test_phys_frame_into_raw() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let addr = {
            let frame = PhysFrame::alloc(&allocator, AllocFlags::empty()).expect("allocation failed");
            frame.into_raw()
        };

        assert!(allocator.borrow().stats().total_allocated() > 0);
        allocator.borrow_mut().free(addr, 1);
    }

    #[test]
    fn test_phys_frame_from_raw() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let addr = allocator.borrow_mut().alloc(1, AllocFlags::empty()).expect("allocation failed");
        let frame = unsafe { PhysFrame::from_raw(addr, Rc::downgrade(&allocator)) };
        assert_eq!(frame.addr(), addr);

        drop(frame);
        assert_eq!(allocator.borrow().stats().total_allocated(), 0);
    }

    #[test]
    fn test_multiple_phys_frames() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));

        let frames: Vec<_> = (0..10)
            .map(|_| PhysFrame::alloc(&allocator, AllocFlags::empty()).expect("alloc failed"))
            .collect();

        for (i, frame) in frames.iter().enumerate() {
            assert!(frame.addr().is_valid(), "frame {} should have valid addr", i);
        }

        assert_eq!(allocator.borrow().stats().total_allocated(), 10 * 4096);
        drop(frames);
        assert_eq!(allocator.borrow().stats().total_allocated(), 0);
    }

    #[test]
    fn test_phys_frame_zero_flag() {
        let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
        let frame = PhysFrame::alloc(&allocator, AllocFlags::ZERO).expect("allocation failed");
        assert!(frame.addr().is_valid());
    }

    #[test]
    fn test_allocator_dropped_before_frame() {
        let frame = {
            let allocator = Rc::new(RefCell::new(PhysMemAllocator::new(512 * 1024 * 1024)));
            PhysFrame::alloc(&allocator, AllocFlags::empty()).expect("alloc failed")
        };
        drop(frame);
    }
}
