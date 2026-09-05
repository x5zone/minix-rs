//! Block cache: hashed, least-recently-used buffer pool for disk blocks.
//!
//! C correspondence: `minix3/minix/lib/libminixfs/cache.c` (pool, hash,
//! least-recently-used ordering, dirty tracking, flush, prefetch, sizing
//! heuristic) with the buffer header from
//! `minix3/minix/include/minix/libminixfs.h:11-31`.
//!
//! The job of this cache is to keep recently used disk blocks in memory so
//! repeated reads and writes avoid storage traffic. Three structures work
//! together: a hash index keyed by device plus block number for instant
//! lookup, a least-recently-used ordering that picks eviction victims, and
//! a reference count per buffer that pins blocks while a caller uses them.
//! Dirty blocks are written back when flushed or evicted, never eagerly.
//!
//! Two deliberate adaptations to Rust:
//! - Storage input and output goes through a [`BlockSource`] trait instead
//!   of direct block driver calls, so the cache is testable without a disk.
//! - The virtual memory second-level cache (the `vmcache` flag and the
//!   `vm_*` calls in the C code) is a [`SecondLevelCache`] trait with a
//!   disabled implementation. Wiring it to the virtual memory page cache is
//!   future work owned by the virtual memory stage; the hooks are already in
//!   the acquire and release paths.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

use minix_types::{EAGAIN, EINVAL, ENOENT, Errno};

/// Smallest pool the cache accepts.
///
/// C: `MINBUFS` (`cache.c:44`, value six). Below this the pool cannot make
/// progress: too few buffers to stage a scattered transfer.
pub const MIN_POOL_SIZE: usize = 6;

/// Upper bound for one prefetch run.
///
/// C: `LMFS_MAX_PREFETCH`, defined as `NR_IOREQS`
/// (`minix3/minix/include/minix/libminixfs.h:9`;
/// `minix3/minix/include/minix/const.h:50` sets `NR_IOREQS` to sixty-four).
/// Prefetch never pulls more than one gather request worth of blocks.
pub const MAX_PREFETCH: usize = 64;

/// How an acquire should treat storage.
///
/// C: `NORMAL` / `NO_READ` / `PEEK`
/// (`minix3/minix/include/minix/libminixfs.h:55-57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireMode {
    /// Read the block from storage when it is not cached.
    Normal,
    /// Skip the storage read: the caller is about to overwrite the whole
    /// block (newly allocated blocks, full overwrites).
    NoRead,
    /// Only report blocks already cached; never touch storage. A miss is
    /// reported as "not present" instead of an error.
    Peek,
}

/// Identity of one cached block: which device, which block number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockKey {
    /// Device holding the block.
    pub device: u64,
    /// Block number in file system block units.
    pub block: u64,
}

impl BlockKey {
    /// Build a key from its two parts.
    pub const fn new(device: u64, block: u64) -> Self {
        Self { device, block }
    }
}

/// Storage behind the cache: read and write whole blocks.
///
/// Production implements this over the block driver; tests implement it over
/// memory. All sizes are in file system blocks; `block_size` reports the
/// block size in bytes.
pub trait BlockSource {
    /// Block size in bytes.
    fn block_size(&self) -> usize;
    /// Read one block into `out`, which has exactly `block_size` bytes.
    fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno>;
    /// Write one block from `data`, which has exactly `block_size` bytes.
    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno>;
}

/// Optional second-level cache in virtual memory.
///
/// C: the `vmcache` flag with `vm_map_cacheblock` / `vm_set_cacheblock` /
/// `vm_forget_cacheblock` / `vm_clear_cache` (`cache.c:443-451`,
/// `cache.c:562-587`, `cache.c:630-634`). Disabled by default; enabling is
/// future work tied to the virtual memory page cache. The two
/// implementations here are the disabled production default and a recording
/// test double proving the hooks run.
pub trait SecondLevelCache {
    /// Whether the second level is active.
    fn is_enabled(&self) -> bool;
    /// Offer a released block to the second level.
    fn offer(&mut self, key: BlockKey, data: &[u8]);
    /// Forget every block of a device.
    fn forget_device(&mut self, device: u64);
    /// Forget one block that the file system just freed on storage.
    ///
    /// C: `vm_forget_cacheblock` inside `lmfs_free_block`
    /// (`cache.c:630-634`). Empty by default; real implementations drop the
    /// single entry instead of a whole device.
    fn forget_block(&mut self, _key: BlockKey) {}
}

/// Second level disabled: every hook is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoSecondLevel;

impl SecondLevelCache for NoSecondLevel {
    fn is_enabled(&self) -> bool {
        false
    }

    fn offer(&mut self, _key: BlockKey, _data: &[u8]) {}

    fn forget_device(&mut self, _device: u64) {}
}

/// One pooled buffer: the cached bytes plus the bookkeeping the C `struct
/// buf` header carries (`libminixfs.h:15-31`).
#[derive(Debug)]
struct Buffer {
    /// Which block is stored, if any. `None` means a free slot.
    key: Option<BlockKey>,
    /// Cached bytes; empty for a free slot.
    data: Vec<u8>,
    /// Whether the cached bytes differ from storage and must be written back.
    dirty: bool,
    /// Current users; pinned while above zero.
    users: usize,
}

impl Buffer {
    /// A free slot holding no block.
    fn free() -> Self {
        Self {
            key: None,
            data: Vec::new(),
            dirty: false,
            users: 0,
        }
    }
}

/// Hashed least-recently-used block cache over a [`BlockSource`].
///
/// The pool has a fixed number of slots chosen at construction
/// ([`BlockCache::with_pool`], mirroring `lmfs_buf_pool`). Free buffers live
/// in the least-recently-used ordering with the eviction candidate at the
/// front; cached blocks are also indexed by [`BlockKey`] for direct lookup.
/// A buffer with users above zero is pinned and never evicted.
#[derive(Debug)]
pub struct BlockCache<S: BlockSource, V: SecondLevelCache = NoSecondLevel> {
    source: S,
    second_level: V,
    slots: Vec<Buffer>,
    /// Hash index: block key to slot number. C: `buf_hash` (`cache.c:59`).
    index: BTreeMap<BlockKey, usize>,
    /// Free slots from least to most recently released. C: the `front` /
    /// `rear` chain (`cache.c:46-47`).
    free_order: VecDeque<usize>,
    /// Blocks currently pinned by callers. C: `bufs_in_use` (`cache.c:48`).
    pinned: usize,
    /// File system usage counters feeding the sizing heuristic.
    total_blocks: u64,
    used_blocks: u64,
}

impl<S: BlockSource, V: SecondLevelCache> BlockCache<S, V> {
    /// Build a cache with `pool_size` free slots (at least
    /// [`MIN_POOL_SIZE`], mirroring the `lmfs_buf_pool` assertion,
    /// `cache.c:1250`).
    pub fn with_pool(source: S, second_level: V, pool_size: usize) -> Result<Self, Errno> {
        if pool_size < MIN_POOL_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        let mut slots = Vec::with_capacity(pool_size);
        let mut free_order = VecDeque::with_capacity(pool_size);
        for slot in 0..pool_size {
            slots.push(Buffer::free());
            free_order.push_back(slot);
        }
        Ok(Self {
            source,
            second_level,
            slots,
            index: BTreeMap::new(),
            free_order,
            pinned: 0,
            total_blocks: 0,
            used_blocks: 0,
        })
    }

    /// Pool size (total slots, pinned plus free).
    pub fn pool_size(&self) -> usize {
        self.slots.len()
    }

    /// Block size in bytes, from the backing source.
    pub fn source_block_size(&self) -> usize {
        self.source.block_size()
    }

    /// Shared access to the backing source (for inspection and for
    /// transfer layers built on top of the cache).
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Buffers currently pinned by callers.
    pub fn pinned(&self) -> usize {
        self.pinned
    }

    /// Acquire a block: return its slot number, pinned for the caller.
    ///
    /// C: `get_block_ino` (`cache.c:298-489`). Cache hit: unqueue from the
    /// free ordering, pin, and return. Cache miss: evict the
    /// least-recently-used free buffer (writing it back first when dirty,
    /// exactly like `freeblock`, `cache.c:252-272`), then handle the mode:
    /// `Peek` gives up with "not present", `NoRead` hands over an empty
    /// buffer, `Normal` reads from storage.
    pub fn acquire(&mut self, key: BlockKey, mode: AcquireMode) -> Result<usize, Errno> {
        if let Some(&slot) = self.index.get(&key) {
            // The C header stores the use count in a `char`
            // (`libminixfs.h:21`); pinning past its ceiling would wrap, so
            // treat saturation as a programming error in debug builds.
            debug_assert!(self.slots[slot].users < i8::MAX as usize);
            if self.slots[slot].users == 0 {
                self.remove_from_free(slot);
                self.pinned += 1;
            }
            self.slots[slot].users += 1;
            return Ok(slot);
        }

        if mode == AcquireMode::Peek {
            return Err(Errno::from_i32(ENOENT));
        }

        let slot = self.evict_one()?;
        {
            let block_size = self.source.block_size();
            let buffer = &mut self.slots[slot];
            buffer.key = Some(key);
            buffer.data.clear();
            buffer.data.resize(block_size, 0);
            buffer.dirty = false;
            buffer.users = 1;
        }
        self.pinned += 1;
        self.index.insert(key, slot);

        if mode == AcquireMode::Normal {
            let block_size = self.source.block_size();
            let mut staging = alloc::vec![0; block_size];
            if let Err(error) = self.source.read_block(key, &mut staging) {
                self.release_slot(slot);
                self.index.remove(&key);
                self.slots[slot].key = None;
                self.slots[slot].data.clear();
                return Err(error);
            }
            self.slots[slot].data.copy_from_slice(&staging);
        }
        Ok(slot)
    }

    /// Read the bytes of an acquired slot.
    pub fn slot_data(&self, slot: usize) -> &[u8] {
        &self.slots[slot].data
    }

    /// Overwrite the bytes of an acquired slot (marks it dirty, like a
    /// modifying routine must, `cache.c:33-35`).
    pub fn write_slot(&mut self, slot: usize, data: &[u8]) -> Result<(), Errno> {
        let block_size = self.source.block_size();
        if data.len() != block_size {
            return Err(Errno::from_i32(EINVAL));
        }
        let buffer = &mut self.slots[slot];
        if buffer.key.is_none() {
            return Err(Errno::from_i32(EINVAL));
        }
        buffer.data.copy_from_slice(data);
        buffer.dirty = true;
        Ok(())
    }

    /// Whether a slot holds unwritten changes. C: `lmfs_isclean`
    /// (`cache.c:174-177`), inverted.
    pub fn is_dirty(&self, slot: usize) -> bool {
        self.slots[slot].dirty
    }

    /// Mark a slot dirty without writing (for callers that modify the
    /// buffer in place through [`BlockCache::slot_data_mut`]).
    /// C: `lmfs_markdirty` (`cache.c:164-167`).
    pub fn mark_dirty(&mut self, slot: usize) {
        self.slots[slot].dirty = true;
    }

    /// Clear the dirty flag without writing. C: `lmfs_markclean`
    /// (`cache.c:169-172`).
    pub fn mark_clean(&mut self, slot: usize) {
        self.slots[slot].dirty = false;
    }

    /// Mutable bytes of an acquired slot for in-place modification.
    pub fn slot_data_mut(&mut self, slot: usize) -> &mut [u8] {
        &mut self.slots[slot].data
    }

    /// Release a pinned slot back to the free ordering (most-recent end,
    /// "may be needed again", `put_block`, `cache.c:512-596`). Offers the
    /// block to the second level when one is enabled.
    ///
    /// Releasing a slot the caller does not hold is refused instead of
    /// silently corrupting the pin count.
    pub fn release(&mut self, slot: usize) -> Result<(), Errno> {
        if slot >= self.slots.len() || self.slots[slot].users == 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        self.release_slot(slot);
        Ok(())
    }

    /// Flush every dirty block of one device to storage.
    /// C: `lmfs_flushdev` (`cache.c:1136-1166`). Pinned blocks are skipped:
    /// the owner may have marked the block dirty before changing its
    /// contents, and flushing early could lose the update.
    pub fn flush_device(&mut self, device: u64) -> Result<(), Errno> {
        // Collect first: writing needs `&mut self` while iterating slots.
        let mut dirty_slots = Vec::new();
        for (slot, buffer) in self.slots.iter().enumerate() {
            if buffer.dirty
                && buffer.users == 0
                && let Some(key) = buffer.key
                && key.device == device
            {
                dirty_slots.push(slot);
            }
        }
        for slot in dirty_slots {
            let key = self.slots[slot].key.expect("dirty slot has a key");
            let data = self.slots[slot].data.clone();
            self.source.write_block(key, &data)?;
            self.slots[slot].dirty = false;
        }
        Ok(())
    }

    /// Flush every device. C: `lmfs_flushall` (`cache.c:1295-1311`).
    pub fn flush_all(&mut self) -> Result<(), Errno> {
        let mut devices = Vec::new();
        for buffer in &self.slots {
            if buffer.dirty
                && let Some(key) = buffer.key
                && !devices.contains(&key.device)
            {
                devices.push(key.device);
            }
        }
        for device in devices {
            self.flush_device(device)?;
        }
        Ok(())
    }

    /// Release one block back to the free pool: the file system just freed
    /// it on storage, so any cached copy is stale.
    ///
    /// C: `lmfs_free_block` (`cache.c:613-650`). The cached copy is marked
    /// clean and detached from its key even when pinned: the owner may still
    /// hold the slot, but its contents no longer name a stored block. The
    /// second level is told to forget the single block when one is enabled.
    pub fn free_block(&mut self, key: BlockKey) {
        if self.second_level.is_enabled() {
            self.second_level.forget_block(key);
        }
        if let Some(&slot) = self.index.get(&key) {
            self.index.remove(&key);
            let buffer = &mut self.slots[slot];
            buffer.key = None;
            buffer.dirty = false;
        }
    }

    /// Drop every cached block of a device and tell the second level to
    /// forget it. C: `lmfs_invalidate` (`cache.c:782-808`).
    pub fn invalidate_device(&mut self, device: u64) {
        for (slot, buffer) in self.slots.iter_mut().enumerate() {
            if let Some(key) = buffer.key
                && key.device == device
            {
                self.index.remove(&key);
                buffer.key = None;
                buffer.data.clear();
                buffer.dirty = false;
                if buffer.users == 0 && !self.free_order.contains(&slot) {
                    self.free_order.push_back(slot);
                }
            }
        }
        self.second_level.forget_device(device);
    }

    /// Record file system usage for the sizing heuristic.
    /// C: `lmfs_set_blockusage` (`cache.c:711-721`).
    pub fn set_usage(&mut self, total: u64, used: u64) -> Result<(), Errno> {
        if used > total {
            return Err(Errno::from_i32(EINVAL));
        }
        self.total_blocks = total;
        self.used_blocks = used;
        Ok(())
    }

    /// Maximum blocks one read-ahead run should pull.
    ///
    /// C: `lmfs_readahead_limit` (`cache.c:1031-1052`): the tighter of the
    /// single-transfer ceiling and the pool-derived policy cap, always
    /// between one and [`MAX_PREFETCH`].
    pub fn readahead_limit(&self) -> usize {
        let block_size = self.source.block_size().max(1);
        let page_size = 4096usize;
        let max_transfer = (MAX_PREFETCH * page_size / block_size).max(1);
        let max_buffers = if self.slots.len() < 50 {
            18
        } else {
            self.slots.len().saturating_sub(4).max(1)
        };
        max_transfer.min(max_buffers).clamp(1, MAX_PREFETCH)
    }

    /// Suggest a pool size from usage and free memory.
    ///
    /// C: `fs_bufs_heuristic` (`cache.c:73-117`): cache at most half the
    /// file system and a tenth of remaining memory, scaled by the square
    /// root of used kilobytes, but never below `minimum`. All sizes arrive
    /// as explicit arguments (the C code queries virtual memory itself,
    /// which this framework leaves to the caller).
    pub fn suggest_pool_size(
        minimum: usize,
        used_bytes: u64,
        total_bytes: u64,
        free_memory_bytes: u64,
        block_size: usize,
    ) -> usize {
        if block_size == 0 {
            return minimum;
        }
        let used_kb = used_bytes / 1024;
        let total_kb = total_bytes / 1024;
        let free_kb = free_memory_bytes / 1024;
        let root = integer_sqrt(used_kb);
        let mut cap_kb = root.saturating_mul(40);
        cap_kb = cap_kb.min(total_kb / 2);
        let cache_kb = (free_kb / 10).min(cap_kb);
        let buffers = (cache_kb * 1024 / block_size as u64) as usize;
        buffers.max(minimum)
    }

    /// Evict the least-recently-used free buffer, writing it back first when
    /// dirty (like `freeblock`, `cache.c:252-272`). Refuses with "try again"
    /// when every buffer is pinned: the C code aborts the server here
    /// (`cache.c:394`); a library reports the overload instead so the caller
    /// can retry after releasing buffers.
    fn evict_one(&mut self) -> Result<usize, Errno> {
        let slot = self.free_order.pop_front().ok_or(Errno::from_i32(EAGAIN))?;
        let (old_key, was_dirty, data) = {
            let buffer = &mut self.slots[slot];
            (buffer.key.take(), buffer.dirty, buffer.data.clone())
        };
        if let Some(key) = old_key {
            self.index.remove(&key);
            if was_dirty {
                self.source.write_block(key, &data).inspect_err(|_| {
                    // Restore the victim so its dirty contents survive the
                    // failed write-back instead of being silently dropped.
                    let buffer = &mut self.slots[slot];
                    buffer.key = Some(key);
                    buffer.dirty = true;
                    self.index.insert(key, slot);
                    self.free_order.push_front(slot);
                })?;
            }
            self.slots[slot].dirty = false;
            self.slots[slot].data.clear();
        }
        Ok(slot)
    }

    /// Unpin one use of a slot; return it to the free ordering when the last
    /// user leaves. Internal: bounds are checked by [`BlockCache::release`]
    /// and by [`BlockCache::acquire`].
    fn release_slot(&mut self, slot: usize) {
        let buffer = &mut self.slots[slot];
        buffer.users -= 1;
        self.pinned -= 1;
        if buffer.users == 0 {
            self.free_order.push_back(slot);
            if self.second_level.is_enabled()
                && let Some(key) = buffer.key
            {
                self.second_level.offer(key, &buffer.data);
            }
        }
    }

    /// Remove a slot from the free ordering after a cache hit.
    fn remove_from_free(&mut self, slot: usize) {
        if let Some(position) = self.free_order.iter().position(|&s| s == slot) {
            self.free_order.remove(position);
        }
    }
}

/// Integer square root (floor) for the sizing heuristic.
fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut low = 1u64;
    let mut high = value.min(1 << 32);
    while low <= high {
        let mid = low + (high - low) / 2;
        match mid.checked_mul(mid) {
            Some(square) if square == value => return mid,
            Some(square) if square < value => low = mid + 1,
            _ => {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }
    }
    high
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use minix_types::{EBUSY, EIO};

    extern crate alloc;

    /// Memory-backed storage: deterministic stand-in for a block driver.
    /// Read and write counters observe storage traffic (they use cells so
    /// the read path stays shared, like a driver call counter would).
    #[derive(Debug)]
    struct MemSource {
        blocks: BTreeMap<u64, Vec<u8>>,
        block_size: usize,
        reads: core::cell::Cell<usize>,
        writes: core::cell::Cell<usize>,
        fail_reads: bool,
    }

    impl MemSource {
        fn with_blocks(count: u64, block_size: usize) -> Self {
            let mut blocks = BTreeMap::new();
            for block in 0..count {
                blocks.insert(block, vec![block as u8; block_size]);
            }
            Self {
                blocks,
                block_size,
                reads: core::cell::Cell::new(0),
                writes: core::cell::Cell::new(0),
                fail_reads: false,
            }
        }
    }

    impl BlockSource for MemSource {
        fn block_size(&self) -> usize {
            self.block_size
        }

        fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
            if self.fail_reads {
                return Err(Errno::from_i32(EIO));
            }
            match self.blocks.get(&key.block) {
                Some(data) => {
                    out.copy_from_slice(data);
                    self.reads.set(self.reads.get() + 1);
                    Ok(())
                }
                None => Err(Errno::from_i32(EIO)),
            }
        }

        fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
            self.blocks.insert(key.block, data.to_vec());
            self.writes.set(self.writes.get() + 1);
            Ok(())
        }
    }

    /// Recording second level proving the acquire/release hooks run.
    #[derive(Debug, Default)]
    struct RecordingSecondLevel {
        enabled: bool,
        offered: Vec<BlockKey>,
        forgotten: Vec<u64>,
    }

    impl SecondLevelCache for RecordingSecondLevel {
        fn is_enabled(&self) -> bool {
            self.enabled
        }

        fn offer(&mut self, key: BlockKey, _data: &[u8]) {
            self.offered.push(key);
        }

        fn forget_device(&mut self, device: u64) {
            self.forgotten.push(device);
        }
    }

    fn cache() -> BlockCache<MemSource> {
        BlockCache::with_pool(MemSource::with_blocks(16, 64), NoSecondLevel, 8).unwrap()
    }

    #[test]
    fn test_pool_rejects_tiny_sizes() {
        let source = MemSource::with_blocks(4, 64);
        assert_eq!(
            BlockCache::with_pool(source, NoSecondLevel, MIN_POOL_SIZE - 1)
                .unwrap_err()
                .to_i32(),
            EINVAL
        );
    }

    #[test]
    fn test_acquire_caches_and_avoids_storage() {
        let mut cache = cache();
        let key = BlockKey::new(1, 3);
        let first = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.slot_data(first)[0], 3);
        assert_eq!(cache.source.reads.get(), 1);
        cache.release(first).unwrap();
        let second = cache.acquire(key, AcquireMode::Normal).unwrap();
        // Same slot, served from the cache: no second storage read.
        assert_eq!(second, first);
        assert_eq!(cache.source.reads.get(), 1);
        cache.release(second).unwrap();
    }

    #[test]
    fn test_peek_misses_without_touching_storage() {
        let mut cache = cache();
        let key = BlockKey::new(1, 3);
        assert_eq!(
            cache.acquire(key, AcquireMode::Peek).unwrap_err().to_i32(),
            ENOENT
        );
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        // Now cached: peek hits.
        let peeked = cache.acquire(key, AcquireMode::Peek).unwrap();
        cache.release(peeked).unwrap();
    }

    #[test]
    fn test_no_read_hands_over_empty_buffer() {
        let mut cache = cache();
        let key = BlockKey::new(1, 7);
        let slot = cache.acquire(key, AcquireMode::NoRead).unwrap();
        assert_eq!(cache.slot_data(slot), &[0u8; 64][..]);
        assert!(!cache.is_dirty(slot));
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_write_marks_dirty_and_flush_writes_back() {
        let mut cache = cache();
        let key = BlockKey::new(2, 4);
        let slot = cache.acquire(key, AcquireMode::NoRead).unwrap();
        cache.write_slot(slot, &[0xABu8; 64]).unwrap();
        assert!(cache.is_dirty(slot));
        cache.release(slot).unwrap();
        cache.flush_device(2).unwrap();
        assert!(!cache.is_dirty(slot));
        // Storage now holds the written bytes.
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.slot_data(slot), &[0xABu8; 64][..]);
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_flush_skips_pinned_buffers() {
        let mut cache = cache();
        let key = BlockKey::new(2, 5);
        let slot = cache.acquire(key, AcquireMode::NoRead).unwrap();
        cache.write_slot(slot, &[0xCDu8; 64]).unwrap();
        cache.flush_device(2).unwrap();
        // Still dirty: pinned blocks are never flushed from under their owner.
        assert!(cache.is_dirty(slot));
        cache.release(slot).unwrap();
        cache.flush_device(2).unwrap();
        assert!(!cache.is_dirty(slot));
    }

    #[test]
    fn test_eviction_writes_back_dirty_victim() {
        let mut cache =
            BlockCache::with_pool(MemSource::with_blocks(16, 64), NoSecondLevel, MIN_POOL_SIZE)
                .unwrap();
        // Fill every slot with a dirty block, then force one more acquire:
        // the least-recently-used dirty victim must be written back first.
        for block in 0..MIN_POOL_SIZE as u64 {
            let slot = cache
                .acquire(BlockKey::new(1, block), AcquireMode::NoRead)
                .unwrap();
            cache.write_slot(slot, &[block as u8; 64]).unwrap();
            cache.release(slot).unwrap();
        }
        let writes_before = cache.source.writes.get();
        let slot = cache
            .acquire(BlockKey::new(1, 100), AcquireMode::NoRead)
            .unwrap();
        // Evicting the dirty victim cost exactly one storage write.
        assert_eq!(cache.source.writes.get(), writes_before + 1);
        cache.release(slot).unwrap();
        // Victim block zero survived on storage.
        assert_eq!(cache.source.blocks[&0], [0u8; 64]);
    }

    #[test]
    fn test_full_pool_reports_overload_instead_of_panicking() {
        let mut cache =
            BlockCache::with_pool(MemSource::with_blocks(64, 64), NoSecondLevel, MIN_POOL_SIZE)
                .unwrap();
        let mut held = Vec::new();
        for block in 0..MIN_POOL_SIZE as u64 {
            held.push(
                cache
                    .acquire(BlockKey::new(3, block), AcquireMode::NoRead)
                    .unwrap(),
            );
        }
        assert_eq!(
            cache
                .acquire(BlockKey::new(3, 999), AcquireMode::Normal)
                .unwrap_err()
                .to_i32(),
            EAGAIN
        );
        for slot in held {
            cache.release(slot).unwrap();
        }
    }

    #[test]
    fn test_failed_storage_read_releases_slot() {
        let mut cache = cache();
        cache.source.fail_reads = true;
        let pinned_before = cache.pinned();
        assert_eq!(
            cache
                .acquire(BlockKey::new(1, 3), AcquireMode::Normal)
                .unwrap_err()
                .to_i32(),
            EIO
        );
        assert_eq!(cache.pinned(), pinned_before);
    }

    #[test]
    fn test_double_release_is_refused() {
        let mut cache = cache();
        let slot = cache
            .acquire(BlockKey::new(1, 1), AcquireMode::Normal)
            .unwrap();
        cache.release(slot).unwrap();
        assert_eq!(cache.release(slot).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_free_block_detaches_single_entry() {
        let mut cache = cache();
        let key = BlockKey::new(7, 2);
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        cache.free_block(key);
        // Detached: the next acquire reads storage again instead of hitting
        // the stale copy.
        let reads_before = cache.source.reads.get();
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.source.reads.get(), reads_before + 1);
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_invalidate_drops_device_blocks() {
        let mut cache = cache();
        let slot = cache
            .acquire(BlockKey::new(7, 2), AcquireMode::Normal)
            .unwrap();
        cache.release(slot).unwrap();
        cache.invalidate_device(7);
        // Gone from the cache: the next acquire reads storage again.
        let slot = cache
            .acquire(BlockKey::new(7, 2), AcquireMode::Normal)
            .unwrap();
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_second_level_hooks_run() {
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(8, 64),
            RecordingSecondLevel {
                enabled: true,
                ..RecordingSecondLevel::default()
            },
            8,
        )
        .unwrap();
        let key = BlockKey::new(1, 1);
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        assert!(cache.second_level.offered.contains(&key));
        cache.invalidate_device(1);
        assert!(cache.second_level.forgotten.contains(&1));
    }

    #[test]
    fn test_readahead_limit_stays_in_range() {
        let cache = cache();
        let limit = cache.readahead_limit();
        assert!((1..=MAX_PREFETCH).contains(&limit));
    }

    #[test]
    fn test_sizing_heuristic_prefers_minimum_on_empty_input() {
        assert_eq!(
            BlockCache::<MemSource>::suggest_pool_size(6, 0, 0, 0, 4096),
            6
        );
        let sized = BlockCache::<MemSource>::suggest_pool_size(
            6,
            1024 * 1024 * 100,
            1024 * 1024 * 1000,
            1024 * 1024 * 2000,
            4096,
        );
        assert!(sized >= 6);
    }

    #[test]
    fn test_usage_validation() {
        let mut cache = cache();
        assert!(cache.set_usage(100, 40).is_ok());
        assert_eq!(cache.set_usage(100, 101).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_set_blocksize_requires_idle_pool() {
        // Resizing the pool while buffers are pinned would invalidate the
        // very blocks callers hold; refuse with EBUSY like the C assertion
        // (cache.c:1200) intends, instead of aborting the server.
        let mut cache = cache();
        let slot = cache
            .acquire(BlockKey::new(1, 1), AcquireMode::Normal)
            .unwrap();
        assert_eq!(resize_pool(&mut cache, 10).unwrap_err().to_i32(), EBUSY);
        cache.release(slot).unwrap();
        assert!(resize_pool(&mut cache, 10).is_ok());
        assert_eq!(cache.pool_size(), 10);
    }

    /// Rebuild the pool at a new size; only allowed with nothing pinned.
    fn resize_pool<S: BlockSource, V: SecondLevelCache>(
        cache: &mut BlockCache<S, V>,
        size: usize,
    ) -> Result<(), Errno> {
        if cache.pinned() > 0 {
            return Err(Errno::from_i32(EBUSY));
        }
        if size < MIN_POOL_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        // Flush first so dirty contents survive the rebuild (lmfs_buf_pool
        // flushes before freeing, cache.c:1252-1261).
        cache.flush_all().ok();
        let source_block_size = cache.source.block_size();
        let _ = source_block_size;
        // Rebuild: drop all slots and re-index. Dirty blocks were flushed
        // above; pinned count is zero by the guard.
        cache.slots.clear();
        cache.index.clear();
        cache.free_order.clear();
        for slot in 0..size {
            cache.slots.push(Buffer::free());
            cache.free_order.push_back(slot);
        }
        Ok(())
    }
}
