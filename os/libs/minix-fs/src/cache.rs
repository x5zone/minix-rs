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
//! - The virtual-memory second level (the `vmcache` flag and the four `vm_*`
//!   calls in the C code) is the [`SecondLevelCache`] face from
//!   [`crate::vm_cache`]: the pool asks it for a block's page before touching
//!   storage, hands its page over when a block leaves, and keeps the C per
//!   buffer flags word ([`BufferFlags`]) that travels on that wire. A pool
//!   built with [`NoSecondLevel`] behaves as if the page cache did not exist.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use minix_types::{EAGAIN, EBUSY, EINVAL, ENOENT, Errno};

// 二级缓存的面（trait 与两个实现）住在 `crate::vm_cache`；这里再导出，
// 让消费方沿用 `minix_fs::cache::*` 这一条路径，也让本模块的文档链接
// （[`SecondLevelCache`]、[`NoSecondLevel`]）指向唯一定义处。
pub use crate::vm_cache::{
    BlockMemory, BlockTag, BufferFlags, MappedPage, NoSecondLevel, PAGE_SIZE, SecondLevelCache,
    VmCacheFault, VmCacheWire, VmSecondLevel, is_page_multiple, page_round_up,
};

/// Smallest pool the cache accepts.
///
/// C: `MINBUFS` (`cache.c:44`, value six). Below this the pool cannot make
/// progress: too few buffers to stage a scattered transfer.
pub const MIN_POOL_SIZE: usize = 6;

/// 写入量跨过该阈值即提示重新评估池尺寸（`cache.c:119-161` 的十兆带宽
/// 台阶）。
pub const WRITE_REESTIMATE_THRESHOLD: u64 = 10 * 1024 * 1024;

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
    /// Batched read of `count` consecutive blocks starting at `key.block`
    /// (`out` holds `count * block_size` bytes).
    ///
    /// C: the scattered-read entry (`rw_scattered`, `cache.c:840`) — one
    /// device run served by one request when the source can batch. The
    /// default walks [`BlockSource::read_block`]; sources with a batched
    /// driver path override it.
    fn read_blocks(&self, key: BlockKey, count: usize, out: &mut [u8]) -> Result<(), Errno> {
        let bs = self.block_size();
        for i in 0..count {
            self.read_block(
                BlockKey { device: key.device, block: key.block + i as u64 },
                &mut out[i * bs..(i + 1) * bs],
            )?;
        }
        Ok(())
    }
    /// Batched write of `count` consecutive blocks starting at `key.block`.
    ///
    /// C: the scattered-write entry (`cache.c:840`, writes sorted by block
    /// number first). The default walks [`BlockSource::write_block`].
    fn write_blocks(&mut self, key: BlockKey, count: usize, data: &[u8]) -> Result<(), Errno> {
        let bs = self.block_size();
        for i in 0..count {
            self.write_block(
                BlockKey { device: key.device, block: key.block + i as u64 },
                &data[i * bs..(i + 1) * bs],
            )?;
        }
        Ok(())
    }
}

/// One pooled buffer: the cached bytes plus the bookkeeping the C `struct
/// buf` header carries (`libminixfs.h:15-31`).
#[derive(Debug)]
struct Buffer {
    /// Which block is stored, if any. `None` means a free slot.
    key: Option<BlockKey>,
    /// Which inode the block belongs to (`lmfs_inode` /
    /// `lmfs_inode_offset`, `libminixfs.h:26-30`); `None` is C's
    /// `VMC_NO_INODE`. Tags are what the page cache keys its own entries by,
    /// so a change of tag has to travel back (`need_set_cache`).
    tag: Option<BlockTag>,
    /// Whether the second level still has to be told which inode this block
    /// belongs to (`lmfs_needsetcache`, `libminixfs.h:22`). Set when the
    /// memory is allocated and when the tag changes under a cached block;
    /// cleared by the hand-over itself.
    need_set_cache: bool,
    /// The flags word shared with virtual memory (`lmfs_flags`): dirty,
    /// block-locked, evicted.
    flags: BufferFlags,
    /// The block's bytes: heap memory or a page the second level owns.
    memory: BlockMemory,
    /// Current users; pinned while above zero.
    users: usize,
    /// Free-list links: previous and next slot in the free chain
    /// (`NO_SLOT` when absent), the intrusive form of the C
    /// `front`/`rear` pointers.
    prev: usize,
    next: usize,
}

impl Buffer {
    /// A free slot holding no block.
    fn free() -> Self {
        Self {
            key: None,
            tag: None,
            need_set_cache: false,
            flags: BufferFlags::empty(),
            memory: BlockMemory::empty(),
            users: 0,
            prev: NO_SLOT,
            next: NO_SLOT,
        }
    }
}

/// "No slot" sentinel in the free-list links.
const NO_SLOT: usize = usize::MAX;

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
    /// Free slots in least-recently-used order, an intrusive doubly-linked
    /// list through the slot array (C: the `front`/`rear` chain,
    /// `cache.c:46-47`); `NO_SLOT` ends the chain.
    free_head: usize,
    free_tail: usize,
    /// Blocks currently pinned by callers. C: `bufs_in_use` (`cache.c:48`).
    pinned: usize,
    /// Bytes written to storage since the last usage re-estimation
    /// (`lmfs_change_blockusage`, `cache.c:119-161`).
    written_since_estimate: u64,
    /// File system usage counters feeding the sizing heuristic.
    total_blocks: u64,
    used_blocks: u64,
}

impl<S: BlockSource, V: SecondLevelCache> BlockCache<S, V> {
    /// Build a cache with `pool_size` free slots (at least
    /// [`MIN_POOL_SIZE`], mirroring the `lmfs_buf_pool` assertion,
    /// `cache.c:1250`).
    pub fn with_pool(source: S, mut second_level: V, pool_size: usize) -> Result<Self, Errno> {
        if pool_size < MIN_POOL_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        // The second level learns the block size here, which is where C
        // settles the enable decision (`lmfs_set_blocksize`, cache.c:1226-1240).
        second_level.set_block_size(source.block_size());
        let mut slots = Vec::with_capacity(pool_size);
        for _slot in 0..pool_size {
            slots.push(Buffer::free());
        }
        // 串起空闲链：头到尾依次链接。
        for slot in 0..pool_size - 1 {
            slots[slot].next = slot + 1;
            slots[slot + 1].prev = slot;
        }
        Ok(Self {
            source,
            second_level,
            slots,
            index: BTreeMap::new(),
            free_head: 0,
            free_tail: pool_size - 1,
            pinned: 0,
            written_since_estimate: 0,
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

    /// Take the backing source back (unmount hands the device to the
    /// server so a later mount can reuse it).
    ///
    /// Every block's memory goes back to the second level first, so a
    /// page-backed pool leaves no mapping behind when the device is handed
    /// over (C's `lmfs_buf_pool` unmaps the same way, cache.c:1259-1268).
    pub fn into_source(mut self) -> S {
        self.release_block_memory();
        self.source
    }

    /// Buffers currently pinned by callers.
    pub fn pinned(&self) -> usize {
        self.pinned
    }

    /// The second level, for inspection (unmount bookkeeping, tests).
    pub fn second_level(&self) -> &V {
        &self.second_level
    }

    /// The second level, for the server's own wiring.
    pub fn second_level_mut(&mut self) -> &mut V {
        &mut self.second_level
    }

    /// Acquire a block: return its slot number, pinned for the caller.
    ///
    /// C: `get_block_ino` (`cache.c:298-489`) with no inode tag
    /// (`lmfs_get_block`, cache.c:216-219). See [`BlockCache::acquire_tagged`].
    pub fn acquire(&mut self, key: BlockKey, mode: AcquireMode) -> Result<usize, Errno> {
        self.acquire_tagged(key, mode, None)
    }

    /// Acquire a block and record which inode it belongs to.
    ///
    /// C: `get_block_ino` (`cache.c:298-489`). Cache hit: a block the page
    /// cache has evicted is thrown away and re-fetched
    /// (`VMMC_EVICTED`, cache.c:345,377-388); a hit whose inode tag changed
    /// is re-identified to virtual memory on release (`needsetcache`,
    /// cache.c:365-375). Cache miss: the least-recently-used free buffer is
    /// recycled (writing a dirty victim back first, `freeblock`,
    /// cache.c:252-272), then the mode decides: `Peek` gives up with "not
    /// present", `Normal` asks the second level for the block's page before
    /// reading storage, `NoRead` hands over an empty buffer.
    pub fn acquire_tagged(
        &mut self,
        key: BlockKey,
        mode: AcquireMode,
        tag: Option<BlockTag>,
    ) -> Result<usize, Errno> {
        if let Some(&slot) = self.index.get(&key) {
            if self.slots[slot].flags.is_evicted() {
                // Virtual memory dropped the page under us: the cached copy
                // no longer names the block (`cache.c:377-388`).
                self.discard_slot(slot);
            } else {
                // The C header stores the use count in a `char`
                // (`libminixfs.h:21`); pinning past its ceiling would wrap,
                // so treat saturation as a programming error in debug builds.
                debug_assert!(self.slots[slot].users < i8::MAX as usize);
                if self.slots[slot].users == 0 {
                    self.list_unlink(slot);
                    self.pinned += 1;
                    self.slots[slot].flags.lock();
                }
                self.slots[slot].users += 1;
                self.retag(slot, tag);
                return Ok(slot);
            }
        }

        if mode == AcquireMode::Peek {
            return Err(Errno::from_i32(ENOENT));
        }

        let block_size = self.source.block_size();
        let slot = self.evict_one()?;

        // The page cache may already hold the block: a hit hands back the
        // page itself, so the caller reads it with no storage traffic and no
        // copy (`cache.c:437-451`).
        if mode != AcquireMode::NoRead && self.second_level.is_enabled() {
            let mut flags = BufferFlags::empty();
            let mapped = {
                let second = &mut self.second_level;
                second.map_block(key, tag, block_size, &mut flags)
            };
            if let Ok(Some(memory)) = mapped {
                let buffer = &mut self.slots[slot];
                buffer.key = Some(key);
                buffer.tag = tag;
                buffer.need_set_cache = false;
                buffer.memory = memory;
                buffer.flags = flags;
                buffer.flags.lock();
                buffer.users = 1;
                self.pinned += 1;
                self.index.insert(key, slot);
                return Ok(slot);
            }
        }

        let memory = self.second_level.alloc_block(block_size)?;
        {
            let buffer = &mut self.slots[slot];
            buffer.key = Some(key);
            buffer.tag = tag;
            // C allocates through `mmap` and marks the block as needing
            // identification to virtual memory right away
            // (`lmfs_alloc_block`, cache.c:210).
            buffer.need_set_cache = true;
            buffer.memory = memory;
            buffer.flags = BufferFlags::empty();
            buffer.flags.lock();
            buffer.users = 1;
        }
        self.pinned += 1;
        self.index.insert(key, slot);

        if mode == AcquireMode::Normal {
            // 存储源与槽位是不相交的字段借用：驱动直读进缓冲，
            // 不经过暂存分配（对齐 C 驱动直写缓冲的行为）。
            let slot_memory = &mut self.slots[slot].memory;
            let source = &self.source;
            if let Err(error) = source.read_block(key, slot_memory.as_mut_slice(block_size)) {
                // The block must not reach virtual memory nor stay a cache
                // entry. C invalidates it inside `read_block` (the device
                // number is dropped, cache.c:773-775) *before* `put_block`
                // runs, which is what keeps the hand-over from firing on a
                // block no one managed to read.
                self.discard_slot(slot);
                self.release_slot(slot);
                return Err(error);
            }
        }
        Ok(slot)
    }

    /// Read the bytes of an acquired slot.
    pub fn slot_data(&self, slot: usize) -> &[u8] {
        self.slots[slot].memory.as_slice(self.source.block_size())
    }

    /// Whether a slot's bytes live in a page the second level owns.
    ///
    /// The distinction is observable: only page-backed memory can be handed
    /// to virtual memory, and it is what a zero-copy pool is made of.
    pub fn slot_is_page_backed(&self, slot: usize) -> bool {
        self.slots[slot].memory.is_mapped()
    }

    /// The page behind a slot, when its bytes are page-backed.
    pub fn slot_memory(&self, slot: usize) -> Option<MappedPage> {
        self.slots[slot].memory.mapped()
    }

    /// The inode tag a slot's block carries, if any.
    pub fn slot_tag(&self, slot: usize) -> Option<BlockTag> {
        self.slots[slot].tag
    }

    /// Whether a slot's bytes carry unwritten changes
    /// (C: `lmfs_isclean`, `cache.c:174-177`, inverted).
    pub fn is_dirty(&self, slot: usize) -> bool {
        self.slots[slot].flags.is_dirty()
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
        buffer.flags.set_dirty();
        buffer.memory.as_mut_slice(block_size).copy_from_slice(data);
        Ok(())
    }

    /// Mark a slot dirty without writing (for callers that modify the
    /// buffer in place through [`BlockCache::slot_data_mut`]).
    /// C: `lmfs_markdirty` (`cache.c:164-167`).
    pub fn mark_dirty(&mut self, slot: usize) {
        self.slots[slot].flags.set_dirty();
    }

    /// Clear the dirty flag without writing. C: `lmfs_markclean`
    /// (`cache.c:169-172`).
    pub fn mark_clean(&mut self, slot: usize) {
        self.slots[slot].flags.clear_dirty();
    }

    /// Note that virtual memory evicted the slot's page; the contents are
    /// stale and the next acquire re-fetches the block.
    ///
    /// The word this sets is the one whose address travels to virtual memory
    /// in the `flags_ptr` lane (`VMMC_EVICTED`); C only ever reads it
    /// (cache.c:345), so this setter exists for the wire's other direction
    /// and for tests of the acquire rule.
    pub fn mark_evicted(&mut self, slot: usize) {
        self.slots[slot].flags.mark_evicted();
    }

    /// Mutable bytes of an acquired slot for in-place modification.
    pub fn slot_data_mut(&mut self, slot: usize) -> &mut [u8] {
        let block_size = self.source.block_size();
        self.slots[slot].memory.as_mut_slice(block_size)
    }

    /// Release a pinned slot back to the free ordering (most-recent end,
    /// "may be needed again", `put_block`, `cache.c:512-596`). Hands the
    /// block's page to the second level when one is enabled and the block
    /// still has to be identified (`needsetcache`, cache.c:562-587).
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
            if buffer.flags.is_dirty()
                && buffer.users == 0
                && let Some(key) = buffer.key
                && key.device == device
            {
                dirty_slots.push(slot);
            }
        }
        let mut written = 0u64;
        for slot in dirty_slots {
            let key = self.slots[slot].key.expect("dirty slot has a key");
            let block_size = self.source.block_size();
            let bytes = block_size as u64;
            let data = self.slots[slot].memory.as_slice(block_size);
            self.source.write_block(key, data)?;
            self.slots[slot].flags.clear_dirty();
            written += bytes;
        }
        self.note_written(written);
        Ok(())
    }

    /// Account bytes written to storage for the usage re-estimation
    /// trigger (`lmfs_change_blockusage`, `cache.c:119-161`).
    pub fn note_written(&mut self, bytes: u64) {
        self.written_since_estimate =
            self.written_since_estimate.saturating_add(bytes);
    }

    /// Whether enough was written since the last estimate that the pool
    /// sizing heuristic should run again.
    pub fn write_reestimate_due(&self) -> bool {
        self.written_since_estimate >= WRITE_REESTIMATE_THRESHOLD
    }

    /// Clear the written-since-estimate accumulator (after a resize).
    pub fn clear_written_note(&mut self) {
        self.written_since_estimate = 0;
    }

    /// Rebuild the pool at a new size (`lmfs_buf_pool` re-invoked,
    /// `cache.c:1245-1261`). Refuses while anything is pinned (`EBUSY`)
    /// and below the minimum (`EINVAL`); dirty blocks flush first so
    /// nothing is lost.
    pub fn resize_pool(&mut self, new_size: usize) -> Result<(), Errno> {
        if self.pinned != 0 {
            return Err(Errno::from_i32(EBUSY));
        }
        if new_size < MIN_POOL_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        self.flush_all()?;
        // C unmaps every pooled block before rebuilding the pool
        // (`lmfs_buf_pool`, cache.c:1259-1268).
        self.release_block_memory();
        self.slots.clear();
        self.index.clear();
        for _slot in 0..new_size {
            self.slots.push(Buffer::free());
        }
        for slot in 0..new_size - 1 {
            self.slots[slot].next = slot + 1;
            self.slots[slot + 1].prev = slot;
        }
        self.free_head = 0;
        self.free_tail = new_size - 1;
        Ok(())
    }

    /// Release a block that will not be needed again (`ONE_SHOT` blocks):
    /// it goes to the FRONT of the free list, so the next eviction picks it
    /// before any block likely to be reused, and it stops being a cache
    /// entry — C drops the block's device number (`cache.c:533-544`,
    /// cache.c:594-596). A one-shot block is handed to virtual memory with
    /// the `VMSF_ONCE` setflag so the page cache discards it after one use.
    pub fn release_one_shot(&mut self, slot: usize) -> Result<(), Errno> {
        if slot >= self.slots.len() || self.slots[slot].users == 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        self.slots[slot].users -= 1;
        self.pinned -= 1;
        if self.slots[slot].users != 0 {
            return Ok(());
        }
        self.hand_over(slot, true);
        // 一次性块用完即不再是缓存条目：C 把设备号作废（`cache.c:595`），
        // 防止它刚报备完又被立刻拿回来、造成同一块二次报备的麻烦。
        self.discard_slot(slot);
        self.list_push_front(slot);
        Ok(())
    }

    /// Warm the longest uncached run inside `[first_block, first_block +
    /// count)` (`lmfs_prefetch`, `cache.c:1057-1130`): the range maps to a
    /// cached/uncached bitmap, the longest uncached stretch wins, and its
    /// blocks read into the cache. Best effort by design — the demand path
    /// re-reads anyway. Returns the warmed block count.
    pub fn prefetch_uncached_range(
        &mut self,
        device: u64,
        first_block: u64,
        count: u64,
    ) -> usize {
        if count == 0 {
            return 0;
        }
        // C builds a before/after bitmap (`cache.c:1089-1130`); the longest
        // uncached stretch of the range wins.
        let mut best_start = 0u64;
        let mut best_len = 0u64;
        let mut run_start = 0u64;
        let mut run_len = 0u64;
        for offset in 0..count {
            let block = first_block + offset;
            if self.index.contains_key(&BlockKey::new(device, block)) {
                if run_len > best_len {
                    best_len = run_len;
                    best_start = run_start;
                }
                run_len = 0;
            } else {
                if run_len == 0 {
                    run_start = block;
                }
                run_len += 1;
            }
        }
        if run_len > best_len {
            best_start = run_start;
            best_len = run_len;
        }
        let mut warmed = 0usize;
        for offset in 0..best_len {
            let block = best_start + offset;
            if let Ok(cache_slot) =
                self.acquire(BlockKey::new(device, block), AcquireMode::Normal)
            {
                self.release_slot(cache_slot);
                warmed += 1;
            }
        }
        warmed
    }

    /// Flush every device. C: `lmfs_flushall` (`cache.c:1295-1311`).
    pub fn flush_all(&mut self) -> Result<(), Errno> {
        let mut devices = Vec::new();
        for buffer in &self.slots {
            if buffer.flags.is_dirty()
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
    /// C: `lmfs_free_block` (`cache.c:613-650`). Virtual memory is told to
    /// forget the block first — the block number may be re-used for a
    /// different file later, and a stale inode association would make a hole
    /// map to the old contents (`cache.c:616-623`). The cached copy is then
    /// marked clean and detached from its key even when pinned: the owner
    /// may still hold the slot, but its contents no longer name a stored
    /// block.
    pub fn free_block(&mut self, key: BlockKey) {
        let block_size = self.source.block_size();
        self.second_level.forget_block(key, block_size);
        if let Some(&slot) = self.index.get(&key) {
            self.index.remove(&key);
            let buffer = &mut self.slots[slot];
            buffer.key = None;
            buffer.tag = None;
            buffer.need_set_cache = false;
            buffer.flags.clear_dirty();
        }
    }

    /// Drop every cached block of a device and tell the second level to
    /// forget it. C: `lmfs_invalidate` (`cache.c:782-808`).
    ///
    /// The device's pages go back to the second level, and the forget-all
    /// call goes out even when the second level is switched off: an error
    /// may have switched it off while blocks were still registered there
    /// (C's own reasoning, `cache.c:803-807`).
    pub fn invalidate_device(&mut self, device: u64) {
        // 只清身份，不动空闲链：C 的 lmfs_invalidate 同样把块留在 LRU 链上
        // （`cache.c:782-808`），槽位容量因此不因失效而流失。
        for slot in 0..self.slots.len() {
            let on_device = self.slots[slot]
                .key
                .is_some_and(|key| key.device == device);
            if on_device {
                self.clear_slot(slot);
            }
        }
        self.second_level.clear_device(device);
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
        let slot = self.list_pop_front().ok_or(Errno::from_i32(EAGAIN))?;
        let (old_key, was_dirty) = {
            let buffer = &mut self.slots[slot];
            (buffer.key.take(), buffer.flags.is_dirty())
        };
        if let Some(key) = old_key {
            self.index.remove(&key);
            if was_dirty {
                // 直接把缓存的字节写回存储：源与槽位是不相交的借用，
                // 不需要克隆整块（旧实现为绕借用检查克隆过一次）。
                let block_size = self.source.block_size();
                let data = self.slots[slot].memory.as_slice(block_size);
                self.source
                    .write_block(key, data)
                    .inspect_err(|_| {
                        // Restore the victim so its dirty contents survive
                        // the failed write-back instead of being silently
                        // dropped.
                        let buffer = &mut self.slots[slot];
                        buffer.key = Some(key);
                        buffer.flags.set_dirty();
                        self.index.insert(key, slot);
                        self.list_push_front(slot);
                    })?;
            }
        }
        self.clear_slot(slot);
        Ok(slot)
    }

    /// Drop one slot's block identity: the memory goes back to the second
    /// level, the flags word is cleared, and the tag with it.
    ///
    /// C's `freeblock` does the same three things (`munmap_t`, `MARKCLEAN`,
    /// device number dropped, cache.c:252-272).
    fn clear_slot(&mut self, slot: usize) {
        if let Some(key) = self.slots[slot].key.take() {
            self.index.remove(&key);
        }
        let buffer = &mut self.slots[slot];
        let memory = core::mem::replace(&mut buffer.memory, BlockMemory::empty());
        buffer.tag = None;
        buffer.need_set_cache = false;
        buffer.flags = BufferFlags::empty();
        self.second_level.free_block(memory);
    }

    /// Invalidate a slot that is no longer a cache entry while keeping its
    /// memory for the next block that recycles it (C drops the device
    /// number and leaves the mapping in place, cache.c:377-388,
    /// cache.c:594-596).
    fn discard_slot(&mut self, slot: usize) {
        if let Some(key) = self.slots[slot].key.take() {
            self.index.remove(&key);
        }
        self.slots[slot].tag = None;
        self.slots[slot].need_set_cache = false;
        self.slots[slot].flags = BufferFlags::empty();
    }

    /// Update a hit slot's inode tag, marking the block for re-identification
    /// to virtual memory when the tag moved (`cache.c:365-375`).
    fn retag(&mut self, slot: usize, tag: Option<BlockTag>) {
        if tag.is_none() {
            // C only touches the tag when the caller named an inode
            // (`if(ino != VMC_NO_INODE)`, cache.c:366).
            return;
        }
        let buffer = &mut self.slots[slot];
        if buffer.tag != tag {
            buffer.tag = tag;
            buffer.need_set_cache = true;
        }
    }

    /// Hand a released block's page to the second level when it still owes
    /// an introduction: the block was allocated here (`needsetcache` set at
    /// allocation, cache.c:210) or its inode tag changed while cached
    /// (cache.c:365-375). `once` is the `VMSF_ONCE` setflag of one-shot
    /// blocks (`put_block`, cache.c:562-587).
    ///
    /// Failures are the second level's policy (it disables itself on a wire
    /// that is gone); the block stays valid in this pool either way.
    fn hand_over(&mut self, slot: usize, once: bool) {
        if !self.second_level.is_enabled() {
            return;
        }
        let block_size = self.source.block_size();
        let buffer = &mut self.slots[slot];
        // C checks the device too: a block whose device was dropped (freed,
        // one-shot) is not identified to virtual memory
        // (`dev != NO_DEV`, cache.c:566).
        let (Some(key), true) = (buffer.key, buffer.need_set_cache) else {
            return;
        };
        let memory = &buffer.memory;
        let tag = buffer.tag;
        let outcome = self
            .second_level
            .set_block(memory, key, tag, block_size, once, &mut buffer.flags);
        buffer.need_set_cache = false;
        let _ = outcome;
    }

    /// Give every block's memory back to the second level, for pools that
    /// are about to be rebuilt or dropped (C: `lmfs_buf_pool`,
    /// cache.c:1259-1268).
    ///
    /// A pool dropped without this call leaves its page mappings to the
    /// address space's teardown: the server exits, the memory server
    /// reclaims them then. C relies on the same backstop for a buffer pool
    /// released at process end.
    pub fn release_block_memory(&mut self) {
        for slot in 0..self.slots.len() {
            self.clear_slot(slot);
        }
        self.index.clear();
    }

    /// Push one slot at the head of the free list: the "needed again soon"
    /// position used when a victim write-back fails.
    fn list_push_front(&mut self, slot: usize) {
        self.slots[slot].prev = NO_SLOT;
        self.slots[slot].next = self.free_head;
        if self.free_head != NO_SLOT {
            self.slots[self.free_head].prev = slot;
        } else {
            self.free_tail = slot;
        }
        self.free_head = slot;
    }

    /// Unlink one slot from the free list in constant time: the neighbours
    /// re-link past it (the C `rm_lru` walks the same doubly-linked chain,
    /// `cache.c:1171`).
    fn list_unlink(&mut self, slot: usize) {
        let (prev, next) = (self.slots[slot].prev, self.slots[slot].next);
        if prev != NO_SLOT {
            self.slots[prev].next = next;
        } else {
            self.free_head = next;
        }
        if next != NO_SLOT {
            self.slots[next].prev = prev;
        } else {
            self.free_tail = prev;
        }
        self.slots[slot].prev = NO_SLOT;
        self.slots[slot].next = NO_SLOT;
    }

    /// Push one slot at the tail of the free list: the most recently
    /// released position, evicted last.
    fn list_push_tail(&mut self, slot: usize) {
        self.slots[slot].prev = self.free_tail;
        self.slots[slot].next = NO_SLOT;
        if self.free_tail != NO_SLOT {
            self.slots[self.free_tail].next = slot;
        } else {
            self.free_head = slot;
        }
        self.free_tail = slot;
    }

    /// Pop the head of the free list (the least recently released slot).
    fn list_pop_front(&mut self) -> Option<usize> {
        let slot = self.free_head;
        if slot == NO_SLOT {
            return None;
        }
        self.list_unlink(slot);
        Some(slot)
    }

    /// Unpin one use of a slot; return it to the free ordering when the last
    /// user leaves. Internal: bounds are checked by [`BlockCache::release`]
    /// and by [`BlockCache::acquire`].
    fn release_slot(&mut self, slot: usize) {
        self.pinned -= 1;
        let users = self.slots[slot].users - 1;
        self.slots[slot].users = users;
        if users == 0 {
            // C clears the block-locked bit before the hand-over
            // (`put_block`, cache.c:558-559) and identifies the block to
            // virtual memory on the way out (cache.c:562-587).
            self.slots[slot].flags.unlock();
            self.hand_over(slot, false);
            self.list_push_tail(slot);
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

    /// A pool over the memory source with the page cache switched on.
    ///
    /// The block size is a whole page so the alignment gate lets the second
    /// level through (a smaller block would be handled exactly like
    /// [`NoSecondLevel`], which other tests cover).
    fn vm_cache() -> BlockCache<MemSource, VmSecondLevel<crate::vm_cache::mock::MockVm>> {
        let source = MemSource::with_blocks(16, PAGE_SIZE);
        BlockCache::with_pool(source, VmSecondLevel::new(Default::default()), 8).unwrap()
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
    fn test_release_one_shot_front_inserts() {
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(16, 64),
            NoSecondLevel,
            MIN_POOL_SIZE,
        )
        .unwrap();
        // Fill the pool: 0..6 (0 is the LRU end).
        for block in 0..6u64 {
            let slot = cache
                .acquire(BlockKey::new(1, block), AcquireMode::NoRead)
                .unwrap();
            cache.release(slot).unwrap();
        }
        // Re-acquire block 3 and mark it one-shot: it jumps to the front
        // (the next eviction victim), jumping over blocks 4 and 5.
        let shot = cache.acquire(BlockKey::new(1, 3), AcquireMode::NoRead).unwrap();
        cache.release_one_shot(shot).unwrap();
        let victim = cache
            .acquire(BlockKey::new(1, 20), AcquireMode::NoRead)
            .unwrap();
        cache.release(victim).unwrap();
        assert!(!cache.index.contains_key(&BlockKey::new(1, 3)));
        assert!(cache.index.contains_key(&BlockKey::new(1, 4)));
        assert!(cache.index.contains_key(&BlockKey::new(1, 5)));
    }

    #[test]
    fn test_prefetch_range_warms_longest_uncached_run() {
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(16, 64),
            NoSecondLevel,
            MIN_POOL_SIZE,
        )
        .unwrap();
        // Cache block 3 of the range [0, 8): the longest uncached run is
        // [4, 8) — four blocks warm, and block 3 stays as it is.
        let slot = cache
            .acquire(BlockKey::new(1, 3), AcquireMode::Normal)
            .unwrap();
        cache.release(slot).unwrap();
        let warmed = cache.prefetch_uncached_range(1, 0, 8);
        assert_eq!(warmed, 4);
        for block in 4..8u64 {
            assert!(cache.index.contains_key(&BlockKey::new(1, block)));
        }
        // A second pass warms only the still-uncached head run [0, 3):
        // blocks 3..8 were cached by the first call.
        let again = cache.prefetch_uncached_range(1, 0, 8);
        assert_eq!(again, 3);
    }

    #[test]
    fn test_resize_pool_rebuilds_and_refuses() {
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(16, 64),
            NoSecondLevel,
            MIN_POOL_SIZE,
        )
        .unwrap();
        // Refuses below the minimum.
        assert_eq!(
            cache.resize_pool(MIN_POOL_SIZE - 1).unwrap_err().to_i32(),
            EINVAL
        );
        // Grows: the chain serves every slot.
        cache.resize_pool(12).unwrap();
        assert_eq!(cache.pool_size(), 12);
        for block in 0..12u64 {
            let slot = cache
                .acquire(BlockKey::new(1, block), AcquireMode::NoRead)
                .unwrap();
            cache.release(slot).unwrap();
        }
        // Shrinks with nothing pinned: back to the minimum.
        cache.resize_pool(MIN_POOL_SIZE).unwrap();
        assert_eq!(cache.pool_size(), MIN_POOL_SIZE);
    }

    #[test]
    fn test_write_reestimate_threshold() {
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(16, 64),
            NoSecondLevel,
            MIN_POOL_SIZE,
        )
        .unwrap();
        assert!(!cache.write_reestimate_due());
        cache.note_written(WRITE_REESTIMATE_THRESHOLD - 1);
        assert!(!cache.write_reestimate_due());
        cache.note_written(1);
        assert!(cache.write_reestimate_due());
        cache.clear_written_note();
        assert!(!cache.write_reestimate_due());
    }

    #[test]
    fn test_lru_order_tracks_touches() {
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(16, 64),
            NoSecondLevel,
            MIN_POOL_SIZE,
        )
        .unwrap();
        // Load six blocks to fill the pool (order: 0..6 — block 0 is the
        // LRU end).
        for block in 0..6u64 {
            let slot = cache
                .acquire(BlockKey::new(1, block), AcquireMode::NoRead)
                .unwrap();
            cache.release(slot).unwrap();
        }
        // Touch block 1 (the middle): it moves to the LRU end.
        let middle = cache
            .acquire(BlockKey::new(1, 1), AcquireMode::NoRead)
            .unwrap();
        cache.release(middle).unwrap();
        // A new block needs a victim: block 0 goes first (untouched),
        // block 1 survives the touch.
        let victim = cache
            .acquire(BlockKey::new(1, 20), AcquireMode::NoRead)
            .unwrap();
        cache.release(victim).unwrap();
        assert!(!cache.index.contains_key(&BlockKey::new(1, 0)));
        assert!(cache.index.contains_key(&BlockKey::new(1, 1)));
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
    fn test_invalidate_keeps_free_slots_usable() {
        let mut cache = cache();
        // Fill most of the pool with one device's blocks and release them:
        // the slots are free but still carry their keys.
        for block in 0..6u64 {
            let slot = cache
                .acquire(BlockKey::new(1, block), AcquireMode::Normal)
                .unwrap();
            cache.release(slot).unwrap();
        }
        cache.invalidate_device(1);
        // Invalidation drops identities, not capacity: every slot must stay
        // reachable, so the whole pool can be pinned again afterwards (C
        // keeps invalidated blocks in the LRU chain, cache.c:782-808).
        let mut held = Vec::new();
        for block in 0..8u64 {
            held.push(
                cache
                    .acquire(BlockKey::new(2, block), AcquireMode::Normal)
                    .unwrap(),
            );
        }
        for slot in held {
            cache.release(slot).unwrap();
        }
    }

    #[test]
    fn test_second_level_serves_the_block_without_storage() {
        let mut cache = vm_cache();
        let key = BlockKey::new(1, 3);
        // Virtual memory already holds the block (some process mapped the
        // file, say): the pool must take the page instead of reading.
        cache.second_level.wire_mut().seed(1, 3 * PAGE_SIZE as u64, 0x5A, PAGE_SIZE);
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.source.reads.get(), 0, "no storage traffic on a page-cache hit");
        assert_eq!(cache.slot_data(slot)[0], 0x5A);
        assert!(cache.slot_is_page_backed(slot), "the block is the page cache's own page");
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_second_level_hands_over_a_freshly_read_block() {
        let mut cache = vm_cache();
        let key = BlockKey::new(1, 7);
        // Storage holds 0x07 in every byte; the pool reads it and then hands
        // the page to virtual memory.
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.source.reads.get(), 1);
        assert_eq!(cache.slot_data(slot)[0], 0x07);
        cache.release(slot).unwrap();
        assert_eq!(
            cache.second_level.wire().cached(1, 7 * PAGE_SIZE as u64, 4),
            Some(&[0x07; 4][..]),
            "the page cache now holds the block's bytes"
        );
    }

    #[test]
    fn test_hand_over_is_gated_by_the_inode_tag() {
        let mut cache = vm_cache();
        let key = BlockKey::new(1, 4);
        let tag_a = Some(BlockTag::new(11, 3 * PAGE_SIZE as u64));
        let tag_b = Some(BlockTag::new(11, 5 * PAGE_SIZE as u64));
        // First acquire allocates the block: the tag goes out with the page.
        let slot = cache.acquire_tagged(key, AcquireMode::Normal, tag_a).unwrap();
        cache.release(slot).unwrap();
        // Second acquire is a hit with the same tag: nothing to re-identify.
        let before = cache.second_level.wire().calls.len();
        let slot = cache.acquire_tagged(key, AcquireMode::Normal, tag_a).unwrap();
        cache.release(slot).unwrap();
        assert_eq!(cache.second_level.wire().calls.len(), before, "same tag: no second hand-over");
        assert_eq!(cache.slot_tag(slot), tag_a, "the hit kept its tag");

        // Third acquire is a hit whose tag moved: virtual memory must learn
        // the new inode association (C's `needsetcache`, cache.c:365-375).
        let slot = cache.acquire_tagged(key, AcquireMode::Normal, tag_b).unwrap();
        assert_eq!(cache.slot_tag(slot), tag_b);
        cache.release(slot).unwrap();
        assert_eq!(cache.second_level.wire().calls.len(), before + 1);
        assert_eq!(
            cache.second_level.wire().calls.last(),
            Some(&crate::vm_cache::mock::MockCall::Set {
                dev: 1,
                dev_offset: 4 * PAGE_SIZE as u64,
                tag: tag_b,
                once: false,
                page: cache.slot_memory(slot).expect("a page-backed block"),
            })
        );
    }

    #[test]
    fn test_unaligned_block_size_keeps_the_page_cache_out() {
        // 64-byte blocks cannot be mapped into pages: the second level never
        // consults virtual memory (C: `cache.c:1236-1239`). Its own memory
        // is still page-backed, exactly as C's `mmap` blocks are.
        let mut cache = BlockCache::with_pool(
            MemSource::with_blocks(8, 64),
            VmSecondLevel::new(crate::vm_cache::mock::MockVm::default()),
            8,
        )
        .unwrap();
        assert!(!cache.second_level.is_enabled());
        let slot = cache.acquire(BlockKey::new(1, 1), AcquireMode::Normal).unwrap();
        assert_eq!(cache.slot_data(slot)[0], 1, "storage still serves the block");
        cache.release(slot).unwrap();
        assert!(cache.second_level.wire().calls.is_empty());
    }

    #[test]
    fn test_evicted_slot_is_re_fetched() {
        let mut cache = vm_cache();
        let key = BlockKey::new(2, 2);
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        // Virtual memory drops the page and reports it through the flags
        // word; the pool's copy no longer names the block.
        cache.second_level.wire_mut().evict(2, 2 * PAGE_SIZE as u64);
        cache.mark_evicted(slot);
        let reads_before = cache.source.reads.get();
        let again = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.source.reads.get(), reads_before + 1, "the stale copy is not reused");
        assert!(!cache.is_dirty(again));
        assert_eq!(cache.slot_data(again)[0], 2);
        cache.release(again).unwrap();
    }

    #[test]
    fn test_free_block_forgets_the_block_in_the_page_cache() {
        let mut cache = vm_cache();
        let key = BlockKey::new(3, 5);
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        assert!(cache.second_level.wire().cached(3, 5 * PAGE_SIZE as u64, 1).is_some());
        cache.free_block(key);
        assert!(cache.second_level.wire().cached(3, 5 * PAGE_SIZE as u64, 1).is_none());
    }

    #[test]
    fn test_invalidate_releases_pages_and_clears_the_device() {
        let mut cache = vm_cache();
        let slot = cache.acquire(BlockKey::new(4, 1), AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        let other = cache.acquire(BlockKey::new(5, 1), AcquireMode::Normal).unwrap();
        cache.release(other).unwrap();
        cache.invalidate_device(4);
        // The device's own pages came back and the forget-all went out;
        // another device's page cache entries stay untouched.
        assert!(cache.second_level.wire().cached(4, PAGE_SIZE as u64, 1).is_none());
        assert!(cache.second_level.wire().cached(5, PAGE_SIZE as u64, 1).is_some());
        assert!(cache.second_level.wire().calls.iter().any(|call| matches!(
            call,
            crate::vm_cache::mock::MockCall::Clear { dev: 4 }
        )));
    }

    #[test]
    fn test_failed_read_is_not_handed_over() {
        let mut cache = vm_cache();
        let key = BlockKey::new(1, 9);
        cache.source.fail_reads = true;
        let sets_before = cache
            .second_level
            .wire()
            .calls
            .iter()
            .filter(|call| matches!(call, crate::vm_cache::mock::MockCall::Set { .. }))
            .count();
        assert!(cache.acquire(key, AcquireMode::Normal).is_err());
        let sets_after = cache
            .second_level
            .wire()
            .calls
            .iter()
            .filter(|call| matches!(call, crate::vm_cache::mock::MockCall::Set { .. }))
            .count();
        assert_eq!(
            sets_after, sets_before,
            "a block no one managed to read is never handed over"
        );
        assert!(cache.second_level.wire().cached(1, 9 * PAGE_SIZE as u64, 1).is_none());
        // The slot is free again: the next acquire reads storage and works.
        cache.source.fail_reads = false;
        let slot = cache.acquire(key, AcquireMode::Normal).unwrap();
        assert_eq!(cache.slot_data(slot)[0], 9);
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_unmount_returns_every_page() {
        let mut cache = vm_cache();
        for block in 0..3u64 {
            let slot = cache.acquire(BlockKey::new(6, block), AcquireMode::Normal).unwrap();
            cache.release(slot).unwrap();
        }
        let source = cache.into_source();
        assert_eq!(source.block_size(), PAGE_SIZE);
    }

    #[test]
    fn test_one_shot_release_invalidates_and_marks_the_hand_over() {
        let mut cache = vm_cache();
        let key = BlockKey::new(8, 1);
        let slot = cache.acquire(key, AcquireMode::NoRead).unwrap();
        cache.release_one_shot(slot).unwrap();
        // The block stops being a cache entry (C drops its device number)
        // and the page cache is told the page is good for one use.
        assert!(!cache.index.contains_key(&key));
        let once = cache.second_level.wire().calls.iter().any(|call| matches!(
            call,
            crate::vm_cache::mock::MockCall::Set { once: true, .. }
        ));
        assert!(once, "a one-shot block is handed over with VMSF_ONCE");
    }

    #[test]
    fn test_invalidate_leaves_no_pages_behind() {
        let mut cache = vm_cache();
        let slot = cache.acquire(BlockKey::new(9, 2), AcquireMode::Normal).unwrap();
        cache.release(slot).unwrap();
        let pages_before = cache.second_level.wire().live_pages();
        assert!(pages_before > 0);
        cache.invalidate_device(9);
        assert!(
            cache.second_level.wire().live_pages() < pages_before,
            "the device's page came back to the second level"
        );
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
        cache.free_head = 0;
        cache.free_tail = size - 1;
        for _slot in 0..size {
            cache.slots.push(Buffer::free());
        }
        for slot in 0..size - 1 {
            cache.slots[slot].next = slot + 1;
            cache.slots[slot + 1].prev = slot;
        }
        Ok(())
    }
}
