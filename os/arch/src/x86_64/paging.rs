//! x86-64 paging implementation (placeholder)
//!
//! TODO: Implement real x86-64 page table operations using CR3, PML4, etc.
//! Currently only provides the type alias for `CurrentPaging` resolution.

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use minix_types::{PhysBytes, VirBytes};

pub struct X86_64Paging;

impl Paging for X86_64Paging {
    const PAGE_SIZE: usize = 4096;

    fn new_empty(root_page: PhysBytes) -> Self {
        // C: alloc_pagetable() — pg_utils.c:123
        //     static u32_t pagetables[6][1024]
        //
        // On x86-64 with Direct Map, phys_to_virt() gives us a pointer to the
        // PML4 table. Zero-fill it and return.
        todo!("x86-64 paging: new_empty — needs Direct Map support in arch crate")
    }

    unsafe fn enable(&self) -> PhysBytes {
        // C: pg_load() + vm_enable_paging() — pg_utils.c:204,247
        unsafe {
            // mov cr3, root_page
            // mov cr0, cr0 | (1 << 31) — PG
            // mov cr0, cr0 | (1 << 31) | (1 << 16) — WP
        }
        todo!("x86-64 paging: enable — needs inline asm")
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
        todo!("x86-64 paging: implement root_paddr()")
    }

    unsafe fn switch(&self) {
        todo!("x86-64 paging: implement switch()")
    }

    unsafe fn flush_tlb(&self) {
        todo!("x86-64 paging: implement flush_tlb()")
    }

    unsafe fn flush_tlb_addr(&self, _vaddr: VirBytes) {
        todo!("x86-64 paging: implement flush_tlb_addr()")
    }
}

impl HugePages for X86_64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21]; // 1GB, 2MB
    const HUGE_PAGE_SIZE: u64 = 1 << 30;           // preferred: 1GB
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;  // fallback: 2MB
    const PTE_HUGE_FLAGS: u64 = 1 << 7;            // PS bit (bit 7 in PDE)

    fn map_huge(
        &mut self,
        _vaddr: VirBytes,
        _paddr: PhysBytes,
        _size: usize,
        _flags: PageFlags,
    ) -> Result<(), PageTableError> {
        todo!("x86-64 paging: implement map_huge()")
    }

    fn supports_1gb_page() -> bool {
        todo!("x86-64 paging: implement supports_1gb_page() — CPUID.80000001H:EDX.GBPAGES")
    }
}
