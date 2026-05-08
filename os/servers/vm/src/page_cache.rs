//! Page cache for file-backed memory.
//!
//! Caches file pages to avoid repeated VFS requests. Uses a two-level
//! HashMap indexed by (device, inode) -> (page_offset) -> PhysBlock.
//!
//! Corresponds to Minix3's `cache_hash_bydev[]` + `cachepage` linked lists.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::ptr::NonNull;
use minix_types::{PhysBytes, VirBytes};
use crate::region::phys_region::PhysBlock;

pub(crate) type DeviceId = u64;
pub(crate) type InodeNum = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CacheKey {
    pub device: DeviceId,
    pub inode: InodeNum,
}

impl CacheKey {
    pub(crate) fn new(device: DeviceId, inode: InodeNum) -> Self {
        Self { device, inode }
    }
}

#[derive(Debug)]
pub(crate) struct CachedPage {
    pub phys_block: Arc<CachedPhysBlock>,
    pub offset: u64,
}

#[derive(Debug)]
pub(crate) struct CachedPhysBlock {
    pub phys: PhysBytes,
    pub refcount: core::sync::atomic::AtomicU32,
}

impl CachedPhysBlock {
    pub(crate) fn new(phys: PhysBytes) -> Self {
        Self {
            phys,
            refcount: core::sync::atomic::AtomicU32::new(1),
        }
    }

    pub(crate) fn add_ref(&self) {
        self.refcount.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn release_ref(&self) -> u32 {
        self.refcount.fetch_sub(1, core::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn refcount(&self) -> u32 {
        self.refcount.load(core::sync::atomic::Ordering::Relaxed)
    }
}

pub(crate) struct PageCache {
    by_key: BTreeMap<CacheKey, BTreeMap<u64, Arc<CachedPhysBlock>>>,
    total_pages: usize,
}

impl PageCache {
    pub(crate) fn new() -> Self {
        Self {
            by_key: BTreeMap::new(),
            total_pages: 0,
        }
    }

    pub(crate) fn lookup(&self, key: CacheKey, offset: u64) -> Option<Arc<CachedPhysBlock>> {
        self.by_key.get(&key).and_then(|pages| pages.get(&offset).cloned())
    }

    pub(crate) fn insert(
        &mut self,
        key: CacheKey,
        offset: u64,
        phys: PhysBytes,
    ) -> Arc<CachedPhysBlock> {
        let entry = self.by_key.entry(key).or_default();
        if let Some(existing) = entry.get(&offset) {
            existing.add_ref();
            return existing.clone();
        }

        let block = Arc::new(CachedPhysBlock::new(phys));
        entry.insert(offset, block.clone());
        self.total_pages += 1;
        block
    }

    pub(crate) fn remove(&mut self, key: CacheKey, offset: u64) -> Option<Arc<CachedPhysBlock>> {
        let entry = self.by_key.get_mut(&key)?;
        let block = entry.remove(&offset)?;
        self.total_pages = self.total_pages.saturating_sub(1);
        if entry.is_empty() {
            self.by_key.remove(&key);
        }
        Some(block)
    }

    pub(crate) fn total_pages(&self) -> usize {
        self.total_pages
    }

    pub(crate) fn pages_for_file(&self, key: CacheKey) -> usize {
        self.by_key.get(&key).map(|m| m.len()).unwrap_or(0)
    }

    pub(crate) fn invalidate_file(&mut self, key: CacheKey) -> usize {
        let removed = self.by_key.remove(&key)
            .map(|m| m.len())
            .unwrap_or(0);
        self.total_pages = self.total_pages.saturating_sub(removed);
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_cache_insert_lookup() {
        let mut cache = PageCache::new();
        let key = CacheKey::new(1, 100);

        let block = cache.insert(key, 0, PhysBytes(0x1000));
        assert_eq!(block.phys, PhysBytes(0x1000));
        assert_eq!(block.refcount(), 1);

        let found = cache.lookup(key, 0);
        assert!(found.is_some());
        assert_eq!(found.unwrap().phys, PhysBytes(0x1000));
    }

    #[test]
    fn test_page_cache_duplicate_insert() {
        let mut cache = PageCache::new();
        let key = CacheKey::new(1, 100);

        let b1 = cache.insert(key, 0, PhysBytes(0x1000));
        assert_eq!(b1.refcount(), 1);

        let b2 = cache.insert(key, 0, PhysBytes(0x2000));
        assert_eq!(b1.refcount(), 2);
        assert!(Arc::ptr_eq(&b1, &b2));
    }

    #[test]
    fn test_page_cache_remove() {
        let mut cache = PageCache::new();
        let key = CacheKey::new(1, 100);

        cache.insert(key, 0, PhysBytes(0x1000));
        cache.insert(key, 4096, PhysBytes(0x2000));
        assert_eq!(cache.total_pages(), 2);

        let removed = cache.remove(key, 0);
        assert!(removed.is_some());
        assert_eq!(cache.total_pages(), 1);
        assert!(cache.lookup(key, 0).is_none());
        assert!(cache.lookup(key, 4096).is_some());
    }

    #[test]
    fn test_page_cache_invalidate_file() {
        let mut cache = PageCache::new();
        let key1 = CacheKey::new(1, 100);
        let key2 = CacheKey::new(2, 200);

        cache.insert(key1, 0, PhysBytes(0x1000));
        cache.insert(key1, 4096, PhysBytes(0x2000));
        cache.insert(key2, 0, PhysBytes(0x3000));
        assert_eq!(cache.total_pages(), 3);

        let removed = cache.invalidate_file(key1);
        assert_eq!(removed, 2);
        assert_eq!(cache.total_pages(), 1);
        assert!(cache.lookup(key1, 0).is_none());
        assert!(cache.lookup(key2, 0).is_some());
    }

    #[test]
    fn test_page_cache_pages_for_file() {
        let mut cache = PageCache::new();
        let key = CacheKey::new(1, 100);

        assert_eq!(cache.pages_for_file(key), 0);

        cache.insert(key, 0, PhysBytes(0x1000));
        cache.insert(key, 4096, PhysBytes(0x2000));
        assert_eq!(cache.pages_for_file(key), 2);
    }

    #[test]
    fn test_cached_phys_block_refcount() {
        let block = CachedPhysBlock::new(PhysBytes(0x5000));
        assert_eq!(block.refcount(), 1);

        block.add_ref();
        assert_eq!(block.refcount(), 2);

        let prev = block.release_ref();
        assert_eq!(prev, 2);
        assert_eq!(block.refcount(), 1);
    }

    #[test]
    fn test_cache_key_ordering() {
        let k1 = CacheKey::new(1, 100);
        let k2 = CacheKey::new(1, 200);
        let k3 = CacheKey::new(2, 100);

        assert!(k1 < k2);
        assert!(k1 < k3);
        assert!(k2 < k3);
    }
}
