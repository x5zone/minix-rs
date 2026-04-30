//! AVL tree implementation for managing virtual regions.

use super::vir_region::VirRegion;
use minix_types::VirBytes;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::marker::PhantomData;

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
pub(crate) struct RegionAvl {
    root: Option<Box<VirRegion>>,
    count: usize,
}

impl RegionAvl {
    pub(crate) fn new() -> Self {
        Self {
            root: None,
            count: 0,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
        Self::find_containing(&self.root, addr)
    }

    fn find_containing(node: &Option<Box<VirRegion>>, addr: VirBytes) -> Option<&VirRegion> {
        let n = node.as_ref()?;

        if addr < n.vaddr {
            Self::find_containing(&n.lower, addr)
        } else if addr >= n.end_addr() {
            Self::find_containing(&n.higher, addr)
        } else {
            Some(n)
        }
    }

    pub(crate) fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion> {
        Self::find_containing_mut(&mut self.root, addr)
    }

    fn find_containing_mut(
        node: &mut Option<Box<VirRegion>>,
        addr: VirBytes,
    ) -> Option<&mut VirRegion> {
        let n = node.as_mut()?;

        if addr < n.vaddr {
            Self::find_containing_mut(&mut n.lower, addr)
        } else if addr >= n.end_addr() {
            Self::find_containing_mut(&mut n.higher, addr)
        } else {
            Some(n)
        }
    }

    pub(crate) fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion> {
        Self::search_node(self.root.as_ref(), key, st)
    }

    fn search_node(
        mut node: Option<&Box<VirRegion>>,
        key: VirBytes,
        st: SearchType,
    ) -> Option<&VirRegion> {
        let target_cmp = if st.contains(SearchType::LESS) {
            1i32
        } else if st.contains(SearchType::GREATER) {
            -1i32
        } else {
            0i32
        };

        let mut match_h: Option<&VirRegion> = None;

        while let Some(h) = node {
            let cmp = key.0.cmp(&h.vaddr.0);

            if cmp == Ordering::Equal {
                if st.contains(SearchType::EQUAL) {
                    return Some(h);
                }
                return if target_cmp < 0 {
                    Self::search_node(h.lower.as_ref(), key, st)
                } else {
                    Self::search_node(h.higher.as_ref(), key, st)
                };
            }

            let cmp_val = if cmp == Ordering::Less { -1i32 } else { 1i32 };
            if target_cmp != 0 && (cmp_val ^ target_cmp) >= 0 {
                match_h = Some(h);
            }

            node = if cmp == Ordering::Less {
                h.lower.as_ref()
            } else {
                h.higher.as_ref()
            };
        }

        match_h
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
        Self::find_overlap_node(self.root.as_ref(), start, end)
    }

    fn find_overlap_node(
        node: Option<&Box<VirRegion>>,
        start: VirBytes,
        end: VirBytes,
    ) -> Option<&VirRegion> {
        let n = node?;

        if n.vaddr < end && n.end_addr() > start {
            return Some(n);
        }

        if end <= n.vaddr {
            Self::find_overlap_node(n.lower.as_ref(), start, end)
        } else if start >= n.end_addr() {
            Self::find_overlap_node(n.higher.as_ref(), start, end)
        } else {
            None
        }
    }

    pub(crate) fn find_all_overlaps<'a>(
        &'a self,
        start: VirBytes,
        end: VirBytes,
    ) -> impl Iterator<Item = &'a VirRegion> {
        self.iter().filter(move |r| r.overlaps(start, end))
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

        let mut prev_end = minv;

        for region in self.iter() {
            if region.vaddr >= prev_end {
                let gap_start = prev_end.max(minv);
                let gap_end = region.vaddr.min(maxv);

                if gap_end.0 > gap_start.0
                    && gap_end.0.saturating_sub(gap_start.0) >= length.0
                {
                    return Some(VirBytes(gap_end.0 - length.0));
                }
            }

            prev_end = region.end_addr().max(prev_end);
        }

        let gap_start = prev_end.max(minv);
        if maxv.0 > gap_start.0
            && maxv.0.saturating_sub(gap_start.0) >= length.0
        {
            return Some(VirBytes(maxv.0 - length.0));
        }

        None
    }

    pub(crate) fn insert(&mut self, region: VirRegion) {
        let was_inserted = Self::insert_node(&mut self.root, region);
        if was_inserted {
            self.count += 1;
        }
    }

    fn insert_node(node: &mut Option<Box<VirRegion>>, mut region: VirRegion) -> bool {
        match node {
            None => {
                *node = Some(Box::new(region));
                true
            }
            Some(n) => {
                if region.vaddr < n.vaddr {
                    Self::insert_node(&mut n.lower, region)
                } else if region.vaddr > n.vaddr {
                    Self::insert_node(&mut n.higher, region)
                } else {
                    region.lower = n.lower.take();
                    region.higher = n.higher.take();
                    region.factor = n.factor;
                    **n = region;
                    false
                }
            }
        }
    }

    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion> {
        Self::remove_node(&mut self.root, addr).map(|region| {
            self.count -= 1;
            *region
        })
    }

    fn remove_node(node: &mut Option<Box<VirRegion>>, addr: VirBytes) -> Option<Box<VirRegion>> {
        let n = node.as_mut()?;

        if addr < n.vaddr {
            return Self::remove_node(&mut n.lower, addr);
        } else if addr > n.vaddr {
            return Self::remove_node(&mut n.higher, addr);
        }

        let mut removed = node.take().unwrap();

        match (removed.lower.take(), removed.higher.take()) {
            (None, None) => Some(removed),
            (Some(left), None) => {
                *node = Some(left);
                Some(removed)
            }
            (None, Some(right)) => {
                *node = Some(right);
                Some(removed)
            }
            (Some(left), Some(right)) => {
                *node = Some(right);
                let mut current = node.as_mut().unwrap();
                while current.lower.is_some() {
                    current = current.lower.as_mut().unwrap();
                }
                current.lower = Some(left);
                Some(removed)
            }
        }
    }

    pub(crate) fn traverse<F>(&self, mut f: F)
    where
        F: FnMut(&VirRegion),
    {
        Self::traverse_node(&self.root, &mut f);
    }

    fn traverse_node<F>(node: &Option<Box<VirRegion>>, f: &mut F)
    where
        F: FnMut(&VirRegion),
    {
        if let Some(n) = node {
            Self::traverse_node(&n.lower, f);
            f(n);
            Self::traverse_node(&n.higher, f);
        }
    }

    pub(crate) fn iter(&self) -> RegionIter<'_> {
        let mut stack = Vec::new();
        let mut node = self.root.as_ref();
        while let Some(n) = node {
            stack.push(n.as_ref());
            node = n.lower.as_ref();
        }
        RegionIter { stack }
    }

    pub(crate) fn iter_mut(&mut self) -> RegionIterMut<'_> {
        let mut stack = Vec::new();
        let mut node = self.root.as_mut();
        while let Some(n) = node {
            stack.push(n.as_mut() as *mut VirRegion);
            node = n.lower.as_mut();
        }
        RegionIterMut { stack, _marker: PhantomData }
    }

    pub(crate) fn clear(&mut self) {
        if let Some(root) = self.root.take() {
            Self::clear_recursive(root);
        }
        self.count = 0;
    }

    fn clear_recursive(node: Box<VirRegion>) {
        if let Some(lower) = node.lower {
            Self::clear_recursive(lower);
        }
        if let Some(higher) = node.higher {
            Self::clear_recursive(higher);
        }
    }
}

pub(crate) struct RegionIter<'a> {
    stack: Vec<&'a VirRegion>,
}

impl<'a> Iterator for RegionIter<'a> {
    type Item = &'a VirRegion;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;

        let mut right = node.higher.as_ref();
        while let Some(r) = right {
            self.stack.push(r.as_ref());
            right = r.lower.as_ref();
        }

        Some(node)
    }
}

pub(crate) struct RegionIterMut<'a> {
    stack: Vec<*mut VirRegion>,
    _marker: PhantomData<&'a mut VirRegion>,
}

impl<'a> Iterator for RegionIterMut<'a> {
    type Item = &'a mut VirRegion;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;

        let node_ref = unsafe { &mut *node };
        let mut right = node_ref.higher.as_mut();
        while let Some(r) = right {
            self.stack.push(r.as_mut() as *mut VirRegion);
            right = r.lower.as_mut();
        }

        Some(unsafe { &mut *node })
    }
}

impl<'a> IntoIterator for &'a RegionAvl {
    type Item = &'a VirRegion;
    type IntoIter = RegionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vir_region::{VrFlags, VirRegion};
    use alloc::vec;

    fn make_region(vaddr: u64, length: u64) -> VirRegion {
        VirRegion::new(VirBytes(vaddr), VirBytes(length), VrFlags::empty())
    }

    #[test]
    fn test_avl_insert_and_find() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        assert_eq!(avl.len(), 3);

        let found = avl.find(VirBytes(0x1500));
        assert!(found.is_some());
        assert_eq!(found.unwrap().vaddr, VirBytes(0x1000));

        assert!(avl.find(VirBytes(0x5000)).is_none());
    }

    #[test]
    fn test_avl_remove() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        assert_eq!(avl.len(), 3);

        avl.remove(VirBytes(0x2000));
        assert_eq!(avl.len(), 2);
        assert!(avl.find(VirBytes(0x2000)).is_none());
        assert!(avl.find(VirBytes(0x1000)).is_some());
        assert!(avl.find(VirBytes(0x3000)).is_some());
    }

    #[test]
    fn test_avl_find_overlap() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let overlap = avl.find_overlap(VirBytes(0x1500), VirBytes(0x2500));
        assert!(overlap.is_some());
        assert_eq!(overlap.unwrap().vaddr, VirBytes(0x1000));

        assert!(avl.find_overlap(VirBytes(0x5000), VirBytes(0x6000)).is_none());
    }

    #[test]
    fn test_avl_traverse() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        let mut addrs = Vec::new();
        avl.traverse(|r| addrs.push(r.vaddr));

        assert_eq!(
            addrs,
            vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_avl_iter() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        let addrs: Vec<_> = avl.iter().map(|r| r.vaddr).collect();

        assert_eq!(
            addrs,
            vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_search_type_less() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x5000, 0x1000));

        let result = avl.find_less(VirBytes(0x4000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = avl.find_less(VirBytes(0x1000));
        assert!(result.is_none());
    }

    #[test]
    fn test_search_type_greater() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x5000, 0x1000));

        let result = avl.find_greater(VirBytes(0x2000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = avl.find_greater(VirBytes(0x5000));
        assert!(result.is_none());
    }

    #[test]
    fn test_search_type_less_equal() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let result = avl.find_less_equal(VirBytes(0x3000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = avl.find_less_equal(VirBytes(0x2500));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x1000));
    }

    #[test]
    fn test_search_type_greater_equal() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let result = avl.find_greater_equal(VirBytes(0x1000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x1000));

        let result = avl.find_greater_equal(VirBytes(0x2000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));
    }

    #[test]
    fn test_find_slot_basic() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let slot = avl.find_slot(VirBytes(0), VirBytes(0x10000), VirBytes(0x800));
        assert!(slot.is_some());
        assert!(slot.unwrap().0 >= 0 && slot.unwrap().0 + 0x800 <= 0x10000);
    }

    #[test]
    fn test_find_slot_in_gap() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x0000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let slot = avl.find_slot(VirBytes(0x1000), VirBytes(0x3000), VirBytes(0x800));
        assert!(slot.is_some());
        let addr = slot.unwrap();
        assert!(addr.0 >= 0x1000);
        assert!(addr.0 + 0x800 <= 0x3000);
    }

    #[test]
    fn test_find_slot_no_space() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x0000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));

        let slot = avl.find_slot(VirBytes(0), VirBytes(0x2000), VirBytes(0x800));
        assert!(slot.is_none());
    }

    #[test]
    fn test_find_all_overlaps() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let overlaps: Vec<_> = avl
            .find_all_overlaps(VirBytes(0x1500), VirBytes(0x3500))
            .map(|r| r.vaddr)
            .collect();

        assert_eq!(
            overlaps,
            vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
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
