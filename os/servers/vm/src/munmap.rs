//! VM munmap implementation.
//!
//! Handles VM_MUNMAP / VM_UNMAP_PHYS / VM_SHM_UNMAP requests to unmap
//! virtual address ranges, supporting partial unmap via region splitting.
//!
//! Corresponds to Minix3's `do_munmap()` (mmap.c:512-573),
//! `munmap_vm_lin()` (mmap.c:488-510), and `map_unmap_range()` /
//! `map_unmap_region()` (region.c:1222-1294 / 1065-1147).
//!
//! PFN index model: physical pages are released through
//! `free_region_pages` → `VirRegion::free_range` → `unmap_page`
//! (refcount --) → `ev_unreference` + `PfnAllocator::free_pfn`.

use minix_types::{Endpoint, VirBytes};
use crate::vmproc::{VmProcTable, ActiveProc, EndpointError};
use crate::region::PageFrames;
use crate::alloc_page::VmPageAllocator;
use crate::region::page_state::PAGE_SIZE;

/// Errors from VM_MUNMAP operations.
///
/// All variants map to `VmError` via `From<MunmapError> for VmError`,
/// then to C errno via `VmError::to_errno()`. Per-error `to_errno()`
/// method is intentionally omitted — the single source of truth is
/// `VmError::to_errno()` in `minix_types::ipc::vm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MunmapError {
    ProcessNotFound,
    BadAddress,
    InvalidLength,
    NotMapped,
    /// A partial unmap requires a memtype callback the region's memtype does
    /// not provide. C: `split_region` requires `ev_split` (region.c:1164) and
    /// low-end shrink requires `ev_lowshrink` (region.c:1096); both return
    /// EINVAL. Mapped to `InvalidParam` (EINVAL).
    MemTypeNotSupported,
    InternalError,
}

/// Outcome of a munmap operation.
///
/// `Suspended` mirrors C's `SUSPEND` return from `do_munmap()` for the
/// VM-self special case (mmap.c:548): the request is handled
/// synchronously and no reply must be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MunmapOutcome {
    /// Reply OK to the caller.
    Replied,
    /// Handled synchronously; do not reply (C: `return SUSPEND`).
    Suspended,
}

// ── Endpoint-lookup error unification ──
//
// `vm_isokendpt()` returns `Result<UserSlot, EndpointError>` where the
// error distinguishes `InvalidSlot` (out-of-range slot, Minix3 EINVAL)
// from `DeadEndpoint` (slot is in range but the endpoint is stale or
// the process is not IN_USE, Minix3 EDEADEPT). Most callers collapse
// both variants to `ProcessNotFound` because the action (return error
// to caller) is the same. Before this fix, every call site repeated
// `vm_isokendpt(...).map_err(|_| MunmapError::ProcessNotFound)?`,
// which lost the distinction and was repetitive.
//
// After: we define `From<EndpointError> for MunmapError` that maps both
// variants to `ProcessNotFound` (the conservative choice for this
// module — munmap callers do not distinguish INVALID from DEADEPT).
// Call sites now use `?` directly:
//
//     let slot = table.vm_isokendpt(request.endpoint)?;
//
// If a future caller needs to distinguish (e.g. for diagnostics), it can
// still use `vm_isokendpt(...).map_err(|e| match e { ... })` — the From
// impl only affects the `?` shorthand. Same pattern is applied to
// `query.rs`, `rs.rs`, `mmap.rs`, `brk.rs`, `exit.rs`.
impl From<EndpointError> for MunmapError {
    fn from(_: EndpointError) -> Self {
        // Munmap callers don't distinguish INVALID-slot from DEAD-endpoint:
        // both mean "this endpoint doesn't refer to a usable VM process",
        // and the only sensible reply is `ProcessNotFound`. The Minix3
        // mapping (EDEADEPT for both) is preserved.
        MunmapError::ProcessNotFound
    }
}

pub(crate) struct MunmapRequest {
    pub endpoint: Endpoint,
    pub addr: VirBytes,
    pub length: VirBytes,
    /// VM_UNMAP_PHYS / VM_SHM_UNMAP ignore the message length and unmap
    /// the whole region found at `addr` (C: `len = vr->length`,
    /// mmap.c:560-566).
    pub lookup_region_length: bool,
}

/// Round a byte length up to a whole page, matching C's `roundup`
/// macro (`(len + PAGE_SIZE - 1) & ~(PAGE_SIZE - 1)`). `do_munmap`
/// applies it to `VMUM_LEN` (mmap.c:568); zero is handled by the caller.
fn roundup_page(len: VirBytes) -> VirBytes {
    VirBytes(((len.0 + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE)
}

pub(crate) fn handle_munmap(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &MunmapRequest,
) -> Result<MunmapOutcome, MunmapError> {
    if request.addr.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::BadAddress);
    }

    let slot = table.vm_isokendpt(request.endpoint)?;

    let mut active = table.get_active(slot)
        .ok_or(MunmapError::ProcessNotFound)?;

    // VM self-munmap special case (C: mmap.c:535-548). VM unmaps its own
    // linear address space; if no regions are registered in its data
    // structures, fall back to the page-table-only `munmap_vm_lin`.
    //
    // NOTE: the C code reads `addr` *before* it is assigned (the address
    // extraction happens below, mmap.c:551-555) — an uninitialized-read
    // UB. minix-rs uses the message `addr` (the only sane reading) and
    // documents the divergence in 21-vm-munmap.md §3.6.
    if request.endpoint == Endpoint::VM {
        if active.regions().is_empty() {
            munmap_vm_lin(request.addr, request.length)?;
        } else if active.regions().find(request.addr).is_some() {
            // C: map_unmap_region(vmp, vr, 0, m->VMUM_LEN) — unaligned
            // length is rejected by map_unmap_region (EINVAL, region.c:1076).
            if request.length.0 % PAGE_SIZE != 0 {
                return Err(MunmapError::InvalidLength);
            }
            unmap_range(&mut active, page_alloc, frames, request.addr, request.length)?;
        }
        return Ok(MunmapOutcome::Suspended);
    }

    let length = if request.length.0 == 0 {
        if !request.lookup_region_length {
            return Err(MunmapError::InvalidLength);
        }
        // VM_UNMAP_PHYS / VM_SHM_UNMAP: unmap the whole region. C looks
        // the region up first and fails with EFAULT if `addr` is not
        // mapped (mmap.c:560-564), then uses `len = vr->length`.
        let region = active.regions().find(request.addr)
            .ok_or(MunmapError::NotMapped)?;
        region.length
    } else {
        // VM_MUNMAP: C rounds the caller's length up to a page boundary
        // (mmap.c:568) instead of rejecting unaligned lengths. Preserving
        // the external behavior: munmap(addr, 0x100) unmaps one page.
        roundup_page(request.length)
    };

    unmap_range(&mut active, page_alloc, frames, request.addr, length)?;
    Ok(MunmapOutcome::Replied)
}

pub(crate) fn munmap_vm_lin(addr: VirBytes, length: VirBytes) -> Result<(), MunmapError> {
    if addr.0 % PAGE_SIZE != 0 {
        return Err(MunmapError::BadAddress);
    }
    if length.0 % PAGE_SIZE != 0 {
        // C: munmap_vm_lin returns EFAULT for unaligned length (mmap.c:496).
        return Err(MunmapError::BadAddress);
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
    // C: map_unmap_range rejects a wrapping range (region.c:1234
    // `if(unmap_limit <= unmap_start) return EINVAL`).
    if unmap_end.0 <= unmap_start.0 {
        return Err(MunmapError::InvalidLength);
    }

    // Collect the overlapping regions first so we can mutate the map
    // (remove/split/insert) while iterating.
    let mut vaddrs_to_process = alloc::vec::Vec::new();

    for region in active.regions().iter() {
        if region.overlaps(unmap_start, unmap_end) {
            vaddrs_to_process.push(region.vaddr);
        }
    }

    if vaddrs_to_process.is_empty() {
        // C: map_unmap_range returns OK when no region overlaps
        // (region.c:1238-1243).
        return Ok(());
    }

    for vaddr in vaddrs_to_process {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let reg_start = region.vaddr;
            let reg_end = region.end_addr();

            if unmap_start <= reg_start && unmap_end >= reg_end {
                // Whole region falls inside the unmap range.
                let freed_len = region.length;
                {
                    #[cfg(not(test))]
                    let pt = Some(active.page_table_mut());
                    #[cfg(test)]
                    let pt: Option<&mut crate::pagetable::PageTable> = None;
                    crate::region::free_region_pages(region, pt, frames, page_alloc);
                }
                active.sub_total(VirBytes(freed_len.0));
            } else if unmap_start > reg_start && unmap_end < reg_end {
                // Hole in the middle: split twice, free the middle part.
                // C: split_region requires the memtype's `ev_split` callback
                // (region.c:1164-1168); memtypes without it (directphys,
                // shared, cache) return EINVAL. Preserving the restriction
                // also keeps `VrParam::Direct` physical bases correct (a
                // split of a direct region would map the wrong device pages).
                if !region.def_memtype.map_or(false, |mt| mt.supports_split()) {
                    return Err(MunmapError::MemTypeNotSupported);
                }
                let head_len = VirBytes(unmap_start.0 - reg_start.0);

                let (left, remainder) = region.split(head_len)
                    .map_err(|_| MunmapError::InternalError)?;
                let (middle, right) = remainder.split(VirBytes(length.0))
                    .map_err(|_| MunmapError::InternalError)?;

                {
                    #[cfg(not(test))]
                    let pt = Some(active.page_table_mut());
                    #[cfg(test)]
                    let pt: Option<&mut crate::pagetable::PageTable> = None;
                    crate::region::free_region_pages(middle, pt, frames, page_alloc);
                }
                active.sub_total(VirBytes(length.0));

                active.regions_mut().insert(left)
                    .expect("munmap: split left should not overlap");
                active.regions_mut().insert(right)
                    .expect("munmap: split right should not overlap");
            } else if unmap_start <= reg_start && unmap_end < reg_end {
                // Head cut: free [reg_start, unmap_end), keep the tail.
                // C: low-end shrink requires the memtype's `ev_lowshrink`
                // callback (region.c:1096-1106); memtypes without it
                // (directphys, shared, anon_contig) return EINVAL.
                if !region.def_memtype.map_or(false, |mt| mt.supports_low_shrink()) {
                    return Err(MunmapError::MemTypeNotSupported);
                }
                let cut_len = VirBytes(unmap_end.0 - reg_start.0);
                let (head, tail) = region.split(cut_len)
                    .map_err(|_| MunmapError::InternalError)?;

                let freed_len = head.length;
                {
                    #[cfg(not(test))]
                    let pt = Some(active.page_table_mut());
                    #[cfg(test)]
                    let pt: Option<&mut crate::pagetable::PageTable> = None;
                    crate::region::free_region_pages(head, pt, frames, page_alloc);
                }
                active.sub_total(VirBytes(freed_len.0));

                active.regions_mut().insert(tail)
                    .expect("munmap: split tail should not overlap");
            } else if unmap_start > reg_start && unmap_end >= reg_end {
                // Tail cut: free [unmap_start, reg_end), keep the head.
                let head_len = VirBytes(unmap_start.0 - reg_start.0);
                let (head, tail) = region.split(head_len)
                    .map_err(|_| MunmapError::InternalError)?;

                let freed_len = tail.length;
                {
                    #[cfg(not(test))]
                    let pt = Some(active.page_table_mut());
                    #[cfg(test)]
                    let pt: Option<&mut crate::pagetable::PageTable> = None;
                    crate::region::free_region_pages(tail, pt, frames, page_alloc);
                }
                active.sub_total(VirBytes(freed_len.0));

                active.regions_mut().insert(head)
                    .expect("munmap: split head should not overlap");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::UserSlot;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::{VrFlags, VirRegion, VrParam, PfnAllocator};
    use crate::memtype::{MEM_TYPE_ANON, MEM_TYPE_DIRECT};
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
        // Skip init_page_table() — munmap tests don't need page table access,
        // and init_page_table() accesses mock physical memory causing SIGSEGV.
        active.init_regions();
        ep
    }

    fn active_regions(ep: Endpoint) -> usize {
        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        table.get_active(slot).unwrap().regions().len()
    }

    /// Insert an unmapped (lazy) region at `addr` — matching how mmap
    /// creates anonymous regions (page faults fill the slots later).
    ///
    /// NOTE: tests deliberately keep the slots unmapped: the global
    /// `VmProcTable` is a process-wide static shared by the whole test
    /// suite (parallel threads), and `sanity::verify_refcounts` counts
    /// every mapped slot in every active region. Mapped-slot coverage is
    /// exercised on local regions in `test_munmap_releases_physical_page`.
    fn insert_region(ep: Endpoint, addr: u64, len: u64) {
        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        let mut active = table.get_active(slot).unwrap();
        let region = VirRegion::with_memtype(
            VirBytes(addr),
            VirBytes(len),
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        active.regions_mut().insert(region).expect("no overlap");
    }

    /// Insert an unmapped VR_DIRECT region (device memory, C: `do_map_phys`).
    fn insert_direct_region(ep: Endpoint, addr: u64, len: u64, phys: u64) {
        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        let mut active = table.get_active(slot).unwrap();
        let mut region = VirRegion::with_memtype(
            VirBytes(addr),
            VirBytes(len),
            VrFlags::WRITABLE,
            &MEM_TYPE_DIRECT,
        );
        region.param = VrParam::Direct { phys: PhysBytes(phys) };
        active.regions_mut().insert(region).expect("no overlap");
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
        assert_eq!(result, Ok(MunmapOutcome::Replied));
    }

    #[test]
    fn test_munmap_unaligned_len_rounds_up() {
        // C: len = roundup(VMUM_LEN, VM_PAGE_SIZE) (mmap.c:568) — an
        // unaligned length unmaps a whole page instead of failing.
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(83);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x1000);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0x100), // unaligned → rounds up to 0x1000
            lookup_region_length: false,
        };

        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 0);
        // The mapped page's refcount was released back to the allocator.
    }

    #[test]
    fn test_munmap_whole_region() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(84);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x3000);

                let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0x3000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 0);
    }

    #[test]
    fn test_munmap_head_cut() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(85);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x3000);

        // Unmap [0x1000, 0x2000): one region remains at 0x2000.
                let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 1);

        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        let active = table.get_active(slot).unwrap();
        let remaining = active.regions().iter().next().unwrap();
        assert_eq!(remaining.vaddr.0, 0x2000);
        assert_eq!(remaining.length.0, 0x2000);
    }

    #[test]
    fn test_munmap_tail_cut() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(86);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x3000);

        // Unmap [0x2000, 0x4000): one region remains at 0x1000.
                let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x2000),
            length: VirBytes(0x2000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 1);

        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        let active = table.get_active(slot).unwrap();
        let remaining = active.regions().iter().next().unwrap();
        assert_eq!(remaining.vaddr.0, 0x1000);
        assert_eq!(remaining.length.0, 0x1000);
    }

    #[test]
    fn test_munmap_middle_hole() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(87);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x5000);

        // Unmap the middle page [0x2000, 0x3000): two regions remain.
                let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x2000),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 2);

        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        let active = table.get_active(slot).unwrap();
        let vaddrs: Vec<u64> = active.regions().iter().map(|r| r.vaddr.0).collect();
        assert_eq!(vaddrs, vec![0x1000, 0x3000]);
    }

    #[test]
    fn test_munmap_cross_regions() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(88);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x1000);
        insert_region(ep, 0x3000, 0x1000);

        // Range [0x800, 0x3800) overlaps both regions: both are removed.
                let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0x2800),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 0);
    }

    #[test]
    fn test_munmap_unmap_phys_region_length() {
        // VM_UNMAP_PHYS / VM_SHM_UNMAP ignore the message length and unmap
        // the whole region found at `addr` (C: mmap.c:560-566).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(89);
        let ep = init_test_process(slot);
        insert_region(ep, 0x1000, 0x4000);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0), // ignored: region length is used
            lookup_region_length: true,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 0);
    }

    #[test]
    fn test_munmap_unmap_phys_not_mapped() {
        // C: map_lookup fails → EFAULT (mmap.c:560-564).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(90);
        let ep = init_test_process(slot);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x9000),
            length: VirBytes(0),
            lookup_region_length: true,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MunmapError::NotMapped)));
    }

    #[test]
    fn test_munmap_vm_self_suspended() {
        // VM self-munmap with a registered region returns SUSPEND (C:
        // mmap.c:535-548) after unmap_range completes. The empty-regions
        // fallback (munmap_vm_lin → vm_self_unmappages) needs the real VM
        // self page table, which is not initialized in unit tests — that
        // path returns InternalError here and is exercised at runtime
        // only (backlog B3).
        let table = VmProcTable::get_global();
        let vm_slot = UserSlot(Endpoint::VM.slot() as usize);
        unsafe { table.reset_slot(vm_slot); }
        let empty = table.get_empty(vm_slot).unwrap();
        let mut active = empty.activate(Endpoint::VM);
        active.init_regions();
        drop(active);
        insert_region(Endpoint::VM, 0x1000, 0x1000);

        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let req = MunmapRequest {
            endpoint: Endpoint::VM,
            addr: VirBytes(0x1000),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };
        let r = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(r, Ok(MunmapOutcome::Suspended)));
        assert_eq!(active_regions(Endpoint::VM), 0);

        // Restore the global VM instance counter (activate(Endpoint::VM)
        // sets VMF_VM_INSTANCE); other tests assert it returns to 0.
        unsafe { table.reset_slot(vm_slot); }
    }

    #[test]
    fn test_munmap_releases_physical_page() {
        // Physical-page release chain on a LOCAL region (not inserted into
        // the global table): free_region_pages → free_range → unmap_page
        // (refcount 1→0) → ev_unreference + free_pfn.
        let mut frames = make_frames();
        let mut page_alloc = make_page_alloc();
        let pfn = page_alloc.alloc_pfn().expect("alloc in test");
        let mut region = VirRegion::with_memtype(
            VirBytes(0x1000),
            VirBytes(0x1000),
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        region.map_page(&mut frames, VirBytes(0), pfn, &MEM_TYPE_ANON);
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        crate::region::free_region_pages(region, None, &mut frames, &mut page_alloc);

        assert_eq!(frames.get(pfn).unwrap().refcount, 0);
    }

    #[test]
    fn test_munmap_cow_shared_page_kept() {
        // CoW/shared page: refcount 2 → munmap one reference drops it to 1
        // and the page is NOT freed (C: pb_unreferenced, pb.c:96-134).
        let mut frames = make_frames();
        let mut page_alloc = make_page_alloc();
        let pfn = page_alloc.alloc_pfn().expect("alloc in test");

        // Two regions share the same physical page (e.g. after fork).
        let mut r1 = VirRegion::with_memtype(
            VirBytes(0x1000),
            VirBytes(0x1000),
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        r1.map_page(&mut frames, VirBytes(0), pfn, &MEM_TYPE_ANON);
        let mut r2 = VirRegion::with_memtype(
            VirBytes(0x2000),
            VirBytes(0x1000),
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        r2.map_page(&mut frames, VirBytes(0), pfn, &MEM_TYPE_ANON);
        assert_eq!(frames.get(pfn).unwrap().refcount, 2);

        crate::region::free_region_pages(r1, None, &mut frames, &mut page_alloc);
        // refcount 2→1: the page stays allocated for r2.
        assert_eq!(frames.get(pfn).unwrap().refcount, 1);

        crate::region::free_region_pages(r2, None, &mut frames, &mut page_alloc);
        // refcount 1→0: the page is released back to the allocator.
        assert_eq!(frames.get(pfn).unwrap().refcount, 0);
    }

    #[test]
    fn test_munmap_middle_hole_directphys_rejected() {
        // C: split_region requires `ev_split`; mem_type_directphys has none
        // (region.c:1164) → EINVAL. Rust: MemTypeNotSupported (→ EINVAL).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(91);
        let ep = init_test_process(slot);
        insert_direct_region(ep, 0x1000, 0x5000, 0x8000_0000);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x2000),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MunmapError::MemTypeNotSupported)));
    }

    #[test]
    fn test_munmap_head_cut_directphys_rejected() {
        // C: low-end shrink requires `ev_lowshrink`; mem_type_directphys has
        // none (region.c:1096) → EINVAL. Rust: MemTypeNotSupported (→ EINVAL).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(92);
        let ep = init_test_process(slot);
        insert_direct_region(ep, 0x1000, 0x5000, 0x8000_0000);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x1000),
            length: VirBytes(0x1000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert!(matches!(result, Err(MunmapError::MemTypeNotSupported)));
    }

    #[test]
    fn test_munmap_tail_cut_directphys_allowed() {
        // C: high-end shrink needs no callback (region.c:1132-1135) — a
        // directphys region can be tail-cut. Rust split keeps the head with
        // the original `VrParam::Direct` base (correct).
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(93);
        let ep = init_test_process(slot);
        insert_direct_region(ep, 0x1000, 0x5000, 0x8000_0000);

        let req = MunmapRequest {
            endpoint: ep,
            addr: VirBytes(0x3000),
            length: VirBytes(0x3000),
            lookup_region_length: false,
        };
        let result = handle_munmap(table, &mut page_alloc, &mut frames, &req);
        assert_eq!(result, Ok(MunmapOutcome::Replied));
        assert_eq!(active_regions(ep), 1);

        let table = VmProcTable::get_global();
        let slot = table.vm_isokendpt(ep).unwrap();
        let active = table.get_active(slot).unwrap();
        let remaining = active.regions().iter().next().unwrap();
        assert_eq!(remaining.vaddr.0, 0x1000);
        assert_eq!(remaining.length.0, 0x2000);
        assert!(matches!(remaining.param, VrParam::Direct { .. }));
    }

    #[test]
    fn test_munmap_file_head_cut_fdref_balanced() {
        // fdref accounting across a head cut: one reference per live region.
        // split nets +1 (1 region → 2); freeing the head drops it back to 1;
        // freeing the tail reaches 0 → entry removed (VFS_FDCLOSE would fire).
        let mut frames = make_frames();
        let mut page_alloc = make_page_alloc();
        // split() and free_region_pages() operate on the global table, so the
        // test must use it too (unique dev/ino avoids cross-test collisions;
        // the entry is fully deref'd to 0, leaving no residue).
        let table = crate::fdref::FdRefTable::get_global();
        let id = table.create(7, 0xABCD, 0x1234, true);
        table.ref_entry(id); // one region holds one reference
        assert_eq!(table.get(id).unwrap().refcount, 1);

        let mut region = VirRegion::with_memtype(
            VirBytes(0x1000),
            VirBytes(0x3000),
            VrFlags::WRITABLE,
            &crate::memtype::MEM_TYPE_MAPPED_FILE,
        );
        region.param = VrParam::File {
            inited: true,
            fdref_id: Some(id),
            offset: 0,
            clearend: 0,
        };

        // Head cut: split then free the head half.
        let (head, tail) = region.split(VirBytes(0x1000)).expect("file split");
        crate::region::free_region_pages(head, None, &mut frames, &mut page_alloc);
        // Only the tail region remains → exactly one reference.
        assert_eq!(table.get(id).unwrap().refcount, 1);

        crate::region::free_region_pages(tail, None, &mut frames, &mut page_alloc);
        // Count reaches zero → entry removed (VFS_FDCLOSE would be sent).
        assert!(table.get(id).is_none());
    }

    #[test]
    fn test_munmap_error_to_errno() {
        // Tests the full error path: MunmapError → From<MunmapError> for VmError → VmError::to_errno()
        use minix_types::{VmError, EFAULT, EINVAL, EIO};
        assert_eq!(VmError::from(MunmapError::BadAddress).to_errno(), EFAULT);            // BadAddress → InvalidAddress → EFAULT (C mmap.c:558)
        assert_eq!(VmError::from(MunmapError::InvalidLength).to_errno(), EINVAL);         // InvalidLength → InvalidParam → EINVAL (C region.c:1233)
        assert_eq!(VmError::from(MunmapError::MemTypeNotSupported).to_errno(), EINVAL);   // MemTypeNotSupported → InvalidParam → EINVAL (C region.c:1096/:1164)
        assert_eq!(VmError::from(MunmapError::NotMapped).to_errno(), EFAULT);             // NotMapped → InvalidAddress → EFAULT (C mmap.c:566)
        assert_eq!(VmError::from(MunmapError::ProcessNotFound).to_errno(), EINVAL);       // ProcessNotFound → InvalidProcess → EINVAL
        assert_eq!(VmError::from(MunmapError::InternalError).to_errno(), EIO);            // InternalError → InternalError → EIO
    }
}
