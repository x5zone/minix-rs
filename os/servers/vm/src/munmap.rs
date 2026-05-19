//! VM munmap implementation.
//!
//! Handles VM_MUNMAP requests to unmap virtual address ranges.
//! Supports partial unmap via region splitting.
//!
//! Corresponds to Minix3's `do_munmap()` and `map_unmap_region()` in `mmap.c`.
//!
//! 方案三：PFN 索引模型: Updated to use PageFrames/PageSlot instead of PhysRegion.

use minix_types::{Endpoint, UserSlot, VirBytes, EINVAL, ESRCH, ENOMEM};
use crate::vmproc::{VmProcTable, ActiveProc, VmFlags};
use crate::region::{VirRegion, VrFlags, RegionMap, PageFrames};
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::AlignedPhysBytes;
use crate::pagetable::{PageTable, Paging};

const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MunmapError {
    ProcessNotFound,
    InvalidAddress,
    InvalidLength,
    NotMapped,
    InternalError,
}

impl MunmapError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::InvalidAddress => EINVAL,
            Self::InvalidLength => EINVAL,
            Self::NotMapped => EINVAL,
            Self::InternalError => ENOMEM,
        }
    }
}

pub(crate) struct MunmapRequest {
    pub endpoint: Endpoint,
    pub addr: VirBytes,
    pub length: VirBytes,
}

pub(crate) fn handle_munmap(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &MunmapRequest,
) -> Result<(), MunmapError> {
    if request.length.0 == 0 {
        return Err(MunmapError::InvalidLength);
    }

    if request.addr.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::InvalidAddress);
    }

    if request.length.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::InvalidLength);
    }

    let slot = table.vm_isokendpt(request.endpoint)
        .map_err(|_| MunmapError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(MunmapError::ProcessNotFound)?;

    unmap_range(&mut active, frames, request.addr, request.length)
}

fn unmap_range(
    active: &mut ActiveProc<'_>,
    frames: &mut PageFrames,
    addr: VirBytes,
    length: VirBytes,
) -> Result<(), MunmapError> {
    let unmap_start = addr;
    let unmap_end = VirBytes(addr.0 + length.0);

    let mut vaddrs_to_process = alloc::vec::Vec::new();

    for region in active.regions_mut().iter() {
        if region.overlaps(unmap_start, unmap_end) {
            vaddrs_to_process.push(region.vaddr);
        }
    }

    if vaddrs_to_process.is_empty() {
        return Ok(());
    }

    for vaddr in vaddrs_to_process {
        if let Some(mut region) = active.regions_mut().remove(vaddr) {
            let reg_start = region.vaddr;
            let reg_end = region.end_addr();

            if unmap_start <= reg_start && unmap_end >= reg_end {
                {
                    let page_table = active.page_table_mut();
                    free_region_pages(&region, page_table, frames);
                }
                active.sub_total(VirBytes(region.length.0));
            } else if unmap_start > reg_start && unmap_end < reg_end {
                let head_len = VirBytes(unmap_start.0 - reg_start.0);

                let (left, remainder) = region.split(head_len)
                    .map_err(|_| MunmapError::InternalError)?;
                let (_middle, right) = remainder.split(VirBytes(length.0))
                    .map_err(|_| MunmapError::InternalError)?;

                {
                    let page_table = active.page_table_mut();
                    free_region_pages(&_middle, page_table, frames);
                }
                active.sub_total(VirBytes(length.0));

                active.regions_mut().insert(left);
                active.regions_mut().insert(right);
            } else if unmap_start <= reg_start && unmap_end < reg_end {
                let cut_len = VirBytes(unmap_end.0 - reg_start.0);
                let (head, tail) = region.split(cut_len)
                    .map_err(|_| MunmapError::InternalError)?;

                {
                    let page_table = active.page_table_mut();
                    free_region_pages(&head, page_table, frames);
                }
                active.sub_total(VirBytes(head.length.0));

                active.regions_mut().insert(tail);
            } else if unmap_start > reg_start && unmap_end >= reg_end {
                let head_len = VirBytes(unmap_start.0 - reg_start.0);
                let (head, tail) = region.split(head_len)
                    .map_err(|_| MunmapError::InternalError)?;

                {
                    let page_table = active.page_table_mut();
                    free_region_pages(&tail, page_table, frames);
                }
                active.sub_total(VirBytes(tail.length.0));

                active.regions_mut().insert(head);
            }
        }
    }

    Ok(())
}

fn free_region_pages(region: &VirRegion, page_table: &mut PageTable, frames: &PageFrames) {
    let page_count = (region.length.0 / PAGE_SIZE) as usize;
    for i in 0..page_count {
        let vaddr = VirBytes(region.vaddr.0 + (i as u64) * PAGE_SIZE);
        let _ = page_table.unmap(vaddr);
    }

    for slot_opt in &region.physblocks {
        if let Some(slot) = slot_opt {
            if slot.is_mapped() {
                let phys = frames.pfn_to_phys(slot.pfn);
                let _ = AlignedPhysBytes::new(phys.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use minix_types::Endpoint;

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let _active = empty.activate(ep);
        ep
    }
}
