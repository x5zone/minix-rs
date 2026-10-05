//! VM brk (heap management) implementation.
//!
//! Handles VM_BRK requests to grow the data segment of a process toward an
//! **absolute** new break address. Senders own the break value: C libc keeps
//! `_brksize` (arch brksize.S seeds it at the link-time `_end`; libc
//! sys/brk.c caches every move), and the VM never reads it back.
//!
//! Corresponds to Minix3's `do_brk()`/`real_brk()` (break.c:44-70) on top of
//! `map_region_extend_upto_v()` (region.c:1002-1064).
//!
//! ## Design decisions vs Minix3
//!
//! **Extend-or-ENOENT, never shrink** (P0, NK4-C §续-279m): C's VM side only
//! ever extends — a request already covered by the containing region returns
//! OK as a no-op (region.c:1017 `if(vr->vaddr + vr->length >= v) return OK`),
//! and there is *no* shrink leg at all in break.c. The previous minix-rs model
//! treated `vm_region_top` as the authoritative current break and freed
//! regions below a lower request — but C's `vm_region_top` is only a
//! slot-placement hint (region.c:391), so the delta-comparison could fire a
//! destructive shrink against addresses userspace never asked to release.
//! Requests below the break now succeed as no-ops; pages come back only via
//! munmap/exit, exactly like C.
//!
//! **Region conflict check**: C asserts `offset <= nextvr->vaddr` before
//! growing (region.c:1029-1032) and fails with "can't grow into next region"
//! when it does not hold (region.c:1034-1038). Rust mirrors both with
//! `find_overlap` on the extension span.
//!
//! **New-region fallback** (create leg): when the containing region has no
//! slot array to extend (boot-image regions materialize pages outside
//! `physblocks`), growth is realized as an adjacent ANON region at the old
//! end — the same end state C produces via `map_page_region(vmp, limit, 0,
//! extralen, VR_WRITABLE|VR_ANON, ...)` (region.c:1040-1044, the
//! no-ev_resize branch).

use minix_types::{Endpoint, VirBytes};
use crate::vmproc::{VmProcTable, ActiveProc, EndpointError};
use crate::region::{VirRegion, VrFlags, RegionMap};
use crate::memtype::MEM_TYPE_ANON;
use crate::region::page_state::PAGE_SIZE;

/// Errors from VM_BRK (heap resize) operations.
///
/// All variants map to `VmError` via `From<BrkError> for VmError`,
/// then to C errno via `VmError::to_errno()`. Per-error `to_errno()`
/// methods are intentionally omitted — the single source of truth is
/// `VmError::to_errno()` in `minix_types::ipc::vm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrkError {
    ProcessNotFound,
    OutOfMemory,
}

// ── Endpoint-lookup error unification ──
//
// See `munmap.rs` for the full rationale. brk doesn't distinguish
// INVALID-slot from DEAD-endpoint.
impl From<EndpointError> for BrkError {
    fn from(_: EndpointError) -> Self {
        BrkError::ProcessNotFound
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
    request: &BrkRequest,
) -> Result<BrkResponse, BrkError> {
    let slot = table.vm_isokendpt(request.endpoint)?;

    let mut active = table.get_active(slot)
        .ok_or(BrkError::ProcessNotFound)?;

    // 续-279m 临时取证探针（first-N 封顶 160，真机 D3a 定性用，结案滚除）：
    // 只打**真正跨出 region 的扩腿请求**（covered no-op 海量不刷），附带
    // 扩腿后的 region 形状。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static BRK_LOG: AtomicUsize = AtomicUsize::new(0);
        let off = request.new_brk_addr.0.div_ceil(PAGE_SIZE as u64) * PAGE_SIZE as u64;
        let pre = avl_less(active.regions(), off);
        if pre.is_some_and(|r| r.end_addr().0 < off)
            && BRK_LOG.fetch_add(1, AtomicOrd::Relaxed) < 160
        {
        }
    }

    // C: real_brk → map_region_extend_upto_v (region.c:1002-1064), the only
    // brk leg in break.c — the request carries the ABSOLUTE new break and
    // the VM extends the region containing it. No delta against
    // vm_region_top: that field is a slot-placement hint in C (region.c:391),
    // not a break value (§续-279m redesign, replacing the gh149/gh150
    // derive/retry incidents).
    extend_upto_v(&mut active, request.new_brk_addr)
        .map(|covered| BrkResponse { new_brk_addr: covered })
}

/// C `map_region_extend_upto_v` (region.c:1002-1064), literal port.
///
/// Returns the covered-up-to address on OK. `requested` stays authoritative
/// in the caller (C: libc caches `_brksize = addr`, not the page-rounded
/// extent the VM actually mapped).
fn extend_upto_v(
    active: &mut ActiveProc<'_>,
    requested: VirBytes,
) -> Result<VirBytes, BrkError> {
    // C:1009 `offset = roundup(offset, VM_PAGE_SIZE)`.
    let offset = VirBytes(requested.0.div_ceil(PAGE_SIZE as u64) * PAGE_SIZE as u64);

    // C:1011-1014 `region_search(&avm, offset, AVL_LESS)` — the region with
    // the greatest vaddr **<= offset**, containment NOT required (C then
    // asserts `vr->vaddr <= offset` at :1020 and extends the region past its
    // own end across the gap). `find_mut` is a containing lookup and
    // `search(Less)` is a strict `<` — neither expresses AVL_LESS `<=`;
    // `avl_less` below is the literal reading (§续-279m: the first port
    // attempt used `find_mut` and reproduced C's "nothing to extend" path on
    // every gap-crossing brk).
    let (base, limit) = avl_less(active.regions(), offset.0)
        .map(|r| (r.vaddr, r.end_addr()))
        .ok_or(BrkError::OutOfMemory)?; // C:1012-1013 "VM: nothing to extend"

    // C:1016 already covered → OK without touching anything. This is also the
    // whole shrink story in C: a lower break never unmaps.
    if limit.0 >= offset.0 {
        return Ok(limit);
    }

    // C:1028-1035 `nextvr = getnextvr(vr)` is the *immediate successor*
    // region: `assert(offset <= nextvr->vaddr)` (release builds assert no
    // overlap) and `nextvr->vaddr < offset` → "can't grow into next region"
    // + ENOMEM. A full span-overlap test would be stricter than C (it also
    // rejects a successor that contains offset — that shape is C's covered
    // arm above), so the port keeps the successor-only reading.
    let blocked = active
        .regions()
        .iter()
        .filter(|r| r.vaddr.0 > base.0)
        .min_by_key(|r| r.vaddr.0)
        .is_some_and(|nextvr| nextvr.vaddr.0 < offset.0);
    if blocked {
        return Err(BrkError::OutOfMemory);
    }

    // C:1037 `if(!vr->def_memtype->ev_resize)` splits the two legs on the
    // memtype's resize capability (`supports_resize`, the NULL-field reading;
    // the trait's `ev_resize` callback itself stays framework-folded per
    // [ARCH: A-12]). Resize leg: physblocks grow in place (C:1047-1057 —
    // anon pages demand-fill on fault). minix-rs realizes the same through
    // VirRegion::extend.
    let grow_len = VirBytes(offset.0 - limit.0);
    let supports_resize = avl_less(active.regions(), offset.0)
        .expect("brk: region present (checked above)")
        .def_memtype
        // CodeReview 续-279m P2-3：`def_memtype==None` 是 C 不存在的形（每个
        // region 都经 map_page_region 带 memtype 诞生）——若残留到此，resize 腿
        // 原地 extend 产出的新页仍会同型 NoMemType 死；路由到 create 腿就地
        // 新建带 MEM_TYPE_ANON 的可 fault 段才是防御性正解。
        .map(|m| m.supports_resize())
        .unwrap_or(false);

    if supports_resize {
        avl_less_mut(active.regions_mut(), offset.0)
            .expect("brk: region present (checked above)")
            .extend(grow_len)
            .map_err(|_| BrkError::OutOfMemory)?;
        // C: the resize leg never touches vm_region_top — the hint is written
        // only by region_find_slot_range (region.c:391) for fresh slots.
    } else {
        // C:1038-1045 no-ev_resize memtype: `map_page_region(vmp, limit, 0,
        // extralen, VR_WRITABLE|VR_ANON, 0, &mem_type_anon)` — maxv==0 means
        // "right here" (region_find_slot_range:324-331), and the fresh region
        // **spans the gap** from limit up to offset (extralen, C:1025).
        // Per-brk fragmentation is deliberately not fought (C has it too —
        // this leg is where C fragments).
        let new_region = VirRegion::with_memtype(
            limit,
            grow_len,
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        active.regions_mut().insert(new_region)
            .map_err(|_| BrkError::OutOfMemory)?;
        // C's hint leg runs for every fresh slot (region.c:391 inside
        // region_find_slot_range, which map_page_region always calls).
        active.set_region_top(offset);
    }

    // minix-rs accounting (no C counterpart: virtual-extent bookkeeping for
    // getrusage/query; C's vm_total_bytes is updated inside memtype/map legs).
    active.add_total(grow_len);

    // C:1057 `ev_resize(vmp, vr, offset - vr->vaddr)` — page material is
    // demand-filled on fault for anon regions; the new span is now faultable,
    // which is what the C return chain guarantees.
    Ok(offset)
}

/// C `region_search(&avm, key, AVL_LESS)` (region.c): the region with the
/// greatest vaddr **<= key**, containment not required. `RegionMap::find`
/// checks containment and `search(SearchType::Less)` is a strict `<` —
/// neither matches the C `<=` predecessor reading, so the literal iter form
/// lives here (single spot, pinned by the tests below).
fn avl_less<'a>(regions: &'a RegionMap, key: u64) -> Option<&'a VirRegion> {
    regions.iter().filter(|r| r.vaddr.0 <= key).max_by_key(|r| r.vaddr.0)
}

/// Mutating twin of [`avl_less`] (same resolution order, `&mut` lanes).
fn avl_less_mut<'a>(regions: &'a mut RegionMap, key: u64) -> Option<&'a mut VirRegion> {
    regions.iter_mut().filter(|r| r.vaddr.0 <= key).max_by_key(|r| r.vaddr.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use core::sync::atomic::{AtomicU32, Ordering};
    use minix_types::UserSlot;

    static NEXT_SLOT: AtomicU32 = AtomicU32::new(100);

    fn next_test_slot() -> UserSlot {
        let s = NEXT_SLOT.fetch_add(1, Ordering::Relaxed);
        UserSlot(s as usize)
    }

    fn brk(ep: Endpoint, addr: VirBytes) -> Result<VirBytes, BrkError> {
        handle_brk(VmProcTable::get_global(), &BrkRequest { endpoint: ep, new_brk_addr: addr })
            .map(|r| r.new_brk_addr)
    }

    /// Seed a process with a `data_len` byte data region at 0x4000_0000 —
    /// the shape a boot-image/exec'd data segment leaves behind (the old
    /// fixture pre-set `region_top` as the delta anchor; the absolute-break
    /// port takes the region table as its only input, just like C).
    fn init_data_proc(slot: UserSlot, data_len: u64) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        // Page tables stay in the software (SimPaging) model (V11/T21); the
        // brk port itself is table-free — extension is demand-filled on
        // fault, mirroring C's anon ev_resize.
        active.init_page_table().unwrap();
        active.init_regions();
        let data = VirRegion::with_memtype(
            VirBytes(0x4000_0000),
            VirBytes(data_len),
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        active.regions_mut().insert(data).unwrap();
        ep
    }

    #[test]
    fn test_brk_error_to_errno() {
        // Tests the full error path: BrkError → From<BrkError> for VmError → VmError::to_errno()
        // This is the actual dispatch path used in production.
        use minix_types::{VmError, EINVAL, ENOMEM};
        assert_eq!(VmError::from(BrkError::ProcessNotFound).to_errno(), EINVAL); // ProcessNotFound → InvalidProcess → EINVAL
        assert_eq!(VmError::from(BrkError::OutOfMemory).to_errno(), ENOMEM);    // OutOfMemory → OutOfMemory → ENOMEM
    }

    #[test]
    fn test_brk_extends_containing_region_c_like() {
        let slot = next_test_slot();
        let ep = init_data_proc(slot, 0x1000);

        assert_eq!(brk(ep, VirBytes(0x4000_2000)), Ok(VirBytes(0x4000_2000)));

        let table = VmProcTable::get_global();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        let vr = active.regions().find(VirBytes(0x4000_0000)).unwrap();
        assert_eq!(vr.end_addr(), VirBytes(0x4000_2000), "C extends the region in place");
        assert_eq!(active.regions().len(), 1, "no new region for the anon resize leg");
        // C: the resize leg never writes vm_region_top (the hint belongs to
        // region_find_slot_range only, region.c:391) — pin the parity.
        assert_eq!(active.region_top(), VirBytes(0), "no hint write on in-place resize");
    }

    #[test]
    fn test_brk_rounds_partial_page() {
        // C:1010 `offset = roundup(v, PAGE)` — a mid-page break maps the whole
        // page, and the request (not the rounded extent) is what callers cache.
        let slot = next_test_slot();
        let ep = init_data_proc(slot, 0x1000);

        assert_eq!(brk(ep, VirBytes(0x4000_1800)), Ok(VirBytes(0x4000_2000)));
        let table = VmProcTable::get_global();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        assert_eq!(active.regions().find(VirBytes(0x4000_0000)).unwrap().end_addr(),
            VirBytes(0x4000_2000));
    }

    #[test]
    fn test_brk_shrink_is_noop_ok() {
        // C break.c has no shrink leg: a break already covered by the region
        // returns OK and unmaps nothing (§续-279m: the old destructive
        // shrink_arm deleted the data region under a lower brk()).
        let slot = next_test_slot();
        let ep = init_data_proc(slot, 0x3000);

        assert_eq!(brk(ep, VirBytes(0x4000_1000)), Ok(VirBytes(0x4000_3000)));
        let table = VmProcTable::get_global();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        let vr = active.regions().find(VirBytes(0x4000_0000)).unwrap();
        assert_eq!(vr.end_addr(), VirBytes(0x4000_3000), "VM-side extent never shrinks");
    }

    #[test]
    fn test_brk_no_resize_memtype_lands_new_region() {
        // C:1037-1045 `if(!vr->def_memtype->ev_resize)` — memtypes without a
        // resize callback (mapped file, mem_file.c table has no .ev_resize)
        // get a fresh ANON region spanning [limit, offset), and the fresh
        // slot writes the placement hint (region.c:391).
        use crate::memtype::MEM_TYPE_MAPPED_FILE;
        let slot = next_test_slot();
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        active.regions_mut().insert(VirRegion::with_memtype(
            VirBytes(0x4000_0000),
            VirBytes(0x1000),
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_MAPPED_FILE,
        )).unwrap();
        drop(active);

        assert!(brk(ep, VirBytes(0x4000_2000)).is_ok());
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        assert_eq!(active.regions().len(), 2, "create leg lands beside the old region");
        assert!(active.regions().find(VirBytes(0x4000_1800)).is_some(),
            "new region spans the gap from the old end (C map_page_region(vmp, limit, 0, extralen))");
        assert_eq!(active.region_top(), VirBytes(0x4000_2000), "fresh slot writes the hint");
    }

    #[test]
    fn test_grow_heap_rejects_overlap_c18() {
        // C's "can't grow into next region" (region.c:1032-1035): the AVL_LESS
        // predecessor must itself need growth (limit < offset), and the
        // immediate successor starts inside the span with its end still below
        // offset — extending would swallow a region whole. (A successor whose
        // end covers offset hits the covered arm at :1016 instead — C calls
        // that OK; see test_brk_interleaved_region_covered_noop.)
        let slot = next_test_slot();
        let ep = init_data_proc(slot, 0x1000);
        {
            let table = VmProcTable::get_global();
            let mut active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
            let neighbor = VirRegion::with_memtype(
                VirBytes(0x4000_2000),
                VirBytes(0x800),
                VrFlags::WRITABLE | VrFlags::ANON,
                &MEM_TYPE_ANON,
            );
            active.regions_mut().insert(neighbor).unwrap();
        }

        let result = brk(ep, VirBytes(0x4000_3000));
        assert!(matches!(result, Err(BrkError::OutOfMemory)),
            "brk must reject growth that would swallow the next region");
        let table = VmProcTable::get_global();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        let data = active.regions().find(VirBytes(0x4000_0800)).unwrap();
        assert_eq!(data.end_addr(), VirBytes(0x4000_1000),
            "the rejected extension must leave the grown-into regions untouched");
    }

    #[test]
    fn test_brk_interleaved_region_covered_noop() {
        // C:1016 pinned: a brk whose AVL_LESS predecessor already reaches the
        // target returns OK untouched — even when the predecessor is NOT the
        // data region (a 1-page gap below an interleaved region is C's
        // accepted no-op; the old delta-vs-region_top model destroyed data
        // regions in shapes like this, §续-279m).
        let slot = next_test_slot();
        let ep = init_data_proc(slot, 0x1000);
        {
            let table = VmProcTable::get_global();
            let mut active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
            let neighbor = VirRegion::new(
                VirBytes(0x4000_2000),
                VirBytes(0x1000),
                VrFlags::WRITABLE | VrFlags::ANON,
            );
            active.regions_mut().insert(neighbor).unwrap();
        }

        assert_eq!(brk(ep, VirBytes(0x4000_2800)), Ok(VirBytes(0x4000_3000)));
        let table = VmProcTable::get_global();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        let data = active.regions().find(VirBytes(0x4000_0800)).unwrap();
        assert_eq!(data.end_addr(), VirBytes(0x4000_1000), "covered arm maps nothing");
        assert_eq!(active.regions().len(), 2, "covered arm creates nothing");
    }

    #[test]
    fn test_brk_far_neighbor_still_grows() {
        // The C-faithful complement: a neighbor at/above `offset` doesn't
        // block (assert(offset <= nextvr->vaddr), region.c:1029) — the data
        // region extends and the neighbor keeps its extent.
        let slot = next_test_slot();
        let ep = init_data_proc(slot, 0x1000);
        {
            let table = VmProcTable::get_global();
            let mut active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
            let far = VirRegion::new(
                VirBytes(0x4000_5000),
                VirBytes(0x1000),
                VrFlags::WRITABLE | VrFlags::ANON,
            );
            active.regions_mut().insert(far).unwrap();
        }

        assert_eq!(brk(ep, VirBytes(0x4000_3000)), Ok(VirBytes(0x4000_3000)));
        let table = VmProcTable::get_global();
        let active = table.get_active(table.vm_isokendpt(ep).unwrap()).unwrap();
        let data = active.regions().find(VirBytes(0x4000_0800)).unwrap();
        assert_eq!(data.end_addr(), VirBytes(0x4000_3000));
        let far = active.regions().find(VirBytes(0x4000_5800)).unwrap();
        assert_eq!((far.vaddr, far.length), (VirBytes(0x4000_5000), VirBytes(0x1000)));
    }

    #[test]
    fn test_brk_nothing_to_extend() {
        // C:1013-1016 — no region at or below the request: "VM: nothing to
        // extend" + ENOMEM (the boot-process 0-region shape degrades here,
        // which is exactly the safe no-op C performs for unknown endpoints).
        let slot = next_test_slot();
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        drop(active);

        assert!(matches!(brk(ep, VirBytes(0x4000_1000)), Err(BrkError::OutOfMemory)));
    }

    #[test]
    fn test_brk_process_not_found() {
        let result = handle_brk(VmProcTable::get_global(),
            &BrkRequest { endpoint: Endpoint::NONE, new_brk_addr: VirBytes(0x4000_1000) });
        assert!(matches!(result, Err(BrkError::ProcessNotFound)));
    }
}
