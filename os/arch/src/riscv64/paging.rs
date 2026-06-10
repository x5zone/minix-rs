//! RISC-V 64-bit (Sv39) paging implementation
//!
//! Boot-stage implementation: new_from_page, enable, map_huge work with identity
//! mapping (VA = PA). Runtime methods (map, unmap, query, etc.) require Direct
//! Map and remain todo!() until that layer is ready.
//!
//! # Sv39 page table architecture (3-level)
//!
//! | Level | Shift | Entry size | Page size   |
//! |-------|-------|------------|-------------|
//! | L2    | 30    | 1 GB       | 1 GB block  |
//! | L1    | 21    | 2 MB       | 2 MB block  |
//! | L0    | 12    | 4 KB       | 4 KB page   |
//!
//! # PTE format
//!
//! - Bit  0: V (Valid)
//! - Bit  1: R (Readable)
//! - Bit  2: W (Writable) — requires R=1
//! - Bit  3: X (Executable)
//! - Bit  4: U (User mode accessible)
//! - Bit  5: G (Global)
//! - Bit  6: A (Accessed)
//! - Bit  7: D (Dirty)
//! - Bits 53:10: PPN (Physical Page Number = PA >> 12)
//!
//! A PTE is a **leaf** if any of R/W/X is set, otherwise it is a **table**
//! pointer (non-leaf).

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use minix_types::{PhysBytes, VirBytes};
use core::arch::asm;

bitflags::bitflags! {
    /// RISC-V Sv39 page table entry flags (hardware encoding).
    ///
    /// This is the hardware-level PTE bit layout, separate from the
    /// OS-semantic `PageFlags`. Key constraints:
    /// - W=1 requires R=1 (RISC-V Privileged Spec §4.3.1)
    /// - R=W=X=0 indicates a non-leaf (table pointer) entry
    /// - R=0,X=1 is execute-only (optional, requires menvcfg.CBIE)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Sv39PteFlags: u64 {
        const V = 1 << 0;  // Valid
        const R = 1 << 1;  // Readable
        const W = 1 << 2;  // Writable (requires R=1)
        const X = 1 << 3;  // Executable
        const U = 1 << 4;  // User mode accessible
        const G = 1 << 5;  // Global
        const A = 1 << 6;  // Accessed
        const D = 1 << 7;  // Dirty
    }
}

const L2_SHIFT: u32 = 30;
const L1_SHIFT: u32 = 21;
/// Mask for the PPN field within a PTE (bits 53:10).
/// In RISC-V Sv39, PTE[53:10] = PPN[43:0], where PPN = PA >> 12.
/// So PTE_PPN = (PA >> 12) << 10 = PA >> 2 (since PA[11:0] = 0).
const PTE_PPN_MASK: u64 = 0x003F_FFFF_FFFF_FC00;

/// Convert a physical address to the PPN field in a PTE.
/// PTE[53:10] = PPN = PA >> 12, so the PTE PPN bits = (PA >> 12) << 10 = PA >> 2.
#[inline]
fn paddr_to_pte(paddr: u64) -> u64 {
    (paddr >> 2) & PTE_PPN_MASK
}

/// Extract the physical address from a PTE's PPN field.
/// PA = PPN << 12 = (PTE[53:10]) << 12 = (PTE & PTE_PPN_MASK) >> 10 << 12.
#[inline]
fn pte_to_paddr(pte: u64) -> u64 {
    ((pte & PTE_PPN_MASK) >> 10) << 12
}

/// Check whether a PTE is a leaf entry (has any of R/W/X set).
/// In Sv39, R=W=X=0 means non-leaf (table pointer); any of R/W/X set means leaf.
#[inline]
fn pte_is_leaf(pte: u64) -> bool {
    let flags = Sv39PteFlags::from_bits_truncate(pte);
    flags.intersects(Sv39PteFlags::R | Sv39PteFlags::W | Sv39PteFlags::X)
}

fn l2_index(vaddr: u64) -> usize {
    ((vaddr >> L2_SHIFT) & 0x1FF) as usize
}

fn l1_index(vaddr: u64) -> usize {
    ((vaddr >> L1_SHIFT) & 0x1FF) as usize
}

unsafe fn read_entry(table: *mut u64, idx: usize) -> u64 {
    core::ptr::read_volatile(table.add(idx))
}

unsafe fn write_entry(table: *mut u64, idx: usize, val: u64) {
    core::ptr::write_volatile(table.add(idx), val);
}

unsafe fn phys_to_ptr(phys: u64) -> *mut u64 {
    phys as *mut u64
}

/// Translate OS-semantic `PageFlags` into Sv39 hardware PTE flags.
///
/// Boot stage: all pages are readable (R=1) because:
/// 1. RISC-V requires R=1 when W=1 (W-implies-R, Privileged Spec §4.3.1)
/// 2. Execute-only pages (R=0,X=1) are optional and require menvcfg.CBIE;
///    boot stage does not set this CSR, so R=1 is required for X=1 as well.
/// 3. Boot mappings (identity + kernel) are always RWX, so R=1 is correct.
///
/// When runtime `map()` is implemented, this function should be revisited
/// to support execute-only pages if the hardware supports them.
fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = Sv39PteFlags::V | Sv39PteFlags::A | Sv39PteFlags::D;
    // R=1 always set in boot stage (see function doc above).
    pte |= Sv39PteFlags::R;
    if flags.contains(PageFlags::WRITABLE) {
        pte |= Sv39PteFlags::W;
    }
    if flags.contains(PageFlags::EXECUTABLE) {
        pte |= Sv39PteFlags::X;
    }
    if flags.contains(PageFlags::USER_ACCESSIBLE) {
        pte |= Sv39PteFlags::U;
    }
    if flags.contains(PageFlags::GLOBAL) {
        pte |= Sv39PteFlags::G;
    }
    pte.bits()
}

pub struct Riscv64Paging {
    root_paddr: u64,
}

impl Paging for Riscv64Paging {
    const PAGE_SIZE: usize = 4096;

    /// Uses a pre-allocated physical page for the root L2 table.
    ///
    /// During boot, the OpenSBI or UEFI shim passes the page for the root
    /// (Sv39 L2) page table. There is no kernel page allocator at this point,
    /// so `new()` (which would allocate internally) cannot be used.
    /// See `Paging::new()` for the runtime variant.
    fn new_from_page(root_page: PhysBytes) -> Self {
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0 }
    }

    unsafe fn enable(&self) -> PhysBytes {
        unsafe {
            // satp = (MODE_Sv39 << 60) | (ASID << 44) | (root_paddr >> 12).
            // Unlike x86-64/aarch64 where UEFI firmware already runs with
            // paging enabled (and we switch to our own page table), OpenSBI
            // on riscv64 does NOT enable the MMU — we enable Sv39 paging
            // from scratch. After this write, all addresses go through the
            // 3-level Sv39 page table rooted at self.root_paddr.
            //
            // Both csrw satp and sfence.vma MUST be in the same asm! block
            // to prevent the compiler from inserting memory accesses between
            // them. After csrw satp, the MMU is on and any subsequent memory
            // access goes through the page table. sfence.vma flushes the TLB
            // so the new mappings are visible.
            let satp = (8u64 << 60) | (self.root_paddr >> 12);
            asm!(
                "csrw satp, {0}",
                "sfence.vma",
                in(reg) satp,
            );
        }
        PhysBytes(self.root_paddr)
    }

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        todo!("riscv64 paging: implement new()")
    }

    unsafe fn destroy(&mut self) {
        todo!("riscv64 paging: implement destroy()")
    }

    fn map(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("riscv64 paging: implement map()")
    }

    fn remap(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _flags: PageFlags,
    ) -> Result<Option<(PhysBytes, PageFlags)>, PageTableError> {
        todo!("riscv64 paging: implement remap()")
    }

    fn unmap(&mut self, _vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        todo!("riscv64 paging: implement unmap()")
    }

    fn update_flags(
        &mut self,
        _vaddr: VirBytes,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("riscv64 paging: implement update_flags()")
    }

    fn query(&self, _vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        todo!("riscv64 paging: implement query()")
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root_paddr)
    }

    unsafe fn switch(&self) {
        unsafe {
            let satp = (8u64 << 60) | (self.root_paddr >> 12);
            asm!("csrw satp, {}", in(reg) satp);
        }
    }

    unsafe fn flush_tlb(&self) {
        unsafe {
            asm!("sfence.vma");
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("sfence.vma {}", in(reg) vaddr.0);
        }
    }
}

impl HugePages for Riscv64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21];
    const HUGE_PAGE_SIZE: u64 = 1 << 30;
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
    /// RISC-V Sv39 does not use a dedicated PTE flag to indicate huge pages;
    /// the page size is determined by which page table level the entry is at.
    /// This constant is 0 to satisfy the `HugePages` trait interface.
    const PTE_HUGE_IDENTIFIER_BIT: u64 = 0;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        let pte_flags = flags_to_pte(flags);

        let l2 = unsafe { phys_to_ptr(self.root_paddr) };
        let i2 = l2_index(vaddr.0);
        let e2 = unsafe { read_entry(l2, i2) };

        if size >= 1 << 30 {
            // 1GB huge page: write directly into the L2 (root) table.
            unsafe { write_entry(l2, i2, paddr_to_pte(paddr.0) | pte_flags) };
            return Ok(());
        }

        // 2MB huge page: need an L1 page table under this L2 entry.
        let l1 = if e2 & Sv39PteFlags::V.bits() != 0 {
            // L2 entry is valid — check whether it's a leaf or a table pointer.
            if pte_is_leaf(e2) {
                // L2 entry is already a 1GB leaf (R/W/X set). Demoting it to
                // a table pointer would silently corrupt the existing 1GB mapping.
                // Return AlreadyMapped to signal the conflict.
                return Err(PageTableError::AlreadyMapped);
            }
            // Non-leaf entry: PPN points to an existing L1 page table.
            pte_to_paddr(e2) as *mut u64
        } else {
            // Allocate a new L1 page table.
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l2, i2, paddr_to_pte(page) | Sv39PteFlags::V.bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i1 = l1_index(vaddr.0);
        unsafe { write_entry(l1, i1, paddr_to_pte(paddr.0) | pte_flags) };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddr_to_pte_roundtrip() {
        for &paddr in &[0x8000_0000u64, 0x8020_0000, 0x0, 0x4000_0000] {
            let pte = paddr_to_pte(paddr);
            let recovered = pte_to_paddr(pte);
            assert_eq!(recovered, paddr,
                "paddr_to_pte roundtrip failed: 0x{:x} → 0x{:x} → 0x{:x}",
                paddr, pte, recovered);
        }
    }

    #[test]
    fn test_flags_to_pte_kernel_read_write_exec() {
        let flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let pte = flags_to_pte(flags);
        let pte_flags = Sv39PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Sv39PteFlags::V), "PTE must be valid");
        assert!(pte_flags.contains(Sv39PteFlags::R), "PTE must be readable (R=1 required for W=1)");
        assert!(pte_flags.contains(Sv39PteFlags::W), "PTE must be writable");
        assert!(pte_flags.contains(Sv39PteFlags::X), "PTE must be executable");
        assert!(pte_flags.contains(Sv39PteFlags::A), "PTE must have Accessed flag");
        assert!(pte_flags.contains(Sv39PteFlags::D), "PTE must have Dirty flag");
        assert!(pte_flags.contains(Sv39PteFlags::G), "PTE must be Global");
    }

    #[test]
    fn test_flags_to_pte_no_wx_combination() {
        // RISC-V forbids W=1,R=0. flags_to_pte always sets R=1 (boot stage),
        // so W=1 always implies R=1.
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE;
        let pte = flags_to_pte(flags);
        let pte_flags = Sv39PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Sv39PteFlags::R), "W=1 requires R=1 in Sv39");
        assert!(pte_flags.contains(Sv39PteFlags::W));
    }

    #[test]
    fn test_pte_is_leaf() {
        // Non-leaf: V=1, R=W=X=0 (table pointer)
        let non_leaf = Sv39PteFlags::V.bits();
        assert!(!pte_is_leaf(non_leaf), "V-only PTE is non-leaf");

        // Leaf: V=1, R=1 (readable page)
        let leaf_r = (Sv39PteFlags::V | Sv39PteFlags::R).bits();
        assert!(pte_is_leaf(leaf_r), "V+R PTE is leaf");

        // Leaf: V=1, X=1, R=1 (execute page)
        let leaf_rx = (Sv39PteFlags::V | Sv39PteFlags::R | Sv39PteFlags::X).bits();
        assert!(pte_is_leaf(leaf_rx), "V+R+X PTE is leaf");

        // Leaf: V=1, R+W+X (full access page)
        let leaf_rwx = (Sv39PteFlags::V | Sv39PteFlags::R | Sv39PteFlags::W | Sv39PteFlags::X).bits();
        assert!(pte_is_leaf(leaf_rwx), "V+R+W+X PTE is leaf");

        // Invalid: V=0
        assert!(!pte_is_leaf(0), "V=0 PTE is neither leaf nor table");
    }

    #[test]
    fn test_l2_index_dram_base() {
        let idx = l2_index(0x8000_0000);
        assert_eq!(idx, 2, "DRAM_BASE should be at L2 index 2");
    }

    #[test]
    fn test_l1_index_2mb_offset() {
        let idx = l1_index(0x8020_0000);
        assert_eq!(idx, 0x401, "2MB offset from DRAM_BASE should be at L1 index 0x401");
    }

    #[test]
    fn test_paddr_to_pte_dram_base() {
        let pte = paddr_to_pte(0x8000_0000);
        assert_eq!(pte, 0x2000_0000, "paddr_to_pte(0x8000_0000) should be 0x2000_0000");
    }
}
