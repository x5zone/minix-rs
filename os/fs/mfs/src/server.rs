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

use alloc::boxed::Box;
use alloc::vec::Vec;

use minix_fs::bio::{bio_transfer, TransferDirection};
use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, VmCacheWire};
use minix_fs::bio::DeviceInfo;
use minix_fs::data::{DataChannel, MemoryBackend};
use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};
use minix_types::{Stat, StatVfs};

use minix_types::{EBADF, EBUSY, EIO, EINVAL, ENOENT, Errno};

use crate::dir::DirError;
use crate::link::LinkCtx;
use crate::dir_io::{load_dir_blocks, store_dir_blocks};
use crate::inode::{InodeIo, InodeTable, TABLE_SLOTS};
use crate::second_level::MfsSecondLevel;

use crate::mfs_cache::ZoneSpace;
use crate::mount::{check_mountpoint, MountedFs};
use crate::open::CreateCtx;
use crate::read::{list_dir_entries, map_file_block, read_file, FileParams, MapParams, ZoneRange};
use crate::superblock::Bitmap;
use crate::write::{truncate_file, write_file};

/// Device facts for raw transfers: the usable size comes from the mounted
/// superblock's zone total, the label binding is a framework no-op here.
struct SuperblockBytes {
    bytes: u64,
}

impl DeviceInfo for SuperblockBytes {
    fn partition_size_bytes(&self, _device: u64) -> Result<u64, Errno> {
        Ok(self.bytes)
    }

    fn bind_label(&mut self, _device: u64, _label: &str) {}
}

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
    pub cache: &'a mut BlockCache<S, MfsSecondLevel>,
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
    /// How a mount reaches the virtual-memory page cache, or `None` to keep
    /// the page cache out (C's `may_use_vmcache(0)`; the MFS start-up grants
    /// it with `lmfs_may_use_vmcache(1)`,
    /// `minix3/minix/fs/mfs/main.c:52`).
    ///
    /// A factory rather than a value because a wire carries the channel to
    /// the memory server and a mount consumes it: every mount builds its own.
    vm_wire: Option<fn() -> Box<dyn VmCacheWire>>,
}

impl<S: BlockSource> MfsServer<S> {
    /// A server around a device with the standard boot configuration.
    pub fn new(source: S) -> Self {
        Self::with_pool(source, crate::startup::DEFAULT_POOL_BUFFERS, zero_clock)
    }

    /// A server with an explicit buffer pool size and clock, page cache off.
    pub const fn with_pool(source: S, pool_buffers: usize, clock: Clock) -> Self {
        Self {
            source: Some(source),
            fs: None,
            pool_buffers,
            clock,
            vm_wire: None,
        }
    }

    /// A server whose mounts use the virtual-memory page cache.
    ///
    /// The boot configuration's `use_vmcache` decides
    /// ([`crate::startup::BootConfig`]); the pool turns the second level off
    /// again by itself when its block size is not a whole page
    /// (`cache.c:1236-1239`).
    pub const fn with_vm_cache(
        source: S,
        pool_buffers: usize,
        clock: Clock,
        vm_wire: fn() -> Box<dyn VmCacheWire>,
    ) -> Self {
        Self {
            source: Some(source),
            fs: None,
            pool_buffers,
            clock,
            vm_wire: Some(vm_wire),
        }
    }

    /// The second level a mount starts from.
    fn mount_second_level(&self) -> MfsSecondLevel {
        match self.vm_wire {
            Some(wire) => MfsSecondLevel::vm(wire()),
            None => MfsSecondLevel::off(),
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

    /// Reclaim the data zones of a fully released inode (the `ReclaimZones`
    /// outcome of a final release): free each zone's bitmap bit and drop the
    /// cached copy, so the next allocation can reuse the space. Zone numbers
    /// of zero are absent slots.
    fn reclaim_zones(fs: &mut MountedFs<S>, zones: &[u64]) {
        let parts = fs.parts();
        let first = parts.superblock.first_data_zone;
        let cell = core::cell::RefCell::new((&mut *parts.zmap, &mut parts.superblock.zsearch));
        let (mut _alloc_bit, mut free_bit) = zone_closures(&cell, first);
        for &zone in zones {
            if zone != 0 {
                crate::mfs_cache::free_zone(parts.cache, parts.device, parts.space, zone, &mut free_bit);
            }
        }
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
        let second_level = self.mount_second_level();
        match crate::mount::mount_with_second_level(source, device, flags, self.pool_buffers, second_level) {
            Ok((fs, node)) => {
                // MFS declares a peek entry point in C (`table.c:20`
                // `.fdr_peek = fs_readwrite`), so the framework's
                // negotiation adds HAS_PEEK; nothing else is set.
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
        let outcome = parts
            .inodes
            .put_count(parts.cache, parts.device, inode, count, &parts.io)
            .map_err(|error| error.to_errno())?;
        // A final release on an unlinked inode carries its data zones: the
        // executor frees the bitmap bits and the cached copies here, the
        // same point where the C `put_inode` runs `truncate_inode`.
        if let crate::inode::ReleaseOutcome::ReclaimZones(zones) = outcome {
            Self::reclaim_zones(fs, &zones);
        }
        Ok(())
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

    fn peek(&mut self, inode: u64, position: i64, length: usize) -> Result<usize, Errno> {
        // C 的 peek 入口就是读写路径本身（`table.c:20`
        // `.fdr_peek = fs_readwrite`）：FSC_PEEK 分支按普通读走块、暖
        // 缓存，只是不向调用方拷贝数据（`read.c:156-159`）。这里的实现
        // 同理：读一遍、字节丢弃，返回字节数（不报新位置）。
        let mut sink = |bytes: &[u8]| {
            let _ = bytes;
        };
        self.read(inode, position, length, &mut sink)
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
            &parts.io,
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

    fn make_node(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        let created = {
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
            crate::open::create_device(
                &mut ctx,
                directory,
                &mut blocks,
                name.as_bytes(),
                mode as u16,
                owner as u16,
                group as u16,
                device,
            )
            .map_err(|error| error.to_errno())?
        };
        Self::store_parent(fs, directory, old_size, &mut blocks)?;
        let _ = created;
        Ok(())
    }

    fn make_dir(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        let mut child_blocks: Vec<Vec<u8>> = Vec::new();
        let created = {
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
            crate::open::create_dir(
                &mut ctx,
                directory,
                &mut blocks,
                name.as_bytes(),
                mode as u16,
                owner as u16,
                group as u16,
                &mut child_blocks,
            )
            .map_err(|error| error.to_errno())?
        };
        Self::store_parent(fs, directory, old_size, &mut blocks)?;
        // The child's dot-entry images map onto a fresh zone here: the child
        // was created at size zero, so every image is an append through the
        // mapping (`create_dir` set the size itself).
        {
            let parts = fs.parts();
            let child_slot = parts
                .inodes
                .get(parts.cache, parts.device, created.number, &parts.io)
                .map_err(|_| Errno::from_i32(ENOENT))?;
            let first = parts.superblock.first_data_zone;
            let cell = core::cell::RefCell::new((&mut *parts.zmap, &mut parts.superblock.zsearch));
            let (mut alloc_bit, mut free_bit) = zone_closures(&cell, first);
            let stored = store_dir_blocks(
                parts.cache,
                parts.device,
                &mut parts.inodes.slot_mut(child_slot).zones,
                parts.map,
                parts.range,
                parts.space,
                &mut alloc_bit,
                &mut free_bit,
                0,
                &child_blocks,
                parts.block_size,
            );
            let _ = parts.inodes.put(parts.cache, child_slot, &parts.io);
            stored?;
        }
        Ok(())
    }

    fn link(
        &mut self,
        directory: u64,
        name: &str,
        inode: u64,
    ) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        {
            let parts = fs.parts();
            let mut ctx = LinkCtx {
                table: parts.inodes,
                cache: parts.cache,
                device: parts.device,
                io: parts.io,
                read_only: parts.read_only,
                block_size: parts.block_size,
                map: parts.map,
                range: parts.range,
            };
            crate::link::create_link(
                &mut ctx,
                directory,
                &mut blocks,
                name.as_bytes(),
                inode,
            )
            .map_err(|error| error.to_errno())?;
        }
        Self::store_parent(fs, directory, old_size, &mut blocks)?;
        Ok(())
    }

    fn unlink(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        let outcome = {
            let parts = fs.parts();
            let mut ctx = LinkCtx {
                table: parts.inodes,
                cache: parts.cache,
                device: parts.device,
                io: parts.io,
                read_only: parts.read_only,
                block_size: parts.block_size,
                map: parts.map,
                range: parts.range,
            };
            crate::link::remove_file(&mut ctx, directory, &mut blocks, name.as_bytes())
                .map_err(|error| error.to_errno())?
        };
        Self::store_parent(fs, directory, old_size, &mut blocks)?;
        Self::reclaim_zones(fs, &outcome.reclaimed);
        Ok(())
    }

    fn remove_dir(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        // Locate the child in the parent images so its own images load for
        // the emptiness walk (`remove_dir` reads the child before deleting).
        let mut found = 0u32;
        {
            let parts = fs.parts();
            let dir_slot = parts
                .inodes
                .get(parts.cache, parts.device, directory, &parts.io)
                .map_err(|_| Errno::from_i32(ENOENT))?;
            let (mode, nlinks, size) = {
                let dir = parts.inodes.slot(dir_slot);
                (dir.mode, dir.nlinks, dir.size as u64)
            };
            crate::dir::lookup_name(
                mode,
                nlinks,
                &mut blocks,
                parts.block_size,
                size,
                name.as_bytes(),
                &mut found,
            )
            .map_err(|error| error.to_errno())?;
            let _ = parts.inodes.put(parts.cache, dir_slot, &parts.io);
        }
        let (_, _, mut child_blocks) = {
            let parts = fs.parts();
            let child_slot = parts
                .inodes
                .get(parts.cache, parts.device, found as u64, &parts.io)
                .map_err(|_| Errno::from_i32(ENOENT))?;
            let (zones, size) = {
                let child = parts.inodes.slot(child_slot);
                (child.zones, child.size as u64)
            };
            let images = load_dir_blocks(
                parts.cache,
                parts.device,
                &zones,
                parts.map,
                parts.range,
                size,
                parts.block_size,
            )?;
            let _ = parts.inodes.put(parts.cache, child_slot, &parts.io);
            (zones, size, images)
        };
        let outcome = {
            let parts = fs.parts();
            let mut ctx = LinkCtx {
                table: parts.inodes,
                cache: parts.cache,
                device: parts.device,
                io: parts.io,
                read_only: parts.read_only,
                block_size: parts.block_size,
                map: parts.map,
                range: parts.range,
            };
            crate::link::remove_directory(
                &mut ctx,
                directory,
                &mut blocks,
                name.as_bytes(),
                &mut child_blocks,
            )
            .map_err(|error| error.to_errno())?
        };
        Self::store_parent(fs, directory, old_size, &mut blocks)?;
        Self::reclaim_zones(fs, &outcome.reclaimed);
        Ok(())
    }

    fn rename(
        &mut self,
        old_directory: u64,
        old_name: &str,
        new_directory: u64,
        new_name: &str,
    ) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut old_blocks, old_size) = Self::load_parent(fs, old_directory)?;
        let same_dir = old_directory == new_directory;
        if same_dir {
            // 同目录:引擎探新名/落新名都走旧镜像(C new_dirp == old_dirp),
            // 新映像参数空置(引擎自测同款)。
            let mut scratch = Vec::new();
            let outcome = {
                let parts = fs.parts();
                let mut ctx = LinkCtx {
                    table: parts.inodes,
                    cache: parts.cache,
                    device: parts.device,
                    io: parts.io,
                    read_only: parts.read_only,
                    block_size: parts.block_size,
                    map: parts.map,
                    range: parts.range,
                };
                crate::link::rename(
                    &mut ctx,
                    old_directory,
                    &mut old_blocks,
                    old_name.as_bytes(),
                    new_directory,
                    &mut scratch,
                    new_name.as_bytes(),
                )
                .map_err(|error| error.to_errno())?
            };
            Self::store_parent(fs, old_directory, old_size, &mut old_blocks)?;
            Self::reclaim_zones(fs, &outcome.reclaimed);
            return Ok(());
        }
        // 跨目录:新旧父各自装载(顺序拿可变借用,装载完即释放)。
        let (mut new_blocks, new_size) = Self::load_parent(fs, new_directory)?;
        let outcome = {
            let parts = fs.parts();
            let mut ctx = LinkCtx {
                table: parts.inodes,
                cache: parts.cache,
                device: parts.device,
                io: parts.io,
                read_only: parts.read_only,
                block_size: parts.block_size,
                map: parts.map,
                range: parts.range,
            };
            crate::link::rename(
                &mut ctx,
                old_directory,
                &mut old_blocks,
                old_name.as_bytes(),
                new_directory,
                &mut new_blocks,
                new_name.as_bytes(),
            )
            .map_err(|error| error.to_errno())?
        };
        Self::store_parent(fs, old_directory, old_size, &mut old_blocks)?;
        Self::store_parent(fs, new_directory, new_size, &mut new_blocks)?;
        Self::reclaim_zones(fs, &outcome.reclaimed);
        Ok(())
    }

    fn symbolic_link(
        &mut self,
        directory: u64,
        name: &str,
        owner: u32,
        group: u32,
        target: &[u8],
    ) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let (mut blocks, old_size) = Self::load_parent(fs, directory)?;
        {
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
            crate::open::create_symlink(
                &mut ctx,
                directory,
                &mut blocks,
                name.as_bytes(),
                owner as u16,
                group as u16,
                target,
            )
            .map_err(|error| error.to_errno())?;
        }
        Self::store_parent(fs, directory, old_size, &mut blocks)?;
        Ok(())
    }

    fn read_link(
        &mut self,
        inode: u64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        let mut ctx = LinkCtx {
            table: parts.inodes,
            cache: parts.cache,
            device: parts.device,
            io: parts.io,
            read_only: parts.read_only,
            block_size: parts.block_size,
            map: parts.map,
            range: parts.range,
        };
        crate::link::read_link(&mut ctx, inode, capacity, out).map_err(|error| error.to_errno())
    }

    fn change_owner(&mut self, inode: u64, owner: u32, group: u32) -> Result<u32, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        crate::meta::change_owner(
            parts.inodes,
            parts.cache,
            &parts.io,
            parts.device,
            inode,
            owner as u16,
            group as u16,
        )
        .map(|mode| mode as u32)
        .map_err(|error| error.to_errno())
    }

    fn change_mode(&mut self, inode: u64, mode: u32) -> Result<u32, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        crate::meta::change_mode(
            parts.inodes,
            parts.cache,
            &parts.io,
            parts.device,
            inode,
            parts.read_only,
            mode as u16,
        )
        .map(|mode| mode as u32)
        .map_err(|error| error.to_errno())
    }

    fn update_times(
        &mut self,
        inode: u64,
        accessed: (i64, i64),
        modified: (i64, i64),
    ) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        crate::meta::update_times(
            parts.inodes,
            parts.cache,
            &parts.io,
            parts.device,
            inode,
            crate::meta::TimeSpec {
                seconds: accessed.0,
                nanoseconds: accessed.1,
            },
            crate::meta::TimeSpec {
                seconds: modified.0,
                nanoseconds: modified.1,
            },
        )
        .map_err(|error| error.to_errno())
    }

    fn stat_vfs(&mut self, vfs: &mut StatVfs) -> Result<(), Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        let zone_free = crate::mount::count_free_bits(
            parts.superblock,
            parts.cache,
            crate::superblock::MAP_ZONE,
        )
        .map_err(|error| error.to_errno())?;
        let volume = crate::meta::read_volume_stat(
            parts.cache,
            parts.superblock,
            parts.superblock.zones,
            parts.superblock.zones.saturating_sub(zone_free),
        )
        .map_err(|error| error.to_errno())?;
        *vfs = StatVfs {
            blocks: volume.blocks,
            blocks_free: volume.blocks_free,
            blocks_available: volume.blocks_available,
            block_size: volume.block_size,
            fragment_size: volume.fragment_size,
            io_size: volume.io_size,
            files: volume.files,
            files_free: volume.files_free,
            files_available: volume.files_available,
            name_max: volume.name_max,
        };
        Ok(())
    }

    fn stat(&mut self, inode: u64, stat: &mut Stat) -> Result<(), Errno> {
        let now = (self.clock)();
        let fs = self.mounted()?;
        let parts = fs.parts();
        let file = crate::meta::read_stat(
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
        *stat = Stat {
            device: parts.device,
            inode,
            mode: file.mode as u32,
            nlinks: file.nlinks as u32,
            owner: file.owner as u32,
            group: file.group as u32,
            special: file.device,
            size: file.size,
            accessed: file.accessed,
            modified: file.modified,
            changed: file.changed,
            block_size: file.block_size,
            blocks: file.blocks,
        };
        Ok(())
    }

    fn block_read(
        &mut self,
        device: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        // 暂存缓冲按分区大小钳制，避免越界长度造成巨大分配。
        let partition = parts.superblock.zones.saturating_mul(parts.block_size as u64);
        let clamped =
            (length as u64).min(partition.saturating_sub(position.max(0) as u64)) as usize;
        let mut staging = alloc::vec![0u8; clamped];
        {
            let mut backend = MemoryBackend {
                storage: &mut staging,
                fail_with: None,
            };
            let mut channel = DataChannel::Present {
                backend: &mut backend,
                size: clamped,
            };
            let mut info = SuperblockBytes {
                bytes: parts.superblock.zones.saturating_mul(parts.block_size as u64),
            };
            bio_transfer(
                parts.cache,
                &mut info,
                device,
                position,
                clamped,
                TransferDirection::Read,
                &mut channel,
            )?;
        }
        out(&staging);
        Ok(staging.len())
    }

    fn block_write(&mut self, device: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        let fs = self.mounted()?;
        let parts = fs.parts();
        let partition = parts.superblock.zones.saturating_mul(parts.block_size as u64);
        let clamped =
            (data.len() as u64).min(partition.saturating_sub(position.max(0) as u64)) as usize;
        let mut staging = data[..clamped].to_vec();
        {
            let mut backend = MemoryBackend {
                storage: &mut staging,
                fail_with: None,
            };
            let mut channel = DataChannel::Present {
                backend: &mut backend,
                size: clamped,
            };
            let mut info = SuperblockBytes {
                bytes: parts.superblock.zones.saturating_mul(parts.block_size as u64),
            };
            bio_transfer(
                parts.cache,
                &mut info,
                device,
                position,
                clamped,
                TransferDirection::Write,
                &mut channel,
            )?;
        }
        Ok(clamped.min(data.len()))
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
        // Bitmaps: bit zero is the reserved bit (always set on a
        // well-formed image), bit one is the root's inode and zone.
        disk.block_mut(2).expect("imap block")[0] |= 0b11;
        disk.block_mut(3).expect("zmap block")[0] |= 0b11;
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

        // Status reports the written size and the owning device.
        let mut stat = Stat::zeroed();
        server.stat(2, &mut stat).unwrap();
        assert_eq!(stat.size, 3);
        assert_eq!(stat.device, DEVICE);

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
    fn test_namespace_family_link_unlink_symlink_mkdir_rmdir_mknod() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        server.create(1, "a", 0o100644, 0, 0).unwrap();
        // Hard link: both names reach the same inode, the link count rises.
        server.link(1, "b", 2).unwrap();
        let (node, _) = server.lookup_child(1, "b").unwrap();
        assert_eq!(node.inode_number, 2);
        let mut stat = Stat::zeroed();
        server.stat(2, &mut stat).unwrap();
        assert_eq!(stat.nlinks, 2);
        // Unlink one name: the other still reaches the file.
        server.unlink(1, "a").unwrap();
        assert!(server.lookup_child(1, "a").is_err());
        let (node, _) = server.lookup_child(1, "b").unwrap();
        assert_eq!(node.inode_number, 2);
        // Symbolic link round trip.
        server.symbolic_link(1, "s", 0, 0, b"b").unwrap();
        let (link_node, _) = server.lookup_child(1, "s").unwrap();
        let mut target = Vec::new();
        let got = server
            .read_link(link_node.inode_number, 32, &mut |bytes: &[u8]| {
                target.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(got, 1);
        assert_eq!(target, b"b".to_vec());
        // Directory create, enumerate the dots, then remove.
        server.make_dir(1, "d", 0o040755, 0, 0).unwrap();
        let (dir_node, _) = server.lookup_child(1, "d").unwrap();
        let mut listed = Vec::new();
        let mut position = 0i64;
        server
            .get_dents(dir_node.inode_number, &mut position, 1024, &mut |bytes: &[u8]| {
                listed.extend_from_slice(bytes)
            })
            .unwrap();
        // The child lists the two dot names: three dot bytes across the
        // packed entries ("." once, ".." twice).
        assert!(listed.iter().filter(|&&byte| byte == b'.').count() >= 3);
        server.remove_dir(1, "d").unwrap();
        assert!(server.lookup_child(1, "d").is_err());
        // Device node carries the device number in its status.
        server.make_node(1, "c", 0o020600, 0, 0, 0x301).unwrap();
        let (node, _) = server.lookup_child(1, "c").unwrap();
        let mut stat = Stat::zeroed();
        server.stat(node.inode_number, &mut stat).unwrap();
        assert_eq!(stat.special, 0x301);
    }

    #[test]
    fn test_statvfs_meta_and_reclaim_visibility() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        let mut scratch = StatVfs::zeroed();
        server.stat_vfs(&mut scratch).unwrap();
        let free_before = scratch.blocks_free;
        // A file with a data zone, synced so the usage is real.
        server.create(1, "a", 0o100644, 0, 0).unwrap();
        server.write(2, 0, b"abc").unwrap();
        server.synchronized();
        server.stat_vfs(&mut scratch).unwrap();
        let free_after_alloc = scratch.blocks_free;
        assert_eq!(free_after_alloc, free_before - 1);
        // Permission bits and timestamps through the metadata family.
        server.change_mode(2, 0o100600).unwrap();
        server.change_owner(2, 5, 7).unwrap();
        server.update_times(2, (10, 0), (20, 0)).unwrap();
        let mut stat = Stat::zeroed();
        server.stat(2, &mut stat).unwrap();
        assert_eq!(stat.mode, 0o100600);
        assert_eq!(stat.owner, 5);
        assert_eq!(stat.group, 7);
        assert_eq!(stat.accessed, 10);
        assert_eq!(stat.modified, 20);
        // Unlinking the last name drops the link count to zero; the zones
        // are freed when the virtual file system releases its reference —
        // then the reclaim executor returns the space, visible after sync.
        server.unlink(1, "a").unwrap();
        server.put_node(2, 1).unwrap();
        server.synchronized();
        server.stat_vfs(&mut scratch).unwrap();
        assert_eq!(scratch.blocks_free, free_before);
    }

    #[test]
    fn test_block_read_raw_root_block() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        // Raw transfer of the root directory block: the dot entry bytes
        // come through untouched.
        let mut seen = Vec::new();
        let got = server
            .block_read(DEVICE, 5 * BLOCK_SIZE as i64, BLOCK_SIZE, &mut |bytes: &[u8]| {
                seen.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(got, BLOCK_SIZE);
        assert_eq!(&seen[..4], &1u32.to_le_bytes());
        assert_eq!(&seen[4..5], b".");
    }

    #[test]
    fn test_peek_warms_cache_without_advancing() {
        let mut server = MfsServer::with_pool(build_image(), 8, zero_clock);
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(DEVICE, flags(false), &mut capabilities).unwrap();
        server.create(1, "a", 0o100644, 0, 0).unwrap();
        server.write(2, 0, b"xyz").unwrap();
        // Peek 只报字节数：读路径暖缓存，不做拷贝，也没有新位置。
        let got = server.peek(2, 0, 16).unwrap();
        assert_eq!(got, 3);
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
