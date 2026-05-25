//! Memory region management module.

pub(crate) mod vir_region;
pub(crate) mod page_state;
pub(crate) mod region_map;

pub(crate) use vir_region::{VirRegion, VrFlags, VrParam, VmError, PageAllocFlags};
pub(crate) use page_state::{PageState, PageFrames, PageSlot, PageFlags, PFN_NONE, PAGE_SIZE, PfnAllocator, PfnAllocError};
pub(crate) use region_map::RegionMap;

use crate::alloc_page::VmPageAllocator;
use crate::pagetable::Paging;
use minix_types::VirBytes;

pub(crate) fn free_region_pages(
    mut region: VirRegion,
    page_table: &mut crate::pagetable::PageTable,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
) {
    let page_count = (region.length.0 / PAGE_SIZE) as usize;
    for i in 0..page_count {
        let vaddr = VirBytes(region.vaddr.0 + (i as u64) * PAGE_SIZE);
        let _ = page_table.unmap(vaddr);
    }

    let fdref_id = if let VrParam::File { fdref_id: Some(id), .. } = region.param {
        Some(id)
    } else {
        None
    };

    if let Some(mt) = region.def_memtype {
        mt.ev_delete(&mut region);
    }

    let pending = region.free_range(frames, VirBytes(0), region.length);
    for (pfn, mt) in pending {
        mt.ev_unreference(frames, pfn);
        page_alloc.free_pfn(pfn);
    }

    if let Some(id) = fdref_id {
        let pending_close = crate::fdref::FdRefTable::get_global().deref_entry(id);
        if let Some(close) = pending_close {
            let _ = close;
            // TODO: send vfs_request(FdClose, close.fd, ...) when VFS IPC is implemented
        }
    }
}
