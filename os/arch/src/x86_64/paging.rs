//! x86-64 paging implementation (placeholder)
//!
//! TODO: Implement real x86-64 page table operations using CR3, PML4, etc.
//! Currently only provides the type alias for `CurrentPaging` resolution.

use crate::paging::{Paging, PageFlags, PageTableError};
use minix_types::{PhysBytes, VirBytes};

pub struct X86_64Paging;

impl Paging for X86_64Paging {
    const PAGE_SIZE: usize = 4096;

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
