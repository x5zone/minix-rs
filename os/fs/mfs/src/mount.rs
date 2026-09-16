//! Mount lifecycle: mount, unmount, and mount-point check.
//!
//! C correspondence: `minix3/minix/fs/mfs/mount.c` (all one hundred
//! seventy-three lines): `fs_mount`, `fs_mountpt`, `fs_unmount`. This is the
//! first end-to-end slice of the disk server: a real image mounts, serves
//! its root, and unmounts cleanly, using only pieces from documents 04, 05,
//! 07, 08, and 09. Device open and close belong to the block-device stage
//! (the ramdisk needs none); the sync callback is injected because cache
//! flushing lives in the maintenance stage (document 17).
//!
//! Design note on ownership. The C code keeps the mounted state in globals
//! (`fs_dev`, `superblock`); here one [`MountedFs`] value owns the device
//! number, the superblock, the cache, and the inode table together. Two
//! mounts in one test process stay independent, which the globals forbid.
//! This mirrors the Linux superblock object (one value per mounted instance)
//! and the Redox mount handle (acquire and release as a unit).

use minix_types::{EBUSY, EINVAL, EIO, ENOTDIR, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, NoSecondLevel};
use minix_fs::protocol::{FileNode, MountFlags};

use crate::inode::{InodeError, InodeTable, ReleaseOutcome, TYPE_BLOCK, TYPE_CHARACTER};
use crate::mfs_cache::ZoneSpace;
use crate::superblock::{
    Bitmap, DiskSuperblock, FLAG_CLEAN, ROOT_INODE_NUMBER, START_BLOCK, SUPER_BLOCK_OFFSET,
    SuperError, Superblock, parse_superblock,
};

/// Why mounting failed. Every variant maps to the wire code the C mount
/// path returns at the same decision point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountError {
    /// Superblock unreadable or rejected (maps the inner reason).
    Super(SuperError),
    /// Cache or inode failure along the way.
    Inner(Errno),
    /// Root inode missing after a good parse (`mount.c:63-68`).
    RootMissing,
    /// Root inode has a zero mode (`mount.c:70-76`).
    RootZeroMode,
    /// Cache block size differs from the image block size.
    BlockSizeMismatch,
    /// Mount point already taken (`EBUSY`, `mount.c:117`).
    Busy,
    /// Mount point is a device node (`ENOTDIR`, `mount.c:121`).
    NotDirectory,
}

impl MountError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Super(error) => error.to_errno(),
            Self::Inner(error) => error,
            Self::RootMissing => Errno::from_i32(EINVAL),
            Self::RootZeroMode => Errno::from_i32(EINVAL),
            Self::BlockSizeMismatch => Errno::from_i32(EINVAL),
            Self::Busy => Errno::from_i32(EBUSY),
            Self::NotDirectory => Errno::from_i32(ENOTDIR),
        }
    }
}

impl From<SuperError> for MountError {
    fn from(error: SuperError) -> Self {
        Self::Super(error)
    }
}

impl From<Errno> for MountError {
    fn from(error: Errno) -> Self {
        Self::Inner(error)
    }
}

impl From<InodeError> for MountError {
    fn from(error: InodeError) -> Self {
        Self::Inner(error.to_errno())
    }
}

/// One mounted file system: device, superblock, cache, and inode table.
///
/// Created by [`mount`], consumed by [`unmount`]. The root slot index is
/// kept so unmount finds the same slot without a second lookup.
#[derive(Debug)]
pub struct MountedFs<S: BlockSource> {
    device: u64,
    superblock: Superblock,
    cache: BlockCache<S, NoSecondLevel>,
    inodes: InodeTable,
    /// Live allocation-policy state (search hints move as zones allocate).
    space: ZoneSpace,
    /// Inode allocation map, loaded at mount, written back at sync.
    imap: Bitmap,
    /// Zone allocation map, loaded at mount, written back at sync.
    zmap: Bitmap,
    read_only: bool,
    downgraded: bool,
    root_slot: usize,
}

impl<S: BlockSource> MountedFs<S> {
    /// Mounted device number.
    pub const fn device(&self) -> u64 {
        self.device
    }

    /// Whether the mount is read-only (requested or downgraded).
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether an unclean image forced a read-only downgrade.
    pub const fn was_downgraded(&self) -> bool {
        self.downgraded
    }

    /// Shared superblock access.
    pub const fn superblock(&self) -> &Superblock {
        &self.superblock
    }

    /// Shared cache access.
    pub const fn cache(&self) -> &BlockCache<S, NoSecondLevel> {
        &self.cache
    }

    /// Exclusive cache access (write-back, invalidation, tests).
    pub fn cache_mut(&mut self) -> &mut BlockCache<S, NoSecondLevel> {
        &mut self.cache
    }

    /// Shared inode table access.
    pub const fn inodes(&self) -> &InodeTable {
        &self.inodes
    }

    /// Root inode number (always one).
    pub const fn root_number() -> u64 {
        ROOT_INODE_NUMBER
    }

    /// Exclusive inode table access.
    pub fn inodes_mut(&mut self) -> &mut InodeTable {
        &mut self.inodes
    }

    /// Exclusive superblock access (search hints move as allocation runs).
    pub fn superblock_mut(&mut self) -> &mut Superblock {
        &mut self.superblock
    }

    /// The root slot's table index.
    pub const fn root_slot(&self) -> usize {
        self.root_slot
    }

    /// The inode geometry for disk transfers.
    pub fn io(&self) -> crate::inode::InodeIo {
        crate::inode::InodeIo::from_superblock(&self.superblock)
    }

    /// Shared inode map access.
    pub const fn imap(&self) -> &Bitmap {
        &self.imap
    }

    /// Exclusive inode map access.
    pub fn imap_mut(&mut self) -> &mut Bitmap {
        &mut self.imap
    }

    /// Shared zone map access.
    pub const fn zmap(&self) -> &Bitmap {
        &self.zmap
    }

    /// Exclusive zone map access.
    pub fn zmap_mut(&mut self) -> &mut Bitmap {
        &mut self.zmap
    }

    /// Disjoint mutable pieces of the mount, one bundle for the data path.
    ///
    /// The stage functions take the table, the cache, the maps, and the
    /// geometry as separate parameters; this splitter hands them out from
    /// one `&mut MountedFs` without aliasing, which a caller reaching
    /// through accessors cannot do.
    pub fn parts(
        &mut self,
    ) -> crate::server::Parts<'_, S> {
        crate::server::Parts {
            device: self.device,
            read_only: self.read_only,
            max_size: self.superblock.max_size as u64,
            block_size: self.superblock.block_size,
            io: crate::inode::InodeIo::from_superblock(&self.superblock),
            map: crate::read::MapParams {
                direct_zones: self.superblock.direct_zones,
                indirect_per_block: self.superblock.indirect_per_block,
            },
            range: crate::read::ZoneRange {
                first: self.superblock.first_data_zone,
                count: self
                    .superblock
                    .zones
                    .saturating_sub(self.superblock.first_data_zone)
                    + 1,
            },
            superblock: &mut self.superblock,
            cache: &mut self.cache,
            inodes: &mut self.inodes,
            imap: &mut self.imap,
            zmap: &mut self.zmap,
            space: &mut self.space,
        }
    }

    /// Write both allocation maps back into their image blocks and mark
    /// them dirty. The block pass of a flush carries them to disk, the same
    /// point where the C code's dirty bitmap blocks land.
    pub fn store_bitmaps(&mut self) -> Result<(), MountError> {
        let start = START_BLOCK;
        let imap_blocks = self.superblock.inode_map_blocks as u64;
        let zmap_blocks = self.superblock.zone_map_blocks as u64;
        let bits_per_block = (self.superblock.block_size * 8) as u64;
        let device = self.device;
        for index in 0..imap_blocks {
            Self::store_map(
                &mut self.cache,
                &self.imap,
                device,
                start + index,
                index * bits_per_block,
            )?;
        }
        for index in 0..zmap_blocks {
            Self::store_map(
                &mut self.cache,
                &self.zmap,
                device,
                start + imap_blocks + index,
                index * bits_per_block,
            )?;
        }
        Ok(())
    }

    /// Write one map block: the map slice `[base, base + block bits)` goes
    /// into the cached block as a whole-block overwrite (no-read acquire),
    /// marked dirty. The flush carries it to disk, the same point where the
    /// C code's dirty bitmap blocks land.
    fn store_map(
        cache: &mut BlockCache<S, NoSecondLevel>,
        map: &Bitmap,
        device: u64,
        block: u64,
        base: u64,
    ) -> Result<(), MountError> {
        let slot = cache
            .acquire(BlockKey::new(device, block), AcquireMode::NoRead)
            .map_err(|_| MountError::Inner(Errno::from_i32(EIO)))?;
        {
            let data = cache.slot_data_mut(slot);
            map.store_image_at(base, data);
        }
        cache.mark_dirty(slot);
        let _ = cache.release(slot);
        Ok(())
    }

    /// Load both allocation maps from their image blocks (the read pass of
    /// `mount`, before any allocation can run).
    fn load_bitmaps(&mut self) -> Result<(), MountError> {
        let start = START_BLOCK;
        let imap_blocks = self.superblock.inode_map_blocks as u64;
        let zmap_blocks = self.superblock.zone_map_blocks as u64;
        let bits_per_block = (self.superblock.block_size * 8) as u64;
        let device = self.device;
        for index in 0..imap_blocks {
            Self::load_map(
                &mut self.cache,
                &mut self.imap,
                device,
                start + index,
                index * bits_per_block,
            )?;
        }
        for index in 0..zmap_blocks {
            Self::load_map(
                &mut self.cache,
                &mut self.zmap,
                device,
                start + imap_blocks + index,
                index * bits_per_block,
            )?;
        }
        Ok(())
    }

    /// Read one map block into the map slice `[base, base + block bits)`.
    fn load_map(
        cache: &mut BlockCache<S, NoSecondLevel>,
        map: &mut Bitmap,
        device: u64,
        block: u64,
        base: u64,
    ) -> Result<(), MountError> {
        let slot = cache
            .acquire(BlockKey::new(device, block), AcquireMode::Normal)
            .map_err(|_| MountError::Inner(Errno::from_i32(EIO)))?;
        let image = cache.slot_data(slot).to_vec();
        let _ = cache.release(slot);
        map.load_image_at(base, &image);
        Ok(())
    }
}

/// Mount a device: parse, check, size, root, dirty-mark.
///
/// C: `fs_mount` (`mount.c:10-98`). Steps in C order: remember the device;
/// read the superblock (failure tears everything down and reports the
/// reason); unclean plus read-write downgrades to read-only (no device
/// reopen here — that belongs to the driver stage); adopt the image block
/// size (mismatch with the cache is refused); count used zones for the
/// cache heuristic; fetch the root inode (missing or zero-mode root tears
/// down); record read-only; report the root properties with an absent
/// device number; mark dirty unless read-only.
///
/// Device open and close are skipped: the ramdisk needs none, and real
/// driver open/close arrive with the block-device stage. The teardown order
/// (invalidate, forget the device) is mirrored by dropping cache state on
/// error paths — every error below leaves no half-mounted value behind.
pub fn mount<S: BlockSource>(
    source: S,
    device: u64,
    flags: MountFlags,
    pool_buffers: usize,
) -> Result<(MountedFs<S>, FileNode), MountError> {
    let mut cache =
        BlockCache::with_pool(source, NoSecondLevel, pool_buffers).map_err(MountError::from)?;
    let requested_read_only = flags.is_read_only();

    // Fetch block zero and parse at the superblock offset (`rw_super` read
    // half plus `read_super`, `super.c:173-236` and `241-355`).
    let image = {
        let slot = cache
            .acquire(BlockKey::new(device, 0), AcquireMode::Normal)
            .map_err(|_| MountError::Inner(Errno::from_i32(EIO)))?;
        let bytes = cache.slot_data(slot).to_vec();
        let _ = cache.release(slot);
        bytes
    };
    let mut superblock = parse_superblock(&image, device, requested_read_only)?;

    // Clean check with downgrade (`mount.c:39-50`).
    let mut downgraded = false;
    let mut read_only = requested_read_only;
    if superblock.flags & FLAG_CLEAN == 0 && !read_only {
        read_only = true;
        downgraded = true;
    }
    superblock.read_only = read_only;

    // Adopt the image block size (`mount.c:52`).
    if cache.source_block_size() != superblock.block_size {
        return Err(MountError::BlockSizeMismatch);
    }

    // Usage counts for the cache heuristic (`mount.c:54-60`).
    let free = count_free_bits(&superblock, &mut cache, crate::superblock::MAP_ZONE)?;
    let total = superblock.zones;
    let used = total.saturating_sub(free);
    cache.set_usage(total, used).map_err(MountError::from)?;

    // Root inode (`mount.c:62-76`).
    let mut inodes = InodeTable::new();
    let io = crate::inode::InodeIo::from_superblock(&superblock);
    let root_slot = inodes
        .get(&mut cache, device, ROOT_INODE_NUMBER, &io)
        .map_err(|_| MountError::RootMissing)?;
    if inodes.slot(root_slot).mode == 0 {
        return Err(MountError::RootZeroMode);
    }
    let root = inodes.slot(root_slot);
    let node = FileNode::new(
        root.number,
        root.mode as u32,
        root.size,
        root.owner as u32,
        root.group as u32,
        0,
    );

    // Dirty-mark unless read-only (`mount.c:90-95`).
    if !read_only {
        superblock.flags &= !FLAG_CLEAN;
        store_superblock(&mut cache, device, &superblock)?;
    }

    // Allocation maps, loaded fresh from the image (`mount.c` keeps its
    // bitmaps in the cache blocks; the in-memory maps here are loaded at
    // mount and written back at sync — the flush point is the same).
    let imap_bits = superblock.inode_count as u64 + 1;
    let zmap_bits =
        superblock
            .zones
            .saturating_sub(superblock.first_data_zone.saturating_sub(1));
    let space = crate::mount::zone_space(&superblock);
    let mut mounted = MountedFs {
        device,
        superblock,
        cache,
        inodes,
        space,
        imap: Bitmap::new(imap_bits),
        zmap: Bitmap::new(zmap_bits),
        read_only,
        downgraded,
        root_slot,
    };
    mounted.load_bitmaps()?;
    Ok((mounted, node))
}

/// Count free bits in a bitmap (`count_free_bits`, `stats.c`, owned here
/// until document 17 canonicalizes it).
///
/// Walks the bitmap blocks from [`START_BLOCK`] and counts clear bits
/// within range. Bit zero of the inode map reads set on a well-formed
/// image; out-of-range tail bits read set by construction.
pub fn count_free_bits<S: BlockSource>(
    superblock: &Superblock,
    cache: &mut BlockCache<S, NoSecondLevel>,
    map: u32,
) -> Result<u64, MountError> {
    let zone_map = match map {
        crate::superblock::MAP_ZONE => true,
        crate::superblock::MAP_INODE => false,
        _ => return Err(MountError::Inner(Errno::from_i32(EINVAL))),
    };
    let (start, blocks, bits) = if zone_map {
        (
            START_BLOCK + superblock.inode_map_blocks as u64,
            superblock.zone_map_blocks as u64,
            superblock
                .zones
                .saturating_sub(superblock.first_data_zone.saturating_sub(1)),
        )
    } else {
        (
            START_BLOCK,
            superblock.inode_map_blocks as u64,
            superblock.inode_count as u64 + 1,
        )
    };
    let block_size = superblock.block_size;
    let mut free = 0u64;
    for index in 0..blocks {
        let slot = cache
            .acquire(
                BlockKey::new(superblock.device, start + index),
                AcquireMode::Normal,
            )
            .map_err(|_| MountError::Inner(Errno::from_i32(EIO)))?;
        let data = cache.slot_data(slot);
        for (word_index, chunk) in data.chunks(4).enumerate() {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            for bit in 0..32u64 {
                let number = index * block_size as u64 * 8 + word_index as u64 * 32 + bit;
                if number >= bits {
                    break;
                }
                if word & (1 << bit) == 0 {
                    free += 1;
                }
            }
        }
        let _ = cache.release(slot);
    }
    Ok(free)
}

/// Write the superblock image back to block zero.
///
/// Rebuilds the thirteen stored fields, copies them at the superblock
/// offset, marks dirty, and flushes — the write half of `rw_super` plus the
/// `write_super` guard, with the read-only refusal as an error.
pub fn store_superblock<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    superblock: &Superblock,
) -> Result<(), MountError> {
    crate::superblock::check_writable(superblock.read_only).map_err(MountError::from)?;
    let slot = cache
        .acquire(BlockKey::new(device, 0), AcquireMode::Normal)
        .map_err(|_| MountError::Inner(Errno::from_i32(EIO)))?;
    {
        let bytes = superblock.rebuild_disk().to_bytes();
        let data = cache.slot_data_mut(slot);
        if data.len() < SUPER_BLOCK_OFFSET + DiskSuperblock::STORED_BYTES {
            let _ = cache.release(slot);
            return Err(MountError::Inner(Errno::from_i32(EIO)));
        }
        data[SUPER_BLOCK_OFFSET..SUPER_BLOCK_OFFSET + DiskSuperblock::STORED_BYTES]
            .copy_from_slice(&bytes);
    }
    cache.mark_dirty(slot);
    let _ = cache.release(slot);
    cache.flush_device(device).map_err(MountError::from)?;
    Ok(())
}

/// Unmount report: integrity observations, never failures.
///
/// Unmount succeeds either way (the virtual file system service expects
/// it); surprises are reported, not errored — except a lost root, which
/// means memory corruption.
pub struct UnmountReport {
    /// In-use references found (one, the root held once, is clean).
    pub busy_count: u32,
    /// Whether the mount had been downgraded to read-only.
    pub was_downgraded: bool,
}

/// Unmount: release the root, sync, mark clean, invalidate.
///
/// C: `fs_unmount` (`mount.c:134-172`). Counts in-use references (one root
/// held once is clean; anything else is reported); releases the root (a
/// missing root is corruption); runs the injected sync; marks clean and
/// writes back unless read-only; invalidates the device cache. Device close
/// belongs to the driver stage.
pub fn unmount<S: BlockSource>(
    mut mounted: MountedFs<S>,
    sync: &mut dyn FnMut(&mut MountedFs<S>),
) -> Result<(UnmountReport, S), MountError> {
    let device = mounted.device;
    let mut busy_count = 0u32;
    // Count in-use slots on this device (`mount.c:144-148`).
    for slot in 0..crate::inode::TABLE_SLOTS {
        let inode = mounted.inodes.slot(slot);
        if inode.count > 0 && inode.device == device {
            busy_count += inode.count;
        }
    }
    let root = mounted
        .inodes
        .find(device, ROOT_INODE_NUMBER)
        .ok_or(MountError::RootMissing)?;
    // The stored slot must agree with a fresh lookup; both name the root.
    debug_assert_eq!(root, mounted.root_slot);
    let io = crate::inode::InodeIo::from_superblock(&mounted.superblock);
    match mounted.inodes.put(&mut mounted.cache, root, &io) {
        Ok(ReleaseOutcome::Released) | Ok(ReleaseOutcome::Held) => {}
        Ok(ReleaseOutcome::ReclaimZones(_)) => return Err(MountError::RootMissing),
        Err(error) => return Err(error.into()),
    }
    sync(&mut mounted);
    if !mounted.superblock.read_only {
        mounted.superblock.flags |= FLAG_CLEAN;
        store_superblock(&mut mounted.cache, device, &mounted.superblock)?;
    }
    mounted.cache.invalidate_device(device);
    let report = UnmountReport {
        busy_count,
        was_downgraded: mounted.downgraded,
    };
    // Hand the device back so the server can mount again: the C server
    // keeps its storage handle across unmounts.
    let source = mounted.cache.into_source();
    Ok((report, source))
}

/// Check a mount point candidate (`fs_mountpt`, `mount.c:104-128`).
///
/// Opens the inode when needed, then three checks in C order: already a
/// mount point is busy; device nodes are not directories; otherwise the
/// flag is set and the answer is success. The inode is released on every
/// path.
pub fn check_mountpoint<S: BlockSource>(
    inodes: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    device: u64,
    number: u64,
) -> Result<(), MountError> {
    let slot = inodes
        .get(cache, device, number, io)
        .map_err(|_| MountError::RootMissing)?;
    let (mountpoint, file_type) = {
        let inode = inodes.slot(slot);
        (
            inode.mountpoint,
            inode.mode as u32 & crate::inode::TYPE_MASK,
        )
    };
    let mut answer = Ok(());
    if mountpoint {
        answer = Err(MountError::Busy);
    } else if file_type == TYPE_BLOCK || file_type == TYPE_CHARACTER {
        answer = Err(MountError::NotDirectory);
    }
    let _ = inodes.put(cache, slot, io);
    if answer.is_ok() {
        inodes.slot_mut(slot).mountpoint = true;
    }
    answer
}

/// Zone parameters for the allocation policy, derived from a superblock.
pub fn zone_space(superblock: &Superblock) -> ZoneSpace {
    ZoneSpace {
        first_data_zone: superblock.first_data_zone,
        zone_count: superblock.zones,
        zsearch: superblock.zsearch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_fs::bio::RamDisk;
    use minix_fs::cache::MIN_POOL_SIZE;

    use crate::inode::{DiskInode, InodeIo};
    use crate::superblock::{Bitmap, MAGIC_V3};

    const DEVICE: u64 = 0x301;
    const BLOCK_SIZE: usize = 4096;
    const INODE_COUNT: u32 = 64;
    const ZONE_TOTAL: u64 = 100;

    /// A minimal well-formed image: superblock, two one-block bitmaps, one
    /// inode block, root directory block. Layout with sixty-four inodes of
    /// sixty-four bytes: inode blocks start at block four, first data zone
    /// is block five; zone five holds the root directory.
    struct Image {
        disk: RamDisk,
    }

    fn root_record() -> DiskInode {
        let mut zones = [0u32; 10];
        zones[0] = 5;
        DiskInode {
            mode: 0o040755,
            nlinks: 3,
            owner: 0,
            group: 0,
            size: 128,
            accessed: 0,
            modified: 0,
            changed: 0,
            zones,
        }
    }

    fn build_image() -> Image {
        let first_data_zone = 2 + 1 + 1 + (INODE_COUNT as u64).div_ceil(64);
        assert_eq!(first_data_zone, 5);
        let mut disk = RamDisk::new(ZONE_TOTAL as usize, BLOCK_SIZE).unwrap();
        // Superblock at block zero plus offset.
        let superblock = Superblock {
            inode_count: INODE_COUNT,
            inode_map_blocks: 1,
            zone_map_blocks: 1,
            flags: crate::superblock::FLAG_CLEAN,
            max_size: 1_000_000,
            zones: ZONE_TOTAL,
            version: 3,
            block_size: BLOCK_SIZE,
            inodes_per_block: 64,
            direct_zones: 7,
            indirect_per_block: 1024,
            first_data_zone,
            zone_total_small: 0,
            first_data_zone_small: 0,
            disk_version: 0,
            device: DEVICE,
            read_only: false,
            isearch: 0,
            zsearch: 0,
        };
        {
            let bytes = superblock.rebuild_disk().to_bytes();
            let block = disk.block_mut(0).expect("block zero exists");
            block[SUPER_BLOCK_OFFSET..SUPER_BLOCK_OFFSET + bytes.len()].copy_from_slice(&bytes);
        }
        // Bitmaps: inode bit one, zone bit one (zone five maps to bit one).
        {
            let mut imap = Bitmap::new(INODE_COUNT as u64 + 1);
            assert_eq!(imap.alloc(0), Some(1));
            write_bitmap(&mut disk, 2, &imap);
            let mut zmap = Bitmap::new(ZONE_TOTAL - (first_data_zone - 1));
            assert_eq!(zmap.alloc(0), Some(1));
            write_bitmap(&mut disk, 3, &zmap);
        }
        // Root inode record at block four offset zero.
        {
            let bytes = root_record().to_bytes();
            disk.block_mut(4).expect("inode block exists")[0..64].copy_from_slice(&bytes);
        }
        // Root directory block five: dot and dot-dot, both to inode one.
        {
            let block = disk.block_mut(5).expect("root block exists");
            block[0..4].copy_from_slice(&1u32.to_le_bytes());
            block[4..5].copy_from_slice(b".");
            block[64..68].copy_from_slice(&1u32.to_le_bytes());
            block[68..70].copy_from_slice(b"..");
        }
        // Sanity: magic lands where the parser looks.
        let magic_at = {
            let block = disk.block_mut(0).expect("block zero exists");
            u16::from_le_bytes([
                block[SUPER_BLOCK_OFFSET + 24],
                block[SUPER_BLOCK_OFFSET + 25],
            ])
        };
        assert_eq!(magic_at, MAGIC_V3);
        Image { disk }
    }

    fn write_bitmap(disk: &mut RamDisk, block: usize, map: &Bitmap) {
        // Serialize the private words through the public probe interface.
        let target = disk.block_mut(block).expect("bitmap block exists");
        for byte in target.iter_mut() {
            *byte = 0;
        }
        for bit in 0..map.bit_count() {
            if map.test(bit) {
                target[bit as usize / 8] |= 1 << (bit % 8);
            }
        }
    }

    fn flags(read_only: bool) -> MountFlags {
        if read_only {
            MountFlags::READ_ONLY
        } else {
            MountFlags::EMPTY
        }
    }

    #[test]
    fn test_mount_serves_root_and_unmounts_clean() {
        let image = build_image();
        let (mounted, node) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        assert_eq!(node.inode_number, 1);
        assert_eq!(node.mode, 0o040755);
        assert_eq!(node.size, 128);
        assert!(!mounted.is_read_only());
        assert!(!mounted.was_downgraded());
        // Mounting dirties the superblock: the clean flag is cleared.
        assert_eq!(mounted.superblock().flags & FLAG_CLEAN, 0);
        let mut synced = false;
        let (report, source) = unmount(mounted, &mut |_| synced = true).unwrap();
        assert!(synced);
        assert_eq!(report.busy_count, 1);
        assert!(!report.was_downgraded);
        // The device comes back: a server can mount again after unmount.
        assert_eq!(source.block_count(), ZONE_TOTAL as usize);
        assert_eq!(source.block_size(), BLOCK_SIZE);
    }

    #[test]
    fn test_bitmaps_loaded_from_image_and_stored_back() {
        let image = build_image();
        let (mut mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        // The image marks zone bit one (root directory zone five).
        assert!(mounted.zmap().test(1));
        assert!(mounted.imap().test(1));
        // Allocate through the map, write the maps back, and read the raw
        // blocks: the new bit must survive the round trip.
        let bit = mounted.zmap_mut().alloc(2).unwrap();
        mounted.store_bitmaps().unwrap();
        let slot = mounted
            .cache_mut()
            .acquire(BlockKey::new(DEVICE, 3), AcquireMode::Normal)
            .unwrap();
        let byte = mounted.cache().slot_data(slot)[bit as usize / 8];
        let _ = mounted.cache_mut().release(slot);
        assert_eq!(byte & (1 << (bit % 8)), 1 << (bit % 8));
    }

    #[test]
    fn test_clean_mark_persists_to_image() {
        let image = build_image();
        let (mut mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        // Mounting cleared the flag in memory; write a clean mark through
        // the same store path unmount uses, then read the raw block back.
        let device = mounted.device();
        let mut marked = *mounted.superblock();
        marked.flags |= FLAG_CLEAN;
        store_superblock(mounted.cache_mut(), device, &marked).unwrap();
        let slot = mounted
            .cache_mut()
            .acquire(BlockKey::new(device, 0), AcquireMode::Normal)
            .unwrap();
        let flag_byte = mounted.cache().slot_data(slot)[SUPER_BLOCK_OFFSET + 14];
        assert_eq!(flag_byte & 1, 1);
        let _ = mounted.cache_mut().release(slot);
    }

    #[test]
    fn test_unclean_downgrades_to_read_only() {
        let mut image = build_image();
        // Clear the clean flag in the image.
        image.disk.block_mut(0).expect("block zero exists")[SUPER_BLOCK_OFFSET + 14] &= !1;
        let (mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        assert!(mounted.is_read_only());
        assert!(mounted.was_downgraded());
    }

    #[test]
    fn test_bad_images_rejected() {
        // Zeroed image: bad magic.
        let disk = RamDisk::new(ZONE_TOTAL as usize, BLOCK_SIZE).unwrap();
        assert!(matches!(
            mount(disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap_err(),
            MountError::Super(_)
        ));
        // Wrong cache block size.
        let image = build_image();
        let (mut mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        let _ = &mut mounted;
        // Tiny pool refused.
        let image = build_image();
        assert!(mount(image.disk, DEVICE, flags(false), 2).is_err());
    }

    #[test]
    fn test_root_zero_mode_rejected() {
        let mut image = build_image();
        // Zero the root mode in the image.
        image.disk.block_mut(4).expect("inode block exists")[0..2].copy_from_slice(&[0, 0]);
        assert_eq!(
            mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap_err(),
            MountError::RootZeroMode
        );
    }

    #[test]
    fn test_mountpoint_transitions() {
        let image = build_image();
        let (mut mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        let device = mounted.device();
        let io = InodeIo::from_superblock(mounted.superblock());
        // Root is a directory: first check succeeds and latches the flag.
        check_mountpoint(&mut mounted.inodes, &mut mounted.cache, &io, device, 1).unwrap();
        // Second check reports busy.
        assert_eq!(
            check_mountpoint(&mut mounted.inodes, &mut mounted.cache, &io, device, 1).unwrap_err(),
            MountError::Busy
        );
    }

    #[test]
    fn test_mountpoint_rejects_device_nodes() {
        let mut image = build_image();
        // Plant a block-device record at inode five and set its bitmap bit,
        // mirroring what the allocator would have written.
        let mut record = root_record();
        record.mode = 0o060000;
        record.nlinks = 1;
        let bytes = record.to_bytes();
        // Inode five lives in block four at record index four.
        image.disk.block_mut(4).expect("inode block exists")[4 * 64..5 * 64]
            .copy_from_slice(&bytes);
        image.disk.block_mut(2).expect("bitmap block exists")[0] |= 1 << 5;
        let (mut mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        let device = mounted.device();
        let io = InodeIo::from_superblock(mounted.superblock());
        assert_eq!(
            check_mountpoint(&mut mounted.inodes, &mut mounted.cache, &io, device, 5).unwrap_err(),
            MountError::NotDirectory
        );
    }

    #[test]
    fn test_count_free_bits_math() {
        let image = build_image();
        let (mut mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        let superblock = *mounted.superblock();
        let free =
            count_free_bits(&superblock, &mut mounted.cache, crate::superblock::MAP_ZONE).unwrap();
        // Ninety-six zone bits total; bits zero (reserved) and one (zone
        // five) read set: ninety-four free.
        assert_eq!(free, 96 - 2);
        let free_inodes = count_free_bits(
            &superblock,
            &mut mounted.cache,
            crate::superblock::MAP_INODE,
        )
        .unwrap();
        // Sixty-five inode bits; bits zero and one read set.
        assert_eq!(free_inodes, 65 - 2);
    }

    #[test]
    fn test_zone_space_view() {
        let image = build_image();
        let (mounted, _) = mount(image.disk, DEVICE, flags(false), MIN_POOL_SIZE).unwrap();
        let space = zone_space(mounted.superblock());
        assert_eq!(space.first_data_zone, 5);
        assert_eq!(space.zone_count, ZONE_TOTAL);
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EBUSY, EINVAL, ENOTDIR};
        assert_eq!(MountError::RootMissing.to_errno().to_i32(), EINVAL);
        assert_eq!(MountError::RootZeroMode.to_errno().to_i32(), EINVAL);
        assert_eq!(MountError::BlockSizeMismatch.to_errno().to_i32(), EINVAL);
        assert_eq!(MountError::Busy.to_errno().to_i32(), EBUSY);
        assert_eq!(MountError::NotDirectory.to_errno().to_i32(), ENOTDIR);
        assert_eq!(FLAG_CLEAN, 1);
    }
}
