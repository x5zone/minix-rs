//! Page cache implementation — the VM-side cache directory for disk blocks.
//!
//! Corresponds to Minix3 `cache.c` + `cache.h` (the data structure side) and
//! the `do_mapcache`/`do_setcache`/`do_forgetcache`/`do_clearcache` handlers
//! in `mem_cache.c` (the IPC side lives in `ipc/dispatcher.rs`).
//!
//! # Model: one entry per (dev, dev_offset), optional inode index
//!
//! Minix3 keeps **exactly one** `cached_page` per `(dev, dev_offset)` pair
//! (the bydev hash is the primary identity, `cache.h:2-21`); `(ino,
//! ino_offset)` is *metadata on the same node*, may be missing
//! (`VMC_NO_INODE` = 0), and is kept in a secondary byino hash. This
//! implementation mirrors that with a primary `BTreeMap<(dev, dev_offset),
//! CachedPage>` plus a secondary index `(dev, ino, ino_offset) → (dev,
//! dev_offset)` (`[ARCH: A-4]` family: chained hash → BTreeMap, same choice
//! as 14-region-lookup).
//!
//! # Reference counting
//!
//! The cache does not keep its own per-entry refcount. The frame refcount in
//! [`PageFrames`] is authoritative: `addcache` bumps it once (the cache's
//! reference, `PBF_INCACHE`, C: cache.c:243-244), and each mapping via
//! `VirRegion::map_page` bumps it again. Eviction (`free_pages`) only drops
//! entries whose frame refcount is 1 — i.e. referenced by the cache alone,
//! matching C's `cache_freepages` (`refcount == 1`, cache.c:288-305).
//!
//! # LRU
//!
//! Minix3 keeps a precise doubly-linked LRU (`lru_add`/`lru_rm`/
//! `cache_lru_touch`, cache.c:29-75). This implementation uses an
//! index-based doubly-linked list over a `Vec` arena with a free list:
//! O(1) push/touch/remove in safe Rust, no intrusive pointers. Precise LRU
//! order is externally observable (under memory pressure the *oldest*
//! unmapped page is evicted first), so it must be preserved.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::region::{PageFrames, PfnAllocator};

/// Sentinel inode value for device files without an associated inode.
/// C: `#define VMC_NO_INODE 0` (minix3/minix/include/minix/vm.h:90).
pub(crate) const VMC_NO_INODE: u64 = 0;

/// One-shot block flag. C: `#define VMSF_ONCE 0x01` (vm.h:93).
/// The FS marks a block as usable once: `do_mapcache` refuses it
/// (mem_cache.c:149), file-mapping faults force a VFS round-trip
/// (mem_file.c:120-131).
pub(crate) const VMSF_ONCE: u32 = 0x01;

/// A read-only view of a cached page, returned by the lookup methods.
///
/// Returned by value (Copy) so callers never hold a borrow into
/// `PageCache` while mutating it (e.g. `find_by_dev` performs the lazy
/// inode update and LRU touch as side effects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CachedPageRef {
    /// Physical frame number backing this cache block.
    pub(crate) pfn: u32,
    /// `true` if the block is marked one-shot (`VMSF_ONCE`).
    pub(crate) once: bool,
}

/// Cache operation errors, mapped to Minix3 errno values by the IPC layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheError {
    /// Block already in cache. C: `addcache` returns `EINVAL` when
    /// `pb->flags & PBF_INCACHE` (cache.c:222-226).
    AlreadyCached,
    /// Invalid parameter (e.g. `dev == NO_DEV`). C: `assert(dev != NO_DEV)`
    /// (cache.c:230) — Rust fails closed instead of panicking.
    InvalidParam,
}

/// One cache entry. Identity = primary key `(dev, dev_offset)` in `by_dev`.
#[derive(Debug)]
pub(crate) struct CachedPage {
    /// Inode metadata, `None` == `VMC_NO_INODE` (unknown). Kept in the
    /// secondary `by_ino` index when `Some`.
    pub(crate) ino: Option<u64>,
    /// Offset within the inode. Meaningful only when `ino.is_some()`.
    pub(crate) ino_offset: u64,
    /// One-shot flag (`VMSF_ONCE`). C: `hb->flags = flags & VMSF_ONCE`
    /// (cache.c:241).
    pub(crate) once: bool,
    /// Physical frame number backing this block.
    pub(crate) pfn: u32,
    /// Index of this entry's node in the LRU arena (O(1) touch/remove).
    lru_node: u32,
}

/// Index-based doubly-linked LRU list.
///
/// Nodes live in a `Vec` arena; removed nodes are recycled through `free`
/// so the arena size tracks the *peak* cache size, not the churn (the same
/// trick as a slab free list; safe, no pointers).
struct LruNode {
    key: (u64, u64),
    prev: Option<u32>,
    next: Option<u32>,
}

struct LruList {
    nodes: Vec<LruNode>,
    free: Vec<u32>,
    /// Oldest end (eviction starts here).
    head: Option<u32>,
    /// Newest end (`push_back`/`touch` attach here).
    tail: Option<u32>,
}

impl LruList {
    fn new() -> Self {
        Self { nodes: Vec::new(), free: Vec::new(), head: None, tail: None }
    }

    fn alloc_node(&mut self, key: (u64, u64)) -> u32 {
        if let Some(idx) = self.free.pop() {
            self.nodes[idx as usize] = LruNode { key, prev: None, next: None };
            idx
        } else {
            self.nodes.push(LruNode { key, prev: None, next: None });
            (self.nodes.len() - 1) as u32
        }
    }

    /// Append `key` at the newest end. Returns the node index.
    fn push_back(&mut self, key: (u64, u64)) -> u32 {
        let idx = self.alloc_node(key);
        self.link_tail(idx);
        idx
    }

    /// Unlink `idx` from the list, then re-attach it at the newest end.
    /// C: `cache_lru_touch` = `lru_rm` + `lru_add` (cache.c:71-75).
    fn touch(&mut self, idx: u32) {
        self.unlink(idx);
        self.link_tail(idx);
    }

    /// Unlink `idx` and recycle its arena slot.
    fn remove(&mut self, idx: u32) {
        self.unlink(idx);
        self.free.push(idx);
    }

    /// Attach the node at `idx` to the newest end. The node must be
    /// currently unlinked.
    fn link_tail(&mut self, idx: u32) {
        let prev = self.tail;
        if let Some(t) = prev {
            self.nodes[t as usize].next = Some(idx);
        } else {
            self.head = Some(idx);
        }
        self.nodes[idx as usize].prev = prev;
        self.nodes[idx as usize].next = None;
        self.tail = Some(idx);
    }

    /// Detach the node at `idx` from the list.
    fn unlink(&mut self, idx: u32) {
        let (prev, next) = {
            let node = &self.nodes[idx as usize];
            (node.prev, node.next)
        };
        if let Some(p) = prev {
            self.nodes[p as usize].next = next;
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            self.nodes[n as usize].prev = prev;
        } else {
            self.tail = prev;
        }
    }

    /// Iterate from the oldest end to the newest (eviction order).
    fn iter_oldest(&self) -> LruIter<'_> {
        LruIter { list: self, cur: self.head }
    }
}

struct LruIter<'a> {
    list: &'a LruList,
    cur: Option<u32>,
}

impl Iterator for LruIter<'_> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.cur?;
        let node = &self.list.nodes[idx as usize];
        self.cur = node.next;
        Some(node.key)
    }
}

/// The VM-side page cache: a directory of cached disk blocks.
pub(crate) struct PageCache {
    /// Primary index: `(dev, dev_offset)` → entry. Exactly one entry per
    /// cached block (C: `cache_hash_bydev`, cache.c:22).
    by_dev: BTreeMap<(u64, u64), CachedPage>,
    /// Secondary index: `(dev, ino, ino_offset)` → primary key. Only
    /// entries with known inode info appear here (C: `cache_hash_byino`,
    /// cache.c:23). The `dev` component is part of the effective lookup key
    /// (C: `find_cached_page_byino` verifies `hb->dev == dev`, cache.c:209).
    by_ino: BTreeMap<(u64, u64, u64), (u64, u64)>,
    /// Precise LRU, oldest first (C: `lru_oldest`/`lru_newest`, cache.c:25).
    lru: LruList,
    /// Number of cached pages. C: `cached_pages` (cache.c:27), reported by
    /// `get_stats_info` (cache.c:328-331).
    total_cached: u64,
}

impl PageCache {
    pub(crate) fn new() -> Self {
        Self {
            by_dev: BTreeMap::new(),
            by_ino: BTreeMap::new(),
            lru: LruList::new(),
            total_cached: 0,
        }
    }

    /// Register a block as cached. C: `addcache` (cache.c:217-262).
    ///
    /// Fails with `AlreadyCached` when the physical frame is already in the
    /// cache (`PBF_INCACHE`, cache.c:222-226) — one block has exactly one
    /// cache entry.
    #[allow(clippy::too_many_arguments)] // V10-P2-1 (DEFERRED): fold into an AddCacheCtx struct
    pub(crate) fn addcache(
        &mut self,
        dev: u64,
        dev_off: u64,
        ino: Option<u64>,
        ino_off: u64,
        once: bool,
        pfn: u32,
        frames: &mut PageFrames,
    ) -> Result<(), CacheError> {
        // C: assert(dev != NO_DEV) (cache.c:230); NO_DEV == 0.
        if dev == VMC_NO_INODE {
            return Err(CacheError::InvalidParam);
        }
        // C: if(pb->flags & PBF_INCACHE) return EINVAL (cache.c:222-226).
        // One block has exactly one cache entry: a duplicate key is a caller
        // bug (C's chained bydev hash would silently hold both nodes and
        // `find_cached_page_bydev` returns the newest — ill-defined; Rust
        // fails closed instead).
        if frames.get(pfn).map(|s| s.is_cached()).unwrap_or(false)
            || self.by_dev.contains_key(&(dev, dev_off))
        {
            return Err(CacheError::AlreadyCached);
        }

        // Cache reference: bump frame refcount + set IN_CACHE
        // (C: hb->page->refcount++; hb->page->flags |= PBF_INCACHE, cache.c:243-244).
        frames.addcache(pfn);

        let lru_node = self.lru.push_back((dev, dev_off));
        self.by_dev.insert(
            (dev, dev_off),
            CachedPage { ino, ino_offset: ino_off, once, pfn, lru_node },
        );
        if let Some(ino) = ino {
            self.by_ino.insert((dev, ino, ino_off), (dev, dev_off));
        }
        self.total_cached += 1;
        Ok(())
    }

    /// Remove a block from the cache. C: `rmcache` (cache.c:264-286).
    ///
    /// Drops the cache reference (`frames.rmcache`), and when the frame's
    /// refcount reaches 0 (no address space maps it anymore) returns the
    /// physical page to the allocator — C: `free_mem(ABS2CLICK(pb->phys),
    /// 1)` (cache.c:280-284).
    ///
    /// Returns `true` if an entry was removed.
    pub(crate) fn rmcache(
        &mut self,
        dev: u64,
        dev_off: u64,
        frames: &mut PageFrames,
        alloc: &mut dyn PfnAllocator,
    ) -> bool {
        let Some(entry) = self.by_dev.remove(&(dev, dev_off)) else {
            return false;
        };
        if let Some(ino) = entry.ino {
            self.by_ino.remove(&(dev, ino, entry.ino_offset));
        }
        self.lru.remove(entry.lru_node);
        frames.rmcache(entry.pfn);
        self.total_cached = self.total_cached.saturating_sub(1);

        // C: if(pb->refcount == 0) free_mem(...) — the cache was the last
        // reference, so the page is no longer mapped anywhere.
        if frames.get(entry.pfn).map(|s| s.refcount() == 0).unwrap_or(false) {
            alloc.free_pfn(entry.pfn);
        }
        true
    }

    /// Look up a block by its device address. C: `find_cached_page_bydev`
    /// (cache.c:177-195).
    ///
    /// When the caller supplies inode info (`ino: Some`) that differs from
    /// the entry's, the entry's inode metadata is updated and re-indexed —
    /// C: `update_inohash` (cache.c:165-175). This is the bridge between the
    /// two-phase registration (FS first registers by disk block address,
    /// file mappings later look up by inode offset).
    ///
    /// When `touch` is true, the entry is moved to the newest LRU end
    /// (C: `if(touchlru) cache_lru_touch(hb)`, cache.c:189).
    pub(crate) fn find_by_dev(
        &mut self,
        dev: u64,
        dev_off: u64,
        ino: Option<u64>,
        ino_off: u64,
        touch: bool,
    ) -> Option<CachedPageRef> {
        let entry = self.by_dev.get_mut(&(dev, dev_off))?;

        // C: if(ino != VMC_NO_INODE) { if(hb->ino != ino || hb->ino_offset
        // != ino_off) update_inohash(hb, ino, ino_off); } (cache.c:183-188).
        if let Some(ino) = ino
            && (entry.ino != Some(ino) || entry.ino_offset != ino_off) {
                if let Some(old_ino) = entry.ino {
                    self.by_ino.remove(&(dev, old_ino, entry.ino_offset));
                }
                entry.ino = Some(ino);
                entry.ino_offset = ino_off;
                self.by_ino.insert((dev, ino, ino_off), (dev, dev_off));
            }

        let lru_node = entry.lru_node;
        if touch {
            self.lru.touch(lru_node);
        }
        Some(CachedPageRef { pfn: entry.pfn, once: entry.once })
    }

    /// Look up a block by inode offset. C: `find_cached_page_byino`
    /// (cache.c:198-215).
    ///
    /// The lookup key is `(dev, ino, ino_offset)` — the `dev` component
    /// distinguishes equal inode numbers on different devices (C verifies
    /// `hb->dev == dev` inside the chain, cache.c:209).
    pub(crate) fn find_by_ino(
        &mut self,
        dev: u64,
        ino: u64,
        ino_off: u64,
        touch: bool,
    ) -> Option<CachedPageRef> {
        let key = *self.by_ino.get(&(dev, ino, ino_off))?;
        let entry = self.by_dev.get_mut(&key)?;
        let lru_node = entry.lru_node;
        if touch {
            self.lru.touch(lru_node);
        }
        Some(CachedPageRef { pfn: entry.pfn, once: entry.once })
    }

    /// Evict cached pages under memory pressure. C: `cache_freepages`
    /// (cache.c:288-305).
    ///
    /// Walks the LRU from the oldest end, dropping entries whose frame is
    /// referenced only by the cache (`refcount == 1` — no address space has
    /// them mapped). Returns the number of pages freed.
    pub(crate) fn free_pages(
        &mut self,
        needed: usize,
        frames: &mut PageFrames,
        alloc: &mut dyn PfnAllocator,
    ) -> usize {
        let mut freed = 0;
        let mut evict: Vec<(u64, u64)> = Vec::new();
        for key in self.lru.iter_oldest() {
            if freed >= needed {
                break;
            }
            let evictable = self
                .by_dev
                .get(&key)
                .and_then(|e| frames.get(e.pfn))
                .map(|s| s.refcount() == 1)
                .unwrap_or(false);
            if evictable {
                evict.push(key);
                freed += 1;
            }
        }
        for (dev, dev_off) in evict {
            self.rmcache(dev, dev_off, frames, alloc);
        }
        freed
    }

    /// Remove every cached block of a device. C: `clear_cache_bydev`
    /// (cache.c:308-325), driven by `do_clearcache` (unmount).
    pub(crate) fn clear_by_dev(
        &mut self,
        dev: u64,
        frames: &mut PageFrames,
        alloc: &mut dyn PfnAllocator,
    ) {
        let keys: Vec<(u64, u64)> =
            self.by_dev.keys().filter(|(d, _)| *d == dev).copied().collect();
        for (d, dev_off) in keys {
            self.rmcache(d, dev_off, frames, alloc);
        }
    }

    /// Number of cached pages. C: `get_stats_info` → `vsi_cached`
    /// (cache.c:328-331).
    pub(crate) fn total_cached(&self) -> u64 {
        self.total_cached
    }

    /// Number of cache entries (diagnostics/tests).
    // V10-P2-1: test-only (the dispatcher queries by dev/offset directly).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn len(&self) -> usize {
        self.by_dev.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_empty(&self) -> bool {
        self.by_dev.is_empty()
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
    use alloc::vec;
    use crate::region::{PfnAllocError, PAGE_SIZE};
    use minix_types::PhysBytes;

    /// Test allocator that hands out sequential PFNs and records frees.
    struct TestAlloc {
        next: u32,
        freed: Vec<u32>,
    }

    impl TestAlloc {
        fn new() -> Self {
            Self { next: 0, freed: Vec::new() }
        }
    }

    impl PfnAllocator for TestAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            let pfn = self.next;
            self.next += 1;
            Ok(pfn)
        }
        fn free_pfn(&mut self, pfn: u32) {
            self.freed.push(pfn);
        }
    }

    fn make_frames(pages: u32) -> PageFrames {
        PageFrames::new(PhysBytes(pages as u64 * PAGE_SIZE))
    }

    #[test]
    fn test_addcache_and_rmcache() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        cache.addcache(1, 0, Some(10), 0, false, 0, &mut frames).unwrap();
        assert_eq!(cache.total_cached(), 1);
        assert_eq!(cache.len(), 1);
        // Cache reference bumps the frame refcount (C: cache.c:243-244).
        assert_eq!(frames.get(0).unwrap().refcount(), 1);
        assert!(frames.get(0).unwrap().is_cached());

        assert!(cache.rmcache(1, 0, &mut frames, &mut alloc));
        assert!(cache.is_empty());
        assert_eq!(cache.total_cached(), 0);
        // No mapping left → page returned to the allocator (C: cache.c:280-284).
        assert_eq!(alloc.freed, vec![0]);
    }

    #[test]
    fn test_addcache_rejects_duplicate_block() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        cache.addcache(1, 0, None, 0, false, 0, &mut frames).unwrap();
        // Same frame, different key → still rejected (PBF_INCACHE is per-page).
        assert_eq!(
            cache.addcache(2, 0, None, 0, false, 0, &mut frames),
            Err(CacheError::AlreadyCached)
        );
        // Same key, different frame → also rejected (one entry per block).
        assert_eq!(
            cache.addcache(1, 0, None, 0, false, 1, &mut frames),
            Err(CacheError::AlreadyCached)
        );
        assert_eq!(cache.len(), 1);
        assert_eq!(frames.get(0).unwrap().refcount(), 1);
        assert_eq!(frames.get(1).unwrap().refcount(), 0);
        cache.clear_by_dev(1, &mut frames, &mut alloc);
    }

    #[test]
    fn test_addcache_rejects_no_device() {
        let mut frames = make_frames(4);
        let mut cache = PageCache::new();
        // C: assert(dev != NO_DEV) (cache.c:230); NO_DEV == 0.
        assert_eq!(
            cache.addcache(0, 0, None, 0, false, 0, &mut frames),
            Err(CacheError::InvalidParam)
        );
    }

    #[test]
    fn test_find_by_dev_and_ino() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        // Registered by device address only (FS raw block write path).
        cache.addcache(1, 0x1000, None, 0, false, 3, &mut frames).unwrap();

        // find_by_dev with inode info lazily updates the ino index
        // (C: update_inohash via find_cached_page_bydev, cache.c:183-188).
        let hit = cache.find_by_dev(1, 0x1000, Some(7), 0x2000, false).unwrap();
        assert_eq!(hit, CachedPageRef { pfn: 3, once: false });
        assert_eq!(hit.pfn, 3);

        // Now the entry is findable by inode offset too.
        let by_ino = cache.find_by_ino(1, 7, 0x2000, false).unwrap();
        assert_eq!(by_ino.pfn, 3);

        // Same inode number on a different device must NOT match.
        assert!(cache.find_by_ino(2, 7, 0x2000, false).is_none());

        cache.clear_by_dev(1, &mut frames, &mut alloc);
        assert_eq!(alloc.freed, vec![3]);
    }

    #[test]
    fn test_find_by_dev_updates_stale_ino() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        // Registered with ino 10@0x1000, then the block is re-filed under
        // ino 20@0x3000 by a later lookup (C update_inohash moves buckets).
        cache.addcache(1, 0x1000, Some(10), 0x1000, false, 3, &mut frames).unwrap();
        let hit = cache.find_by_dev(1, 0x1000, Some(20), 0x3000, false).unwrap();
        assert_eq!(hit.pfn, 3);

        assert!(cache.find_by_ino(1, 10, 0x1000, false).is_none());
        assert!(cache.find_by_ino(1, 20, 0x3000, false).is_some());

        // Passing VMC_NO_INODE (None) never clears the ino info (C: the
        // update only runs for real inode numbers, cache.c:183-184).
        let hit = cache.find_by_dev(1, 0x1000, None, 0, false).unwrap();
        assert_eq!(hit.pfn, 3);
        assert!(cache.find_by_ino(1, 20, 0x3000, false).is_some());

        cache.clear_by_dev(1, &mut frames, &mut alloc);
    }

    #[test]
    fn test_lru_touch_orders_eviction() {
        let mut frames = make_frames(16);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        // Insert three blocks: LRU order oldest→newest = A, B, C.
        cache.addcache(1, 0x0000, None, 0, false, 0, &mut frames).unwrap();
        cache.addcache(1, 0x1000, None, 0, false, 1, &mut frames).unwrap();
        cache.addcache(1, 0x2000, None, 0, false, 2, &mut frames).unwrap();

        // Touch A → order becomes B, C, A; evicting 1 must drop B.
        cache.find_by_dev(1, 0x0000, None, 0, true).unwrap();
        assert_eq!(cache.free_pages(1, &mut frames, &mut alloc), 1);
        assert!(cache.find_by_dev(1, 0x1000, None, 0, false).is_none());
        assert!(cache.find_by_dev(1, 0x0000, None, 0, false).is_some());
        assert!(cache.find_by_dev(1, 0x2000, None, 0, false).is_some());

        cache.clear_by_dev(1, &mut frames, &mut alloc);
    }

    #[test]
    fn test_free_pages_skips_mapped_frames() {
        let mut frames = make_frames(16);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        cache.addcache(1, 0x0000, None, 0, false, 0, &mut frames).unwrap();
        cache.addcache(1, 0x1000, None, 0, false, 1, &mut frames).unwrap();

        // Frame 0 is mapped by some address space (extra reference) —
        // C: refcount != 1 → skip (cache.c:296-302).
        frames.get_mut(0).unwrap().refcount += 1;

        assert_eq!(cache.free_pages(2, &mut frames, &mut alloc), 1);
        assert!(cache.find_by_dev(1, 0x0000, None, 0, false).is_some());
        assert!(cache.find_by_dev(1, 0x1000, None, 0, false).is_none());
        // Frame 1 had only the cache reference → freed back to the allocator.
        // Frame 0 is still mapped, so nothing else was freed.
        assert_eq!(alloc.freed, vec![1]);

        // Unmap frame 0 → now evictable; rmcache drops the cache ref and frees.
        frames.get_mut(0).unwrap().refcount -= 1;
        assert_eq!(cache.free_pages(1, &mut frames, &mut alloc), 1);
        assert_eq!(alloc.freed, vec![1, 0]);
    }

    #[test]
    fn test_clear_by_dev() {
        let mut frames = make_frames(16);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        cache.addcache(1, 0x0000, Some(1), 0, false, 0, &mut frames).unwrap();
        cache.addcache(1, 0x1000, None, 0, false, 1, &mut frames).unwrap();
        cache.addcache(2, 0x0000, Some(2), 0, false, 2, &mut frames).unwrap();

        cache.clear_by_dev(1, &mut frames, &mut alloc);
        assert_eq!(cache.len(), 1);
        assert!(cache.find_by_ino(2, 2, 0, false).is_some());
        assert_eq!(cache.total_cached(), 1);
        // Both device-1 frames freed (no mappings).
        assert_eq!(alloc.freed, vec![0, 1]);

        cache.clear_by_dev(2, &mut frames, &mut alloc);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_rmcache_keeps_mapped_page_alive() {
        let mut frames = make_frames(8);
        let mut alloc = TestAlloc::new();
        let mut cache = PageCache::new();

        cache.addcache(1, 0, None, 0, false, 4, &mut frames).unwrap();
        // A mapping references the frame (refcount 1→2).
        frames.get_mut(4).unwrap().refcount += 1;

        cache.rmcache(1, 0, &mut frames, &mut alloc);
        assert!(cache.is_empty());
        // Frame still referenced by the mapping — NOT freed.
        assert!(alloc.freed.is_empty());
        assert_eq!(frames.get(4).unwrap().refcount(), 1);
        assert!(!frames.get(4).unwrap().is_cached());
    }

    #[test]
    fn test_find_miss() {
        let mut frames = make_frames(8);
        let mut cache = PageCache::new();
        assert!(cache.find_by_dev(1, 0, None, 0, false).is_none());
        assert!(cache.find_by_ino(1, 1, 0, false).is_none());
    }
}
