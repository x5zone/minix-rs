//! VM query service handlers.
//!
//! Handles VM_INFO, VM_GETPHYS, VM_GETREF, and VM_GETRUSAGE requests.
//! These are read-only, side-effect-free queries into VM's internal state.
//!
//! Corresponds to Minix3's `do_info()` (utility.c:100),
//! `do_get_phys()` (mmap.c:438), `do_get_refcount()` (mmap.c:463),
//! and `do_getrusage()` (utility.c:426).

use minix_types::{VirBytes, EINVAL, ESRCH, Endpoint, PhysBytes};
use crate::vmproc::{VmProcTable, EndpointError};
use crate::region::PageFrames;
use crate::alloc_page::VmPageAllocator;
use crate::region::page_state::PAGE_SIZE;

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
    InvalidQuery,
}

impl QueryError {
    /// Context-dependent errno mapping for getrusage.
    ///
    /// C `do_getrusage()` (utility.c:426) returns ESRCH when the target
    /// process is not found, while other query operations return EINVAL.
    /// This cannot be unified into `From<QueryError> for VmError` because
    /// the mapping depends on the call site.
    pub(crate) fn to_errno_for_rusage(&self) -> i32 {
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
    Region { target: Endpoint, count: usize, next: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatsInfo {
    pub page_size: u64,
    pub total_pages: u32,
    pub free_pages: u32,
    pub largest_contiguous: u32,
}

/// Per-process memory usage, aligned with Minix3's `struct vm_usage_info`
/// (minix/include/minix/vm.h:48).
///
/// Field semantics match C's `get_usage_info()` (region.c:1395):
/// - `total`: sum of mapped (present) page sizes across all regions
/// - `common`: pages with refcount > 1 (shared between processes)
/// - `shared`: common pages in regions with `VR_SHARED` flag (non-COW)
/// - `virtual`: sum of all region lengths (total virtual address space)
/// - `mvirtual`: virtual minus unmapped stack pages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsageInfo {
    pub total: VirBytes,
    pub common: VirBytes,
    pub shared: VirBytes,
    pub virtual_total: VirBytes,
    pub mvirtual: VirBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegionInfo {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InfoResult {
    Stats(StatsInfo),
    Usage(UsageInfo),
    Region {
        // Mirrors Minix3 MAX_VRI_COUNT; fixed-size array avoids allocation.
        regions: [RegionInfo; 8], count: usize, next: usize
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

// ── Handler: GETPHYS ─────────────────────────────────────────────────

/// Handle VM_GETPHYS — query physical address for a virtual address.
///
/// Corresponds to Minix3's `do_get_phys()` (mmap.c:438) +
/// `map_get_phys()` (region.c:1323).
///
/// The address must exactly match a region's start address, and the
/// region's memtype must support `regionid` (physical base address query).
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

    // C: vr->def_memtype->regionid(vr)
    // In PFN model, physical base = first page's physical address.
    // Only VR_DIRECT regions have a meaningful physical base.
    match &vr.param {
        crate::region::VrParam::Direct { phys } => {
            Ok(*phys)
        }
        _ => Err(QueryError::NotSupported),
    }
}

// ── Handler: GETREF ──────────────────────────────────────────────────

/// Handle VM_GETREF — query reference count for a virtual address.
///
/// Corresponds to Minix3's `do_get_refcount()` (mmap.c:463) +
/// `map_get_ref()` (region.c:1343).
///
/// Same constraints as GETPHYS: address must match region start,
/// and memtype must support `refcount`.
pub(crate) fn handle_get_refcount(
    table: &VmProcTable,
    frames: &PageFrames,
    target: Endpoint,
    addr: VirBytes,
) -> Result<u8, QueryError> {
    let slot = table.vm_isokendpt(target)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    let vr = active.regions().find(addr)
        .ok_or(QueryError::NotMapped)?;

    if vr.vaddr != addr {
        return Err(QueryError::NotMapped);
    }

    // C: vr->def_memtype->refcount(vr)
    // In PFN model, refcount is tracked per-page in PageFrames.
    // Return the refcount of the first page as the region's refcount.
    let first_pfn = vr.physblocks.first()
        .filter(|s| s.is_mapped())
        .map(|ps| ps.pfn);
    match first_pfn {
        Some(pfn) => {
            frames.get(pfn)
                .map(|state| state.refcount() as u8)
                .ok_or(QueryError::NotSupported)
        }
        None => Err(QueryError::NotSupported),
    }
}

// ── Handler: INFO ────────────────────────────────────────────────────

/// Handle VM_INFO — query memory statistics, usage, or region info.
///
/// Corresponds to Minix3's `do_info()` (utility.c:100).
///
/// Three query modes:
/// - **Stats**: Global memory statistics (page size, total/free pages).
/// - **Usage**: Per-process memory usage.
/// - **Region**: Per-process region list (paginated via `next` cursor).
///
/// Note: Minix3's `do_info` calls `handle_memory_once()` before
/// `sys_datacopy` to prevent deadlocks from page faults during the
/// copy. Rust does not need this because query results are returned
/// directly through the IPC message, not via cross-address-space copy.
pub(crate) fn handle_info(
    table: &VmProcTable,
    page_alloc: &VmPageAllocator,
    frames: &PageFrames,
    query: InfoQuery,
) -> Result<InfoResult, QueryError> {
    match query {
        InfoQuery::Stats => {
            let stats = page_alloc.phys_alloc().memstats();
            Ok(InfoResult::Stats(StatsInfo {
                page_size: 4096,
                total_pages: page_alloc.total_pages() as u32,
                free_pages: stats.free_pages as u32,
                largest_contiguous: stats.largest_free as u32,
            }))
        }
        InfoQuery::Usage { target } => {
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

                let is_shared_region = vr.flags.contains(
                    crate::region::VrFlags::SHARED
                );

                for slot in &vr.physblocks {
                    if !slot.is_mapped() {
                        // C: unmapped stack pages are discounted from mvirtual.
                        // is_stack_region() heuristic (region.c:1385):
                        // vaddr == VM_STACKTOP - DEFAULT_STACK_LIMIT &&
                        // length == DEFAULT_STACK_LIMIT.
                        // In Rust, we use the same heuristic: the stack
                        // region's end_addr() == active.region_top().
                        if vr.end_addr() == active.region_top() {
                            mvirtual = mvirtual.saturating_sub(PAGE_SIZE);
                        }
                        continue;
                    }

                    // Page is mapped → count towards total.
                    total = total.saturating_add(PAGE_SIZE);

                    // C: ph->ph->refcount > 1 → common.
                    // We look up the refcount from PageFrames.
                    if let Some(state) = frames.get(slot.pfn) {
                        if state.refcount() > 1 {
                            common = common.saturating_add(PAGE_SIZE);
                            // C: common + VR_SHARED → shared (non-COW).
                            if is_shared_region {
                                shared = shared.saturating_add(PAGE_SIZE);
                            }
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
            }))
        }
        InfoQuery::Region { target, count, next } => {
            let slot = table.vm_isokendpt(target)?;
            let active = table.get_active(slot)
                .ok_or(QueryError::ProcessNotFound)?;

            // C uses AVL tree iteration with vaddr-based cursor (`next`).
            // Rust uses Vec-based index cursor (`next` = array index offset).
            let mut result = [RegionInfo {
                vaddr: VirBytes(0),
                length: VirBytes(0),
                flags: 0u16,
            }; 8];
            let mut idx = 0;
            let mut iter_next = next;
            let regions = active.regions();

            for (i, vr) in regions.iter().enumerate() {
                if i < next {
                    continue;
                }
                if idx >= count.min(8) {
                    break;
                }
                result[idx] = RegionInfo {
                    vaddr: vr.vaddr,
                    length: vr.length,
                    flags: vr.flags.bits(),
                };
                idx += 1;
                iter_next = i + 1;
            }

            let has_more = regions.len() > iter_next + idx;
            Ok(InfoResult::Region {
                regions: result,
                count: idx,
                next: if has_more { iter_next + 1 } else { 0 },
            })
        }
    }
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
    // C: if (m->m_source != PM_PROC_NR) return OK;
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

    #[test]
    fn test_query_error_errno() {
        // Tests the full error path: QueryError → From<QueryError> for VmError → VmError::to_errno()
        use minix_types::{VmError, EINVAL, EFAULT, ESRCH};
        assert_eq!(VmError::from(QueryError::ProcessNotFound).to_errno(), EINVAL);  // → InvalidProcess → EINVAL
        assert_eq!(VmError::from(QueryError::NotMapped).to_errno(), EFAULT);        // → InvalidAddress → EFAULT
        assert_eq!(VmError::from(QueryError::NotSupported).to_errno(), EFAULT);     // → InvalidAddress → EFAULT
        assert_eq!(VmError::from(QueryError::InvalidQuery).to_errno(), EFAULT);     // → InvalidAddress → EFAULT
        // Context-dependent mapping for getrusage (C: utility.c:426)
        assert_eq!(QueryError::ProcessNotFound.to_errno_for_rusage(), ESRCH);
    }

    #[test]
    fn test_get_phys_invalid_endpoint() {
        let table = VmProcTable::get_global();
        let result = handle_get_phys(
            table,
            Endpoint(9999),
            VirBytes(0x1000),
        );
        assert_eq!(result, Err(QueryError::ProcessNotFound));
    }

    #[test]
    fn test_get_refcount_invalid_endpoint() {
        use crate::region::PageFrames;
        use minix_types::PhysBytes;
        let table = VmProcTable::get_global();
        let frames = PageFrames::new(PhysBytes(256 * 4096));
        let result = handle_get_refcount(
            table,
            &frames,
            Endpoint(9999),
            VirBytes(0x1000),
        );
        assert_eq!(result, Err(QueryError::ProcessNotFound));
    }

    #[test]
    fn test_getrusage_non_pm() {
        let table = VmProcTable::get_global();
        let result = handle_getrusage(
            table,
            Endpoint(100),
            Endpoint(50),
            false,
        );
        assert_eq!(result, Ok(GetrusageResult::Ok));
    }

    #[test]
    fn test_getrusage_pm_invalid_endpoint() {
        let table = VmProcTable::get_global();
        let result = handle_getrusage(
            table,
            Endpoint::PM,
            Endpoint(9999),
            false,
        );
        assert_eq!(result, Err(QueryError::ProcessNotFound));
    }

    #[test]
    fn test_stats_info_defaults() {
        let info = StatsInfo {
            page_size: 4096,
            total_pages: 0,
            free_pages: 0,
            largest_contiguous: 0,
        };
        assert_eq!(info.page_size, 4096);
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
        // Verify UsageInfo fields match C's struct vm_usage_info
        // (minix/include/minix/vm.h:48):
        //   vui_total, vui_common, vui_shared, vui_virtual, vui_mvirtual
        let info = UsageInfo {
            total: VirBytes(4096),
            common: VirBytes(2048),
            shared: VirBytes(1024),
            virtual_total: VirBytes(8192),
            mvirtual: VirBytes(6144),
        };
        assert_eq!(info.total.0, 4096);
        assert_eq!(info.common.0, 2048);
        assert_eq!(info.shared.0, 1024);
        assert_eq!(info.virtual_total.0, 8192);
        assert_eq!(info.mvirtual.0, 6144);
    }

    #[test]
    fn test_usage_info_shared_semantics() {
        // C semantics (region.c:1417-1420):
        //   shared = pages where refcount > 1 AND region has VR_SHARED flag.
        // This test verifies the field exists and can represent the C value.
        let info = UsageInfo {
            total: VirBytes(3 * 4096),  // 3 mapped pages
            common: VirBytes(2 * 4096), // 2 pages with refcount > 1
            shared: VirBytes(1 * 4096), // 1 of those 2 is VR_SHARED
            virtual_total: VirBytes(4 * 4096),
            mvirtual: VirBytes(4 * 4096),
        };
        // shared <= common (shared is a subset of common)
        assert!(info.shared.0 <= info.common.0);
        // common <= total (common is a subset of total mapped)
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
        };
        let result = InfoResult::Usage(usage);
        if let InfoResult::Usage(u) = result {
            assert_eq!(u.total.0, 0);
            assert_eq!(u.common.0, 0);
            assert_eq!(u.shared.0, 0);
            assert_eq!(u.virtual_total.0, 0);
            assert_eq!(u.mvirtual.0, 0);
        } else {
            panic!("expected InfoResult::Usage");
        }
    }
}
