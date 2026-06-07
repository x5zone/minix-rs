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

const PTE_V: u64 = 1 << 0;
const PTE_R: u64 = 1 << 1;
const PTE_W: u64 = 1 << 2;
const PTE_X: u64 = 1 << 3;
const PTE_U: u64 = 1 << 4;
const PTE_G: u64 = 1 << 5;
const PTE_A: u64 = 1 << 6;
const PTE_D: u64 = 1 << 7;

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

fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = PTE_V | PTE_A | PTE_D;
    pte |= PTE_R;
    if flags.contains(PageFlags::WRITABLE) {
        pte |= PTE_W;
    }
    if flags.contains(PageFlags::EXECUTABLE) {
        pte |= PTE_X;
    }
    if flags.contains(PageFlags::USER_ACCESSIBLE) {
        pte |= PTE_U;
    }
    if flags.contains(PageFlags::GLOBAL) {
        pte |= PTE_G;
    }
    pte
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
    const PTE_HUGE_FLAGS: u64 = 0;

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
            unsafe { write_entry(l2, i2, paddr_to_pte(paddr.0) | pte_flags) };
            return Ok(());
        }

        let l1 = if e2 & PTE_V != 0 {
            pte_to_paddr(e2) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l2, i2, paddr_to_pte(page) | PTE_V) };
            unsafe { phys_to_ptr(page) }
        };

        let i1 = l1_index(vaddr.0);
        unsafe { write_entry(l1, i1, paddr_to_pte(paddr.0) | pte_flags) };
        Ok(())
    }
}