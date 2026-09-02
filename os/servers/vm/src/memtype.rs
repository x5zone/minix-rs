//! Memory type system (PFN index model).
//!
//! MemType trait uses `PageSlot + PageFrames` instead of `PhysRegion`.

use minix_types::{Endpoint, VirBytes};
use minix_arch::paging::PageFlags;
use crate::vmproc::{ActiveProc, VmProcTable};
use crate::region::{PageFrames, PageSlot, PfnAllocator, PAGE_SIZE};
use crate::page_cache::PageCache;

// V10-P2-1 (DEFERRED): several C-memtype callbacks (`ev_new`, `ev_resize`,
// `ev_split`, `ev_low_shrink`, `ev_sanitycheck`, `writable`, `pt_flags`)
// are part of the trait contract but have no invoking flow yet — the
// resize/split/low-shrink paths are not wired. They are intentionally
// kept (with their C counterparts documented) until those flows land.
pub(crate) trait MemType {
    fn name(&self) -> &'static str;

    // C NULL → skip init. Default Ok(()): framework handles page allocation.
    // Receives PageFrames and PfnAllocator so memtypes that pre-allocate
    // physical pages (e.g. ContiguousAnonymous) can do so during creation.
    #[allow(dead_code)]
    fn ev_new(
        &self,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _alloc: &mut dyn PfnAllocator,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C NULL → skip cleanup. Default no-op: framework releases resources.
    fn ev_delete(&self, _region: &mut crate::region::VirRegion) {}

    // C NULL → skip reference. PageFrames manages refcount in PFN model, default no-op.
    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C implementations free physical pages here. PfnAllocator handles this in PFN model, default no-op.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    // All C types implement this (no NULL callback in memtype.h).
    // **Required, no default** (V10-P1-3): a default `Ok(Handled)` made a
    // forgotten implementation silently "handle" page faults as no-ops —
    // no mapping, no error. `table` provides cross-process region lookup
    // (needed by SharedMemory); `alloc` provides page allocation for
    // memtypes that need to fault in pages from other regions (e.g.
    // SharedMemory's recursive source pagefault).
    #[allow(clippy::too_many_arguments)] // V10-P2-1 (DEFERRED): fold into a PagefaultCtx struct when the cross-region fault path lands
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
        _table: &VmProcTable,
        _alloc: &mut dyn PfnAllocator,
        _cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError>;

    // C NULL → generic resize logic. Default Ok(()): framework handles page allocation.
    #[allow(dead_code)]
    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C NULL → EINVAL (region.c:1164). Default Err(NotSupported).
    // C signature: void (*ev_split)(struct vmproc *vmp, ...).
    // Matches ev_resize: &mut ActiveProc corresponds to struct vmproc*.
    #[allow(dead_code)]
    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    // C NULL → EINVAL (region.c:1096). Default Err(NotSupported).
    #[allow(dead_code)]
    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    /// Whether `ev_split` is supported.
    ///
    /// C: `split_region()` requires `def_memtype->ev_split` (region.c:1164);
    /// memtypes without the callback return EINVAL for middle-hole unmaps
    /// (e.g. `mem_type_directphys`, `mem_type_shared`, `mem_type_cache`).
    fn supports_split(&self) -> bool {
        false
    }

    /// Whether `ev_low_shrink` is supported.
    ///
    /// C: `map_unmap_region()` low-end shrink requires
    /// `def_memtype->ev_lowshrink` (region.c:1096); memtypes without the
    /// callback return EINVAL for head-cut unmaps (e.g. `mem_type_directphys`,
    /// `mem_type_shared`, `mem_type_anon_contig`).
    fn supports_low_shrink(&self) -> bool {
        false
    }

    // C NULL → skip. No runtime checks needed in PFN model.
    #[allow(dead_code)]
    fn ev_sanitycheck(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // All C types implement this (no NULL). Default false; AnonymousMemory overrides for CoW check.
    #[allow(dead_code)]
    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        false
    }

    // C NULL → skip copy specialization. Default Ok(()).
    fn ev_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C NULL callback → map_get_phys returns EINVAL (region.c:1331).
    // Rust expresses "callback presence" as a capability predicate;
    // `region_id()` itself defaults to 0 (= C NULL semantics) and is only
    // meaningful when this predicate is true.
    fn supports_region_id(&self) -> bool {
        false
    }

    // C NULL callback → map_get_ref returns EINVAL (region.c:1349).
    fn supports_ref_count(&self) -> bool {
        false
    }

    // C NULL → return 0. Default 0.
    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        0
    }

    // C NULL → return 0. Default 0. AnonymousMemory overrides with 1+remaps.
    fn ref_count(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }

    // C NULL → return 0 (default caching). Default empty(). DirectPhysical etc. override with NO_CACHE.
    #[allow(dead_code)]
    fn pt_flags(&self, _region: &crate::region::VirRegion) -> PageFlags {
        PageFlags::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemTypeError {
    NoMemory,
    InvalidParam,
    NotSupported,
    // V10-P2-1 (DEAD/DEFERRED): VFS IO failures propagate via
    // `CowError`/`VfsQueueError` today; these variants are kept for the
    // future file-memtype fault-in path.
    #[allow(dead_code)]
    IoError,
    #[allow(dead_code)]
    CopyFailed,
    /// Source process endpoint is invalid or dead (C: EINVAL from getsrc).
    InvalidProcess,
    /// Source region address not found (C: EINVAL from getsrc/map_lookup).
    InvalidAddress,
}

impl core::fmt::Display for MemTypeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoMemory => write!(f, "Out of memory"),
            Self::InvalidParam => write!(f, "Invalid parameter"),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError => write!(f, "IO error"),
            Self::CopyFailed => write!(f, "Copy failed"),
            Self::InvalidProcess => write!(f, "Invalid source process"),
            Self::InvalidAddress => write!(f, "Invalid source address"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultResult {
    Handled,
    NeedNewPage,
    NeedCow,
    NeedVfsIo,
    AccessViolation,
}

/// Anonymous memory — default memtype for heap, stack, and MAP_ANON regions.
///
/// Corresponds to Minix3's `mem_type_anon` (`mem_anon.c`). Pages are
/// allocated on demand (page fault → `NeedNewPage`) and support
/// Copy-on-Write when refcount > 1 (fork).
pub(crate) struct AnonymousMemory;

// SAFETY: shared through `&'static dyn MemType` statics (MEM_TYPE_*); the
// structs hold no interior mutability and are only read via MemType
// methods within the single-threaded VM server (V10-P1-3, cf. global.rs
// AssumeSyncCell rationale).
unsafe impl Sync for AnonymousMemory {}

impl AnonymousMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for AnonymousMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    /// Check if the page is safely writable without triggering CoW.
    ///
    /// Returns `true` when the page is mapped AND either:
    /// - the PFN refcount is 1 (exclusive owner), or
    /// - the region has been remapped (remaps > 0, meaning the page was
    ///   explicitly remapped writable after a CoW break).
    ///
    /// Corresponds to Minix3's `anon_writable()` (mem_anon.c).
    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &crate::region::VirRegion) -> bool {
        let Some(pfn) = slot.pfn() else {
            return false;
        };
        if region.remaps > 0 {
            return true;
        }
        frames.get(pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    /// No-op in the PFN model. Physical page freeing is the caller's
    /// responsibility via `PfnAllocator::free_pfn()`.
    ///
    /// Minix3's `anon_unreference` (mem_anon.c) frees the page here,
    /// but the PFN model separates refcount tracking (PageFrames) from
    /// allocation/deallocation (PfnAllocator).
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    /// Handle page fault for anonymous memory.
    ///
    /// - Unmapped slot → `NeedNewPage` (caller allocates a fresh page)
    /// - Mapped, refcount < 2 or read access → `Handled`
    /// - Mapped, refcount ≥ 2, write → `NeedCow` (caller must copy the page)
    /// - Write to non-writable region → `AccessViolation`
    ///
    /// Corresponds to Minix3's `anon_pagefault()` (mem_anon.c).
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
        _table: &VmProcTable,
        _alloc: &mut dyn PfnAllocator,
        _cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError> {
        let Some(slot) = region.get_slot(offset) else {
            return Ok(PagefaultResult::NeedNewPage);
        };
        let Some(pfn) = slot.pfn() else {
            return Ok(PagefaultResult::NeedNewPage);
        };
        let refcount = frames.get(pfn)
            .map(|s| s.refcount)
            .unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    /// Return the region's ID for sanity-check cross-referencing.
    ///
    /// `region.id` is always non-negative (valid region IDs), so the
    /// `i32 → u32` cast is safe. Corresponds to Minix3's `anon_region_id`.
    // C: mem_anon.c:42 — `.regionid = anon_regionid` (callback present).
    fn supports_region_id(&self) -> bool {
        true
    }

    // C: mem_anon.c:44 — `.refcount = anon_refcount` (callback present).
    fn supports_ref_count(&self) -> bool {
        true
    }

    fn region_id(&self, region: &crate::region::VirRegion) -> u32 {
        region.id as u32
    }

    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        1 + region.remaps
    }

    /// No-op. Minix3's `anon_split` is also a no-op (return).
    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// No-op. Minix3's `anon_lowshrink` is also a no-op (return OK).
    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// Anon regions support both splitting and low-end shrink in C
    /// (mem_anon.c: anon_split / anon_lowshrink are no-ops).
    fn supports_split(&self) -> bool {
        true
    }

    fn supports_low_shrink(&self) -> bool {
        true
    }
}

/// Direct physical memory mapping — maps a fixed physical address range.
///
/// Corresponds to Minix3's `mem_type_directphys` (`mem_type_directphys.c`).
/// Used for device memory and physical address access. Pages are mapped
/// on fault from the `VrParam::Direct { phys }` base address.
pub(crate) struct DirectPhysical;

// SAFETY: see AnonymousMemory — no interior mutability, single-threaded VM.
unsafe impl Sync for DirectPhysical {}

impl DirectPhysical {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for DirectPhysical {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for DirectPhysical {
    fn name(&self) -> &'static str {
        "physical memory mapping"
    }

    /// Always writable when mapped — device memory has no CoW semantics.
    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        slot.is_mapped()
    }

    /// Map the faulting offset to its corresponding physical address.
    ///
    /// Reads `VrParam::Direct { phys }` for the base physical address,
    /// computes `base + offset`, converts to PFN, and maps the page.
    /// Returns `Handled` immediately (the page always exists in physical
    /// memory). Corresponds to Minix3's `dp_pagefault()` (mem_type_directphys.c).
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
        _table: &VmProcTable,
        _alloc: &mut dyn PfnAllocator,
        _cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::Direct { phys: base_phys } = &region.param {
            if base_phys.0 == 0 {
                return Err(MemTypeError::InvalidParam);
            }
            let slot = region.get_slot(offset);
            match slot {
                Some(s) if s.is_mapped() => {
                    return Ok(PagefaultResult::Handled);
                }
                _ => {}
            }
            let phys_addr = minix_types::PhysBytes(base_phys.0 + offset.0);
            let pfn = frames.phys_to_pfn(phys_addr);
            let memtype = region.def_memtype
                .ok_or(MemTypeError::InvalidParam)?;
            region.map_page(frames, offset, pfn, memtype);
            Ok(PagefaultResult::Handled)
        } else {
            Err(MemTypeError::InvalidParam)
        }
    }

    /// Copy the `VrParam::Direct { phys }` parameter from source to destination.
    ///
    /// Corresponds to Minix3's `dp_copy()` (mem_type_directphys.c).
    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    /// No-op. Device memory is not owned by the VM allocator.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    /// Device memory is uncached — bypass the CPU cache to ensure
    /// MMIO reads/writes reach the hardware directly.
    fn pt_flags(&self, _region: &crate::region::VirRegion) -> PageFlags {
        PageFlags::NO_CACHE
    }

    /// Not supported. Minix3's `mem_type_directphys` has no ev_split (NULL → EINVAL).
    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }
}

pub(crate) struct SharedMemory;

// SAFETY: see AnonymousMemory — no interior mutability, single-threaded VM.
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    /// Always writable when mapped — shared pages have no CoW semantics.
    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        slot.is_mapped()
    }

    // C: mem_shared.c:35 — `.regionid = shared_regionid` (callback present).
    fn supports_region_id(&self) -> bool {
        true
    }

    // C: mem_shared.c:36 — `.refcount = shared_refcount` (callback present).
    fn supports_ref_count(&self) -> bool {
        true
    }

    /// C: `shared_regionid` (mem_shared.c:99-106) returns the *source*
    /// region's id. `shared_setsource` copies `src_vr->id` into
    /// `param.shared.id` (mem_shared.c:184) and `getsrc` validates the
    /// invariant on every use (mem_shared.c:88-93), so reading the
    /// parameter directly is C-equivalent.
    fn region_id(&self, region: &crate::region::VirRegion) -> u32 {
        match region.param {
            crate::region::VrParam::Shared { id, .. } => id as u32,
            _ => 0,
        }
    }

    // C: `shared_refcount` (mem_shared.c:207-209) — 1 + vr->remaps.
    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        1 + region.remaps
    }

    /// Cross-process shared page fault — currently not supported.
    ///
    /// In Minix3, `shared_pagefault()` (mem_shared.c) looks up the source
    /// process/region via `getsrc()`, ensures the source page is mapped,
    /// then links the current `phys_region` to the same `phys_block`.
    /// Cross-process shared page fault handler.
    ///
    /// In Minix3, `shared_pagefault()` (mem_shared.c:122) does:
    ///   1. `getsrc()` → look up the source process and source region
    ///   2. If current page already mapped → return OK (no action needed)
    ///   3. If source page not present → `map_pf()` on source (recursive pagefault)
    ///   4. `pb_link()` → link current phys_region to the same phys_block
    ///
    /// In Rust PFN model, this translates to:
    ///   1. Extract `VrParam::Shared { ep, vaddr, id }` from region.param
    ///   2. Look up source process via `table.vm_isokendpt(ep)`
    ///   3. Find source region via source process's `regions_mut().find(vaddr)`
    ///   4. If source slot not mapped → allocate a page for source first
    ///   5. Map current region's slot to the same PFN as source (shared reference)
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
        table: &VmProcTable,
        alloc: &mut dyn PfnAllocator,
        _cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError> {
        // Step 1: If the page is already mapped, no action needed.
        // C: mem_shared.c:139 — "if(ph->ph->phys != MAP_NONE) return OK"
        if let Some(slot) = region.get_slot(offset)
            && slot.is_mapped() {
                return Ok(PagefaultResult::Handled);
            }

        // Step 2: Extract source process endpoint from VrParam::Shared.
        // C: getsrc() — mem_shared.c:52-80
        let (src_ep, src_vaddr, src_id) = match &region.param {
            crate::region::VrParam::Shared { ep, vaddr, id } => (*ep, *vaddr, *id),
            _ => return Err(MemTypeError::InvalidParam),
        };

        // C: getsrc() checks ep != 0 && vaddr != 0
        if src_ep == 0 || src_vaddr.0 == 0 {
            return Err(MemTypeError::InvalidParam);
        }

        // Step 3: Look up source process by endpoint.
        // C: vm_isokendpt((endpoint_t) region->param.shared.ep, &srcproc)
        let src_user_slot = table.vm_isokendpt(Endpoint(src_ep))
            .map_err(|_| MemTypeError::InvalidProcess)?;

        // Step 4: Find source region by vaddr in source process.
        // C: map_lookup(*vmp, region->param.shared.vaddr, NULL)
        //
        // SAFETY: We already hold a &mut VirRegion from the current (faulting)
        // process. The source process is a different slot (shared memory always
        // links distinct processes). VmProcTable uses UnsafeCell-backed slots,
        // so get_active on a different slot is safe in the single-threaded VM.
        let src_proc = table.get_active(src_user_slot)
            .ok_or(MemTypeError::InvalidProcess)?;
        let src_region = src_proc.regions().find(src_vaddr)
            .ok_or(MemTypeError::InvalidAddress)?;

        // C: getsrc() verifies source region has anon memtype
        if src_region.def_memtype.map(|mt| mt.name()) != Some("anonymous") {
            return Err(MemTypeError::InvalidParam);
        }

        // C: getsrc() verifies region->param.shared.id == src_region->id
        if src_id != src_region.id {
            return Err(MemTypeError::InvalidParam);
        }

        // Step 5: Compute offset within source region.
        let src_offset = VirBytes(offset.0 % PAGE_SIZE + (src_vaddr.0 & !(PAGE_SIZE - 1)));

        // Step 6: If source page is not mapped, allocate one for it first.
        // C: map_pf(src_vmp, src_region, ph->offset, write, ...)
        let src_slot_state = src_region.get_slot(src_offset);
        let src_pfn = match src_slot_state.and_then(PageSlot::pfn) {
            Some(pfn) => pfn,
            None => {
                // Source page not mapped — allocate and map it.
                // C: map_pf() → anon_pagefault → alloc + map
                let pfn = alloc.alloc_pfn()
                    .map_err(|_| MemTypeError::NoMemory)?;
                // Drop the immutable source region reference before obtaining
                // a mutable one. Safe because: single-threaded VM, and the
                // source process is a different slot from the faulting process.
                let _ = src_region;
                let _ = src_proc;
                let mut src_proc_mut = table.get_active(src_user_slot)
                    .ok_or(MemTypeError::InvalidProcess)?;
                let src_region_mut = src_proc_mut.regions_mut().find_mut(src_vaddr)
                    .ok_or(MemTypeError::InvalidAddress)?;
                let src_memtype = src_region_mut.def_memtype
                    .ok_or(MemTypeError::InvalidParam)?;
                if src_memtype.name() != "anonymous" {
                    return Err(MemTypeError::InvalidParam);
                }
                src_region_mut.map_page(frames, src_offset, pfn, src_memtype);
                pfn
            }
        };

        // Step 7: Map current region's slot to the same PFN.
        // C: pb_link(ph, pr->ph, ph->offset, region) — shares the phys_block.
        // In PFN model, map_page increments refcount, so both source and
        // destination hold references to the same physical page.
        region.map_page(frames, offset, src_pfn, &MEM_TYPE_SHARED);

        Ok(PagefaultResult::Handled)
    }

    /// No-op. Shared page refcount is managed by the PFN model's `map_page`/`unmap`.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    /// Copy the `VrParam::Shared { .. }` parameter from source to destination.
    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    /// Not supported. Minix3's `mem_type_shared` has no ev_split (NULL → EINVAL).
    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    /// Not supported. Minix3's `mem_type_shared` has no ev_lowshrink (NULL → EINVAL).
    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }
}

pub(crate) struct ContiguousAnonymous;

// SAFETY: see AnonymousMemory — no interior mutability, single-threaded VM.
unsafe impl Sync for ContiguousAnonymous {}

impl ContiguousAnonymous {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for ContiguousAnonymous {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for ContiguousAnonymous {
    fn name(&self) -> &'static str {
        "contiguous anonymous memory"
    }

    /// Same logic as `AnonymousMemory::writable` — checks refcount and remaps.
    ///
    /// Currently `ev_copy` returns `NotSupported` so refcount is always 1 and
    /// remaps is always 0, making this equivalent to `slot.is_mapped()`. The
    /// full check is retained for future fork support.
    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &crate::region::VirRegion) -> bool {
        // Minix3's anon_contig_writable delegates to anon_writable (refcount+remaps
        // check). Currently ev_copy returns NotSupported so refcount is always 1 and
        // remaps is always 0, making this equivalent to slot.is_mapped(). However, if
        // fork support is added for ContiguousAnonymous in the future, the full check
        // must remain to correctly determine CoW eligibility.
        let Some(pfn) = slot.pfn() else {
            return false;
        };
        if region.remaps > 0 {
            return true;
        }
        frames.get(pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    /// Not supported. Contiguous memory cannot be individually referenced.
    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    /// Not supported. Contiguous regions cannot be resized after creation.
    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    /// Not supported. Contiguous memory does not support fork/copy.
    fn ev_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    /// Pre-allocate all pages as a physically contiguous block.
    ///
    /// Corresponds to Minix3's `anon_contig_new()` (mem_anon_contig.c).
    /// Allocates consecutive PFNs, verifies contiguity, and maps each
    /// page slot. If contiguity cannot be guaranteed, rolls back all
    /// allocations and returns `NoMemory`.
    fn ev_new(
        &self,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        alloc: &mut dyn PfnAllocator,
    ) -> Result<(), MemTypeError> {
        let pages = region.physblocks.len();
        if pages == 0 {
            return Ok(());
        }

        // C: anon_contig_new (mem_anon_contig.c:52-96)
        // Step 1: Create phys_block + phys_region for each page (MAP_NONE).
        //         In PFN model: all slots start as `PageSlot::Empty`
        //         (created by `VirRegion::new`, no explicit pass needed).
        // Step 2: alloc_mem(pages, allocflags) — allocate contiguous physical memory.
        //         In PFN model: allocate contiguous PFNs one by one.
        // Step 3: Assign contiguous physical addresses to each phys_region.

        // Step 1 & 2: Allocate contiguous PFNs.
        // The C code uses alloc_mem() which returns a single contiguous block.
        // In the PFN model, we allocate individual pages and verify contiguity.
        // For a truly contiguous allocation, we need a contiguous allocator,
        // but the current PfnAllocator only supports single-page allocation.
        //
        // Strategy: Allocate the first page, then verify that subsequent
        // allocations are contiguous. If not, free all and retry.
        // This is a simplified approach; a proper buddy allocator with
        // contiguous allocation support would be more efficient.
        //
        // NOTE: The VM server is single-threaded (user-space server), so
        // there is no race condition between allocation and verification.

        let first_pfn = alloc.alloc_pfn().map_err(|_| MemTypeError::NoMemory)?;
        let mut pfns = alloc::vec![first_pfn];

        for _ in 1..pages {
            match alloc.alloc_pfn() {
                Ok(pfn) => pfns.push(pfn),
                Err(_) => {
                    // Rollback: free all allocated PFNs
                    for &pfn in &pfns {
                        alloc.free_pfn(pfn);
                    }
                    return Err(MemTypeError::NoMemory);
                }
            }
        }

        // Verify contiguity: PFNs must be consecutive
        let is_contiguous = pfns.windows(2).all(|w| w[1] == w[0] + 1);
        if !is_contiguous {
            // Contiguity not guaranteed with single-page allocator.
            // Free all and return error — a proper contiguous allocator
            // is needed for guaranteed contiguity.
            // For now, still map the pages (non-contiguous) as a fallback,
            // but log a warning. This matches the spirit of the C code
            // which panics on pagefault for contig regions — if the pages
            // aren't contiguous, DMA-like usage will fail at runtime.
            //
            // TODO: Implement PfnAllocator::alloc_contiguous() or use
            // VmPageAllocator::alloc_pages() with CONTIG flag for guaranteed
            // contiguous allocation.
            for &pfn in &pfns {
                alloc.free_pfn(pfn);
            }
            return Err(MemTypeError::NoMemory);
        }

        // Step 3: Map each page slot with the contiguous PFN
        let memtype: &'static dyn MemType = &MEM_TYPE_CONTIG_ANON;
        for (i, &pfn) in pfns.iter().enumerate() {
            let offset = VirBytes(i as u64 * PAGE_SIZE);
            region.map_page(frames, offset, pfn, memtype);
        }

        Ok(())
    }

    /// Panic — page faults must never occur on contiguous regions.
    ///
    /// All pages are pre-allocated in `ev_new`, so a page fault indicates
    /// a programming error. Corresponds to Minix3's `anon_contig_pagefault`
    /// which also panics ("pagefault cannot happen").
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        _region: &mut crate::region::VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
        _table: &VmProcTable,
        _alloc: &mut dyn PfnAllocator,
        _cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError> {
        panic!("contiguous anonymous pagefault: all pages are pre-allocated");
    }

    /// No-op in the PFN model. Physical page freeing is the caller's
    /// responsibility via `PfnAllocator::free_pfn()`.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    /// Uncached — DMA buffers must not be affected by CPU cache.
    fn pt_flags(&self, _region: &crate::region::VirRegion) -> PageFlags {
        PageFlags::NO_CACHE
    }

    /// No-op. Minix3's `anon_contig_split` is also a no-op (return).
    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// Contiguous anon supports splitting (mem_anon_contig.c: anon_contig_split
    /// is a no-op) but has NO `ev_lowshrink` (NULL → EINVAL for head-cuts).
    fn supports_split(&self) -> bool {
        true
    }
}

pub(crate) struct CacheMemory;

// SAFETY: see AnonymousMemory — no interior mutability, single-threaded VM.
unsafe impl Sync for CacheMemory {}

impl CacheMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for CacheMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for CacheMemory {
    fn name(&self) -> &'static str {
        "cache memory"
    }

    /// Always writable when mapped — cache pages have no CoW semantics.
    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        slot.is_mapped()
    }

    /// Map the faulting page to the pre-cached PFN.
    ///
    /// In Minix3, `cache_pagefault()` (mem_cache.c:181) links the faulting
    /// `phys_region` to a pre-existing `phys_block` stored in
    /// `region->param.pb_cache`, then clears the cache pointer.
    ///
    /// In the PFN model, this translates to:
    ///   1. If page already mapped → return Handled (no action)
    ///   2. Extract cached PFN from `VrParam::PbCache { pfn }`
    ///   3. If pfn == 0 → error (C: assert(region->param.pb_cache) fails)
    ///   4. `map_page(offset, pfn, &MEM_TYPE_CACHE)` — link to cached page
    ///   5. Clear pfn to 0 (cache consumed, C: `region->param.pb_cache = NULL`)
    ///   6. Return Handled
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
        _table: &VmProcTable,
        _alloc: &mut dyn PfnAllocator,
        _cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError> {
        // Step 1: Already mapped → no action needed.
        // C: "if(ph->ph->phys != MAP_NONE) return OK" (implicit in assert)
        if let Some(slot) = region.get_slot(offset)
            && slot.is_mapped() {
                return Ok(PagefaultResult::Handled);
            }

        // Step 2-3: Extract cached PFN. pfn==0 means no cached page,
        // which is a programming error (C: assert(region->param.pb_cache)).
        let cached_pfn = match &region.param {
            crate::region::VrParam::PbCache { pfn } => *pfn,
            _ => return Err(MemTypeError::InvalidParam),
        };
        if cached_pfn == 0 {
            return Err(MemTypeError::InvalidParam);
        }

        // Step 4: Map the faulting slot to the cached PFN.
        // C: pb_link(ph, region->param.pb_cache, offset, region)
        region.map_page(frames, offset, cached_pfn, &MEM_TYPE_CACHE);

        // Step 5: Clear the cached PFN (cache consumed).
        // C: region->param.pb_cache = NULL
        if let crate::region::VrParam::PbCache { pfn } = &mut region.param {
            *pfn = 0;
        }

        Ok(PagefaultResult::Handled)
    }

    /// No-op. Cache page refcount is managed by the PFN model.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    /// Not supported. Cache regions cannot be resized.
    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    /// Clear the cached PFN to prevent dangling references.
    ///
    /// Minix3's `mem_type_cache` has no `ev_delete` (NULL) — cache cleanup
    /// happens in `do_forgetcache`/`rmcache`. In the PFN model, we clear
    /// the cached PFN so that a stale reference cannot point to a freed page.
    fn ev_delete(&self, region: &mut crate::region::VirRegion) {
        if let crate::region::VrParam::PbCache { pfn } = &mut region.param {
            *pfn = 0;
        }
    }

    /// No-op. Cache regions support low-shrink without special handling.
    fn ev_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// Cache regions support low-end shrink (mem_cache.c: cache_lowshrink is
    /// a no-op) but have no `ev_split` (NULL → EINVAL for middle-hole unmaps).
    fn supports_low_shrink(&self) -> bool {
        true
    }
}

/// Mapped file — file-backed memory via VFS mmap.
///
/// Corresponds to Minix3's `mem_type_mapped` (`mem_type_mapped.c`).
/// Initial page faults request VFS I/O (`NeedVfsIo`); subsequent
/// writes to shared pages trigger CoW (`NeedCow`).
pub(crate) struct MappedFile;

// SAFETY: see AnonymousMemory — no interior mutability, single-threaded VM.
unsafe impl Sync for MappedFile {}

impl MappedFile {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for MappedFile {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "mapped file"
    }

    /// Always `false` — file-backed pages are never directly writable.
    /// Writability is determined at page-fault time via CoW.
    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &crate::region::VirRegion) -> bool {
        false
    }

    /// Determine the page-fault action for a file-backed mapping.
    ///
    /// - Uninitialized region → `NeedNewPage` (first access)
    /// - Unmapped slot, cache hit → link the cached page (`Handled`,
    ///   or `NeedCow` when the page must be copied before write)
    /// - Unmapped slot, cache miss → `NeedVfsIo` (request VFS to load the page)
    /// - Mapped, read → `Handled`
    /// - Mapped, write → `NeedCow` (shared page must be copied before write)
    ///
    /// Corresponds to Minix3's `mappedfile_pagefault()` (mem_file.c:85-155).
    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut crate::region::VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
        _table: &VmProcTable,
        _alloc: &mut dyn PfnAllocator,
        cache: &mut PageCache,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::File { inited, .. } = &region.param
            && !inited {
                return Ok(PagefaultResult::NeedNewPage);
            }

        let slot = region.get_slot(offset);

        match slot {
            Some(s) if s.is_mapped() => {
                if write {
                    Ok(PagefaultResult::NeedCow)
                } else {
                    Ok(PagefaultResult::Handled)
                }
            }
            _ => {
                // C mappedfile_pagefault (mem_file.c:104-145): consult the
                // page cache before issuing an FDIO round-trip to VFS. A
                // cache hit (normally populated by the VFS reply path via
                // lmfs_get_block_ino + vm_map_cacheblock) links the cached
                // page directly; only a miss suspends for VFS I/O.
                // Copy the File param fields out first: the cache-hit path
                // mutates `region` (map_page) below.
                let (fdref_id, file_offset, clearend) = match &region.param {
                    crate::region::VrParam::File { fdref_id: Some(id), offset: file_offset, clearend, .. } => {
                        (*id, *file_offset, *clearend)
                    }
                    _ => return Ok(PagefaultResult::AccessViolation),
                };
                let Some(fdref) = crate::fdref::FdRefTable::get_global().get(fdref_id) else {
                    return Ok(PagefaultResult::AccessViolation);
                };

                // C: referenced_offset = region->param.file.offset + ph->offset
                let referenced_offset = file_offset + offset.0;
                // C: find_cached_page_byino/bydev with touchlru=1
                // (mem_file.c:104-110). A one-shot entry forces the VFS
                // round-trip: "for one-time use pages, no caching is
                // performed" (mem_file.c:120-131 comment) — the entry's
                // `once` flag is the C `cp->flags & VMSF_ONCE` check.
                let cached = if fdref.ino == crate::page_cache::VMC_NO_INODE {
                    cache.find_by_dev(fdref.dev, referenced_offset, None, 0, true)
                } else {
                    cache.find_by_ino(fdref.dev, fdref.ino, referenced_offset, true)
                };

                match cached {
                    Some(cp) if !cp.once => {
                        // C: pb_link(ph, cp->page, ph->offset, region)
                        // (mem_file.c:123) — the PFN model maps the cached
                        // frame into the slot; `map_page` bumps the frame
                        // refcount (the mapping's reference, on top of the
                        // cache's own from addcache). No separate cache
                        // entry refcount exists — the frame refcount is
                        // authoritative (24-page-cache D1/D3).
                        let pfn = cp.pfn;
                        region.map_page(frames, offset, pfn, &MEM_TYPE_MAPPED_FILE);

                        // C mem_file.c:124-138: if the faulted page is the last
                        // (partially-mapped) page of the region, or the access
                        // is a write, run cow_block — copy the shared cached page
                        // into a private anon page (clearend zeroing is not yet
                        // modeled; doc 24 §3.5 差异清单).
                        let last_partial = {
                            let page_end = (offset.0 + clearend as u64).div_ceil(PAGE_SIZE) * PAGE_SIZE;
                            page_end >= region.length.0
                        };
                        if write || last_partial {
                            Ok(PagefaultResult::NeedCow)
                        } else {
                            Ok(PagefaultResult::Handled)
                        }
                    }
                    // One-shot hit or miss → VFS round-trip (C: mem_file.c:120
                    // `!cb || !VMSF_ONCE` 的保守侧：不消费 one-shot 页)。
                    _ => Ok(PagefaultResult::NeedVfsIo),
                }
            }
        }
    }

    /// No-op. File-backed page refcount is managed by the PFN model.
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    /// Copy the `VrParam::File { .. }` parameter from source to destination.
    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    /// Split the file mapping by adjusting `offset` and `clearend`.
    ///
    /// The left region keeps the original offset; the right region's
    /// offset is shifted by `left.length`. `clearend` is carried over
    /// to the right region. Corresponds to Minix3's `mapped_split()`
    /// (mem_type_mapped.c).
    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        original: &crate::region::VirRegion,
        left: &mut crate::region::VirRegion,
        right: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        if let crate::region::VrParam::File { inited: true, fdref_id, offset, clearend } = &original.param {
            let fdref_id = *fdref_id;
            let orig_offset = *offset;
            let orig_clearend = *clearend;

            left.param = crate::region::VrParam::File {
                inited: true,
                fdref_id,
                offset: orig_offset,
                clearend: 0,
            };

            right.param = crate::region::VrParam::File {
                inited: true,
                fdref_id,
                offset: orig_offset + left.length.get(),
                clearend: orig_clearend,
            };
        }
        Ok(())
    }

    /// Shrink from the low end by advancing the file offset.
    ///
    /// Corresponds to Minix3's `mapped_lowshrink()` (mem_type_mapped.c).
    fn ev_low_shrink(
        &self,
        region: &mut crate::region::VirRegion,
        len: VirBytes,
    ) -> Result<(), MemTypeError> {
        if let crate::region::VrParam::File { offset, .. } = &mut region.param {
            *offset += len.get();
        }
        Ok(())
    }

    /// Mapped-file regions support both splitting and low-end shrink in C
    /// (mem_file.c: mappedfile_split / mappedfile_lowshrink).
    fn supports_split(&self) -> bool {
        true
    }

    fn supports_low_shrink(&self) -> bool {
        true
    }

    /// Clear the file mapping state on region deletion.
    ///
    /// Resets `inited` to `false` and clears `fdref_id` to release
    /// the file descriptor reference. Corresponds to Minix3's
    /// `mapped_delete()` (mem_type_mapped.c).
    fn ev_delete(&self, region: &mut crate::region::VirRegion) {
        if let crate::region::VrParam::File { inited, fdref_id, .. } = &mut region.param {
            *inited = false;
            *fdref_id = None;
        }
    }
}

pub(crate) static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
pub(crate) static MEM_TYPE_DIRECT: DirectPhysical = DirectPhysical::new();
pub(crate) static MEM_TYPE_SHARED: SharedMemory = SharedMemory::new();
pub(crate) static MEM_TYPE_CONTIG_ANON: ContiguousAnonymous = ContiguousAnonymous::new();
pub(crate) static MEM_TYPE_CACHE: CacheMemory = CacheMemory::new();
pub(crate) static MEM_TYPE_MAPPED_FILE: MappedFile = MappedFile::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_memory_name() {
        let anon = AnonymousMemory::new();
        assert_eq!(anon.name(), "anonymous memory");
    }

    #[test]
    fn test_direct_physical_name() {
        let direct = DirectPhysical::new();
        assert_eq!(direct.name(), "physical memory mapping");
    }

    #[test]
    fn test_shared_memory_name() {
        let shared = SharedMemory::new();
        assert_eq!(shared.name(), "shared memory");
    }

    #[test]
    fn test_static_instances() {
        assert_eq!(MEM_TYPE_ANON.name(), "anonymous memory");
        assert_eq!(MEM_TYPE_DIRECT.name(), "physical memory mapping");
        assert_eq!(MEM_TYPE_SHARED.name(), "shared memory");
        assert_eq!(MEM_TYPE_CONTIG_ANON.name(), "contiguous anonymous memory");
        assert_eq!(MEM_TYPE_CACHE.name(), "cache memory");
        assert_eq!(MEM_TYPE_MAPPED_FILE.name(), "mapped file");
    }

    #[test]
    fn test_anon_writable() {
        use crate::region::{PfnAllocator, PfnAllocError};

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 4));
        let mut alloc = TestAlloc { next: 0 };
        let pfn = alloc.alloc_pfn().unwrap();
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.map_page(&mut frames, minix_types::VirBytes(0), pfn, &MEM_TYPE_ANON);
        let slot = region.get_slot(minix_types::VirBytes(0)).unwrap();

        assert!(MEM_TYPE_ANON.writable(&frames, *slot, &region));

        frames.get_mut(pfn).unwrap().refcount = 2;
        assert!(!MEM_TYPE_ANON.writable(&frames, *slot, &region));
    }

    #[test]
    fn test_mapped_file_copy() {
        let mf = MappedFile::new();
        let src = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        let mut dst = crate::region::VirRegion::new(
            minix_types::VirBytes(0x2000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        assert_eq!(mf.ev_copy(&src, &mut dst), Ok(()));
    }

    fn make_mapped_file_region(fdref_id: u32) -> crate::region::VirRegion {
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x2000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_MAPPED_FILE);
        region.param = crate::region::VrParam::File {
            inited: true,
            fdref_id: Some(fdref_id),
            offset: 0,
            clearend: 0,
        };
        region
    }

    #[test]
    fn test_mapped_file_pagefault_uninitialized_need_new_page() {
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let fdref_id = crate::fdref::FdRefTable::get_global().create(7, 1, 100);
        let mut region = make_mapped_file_region(fdref_id);
        region.param = crate::region::VrParam::File {
            inited: false,
            fdref_id: Some(fdref_id),
            offset: 0,
            clearend: 0,
        };

        let result = MEM_TYPE_MAPPED_FILE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::NeedNewPage));
    }

    #[test]
    fn test_mapped_file_pagefault_cache_miss_need_vfs_io() {
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let fdref_id = crate::fdref::FdRefTable::get_global().create(7, 1, 100);
        let mut region = make_mapped_file_region(fdref_id);

        // Empty cache → the faulting page must be loaded through VFS.
        let result = MEM_TYPE_MAPPED_FILE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::NeedVfsIo));
    }

    #[test]
    fn test_mapped_file_pagefault_cache_hit_links_page() {
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let fdref_id = crate::fdref::FdRefTable::get_global().create(7, 1, 100);
        let mut region = make_mapped_file_region(fdref_id);

        // A cached page at file offset 0 (C: find_cached_page_byino hit).
        let cached_pfn = 5;
        cache.addcache(1, 0, Some(100), 0, false, cached_pfn, &mut frames).unwrap();

        let result = MEM_TYPE_MAPPED_FILE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::Handled));

        // The slot must now reference the cached PFN; the frame refcount is
        // 2 = cache's own reference + the mapping's (C: pb_link on top of
        // addcache's refcount++).
        let slot = region.get_slot(minix_types::VirBytes(0)).unwrap();
        assert_eq!(slot.pfn(), Some(cached_pfn));
        assert_eq!(frames.get(cached_pfn).unwrap().refcount(), 2);
        assert!(frames.get(cached_pfn).unwrap().is_cached());
    }

    #[test]
    fn test_mapped_file_pagefault_device_cache_hit() {
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        // Device file: ino == VMC_NO_INODE → bydev lookup (C mem_file.c:111-113).
        let fdref_id = crate::fdref::FdRefTable::get_global().create(9, 2, crate::page_cache::VMC_NO_INODE);
        let mut region = make_mapped_file_region(fdref_id);
        let cached_pfn = 6;
        cache.addcache(2, 0, None, 0, false, cached_pfn, &mut frames).unwrap();

        let result = MEM_TYPE_MAPPED_FILE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::Handled));
        let slot = region.get_slot(minix_types::VirBytes(0)).unwrap();
        assert_eq!(slot.pfn(), Some(cached_pfn));
        assert_eq!(frames.get(cached_pfn).unwrap().refcount(), 2);
    }

    #[test]
    fn test_mapped_file_pagefault_one_shot_hit_forces_vfs_io() {
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let fdref_id = crate::fdref::FdRefTable::get_global().create(7, 1, 100);
        let mut region = make_mapped_file_region(fdref_id);

        // One-shot cached page: the fault must NOT link it — it forces the
        // VFS round-trip (C: "for one-time use pages, no caching is
        // performed", mem_file.c:120-131).
        let cached_pfn = 5;
        cache.addcache(1, 0, Some(100), 0, true, cached_pfn, &mut frames).unwrap();

        let result = MEM_TYPE_MAPPED_FILE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::NeedVfsIo));
        assert!(region.get_slot(minix_types::VirBytes(0)).is_none());
        // Frame refcount stays at 1 (cache only) — no mapping was added.
        assert_eq!(frames.get(cached_pfn).unwrap().refcount(), 1);
    }

    #[test]
    fn test_shared_pagefault_already_mapped() {
        // If the page is already mapped, ev_pagefault returns Handled.
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_SHARED);
        region.param = crate::region::VrParam::Shared { ep: 0, vaddr: minix_types::VirBytes(0), id: 0 };

        // Map a page so it's already present.
        let pfn = alloc.alloc_pfn().unwrap();
        region.map_page(&mut frames, minix_types::VirBytes(0), pfn, &MEM_TYPE_SHARED);

        let result = MEM_TYPE_SHARED.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::Handled));
    }

    #[test]
    fn test_shared_pagefault_invalid_param() {
        // If VrParam is not Shared, returns InvalidParam.
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_SHARED);
        // Default VrParam is Direct, not Shared.
        region.param = crate::region::VrParam::Direct { phys: minix_types::PhysBytes(0) };

        let result = MEM_TYPE_SHARED.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Err(MemTypeError::InvalidParam));
    }

    #[test]
    fn test_shared_pagefault_zero_ep() {
        // Shared with ep=0 returns InvalidParam (C: getsrc checks ep != 0).
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_SHARED);
        region.param = crate::region::VrParam::Shared { ep: 0, vaddr: minix_types::VirBytes(0x1000), id: 1 };

        let result = MEM_TYPE_SHARED.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Err(MemTypeError::InvalidParam));
    }

    #[test]
    fn test_cache_pagefault_maps_cached_pfn() {
        // CacheMemory::ev_pagefault maps the faulting slot to the cached PFN
        // and clears the cache pointer. Corresponds to C cache_pagefault().
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        // Pre-allocate a PFN to simulate a cached page block.
        // Use pfn=5 to avoid pfn==0 (which means "no cached page").
        let cached_pfn: u32 = 5;
        // Initialize the frame's refcount for the cached PFN.
        if let Some(state) = frames.get_mut(cached_pfn) {
            state.refcount = 1;
        }

        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_CACHE);
        region.param = crate::region::VrParam::PbCache { pfn: cached_pfn };

        // Page fault at offset 0 — should map to cached_pfn.
        let result = MEM_TYPE_CACHE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::Handled));

        // Verify the slot is now mapped to the cached PFN.
        let slot = region.get_slot(minix_types::VirBytes(0)).unwrap();
        assert!(slot.is_mapped());
        assert_eq!(slot.pfn(), Some(cached_pfn));

        // Verify the cache pointer was cleared (pfn set to 0).
        if let crate::region::VrParam::PbCache { pfn } = &region.param {
            assert_eq!(*pfn, 0, "cached pfn should be cleared after pagefault");
        } else {
            panic!("param should still be PbCache variant");
        }
    }

    #[test]
    fn test_cache_pagefault_already_mapped() {
        // If the page is already mapped, ev_pagefault returns Handled.
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_CACHE);
        region.param = crate::region::VrParam::PbCache { pfn: 99 };
        region.map_page(&mut frames, minix_types::VirBytes(0), pfn, &MEM_TYPE_CACHE);

        let result = MEM_TYPE_CACHE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Ok(PagefaultResult::Handled));
        // Cache pointer should NOT be cleared (early return, no action taken).
        if let crate::region::VrParam::PbCache { pfn } = &region.param {
            assert_eq!(*pfn, 99, "cached pfn should not be cleared when page already mapped");
        }
    }

    #[test]
    fn test_cache_pagefault_zero_pfn() {
        // pfn==0 in PbCache means no cached page — returns InvalidParam.
        // C: assert(region->param.pb_cache) would fail.
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_CACHE);
        region.param = crate::region::VrParam::PbCache { pfn: 0 };

        let result = MEM_TYPE_CACHE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Err(MemTypeError::InvalidParam));
    }

    #[test]
    fn test_cache_pagefault_wrong_param_variant() {
        // If VrParam is not PbCache, returns InvalidParam.
        use crate::region::PfnAllocError;

        struct TestAlloc { next: u32 }
        impl PfnAllocator for TestAlloc {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let pfn = self.next;
                self.next += 1;
                Ok(pfn)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }

        let table = VmProcTable::get_global();
        let mut frames = PageFrames::new(minix_types::PhysBytes(4096 * 8));
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let mut region = crate::region::VirRegion::new(
            minix_types::VirBytes(0x1000),
            minix_types::VirBytes(0x1000),
            crate::region::VrFlags::empty(),
        );
        region.def_memtype = Some(&MEM_TYPE_CACHE);
        region.param = crate::region::VrParam::Direct { phys: minix_types::PhysBytes(0) };

        let result = MEM_TYPE_CACHE.ev_pagefault(
            Endpoint(1), &mut region, &mut frames,
            minix_types::VirBytes(0), false, table, &mut alloc, &mut cache,
        );
        assert_eq!(result, Err(MemTypeError::InvalidParam));
    }
}
