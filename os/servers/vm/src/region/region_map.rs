//! Region map implementation using BTreeMap.
//!
//! Manages process virtual regions ordered by virtual address.
//! Replaces Minix3's custom AVL tree with `alloc::collections::BTreeMap`,
//! providing the same O(log n) semantics with standard library quality.

use super::vir_region::VirRegion;
use super::page_state::PAGE_SIZE;
use minix_types::VirBytes;
use alloc::collections::BTreeMap;

/// Search direction for region lookups.
///
/// Unlike bitflags, these are mutually exclusive — a search goes in
/// exactly one direction. Using an enum prevents invalid combinations
/// (e.g., LESS | GREATER) that the old bitflags-style u8 allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SearchType {
    /// Exact match only.
    #[default]
    Equal,
    /// Strictly less than target.
    Less,
}

#[derive(Debug, Default)]
pub(crate) struct RegionMap {
    regions: BTreeMap<VirBytes, VirRegion>,
}

impl RegionMap {
    pub(crate) fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
        }
    }

    // V10-P2-1: `len`/`is_empty`/`find_less`/`traverse`/`find_all_overlaps`
    // are test-only today (production uses `find`/`find_mut`/`find_slot`/
    // `find_overlap`/`iter`). Kept as the documented lookup surface; no
    // production caller yet.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn len(&self) -> usize {
        self.regions.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
        self.regions
            .range(..=addr)
            .next_back()
            .filter(|(_, r)| r.contains_addr(addr))
            .map(|(_, r)| r)
    }

    pub(crate) fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion> {
        let key = self
            .regions
            .range(..=addr)
            .next_back()
            .filter(|(_, r)| r.contains_addr(addr))
            .map(|(k, _)| *k)?;

        self.regions.get_mut(&key)
    }

    pub(crate) fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion> {
        match st {
            SearchType::Equal => self.regions.get(&key),
            SearchType::Less => self.regions.range(..key).next_back().map(|(_, r)| r),
        }
    }

    pub(crate) fn find_less(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::Less)
    }

    pub(crate) fn find_mut_by_end(&mut self, end_addr: VirBytes) -> Option<&mut VirRegion> {
        let key = self
            .regions
            .range(..end_addr)
            .next_back()
            .filter(|(_, r)| r.end_addr() == end_addr)
            .map(|(k, _)| *k)?;

        self.regions.get_mut(&key)
    }

    pub(crate) fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion> {
        for (_, r) in self.regions.range(..end) {
            if r.overlaps(start, end) {
                return Some(r);
            }
            if r.vaddr >= end {
                break;
            }
        }
        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn find_all_overlaps(
        &self,
        start: VirBytes,
        end: VirBytes,
    ) -> impl Iterator<Item = &VirRegion> {
        self.regions
            .range(..end)
            .filter(move |(_, r)| r.overlaps(start, end))
            .map(|(_, r)| r)
    }

    pub(crate) fn find_slot(
        &self,
        minv: VirBytes,
        maxv: VirBytes,
        length: VirBytes,
    ) -> Option<VirBytes> {
        if length.0 == 0 {
            return None;
        }

        let maxv = if maxv.0 == 0 {
            VirBytes(minv.0.checked_add(length.0)?)
        } else {
            maxv
        };

        if minv.0 >= maxv.0 || minv.0.checked_add(length.0)? > maxv.0 {
            return None;
        }

        let try_gap = |gap_start: VirBytes, gap_end: VirBytes| -> Option<VirBytes> {
            let frstart = gap_start.max(minv);
            let frend = gap_end.min(maxv);
            if frend.0 > frstart.0 && frend.0.saturating_sub(frstart.0) >= length.0 {
                // Page-align the gap boundaries; VM regions must be page-aligned.
                let aligned_start = (frstart.0 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                let aligned_end = frend.0 & !(PAGE_SIZE - 1);
                if aligned_end > aligned_start
                    && aligned_end.saturating_sub(aligned_start) >= length.0
                {
                    return Some(VirBytes(aligned_end - length.0));
                }
                // No page-aligned fit in this gap — skip rather than returning
                // an unaligned address (alignment fix).
                None
            } else {
                None
            }
        };

        let mut prev_end = minv;

        for region in self.iter() {
            if region.vaddr >= prev_end
                && let Some(addr) = try_gap(prev_end, region.vaddr) {
                    return Some(addr);
                }

            prev_end = region.end_addr().max(prev_end);
        }

        try_gap(prev_end, maxv)
    }

    /// Insert a region into the map.
    ///
    /// Returns `Err(region)` if the new region overlaps an existing region.
    /// The caller is responsible for handling the overlap (e.g., unmapping
    /// the overlapping range first, as MAP_FIXED does).
    /// If the region's start address already exists, returns the old region
    /// via `Option<VirRegion>` (BTreeMap replacement semantics).
    pub(crate) fn insert(&mut self, region: VirRegion) -> Result<Option<VirRegion>, VirRegion> {
        let end = region.end_addr();
        // Check for overlap with any existing region. BTreeMap is sorted by vaddr,
        // so we only need to check the two nearest neighbors.
        if let Some(_existing) = self.find_overlap(region.vaddr, end) {
            return Err(region);
        }
        Ok(self.regions.insert(region.vaddr, region))
    }

    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion> {
        self.regions.remove(&addr)
    }

    pub(crate) fn get_mut(&mut self, addr: &VirBytes) -> Option<&mut VirRegion> {
        self.regions.get_mut(addr)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn traverse<F>(&self, mut f: F)
    where
        F: FnMut(&VirRegion),
    {
        for r in self.regions.values() {
            f(r);
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &VirRegion> {
        self.regions.values()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut VirRegion> {
        self.regions.values_mut()
    }

    pub(crate) fn clear(&mut self) {
        self.regions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vir_region::{VrFlags, VirRegion};

    fn make_region(vaddr: u64, length: u64) -> VirRegion {
        VirRegion::new(VirBytes(vaddr), VirBytes(length), VrFlags::empty())
    }

    /// Helper for tests: insert and unwrap, since test regions never overlap.
    fn insert_unwrap(map: &mut RegionMap, region: VirRegion) -> Option<VirRegion> {
        map.insert(region).unwrap()
    }

    #[test]
    fn test_insert_and_find() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));

        assert_eq!(map.len(), 3);

        let found = map.find(VirBytes(0x1500));
        assert!(found.is_some());
        assert_eq!(found.unwrap().vaddr, VirBytes(0x1000));

        assert!(map.find(VirBytes(0x5000)).is_none());
    }

    #[test]
    fn test_remove() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));

        assert_eq!(map.len(), 3);

        map.remove(VirBytes(0x2000));
        assert_eq!(map.len(), 2);
        assert!(map.find(VirBytes(0x2000)).is_none());
        assert!(map.find(VirBytes(0x1000)).is_some());
        assert!(map.find(VirBytes(0x3000)).is_some());
    }

    #[test]
    fn test_find_overlap() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));

        let overlap = map.find_overlap(VirBytes(0x1500), VirBytes(0x2500));
        assert!(overlap.is_some());
        assert_eq!(overlap.unwrap().vaddr, VirBytes(0x1000));

        assert!(map.find_overlap(VirBytes(0x5000), VirBytes(0x6000)).is_none());
    }

    #[test]
    fn test_traverse() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x3000, 0x1000));
        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));

        let mut addrs = alloc::vec::Vec::new();
        map.traverse(|r| addrs.push(r.vaddr));

        assert_eq!(
            addrs,
            alloc::vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_iter() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x3000, 0x1000));
        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));

        let addrs: alloc::vec::Vec<_> = map.iter().map(|r| r.vaddr).collect();

        assert_eq!(
            addrs,
            alloc::vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_search_type_less() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));
        insert_unwrap(&mut map, make_region(0x5000, 0x1000));

        let result = map.find_less(VirBytes(0x4000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = map.find_less(VirBytes(0x1000));
        assert!(result.is_none());
    }

    #[test]
    fn test_find_slot_basic() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));

        let slot = map.find_slot(VirBytes(0), VirBytes(0x10000), VirBytes(0x800));
        assert!(slot.is_some());
        let s = slot.unwrap();
        assert!(s.0 + 0x800 <= 0x10000, "slot {:?} + 0x800 exceeds 0x10000", s);
    }

    #[test]
    fn test_find_slot_in_gap() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x0000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));

        let slot = map.find_slot(VirBytes(0x1000), VirBytes(0x3000), VirBytes(0x800));
        assert!(slot.is_some());
        let addr = slot.unwrap();
        assert!(addr.0 >= 0x1000);
        assert!(addr.0 + 0x800 <= 0x3000);
    }

    #[test]
    fn test_find_slot_no_space() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x0000, 0x1000));
        insert_unwrap(&mut map, make_region(0x1000, 0x1000));

        let slot = map.find_slot(VirBytes(0), VirBytes(0x2000), VirBytes(0x800));
        assert!(slot.is_none());
    }

    #[test]
    fn test_find_all_overlaps() {
        let mut map = RegionMap::new();

        insert_unwrap(&mut map, make_region(0x1000, 0x1000));
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));
        insert_unwrap(&mut map, make_region(0x3000, 0x1000));

        let overlaps: alloc::vec::Vec<_> = map
            .find_all_overlaps(VirBytes(0x1500), VirBytes(0x3500))
            .map(|r| r.vaddr)
            .collect();

        assert_eq!(
            overlaps,
            alloc::vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    /// Alignment regression test: find_slot must return page-aligned addresses.
    /// When the gap boundaries are not page-aligned, the returned address
    /// must still be page-aligned, or None if no page-aligned fit exists.
    #[test]
    fn test_find_slot_alignment_c09() {
        let mut map = RegionMap::new();

        // Region ends at 0x1F00 (not page-aligned end, but region vaddr is page-aligned)
        // Actually, regions are always page-aligned, so create a gap with
        // non-page-aligned minv/maxv to test alignment logic.
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));

        // Gap: [0, 0x2000). minv=0x0800 (not page-aligned), length=0x1000.
        // Aligned start = round_up(0x0800) = 0x1000.
        // Aligned end = round_down(0x2000) = 0x2000.
        // Available = 0x2000 - 0x1000 = 0x1000 >= length. Should succeed.
        let slot = map.find_slot(VirBytes(0x0800), VirBytes(0x2000), VirBytes(0x1000));
        assert!(slot.is_some());
        let addr = slot.unwrap();
        assert_eq!(addr.0 % PAGE_SIZE, 0, "returned address must be page-aligned");
        assert!(addr.0 >= 0x1000, "should start at aligned boundary 0x1000");
        assert!(addr.0 + 0x1000 <= 0x2000);

        // Gap too small after alignment: minv=0x1800, maxv=0x2000, length=0x1000.
        // Aligned start = round_up(0x1800) = 0x2000. Aligned end = 0x2000.
        // Available = 0. Should return None.
        let slot = map.find_slot(VirBytes(0x1800), VirBytes(0x2000), VirBytes(0x1000));
        assert!(slot.is_none(), "unaligned gap too small after alignment should return None");
    }

    /// Alignment regression test: find_slot returns None when only sub-page gaps exist.
    #[test]
    fn test_find_slot_subpage_gap_c09() {
        let mut map = RegionMap::new();

        // Two regions with only 0x800 bytes between them — less than one page.
        insert_unwrap(&mut map, make_region(0x0000, 0x1800));
        insert_unwrap(&mut map, make_region(0x2000, 0x1000));

        // Gap is [0x1800, 0x2000) = 0x800 bytes, but after alignment:
        // aligned_start = round_up(0x1800) = 0x2000, aligned_end = 0x2000 → 0 available.
        let slot = map.find_slot(VirBytes(0x1800), VirBytes(0x2000), VirBytes(0x1000));
        assert!(slot.is_none(), "sub-page gap should not yield a page-aligned slot");
    }
}
