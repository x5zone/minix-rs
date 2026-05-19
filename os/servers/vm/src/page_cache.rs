//! Page cache implementation (方案三：PFN 索引模型).
//!
//! Uses PFN-based indexing with PageFrames for refcount management.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use minix_types::PhysBytes;
use crate::region::{PageFrames, PageSlot, PageFlags, PFN_NONE, PfnAllocator, PfnAllocError};

#[derive(Debug)]
pub(crate) struct PageCacheEntry {
    pub pfn: u32,
    pub refcount: u16,
}

pub(crate) struct PageCache {
    entries: BTreeMap<u64, PageCacheEntry>,
    total_cached: u64,
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            total_cached: 0,
        }
    }

    pub fn get(&self, block: u64) -> Option<&PageCacheEntry> {
        self.entries.get(&block)
    }

    pub fn get_mut(&mut self, block: u64) -> Option<&mut PageCacheEntry> {
        self.entries.get_mut(&block)
    }

    pub fn insert(&mut self, block: u64, pfn: u32, frames: &mut PageFrames) {
        frames.addcache(pfn);
        self.entries.insert(block, PageCacheEntry { pfn, refcount: 1 });
        self.total_cached += 1;
    }

    pub fn remove(&mut self, block: u64, frames: &mut PageFrames) -> Option<u32> {
        if let Some(entry) = self.entries.remove(&block) {
            frames.rmcache(entry.pfn);
            self.total_cached = self.total_cached.saturating_sub(1);
            Some(entry.pfn)
        } else {
            None
        }
    }

    pub fn increase_refcount(&mut self, block: u64) -> bool {
        if let Some(entry) = self.entries.get_mut(&block) {
            entry.refcount = entry.refcount.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn decrease_refcount(&mut self, block: u64, frames: &mut PageFrames) -> Option<u16> {
        if let Some(entry) = self.entries.get_mut(&block) {
            if entry.refcount > 0 {
                entry.refcount -= 1;
            }
            if entry.refcount == 0 {
                let pfn = entry.pfn;
                frames.rmcache(pfn);
                self.entries.remove(&block);
                self.total_cached = self.total_cached.saturating_sub(1);
                return Some(0);
            }
            Some(entry.refcount)
        } else {
            None
        }
    }

    pub fn total_cached(&self) -> u64 {
        self.total_cached
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn find_by_pfn(&self, pfn: u32) -> Option<u64> {
        for (&block, entry) in &self.entries {
            if entry.pfn == pfn {
                return Some(block);
            }
        }
        None
    }

    pub fn flush_all(&mut self, frames: &mut PageFrames) {
        let keys: alloc::vec::Vec<u64> = self.entries.keys().copied().collect();
        for key in keys {
            if let Some(entry) = self.entries.remove(&key) {
                frames.rmcache(entry.pfn);
            }
        }
        self.total_cached = 0;
    }
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::PAGE_SIZE;

    struct TestAlloc { next: u32 }
    impl PfnAllocator for TestAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            let pfn = self.next;
            self.next += 1;
            Ok(pfn)
        }
        fn free_pfn(&mut self, _pfn: u32) {}
    }

    fn make_frames(pages: u32) -> PageFrames {
        PageFrames::new(PhysBytes(pages as u64 * PAGE_SIZE))
    }

    #[test]
    fn test_page_cache_insert_remove() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        cache.insert(42, pfn, &mut frames);

        assert!(cache.get(42).is_some());
        assert_eq!(cache.get(42).unwrap().pfn, pfn);
        assert_eq!(cache.len(), 1);

        let removed_pfn = cache.remove(42, &mut frames).unwrap();
        assert_eq!(removed_pfn, pfn);
        assert!(cache.get(42).is_none());
    }

    #[test]
    fn test_page_cache_refcount() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        cache.insert(42, pfn, &mut frames);

        assert!(cache.increase_refcount(42));
        assert_eq!(cache.get(42).unwrap().refcount, 2);

        let rc = cache.decrease_refcount(42, &mut frames);
        assert_eq!(rc, Some(1));

        let rc = cache.decrease_refcount(42, &mut frames);
        assert_eq!(rc, Some(0));
        assert!(cache.get(42).is_none());
    }

    #[test]
    fn test_page_cache_find_by_pfn() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        cache.insert(100, pfn, &mut frames);

        assert_eq!(cache.find_by_pfn(pfn), Some(100));
        assert_eq!(cache.find_by_pfn(999), None);
    }

    #[test]
    fn test_page_cache_flush_all() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        cache.insert(1, pfn0, &mut frames);
        cache.insert(2, pfn1, &mut frames);

        assert_eq!(cache.len(), 2);

        cache.flush_all(&mut frames);
        assert!(cache.is_empty());
    }
}
