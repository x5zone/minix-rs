//! Server assembly: one MFS server value wired to the framework trait.
//!
//! C correspondence: the mounted globals of `main.c` (`fs_dev`,
//! `super_block`) plus the callback surface `table.c` wires, assembled into
//! a single value. The stage functions own the decisions; this module owns
//! the plumbing: it derives the geometry from the superblock, loads and
//! stores directory images around the walk, keeps the allocation maps and
//! search hints coherent, and turns stage errors into wire errors.
//!
//! Reference discipline: every method leaves the inode table exactly as the
//! C entry points do — cold inodes load through the table, references taken
//! along the way are released on every path, and the child of a successful
//! create stays referenced for the virtual file system to release later.
//! Clock reads arrive through an injected function because a library cannot
//! know where the time comes from; the runtime wiring supplies the real
//! source when the process starts.
//!
//! Every method opens with the same move: split the mount into its
//! [`Parts`]. The stage functions take the table, the cache, the maps, and
//! the geometry as separate parameters, and the split hands them out from
//! one `&mut MountedFs` without aliasing.

use alloc::vec::Vec;

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, NoSecondLevel};
use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};

use minix_types::{EBADF, EBUSY, EIO, EINVAL, ENOENT, Errno};

use crate::dir::DirError;
use crate::dir_io::{load_dir_blocks, store_dir_blocks};
use crate::inode::{InodeIo, InodeTable, TABLE_SLOTS};

use crate::mfs_cache::ZoneSpace;
use crate::mount::{check_mountpoint, MountedFs};
use crate::open::CreateCtx;
use crate::read::{list_dir_entries, map_file_block, read_file, FileParams, MapParams, ZoneRange};
use crate::superblock::Bitmap;
use crate::write::{truncate_file, write_file};

/// Byte layout of the status reply filled by [`FsDriver::stat`]: mode,
/// nlinks, owner, group (two bytes each), then device, size, access,
/// modify, change, block size, block count (eight bytes each), little
/// endian throughout — sixty-four bytes. The C adapter pre-fills the
/// device and inode fields in the reply (`call.c:732-738`); this server
/// writes every field into the buffer the transport offers.
pub const STAT_LAYOUT_SIZE: usize = 64;

/// Where the server reads the wall clock. A function pointer because the
/// server needs exactly one time source and never inspects it; the runtime
/// wiring supplies the real one when the process starts.
pub type Clock = fn() -> i64;

/// The neutral clock: zero for every read. Tests stay deterministic.
pub const fn zero_clock() -> i64 {
    0
}

/// Disjoint mutable pieces of one mounted file system.
///
/// The stage functions take the table, the cache, the maps, and the
/// geometry as separate parameters; a caller reaching through accessors
/// cannot hand them out without aliasing, so the mount splits itself. The
/// geometry copies ride along because every stage call needs them.
pub struct Parts<'a, S: BlockSource> {
    /// Mounted device number.
    pub device: u64,
    /// Whether the mount is read-only.
    pub read_only: bool,
    /// Largest file size in bytes.
    pub max_size: u64,
    /// Block size in bytes.
    pub block_size: usize,
    /// Inode geometry for disk transfers.
    pub io: InodeIo,
    /// Mapping geometry (direct zones, indirect density).
    pub map: MapParams,
    /// Valid zone range for indirect validation.
    pub range: ZoneRange,
    /// Superblock (search hints move as allocation runs).
    pub superblock: &'a mut crate::superblock::Superblock,
    /// Block cache.
    pub cache: &'a mut BlockCache<S, NoSecondLevel>,
    /// Inode table.
    pub inodes: &'a mut InodeTable,
    /// Inode allocation map.
    pub imap: &'a mut Bitmap,
    /// Zone allocation map.
    pub zmap: &'a mut Bitmap,
    /// Zone allocation policy state.
    pub space: &'a mut ZoneSpace,
}

/// The zone allocator pair the write path takes, over one map behind a
/// shared cell. The stage signature wants two closures at once and never
/// calls them re-entrantly, so the map sits behind a run-time borrow that
/// each call takes and drops.
fn zone_closures<'a, 'b>(
    cell: &'b core::cell::RefCell<(&'a mut Bitmap, &'a mut u64)>,
    first_data_zone: u64,
) -> (
    impl FnMut(u64) -> Option<u64> + 'b,
    impl FnMut(u64) + 'b,
) {
    let alloc = move |hint: u64| -> Option<u64> {
        let mut guard = cell.borrow_mut();
        let (zmap, zsearch) = &mut *guard;
        let hint = hint.saturating_sub(first_data_zone.saturating_sub(1));
        let bit = zmap.alloc(hint)?;
        **zsearch = bit;
        Some(bit)
    };
    let free = move |bit: u64| {
        let mut guard = cell.borrow_mut();
        let (zmap, zsearch) = &mut *guard;
        if zmap.free(bit) && bit < **zsearch {
            **zsearch = bit;
        }
    };
    (alloc, free)
}

/// One assembled MFS server: a detached device or a mounted file system.
pub struct MfsServer<S: BlockSource> {
    /// Device storage between mounts (taken by mount, returned by unmount).
    source: Option<S>,
    /// The mounted file system, when one is up.
    fs: Option<MountedFs<S>>,
    /// Buffer pool size from the boot configuration.
    pool_buffers: usize,
    /// Wall-clock source for timestamp decisions.
    clock: Clock,
}

impl<S: BlockSource> MfsServer<S> {
    /// A server around a device with the standard boot configuration.
    pub fn new(source: S) -> Self {
        Self::with_pool(source, crate::startup::DEFAULT_POOL_BUFFERS, zero_clock)
    }

    /// A server with an explicit buffer pool size and clock.
    pub const fn with_pool(source: S, pool_buffers: usize, clock: Clock) -> Self {
        Self {
            source: Some(source),
            fs: None,
            pool_buffers,
            clock,
        }
    }

    /// Whether a file system is currently mounted.
    pub const fn is_mounted(&self) -> bool {
        self.fs.is_some()
    }

    /// Take the detached device back (shutdown handback, remount tests).
    pub fn take_source(&mut self) -> Option<S> {
        self.source.take()
    }

    /// The mounted state, refused when detached. The framework gate keeps
    /// this path cold for every request but the mount; the answer matches
    /// the gate.
    fn mounted(&mut self) -> Result<&mut MountedFs<S>, Errno> {
        self.fs.as_mut().ok_or_else(|| Errno::from_i32(EINVAL))
    }

    /// Sync one mount in the `fs_sync` order (`misc.c:8-23`): dirty inodes
    /// first (their bytes stage in the block cache), then the allocation
    /// maps (staged the same way), then every dirty block. The search hint
    /// copies back into the superblock, which is where the disk image gets
    /// it from.
    fn sync_mounted(fs: &mut MountedFs<S>) -> Result<(), Errno> {
        {
            let parts = fs.parts();
            for slot in 0..TABLE_SLOTS {
                if parts.inodes.slot(slot).dirty {
                    parts
                        .inodes
                        .write_back(parts.cache, slot, &parts.io)
                        .map_err(|_| Errno::from_i32(EIO))?;
                }
            }
            parts.superblock.zsearch = parts.space.zsearch;
        }
        fs.store_bitmaps().map_err(|error| error.to_errno())?;
        fs.cache_mut().flush_all()?;
        Ok(())
    }

    /// Load a parent directory's images for the walk-and-modify entry
    /// points, releasing the caller's reference once the bytes are copied
    /// out. Returns the images and the size they were loaded against (the
    /// append boundary the store half needs).
    fn load_parent(
        fs: &mut MountedFs<S>,
        directory: u64,
    ) -> Result<(Vec<Vec<u8>>, u64), Errno> {
        let parts = fs.parts();
        let dir_slot = parts
            .inodes
            .get(parts.cache, parts.device, directory, &parts.io)
            .map_err(|_| Errno::from_i32(ENOENT))?;
        let (zones, old_size) = {
            let dir = parts.inodes.slot(dir_slot);
            (dir.zones, dir.size as u64)
        };
        let blocks = load_dir_blocks(
            parts.cache,
            parts.device,
            &zones,
            parts.map,
            parts.range,
            old_size,
            parts.block_size,
        )?;
        let _ = parts.inodes.put(parts.cache, dir_slot, &parts.io);
        Ok((blocks, old_size))
    }

    /// Store a parent directory's images back: mapping updates land in the
    /// directory's zone slots, then the caller's reference goes back.
    fn store_parent(
        fs: &mut MountedFs<S>,
        directory: u64,
        old_size: u64,
        blocks: &mut [Vec<u8>],
    ) -> Result<(), Errno> {
        let parts = fs.parts();
        let dir_slot = parts
            .inodes
            .get(parts.cache, parts.device, directory, &parts.io)
            .map_err(|_| Errno::from_i32(ENOENT))?;
        let first = parts.superblock.first_data_zone;
        let cell = core::cell::RefCell::new((&mut *parts.zmap, &mut parts.superblock.zsearch));
        let (mut alloc_bit, mut free_bit) = zone_closures(&cell, first);
        let result = store_dir_blocks(
            parts.cache,
            parts.device,
            &mut parts.inodes.slot_mut(dir_slot).zones,
            parts.map,
            parts.range,
            parts.space,
            &mut alloc_bit,
            &mut free_bit,
            old_size,
            &*blocks,
            parts.block_size,
        );
        let _ = parts.inodes.put(parts.cache, dir_slot, &parts.io);
        result
    }
}

impl<S: BlockSource> FsDriver for MfsServer<S> {
    fn mount(
        &mut self,
        device: u64,
        flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        let source = self.source.take().ok_or_else(|| Errno::from_i32(EBUSY))?;
        match crate::mount::mount(source, device, flags, self.pool_buffers) {
            Ok((fs, node)) => {
                // C wires no capability bits for MFS: the framework only
                // ever adds the peek flag (`call.c:48-51`), which needs peek
                // entry points or no backing device — this server has the
                // device and not the entry points, so nothing is added.
                *capabilities = CapabilityFlags::EMPTY;
                self.fs = Some(fs);
                Ok(node)
            }
            Err(error) => Err(error.to_errno()),
        }
    }

    fn unmounted(&mut self) {
        if let Some(fs) = self.fs.take() {
            let result = crate::mount::unmount(fs, &mut |fs: &mut MountedFs<S>| {
                let _ = Self::sync_mounted(fs);
            });
            if let Ok((_, source)) = result {
                self.source = Some(source);
            }
        }
    }

    fn put_node(&mut self, inode: u64, count: u32) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        parts
            .inodes
            .put_count(parts.cache, parts.device, inode, count, &parts.io)
            .map(|_| ())
            .map_err(|error| error.to_errno())
    }

    fn is_mount_point(&mut self, inode: u64) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        check_mountpoint(
            parts.inodes,
            parts.cache,
            &parts.io,
            parts.device,
            inode,
        )
        .map_err(|error| error.to_errno())
    }

    fn read(
        &mut self,
        inode: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        let params = FileParams {
            map: parts.map,
            range: parts.range,
            block_size: parts.block_size,
            read_only: parts.read_only,
        };
        read_file(
            parts.inodes,
            parts.cache,
            parts.device,
            inode,
            position,
            length,
            params,
            out,
        )
        .map_err(|error| error.to_errno())
    }

    fn write(&mut self, inode: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        let slot = parts
            .inodes
            .find(parts.device, inode)
            .ok_or_else(|| Errno::from_i32(EBADF))?;
        let first = parts.superblock.first_data_zone;
        let cell = core::cell::RefCell::new((&mut *parts.zmap, &mut parts.superblock.zsearch));
        let (mut alloc_bit, mut free_bit) = zone_closures(&cell, first);
        write_file(
            parts.inodes,
            parts.cache,
            parts.space,
            &mut alloc_bit,
            &mut free_bit,
            parts.device,
            slot,
            parts.map,
            parts.range,
            parts.block_size,
            parts.max_size,
            parts.read_only,
            position,
            data,
        )
        .map_err(|error| error.to_errno())
    }

    fn get_dents(
        &mut self,
        inode: u64,
        position: &mut i64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if *position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let fs = self.mounted()?;
        let parts = fs.parts();
        let params = FileParams {
            map: parts.map,
            range: parts.range,
            block_size: parts.block_size,
            read_only: parts.read_only,
        };
        let mut resume = *position as u64;
        let moved = list_dir_entries(
            parts.inodes,
            parts.cache,
            parts.device,
            inode,
            &mut resume,
            capacity,
            &params,
            out,
        )
        .map_err(|error| error.to_errno())?;
        *position = resume as i64;
        Ok(moved)
    }

    fn truncate(&mut self, inode: u64, start: i64, end: i64) -> Result<(), Errno> {
        if start < 0 || end < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let fs = self.mounted()?;
        let parts = fs.parts();
        let slot = parts
            .inodes
            .find(parts.device, inode)
            .ok_or_else(|| Errno::from_i32(EBADF))?;
        if end == 0 {
            // Whole-file truncate down to `start` (`link.c:439-441`).
            let first = parts.superblock.first_data_zone;
            let cell = core::cell::RefCell::new((&mut *parts.zmap, &mut parts.superblock.zsearch));
            let (mut alloc_bit, mut free_bit) = zone_closures(&cell, first);
            return truncate_file(
                parts.inodes,
                parts.cache,
                parts.space,
                &mut alloc_bit,
                &mut free_bit,
                parts.device,
                slot,
                parts.map,
                parts.range,
                parts.block_size,
                parts.max_size,
                start as u64,
            )
            .map_err(|error| error.to_errno());
        }
        // Punch `[start, end)` (`link.c:442`, the `freesp` leg): plan, zero
        // the partial edges through the cache, free the whole zones through
        // the mapping.
        let size = parts.inodes.slot(slot).size as u64;
        let plan = crate::link::plan_free_range(
            size,
            parts.block_size as u64,
            start as u64,
            end as u64,
        )
        .map_err(|_| Errno::from_i32(EINVAL))?;
        for (range_start, length) in &plan.zero_ranges {
            let mut remaining = *length;
            let mut position = *range_start;
            while remaining > 0 {
                let offset = (position % parts.block_size as u64) as usize;
                let chunk = (parts.block_size - offset).min(remaining as usize);
                let file_block = position / parts.block_size as u64;
                let zones = parts.inodes.slot(slot).zones;
                let mapped = map_file_block(
                    parts.cache,
                    parts.device,
                    &zones,
                    parts.map,
                    parts.range,
                    file_block,
                )
                .map_err(|_| Errno::from_i32(EIO))?;
                if let Some(zone) = mapped {
                    let cache_slot = parts
                        .cache
                        .acquire(BlockKey::new(parts.device, zone), AcquireMode::Normal)
                        .map_err(|_| Errno::from_i32(EIO))?;
                    {
                        let data = parts.cache.slot_data_mut(cache_slot);
                        for byte in data[offset..offset + chunk].iter_mut() {
                            *byte = 0;
                        }
                    }
                    parts.cache.mark_dirty(cache_slot);
                    let _ = parts.cache.release(cache_slot);
                }
                position += chunk as u64;
                remaining -= chunk as u64;
            }
        }
        for &zone in &plan.free_zones {
            // Free through the mapping: the file block whose map entry names
            // this zone clears both the entry and the bitmap bit.
            let first_block = start as u64 / parts.block_size as u64;
            let last_block = end as u64 / parts.block_size as u64;
            for file_block in first_block..last_block {
                let zones = parts.inodes.slot(slot).zones;
                if map_file_block(
                    parts.cache,
                    parts.device,
                    &zones,
                    parts.map,
                    parts.range,
                    file_block,
                )
                .map_err(|_| Errno::from_i32(EIO))?
                    == Some(zone)
                {
                    let first = parts.superblock.first_data_zone;
                    let cell =
                        core::cell::RefCell::new((&mut *parts.zmap, &mut parts.superblock.zsearch));
                    let (mut alloc_bit, mut free_bit) = zone_closures(&cell, first);
                    crate::write::write_map(
                        &mut parts.inodes.slot_mut(slot).zones,
                        parts.map,
                        parts.space,
                        &mut alloc_bit,
                        &mut free_bit,
                        parts.cache,
                        parts.device,
                        parts.range,
                        file_block * parts.block_size as u64,
                        parts.block_size,
                        0,
                        true,
                    )
                    .map_err(|error| error.to_errno())?;
                }
            }
        }
        Ok(())
    }

    fn sought(&mut self, inode: u64) {
        if let Some(fs) = self.fs.as_mut() {
            let parts = fs.parts();
            crate::open::mark_seek(parts.inodes, parts.device, inode);
        }
    }

    fn lookup_child(&mut self, directory: u64, name: &str) -> Result<(FileNode, bool), Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        let dir_slot = parts
            .inodes
            .get(parts.cache, parts.device, directory, &parts.io)
            .map_err(|_| Errno::from_i32(ENOENT))?;
        let (mode, nlinks, size, zones) = {
            let dir = parts.inodes.slot(dir_slot);
            (dir.mode, dir.nlinks, dir.size as u64, dir.zones)
        };
        let mut blocks = load_dir_blocks(
            parts.cache,
            parts.device,
            &zones,
            parts.map,
            parts.range,
            size,
            parts.block_size,
        )?;
        let mut found = 0u32;
        let search = crate::dir::lookup_name(
            mode,
            nlinks,
            &mut blocks,
            parts.block_size,
            size,
            name.as_bytes(),
            &mut found,
        );
        let _ = parts.inodes.put(parts.cache, dir_slot, &parts.io);
        search.map_err(|error: DirError| error.to_errno())?;
        let child_slot = parts
            .inodes
            .get(parts.cache, parts.device, found as u64, &parts.io)
            .map_err(|_| Errno::from_i32(ENOENT))?;
        let child = parts.inodes.slot(child_slot);
        let node = FileNode::new(
            child.number,
            child.mode as u32,
            child.size,
            child.owner as u32,
            child.group as u32,
            0,
        );
        let mount_point = child.mountpoint;
        Ok((node, mount_point))
    }

    fn create(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<FileNode, Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        let (file, _) = {
            let parts = fs.parts();
            let mut ctx = CreateCtx {
                table: parts.inodes,
                cache: parts.cache,
                superblock: parts.superblock,
                bitmap: parts.imap,
                device: parts.device,
                io: parts.io,
                read_only: parts.read_only,
            };
            crate::open::create_file(
                &mut ctx,
                directory,
                &mut blocks,
                name.as_bytes(),
                mode as u16,
                owner as u16,
                group as u16,
            )
            .map_err(|error| error.to_errno())?
        };
        Self::store_parent(fs, directory, old_size, blocks.as_mut_slice())?;
        Ok(FileNode::new(
            file.ino,
            file.mode as u32,
            file.size,
            file.owner as u32,
            file.group as u32,
            0,
        ))
    }

    fn stat(&mut self, inode: u64, out: &mut [u8]) -> Result<(), Errno> {
        if out.len() < STAT_LAYOUT_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        let now = (self.clock)();
        let fs = self.mounted()?;
        let parts = fs.parts();
        let stat = crate::meta::read_stat(
            parts.inodes,
            parts.cache,
            &parts.io,
            parts.device,
            inode,
            parts.block_size as u64,
            parts.map.direct_zones,
            parts.map.indirect_per_block,
            now,
            parts.read_only,
        )
        .map_err(|error| error.to_errno())?;
        out[0..2].copy_from_slice(&stat.mode.to_le_bytes());
        out[2..4].copy_from_slice(&stat.nlinks.to_le_bytes());
        out[4..6].copy_from_slice(&stat.owner.to_le_bytes());
        out[6..8].copy_from_slice(&stat.group.to_le_bytes());
        out[8..16].copy_from_slice(&stat.device.to_le_bytes());
        out[16..24].copy_from_slice(&stat.size.to_le_bytes());
        out[24..32].copy_from_slice(&stat.accessed.to_le_bytes());
        out[32..40].copy_from_slice(&stat.modified.to_le_bytes());
        out[40..48].copy_from_slice(&stat.changed.to_le_bytes());
        out[48..56].copy_from_slice(&stat.block_size.to_le_bytes());
        out[56..64].copy_from_slice(&stat.blocks.to_le_bytes());
        Ok(())
    }

    fn synchronized(&mut self) {
        if let Some(fs) = self.fs.as_mut() {
            let _ = Self::sync_mounted(fs);
        }
    }

    fn flushed(&mut self, device: u64) {
        if let Some(fs) = self.fs.as_mut() {
            let _ = fs.cache_mut().flush_device(device);
            fs.cache_mut().invalidate_device(device);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inode::DiskInode;
    use crate::superblock::{SUPER_BLOCK_OFFSET, FLAG_CLEAN, MAGIC_V3};
    use minix_fs::bio::RamDisk;

    const DEVICE: u64 = 0x301;
    const BLOCK_SIZE: usize = 4096;
    const INODE_COUNT: u32 = 64;
    const ZONE_TOTAL: u64 = 100;

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

    /// A minimal well-formed MFS image: superblock, two one-block bitmaps,
    /// one inode block, and a root directory holding the dot entries.
    fn build_image() -> RamDisk {
        let first_data_zone = 2 + 1 + 1 + (INODE_COUNT as u64).div_ceil(64);
        let mut disk = RamDisk::new(ZONE_TOTAL as usize, BLOCK_SIZE).unwrap();
        let superblock = crate::superblock::Superblock {
            inode_count: INODE_COUNT,
            inode_map_blocks: 1,
            zone_map_blocks: 1,
            flags: FLAG_CLEAN,
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
            disk.block_mut(0).expect("block zero exists")
                [SUPER_BLOCK_OFFSET..SUPER_BLOCK_OFFSET + bytes.len()]
                .copy_from_slice(&bytes);
        }
        // Bitmaps: inode bit one and zone bit one (the root's zone).
        disk.block_mut(2).expect("imap block")[0] |= 1 << 1;
        disk.block_mut(3).expect("zmap block")[0] |= 1 << 1;
        // Root inode record.
        {
            let bytes = root_record().to_bytes();
            disk.block_mut(4).expect("inode block")[0..64].copy_from_slice(&bytes);
        }
        // Root directory block: dot and dot-dot, both inode one.
        {
            let block = disk.block_mut(5).expect("root block exists");
            block[0..4].copy_from_slice(&1u32.to_le_bytes());
            block[4..5].copy_from_slice(b".");
            block[64..68].copy_from_slice(&1u32.to_le_bytes());
            block[68..70].copy_from_slice(b"..");
        }
        let _ = MAGIC_V3;
        disk
    }

    fn flags(read_only: bool) -> MountFlags {
        if read_only {
            MountFlags::READ_ONLY
        } else {
            MountFlags::EMPTY
        }
    }

    #[test]
    fn test_mount_create_write_read_sync_unmount_remount() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        let root = server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        assert_eq!(root.inode_number, 1);
        assert!(server.is_mounted());

        // Lookup through the assembled directory walk: the root finds itself.
        let (node, mount_point) = server.lookup_child(1, ".").unwrap();
        assert_eq!(node.inode_number, 1);
        assert!(!mount_point);

        // Create a file and write through the assembled write path.
        let file = server
            .create(1, "hello", 0o100644, 0, 0)
            .unwrap();
        assert_eq!(file.inode_number, 2);
        let moved = server.write(2, 0, b"abc").unwrap();
        assert_eq!(moved, 3);

        // Read the bytes back through the assembled read path.
        let mut seen = Vec::new();
        let got = server
            .read(2, 0, 16, &mut |bytes: &[u8]| seen.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(got, 3);
        assert_eq!(seen, b"abc".to_vec());

        // Directory enumeration reports the new name.
        let mut listed = Vec::new();
        let mut position = 0i64;
        let listed_bytes = server
            .get_dents(1, &mut position, 1024, &mut |bytes: &[u8]| {
                listed.extend_from_slice(bytes)
            })
            .unwrap();
        // The position resumes at the directory's byte offset (three
        // sixty-four-byte slots after the create); the listing itself is
        // the packed caller format: dot entries round to sixteen bytes,
        // the five-byte name to twenty-four.
        assert_eq!(position, 3 * 64);
        assert_eq!(listed_bytes, 16 + 16 + 24);
        assert!(listed.windows(5).any(|window| window == b"hello"));

        // Status reports the written size.
        let mut stat = [0u8; STAT_LAYOUT_SIZE];
        server.stat(2, &mut stat).unwrap();
        assert_eq!(stat[16..24], 3i64.to_le_bytes());

        // Sync flushes; unmount returns the device for a later mount.
        server.synchronized();
        server.unmounted();
        assert!(!server.is_mounted());
        let source = server.take_source().unwrap();
        // The written bytes survive on the device the server handed back.
        let mut block = alloc::vec![0u8; BLOCK_SIZE];
        source
            .read_block(BlockKey::new(DEVICE, 6), &mut block)
            .unwrap();
        assert_eq!(&block[..3], b"abc");

        // Remount on the handed-back device: the file is still there.
        let mut server = MfsServer::with_pool(source, 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        let (node, _) = server.lookup_child(1, "hello").unwrap();
        assert_eq!(node.inode_number, 2);
        let mut seen = Vec::new();
        let got = server
            .read(2, 0, 16, &mut |bytes: &[u8]| seen.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(got, 3);
        assert_eq!(seen, b"abc".to_vec());
    }

    #[test]
    fn test_put_node_releases_and_refuses_overcount() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        server.create(1, "temp", 0o100644, 0, 0).unwrap();
        // The create leaves one reference: releasing it retires the slot.
        server.put_node(2, 1).unwrap();
        let error = server
            .read(2, 0, 4, &mut |bytes: &[u8]| {
                let _ = bytes;
            })
            .unwrap_err();
        // The read path answers invalid for a released number, the same
        // error the C read half reports when `get_inode` fails.
        assert_eq!(error.to_i32(), EINVAL);
        // A count past the reference total is refused untouched.
        assert!(server.put_node(1, 5).is_err());
        let (node, _) = server.lookup_child(1, ".").unwrap();
        assert_eq!(node.inode_number, 1);
    }

    #[test]
    fn test_unmounted_server_refuses_reads() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut seen = Vec::new();
        let error = server
            .read(1, 0, 8, &mut |bytes: &[u8]| seen.extend_from_slice(bytes))
            .unwrap_err();
        assert_eq!(error.to_i32(), EINVAL);
        let _ = seen;
    }
}
