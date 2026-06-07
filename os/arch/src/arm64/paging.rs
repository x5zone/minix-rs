//! ARM64 (aarch64) paging implementation
//!
//! Boot-stage implementation: new_from_page, enable, map_huge work with UEFI
//! identity-mapped memory (VA = PA). Runtime methods (map, unmap, query, etc.)
//! require Direct Map and remain todo!() until that layer is ready.
//!
//! # ARM64 page table architecture (4KB granule, 4-level)
//!
//! | Level | Name | Shift  | Entry size  | Page size |
//! |-------|------|--------|-------------|-----------|
//! | L0    | PGD  | 39     | 512 GB      | —         |
//! | L1    | PUD  | 30     | 1 GB        | 1 GB blk  |
//! | L2    | PMD  | 21     | 2 MB        | 2 MB blk  |
//! | L3    | PTE  | 12     | 4 KB        | 4 KB page |
//!
//! # PTE format (Stage 1, EL1&0)
//!
//! - Bit  0: Valid (1)
//! - Bit  1: Type (0=block/page, 1=table)
//! - Bit  5: AP[1]  (0=EL1 only, 1=EL0+EL1)
//! - Bit  6: AP[2]  (0=writable, 1=read-only at EL1)
//! - Bit 10: AF (Access Flag)
//! - Bit 11: nG (0=global, 1=non-global)
//! - Bit 53: PXN (Privileged Execute Never)
//! - Bit 54: XN  (Execute Never for EL0)

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use minix_types::{PhysBytes, VirBytes};
use core::arch::asm;

const PTE_VALID: u64 = 1 << 0;
const PTE_AP1: u64 = 1 << 5;
const PTE_AP2: u64 = 1 << 6;
const PTE_AF: u64 = 1 << 10;
const PTE_N_G: u64 = 1 << 11;
const PTE_XN: u64 = 1 << 54;

const L0_SHIFT: u32 = 39;
const L1_SHIFT: u32 = 30;
const L2_SHIFT: u32 = 21;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

fn l0_index(vaddr: u64) -> usize {
    ((vaddr >> L0_SHIFT) & 0x1FF) as usize
}

fn l1_index(vaddr: u64) -> usize {
    ((vaddr >> L1_SHIFT) & 0x1FF) as usize
}

fn l2_index(vaddr: u64) -> usize {
    ((vaddr >> L2_SHIFT) & 0x1FF) as usize
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
    let mut pte = PTE_VALID | PTE_AF;
    // ARM64 AP[2] (bit 6): 0 = writable at EL1, 1 = read-only at EL1.
    // So we set AP2 only when the page is NOT writable.
    if !flags.contains(PageFlags::WRITABLE) {
        pte |= PTE_AP2;
    }
    if flags.contains(PageFlags::USER_ACCESSIBLE) {
        pte |= PTE_AP1;
    }
    if !flags.contains(PageFlags::GLOBAL) {
        pte |= PTE_N_G;
    }
    if !flags.contains(PageFlags::EXECUTABLE) {
        pte |= PTE_XN;
    }
    pte
}

pub struct AArch64Paging {
    root_paddr: u64,
}

impl Paging for AArch64Paging {
    const PAGE_SIZE: usize = 4096;

    /// Uses a pre-allocated physical page for the root L0 (PGD) table.
    ///
    /// During boot, the caller (UEFI shim) allocates a page via
    /// `AllocatePages` and passes it here. There is no kernel page allocator
    /// at this point, so `new()` (which would allocate internally) cannot be
    /// used. See `Paging::new()` for the runtime variant.
    fn new_from_page(root_page: PhysBytes) -> Self {
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0 }
    }

    unsafe fn enable(&self) -> PhysBytes {
        unsafe {
            // MAIR_EL1: Attr0 = Normal WBWA (0xFF).
            // UEFI firmware may have set its own MAIR; we override to ensure
            // the sole memory attribute (Normal Write-Back) is available.
            asm!("msr mair_el1, {}", in(reg) 0xFFu64);

            // TCR_EL1: Translation Control Register.
            // 4KB granule, 48-bit VA (T0SZ=16, T1SZ=16), 48-bit PA (IPS=5).
            // UEFI firmware configures TCR for its own page tables; we must
            // reconfigure it for ours (same granule/size is typical but not
            // guaranteed by the UEFI spec).
            let tcr: u64 = (16 << 0)
                | (0 << 6)
                | (0 << 7)
                | (1 << 8)
                | (1 << 10)
                | (3 << 12)
                | (0 << 14)
                | (16 << 16)
                | (0 << 22)
                | (0 << 23)
                | (1 << 24)
                | (1 << 26)
                | (3 << 28)
                | (2 << 30)
                | (5 << 32)
                | (0 << 36)
                | (0 << 37);
            asm!("msr tcr_el1, {}", in(reg) tcr);

            // Set both TTBR0 (user space) and TTBR1 (kernel space) to our
            // root page table. UEFI firmware sets these for its own use;
            // we replace them with ours which contains identity + kernel
            // mappings built by map_huge().
            asm!("msr ttbr0_el1, {}", in(reg) self.root_paddr);
            asm!("msr ttbr1_el1, {}", in(reg) self.root_paddr);

            asm!("dsb sy");
            asm!("isb");

            // Enable MMU (SCTLR_EL1.M = 1).
            // UEFI firmware already has the MMU on, but we must re-enable
            // it after changing MAIR/TCR/TTBR (per ARM ARM, changes only
            // take effect after SCTLR.M is toggled or on a subsequent
            // context synchronization event).
            let mut sctlr: u64;
            asm!("mrs {}, sctlr_el1", out(reg) sctlr);
            sctlr |= 1;
            asm!("msr sctlr_el1, {}", in(reg) sctlr);
            asm!("isb");
        }
        PhysBytes(self.root_paddr)
    }

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        todo!("aarch64 paging: implement new()")
    }

    unsafe fn destroy(&mut self) {
        todo!("aarch64 paging: implement destroy()")
    }

    fn map(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("aarch64 paging: implement map()")
    }

    fn remap(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _flags: PageFlags,
    ) -> Result<Option<(PhysBytes, PageFlags)>, PageTableError> {
        todo!("aarch64 paging: implement remap()")
    }

    fn unmap(&mut self, _vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        todo!("aarch64 paging: implement unmap()")
    }

    fn update_flags(
        &mut self,
        _vaddr: VirBytes,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("aarch64 paging: implement update_flags()")
    }

    fn query(&self, _vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        todo!("aarch64 paging: implement query()")
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root_paddr)
    }

    unsafe fn switch(&self) {
        unsafe {
            asm!("dsb sy");
            asm!("msr ttbr1_el1, {}", in(reg) self.root_paddr);
            asm!("isb");
        }
    }

    unsafe fn flush_tlb(&self) {
        unsafe {
            asm!("dsb sy");
            asm!("tlbi vmalle1is");
            asm!("dsb sy");
            asm!("isb");
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("dsb sy");
            asm!("tlbi vae1is, {}", in(reg) vaddr.0);
            asm!("dsb sy");
            asm!("isb");
        }
    }
}

impl HugePages for AArch64Paging {
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

        let l0 = unsafe { phys_to_ptr(self.root_paddr) };
        let i0 = l0_index(vaddr.0);
        let e0 = unsafe { read_entry(l0, i0) };
        let l1 = if e0 & PTE_VALID != 0 {
            (e0 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l0, i0, page | PTE_VALID | PTE_AF | (1 << 1)) };
            unsafe { phys_to_ptr(page) }
        };

        if size >= 1 << 30 {
            unsafe { write_entry(l1, l1_index(vaddr.0), (paddr.0 & ADDR_MASK) | pte_flags) };
            return Ok(());
        }

        let i1 = l1_index(vaddr.0);
        let e1 = unsafe { read_entry(l1, i1) };
        let l2 = if e1 & PTE_VALID != 0 && e1 & (1 << 1) != 0 {
            (e1 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l1, i1, page | PTE_VALID | PTE_AF | (1 << 1)) };
            unsafe { phys_to_ptr(page) }
        };

        let i2 = l2_index(vaddr.0);
        unsafe { write_entry(l2, i2, (paddr.0 & ADDR_MASK) | pte_flags) };
        Ok(())
    }
}