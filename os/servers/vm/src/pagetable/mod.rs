//! Page table management module.
//!
//! Provides the page table type alias and related constants.

use crate::phys_mem::PhysAddr;
use minix_types::VirBytes;

/// Page table type using the current architecture implementation.
///
/// This is a type alias to the architecture-specific paging implementation
/// from `minix_arch`. VM process code should use this type rather than
/// depending directly on `minix_arch` types.
pub(crate) type PageTable = minix_arch::CurrentPaging;

pub(crate) const PAGE_SIZE: usize = 4096;

pub(crate) const PT_ENTRIES: usize = 1024;

pub(crate) const PD_ENTRIES: usize = 1024;

pub(crate) const PAGE_SHIFT: usize = 12;

pub(crate) const PD_SHIFT: usize = 22;

pub(crate) const PT_MASK: usize = 0x3FF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PtFlags(u32);

impl PtFlags {
    pub(crate) const PRESENT: u32 = 0x001;
    pub(crate) const WRITE: u32 = 0x002;
    pub(crate) const USER: u32 = 0x004;
    pub(crate) const PWT: u32 = 0x008;
    pub(crate) const PCD: u32 = 0x010;
    pub(crate) const ACCESSED: u32 = 0x020;
    pub(crate) const DIRTY: u32 = 0x040;
    pub(crate) const BIGPAGE: u32 = 0x080;
    pub(crate) const GLOBAL: u32 = 0x100;

    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    pub(crate) const fn new(flags: u32) -> Self {
        Self(flags)
    }

    pub(crate) fn contains(&self, flags: u32) -> bool {
        (self.0 & flags) == flags
    }

    pub(crate) fn insert(&mut self, flags: u32) {
        self.0 |= flags;
    }

    pub(crate) fn remove(&mut self, flags: u32) {
        self.0 &= !flags;
    }

    pub(crate) fn bits(&self) -> u32 {
        self.0
    }
}

impl Default for PtFlags {
    fn default() -> Self {
        Self::empty()
    }
}

// ---- Page table helper functions ----
// These are standalone utility functions for page table operations.
// The PageTable type itself is a type alias to minix_arch::CurrentPaging.

pub(crate) fn pde_index(vaddr: VirBytes) -> usize {
    (vaddr.0 as usize) >> PD_SHIFT
}

pub(crate) fn pte_index(vaddr: VirBytes) -> usize {
    ((vaddr.0 as usize) >> PAGE_SHIFT) & PT_MASK
}

pub(crate) fn page_offset(vaddr: VirBytes) -> usize {
    (vaddr.0 as usize) & (PAGE_SIZE - 1)
}

pub(crate) fn phys_to_pfn(phys: PhysAddr) -> u32 {
    (phys.as_u64() >> PAGE_SHIFT) as u32
}

pub(crate) fn pfn_to_phys(pfn: u32) -> PhysAddr {
    PhysAddr::new((pfn as u64) << PAGE_SHIFT)
}

pub(crate) fn make_pte(phys: PhysAddr, flags: PtFlags) -> u32 {
    (phys_to_pfn(phys) << PAGE_SHIFT) | (flags.bits() & 0xFFF)
}

pub(crate) fn make_pde(pt_phys: PhysAddr, flags: PtFlags) -> u32 {
    (phys_to_pfn(pt_phys) << PAGE_SHIFT) | (flags.bits() & 0xFFF)
}

pub(crate) fn pte_phys(pte: u32) -> PhysAddr {
    PhysAddr::new(((pte as u64) & !0xFFF) as u64)
}

pub(crate) fn pte_flags(pte: u32) -> PtFlags {
    PtFlags(pte & 0xFFF)
}

pub(crate) fn is_present(pte: u32) -> bool {
    (pte & PtFlags::PRESENT) != 0
}

pub(crate) fn page_align(addr: VirBytes) -> VirBytes {
    VirBytes((addr.0 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1))
}

pub(crate) fn page_align_down(addr: VirBytes) -> VirBytes {
    VirBytes(addr.0 & !(PAGE_SIZE as u64 - 1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageTableError {
    InvalidAddress,
    NotMapped,
    PermissionDenied,
    AllocationFailed,
}

impl core::fmt::Display for PageTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidAddress => write!(f, "Invalid address"),
            Self::NotMapped => write!(f, "Page not mapped"),
            Self::PermissionDenied => write!(f, "Permission denied"),
            Self::AllocationFailed => write!(f, "Allocation failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_constants() {
        assert_eq!(PAGE_SIZE, 4096);
        assert_eq!(PT_ENTRIES, 1024);
        assert_eq!(PD_ENTRIES, 1024);
        assert_eq!(1 << PAGE_SHIFT, PAGE_SIZE);
    }

    #[test]
    fn test_pt_flags() {
        let mut flags = PtFlags::empty();
        assert!(!flags.contains(PtFlags::PRESENT));
        
        flags.insert(PtFlags::PRESENT | PtFlags::WRITE);
        assert!(flags.contains(PtFlags::PRESENT));
        assert!(flags.contains(PtFlags::WRITE));
        assert!(!flags.contains(PtFlags::USER));
        
        flags.remove(PtFlags::WRITE);
        assert!(!flags.contains(PtFlags::WRITE));
    }

    #[test]
    fn test_address_calculation() {
        let vaddr = VirBytes(0x12345678);

        assert_eq!(pde_index(vaddr), 0x48);
        assert_eq!(pte_index(vaddr), 0x345);
        assert_eq!(page_offset(vaddr), 0x678);
    }

    #[test]
    fn test_pte_operations() {
        let phys = PhysAddr::new(0x12345000);
        let flags = PtFlags::new(PtFlags::PRESENT | PtFlags::WRITE);

        let pte = make_pte(phys, flags);
        assert!(is_present(pte));
        assert!(pte_flags(pte).contains(PtFlags::WRITE));
        assert_eq!(pte_phys(pte), phys);
    }

    #[test]
    fn test_page_align() {
        assert_eq!(page_align(VirBytes(0x1234)), VirBytes(0x2000));
        assert_eq!(page_align(VirBytes(0x1000)), VirBytes(0x1000));
        assert_eq!(page_align_down(VirBytes(0x1234)), VirBytes(0x1000));
    }
}
