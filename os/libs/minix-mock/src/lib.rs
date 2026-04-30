//! Minix-RS Mock Library.
//!
//! Hardware mock implementation for development testing.

#![no_std]

use core::marker::PhantomData;

/// Mock memory management unit.
pub struct MockMMU {
    _phantom: PhantomData<()>,
}

impl MockMMU {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    /// Mock page table allocation.
    pub fn alloc_page_table(&self) -> MockPageTable {
        MockPageTable::new()
    }

    /// Mock address mapping.
    pub fn map(&self, vaddr: VirtAddr, paddr: PhysAddr, flags: PageFlags) {
        log::debug!("mock map: {:?} -> {:?}, flags={:?}", vaddr, paddr, flags);
    }

    /// Mock address unmapping.
    pub fn unmap(&self, vaddr: VirtAddr) {
        log::debug!("mock unmap: {:?}", vaddr);
    }
}

/// Mock page table.
pub struct MockPageTable;

impl MockPageTable {
    pub fn new() -> Self {
        Self
    }
}

/// Mock interrupt controller.
pub struct MockPIC;

impl MockPIC {
    pub fn new() -> Self {
        Self
    }

    pub fn enable_irq(&self, irq: u8) {
        log::debug!("mock enable_irq: {}", irq);
    }

    pub fn disable_irq(&self, irq: u8) {
        log::debug!("mock disable_irq: {}", irq);
    }

    pub fn send_eoi(&self, irq: u8) {
        log::debug!("mock send_eoi: {}", irq);
    }
}

/// Mock timer.
pub struct MockTimer;

impl MockTimer {
    pub fn new() -> Self {
        Self
    }

    pub fn set_frequency(&self, hz: u32) {
        log::debug!("mock set_frequency: {} Hz", hz);
    }

    pub fn get_ticks(&self) -> u64 {
        0 // Mock value
    }
}

/// Mock serial port.
pub struct MockSerial;

impl MockSerial {
    pub fn new() -> Self {
        Self
    }

    pub fn write(&self, byte: u8) {
        log::debug!("mock serial write: 0x{:02x}", byte);
    }

    pub fn read(&self) -> Option<u8> {
        None // Mock no input
    }
}

/// Virtual address.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct VirtAddr(pub u64);

/// Physical address.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct PhysAddr(pub u64);

/// Page flags.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct PageFlags(pub u64);

impl PageFlags {
    pub const PRESENT: PageFlags = PageFlags(1 << 0);
    pub const WRITABLE: PageFlags = PageFlags(1 << 1);
    pub const USER: PageFlags = PageFlags(1 << 2);
    pub const HUGE: PageFlags = PageFlags(1 << 7);
    pub const NO_EXECUTE: PageFlags = PageFlags(1 << 63);

    pub fn contains(&self, other: PageFlags) -> bool {
        self.0 & other.0 != 0
    }
}

impl core::ops::BitOr for PageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        PageFlags(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_mmu() {
        let mmu = MockMMU::new();
        let pt = mmu.alloc_page_table();
        mmu.map(VirtAddr(0x1000), PhysAddr(0x2000), PageFlags::PRESENT);
    }

    #[test]
    fn test_page_flags() {
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE;
        assert!(flags.contains(PageFlags::PRESENT));
        assert!(flags.contains(PageFlags::WRITABLE));
    }
}
