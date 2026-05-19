//! VM brk (heap management) implementation.
//!
//! Handles VM_BRK requests from PM to change process heap size.
//! Supports growing and shrinking the heap region.
//!
//! Corresponds to Minix3's `do_brk()` and `real_brk()` in `brk.c`.
//!
//! 方案三：PFN 索引模型: Updated to use PageFrames/PageSlot instead of PhysRegion.

use minix_types::{Endpoint, UserSlot, VirBytes, ENOMEM, EINVAL, ESRCH};
use crate::vmproc::{VmProcTable, ActiveProc, VmFlags};
use crate::region::{VirRegion, VrFlags, RegionMap, PageFrames};
use crate::alloc_page::VmPageAllocator;
use crate::memtype::MEM_TYPE_ANON;
use crate::phys_mem::AlignedPhysBytes;
use crate::pagetable::{PageTable, Paging};

const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrkError {
    ProcessNotFound,
    InvalidAddress,
    OutOfMemory,
    AlreadyMapped,
    InternalError,
}

impl BrkError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::InvalidAddress => EINVAL,
            Self::OutOfMemory => ENOMEM,
            Self::AlreadyMapped => EINVAL,
            Self::InternalError => EINVAL,
        }
    }
}

pub(crate) struct BrkRequest {
    pub endpoint: Endpoint,
    pub new_brk_addr: VirBytes,
}

pub(crate) struct BrkResponse {
    pub new_brk_addr: VirBytes,
}

pub(crate) fn handle_brk(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &BrkRequest,
) -> Result<BrkResponse, BrkError> {
    let slot = table.vm_isokendpt(request.endpoint)
        .map_err(|_| BrkError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(BrkError::ProcessNotFound)?;

    let current_brk = active.region_top();
    let requested = request.new_brk_addr;

    if requested.0 < current_brk.0 {
        shrink_heap(&mut active, page_alloc, frames, requested)
    } else if requested.0 > current_brk.0 {
        grow_heap(&mut active, page_alloc, requested)
    } else {
        Ok(BrkResponse { new_brk_addr: current_brk })
    }
}

fn grow_heap(
    active: &mut ActiveProc<'_>,
    page_alloc: &mut VmPageAllocator,
    new_brk: VirBytes,
) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();
    let grow_len = new_brk.0 - current_top.0;

    if grow_len == 0 {
        return Ok(BrkResponse { new_brk_addr: current_top });
    }

    let aligned_len = VirBytes(((grow_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);
    let region_start = current_top;

    let new_region = VirRegion::with_memtype(
        region_start,
        aligned_len,
        VrFlags::WRITABLE | VrFlags::ANON,
        &MEM_TYPE_ANON,
    );

    active.regions_mut().insert(new_region);
    active.add_total(aligned_len);
    active.set_region_top(VirBytes(region_start.0 + aligned_len.0));

    Ok(BrkResponse { new_brk_addr: new_brk })
}

fn shrink_heap(
    active: &mut ActiveProc<'_>,
    _page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    new_brk: VirBytes,
) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();

    if new_brk.0 >= current_top.0 {
        return Ok(BrkResponse { new_brk_addr: current_top });
    }

    let mut regions_to_remove = alloc::vec::Vec::new();
    let mut regions_to_shrink = alloc::vec::Vec::new();

    for region in active.regions_mut().iter() {
        if region.vaddr.0 >= new_brk.0 {
            regions_to_remove.push(region.vaddr);
        } else if region.end_addr().0 > new_brk.0 && region.vaddr.0 < new_brk.0 {
            regions_to_shrink.push(region.vaddr);
        }
    }

    for vaddr in regions_to_shrink {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let split_point = VirBytes(new_brk.0 - region.vaddr.0);
            let region_len = region.length;
            if split_point.0 > 0 && split_point.0 < region_len.0 {
                match region.split(split_point) {
                    Ok((left, right)) => {
                        {
                            let page_table = active.page_table_mut();
                            free_region_pages(&right, page_table, frames);
                        }
                        active.sub_total(VirBytes(right.length.0));
                        active.regions_mut().insert(left);
                    }
                    Err(_) => {
                        active.sub_total(VirBytes(region_len.0));
                    }
                }
            } else {
                active.regions_mut().insert(region);
            }
        }
    }

    for vaddr in regions_to_remove {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            {
                let page_table = active.page_table_mut();
                free_region_pages(&region, page_table, frames);
            }
            active.sub_total(VirBytes(region.length.0));
        }
    }

    active.set_region_top(new_brk);

    Ok(BrkResponse { new_brk_addr: new_brk })
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

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        active.set_region_top(VirBytes(0x4000_0000));
        ep
    }

    #[test]
    fn test_brk_error_to_errno() {
        assert_eq!(BrkError::ProcessNotFound.to_errno(), ESRCH);
        assert_eq!(BrkError::InvalidAddress.to_errno(), EINVAL);
        assert_eq!(BrkError::OutOfMemory.to_errno(), ENOMEM);
    }
}
