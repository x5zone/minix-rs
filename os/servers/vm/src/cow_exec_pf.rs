//! Copy-on-Write, Exec, and Page Fault handling.
//!
//! Implements the core page fault resolution logic:
//! - Anonymous page faults: allocate new physical page
//! - CoW page faults: copy page on write
//! - Direct physical page faults: map physical address
//! - Exec: replace process address space
//!
//! Corresponds to Minix3's `map_pf()` in `region.c` and `do_pagefaults()` in `main.c`.

use minix_types::{Endpoint, UserSlot, VirBytes, PhysBytes as MtPhysBytes, EACCES, ENOMEM, EINVAL, ESRCH};
use crate::vmproc::{VmProcTable, ActiveProc, VmFlags};
use crate::region::{VirRegion, VrFlags, PhysRegion, PhysBlock, RegionAvl};
use crate::alloc_page::VmPageAllocator;
use crate::memtype::{PagefaultResult, MEM_TYPE_ANON};
use crate::pagetable::{PageTable, PageFlags, Paging};
use crate::phys_mem::PhysBytes as PmPhysBytes;
use crate::direct_map::vm_phys_to_virt;
use core::ptr::NonNull;

const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageFaultError {
    ProcessNotFound,
    InvalidAddress,
    AccessViolation,
    OutOfMemory,
    InternalError,
}

impl PageFaultError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::InvalidAddress => EINVAL,
            Self::AccessViolation => EACCES,
            Self::OutOfMemory => ENOMEM,
            Self::InternalError => ENOMEM,
        }
    }
}

pub(crate) struct PageFaultInfo {
    pub endpoint: Endpoint,
    pub vaddr: VirBytes,
    pub write: bool,
}

pub(crate) fn handle_pagefault(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    fault: &PageFaultInfo,
) -> Result<(), PageFaultError> {
    let slot = table.vm_isokendpt(fault.endpoint)
        .map_err(|_| PageFaultError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(PageFaultError::ProcessNotFound)?;

    let (region_vaddr, page_index, memtype, is_unmapped, needs_cow, is_writable) = {
        let region = active.regions_mut().find(fault.vaddr)
            .ok_or(PageFaultError::InvalidAddress)?;

        let offset = VirBytes(fault.vaddr.0 - region.vaddr.0);
        let page_index = (offset.0 / PAGE_SIZE) as usize;

        if page_index >= region.physblocks.len() {
            return Err(PageFaultError::InvalidAddress);
        }

        let memtype = region.def_memtype
            .ok_or(PageFaultError::InternalError)?;

        let is_unmapped = region.physblocks.get(page_index)
            .and_then(|opt| opt.as_ref())
            .map(|pr| pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) == PhysBlock::MAP_NONE)
            .unwrap_or(true);

        let needs_cow = region.physblocks.get(page_index)
            .and_then(|opt| opt.as_ref())
            .map(|pr| pr.needs_cow())
            .unwrap_or(false);

        let is_writable = region.is_writable();

        (region.vaddr, page_index, memtype, is_unmapped, needs_cow, is_writable)
    };

    if is_unmapped {
        let (_new_virt, new_phys) = page_alloc.alloc_page()
            .ok_or(PageFaultError::OutOfMemory)?;

        let new_phys_mt = MtPhysBytes::new(new_phys.as_u64());

        {
            let page_table = active.page_table_mut();
            let vaddr = VirBytes(region_vaddr.0 + (page_index as u64) * PAGE_SIZE);
            let flags = if is_writable {
                PageFlags::read_write()
            } else {
                PageFlags::read_only()
            };
            page_table.map(vaddr, new_phys_mt, flags)
                .map_err(|_| PageFaultError::InternalError)?;
        }

        {
            let region = active.regions_mut().find_mut(fault.vaddr)
                .ok_or(PageFaultError::InvalidAddress)?;
            let offset = VirBytes(fault.vaddr.0 - region_vaddr.0);
            let mut new_phys_region = PhysRegion::new(offset);
            let block = alloc::boxed::Box::new(PhysBlock::new(new_phys_mt));
            let block_ptr = NonNull::new(alloc::boxed::Box::into_raw(block)).unwrap();
            unsafe { new_phys_region.bind_block(block_ptr); }
            new_phys_region.memtype = Some(memtype);
            region.set_phys_region(offset, new_phys_region);
        }

        active.inc_minor_fault();
        return Ok(());
    }

    if needs_cow && fault.write {
        if !is_writable {
            return Err(PageFaultError::AccessViolation);
        }

        let old_phys_mt: MtPhysBytes = {
            let region = active.regions_mut().find(fault.vaddr)
                .ok_or(PageFaultError::InvalidAddress)?;
            region.physblocks.get(page_index)
                .and_then(|opt| opt.as_ref())
                .and_then(|pr| pr.get_phys_addr())
                .ok_or(PageFaultError::InternalError)?
        };

        let (_new_virt, new_phys) = page_alloc.alloc_page()
            .ok_or(PageFaultError::OutOfMemory)?;

        let new_phys_mt = MtPhysBytes::new(new_phys.as_u64());

        unsafe {
            let src = vm_phys_to_virt(PmPhysBytes::new(old_phys_mt.get())).0 as *const u8;
            let dst = vm_phys_to_virt(new_phys).0 as *mut u8;
            core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE as usize);
        }

        {
            let page_table = active.page_table_mut();
            let vaddr = VirBytes(region_vaddr.0 + (page_index as u64) * PAGE_SIZE);
            page_table.map(vaddr, new_phys_mt, PageFlags::read_write())
                .map_err(|_| PageFaultError::InternalError)?;
        }

        {
            let region = active.regions_mut().find_mut(fault.vaddr)
                .ok_or(PageFaultError::InvalidAddress)?;
            let offset = VirBytes(fault.vaddr.0 - region_vaddr.0);

            if let Some(pr) = region.physblocks.get_mut(page_index) {
                if let Some(existing) = pr {
                    let should_free = existing.unbind_block();
                    if should_free {
                        if let Some(mt) = existing.memtype {
                            let _ = mt.on_unreference(existing);
                        }
                    }
                }

                let mut new_phys_region = PhysRegion::new(offset);
                let block = alloc::boxed::Box::new(PhysBlock::new(new_phys_mt));
                let block_ptr = NonNull::new(alloc::boxed::Box::into_raw(block)).unwrap();
                unsafe { new_phys_region.bind_block(block_ptr); }
                new_phys_region.memtype = Some(memtype);
                *pr = Some(alloc::boxed::Box::new(new_phys_region));
            }
        }

        active.inc_major_fault();
        return Ok(());
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecError {
    ProcessNotFound,
    OutOfMemory,
    InternalError,
}

impl ExecError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::OutOfMemory => ENOMEM,
            Self::InternalError => ENOMEM,
        }
    }
}

pub(crate) struct ExecNewmemRequest {
    pub endpoint: Endpoint,
    pub text_addr: VirBytes,
    pub text_len: VirBytes,
    pub data_addr: VirBytes,
    pub data_len: VirBytes,
    pub pc: VirBytes,
}

pub(crate) fn handle_exec_newmem(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    request: &ExecNewmemRequest,
) -> Result<(), ExecError> {
    let slot = table.vm_isokendpt(request.endpoint)
        .map_err(|_| ExecError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(ExecError::ProcessNotFound)?;

    {
        let regions = active.regions_mut();
        for region in regions.iter() {
            free_region_pages(region, page_alloc);
        }
        regions.clear();
    }

    if request.text_len.0 > 0 {
        let text_pages = ((request.text_len.0 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        let aligned_len = VirBytes(text_pages as u64 * PAGE_SIZE);

        let text_region = VirRegion::with_memtype(
            request.text_addr,
            aligned_len,
            VrFlags(VrFlags::ANON),
            &MEM_TYPE_ANON,
        );

        active.regions_mut().insert(text_region);
        active.add_total(aligned_len);
    }

    if request.data_len.0 > 0 {
        let data_pages = ((request.data_len.0 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        let aligned_len = VirBytes(data_pages as u64 * PAGE_SIZE);

        let data_region = VirRegion::with_memtype(
            request.data_addr,
            aligned_len,
            VrFlags(VrFlags::WRITABLE | VrFlags::ANON),
            &MEM_TYPE_ANON,
        );

        active.regions_mut().insert(data_region);
        active.add_total(aligned_len);
        active.set_region_top(VirBytes(request.data_addr.0 + aligned_len.0));
    }

    Ok(())
}

fn free_region_pages(region: &VirRegion, page_alloc: &mut VmPageAllocator) {
    for phys_opt in &region.physblocks {
        if let Some(pr) = phys_opt {
            if let Some(phys) = pr.get_phys_addr() {
                page_alloc.free_page(PmPhysBytes::new(phys.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        ep
    }

    #[test]
    fn test_pagefault_error_to_errno() {
        assert_eq!(PageFaultError::ProcessNotFound.to_errno(), ESRCH);
        assert_eq!(PageFaultError::AccessViolation.to_errno(), EACCES);
        assert_eq!(PageFaultError::OutOfMemory.to_errno(), ENOMEM);
    }

    #[test]
    fn test_exec_error_to_errno() {
        assert_eq!(ExecError::ProcessNotFound.to_errno(), ESRCH);
        assert_eq!(ExecError::OutOfMemory.to_errno(), ENOMEM);
    }

    #[test]
    fn test_pagefault_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let fault = PageFaultInfo {
            endpoint: Endpoint::NONE,
            vaddr: VirBytes(0x1000),
            write: false,
        };

        let result = handle_pagefault(table, &mut page_alloc, &fault);
        assert!(matches!(result, Err(PageFaultError::ProcessNotFound)));
    }

    #[test]
    fn test_pagefault_no_region() {
        let ep = init_test_process(UserSlot::new(70));
        let table = VmProcTable::get_global();
        let mut page_alloc = VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)));

        let fault = PageFaultInfo {
            endpoint: ep,
            vaddr: VirBytes(0xDEAD_0000),
            write: false,
        };

        let result = handle_pagefault(table, &mut page_alloc, &fault);
        assert!(matches!(result, Err(PageFaultError::InvalidAddress)));
    }
}
