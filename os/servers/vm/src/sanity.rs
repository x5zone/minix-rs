//! VM sanity checking — physical page refcount verification.
//!
//! Corresponds to Minix3's `map_sanitycheck()` in `region.c:168-261`.
//!
//! # Algorithm
//!
//! 1. Traverse all active processes' VirRegions, counting per-PFN references
//!    from `physblocks` (each mapped PageSlot counts as one reference).
//! 2. For each PFN with `IN_CACHE` flag, add one extra reference
//!    (cache holds a reference, matching C's `PBF_INCACHE` logic).
//! 3. Compare computed counts with `PageFrames.refcount`.
//! 4. Return `Ok(())` if all match, or `Err` with mismatch details.
//!
//! # Differences from Minix3
//!
//! - Minix3 uses `seencount` field on `phys_block` (mutates during check).
//!   Rust uses a local `BTreeMap` to avoid mutating shared state.
//! - Minix3 checks `firstregion` linked list integrity. Rust's `Vec<Option<PageSlot>>`
//!   replaces the linked list, so this check is unnecessary.
//! - Minix3 verifies page table mappings via `map_sanitycheck_pt`. This is
//!   deferred to a future implementation (requires Paging trait access).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use minix_types::{Endpoint, UserSlot};
use crate::region::page_state::{PageFrames, PageFlags};
use crate::region::VirRegion;
use crate::vmproc::VmProcTable;

/// A single refcount mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // V10-P2-1 (DEFERRED): see verify_refcounts below
pub struct RefcountMismatch {
    pub pfn: u32,
    pub expected: u32,
    pub actual: u32,
}

/// Verifies that physical page reference counts are consistent with
/// the actual mapping state across all processes.
///
/// Returns `Ok(())` if all refcounts match, or `Err(Vec<RefcountMismatch>)`
/// listing each PFN where `PageFrames.refcount` differs from the computed count.
///
/// # C Reference
///
/// `map_sanitycheck()` — `region.c:168-261`
///
/// ```c
/// ALLREGIONS(;,USE(pr->ph, pr->ph->seencount = 0;););
/// ALLREGIONS(;,USE(pr->ph, pr->ph->seencount++;););
/// ALLREGIONS(;,if(pr->ph->flags & PBF_INCACHE) pr->ph->seencount++;);
/// ALLREGIONS(;,MYASSERT(pr->ph->refcount == pr->ph->seencount););
/// ```
// V10-P2-1 (DEFERRED): C compiles this under SANITYCHECKS; Rust has no
// production entry point yet — wire it behind a `sanity_checks` feature +
// periodic main-loop call when the allocator/region work stabilizes.
#[cfg_attr(not(test), allow(dead_code))]
pub fn verify_refcounts(
    frames: &PageFrames,
    table: &VmProcTable,
) -> Result<(), Vec<RefcountMismatch>> {
    // Phase 1: Count per-PFN references from all processes' VirRegions.
    // Equivalent to C's seencount pass (region.c:218-226).
    let mut seen: BTreeMap<u32, u32> = BTreeMap::new();

    table.for_each_active_region(|_slot: UserSlot, _endpoint: Endpoint, region: &VirRegion| {
        for slot in region.physblocks.iter() {
            if let Some(pfn) = slot.pfn() {
                *seen.entry(pfn).or_insert(0) += 1;
            }
        }
    });

    // Phase 2: Add cache references.
    // C: `if(pr->ph->flags & PBF_INCACHE) pr->ph->seencount++;`
    // In Rust, IN_CACHE on PageState means the page is in the page cache,
    // which holds an extra reference (matching C's PBF_INCACHE logic).
    for (&pfn, count) in seen.iter_mut() {
        if let Some(state) = frames.get(pfn)
            && state.flags.contains(PageFlags::IN_CACHE) {
                *count += 1;
            }
    }

    // Also check PFNs that have refcount > 0 or IN_CACHE but no VirRegion references.
    // These include cache-only pages (IN_CACHE + refcount=1) and orphaned pages
    // (refcount > 0 but no references found).
    for pfn in 0..frames.total_pages() {
        if let Some(state) = frames.get(pfn)
            && !seen.contains_key(&pfn) {
                let has_refcount = state.refcount > 0;
                let has_cache = state.flags.contains(PageFlags::IN_CACHE);
                if has_refcount || has_cache {
                    // No VirRegion references, so computed count = 1 if IN_CACHE, else 0.
                    let expected = if has_cache { 1 } else { 0 };
                    seen.insert(pfn, expected);
                }
            }
    }

    // Phase 3: Compare with PageFrames.refcount.
    // C: `MYASSERT(pr->ph->refcount == pr->ph->seencount);`
    let mut mismatches = Vec::new();

    for (&pfn, &computed_count) in seen.iter() {
        let actual = frames.get(pfn).map_or(0, |s| s.refcount);
        if computed_count != actual {
            mismatches.push(RefcountMismatch {
                pfn,
                expected: computed_count,
                actual,
            });
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::page_state::{PageFrames, PageFlags, PAGE_SIZE};
    use minix_types::PhysBytes;

    #[test]
    fn test_verify_refcounts_empty() {
        let frames = PageFrames::new(PhysBytes(PAGE_SIZE * 4));
        let table = VmProcTable::get_global();
        // No processes, no pages mapped — should pass.
        assert!(verify_refcounts(&frames, table).is_ok());
    }

    #[test]
    fn test_verify_refcounts_mismatch_detected() {
        let mut frames = PageFrames::new(PhysBytes(PAGE_SIZE * 4));
        // Manually set a refcount that doesn't match any mapping.
        if let Some(state) = frames.get_mut(0) {
            state.refcount = 5;
        }
        let table = VmProcTable::get_global();
        let result = verify_refcounts(&frames, table);
        assert!(result.is_err());
        let mismatches = result.unwrap_err();
        assert!(mismatches.iter().any(|m| m.pfn == 0 && m.actual == 5));
    }

    #[test]
    fn test_verify_refcounts_cache_only_page() {
        let mut frames = PageFrames::new(PhysBytes(PAGE_SIZE * 4));
        // A page in cache with refcount=1 should pass.
        frames.addcache(1);
        let table = VmProcTable::get_global();
        assert!(verify_refcounts(&frames, table).is_ok());
    }

    #[test]
    fn test_verify_refcounts_cache_mismatch() {
        let mut frames = PageFrames::new(PhysBytes(PAGE_SIZE * 4));
        // Set IN_CACHE flag but refcount=0 — mismatch.
        if let Some(state) = frames.get_mut(2) {
            state.flags.insert(PageFlags::IN_CACHE);
            // refcount stays 0 (addcache would have set it to 1)
        }
        let table = VmProcTable::get_global();
        let result = verify_refcounts(&frames, table);
        assert!(result.is_err());
        let mismatches = result.unwrap_err();
        assert!(mismatches.iter().any(|m| m.pfn == 2));
    }
}
