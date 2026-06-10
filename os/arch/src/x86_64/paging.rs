//! x86-64 paging implementation
//!
//! Boot-stage implementation: new_from_page, enable, map_huge work with UEFI
//! identity-mapped memory (VA = PA). Runtime methods (map, unmap, query, etc.)
//! require Direct Map and remain todo!() until that layer is ready.

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use minix_types::{PhysBytes, VirBytes};
use core::arch::asm;

bitflags::bitflags! {
    /// x86-64 page table entry flags (hardware encoding).
    ///
    /// This is the hardware-level PTE bit layout for Intel/AMD long mode,
    /// separate from the OS-semantic `PageFlags`. Key differences:
    /// - Executable is the *absence* of the NX (No-eXecute) bit (bit 63)
    /// - PS bit (bit 7) indicates huge page at PDPT (1GB) or PD (2MB) level
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct X64PteFlags: u64 {
        const PRESENT  = 1 << 0;   // Present
        const WRITABLE = 1 << 1;   // Read/Write
        const USER     = 1 << 2;   // User/Supervisor
        const PS       = 1 << 7;   // Page Size (huge page indicator)
        const GLOBAL   = 1 << 8;   // Global page
        const NX       = 1 << 63;  // No Execute
    }
}

const PML4_SHIFT: u32 = 39;
const PDPT_SHIFT: u32 = 30;
const PD_SHIFT: u32 = 21;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

fn pml4_index(vaddr: u64) -> usize {
    ((vaddr >> PML4_SHIFT) & 0x1FF) as usize
}

fn pdpt_index(vaddr: u64) -> usize {
    ((vaddr >> PDPT_SHIFT) & 0x1FF) as usize
}

fn pd_index(vaddr: u64) -> usize {
    ((vaddr >> PD_SHIFT) & 0x1FF) as usize
}

unsafe fn read_entry(table: *mut u64, idx: usize) -> u64 {
    core::ptr::read_volatile(table.add(idx))
}

unsafe fn write_entry(table: *mut u64, idx: usize, val: u64) {
    core::ptr::write_volatile(table.add(idx), val);
    // NOTE: invlpg with address 0 is a conservative flush. For intermediate
    // page table entries (PML4/PDPT/PD), no TLB entry exists yet because
    // the mapping hasn't been used. For final PTE entries, the correct
    // virtual address should ideally be used. However, during boot the
    // page table is not yet active (CR3 hasn't been switched), so this
    // flush is effectively a no-op. After boot, map() should use the
    // actual virtual address for correctness.
    asm!("invlpg [{}]", in(reg) 0u64, options(nostack, preserves_flags));
}

unsafe fn phys_to_ptr(phys: u64) -> *mut u64 {
    phys as *mut u64
}

/// Translate OS-semantic `PageFlags` into x86-64 hardware PTE flags.
///
/// x86-64 uses inverted semantics for execute permission: the NX (No-eXecute)
/// bit in bit 63 must be *cleared* for executable pages. All other flags
/// use normal (set=enabled) semantics.
fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = X64PteFlags::PRESENT;
    if flags.contains(PageFlags::WRITABLE) { pte |= X64PteFlags::WRITABLE; }
    if flags.contains(PageFlags::USER_ACCESSIBLE) { pte |= X64PteFlags::USER; }
    if flags.contains(PageFlags::GLOBAL) { pte |= X64PteFlags::GLOBAL; }
    // NX is inverted: set NX when NOT executable
    if !flags.contains(PageFlags::EXECUTABLE) { pte |= X64PteFlags::NX; }
    pte.bits()
}

pub struct X86_64Paging {
    root_paddr: u64,
}

impl Paging for X86_64Paging {
    const PAGE_SIZE: usize = 4096;

    /// Uses a pre-allocated physical page for the root PML4 table.
    ///
    /// During boot, the caller (UEFI shim) allocates a page via
    /// `AllocatePages` and passes it here. There is no kernel page allocator
    /// at this point, so `new()` (which would allocate internally) cannot be
    /// used. See `Paging::new()` for the runtime variant.
    fn new_from_page(root_page: PhysBytes) -> Self {
        // SAFETY: root_page must be a valid, 4KB-aligned physical address
        // that is writable through the current page table (UEFI identity map
        // during boot). The page must not be aliased by any other live
        // reference. phys_to_ptr converts the physical address to a virtual
        // address using the UEFI identity mapping.
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        // SAFETY: ptr points to a 4KB-aligned, 512-entry PML4 table.
        // write_bytes zeroes all entries, marking them "not present".
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0 }
    }

    unsafe fn enable(&self) -> PhysBytes {
        unsafe {
            // Switch CR3 to our own root page table.
            // UEFI already runs in long mode (CR0.PG=1) with its own page
            // table; we must replace it with ours, which contains both the
            // identity and kernel high mappings set up by map_huge().
            asm!("mov cr3, {}", in(reg) self.root_paddr);
            // Ensure WP (Write Protect, CR0 bit 16) is set so the kernel
            // cannot write read-only pages (W^X enforcement). UEFI firmware
            // does not guarantee WP is enabled.
            let mut cr0: u64;
            asm!("mov {}, cr0", out(reg) cr0);
            cr0 |= 1 << 31;
            cr0 |= 1 << 16;
            asm!("mov cr0, {}", in(reg) cr0);
        }
        PhysBytes(self.root_paddr)
    }

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        todo!("x86-64 paging: implement new()")
    }

    unsafe fn destroy(&mut self) {
        todo!("x86-64 paging: implement destroy()")
    }

    fn map(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("x86-64 paging: implement map()")
    }

    fn remap(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _flags: PageFlags,
    ) -> Result<Option<(PhysBytes, PageFlags)>, PageTableError> {
        todo!("x86-64 paging: implement remap()")
    }

    fn unmap(&mut self, _vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        todo!("x86-64 paging: implement unmap()")
    }

    fn update_flags(
        &mut self,
        _vaddr: VirBytes,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("x86-64 paging: implement update_flags()")
    }

    fn query(&self, _vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        todo!("x86-64 paging: implement query()")
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root_paddr)
    }

    unsafe fn switch(&self) {
        unsafe {
            asm!("mov cr3, {}", in(reg) self.root_paddr, options(att_syntax));
        }
    }

    unsafe fn flush_tlb(&self) {
        unsafe {
            asm!("mov cr3, {}", in(reg) self.root_paddr, options(att_syntax));
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("invlpg [{}]", in(reg) vaddr.0, options(nostack, preserves_flags));
        }
    }
}

impl HugePages for X86_64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21];
    const HUGE_PAGE_SIZE: u64 = 1 << 30;
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
    /// PS bit (bit 7) indicates a huge page at PDPT level (1GB) or PD level (2MB).
    /// The same bit position is used for both sizes; the level determines the
    /// actual page size.
    const PTE_HUGE_IDENTIFIER_BIT: u64 = X64PteFlags::PS.bits();

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        let pte_flags = flags_to_pte(flags);
        let shift = if size >= 1 << 30 { PDPT_SHIFT } else { PD_SHIFT };
        // PS bit (bit 7) is the same for both 1GB (PDPT.PS) and 2MB (PD.PS) pages.
        let huge_flag = X64PteFlags::PS.bits();

        let pml4 = unsafe { phys_to_ptr(self.root_paddr) };
        let i4 = pml4_index(vaddr.0);
        let e4 = unsafe { read_entry(pml4, i4) };

        let pdpt = if e4 & X64PteFlags::PRESENT.bits() != 0 {
            (e4 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(pml4, i4, page | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i3 = pdpt_index(vaddr.0);
        let e3 = unsafe { read_entry(pdpt, i3) };

        if shift == PDPT_SHIFT {
            unsafe { write_entry(pdpt, i3, (paddr.0 & ADDR_MASK) | pte_flags | huge_flag) };
            return Ok(());
        }

        // Check if PDPT entry is already a 1GB leaf (PRESENT + PS both set).
        // If so, demoting it would corrupt the existing mapping.
        if e3 & X64PteFlags::PRESENT.bits() != 0 && e3 & X64PteFlags::PS.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }

        let pd = if e3 & X64PteFlags::PRESENT.bits() != 0 {
            (e3 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(pdpt, i3, page | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i2 = pd_index(vaddr.0);
        unsafe { write_entry(pd, i2, (paddr.0 & ADDR_MASK) | pte_flags | huge_flag) };
        Ok(())
    }

    /// Whether the CPU supports 1 GB huge pages (CPUID.80000001H:EDX.GBPAGES bit 26).
    fn supports_1gb_page() -> bool {
        let mut edx: u32;
        unsafe {
            asm!(
                "push rbx",
                "mov eax, 0x80000001",
                "cpuid",
                "pop rbx",
                out("edx") edx,
                out("eax") _,
                out("ecx") _,
                options(nomem, nostack, preserves_flags)
            );
        }
        (edx & (1 << 26)) != 0
    }
}
