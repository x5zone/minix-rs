use core::fmt;
use super::CLICK_SIZE;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysBytes(u64);

impl PhysBytes {
    pub fn new(addr: u64) -> Self {
        assert!(addr % CLICK_SIZE as u64 == 0, "PhysBytes must be page-aligned, got {addr:#x}");
        PhysBytes(addr)
    }

    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    pub fn from_page_index(idx: usize) -> Self {
        PhysBytes((idx as u64) * CLICK_SIZE as u64)
    }

    pub fn page_index(&self) -> usize {
        (self.0 as usize) / CLICK_SIZE
    }

    pub fn add(&self, offset: usize) -> Self {
        let new_addr = self.0 + offset as u64;
        assert!(new_addr >= self.0, "PhysBytes::add overflow: {:#x} + {:#x}", self.0, offset);
        PhysBytes(new_addr)
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageAllocFlags: u32 {
        const CLEAR = 0x01;
        const CONTIG = 0x02;
        const ALIGN64K = 0x04;
        const LOWER16MB = 0x08;
        const LOWER1MB = 0x10;
        const ALIGN16K = 0x40;
    }
}

impl Default for PageAllocFlags {
    fn default() -> Self {
        PageAllocFlags::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    OutOfMemory,
    LowMemoryExhausted,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::OutOfMemory => write!(f, "out of memory"),
            AllocError::LowMemoryExhausted => write!(f, "low memory exhausted"),
        }
    }
}
