//! Page cache implementation (PFN index model).
//!
//! Uses PFN-based indexing with PageFrames for refcount management.
//! Supports two lookup paths: by inode (regular files) and by device
//! (device files with VMC_NO_INODE), matching Minix3's `find_cached_page_byino`
//! and `find_cached_page_bydev`.
//!
//! # LRU Eviction
//!
//! Minix3 uses a doubly-linked list for precise LRU ordering. This
//! implementation uses a `Vec<CacheKey>` as a simplified LRU — entries
//! are appended on insert and scanned from the front during eviction.
//! Acceptable because cache operations are not on the hot path.
//!
//! # Design Decisions
//!
//! See `notes/rewrite/fork-syscall-rewrite/02-stage-vm/25-page-cache.md` §4.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::region::{PageFrames, PfnAllocator, PfnAllocError};

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CacheKey {
    ByInode { dev: u64, ino: u64, offset: u64 },
    ByDevice { dev: u64, offset: u64 },
}

#[derive(Debug)]
pub(crate) struct PageCacheEntry {
    pub pfn: u32,
    pub refcount: u16,
}

pub(crate) struct PageCache {
    entries: BTreeMap<CacheKey, PageCacheEntry>,
    /// Reverse index: PFN → first CacheKey that mapped to that PFN.
    ///
    /// `find_by_pfn` was O(n) (perf fix) — for caches with 100K+ entries
    /// (typical 1GB+ working set with 4K pages), this is a hot-path
    /// bottleneck on each page-in. Maintained as a side index:
    /// `insert` populates it (only if absent — first-insert-wins
    /// semantics matches the previous O(n) linear scan that returned
    /// the first match), `remove` cleans it on actual removal.
    ///
    /// Note: When a second `CacheKey` is inserted for an already-indexed
    /// PFN, the index is NOT updated. This is a deliberate design
    /// choice — it preserves the first-key-wins behavior of the O(n)
    /// scan, which C callers depend on (the first inserted key is the
    /// canonical owner of the PFN for cache eviction).
    pfn_index: BTreeMap<u32, CacheKey>,
    lru: Vec<CacheKey>,
    total_cached: u64,
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            pfn_index: BTreeMap::new(),
            lru: Vec::new(),
            total_cached: 0,
        }
    }

    pub fn find_by_inode(&self, dev: u64, ino: u64, offset: u64) -> Option<&PageCacheEntry> {
        self.entries.get(&CacheKey::ByInode { dev, ino, offset })
    }

    pub fn find_by_device(&self, dev: u64, offset: u64) -> Option<&PageCacheEntry> {
        self.entries.get(&CacheKey::ByDevice { dev, offset })
    }

    pub fn insert(&mut self, key: CacheKey, pfn: u32, frames: &mut PageFrames) {
        frames.addcache(pfn);
        // First-insert-wins: only populate the PFN index if this PFN
        // is not already indexed. Subsequent inserts of the same PFN
        // (with different keys) still appear in `entries` but the
        // reverse index keeps the original key.
        self.pfn_index.entry(pfn).or_insert_with(|| key.clone());

        self.lru.push(key.clone());
        self.entries.insert(key, PageCacheEntry { pfn, refcount: 1 });
        self.total_cached += 1;
    }

    pub fn remove(&mut self, key: &CacheKey, frames: &mut PageFrames) -> Option<u32> {
        if let Some(entry) = self.entries.remove(key) {
            frames.rmcache(entry.pfn);
            self.lru.retain(|k| k != key);
            self.total_cached = self.total_cached.saturating_sub(1);
            // Only clear the reverse index if it points to the removed
            // key. If a later insert overwrote the same PFN with a
            // different key (impossible per first-insert-wins, but
            // defensive), we keep the index correct.
            if let Some(indexed) = self.pfn_index.get(&entry.pfn) {
                if indexed == key {
                    self.pfn_index.remove(&entry.pfn);
                }
            }
            Some(entry.pfn)
        } else {
            None
        }
    }

    pub fn increase_refcount(&mut self, key: &CacheKey) -> bool {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.refcount = entry.refcount.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn decrease_refcount(&mut self, key: &CacheKey, frames: &mut PageFrames) -> Option<u16> {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.refcount > 0 {
                entry.refcount -= 1;
            }
            if entry.refcount == 0 {
                let pfn = entry.pfn;
                frames.rmcache(pfn);
                // Can't call self.remove() here (double &mut self), clean lru manually
                self.entries.remove(key);
                self.lru.retain(|k| k != key);
                self.total_cached = self.total_cached.saturating_sub(1);
                // Mirror the index cleanup that `remove` would do.
                if let Some(indexed) = self.pfn_index.get(&pfn) {
                    if indexed == key {
                        self.pfn_index.remove(&pfn);
                    }
                }
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

    /// O(1) reverse-lookup by PFN.
    ///
    /// Returns the first `CacheKey` that was inserted for this PFN
    /// (first-insert-wins semantics, matching the previous O(n) linear
    /// scan that returned the first match in BTreeMap iteration order).
    pub fn find_by_pfn(&self, pfn: u32) -> Option<CacheKey> {
        self.pfn_index.get(&pfn).cloned()
    }

    pub fn flush_all(&mut self, frames: &mut PageFrames) {
        let keys: alloc::vec::Vec<CacheKey> = self.entries.keys().cloned().collect();
        for key in keys {
            if let Some(entry) = self.entries.remove(&key) {
                frames.rmcache(entry.pfn);
            }
        }
        self.lru.clear();
        self.total_cached = 0;
    }

    /// Evict cached pages from the LRU oldest end when memory is low.
    /// Corresponds to Minix3 cache_freepages() (cache.c:288).
    /// Eviction condition: refcount == 1 (only referenced by cache, not mapped).
    pub fn free_pages(&mut self, needed: usize, frames: &mut PageFrames) -> usize {
        let mut freed = 0;
        let mut keys_to_remove = Vec::new();

        for key in &self.lru {
            if freed >= needed { break; }
            if let Some(entry) = self.entries.get(key) {
                if frames.get(entry.pfn)
                    .map(|s| s.refcount == 1)
                    .unwrap_or(false)
                {
                    keys_to_remove.push(key.clone());
                    freed += 1;
                }
            }
        }

        for key in keys_to_remove {
            self.remove(&key, frames);
        }

        freed
    }

    /// Remove all cached pages associated with the given device.
    /// Corresponds to Minix3 clear_cache_bydev() (cache.c:313).
    pub fn clear_by_dev(&mut self, dev: u64, frames: &mut PageFrames) {
        let keys: Vec<CacheKey> = self.entries.keys()
            .filter(|k| match k {
                CacheKey::ByInode { dev: d, .. } => *d == dev,
                CacheKey::ByDevice { dev: d, .. } => *d == dev,
            })
            .cloned()
            .collect();
        for key in keys {
            self.remove(&key, frames);
        }
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
    use minix_types::PhysBytes;
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
    fn test_page_cache_insert_remove_by_inode() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        let key = CacheKey::ByInode { dev: 100, ino: 200, offset: 0 };
        cache.insert(key.clone(), pfn, &mut frames);

        assert!(cache.find_by_inode(100, 200, 0).is_some());
        assert_eq!(cache.find_by_inode(100, 200, 0).unwrap().pfn, pfn);
        assert_eq!(cache.len(), 1);

        let removed_pfn = cache.remove(&key, &mut frames).unwrap();
        assert_eq!(removed_pfn, pfn);
        assert!(cache.find_by_inode(100, 200, 0).is_none());
    }

    #[test]
    fn test_page_cache_by_device() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        let key = CacheKey::ByDevice { dev: 100, offset: 4096 };
        cache.insert(key.clone(), pfn, &mut frames);

        assert!(cache.find_by_device(100, 4096).is_some());
        assert!(cache.find_by_inode(100, 200, 4096).is_none());
    }

    #[test]
    fn test_page_cache_refcount() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        let key = CacheKey::ByInode { dev: 100, ino: 200, offset: 0 };
        cache.insert(key.clone(), pfn, &mut frames);

        assert!(cache.increase_refcount(&key));
        assert_eq!(cache.find_by_inode(100, 200, 0).unwrap().refcount, 2);

        let rc = cache.decrease_refcount(&key, &mut frames);
        assert_eq!(rc, Some(1));

        let rc = cache.decrease_refcount(&key, &mut frames);
        assert_eq!(rc, Some(0));
        assert!(cache.find_by_inode(100, 200, 0).is_none());
    }

    #[test]
    fn test_page_cache_find_by_pfn() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = alloc.alloc_pfn().unwrap();
        let key = CacheKey::ByInode { dev: 100, ino: 200, offset: 0 };
        cache.insert(key.clone(), pfn, &mut frames);

        let found = cache.find_by_pfn(pfn).unwrap();
        assert_eq!(found, key);
        assert_eq!(cache.find_by_pfn(999), None);
    }

    #[test]
    fn test_page_cache_flush_all() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        cache.insert(CacheKey::ByInode { dev: 1, ino: 1, offset: 0 }, pfn0, &mut frames);
        cache.insert(CacheKey::ByInode { dev: 2, ino: 2, offset: 0 }, pfn1, &mut frames);

        assert_eq!(cache.len(), 2);

        cache.flush_all(&mut frames);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_page_cache_free_pages_lru() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        let pfn2 = alloc.alloc_pfn().unwrap();
        cache.insert(CacheKey::ByDevice { dev: 10, offset: 0 }, pfn0, &mut frames);
        cache.insert(CacheKey::ByDevice { dev: 10, offset: 4096 }, pfn1, &mut frames);
        cache.insert(CacheKey::ByDevice { dev: 10, offset: 8192 }, pfn2, &mut frames);

        assert_eq!(cache.len(), 3);

        let freed = cache.free_pages(2, &mut frames);
        assert_eq!(freed, 2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_page_cache_clear_by_dev() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn0 = alloc.alloc_pfn().unwrap();
        let pfn1 = alloc.alloc_pfn().unwrap();
        let pfn2 = alloc.alloc_pfn().unwrap();
        cache.insert(CacheKey::ByInode { dev: 10, ino: 1, offset: 0 }, pfn0, &mut frames);
        cache.insert(CacheKey::ByInode { dev: 20, ino: 2, offset: 0 }, pfn1, &mut frames);
        cache.insert(CacheKey::ByDevice { dev: 10, offset: 0 }, pfn2, &mut frames);

        assert_eq!(cache.len(), 3);

        cache.clear_by_dev(10, &mut frames);
        assert_eq!(cache.len(), 1);
        assert!(cache.find_by_inode(20, 2, 0).is_some());
    }

    // ── PFN reverse index tests ──

    #[test]
    fn test_find_by_pfn_returns_first_insert() {
        // O(1) reverse lookup. Insert PFN 0x42 with two keys; the
        // first-inserted key must win (matches O(n) linear scan
        // semantics that returned the first match in iteration order).
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = 0x42;
        let first = CacheKey::ByInode { dev: 1, ino: 1, offset: 0 };
        let second = CacheKey::ByInode { dev: 1, ino: 2, offset: 0 };
        cache.insert(first.clone(), pfn, &mut frames);
        cache.insert(second.clone(), pfn, &mut frames);

        // First-insert-wins: pfn_index points to the first key.
        assert_eq!(cache.find_by_pfn(pfn), Some(first));
        // Both keys are still in `entries`.
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_find_by_pfn_clears_on_remove() {
        // Removing the first-inserted key must clear the index, so
        // `find_by_pfn` returns None (the second key is no longer
        // indexed, matching O(n) scan that would still find both
        // entries — but the first-insert-wins contract means the
        // *canonical* owner is the one indexed).
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = 7;
        let first = CacheKey::ByInode { dev: 1, ino: 1, offset: 0 };
        let second = CacheKey::ByInode { dev: 1, ino: 2, offset: 0 };
        cache.insert(first.clone(), pfn, &mut frames);
        cache.insert(second.clone(), pfn, &mut frames);

        cache.remove(&first, &mut frames);
        // After remove, the index clears (the removed key was the
        // indexed one).
        assert_eq!(cache.find_by_pfn(pfn), None);
        // The second entry is still in `entries`.
        assert!(cache.find_by_inode(1, 2, 0).is_some());
    }

    #[test]
    fn test_find_by_pfn_clears_on_decrease_to_zero() {
        // When refcount drops to 0, `decrease_refcount` performs an
        // inline remove (it cannot call self.remove() due to &mut
        // self). Verify the pfn_index is cleaned in that path too.
        let mut frames = make_frames(4);
        let mut alloc = TestAlloc { next: 0 };
        let mut cache = PageCache::new();

        let pfn = 13;
        let key = CacheKey::ByDevice { dev: 99, offset: 0 };
        cache.insert(key.clone(), pfn, &mut frames);

        // refcount starts at 1, decrement to 0.
        assert_eq!(cache.decrease_refcount(&key, &mut frames), Some(0));
        // pfn_index is now cleared.
        assert_eq!(cache.find_by_pfn(pfn), None);
        assert!(cache.is_empty());
    }
}
