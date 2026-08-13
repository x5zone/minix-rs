//! x86-64 page table entry definitions
//!
//! Defines PTE/PDE bit fields, index calculations, and entry construction
//! functions for the x86-64 architecture. These are internal details of
//! the `X86_64Paging` implementation and are not exposed to the OS layer.

use crate::paging::PageFlags;
use minix_types::PhysBytes;

pub const PML4_ENTRIES: usize = 512;
pub const PDPT_ENTRIES: usize = 512;
pub const PD_ENTRIES: usize = 512;
pub const PT_ENTRIES: usize = 512;

pub const PAGE_SIZE: usize = 4096;
pub const HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;
pub const GIANT_PAGE_SIZE: usize = 1024 * 1024 * 1024;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_WRITE_THROUGH: u64 = 1 << 3;
const PTE_NO_CACHE: u64 = 1 << 4;
const PTE_ACCESSED: u64 = 1 << 5;
const PTE_DIRTY: u64 = 1 << 6;
#[allow(dead_code)] // huge page PTE flag; not yet wired to all call sites
const PTE_HUGE: u64 = 1 << 7;
const PTE_GLOBAL: u64 = 1 << 8;
const PTE_NO_EXECUTE: u64 = 1 << 63;

const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

pub fn pml4_index(vaddr: u64) -> usize {
    ((vaddr >> 39) & 0x1FF) as usize
}

pub fn pdpt_index(vaddr: u64) -> usize {
    ((vaddr >> 30) & 0x1FF) as usize
}

pub fn pd_index(vaddr: u64) -> usize {
    ((vaddr >> 21) & 0x1FF) as usize
}

pub fn pt_index(vaddr: u64) -> usize {
    ((vaddr >> 12) & 0x1FF) as usize
}

pub fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = 0u64;
    if flags.contains(PageFlags::PRESENT) { pte |= PTE_PRESENT; }
    if flags.contains(PageFlags::WRITABLE) { pte |= PTE_WRITABLE; }
    if flags.contains(PageFlags::USER_ACCESSIBLE) { pte |= PTE_USER; }
    if flags.contains(PageFlags::WRITE_THROUGH) { pte |= PTE_WRITE_THROUGH; }
    if flags.contains(PageFlags::NO_CACHE) { pte |= PTE_NO_CACHE; }
    if flags.contains(PageFlags::ACCESSED) { pte |= PTE_ACCESSED; }
    if flags.contains(PageFlags::DIRTY) { pte |= PTE_DIRTY; }
    if flags.contains(PageFlags::GLOBAL) { pte |= PTE_GLOBAL; }
    if !flags.contains(PageFlags::EXECUTABLE) { pte |= PTE_NO_EXECUTE; }
    pte
}

pub fn pte_to_flags(pte: u64) -> PageFlags {
    let mut flags = PageFlags::empty();
    if pte & PTE_PRESENT != 0 { flags |= PageFlags::PRESENT; }
    if pte & PTE_WRITABLE != 0 { flags |= PageFlags::WRITABLE; }
    if pte & PTE_USER != 0 { flags |= PageFlags::USER_ACCESSIBLE; }
    if pte & PTE_WRITE_THROUGH != 0 { flags |= PageFlags::WRITE_THROUGH; }
    if pte & PTE_NO_CACHE != 0 { flags |= PageFlags::NO_CACHE; }
    if pte & PTE_ACCESSED != 0 { flags |= PageFlags::ACCESSED; }
    if pte & PTE_DIRTY != 0 { flags |= PageFlags::DIRTY; }
    if pte & PTE_GLOBAL != 0 { flags |= PageFlags::GLOBAL; }
    if pte & PTE_NO_EXECUTE == 0 { flags |= PageFlags::EXECUTABLE; }
    flags
}

pub fn make_pte(paddr: PhysBytes, flags: PageFlags) -> u64 {
    (paddr.0 & ADDR_MASK) | flags_to_pte(flags)
}

pub fn pte_paddr(pte: u64) -> PhysBytes {
    PhysBytes(pte & ADDR_MASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_calculation() {
        let vaddr = 0xFFFF_8000_0040_1000u64;
        assert_eq!(pml4_index(vaddr), 256);
        assert_eq!(pdpt_index(vaddr), 0);
        assert_eq!(pd_index(vaddr), 2);
        assert_eq!(pt_index(vaddr), 1);
    }

    #[test]
    fn test_flags_roundtrip() {
        let flags = PageFlags::read_write();
        let pte = flags_to_pte(flags);
        let recovered = pte_to_flags(pte);
        assert!(recovered.contains(PageFlags::PRESENT));
        assert!(recovered.contains(PageFlags::WRITABLE));
        assert!(recovered.contains(PageFlags::USER_ACCESSIBLE));
        // read_write() does NOT set EXECUTABLE — x86-64 NX bit is set
        assert!(!recovered.contains(PageFlags::EXECUTABLE));
    }

    #[test]
    fn test_no_execute_roundtrip() {
        let flags = PageFlags::kernel_read_only();
        let pte = flags_to_pte(flags);
        assert!(pte & PTE_NO_EXECUTE != 0);
        let recovered = pte_to_flags(pte);
        assert!(!recovered.contains(PageFlags::EXECUTABLE));
    }

    #[test]
    fn test_make_pte() {
        let paddr = PhysBytes(0x2000);
        let flags = PageFlags::read_write();
        let pte = make_pte(paddr, flags);
        assert_eq!(pte_paddr(pte), paddr);
        assert!(pte & PTE_PRESENT != 0);
        assert!(pte & PTE_WRITABLE != 0);
    }
}
