//! VM query service handlers.
//!
//! Handles VM_INFO, VM_GETPHYS, VM_GETREF, and VM_GETRUSAGE requests —
//! the read-only, side-effect-free queries into VM's internal state.
//!
//! Corresponds to Minix3's `do_info()` (utility.c:100),
//! `do_get_phys()` (mmap.c:438), `do_get_refcount()` (mmap.c:463),
//! `do_getrusage()` (utility.c:426), the region-side helpers
//! `map_get_phys()` / `map_get_ref()` / `get_usage_info()` /
//! `get_usage_info_kernel()` / `get_usage_info_vm()` / `get_region_info()`
//! (region.c), and `get_stats_info()` (cache.c).

use minix_types::{VirBytes, EINVAL, ESRCH, Endpoint, PhysBytes};
use crate::vmproc::{VmProcTable, EndpointError};
use crate::region::PageFrames;
use crate::alloc_page::VmPageAllocator;
use crate::region::page_state::PAGE_SIZE;
use crate::region::{VirRegion, VrFlags};

/// Max regions reported in one VMIW_REGION reply.
///
/// C: `MAX_VRI_COUNT` (minix/include/minix/vm.h:66) = 64. Both the
/// caller-side buffer (procfs pid.c:196, vfs/coredump.c:196) and VM's
/// static array (utility.c:104) use this bound; callers loop while a
/// reply returns exactly this many entries.
pub(crate) const MAX_VRI_COUNT: usize = 64;

// PROT_* constants — C: sys/sys/mman.h:63-64.
const PROT_READ: u16 = 0x01;
const PROT_WRITE: u16 = 0x02;

// ── Error type ───────────────────────────────────────────────────────
//
// QueryError maps to VmError via `From<QueryError> for VmError`,
// then to C errno via `VmError::to_errno()`. The per-error `to_errno()`
// method is intentionally omitted — the single source of truth is
// `VmError::to_errno()` in `minix_types::ipc::vm`.
//
// Exception: `to_errno_for_rusage()` is context-dependent —
// QueryError::ProcessNotFound maps to ESRCH in getrusage context
// (C: utility.c:426) but EINVAL in other query contexts.
// This cannot be expressed via `From<QueryError> for VmError` because
// the mapping depends on the call site, not just the error variant.
// The dispatcher uses `query_rusage_error_to_vm_error()` for getrusage.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryError {
    ProcessNotFound,
    NotMapped,
    NotSupported,
    // V10-P2-1 (DEFERRED): no constructor yet — kept for the errno-mapping
    // surface (V10-P2-3) and future query kinds.
    #[allow(dead_code)]
    InvalidQuery,
}

impl QueryError {
    /// Context-dependent errno mapping for getrusage.
    ///
    /// C `do_getrusage()` (utility.c:426) returns ESRCH when the target
    /// process is not found, while other query operations return EINVAL.
    /// This cannot be unified into `From<QueryError> for VmError` because
    /// the mapping depends on the call site.
    #[cfg_attr(not(test), allow(dead_code))] // V10-P2-1: dispatcher uses the From impl; this is the test-side mirror
    pub(crate) fn to_errno_for_rusage(self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::NotMapped => EINVAL,
            Self::NotSupported => EINVAL,
            Self::InvalidQuery => EINVAL,
        }
    }
}

// ── Endpoint-lookup error unification ──
//
// See `munmap.rs` for the full rationale. The same pattern is applied
// here: `vm_isokendpt()` returns `EndpointError` (InvalidSlot or
// DeadEndpoint), and most query handlers collapse both to
// `QueryError::ProcessNotFound`. The getrusage context (ESRCH vs EINVAL)
// is handled at the dispatcher's `query_rusage_error_to_vm_error` boundary,
// not at the endpoint-lookup boundary.
impl From<EndpointError> for QueryError {
    fn from(_: EndpointError) -> Self {
        QueryError::ProcessNotFound
    }
}

// ── Query types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InfoQuery {
    Stats,
    Usage { target: Endpoint },
    Region { target: Endpoint, count: usize, next: VirBytes },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatsInfo {
    pub page_size: u64,
    pub total_pages: u32,
    pub free_pages: u32,
    pub largest_contiguous: u32,
    /// C: `vsi_cached` — pages cached for file systems (`get_stats_info`,
    /// cache.c:328-331).
    pub cached_pages: u64,
}

/// Per-process memory usage, aligned with Minix3's `struct vm_usage_info`
/// (minix/include/minix/vm.h:48).
///
/// Field semantics match C's `get_usage_info()` (region.c:1395):
/// - `total`: sum of mapped (present) page sizes across all regions
/// - `common`: pages with refcount > 1 (shared between processes)
/// - `shared`: common pages in regions with `VR_SHARED` flag (non-COW)
/// - `virtual_total`: sum of all region lengths (total virtual address space)
/// - `mvirtual`: virtual minus unmapped stack pages
/// - `max_rss_kb` / `minor_faults` / `major_faults`: getrusage fields
///   included so the MIB service needs only one VM call per process
///   (region.c:1440-1443 comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsageInfo {
    pub total: VirBytes,
    pub common: VirBytes,
    pub shared: VirBytes,
    pub virtual_total: VirBytes,
    pub mvirtual: VirBytes,
    pub max_rss_kb: u64,
    pub minor_faults: u64,
    pub major_faults: u64,
}

/// One region entry in a VMIW_REGION reply.
///
/// Mirrors C's `struct vm_region_info` used portion (region.c:1469-1478):
/// `addr`/`length` cover the *used* range (first mapped page .. last mapped
/// page), not the full region reservation. `vri_flags` is not modeled —
/// C's static reply array is zero-initialized and `get_region_info` never
/// writes the field, so it is always 0 on the wire (dead field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegionInfo {
    pub addr: VirBytes,
    pub length: VirBytes,
    pub prot: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // V10-P2-1 (DEFERRED): InfoResult payloads converge in the codec work
pub(crate) enum InfoResult {
    Stats(StatsInfo),
    Usage(UsageInfo),
    Region {
        regions: [RegionInfo; MAX_VRI_COUNT],
        count: usize,
        /// Vaddr cursor: end address of the last visited region
        /// (C: `*nextp`, region.c:1464). Caller passes it back verbatim.
        next: VirBytes,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceUsage {
    pub max_rss_kb: u64,
    pub minor_faults: u64,
    pub major_faults: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GetrusageResult {
    Ok,
    Data(ResourceUsage),
}

/// Boot-time byte totals backing the kernel / VM-self usage queries.
///
/// C: `do_info` VMIW_USAGE with `ep < 0` → `get_usage_info_kernel()`
/// (region.c:1357-1364) = `kernel_allocated_bytes + _dynamic`;
/// `ep == VM_PROC_NR` → `get_usage_info_vm()` (region.c:1366-1373)
/// = `vm_allocated_bytes + get_vm_self_pages() * VM_PAGE_SIZE`.
/// The caller (VmServer) computes both totals at query time so the
/// handler stays pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsageSources {
    pub kernel_bytes: u64,
    pub vm_self_bytes: u64,
}

// ── Handler: GETPHYS ─────────────────────────────────────────────────

/// Handle VM_GETPHYS — query the region identifier at a virtual address.
///
/// Corresponds to Minix3's `do_get_phys()` (mmap.c:438) +
/// `map_get_phys()` (region.c:1323).
///
/// The address must exactly match a region's start address, and the
/// region's memtype must implement `regionid` (anon / shared in C).
///
/// Note: despite the name, C's `regionid` callbacks return the *region id*,
/// not a physical address — `anon_regionid` returns `region->id`
/// (mem_anon.c:132-134), `shared_regionid` returns the source region's id
/// (mem_shared.c:99-106). The consumer is the IPC shm server, which uses
/// the id as a shared-memory token (ipc/shm.c:118).
pub(crate) fn handle_get_phys(
    table: &VmProcTable,
    target: Endpoint,
    addr: VirBytes,
) -> Result<PhysBytes, QueryError> {
    let slot = table.vm_isokendpt(target)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    let vr = active.regions().find(addr)
        .ok_or(QueryError::NotMapped)?;

    // C: vr->vaddr != addr → EINVAL (must match region start exactly)
    if vr.vaddr != addr {
        return Err(QueryError::NotMapped);
    }

    // C: !vr->def_memtype->regionid → EINVAL (region.c:1331)
    let memtype = vr.def_memtype.ok_or(QueryError::NotSupported)?;
    if !memtype.supports_region_id() {
        return Err(QueryError::NotSupported);
    }
    Ok(PhysBytes(memtype.region_id(vr) as u64))
}

// ── Handler: GETREF ──────────────────────────────────────────────────

/// Handle VM_GETREF — query a region's reference count.
///
/// Corresponds to Minix3's `do_get_refcount()` (mmap.c:463) +
/// `map_get_ref()` (region.c:1343).
///
/// Same constraints as GETPHYS: address must match region start, and
/// memtype must implement `refcount` (anon / shared in C). C's
/// `anon_refcount`/`shared_refcount` return `1 + vr->remaps`
/// (mem_anon.c:142-144, mem_shared.c:207-209) — the region's remap
/// count, not the per-page physical refcount.
pub(crate) fn handle_get_refcount(
    table: &VmProcTable,
    target: Endpoint,
    addr: VirBytes,
) -> Result<u8, QueryError> {
    let slot = table.vm_isokendpt(target)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    let vr = active.regions().find(addr)
        .ok_or(QueryError::NotMapped)?;

    // C: vr->vaddr != addr → EINVAL
    if vr.vaddr != addr {
        return Err(QueryError::NotMapped);
    }

    // C: !vr->def_memtype->refcount → EINVAL (region.c:1349)
    let memtype = vr.def_memtype.ok_or(QueryError::NotSupported)?;
    if !memtype.supports_ref_count() {
        return Err(QueryError::NotSupported);
    }
    Ok(memtype.ref_count(vr) as u8)
}

// ── Handler: INFO ────────────────────────────────────────────────────

/// Handle VM_INFO — query memory statistics, usage, or region info.
///
/// Corresponds to Minix3's `do_info()` (utility.c:100).
///
/// Three query modes:
/// - **Stats**: Global memory statistics (page size, total/free pages,
///   largest free run, cached pages).
/// - **Usage**: Per-process memory usage — or, for negative endpoints
///   (kernel tasks), kernel usage; for `Endpoint::VM`, VM's own usage.
/// - **Region**: Per-process region list (paginated via a vaddr `next`
///   cursor).
///
/// Note: Minix3's `do_info` calls `handle_memory_once()` before
/// `sys_datacopy` to prevent deadlocks from page faults during the
/// copy. Rust returns query results directly through the IPC message
/// (no cross-address-space copy), so the deadlock window is
/// structurally eliminated ([ARCH: 26-D1]).
pub(crate) fn handle_info(
    table: &VmProcTable,
    page_alloc: &VmPageAllocator,
    frames: &PageFrames,
    sources: UsageSources,
    cached_pages: u64,
    query: InfoQuery,
) -> Result<InfoResult, QueryError> {
    match query {
        InfoQuery::Stats => {
            let stats = page_alloc.phys_alloc().memstats();
            Ok(InfoResult::Stats(StatsInfo {
                page_size: PAGE_SIZE,
                total_pages: page_alloc.total_pages() as u32,
                free_pages: stats.free_pages as u32,
                largest_contiguous: stats.largest_free as u32,
                // C: get_stats_info() — vsi_cached = cached_pages (cache.c:328-331)
                cached_pages,
            }))
        }
        InfoQuery::Usage { target } => {
            // C: do_info — if (ep < 0) get_usage_info_kernel(&vui)
            // (utility.c:131-132). Kernel tasks have negative endpoints.
            if target.get() < 0 {
                return Ok(InfoResult::Usage(UsageInfo {
                    total: VirBytes(sources.kernel_bytes),
                    common: VirBytes(0),
                    shared: VirBytes(0),
                    virtual_total: VirBytes(sources.kernel_bytes),
                    mvirtual: VirBytes(sources.kernel_bytes),
                    max_rss_kb: 0,
                    minor_faults: 0,
                    major_faults: 0,
                }));
            }

            // C: get_usage_info — if (vmp->vm_endpoint == VM_PROC_NR)
            // get_usage_info_vm(vui) (region.c:1398-1399).
            if target == Endpoint::VM {
                return Ok(InfoResult::Usage(UsageInfo {
                    total: VirBytes(sources.vm_self_bytes),
                    common: VirBytes(0),
                    shared: VirBytes(0),
                    virtual_total: VirBytes(sources.vm_self_bytes),
                    mvirtual: VirBytes(sources.vm_self_bytes),
                    max_rss_kb: 0,
                    minor_faults: 0,
                    major_faults: 0,
                }));
            }

            let slot = table.vm_isokendpt(target)?;
            let active = table.get_active(slot)
                .ok_or(QueryError::ProcessNotFound)?;

            // C: get_usage_info() (region.c:1395) iterates all regions,
            // then per-page within each region, accumulating statistics.
            let mut total: u64 = 0;
            let mut common: u64 = 0;
            let mut shared: u64 = 0;
            let mut virtual_total: u64 = 0;
            let mut mvirtual: u64 = 0;

            for vr in active.regions().iter() {
                virtual_total = virtual_total.saturating_add(vr.length.0);
                mvirtual = mvirtual.saturating_add(vr.length.0);

                let is_shared_region = vr.flags.contains(VrFlags::SHARED);

                for slot in &vr.physblocks {
                    let Some(pfn) = slot.pfn() else {
                        // C: unmapped stack pages are discounted from mvirtual.
                        // is_stack_region() heuristic (region.c:1384-1393):
                        // vaddr == VM_STACKTOP - DEFAULT_STACK_LIMIT &&
                        // length == DEFAULT_STACK_LIMIT. The Rust model
                        // approximates with `end_addr() == region_top()`
                        // (doc 26 §3.10 — both sides admit guesswork).
                        if vr.end_addr() == active.region_top() {
                            mvirtual = mvirtual.saturating_sub(PAGE_SIZE);
                        }
                        continue;
                    };

                    // Page is mapped → count towards total.
                    total = total.saturating_add(PAGE_SIZE);

                    // C: ph->ph->refcount > 1 → common.
                    // We look up the refcount from PageFrames.
                    if let Some(state) = frames.get(pfn)
                        && state.refcount() > 1 {
                            common = common.saturating_add(PAGE_SIZE);
                            // C: common + VR_SHARED → shared (non-COW).
                            if is_shared_region {
                                shared = shared.saturating_add(PAGE_SIZE);
                            }
                        }
                }
            }

            Ok(InfoResult::Usage(UsageInfo {
                total: VirBytes(total),
                common: VirBytes(common),
                shared: VirBytes(shared),
                virtual_total: VirBytes(virtual_total),
                mvirtual: VirBytes(mvirtual),
                // C: get_usage_info tail — vui_maxrss/minflt/majflt
                // (region.c:1420-1426) so MIB needs one call per process.
                max_rss_kb: active.total_max().0 / 1024,
                minor_faults: active.minor_fault(),
                major_faults: active.major_fault(),
            }))
        }
        InfoQuery::Region { target, count, next } => {
            // C: get_region_info — if (!max) return 0 (region.c:1456)
            if count == 0 {
                return Ok(InfoResult::Region {
                    regions: [RegionInfo { addr: VirBytes(0), length: VirBytes(0), prot: 0 };
                        MAX_VRI_COUNT],
                    count: 0,
                    next,
                });
            }

            let slot = table.vm_isokendpt(target)?;
            let active = table.get_active(slot)
                .ok_or(QueryError::ProcessNotFound)?;

            // C: count = MIN(requested, MAX_VRI_COUNT) (utility.c:149)
            let max = count.min(MAX_VRI_COUNT);

            // C: get_region_info (region.c:1428-1478):
            //   - cursor `next` is a vaddr; iteration resumes at the first
            //     region with vaddr >= next (AVL_GREATER_EQUAL, :1440)
            //   - for each region visited: next = vr->vaddr + vr->length
            //     (regardless of whether it is reported, :1464)
            //   - regions with no mapped pages are skipped (:1469-1474)
            //   - reported entries cover only the used portion (:1469-1478)
            let mut result = [RegionInfo {
                addr: VirBytes(0),
                length: VirBytes(0),
                prot: 0,
            }; MAX_VRI_COUNT];
            let mut idx = 0usize;
            let mut cursor = next;
            for vr in active.regions().iter() {
                if vr.vaddr < next {
                    continue;
                }
                if idx >= max {
                    break;
                }
                cursor = vr.end_addr();

                let (Some(first), Some(last)) = used_page_range(vr) else {
                    // C: "skipping empty region" — no mapped pages
                    // (region.c:1469-1474). Rust skips silently: query
                    // paths do not print (debug output belongs to sanity).
                    continue;
                };
                let used_len = last.offset().0 + PAGE_SIZE - first.offset().0;
                result[idx] = RegionInfo {
                    addr: VirBytes(vr.vaddr.0 + first.offset().0),
                    length: VirBytes(used_len),
                    // C: vri_prot = PROT_READ | (VR_WRITABLE ? PROT_WRITE : 0)
                    // (region.c:1476-1478). PROT_EXEC is never set.
                    prot: if vr.flags.contains(VrFlags::WRITABLE) {
                        PROT_READ | PROT_WRITE
                    } else {
                        PROT_READ
                    },
                };
                idx += 1;
            }

            Ok(InfoResult::Region {
                regions: result,
                count: idx,
                next: cursor,
            })
        }
    }
}

/// First and last mapped page slots of a region, if any.
///
/// C: `get_region_info` scans `physblock_get(vr, voffset)` over the whole
/// region and keeps `ph1` (first) / `ph2` (last) (region.c:1465-1468).
fn used_page_range(vr: &VirRegion) -> (Option<&crate::region::PageSlot>, Option<&crate::region::PageSlot>) {
    let mut first = None;
    let mut last = None;
    for slot in &vr.physblocks {
        if slot.is_mapped() {
            if first.is_none() {
                first = Some(slot);
            }
            last = Some(slot);
        }
    }
    (first, last)
}

// ── Handler: GETRUSAGE ───────────────────────────────────────────────

/// Handle VM_GETRUSAGE — query process resource usage.
///
/// Corresponds to Minix3's `do_getrusage()` (utility.c:426).
///
/// Only PM receives real data; non-PM callers get `Ok` (backward compat).
/// Returns `max_rss_kb`, `minor_faults`, and `major_faults` fields.
/// The `children` path is not implemented (same as C source).
pub(crate) fn handle_getrusage(
    table: &VmProcTable,
    caller: Endpoint,
    target: Endpoint,
    children: bool,
) -> Result<GetrusageResult, QueryError> {
    // C: if (m->m_source != PM_PROC_NR) return OK (utility.c:432-435);
    // obsolete userland-direct construction, backward compatibility.
    if !is_pm(caller) {
        return Ok(GetrusageResult::Ok);
    }

    let slot = table.vm_isokendpt(target)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    if !children {
        Ok(GetrusageResult::Data(ResourceUsage {
            max_rss_kb: active.total_max().0 / 1024,
            minor_faults: active.minor_fault(),
            major_faults: active.major_fault(),
        }))
    } else {
        // Minix3 C also does not implement the children path (utility.c:455-461):
        // "XXX TODO: return the fields for terminated, waited-for children
        //  of the given process. We currently do not have this information!"
        // C assumes PM clears the rusage struct before calling, so returning
        // zeros is semantically equivalent to the C behavior.
        Ok(GetrusageResult::Data(ResourceUsage {
            max_rss_kb: 0,
            minor_faults: 0,
            major_faults: 0,
        }))
    }
}

fn is_pm(ep: Endpoint) -> bool {
    ep == Endpoint::PM
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memtype::MemType;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::page_state::PfnAllocator;
    use minix_types::{UserSlot, PhysBytes, VmError, EINVAL, ESRCH};

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * PAGE_SIZE as u64))
    }

    fn make_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    /// Register a scratch process at `slot` (typestate: empty → active).
    /// The active handle is dropped; regions persist in the table slot.
    fn register_scratch(table: &VmProcTable, slot: usize) -> Endpoint {
        // SAFETY: test-only. Single-threaded, no concurrent access.
        unsafe { table.reset_slot(UserSlot::new(slot)); }
        let empty = table.get_empty(UserSlot::new(slot)).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot as i32);
        let mut active = empty.activate(ep);
        active.init_regions();
        ep
    }

    fn unregister(table: &VmProcTable, slot: usize) {
        // SAFETY: test-only. Single-threaded, no concurrent access.
        unsafe { table.reset_slot(UserSlot::new(slot)); }
    }

    fn empty_sources() -> UsageSources {
        UsageSources { kernel_bytes: 0, vm_self_bytes: 0 }
    }

    /// Minimal PFN allocator for tests (C: pfns handed out sequentially).
    struct TestAlloc {
        next: u32,
    }

    impl crate::region::PfnAllocator for TestAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, crate::region::PfnAllocError> {
            let pfn = self.next;
            self.next += 1;
            Ok(pfn)
        }

        fn free_pfn(&mut self, _pfn: u32) {}
    }

    #[test]
    fn test_query_error_errno() {
        // Full error path: QueryError → From<QueryError> for VmError → VmError::to_errno().
        // C: do_info/do_get_phys/do_get_refcount return EINVAL for every
        // failure (utility.c:110, mmap.c:444-452); getrusage returns ESRCH
        // (utility.c:442).
        assert_eq!(VmError::from(QueryError::ProcessNotFound).to_errno(), EINVAL);  // → InvalidProcess → EINVAL
        assert_eq!(VmError::from(QueryError::NotMapped).to_errno(), EINVAL);        // → InvalidParam → EINVAL
        assert_eq!(VmError::from(QueryError::NotSupported).to_errno(), EINVAL);     // → InvalidParam → EINVAL
        assert_eq!(VmError::from(QueryError::InvalidQuery).to_errno(), EINVAL);     // → InvalidParam → EINVAL
        // Context-dependent mapping for getrusage (C: utility.c:426)
        assert_eq!(QueryError::ProcessNotFound.to_errno_for_rusage(), ESRCH);
    }

    #[test]
    fn test_get_phys_invalid_endpoint() {
        let table = VmProcTable::get_global();
        assert_eq!(
            handle_get_phys(table, Endpoint(9999), VirBytes(0x1000)),
            Err(QueryError::ProcessNotFound)
        );
    }

    #[test]
    fn test_get_refcount_invalid_endpoint() {
        let table = VmProcTable::get_global();
        assert_eq!(
            handle_get_refcount(table, Endpoint(9999), VirBytes(0x1000)),
            Err(QueryError::ProcessNotFound)
        );
    }

    #[test]
    fn test_get_phys_anon_region_returns_region_id() {
        // C: anon_regionid returns region->id (mem_anon.c:132-134) — not a
        // physical address. The IPC shm server uses it as a shared-memory
        // token (ipc/shm.c:118).
        let table = VmProcTable::get_global();
        let slot = 30usize;
        let ep = register_scratch(&table, slot);
        let vaddr = VirBytes(0x2000_0000);
        let mut region = VirRegion::new(vaddr, VirBytes(PAGE_SIZE), VrFlags::ANON);
        region.id = 42;
        region.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);
        table.get_active(UserSlot::new(slot)).unwrap()
            .regions_mut().insert(region).unwrap();

        assert_eq!(handle_get_phys(&table, ep, vaddr).unwrap().0, 42);

        // C: vr->vaddr != addr → EINVAL (region.c:1328)
        assert_eq!(
            handle_get_phys(&table, ep, VirBytes(vaddr.0 + PAGE_SIZE)),
            Err(QueryError::NotMapped)
        );

        unregister(&table, slot);
    }

    #[test]
    fn test_get_refcount_anon_returns_1_plus_remaps() {
        // C: anon_refcount = 1 + vr->remaps (mem_anon.c:142-144).
        let table = VmProcTable::get_global();
        let slot = 31usize;
        let ep = register_scratch(&table, slot);
        let vaddr = VirBytes(0x2000_0000);
        let mut region = VirRegion::new(vaddr, VirBytes(PAGE_SIZE), VrFlags::ANON);
        region.remaps = 3;
        region.def_memtype = Some(&crate::memtype::MEM_TYPE_ANON);
        table.get_active(UserSlot::new(slot)).unwrap()
            .regions_mut().insert(region).unwrap();

        assert_eq!(handle_get_refcount(&table, ep, vaddr).unwrap(), 4);

        unregister(&table, slot);
    }

    #[test]
    fn test_get_phys_unsupported_memtype() {
        // C: no regionid/refcount callback → EINVAL (region.c:1331/:1349).
        // DirectPhysical has no callbacks in C either.
        let table = VmProcTable::get_global();
        let slot = 32usize;
        let ep = register_scratch(&table, slot);
        let vaddr = VirBytes(0x2000_0000);
        let region = VirRegion::with_memtype(
            vaddr,
            VirBytes(PAGE_SIZE),
            VrFlags::DIRECT,
            &crate::memtype::MEM_TYPE_DIRECT,
        );
        table.get_active(UserSlot::new(slot)).unwrap()
            .regions_mut().insert(region).unwrap();

        assert_eq!(
            handle_get_phys(&table, ep, vaddr),
            Err(QueryError::NotSupported)
        );
        assert_eq!(
            handle_get_refcount(&table, ep, vaddr),
            Err(QueryError::NotSupported)
        );

        unregister(&table, slot);
    }

    #[test]
    fn test_getrusage_non_pm() {
        let table = VmProcTable::get_global();
        assert_eq!(
            handle_getrusage(table, Endpoint(100), Endpoint(50), false),
            Ok(GetrusageResult::Ok)
        );
    }

    #[test]
    fn test_getrusage_pm_invalid_endpoint() {
        let table = VmProcTable::get_global();
        assert_eq!(
            handle_getrusage(table, Endpoint::PM, Endpoint(9999), false),
            Err(QueryError::ProcessNotFound)
        );
    }

    #[test]
    fn test_stats_info_defaults() {
        let info = StatsInfo {
            page_size: PAGE_SIZE as u64,
            total_pages: 0,
            free_pages: 0,
            largest_contiguous: 0,
            cached_pages: 0,
        };
        assert_eq!(info.page_size, PAGE_SIZE as u64);
        assert_eq!(info.cached_pages, 0);
    }

    #[test]
    fn test_resource_usage_fields() {
        let usage = ResourceUsage {
            max_rss_kb: 1024,
            minor_faults: 100,
            major_faults: 5,
        };
        assert_eq!(usage.max_rss_kb, 1024);
        assert_eq!(usage.minor_faults, 100);
        assert_eq!(usage.major_faults, 5);
    }

    #[test]
    fn test_is_pm() {
        assert!(is_pm(Endpoint::PM));
        assert!(!is_pm(Endpoint(100)));
    }

    #[test]
    fn test_usage_info_fields_align_with_c() {
        // All 8 fields of C's struct vm_usage_info (minix/include/minix/vm.h:48):
        //   vui_total, vui_common, vui_shared, vui_virtual, vui_mvirtual,
        //   vui_maxrss, vui_minflt, vui_majflt
        let info = UsageInfo {
            total: VirBytes(4096),
            common: VirBytes(2048),
            shared: VirBytes(1024),
            virtual_total: VirBytes(8192),
            mvirtual: VirBytes(6144),
            max_rss_kb: 16,
            minor_faults: 7,
            major_faults: 1,
        };
        assert_eq!(info.total.0, 4096);
        assert_eq!(info.common.0, 2048);
        assert_eq!(info.shared.0, 1024);
        assert_eq!(info.virtual_total.0, 8192);
        assert_eq!(info.mvirtual.0, 6144);
        assert_eq!(info.max_rss_kb, 16);
        assert_eq!(info.minor_faults, 7);
        assert_eq!(info.major_faults, 1);
    }

    #[test]
    fn test_usage_info_shared_semantics() {
        // C semantics (region.c:1417-1420): shared = pages where refcount
        // > 1 AND region has VR_SHARED flag (subset of common ⊆ total).
        let info = UsageInfo {
            total: VirBytes(3 * PAGE_SIZE),
            common: VirBytes(2 * PAGE_SIZE),
            shared: VirBytes(1 * PAGE_SIZE),
            virtual_total: VirBytes(4 * PAGE_SIZE),
            mvirtual: VirBytes(4 * PAGE_SIZE),
            max_rss_kb: 0,
            minor_faults: 0,
            major_faults: 0,
        };
        assert!(info.shared.0 <= info.common.0);
        assert!(info.common.0 <= info.total.0);
    }

    #[test]
    fn test_info_result_usage_construction() {
        let usage = UsageInfo {
            total: VirBytes(0),
            common: VirBytes(0),
            shared: VirBytes(0),
            virtual_total: VirBytes(0),
            mvirtual: VirBytes(0),
            max_rss_kb: 0,
            minor_faults: 0,
            major_faults: 0,
        };
        match InfoResult::Usage(usage) {
            InfoResult::Usage(u) => {
                assert_eq!(u.total.0, 0);
                assert_eq!(u.common.0, 0);
                assert_eq!(u.shared.0, 0);
                assert_eq!(u.virtual_total.0, 0);
                assert_eq!(u.mvirtual.0, 0);
            }
            _ => panic!("expected InfoResult::Usage"),
        }
    }

    #[test]
    fn test_handle_info_stats_cached_pages() {
        // C: get_stats_info fills vsi_cached = cached_pages (cache.c:328-331).
        let table = VmProcTable::get_global();
        let result = handle_info(
            table,
            &make_alloc(),
            &make_frames(),
            empty_sources(),
            123,
            InfoQuery::Stats,
        )
        .unwrap();
        match result {
            InfoResult::Stats(s) => {
                assert_eq!(s.page_size, PAGE_SIZE as u64);
                assert_eq!(s.cached_pages, 123);
            }
            _ => panic!("expected Stats"),
        }
    }

    #[test]
    fn test_handle_info_usage_kernel_target() {
        // C: do_info — ep < 0 → get_usage_info_kernel (utility.c:131-132):
        // total = kernel_allocated_bytes + _dynamic; virtual = mvirtual = total.
        let table = VmProcTable::get_global();
        let result = handle_info(
            table,
            &make_alloc(),
            &make_frames(),
            UsageSources { kernel_bytes: 5000, vm_self_bytes: 0 },
            0,
            InfoQuery::Usage { target: Endpoint::KERNEL },
        )
        .unwrap();
        match result {
            InfoResult::Usage(u) => {
                assert_eq!(u.total.0, 5000);
                assert_eq!(u.virtual_total.0, 5000);
                assert_eq!(u.mvirtual.0, 5000);
            }
            _ => panic!("expected Usage"),
        }
    }

    #[test]
    fn test_handle_info_usage_vm_self_target() {
        // C: get_usage_info — vm_endpoint == VM_PROC_NR → get_usage_info_vm
        // (region.c:1398-1399): total = vm_allocated_bytes + self pages.
        let table = VmProcTable::get_global();
        let result = handle_info(
            table,
            &make_alloc(),
            &make_frames(),
            UsageSources { kernel_bytes: 0, vm_self_bytes: 7000 },
            0,
            InfoQuery::Usage { target: Endpoint::VM },
        )
        .unwrap();
        match result {
            InfoResult::Usage(u) => {
                assert_eq!(u.total.0, 7000);
                assert_eq!(u.virtual_total.0, 7000);
                assert_eq!(u.mvirtual.0, 7000);
            }
            _ => panic!("expected Usage"),
        }
    }

    #[test]
    fn test_handle_info_usage_invalid_endpoint() {
        let table = VmProcTable::get_global();
        assert_eq!(
            handle_info(
                table,
                &make_alloc(),
                &make_frames(),
                empty_sources(),
                0,
                InfoQuery::Usage { target: Endpoint(9999) },
            ),
            Err(QueryError::ProcessNotFound)
        );
    }

    #[test]
    fn test_handle_info_usage_process_accumulation() {
        // C: get_usage_info (region.c:1395-1426) — one region, one mapped
        // page with refcount 2 in a VR_SHARED region → total=common=shared=1
        // page; maxrss = total_max/1024; faults copied from the proc.
        let table = VmProcTable::get_global();
        let slot = 33usize;
        let ep = register_scratch(&table, slot);
        let mut frames = make_frames();
        let mut alloc = TestAlloc { next: 0 };
        let pfn = alloc.alloc_pfn().unwrap();
        frames.get_mut(pfn).unwrap().refcount = 2;

        let mut region = VirRegion::with_memtype(
            VirBytes(0x1000_0000),
            VirBytes(PAGE_SIZE),
            VrFlags::SHARED | VrFlags::WRITABLE,
            &crate::memtype::MEM_TYPE_ANON,
        );
        region.map_page(&mut frames, VirBytes(0), pfn, &crate::memtype::MEM_TYPE_ANON);
        table.get_active(UserSlot::new(slot)).unwrap()
            .regions_mut().insert(region).unwrap();
        table.get_active(UserSlot::new(slot)).unwrap()
            .set_total_max(VirBytes(2 * 1024 * 1024));

        let result = handle_info(
            &table,
            &make_alloc(),
            &frames,
            empty_sources(),
            0,
            InfoQuery::Usage { target: ep },
        )
        .unwrap();
        match result {
            InfoResult::Usage(u) => {
                assert_eq!(u.total.0, PAGE_SIZE);
                assert_eq!(u.common.0, PAGE_SIZE);
                assert_eq!(u.shared.0, PAGE_SIZE);
                assert_eq!(u.virtual_total.0, PAGE_SIZE);
                assert_eq!(u.mvirtual.0, PAGE_SIZE);
                // C: vui_maxrss = vm_total_max / 1024 (region.c:1444)
                assert_eq!(u.max_rss_kb, 2048);
            }
            _ => panic!("expected Usage"),
        }

        unregister(&table, slot);
    }

    #[test]
    fn test_handle_info_region_pagination() {
        // C: get_region_info — vaddr `next` cursor; caller loops while
        // count == MAX_VRI_COUNT (procfs pid.c:229).
        let table = VmProcTable::get_global();
        let slot = 34usize;
        let ep = register_scratch(&table, slot);
        let mut frames = make_frames();
        let mut alloc = TestAlloc { next: 0 };

        let pfn1 = alloc.alloc_pfn().unwrap();
        let mut r1 = VirRegion::with_memtype(
            VirBytes(0x1000_0000),
            VirBytes(2 * PAGE_SIZE),
            VrFlags::WRITABLE | VrFlags::ANON,
            &crate::memtype::MEM_TYPE_ANON,
        );
        r1.map_page(&mut frames, VirBytes(0), pfn1, &crate::memtype::MEM_TYPE_ANON);

        // Empty region → skipped (C: region.c:1469-1474).
        let r2 = VirRegion::new(VirBytes(0x2000_0000), VirBytes(PAGE_SIZE), VrFlags::ANON);

        let pfn2 = alloc.alloc_pfn().unwrap();
        let mut r3 = VirRegion::with_memtype(
            VirBytes(0x3000_0000),
            VirBytes(2 * PAGE_SIZE),
            VrFlags::ANON,
            &crate::memtype::MEM_TYPE_ANON,
        );
        r3.map_page(&mut frames, VirBytes(PAGE_SIZE), pfn2, &crate::memtype::MEM_TYPE_ANON);

        {
            let mut regions = table.get_active(UserSlot::new(slot)).unwrap();
            let map = regions.regions_mut();
            map.insert(r1).unwrap();
            map.insert(r2).unwrap();
            map.insert(r3).unwrap();
        }

        // First page: region 1 (used = 1 page, writable) and region 3
        // (used = 1 page starting at +PAGE_SIZE); region 2 skipped.
        let result = handle_info(
            &table,
            &make_alloc(),
            &frames,
            empty_sources(),
            0,
            InfoQuery::Region {
                target: ep,
                count: MAX_VRI_COUNT,
                next: VirBytes(0),
            },
        )
        .unwrap();
        match result {
            InfoResult::Region { regions, count, next } => {
                assert_eq!(count, 2);
                assert_eq!(regions[0].addr.0, 0x1000_0000);
                assert_eq!(regions[0].length.0, PAGE_SIZE);
                assert_eq!(regions[0].prot, PROT_READ | PROT_WRITE);
                assert_eq!(regions[1].addr.0, 0x3000_0000 + PAGE_SIZE);
                assert_eq!(regions[1].length.0, PAGE_SIZE);
                assert_eq!(regions[1].prot, PROT_READ);
                // Cursor = end of last visited region.
                assert_eq!(next.0, 0x3000_0000 + 2 * PAGE_SIZE);
            }
            _ => panic!("expected Region"),
        }

        // Second page: cursor resumes at last end → no more regions.
        let result2 = handle_info(
            &table,
            &make_alloc(),
            &frames,
            empty_sources(),
            0,
            InfoQuery::Region {
                target: ep,
                count: MAX_VRI_COUNT,
                next: VirBytes(0x3000_0000 + 2 * PAGE_SIZE),
            },
        )
        .unwrap();
        match result2 {
            InfoResult::Region { count, next, .. } => {
                assert_eq!(count, 0);
                assert_eq!(next.0, 0x3000_0000 + 2 * PAGE_SIZE);
            }
            _ => panic!("expected Region"),
        }

        unregister(&table, slot);
    }

    #[test]
    fn test_handle_info_region_count_zero() {
        // C: get_region_info — if (!max) return 0 (region.c:1456).
        let table = VmProcTable::get_global();
        let result = handle_info(
            table,
            &make_alloc(),
            &make_frames(),
            empty_sources(),
            0,
            InfoQuery::Region {
                target: Endpoint(1),
                count: 0,
                next: VirBytes(0),
            },
        )
        .unwrap();
        match result {
            InfoResult::Region { count, .. } => assert_eq!(count, 0),
            _ => panic!("expected Region"),
        }
    }

    #[test]
    fn test_memtype_capability_gates() {
        // C: only anon/shared register regionid/refcount callbacks
        // (mem_anon.c:42/:44, mem_shared.c:35/:36) — every other memtype
        // makes map_get_phys/map_get_ref fail with EINVAL.
        assert!(crate::memtype::MEM_TYPE_ANON.supports_region_id());
        assert!(crate::memtype::MEM_TYPE_ANON.supports_ref_count());
        assert!(crate::memtype::MEM_TYPE_SHARED.supports_region_id());
        assert!(crate::memtype::MEM_TYPE_SHARED.supports_ref_count());
        assert!(!crate::memtype::MEM_TYPE_DIRECT.supports_region_id());
        assert!(!crate::memtype::MEM_TYPE_DIRECT.supports_ref_count());
        assert!(!crate::memtype::MEM_TYPE_CONTIG_ANON.supports_region_id());
        assert!(!crate::memtype::MEM_TYPE_CACHE.supports_region_id());
        assert!(!crate::memtype::MEM_TYPE_MAPPED_FILE.supports_region_id());
    }

    #[test]
    fn test_shared_region_id_and_refcount() {
        // C: shared_regionid returns the source region id stored in
        // param.shared.id (mem_shared.c:99-106, :184); shared_refcount
        // = 1 + vr->remaps (mem_shared.c:207-209).
        let mut region = VirRegion::new(
            VirBytes(0x4000_0000),
            VirBytes(PAGE_SIZE),
            VrFlags::SHARED,
        );
        region.def_memtype = Some(&crate::memtype::MEM_TYPE_SHARED);
        region.param = crate::region::VrParam::Shared {
            ep: 1,
            vaddr: VirBytes(0x1000_0000),
            id: 77,
        };
        region.remaps = 2;
        assert_eq!(crate::memtype::MEM_TYPE_SHARED.region_id(&region), 77);
        assert_eq!(crate::memtype::MEM_TYPE_SHARED.ref_count(&region), 3);
    }

    #[test]
    fn test_used_page_range_empty() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(2 * PAGE_SIZE), VrFlags::ANON);
        let (first, last) = used_page_range(&region);
        assert!(first.is_none() && last.is_none());
    }
}
