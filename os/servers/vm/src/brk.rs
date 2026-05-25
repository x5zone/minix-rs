//! VM brk (heap management) implementation.
//!
//! Handles VM_BRK requests from PM to change process heap size.
//! Supports growing and shrinking the heap region.
//!
//! Corresponds to Minix3's `do_brk()` and `real_brk()` in `brk.c`.
//!
//! ## Design decisions vs Minix3
//!
//! **Extend existing region** (P1): Minix3's `map_region_extend_upto_v` extends
//! the existing heap region via `realloc(physblocks)` + `anon_resize`. Rust mirrors
//! this by calling `VirRegion::extend()` on the topmost heap region, avoiding
//! fragmentation from creating a new VirRegion per brk.
//!
//! **Region conflict check** (P0): Minix3 checks `nextvr->vaddr < offset` before
//! extending. Rust checks `find_overlap` to prevent the heap from growing into
//! the stack or other regions.
//!
//! **Shrink releases pages** (P1): Minix3's `anon_resize` silently ignores
//! shrinkage. Rust's `shrink_heap` actually frees physical pages and unmaps
//! page table entries, improving memory reclamation.

use minix_types::{Endpoint, UserSlot, VirBytes, ENOMEM, ESRCH};
use crate::vmproc::{VmProcTable, ActiveProc};
use crate::region::{VirRegion, VrFlags, RegionMap, PageFrames, PfnAllocator};
use crate::alloc_page::VmPageAllocator;
use crate::memtype::MEM_TYPE_ANON;
use crate::pagetable::{PageTable, Paging};
use crate::region::page_state::PAGE_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrkError {
    ProcessNotFound,
    OutOfMemory,
}

impl BrkError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::OutOfMemory => ENOMEM,
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
        grow_heap(&mut active, page_alloc, frames, requested)
    } else {
        Ok(BrkResponse { new_brk_addr: current_brk })
    }
}

fn grow_heap(
    active: &mut ActiveProc<'_>,
    _page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    new_brk: VirBytes,
) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();
    let grow_len = new_brk.0 - current_top.0;

    if grow_len == 0 {
        return Ok(BrkResponse { new_brk_addr: current_top });
    }

    let aligned_len = VirBytes(((grow_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);
    let new_end = VirBytes(current_top.0 + aligned_len.0);

    if active.regions().find_overlap(current_top, new_end).is_some() {
        return Err(BrkError::OutOfMemory);
    }

    if let Some(top_region) = active.regions_mut().find_mut(current_top) {
        top_region.extend(aligned_len)
            .map_err(|_| BrkError::OutOfMemory)?;
    } else if let Some(top_region) = active.regions_mut().find_mut_by_end(current_top) {
        top_region.extend(aligned_len)
            .map_err(|_| BrkError::OutOfMemory)?;
    } else {
        let new_region = VirRegion::with_memtype(
            current_top,
            aligned_len,
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        active.regions_mut().insert(new_region);
    }

    active.add_total(aligned_len);
    active.set_region_top(new_end);

    Ok(BrkResponse { new_brk_addr: new_brk })
}

fn shrink_heap(
    active: &mut ActiveProc<'_>,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    new_brk: VirBytes,
) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();

    if new_brk.0 >= current_top.0 {
        return Ok(BrkResponse { new_brk_addr: current_top });
    }

    let mut regions_to_remove = alloc::vec::Vec::new();
    let mut regions_to_shrink = alloc::vec::Vec::new();

    for region in active.regions().iter() {
        if region.vaddr.0 >= new_brk.0 {
            regions_to_remove.push(region.vaddr);
        } else if region.end_addr().0 > new_brk.0 && region.vaddr.0 < new_brk.0 {
            regions_to_shrink.push(region.vaddr);
        }
    }

    for vaddr in regions_to_shrink {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let raw_split = new_brk.0 - region.vaddr.0;
            let aligned_split = raw_split & !(PAGE_SIZE - 1);
            let split_point = VirBytes(aligned_split);
            let region_vaddr = region.vaddr;
            let region_len = region.length;
            let region_flags = region.flags;
            if split_point.0 > 0 && split_point.0 < region_len.0 {
                match region.split(split_point) {
                    Ok((left, right)) => {
                        let freed_len = right.length;
                        {
                            let page_table = active.page_table_mut();
                            crate::region::free_region_pages(right, page_table, frames, page_alloc);
                        }
                        active.sub_total(VirBytes(freed_len.0));
                        active.regions_mut().insert(left);
                    }
                    Err(_) => {
                        active.regions_mut().insert(VirRegion::new(
                            region_vaddr, region_len, region_flags,
                        ));
                    }
                }
            } else {
                active.regions_mut().insert(region);
            }
        }
    }

    for vaddr in regions_to_remove {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let freed_len = region.length;
            {
                let page_table = active.page_table_mut();
                crate::region::free_region_pages(region, page_table, frames, page_alloc);
            }
            active.sub_total(VirBytes(freed_len.0));
        }
    }

    active.set_region_top(new_brk);

    Ok(BrkResponse { new_brk_addr: new_brk })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;

    fn make_frames() -> PageFrames {
        PageFrames::new(minix_types::PhysBytes(256 * REGION_PAGE_SIZE as u64))
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
        active.set_region_top(VirBytes(0x4000_0000));
        ep
    }

    #[test]
    fn test_brk_error_to_errno() {
        assert_eq!(BrkError::ProcessNotFound.to_errno(), ESRCH);
        assert_eq!(BrkError::OutOfMemory.to_errno(), ENOMEM);
    }

    #[test]
    fn test_grow_heap_creates_region_on_first_brk() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let slot = UserSlot::new(0);
        let ep = init_test_process(slot);

        let request = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_1000),
        };

        let result = handle_brk(table, &mut page_alloc, &mut frames, &request);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().new_brk_addr, VirBytes(0x4000_1000));
    }

    #[test]
    fn test_grow_heap_extends_existing_region() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let slot = UserSlot::new(0);
        let ep = init_test_process(slot);

        let request1 = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_1000),
        };
        handle_brk(table, &mut page_alloc, &mut frames, &request1).unwrap();

        let request2 = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_2000),
        };
        let result = handle_brk(table, &mut page_alloc, &mut frames, &request2);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().new_brk_addr, VirBytes(0x4000_2000));
    }

    #[test]
    fn test_brk_no_change() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let slot = UserSlot::new(0);
        let ep = init_test_process(slot);

        let request = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_0000),
        };

        let result = handle_brk(table, &mut page_alloc, &mut frames, &request);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().new_brk_addr, VirBytes(0x4000_0000));
    }

    #[test]
    fn test_brk_process_not_found() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let request = BrkRequest {
            endpoint: Endpoint::NONE,
            new_brk_addr: VirBytes(0x4000_1000),
        };

        let result = handle_brk(table, &mut page_alloc, &mut frames, &request);
        assert!(matches!(result, Err(BrkError::ProcessNotFound)));
    }

    #[test]
    fn test_shrink_heap_basic() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let slot = UserSlot::new(0);
        let ep = init_test_process(slot);

        let grow = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_3000),
        };
        handle_brk(table, &mut page_alloc, &mut frames, &grow).unwrap();

        let shrink = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_1000),
        };
        let result = handle_brk(table, &mut page_alloc, &mut frames, &shrink);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().new_brk_addr, VirBytes(0x4000_1000));
    }

    #[test]
    fn test_grow_heap_extends_region_at_boundary() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();

        let slot = UserSlot::new(0);
        let ep = init_test_process(slot);

        let request1 = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_1000),
        };
        handle_brk(table, &mut page_alloc, &mut frames, &request1).unwrap();

        let slot_data = table.vm_isokendpt(ep).unwrap();
        let active = table.get_active(slot_data).unwrap();
        let region_count_before = active.regions().len();
        drop(active);

        let request2 = BrkRequest {
            endpoint: ep,
            new_brk_addr: VirBytes(0x4000_2000),
        };
        let result = handle_brk(table, &mut page_alloc, &mut frames, &request2);
        assert!(result.is_ok());

        let active = table.get_active(slot_data).unwrap();
        let region_count_after = active.regions().len();
        assert_eq!(region_count_after, region_count_before,
            "extending at region boundary should reuse existing region, not create a new one");
    }
}
