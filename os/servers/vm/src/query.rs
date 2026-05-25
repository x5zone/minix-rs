//! VM query service handlers.
//!
//! Handles VM_INFO, VM_GETPHYS, VM_GETREF, and VM_GETRUSAGE requests.
//! These are read-only, side-effect-free queries into VM's internal state.
//!
//! Corresponds to Minix3's `do_info()` (utility.c:100),
//! `do_get_phys()` (mmap.c:438), `do_get_refcount()` (mmap.c:463),
//! and `do_getrusage()` (utility.c:426).

use minix_types::{VirBytes, EINVAL, ESRCH, Endpoint, PhysBytes};
use crate::vmproc::VmProcTable;
use crate::region::{PageFrames, VrFlags};
use crate::alloc_page::VmPageAllocator;
use crate::phys_mem::PhysMemStats;

// ── Error type ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryError {
    ProcessNotFound,
    NotMapped,
    NotSupported,
    InvalidQuery,
}

impl QueryError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => EINVAL,
            Self::NotMapped => EINVAL,
            Self::NotSupported => EINVAL,
            Self::InvalidQuery => EINVAL,
        }
    }

    pub(crate) fn to_errno_for_rusage(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            _ => self.to_errno(),
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsageInfo {
    pub total: VirBytes,
    pub shared: VirBytes,
    pub text: VirBytes,
    pub data: VirBytes,
    pub stack: VirBytes,
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
    let slot = table.vm_isokendpt(target)
        .map_err(|_| QueryError::ProcessNotFound)?;

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
    let slot = table.vm_isokendpt(target)
        .map_err(|_| QueryError::ProcessNotFound)?;

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
        .and_then(|s| s.as_ref())
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
            let slot = table.vm_isokendpt(target)
                .map_err(|_| QueryError::ProcessNotFound)?;
            let active = table.get_active(slot)
                .ok_or(QueryError::ProcessNotFound)?;
            Ok(InfoResult::Usage(UsageInfo {
                total: active.total(),
                shared: VirBytes(0),
                text: VirBytes(0),
                data: VirBytes(0),
                stack: VirBytes(0),
            }))
        }
        InfoQuery::Region { target, count, next } => {
            let slot = table.vm_isokendpt(target)
                .map_err(|_| QueryError::ProcessNotFound)?;
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

    let slot = table.vm_isokendpt(target)
        .map_err(|_| QueryError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    if !children {
        Ok(GetrusageResult::Data(ResourceUsage {
            max_rss_kb: active.total_max().0 / 1024,
            minor_faults: active.minor_fault(),
            major_faults: active.major_fault(),
        }))
    } else {
        // C: XXX TODO — children path not implemented
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
        assert_eq!(QueryError::ProcessNotFound.to_errno(), EINVAL);
        assert_eq!(QueryError::NotMapped.to_errno(), EINVAL);
        assert_eq!(QueryError::NotSupported.to_errno(), EINVAL);
        assert_eq!(QueryError::InvalidQuery.to_errno(), EINVAL);
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
}
