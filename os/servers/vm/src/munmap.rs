//! VM munmap implementation.
//!
//! Handles VM_MUNMAP requests to unmap virtual address ranges.
//! Supports partial unmap via region splitting.
//!
//! Corresponds to Minix3's `do_munmap()` and `map_unmap_region()` in `mmap.c`.
//!
//! PFN index model: Updated to use PageFrames/PageSlot instead of PhysRegion.

use minix_types::{Endpoint, UserSlot, VirBytes, EFAULT, EINVAL, ESRCH, ENOMEM};
use crate::vmproc::{VmProcTable, ActiveProc, VmFlags};
use crate::region::{VirRegion, VrFlags, RegionMap, PageFrames, PfnAllocator};
use crate::alloc_page::VmPageAllocator;
use crate::pagetable::{PageTable, Paging};
use crate::region::page_state::PAGE_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MunmapError {
    ProcessNotFound,
    BadAddress,
    InvalidLength,
    NotMapped,
    InternalError,
}

impl MunmapError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::BadAddress => EFAULT,
            Self::InvalidLength => EINVAL,
            Self::NotMapped => EFAULT,
            Self::InternalError => ENOMEM,
        }
    }
}

pub(crate) struct MunmapRequest {
    pub endpoint: Endpoint,
    pub addr: VirBytes,
    pub length: VirBytes,
    pub lookup_region_length: bool,
}

pub(crate) fn handle_munmap(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &MunmapRequest,
) -> Result<(), MunmapError> {
    if request.addr.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::BadAddress);
    }

    let slot = table.vm_isokendpt(request.endpoint)
        .map_err(|_| MunmapError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(MunmapError::ProcessNotFound)?;

    let length = if request.length.0 == 0 {
        if !request.lookup_region_length {
            return Err(MunmapError::InvalidLength);
        }
        let region = active.regions().find(request.addr)
            .ok_or(MunmapError::NotMapped)?;
        region.length
    } else {
        if request.length.0 % PAGE_SIZE != 0 {
            return Err(MunmapError::InvalidLength);
        }
        request.length
    };

    unmap_range(&mut active, page_alloc, frames, request.addr, length)
}

pub(crate) fn munmap_vm_lin(addr: VirBytes, length: VirBytes) -> Result<(), MunmapError> {
    if addr.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::BadAddress);
    }
    if length.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::InvalidLength);
    }
    let pages = (length.0 / PAGE_SIZE) as usize;
    crate::pagetable::vm_self_unmappages(addr, pages)
        .map_err(|_| MunmapError::InternalError)
}

pub(crate) fn unmap_range(
    active: &mut ActiveProc<'_>,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    addr: VirBytes,
    length: VirBytes,
) -> Result<(), MunmapError> {
    let unmap_start = addr;
    let unmap_end = VirBytes(addr.0 + length.0);

    let mut vaddrs_to_process = alloc::vec::Vec::new();

    for region in active.regions().iter() {
        if region.overlaps(unmap_start, unmap_end) {
            vaddrs_to_process.push(region.vaddr);
        }
    }

    if vaddrs_to_process.is_empty() {
        return Ok(());
    }

    for vaddr in vaddrs_to_process {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let reg_start = region.vaddr;
            let reg_end = region.end_addr();

            if unmap_start <= reg_start && unmap_end >= reg_end {
                let freed_len = region.length;
                {
                    let page_table = active.page_table_mut();
                    crate::region::free_region_pages(region, page_table, frames, page_alloc);
                }
                active.sub_total(VirBytes(freed_len.0));
            } else if unmap_start > reg_start && unmap_end < reg_end {
                let head_len = VirBytes(unmap_start.0 - reg_start.0);

                let (left, remainder) = region.split(head_len)
                    .map_err(|_| MunmapError::InternalError)?;
                let (middle, right) = remainder.split(VirBytes(length.0))
                    .map_err(|_| MunmapError::InternalError)?;

                {
                    let page_table = active.page_table_mut();
                    crate::region::free_region_pages(middle, page_table, frames, page_alloc);
                }
                active.sub_total(VirBytes(length.0));

                active.regions_mut().insert(left);
                active.regions_mut().insert(right);
            } else if unmap_start <= reg_start && unmap_end < reg_end {
                let cut_len = VirBytes(unmap_end.0 - reg_start.0);
                let (head, tail) = region.split(cut_len)
                    .map_err(|_| MunmapError::InternalError)?;

                let freed_len = head.length;
                {
                    let page_table = active.page_table_mut();
                    crate::region::free_region_pages(head, page_table, frames, page_alloc);
                }
                active.sub_total(VirBytes(freed_len.0));

                active.regions_mut().insert(tail);
            } else if unmap_start > reg_start && unmap_end >= reg_end {
                let head_len = VirBytes(unmap_start.0 - reg_start.0);
                let (head, tail) = region.split(head_len)
                    .map_err(|_| MunmapError::InternalError)?;

                let freed_len = tail.length;
                {
                    let page_table = active.page_table_mut();
                    crate::region::free_region_pages(tail, page_table, frames, page_alloc);
                }
                active.sub_total(VirBytes(freed_len.0));

                active.regions_mut().insert(head);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;
    use minix_types::{Endpoint, PhysBytes};

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

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
    fn test_munmap_unaligned_addr_returns_bad_address() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(80);
        let ep = init_test_process(slot);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1001),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };

        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MunmapError::BadAddress)));
    }

    #[test]
    fn test_munmap_zero_length_returns_invalid_length() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(81);
        let ep = init_test_process(slot);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0),
            lookup_region_length: false,
        };

        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MunmapError::InvalidLength)));
    }

    #[test]
    fn test_munmap_no_region_silent_ok() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(82);
        let ep = init_test_process(slot);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x10000),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };

        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(result.is_ok());
    }

    #[test]
    fn test_munmap_error_to_errno() {
        assert_eq!(MunmapError::BadAddress.to_errno(), EFAULT);
        assert_eq!(MunmapError::InvalidLength.to_errno(), EINVAL);
        assert_eq!(MunmapError::NotMapped.to_errno(), EFAULT);
        assert_eq!(MunmapError::ProcessNotFound.to_errno(), ESRCH);
        assert_eq!(MunmapError::InternalError.to_errno(), ENOMEM);
    }
}
