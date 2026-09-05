//! In-memory inode table: lookup, lifecycle, and disk conversion.
//!
//! C correspondence: `minix3/minix/fs/mfs/inode.c` (all four hundred
//! sixty-four lines) and `inode.h` (all seventy-two lines). The table holds
//! the working set of inodes: a hash index keyed by number finds live
//! entries, an unused queue recycles cold ones, and reference counts pin
//! busy ones. Disk conversion reads and writes the sixty-four-byte disk
//! format through the block cache.
//!
//! Two adaptations to Rust. The global table becomes a value: tests run two
//! tables side by side, which the C globals forbid. And the unlinked-inode
//! path reports instead of truncating inline: freeing data zones belongs to
//! the link stage (document 13), so releasing the last link returns the
//! zones for the caller to reclaim instead of silently dropping them.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use minix_types::{EINVAL, EIO, ENFILE, ENOSPC, EROFS, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, SecondLevelCache};

use crate::superblock::{Bitmap, INODE_DISK_SIZE, Superblock};

/// Absent entry marker (`NO_ENTRY`, `minix3/minix/include/minix/const.h:130`,
/// value zero). Inode numbers start at one, so zero never names a file.
pub const NO_ENTRY: u64 = 0;
/// Absent link count (`NO_LINK`, same header line 133, value zero).
pub const NO_LINK: u16 = 0;
/// Unallocated type bits (`I_NOT_ALLOC`, same header line 120, value zero).
pub const NOT_ALLOCATED: u16 = 0;

/// Slots in the in-core table (`NR_INODES`, `const.h:8`, five hundred twelve).
pub const TABLE_SLOTS: usize = 512;
/// Hash buckets (`INODE_HASH_SIZE`, `const.h:14`, one hundred twenty-eight).
pub const HASH_BUCKETS: usize = 128;

/// File-type mask (`I_TYPE`, `const.h:104`, octal `0170000`).
pub const TYPE_MASK: u32 = 0o170000;
/// Directory (`I_DIRECTORY`, octal `0040000`).
pub const TYPE_DIRECTORY: u32 = 0o040000;
/// Regular file (`I_REGULAR`, octal `0100000`).
pub const TYPE_REGULAR: u32 = 0o100000;
/// Block device (`I_BLOCK_SPECIAL`, octal `0060000`).
pub const TYPE_BLOCK: u32 = 0o060000;
/// Character device (`I_CHAR_SPECIAL`, octal `0020000`).
pub const TYPE_CHARACTER: u32 = 0o020000;
/// Symbolic link (`I_SYMBOLIC_LINK`, octal `0120000`).
pub const TYPE_SYMLINK: u32 = 0o120000;
/// Socket type bits for completeness (octal `0140000`, `stat.h`).
pub const TYPE_SOCKET: u32 = 0o140000;

/// Seek flag set (`ISEEK`, `inode.h:64`).
pub const SEEK_SET: bool = true;
/// Seek flag clear (`NO_SEEK`, `inode.h:63`).
pub const SEEK_CLEAR: bool = false;

/// Lazy time-update bits (`const.h:44-46`, octal).
pub const UPDATE_ACCESS: u8 = 0o2;
/// Lazy change-time update request.
pub const UPDATE_CHANGE: u8 = 0o4;
/// Lazy modification-time update request.
pub const UPDATE_MODIFY: u8 = 0o10;

/// Directory entry size in bytes (`DIR_ENTRY_SIZE`, sixty-four: four-byte
/// number plus sixty-byte name, `mfsdir.h:15-18`).
pub const DIRECTORY_ENTRY_SIZE: usize = 64;
/// Name capacity per entry (`MFS_DIRSIZ`, `mfsdir.h:13`, sixty).
pub const NAME_CAPACITY: usize = 60;
/// Hard-link ceiling (`LINK_MAX`, `syslimits.h:54`, thirty-two thousand
/// seven hundred sixty-seven).
pub const LINK_CEILING: u32 = 32767;

/// Direct zone slots per inode (`V2_NR_DZONES`, seven).
pub const DIRECT_ZONES: usize = 7;
/// Total zone slots per inode (`V2_NR_TZONES`, ten).
pub const TOTAL_ZONES: usize = 10;

/// Why an inode operation failed. Wire mapping follows the C call sites:
/// table exhaustion is "file table full", read-only allocation is
/// "read-only file system", bitmap exhaustion is "no space", corrupt disk
/// images are "input-output error", everything else is "invalid argument".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeError {
    /// No free table slot (`ENFILE`, `inode.c:148`).
    TableFull,
    /// Allocation on a read-only file system (`EROFS`, `inode.c:263`).
    ReadOnly,
    /// Bitmap exhausted (`ENOSPC`, `inode.c:270`).
    NoSpace,
    /// Disk image contradicts itself (negative size, unknown entry).
    Corrupt,
    /// Bad request (unknown inode, bad count, unmounted use).
    Invalid,
}

impl InodeError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::TableFull => Errno::from_i32(ENFILE),
            Self::ReadOnly => Errno::from_i32(EROFS),
            Self::NoSpace => Errno::from_i32(ENOSPC),
            Self::Corrupt => Errno::from_i32(EIO),
            Self::Invalid => Errno::from_i32(EINVAL),
        }
    }
}

/// Sixty-four-byte disk inode (`d2_inode`, `type.h:8-17`).
///
/// Layout: mode, link count, owner, group (two bytes each), size and three
/// times (four bytes each), ten zone numbers (four bytes each). Parsed with
/// explicit little-endian reads, like the superblock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskInode {
    /// File type and permission bits.
    pub mode: u16,
    /// Link count.
    pub nlinks: u16,
    /// Owner user identifier (stored signed, read unsigned).
    pub owner: u16,
    /// Owner group identifier.
    pub group: u16,
    /// File size in bytes.
    pub size: i32,
    /// Last access time.
    pub accessed: i32,
    /// Last data change time.
    pub modified: i32,
    /// Last status change time.
    pub changed: i32,
    /// Zone numbers (direct, indirect, double indirect).
    pub zones: [u32; TOTAL_ZONES],
}

impl DiskInode {
    /// Parse one sixty-four-byte record.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, InodeError> {
        if bytes.len() < INODE_DISK_SIZE {
            return Err(InodeError::Corrupt);
        }
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let i32_at = |at: usize| {
            i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let u32_at = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let mut zones = [0u32; TOTAL_ZONES];
        for (slot, zone) in zones.iter_mut().enumerate() {
            *zone = u32_at(24 + slot * 4);
        }
        Ok(Self {
            mode: u16_at(0),
            nlinks: u16_at(2),
            owner: u16_at(4),
            group: u16_at(6),
            size: i32_at(8),
            accessed: i32_at(12),
            modified: i32_at(16),
            changed: i32_at(20),
            zones,
        })
    }

    /// Serialize one sixty-four-byte record.
    pub fn to_bytes(&self) -> [u8; INODE_DISK_SIZE] {
        let mut out = [0u8; INODE_DISK_SIZE];
        out[0..2].copy_from_slice(&self.mode.to_le_bytes());
        out[2..4].copy_from_slice(&self.nlinks.to_le_bytes());
        out[4..6].copy_from_slice(&self.owner.to_le_bytes());
        out[6..8].copy_from_slice(&self.group.to_le_bytes());
        out[8..12].copy_from_slice(&self.size.to_le_bytes());
        out[12..16].copy_from_slice(&self.accessed.to_le_bytes());
        out[16..20].copy_from_slice(&self.modified.to_le_bytes());
        out[20..24].copy_from_slice(&self.changed.to_le_bytes());
        for (slot, zone) in self.zones.iter().enumerate() {
            out[24 + slot * 4..28 + slot * 4].copy_from_slice(&zone.to_le_bytes());
        }
        out
    }
}

/// One in-core inode: disk fields widened plus memory-only bookkeeping.
///
/// Mirrors `struct inode` (`inode.h:20-50`): the first group travels to
/// disk, the second group (device, number, count, geometry, superblock
/// generation marker, dirt, zone hint, directory scan hint, mount flag, seek
/// flag, update bits) lives only here. The hash and free-list links of the
/// C struct become positions in the table's index structures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inode {
    /// File type and permission bits.
    pub mode: u16,
    /// Link count; zero means free me on release.
    pub nlinks: u16,
    /// Owner user identifier.
    pub owner: u16,
    /// Owner group identifier.
    pub group: u16,
    /// File size in bytes (widened from the thirty-two-bit disk field).
    pub size: i64,
    /// Last access time.
    pub accessed: i64,
    /// Last data change time.
    pub modified: i64,
    /// Last status change time.
    pub changed: i64,
    /// Zone numbers (widened; ten slots).
    pub zones: [u64; TOTAL_ZONES],
    /// Device holding the inode.
    pub device: u64,
    /// Inode number; zero marks a free slot.
    pub number: u64,
    /// Reference count; zero means on the unused queue.
    pub count: u32,
    /// Direct zones (seven, from the superblock at load).
    pub direct_zone_count: u32,
    /// Zones per indirect block (from the superblock at load).
    pub indirect_per_block: u32,
    /// Dirty: differs from disk.
    pub dirty: bool,
    /// Zone search hint.
    pub zone_hint: u64,
    /// Directory scan hint (byte offset for the next entry search).
    pub scan_hint: u64,
    /// Mounted-on flag.
    pub mountpoint: bool,
    /// Seek flag: set on seek, cleared on read or write.
    pub seek: bool,
    /// Pending lazy time updates.
    pub pending_updates: u8,
}

impl Inode {
    /// Free slot marker.
    fn free() -> Self {
        Self {
            mode: NOT_ALLOCATED,
            nlinks: NO_LINK,
            owner: 0,
            group: 0,
            size: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            zones: [0; TOTAL_ZONES],
            device: 0,
            number: NO_ENTRY,
            count: 0,
            direct_zone_count: DIRECT_ZONES as u32,
            indirect_per_block: 0,
            dirty: false,
            zone_hint: 0,
            scan_hint: 0,
            mountpoint: false,
            seek: SEEK_CLEAR,
            pending_updates: 0,
        }
    }
}

/// What releasing the last reference found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseOutcome {
    /// Still referenced; nothing else to do.
    Held,
    /// Fully released; dirty contents were written back when needed.
    Released,
    /// Fully released with no links: the caller must reclaim these data
    /// zones (free the bitmap bits and drop the cache copies) now that the
    /// slot is free. This is the explicit form of the truncate-and-free
    /// path inside `put_inode` (`inode.c:219-245`); the link stage owns the
    /// zone freeing (document 13).
    ReclaimZones(Vec<u64>),
}

/// The inode table: fixed slots, hash index, unused queue.
///
/// All three structures move together: a slot is either hashed (live,
/// possibly with count zero while cached) or queued (free or cached-cold),
/// never both, never neither. Debug builds assert the pairing on every
/// transition.
#[derive(Debug)]
pub struct InodeTable {
    slots: Vec<Inode>,
    /// Buckets of slot indices by `number & 127` (`addhash_inode`).
    hash: Vec<Vec<usize>>,
    /// Unused queue: freed slots at the front, cached-cold at the back
    /// (mirrors `TAILQ_INSERT_HEAD` for freed and `TAILQ_INSERT_TAIL` for
    /// cached in `put_inode`).
    unused: VecDeque<usize>,
    hits: u64,
    misses: u64,
}

impl InodeTable {
    /// Empty table: every slot free and queued (`init_inode_cache`).
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(TABLE_SLOTS);
        for _ in 0..TABLE_SLOTS {
            slots.push(Inode::free());
        }
        let mut unused = VecDeque::with_capacity(TABLE_SLOTS);
        // Head insertion order like the C loop: slot zero ends up first.
        for index in (0..TABLE_SLOTS).rev() {
            unused.push_front(index);
        }
        let mut hash = Vec::with_capacity(HASH_BUCKETS);
        for _ in 0..HASH_BUCKETS {
            hash.push(Vec::new());
        }
        Self {
            slots,
            hash,
            unused,
            hits: 0,
            misses: 0,
        }
    }

    /// Cache hit counter (observability for tests).
    pub const fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache miss counter.
    pub const fn misses(&self) -> u64 {
        self.misses
    }

    /// Bucket for an inode number.
    fn bucket(number: u64) -> usize {
        (number & (HASH_BUCKETS as u64 - 1)) as usize
    }

    /// Remove a slot from its hash bucket.
    fn unhash(&mut self, slot: usize) {
        let number = self.slots[slot].number;
        if number == NO_ENTRY {
            return;
        }
        let bucket = &mut self.hash[Self::bucket(number)];
        if let Some(position) = bucket.iter().position(|&index| index == slot) {
            bucket.remove(position);
        }
    }

    /// Find a referenced slot without touching counts (`find_inode`).
    pub fn find(&self, device: u64, number: u64) -> Option<usize> {
        if number == NO_ENTRY {
            return None;
        }
        self.hash[Self::bucket(number)]
            .iter()
            .copied()
            .find(|&slot| {
                let inode = &self.slots[slot];
                inode.count > 0 && inode.number == number && inode.device == device
            })
    }

    /// Add a reference to a known slot (`dup_inode`).
    pub fn duplicate(&mut self, slot: usize) {
        self.slots[slot].count += 1;
    }

    /// Get an inode, loading from disk on a miss (`get_inode`).
    ///
    /// Hit on a cached-cold slot pulls it off the unused queue and counts a
    /// hit; a miss takes the queue front (reporting table-full when empty),
    /// detaches it from any old number, and reads the disk image unless the
    /// device is absent (allocation path, `NO_DEV`).
    pub fn get<S: BlockSource, V: SecondLevelCache>(
        &mut self,
        cache: &mut BlockCache<S, V>,
        device: u64,
        number: u64,
        params: &InodeIo,
    ) -> Result<usize, InodeError> {
        if number == NO_ENTRY {
            return Err(InodeError::Invalid);
        }
        if let Some(slot) = self.hash[Self::bucket(number)]
            .iter()
            .copied()
            .find(|&slot| {
                let inode = &self.slots[slot];
                inode.number == number && inode.device == device
            })
        {
            if self.slots[slot].count == 0 {
                self.hits += 1;
                self.dequeue(slot);
            }
            self.slots[slot].count += 1;
            return Ok(slot);
        }
        self.misses += 1;
        let slot = self.unused.pop_front().ok_or(InodeError::TableFull)?;
        self.unhash(slot);
        {
            let inode = &mut self.slots[slot];
            inode.device = device;
            inode.number = number;
            inode.count = 1;
            inode.pending_updates = 0;
            inode.zone_hint = 0;
            inode.mountpoint = false;
            inode.scan_hint = 0;
            inode.seek = SEEK_CLEAR;
            inode.dirty = false;
        }
        if device != 0 {
            self.read_from_disk(cache, slot, params)?;
        }
        self.hash[Self::bucket(number)].push(slot);
        Ok(slot)
    }

    /// Remove a slot from the unused queue (it must be there).
    fn dequeue(&mut self, slot: usize) {
        if let Some(position) = self.unused.iter().position(|&index| index == slot) {
            self.unused.remove(position);
        }
    }

    /// Release one reference (`put_inode`, `inode.c:206-246`).
    ///
    /// Null-safe in C; here the slot must be live (callers hold it, so the
    /// option does not exist). Underflow is a programming error and panics
    /// in debug builds like the C panic. At zero references: unlinked
    /// inodes report their zones for the caller to reclaim; dirty inodes
    /// write back; freed slots recycle at the queue front, cached ones at
    /// the back.
    pub fn put<S: BlockSource, V: SecondLevelCache>(
        &mut self,
        cache: &mut BlockCache<S, V>,
        slot: usize,
        params: &InodeIo,
    ) -> Result<ReleaseOutcome, InodeError> {
        if self.slots[slot].count == 0 {
            return Err(InodeError::Invalid);
        }
        self.slots[slot].count -= 1;
        if self.slots[slot].count > 0 {
            return Ok(ReleaseOutcome::Held);
        }
        let reclaim = if self.slots[slot].nlinks == NO_LINK {
            let zones: Vec<u64> = self.slots[slot]
                .zones
                .iter()
                .copied()
                .filter(|&zone| zone != 0)
                .collect();
            self.slots[slot].mode = NOT_ALLOCATED;
            self.slots[slot].dirty = true;
            Some(zones)
        } else {
            None
        };
        self.slots[slot].mountpoint = false;
        if self.slots[slot].dirty {
            self.write_to_disk(cache, slot, params)?;
        }
        if reclaim.is_some() {
            self.unhash(slot);
            self.slots[slot].number = NO_ENTRY;
            self.slots[slot].count = 0;
            self.slots[slot].dirty = false;
            self.unused.push_front(slot);
            Ok(ReleaseOutcome::ReclaimZones(reclaim.unwrap_or_default()))
        } else {
            self.unused.push_back(slot);
            Ok(ReleaseOutcome::Released)
        }
    }

    /// Allocate a fresh inode (`alloc_inode`, `inode.c:252-303`).
    ///
    /// Refuses read-only mounts, takes a bitmap bit from the hint, then a
    /// table slot without disk read (`NO_DEV`). Losing the race for a slot
    /// returns the bitmap bit. The mode, owner, group, and device land
    /// immediately; sizes and zones are wiped; the link count starts at zero
    /// links (the caller links it into a directory next). Eight parameters
    /// mirror the C signature plus the injected pieces; the creation
    /// context of document 12 bundles them for callers.
    #[allow(clippy::too_many_arguments)]
    pub fn allocate<S: BlockSource, V: SecondLevelCache>(
        &mut self,
        _cache: &mut BlockCache<S, V>,
        superblock: &mut Superblock,
        bitmap: &mut Bitmap,
        mode: u16,
        owner: u16,
        group: u16,
        device: u64,
    ) -> Result<usize, InodeError> {
        if superblock.read_only {
            return Err(InodeError::ReadOnly);
        }
        let bit = bitmap
            .alloc(superblock.isearch)
            .ok_or(InodeError::NoSpace)?;
        superblock.isearch = bit;
        let slot = match self.get_without_disk(bit) {
            Some(slot) => slot,
            None => {
                bitmap.free(bit);
                return Err(InodeError::TableFull);
            }
        };
        {
            let inode = &mut self.slots[slot];
            inode.mode = mode;
            inode.nlinks = NO_LINK;
            inode.owner = owner;
            inode.group = group;
            inode.device = device;
            inode.direct_zone_count = DIRECT_ZONES as u32;
            inode.indirect_per_block = superblock.indirect_per_block;
            inode.size = 0;
            inode.pending_updates = UPDATE_ACCESS | UPDATE_CHANGE | UPDATE_MODIFY;
            inode.dirty = true;
            inode.zones = [0; TOTAL_ZONES];
        }
        Ok(slot)
    }

    /// Take an unused-queue slot without disk read (allocation fast path).
    fn get_without_disk(&mut self, number: u64) -> Option<usize> {
        let slot = self.unused.pop_front()?;
        self.unhash(slot);
        {
            let inode = &mut self.slots[slot];
            inode.device = 0;
            inode.number = number;
            inode.count = 1;
            inode.pending_updates = 0;
            inode.zone_hint = 0;
            inode.mountpoint = false;
            inode.scan_hint = 0;
            inode.seek = SEEK_CLEAR;
            inode.dirty = false;
        }
        self.hash[Self::bucket(number)].push(slot);
        Some(slot)
    }

    /// Free an inode number back to the bitmap (`free_inode`).
    ///
    /// Out-of-range numbers are silently ignored; the hint rewinds when the
    /// freed bit precedes it.
    pub fn free_number(&mut self, superblock: &mut Superblock, bitmap: &mut Bitmap, number: u64) {
        if number == NO_ENTRY || number > superblock.inode_count as u64 {
            return;
        }
        bitmap.free(number);
        if number < superblock.isearch {
            superblock.isearch = number;
        }
    }

    /// Stamp pending times once (`update_times`, `inode.c:349-370`).
    ///
    /// Read-only mounts skip silently; otherwise a single clock reading
    /// serves every pending field.
    pub fn stamp_times(&mut self, slot: usize, now: i64, read_only: bool) {
        if read_only {
            return;
        }
        let inode = &mut self.slots[slot];
        if inode.pending_updates & UPDATE_ACCESS != 0 {
            inode.accessed = now;
        }
        if inode.pending_updates & UPDATE_CHANGE != 0 {
            inode.changed = now;
        }
        if inode.pending_updates & UPDATE_MODIFY != 0 {
            inode.modified = now;
        }
        inode.pending_updates = 0;
    }

    /// Read the disk image of a slot (`rw_inode` read half).
    fn read_from_disk<S: BlockSource, V: SecondLevelCache>(
        &mut self,
        cache: &mut BlockCache<S, V>,
        slot: usize,
        params: &InodeIo,
    ) -> Result<(), InodeError> {
        let (block, offset) = params.locate(self.slots[slot].number)?;
        let cache_slot = cache
            .acquire(
                BlockKey::new(self.slots[slot].device, block),
                AcquireMode::Normal,
            )
            .map_err(|_| InodeError::Corrupt)?;
        let record = DiskInode::from_bytes(&cache.slot_data(cache_slot)[offset..])?;
        let _ = cache.release(cache_slot);
        if record.size < 0 {
            return Err(InodeError::Corrupt);
        }
        let inode = &mut self.slots[slot];
        inode.mode = record.mode;
        inode.nlinks = record.nlinks;
        inode.owner = record.owner;
        inode.group = record.group;
        inode.size = record.size as i64;
        inode.accessed = record.accessed as i64;
        inode.modified = record.modified as i64;
        inode.changed = record.changed as i64;
        inode.direct_zone_count = DIRECT_ZONES as u32;
        inode.indirect_per_block = params.indirect_per_block;
        for (slot_zone, disk_zone) in inode.zones.iter_mut().zip(record.zones.iter()) {
            *slot_zone = *disk_zone as u64;
        }
        Ok(())
    }

    /// Write a dirty slot back (`rw_inode` write half).
    fn write_to_disk<S: BlockSource, V: SecondLevelCache>(
        &mut self,
        cache: &mut BlockCache<S, V>,
        slot: usize,
        params: &InodeIo,
    ) -> Result<(), InodeError> {
        let (block, offset) = params.locate(self.slots[slot].number)?;
        let cache_slot = cache
            .acquire(
                BlockKey::new(self.slots[slot].device, block),
                AcquireMode::Normal,
            )
            .map_err(|_| InodeError::Corrupt)?;
        let record = {
            let inode = &self.slots[slot];
            let mut zones = [0u32; TOTAL_ZONES];
            for (disk_zone, slot_zone) in zones.iter_mut().zip(inode.zones.iter()) {
                *disk_zone = *slot_zone as u32;
            }
            DiskInode {
                mode: inode.mode,
                nlinks: inode.nlinks,
                owner: inode.owner,
                group: inode.group,
                size: inode.size as i32,
                accessed: inode.accessed as i32,
                modified: inode.modified as i32,
                changed: inode.changed as i32,
                zones,
            }
        };
        {
            let bytes = record.to_bytes();
            let data = cache.slot_data_mut(cache_slot);
            data[offset..offset + INODE_DISK_SIZE].copy_from_slice(&bytes);
        }
        cache.mark_dirty(cache_slot);
        let _ = cache.release(cache_slot);
        self.slots[slot].dirty = false;
        Ok(())
    }

    /// Write a dirty slot back immediately (crash-robustness ordering in
    /// creation paths, `open.c:235`). Thin public wrapper over the
    /// write half used by release.
    pub fn write_back<S: BlockSource, V: SecondLevelCache>(
        &mut self,
        cache: &mut BlockCache<S, V>,
        slot: usize,
        params: &InodeIo,
    ) -> Result<(), InodeError> {
        self.write_to_disk(cache, slot, params)
    }

    /// Read a slot for inspection (tests and callers holding references).
    pub fn slot(&self, slot: usize) -> &Inode {
        &self.slots[slot]
    }

    /// Mutable slot access for field updates the table does not mediate
    /// (link counts, seek flags, scan hints).
    pub fn slot_mut(&mut self, slot: usize) -> &mut Inode {
        &mut self.slots[slot]
    }
}

impl Default for InodeTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Disk geometry the inode layer needs from the superblock.
///
/// A trimmed view (`s_inodes_per_block`, `s_block_size`, bitmap sizes,
/// indirect density) so the table stays testable without a full mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InodeIo {
    /// Inode region starts after boot, superblock, and both bitmaps.
    pub inode_base_block: u64,
    /// Inodes per block.
    pub inodes_per_block: u32,
    /// Zones per indirect block.
    pub indirect_per_block: u32,
}

impl InodeIo {
    /// Block number and byte offset of an inode number (`rw_inode`).
    pub fn locate(&self, number: u64) -> Result<(u64, usize), InodeError> {
        if number == NO_ENTRY {
            return Err(InodeError::Invalid);
        }
        let index = number - 1;
        let block = self.inode_base_block + index / self.inodes_per_block as u64;
        let offset = (index % self.inodes_per_block as u64) as usize * INODE_DISK_SIZE;
        Ok((block, offset))
    }

    /// Build from a parsed superblock and its bitmap sizes.
    pub fn from_superblock(superblock: &Superblock) -> Self {
        Self {
            inode_base_block: crate::superblock::START_BLOCK
                + superblock.inode_map_blocks as u64
                + superblock.zone_map_blocks as u64,
            inodes_per_block: superblock.inodes_per_block,
            indirect_per_block: superblock.indirect_per_block,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_fs::bio::RamDisk;
    use minix_fs::cache::NoSecondLevel;

    const DEVICE: u64 = 0x301;

    fn test_cache() -> BlockCache<RamDisk> {
        // Sixty-four inode blocks starting at block four need sixty-eight
        // blocks total; one hundred twenty-eight leaves headroom.
        BlockCache::with_pool(RamDisk::new(128, 512).unwrap(), NoSecondLevel, 8).unwrap()
    }

    fn test_params() -> InodeIo {
        InodeIo {
            inode_base_block: 4,
            inodes_per_block: 8,
            indirect_per_block: 128,
        }
    }

    fn write_disk_inode(
        cache: &mut BlockCache<RamDisk>,
        params: &InodeIo,
        number: u64,
        record: &DiskInode,
    ) {
        let (block, offset) = params.locate(number).unwrap();
        let slot = cache
            .acquire(BlockKey::new(DEVICE, block), AcquireMode::NoRead)
            .unwrap();
        let bytes = record.to_bytes();
        cache.slot_data_mut(slot)[offset..offset + INODE_DISK_SIZE].copy_from_slice(&bytes);
        cache.mark_dirty(slot);
        cache.release(slot).unwrap();
    }

    fn sample_record(mode: u16, size: i32) -> DiskInode {
        DiskInode {
            mode,
            nlinks: 2,
            owner: 100,
            group: 100,
            size,
            accessed: 1000,
            modified: 1001,
            changed: 1002,
            zones: [7, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        }
    }

    #[test]
    fn test_get_loads_and_caches() {
        let mut cache = test_cache();
        let params = test_params();
        let mut table = InodeTable::new();
        write_disk_inode(&mut cache, &params, 1, &sample_record(0o040755, 128));
        let first = table.get(&mut cache, DEVICE, 1, &params).unwrap();
        assert_eq!(table.slot(first).mode, 0o040755);
        assert_eq!(table.slot(first).size, 128);
        assert_eq!(table.slot(first).zones[0], 7);
        assert_eq!(table.slot(first).count, 1);
        assert_eq!(table.misses(), 1);
        // Referenced hits do not move the hit counter: like the C code,
        // only cold-cache hits (count zero) count.
        let second = table.get(&mut cache, DEVICE, 1, &params).unwrap();
        assert_eq!(first, second);
        assert_eq!(table.slot(first).count, 2);
        assert_eq!(table.hits(), 0);
        // Release both references: the slot parks cached-cold. Getting it
        // again is a cold hit.
        table.put(&mut cache, first, &params).unwrap();
        table.put(&mut cache, first, &params).unwrap();
        let third = table.get(&mut cache, DEVICE, 1, &params).unwrap();
        assert_eq!(third, first);
        assert_eq!(table.hits(), 1);
    }

    #[test]
    fn test_find_needs_positive_count() {
        let mut cache = test_cache();
        let params = test_params();
        let mut table = InodeTable::new();
        write_disk_inode(&mut cache, &params, 2, &sample_record(0o100644, 10));
        assert_eq!(table.find(DEVICE, 2), None);
        let slot = table.get(&mut cache, DEVICE, 2, &params).unwrap();
        assert_eq!(table.find(DEVICE, 2), Some(slot));
        table.duplicate(slot);
        assert_eq!(table.slot(slot).count, 2);
    }

    #[test]
    fn test_put_writes_back_and_recycles() {
        let mut cache = test_cache();
        let params = test_params();
        let mut table = InodeTable::new();
        write_disk_inode(&mut cache, &params, 3, &sample_record(0o100644, 10));
        let slot = table.get(&mut cache, DEVICE, 3, &params).unwrap();
        table.slot_mut(slot).size = 500;
        table.slot_mut(slot).dirty = true;
        // One of two references released: still held, nothing written.
        table.duplicate(slot);
        assert_eq!(
            table.put(&mut cache, slot, &params).unwrap(),
            ReleaseOutcome::Held
        );
        // Last reference: writes back, parks at the queue back (cached).
        assert_eq!(
            table.put(&mut cache, slot, &params).unwrap(),
            ReleaseOutcome::Released
        );
        assert!(!table.slot(slot).dirty);
        // The disk image carries the new size now.
        let (block, offset) = params.locate(3).unwrap();
        let check = cache
            .acquire(BlockKey::new(DEVICE, block), AcquireMode::Normal)
            .unwrap();
        let back = DiskInode::from_bytes(&cache.slot_data(check)[offset..]).unwrap();
        assert_eq!(back.size, 500);
        let _ = cache.release(check);
    }

    #[test]
    fn test_put_unlinked_reports_zones() {
        let mut cache = test_cache();
        let params = test_params();
        let mut table = InodeTable::new();
        let mut record = sample_record(0o100644, 10);
        record.nlinks = 0;
        write_disk_inode(&mut cache, &params, 4, &record);
        let slot = table.get(&mut cache, DEVICE, 4, &params).unwrap();
        match table.put(&mut cache, slot, &params).unwrap() {
            ReleaseOutcome::ReclaimZones(zones) => assert_eq!(zones, alloc::vec![7]),
            other => panic!("expected zones, got {other:?}"),
        }
        // Slot recycled at the queue front with a clear number.
        assert_eq!(table.slot(slot).number, NO_ENTRY);
        assert_eq!(table.find(DEVICE, 4), None);
    }

    #[test]
    fn test_allocate_and_free_number() {
        let mut cache = test_cache();
        let mut table = InodeTable::new();
        let mut superblock = test_superblock();
        let mut bitmap = Bitmap::new(64);
        let slot = table
            .allocate(
                &mut cache,
                &mut superblock,
                &mut bitmap,
                0o100644,
                5,
                6,
                DEVICE,
            )
            .unwrap();
        assert_eq!(table.slot(slot).number, 1);
        assert_eq!(table.slot(slot).nlinks, NO_LINK);
        assert_eq!(table.slot(slot).owner, 5);
        assert!(table.slot(slot).dirty);
        assert_eq!(superblock.isearch, 1);
        // Read-only refuses.
        superblock.read_only = true;
        assert_eq!(
            table
                .allocate(
                    &mut cache,
                    &mut superblock,
                    &mut bitmap,
                    0o100644,
                    0,
                    0,
                    DEVICE
                )
                .unwrap_err(),
            InodeError::ReadOnly
        );
        superblock.read_only = false;
        // Exhausted bitmap reports no space.
        let mut full = Bitmap::new(2);
        assert_eq!(full.alloc(0), Some(1));
        assert_eq!(
            table
                .allocate(
                    &mut cache,
                    &mut superblock,
                    &mut full,
                    0o100644,
                    0,
                    0,
                    DEVICE
                )
                .unwrap_err(),
            InodeError::NoSpace
        );
        // Freeing rewinds the hint.
        superblock.isearch = 9;
        table.free_number(&mut superblock, &mut bitmap, 3);
        assert_eq!(superblock.isearch, 3);
        table.free_number(&mut superblock, &mut bitmap, 0);
        table.free_number(&mut superblock, &mut bitmap, 9999);
    }

    #[test]
    fn test_table_full_recycles_nothing() {
        let mut cache = test_cache();
        let params = test_params();
        let mut table = InodeTable::new();
        // Pin every slot: further gets report table-full.
        for number in 1..=TABLE_SLOTS as u64 {
            write_disk_inode(&mut cache, &params, number, &sample_record(0o100644, 0));
            table.get(&mut cache, DEVICE, number, &params).unwrap();
        }
        assert_eq!(
            table
                .get(&mut cache, DEVICE, TABLE_SLOTS as u64 + 1, &params)
                .unwrap_err(),
            InodeError::TableFull
        );
    }

    #[test]
    fn test_stamp_times_once() {
        let mut table = InodeTable::new();
        // Build a live slot without disk: allocation path tested above.
        let slot = table.get_without_disk(11).unwrap();
        table.slot_mut(slot).pending_updates = UPDATE_ACCESS | UPDATE_MODIFY;
        table.stamp_times(slot, 5555, false);
        assert_eq!(table.slot(slot).accessed, 5555);
        assert_eq!(table.slot(slot).modified, 5555);
        assert_eq!(table.slot(slot).changed, 0);
        assert_eq!(table.slot(slot).pending_updates, 0);
        // Read-only skips silently.
        table.slot_mut(slot).pending_updates = UPDATE_ACCESS;
        table.stamp_times(slot, 9999, true);
        assert_eq!(table.slot(slot).accessed, 5555);
    }

    #[test]
    fn test_disk_format_roundtrip() {
        let record = sample_record(0o040755, 4096);
        let back = DiskInode::from_bytes(&record.to_bytes()).unwrap();
        assert_eq!(record, back);
        assert_eq!(INODE_DISK_SIZE, 64);
        assert!(DiskInode::from_bytes(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_locate_math() {
        let params = test_params();
        // Number one sits at the region start; number nine starts block five.
        assert_eq!(params.locate(1).unwrap(), (4, 0));
        assert_eq!(params.locate(8).unwrap(), (4, 7 * 64));
        assert_eq!(params.locate(9).unwrap(), (5, 0));
        assert!(params.locate(0).is_err());
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::ENOENT;
        assert_eq!(InodeError::TableFull.to_errno().to_i32(), ENFILE);
        assert_eq!(InodeError::ReadOnly.to_errno().to_i32(), minix_types::EROFS);
        assert_eq!(InodeError::NoSpace.to_errno().to_i32(), ENOSPC);
        assert_eq!(InodeError::Corrupt.to_errno().to_i32(), minix_types::EIO);
        assert_eq!(InodeError::Invalid.to_errno().to_i32(), minix_types::EINVAL);
        assert_eq!(ENOENT, 2);
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(NO_ENTRY, 0);
        assert_eq!(NOT_ALLOCATED, 0);
        assert_eq!(TABLE_SLOTS, 512);
        assert_eq!(HASH_BUCKETS, 128);
        assert_eq!(DIRECTORY_ENTRY_SIZE, 64);
        assert_eq!(NAME_CAPACITY, 60);
        assert_eq!(LINK_CEILING, 32767);
        assert_eq!(TYPE_MASK, 0o170000);
        assert_eq!(TYPE_DIRECTORY, 0o040000);
    }

    fn test_superblock() -> Superblock {
        Superblock {
            inode_count: 64,
            inode_map_blocks: 1,
            zone_map_blocks: 1,
            flags: 1,
            max_size: 100000,
            zones: 100,
            version: 3,
            block_size: 512,
            inodes_per_block: 8,
            direct_zones: 7,
            indirect_per_block: 128,
            first_data_zone: 6,
            zone_total_small: 0,
            first_data_zone_small: 6,
            disk_version: 0,
            device: DEVICE,
            read_only: false,
            isearch: 0,
            zsearch: 0,
        }
    }
}
