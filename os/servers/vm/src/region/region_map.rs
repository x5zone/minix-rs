//! Region map implementation using BTreeMap.
//!
//! Manages process virtual regions ordered by virtual address.
//! Replaces Minix3's custom AVL tree with `alloc::collections::BTreeMap`,
//! providing the same O(log n) semantics with standard library quality.

use super::vir_region::VirRegion;
use super::page_state::PAGE_SIZE;
use minix_types::VirBytes;
use alloc::collections::BTreeMap;
use core::ops::Bound;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchType(u8);

impl SearchType {
    pub(crate) const EQUAL: Self = Self(1);
    pub(crate) const LESS: Self = Self(2);
    pub(crate) const GREATER: Self = Self(4);
    pub(crate) const LESS_EQUAL: Self = Self(3);
    pub(crate) const GREATER_EQUAL: Self = Self(5);

    pub(crate) fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl Default for SearchType {
    fn default() -> Self {
        Self::EQUAL
    }
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
        if st.contains(SearchType::EQUAL) {
            if let Some(r) = self.regions.get(&key) {
                return Some(r);
            }
        }

        if st.contains(SearchType::LESS) {
            if let Some((_, r)) = self.regions.range(..key).next_back() {
                return Some(r);
            }
        }

        if st.contains(SearchType::GREATER) {
            if let Some((_, r)) = self.regions.range((Bound::Excluded(key), Bound::Unbounded)).next() {
                return Some(r);
            }
        }

        None
    }

    pub(crate) fn find_less(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS)
    }

    pub(crate) fn find_greater(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER)
    }

    pub(crate) fn find_less_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS_EQUAL)
    }

    pub(crate) fn find_greater_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER_EQUAL)
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

    pub(crate) fn find_all_overlaps<'a>(
        &'a self,
        start: VirBytes,
        end: VirBytes,
    ) -> impl Iterator<Item = &'a VirRegion> {
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
                let aligned_start = (frstart.0 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                let aligned_end = frend.0 & !(PAGE_SIZE - 1);
                if aligned_end > aligned_start
                    && aligned_end.saturating_sub(aligned_start) >= length.0
                {
                    return Some(VirBytes(aligned_end - length.0));
                }
                return Some(VirBytes(frend.0 - length.0));
            }
            None
        };

        let mut prev_end = minv;

        for region in self.iter() {
            if region.vaddr >= prev_end {
                if let Some(addr) = try_gap(prev_end, region.vaddr) {
                    return Some(addr);
                }
            }

            prev_end = region.end_addr().max(prev_end);
        }

        try_gap(prev_end, maxv)
    }

    pub(crate) fn insert(&mut self, region: VirRegion) -> Option<VirRegion> {
        self.regions.insert(region.vaddr, region)
    }

    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion> {
        self.regions.remove(&addr)
    }

    pub(crate) fn traverse<F>(&self, mut f: F)
    where
        F: FnMut(&VirRegion),
    {
        for (_, r) in &self.regions {
            f(r);
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &VirRegion> {
        self.regions.iter().map(|(_, r)| r)
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut VirRegion> {
        self.regions.iter_mut().map(|(_, r)| r)
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

    #[test]
    fn test_insert_and_find() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));
        map.insert(make_region(0x2000, 0x1000));

        assert_eq!(map.len(), 3);

        let found = map.find(VirBytes(0x1500));
        assert!(found.is_some());
        assert_eq!(found.unwrap().vaddr, VirBytes(0x1000));

        assert!(map.find(VirBytes(0x5000)).is_none());
    }

    #[test]
    fn test_remove() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x2000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

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

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

        let overlap = map.find_overlap(VirBytes(0x1500), VirBytes(0x2500));
        assert!(overlap.is_some());
        assert_eq!(overlap.unwrap().vaddr, VirBytes(0x1000));

        assert!(map.find_overlap(VirBytes(0x5000), VirBytes(0x6000)).is_none());
    }

    #[test]
    fn test_traverse() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x3000, 0x1000));
        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x2000, 0x1000));

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

        map.insert(make_region(0x3000, 0x1000));
        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x2000, 0x1000));

        let addrs: alloc::vec::Vec<_> = map.iter().map(|r| r.vaddr).collect();

        assert_eq!(
            addrs,
            alloc::vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_search_type_less() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));
        map.insert(make_region(0x5000, 0x1000));

        let result = map.find_less(VirBytes(0x4000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = map.find_less(VirBytes(0x1000));
        assert!(result.is_none());
    }

    #[test]
    fn test_search_type_greater() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));
        map.insert(make_region(0x5000, 0x1000));

        let result = map.find_greater(VirBytes(0x2000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = map.find_greater(VirBytes(0x5000));
        assert!(result.is_none());
    }

    #[test]
    fn test_search_type_less_equal() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

        let result = map.find_less_equal(VirBytes(0x3000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = map.find_less_equal(VirBytes(0x2500));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x1000));
    }

    #[test]
    fn test_search_type_greater_equal() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

        let result = map.find_greater_equal(VirBytes(0x1000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x1000));

        let result = map.find_greater_equal(VirBytes(0x2000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));
    }

    #[test]
    fn test_find_slot_basic() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

        let slot = map.find_slot(VirBytes(0), VirBytes(0x10000), VirBytes(0x800));
        assert!(slot.is_some());
        assert!(slot.unwrap().0 >= 0 && slot.unwrap().0 + 0x800 <= 0x10000);
    }

    #[test]
    fn test_find_slot_in_gap() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x0000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

        let slot = map.find_slot(VirBytes(0x1000), VirBytes(0x3000), VirBytes(0x800));
        assert!(slot.is_some());
        let addr = slot.unwrap();
        assert!(addr.0 >= 0x1000);
        assert!(addr.0 + 0x800 <= 0x3000);
    }

    #[test]
    fn test_find_slot_no_space() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x0000, 0x1000));
        map.insert(make_region(0x1000, 0x1000));

        let slot = map.find_slot(VirBytes(0), VirBytes(0x2000), VirBytes(0x800));
        assert!(slot.is_none());
    }

    #[test]
    fn test_find_all_overlaps() {
        let mut map = RegionMap::new();

        map.insert(make_region(0x1000, 0x1000));
        map.insert(make_region(0x2000, 0x1000));
        map.insert(make_region(0x3000, 0x1000));

        let overlaps: alloc::vec::Vec<_> = map
            .find_all_overlaps(VirBytes(0x1500), VirBytes(0x3500))
            .map(|r| r.vaddr)
            .collect();

        assert_eq!(
            overlaps,
            alloc::vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_search_type_flags() {
        assert!(SearchType::LESS_EQUAL.contains(SearchType::EQUAL));
        assert!(SearchType::LESS_EQUAL.contains(SearchType::LESS));
        assert!(!SearchType::LESS_EQUAL.contains(SearchType::GREATER));

        assert!(SearchType::GREATER_EQUAL.contains(SearchType::EQUAL));
        assert!(SearchType::GREATER_EQUAL.contains(SearchType::GREATER));
        assert!(!SearchType::GREATER_EQUAL.contains(SearchType::LESS));
    }
}
