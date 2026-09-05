//! Creation and opening: new nodes, files, directories, device nodes,
//! symbolic links, and seek flags.
//!
//! C correspondence: `minix3/minix/fs/mfs/open.c` (all two hundred seventy
//! lines): `fs_create`, `fs_mknod`, `fs_mkdir`, `fs_slink`, `new_node`,
//! `fs_seek`. Creation is a four-beat dance shared by every maker: check
//! the parent, make sure the name is free, allocate and publish the inode
//! first (a lone inode beats a dangling name on crash), then enter the name.
//! Rollback on any failure returns the bitmap bit and the slot; the link
//! stage owns data-zone reclamation for non-empty files (the create paths
//! only ever roll back empty inodes, whose zone lists are empty).
//!
//! Directory content travels as caller-supplied block images in directory
//! order (see `dir.rs`): the data path feeds real cache bytes later, and
//! every rule is testable now. Directory size and scan hints live only in
//! the parent slot (`i_size`, `i_last_dpos`); there is no parallel state.

use alloc::vec::Vec;

use minix_types::{EEXIST, EFBIG, EINVAL, EMLINK, ENAMETOOLONG, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource};

use crate::dir::{DirError, DirScan, SearchOp, lookup_name, search_blocks};
use crate::inode::{
    InodeError, InodeIo, InodeTable, LINK_CEILING, NO_LINK, TYPE_DIRECTORY, TYPE_MASK, TYPE_SYMLINK,
};
use crate::mfs_cache::{ZoneSpace, alloc_zone, free_zone, mfs_get_block};

/// Why a creation failed. Each variant maps to the wire code the C
/// functions return at the same decision point; table exhaustion and
/// bitmap exhaustion keep their precise codes instead of collapsing to
/// "invalid argument" (more precise, never less safe to forward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// Bad request (unknown directory, removed parent, corrupt state).
    Invalid,
    /// Name already exists (`EEXIST`).
    Exists,
    /// Parent link ceiling hit (`EMLINK`).
    LinkCeiling,
    /// Table or bitmap exhaustion (precise code preserved).
    NoSpace(Errno),
    /// Link target too long or poisoned (`ENAMETOOLONG`).
    NameTooLong,
    /// Directory size overflow (`EFBIG`).
    TooBig,
}

impl OpenError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::Exists => Errno::from_i32(EEXIST),
            Self::LinkCeiling => Errno::from_i32(EMLINK),
            Self::NoSpace(error) => error,
            Self::NameTooLong => Errno::from_i32(ENAMETOOLONG),
            Self::TooBig => Errno::from_i32(EFBIG),
        }
    }
}

impl From<InodeError> for OpenError {
    fn from(error: InodeError) -> Self {
        match error {
            InodeError::TableFull => Self::NoSpace(Errno::from_i32(minix_types::ENFILE)),
            InodeError::NoSpace => Self::NoSpace(Errno::from_i32(minix_types::ENOSPC)),
            InodeError::ReadOnly => Self::NoSpace(Errno::from_i32(minix_types::EROFS)),
            InodeError::Corrupt => Self::Invalid,
            InodeError::Invalid => Self::Invalid,
        }
    }
}

impl From<DirError> for OpenError {
    fn from(error: DirError) -> Self {
        match error {
            DirError::NotDirectory => Self::Invalid,
            DirError::ReadOnly => Self::NoSpace(Errno::from_i32(minix_types::EROFS)),
            DirError::NotFound => Self::Invalid,
            DirError::NotEmpty => Self::Invalid,
            DirError::TooBig => Self::TooBig,
            DirError::Invalid => Self::Invalid,
        }
    }
}

/// A new file described the way the framework expects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewFile {
    /// Inode number.
    pub ino: u64,
    /// File mode.
    pub mode: u16,
    /// File size (zero for fresh nodes).
    pub size: i64,
    /// Owner user identifier.
    pub owner: u16,
    /// Owner group identifier.
    pub group: u16,
}

/// A finished creation whose references are already released.
///
/// `mkdir`, `mknod`, and `slink` release every slot before returning (like
/// the C functions); the slot index stays valid for inspection while the
/// table lives, and `number` names the file. Contrast [`create_file`],
/// whose child stays referenced for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Created {
    /// Table slot index (reference released).
    pub slot: usize,
    /// Inode number.
    pub number: u64,
}

/// Creation context: the shared pieces every maker threads through.
pub struct CreateCtx<'a, S: BlockSource> {
    /// Inode table.
    pub table: &'a mut InodeTable,
    /// Block cache.
    pub cache: &'a mut BlockCache<S, minix_fs::cache::NoSecondLevel>,
    /// Superblock (search hints, read-only flag).
    pub superblock: &'a mut crate::superblock::Superblock,
    /// Inode bitmap.
    pub bitmap: &'a mut crate::superblock::Bitmap,
    /// Device being served.
    pub device: u64,
    /// Inode geometry for disk transfers.
    pub io: InodeIo,
    /// Whether the mount is read-only.
    pub read_only: bool,
}

impl<S: BlockSource> CreateCtx<'_, S> {
    /// Cache block size for directory images.
    fn block_size(&self) -> usize {
        self.cache.source_block_size()
    }
}

/// Allocate and publish one inode under a parent directory.
///
/// C: `new_node` (`open.c:192-257`). Nine parameters mirror the C signature
/// plus the context bundle; callers pass them positionally in C order.
/// Guards first (removed parent, link ceiling), then advance: an existing
/// name is "already exists", any other advance failure propagates. On a
/// missing name: allocate, bump the link count, stamp the device zone, force the inode to disk (crash ordering:
/// lone inode beats dangling name), then enter the name. Enter failure
/// unlinks the inode and returns the bitmap bit. The parent slot stays open
/// throughout; the caller releases it. Returns the child slot, still
/// referenced.
#[allow(clippy::too_many_arguments)]
pub fn new_node<S: BlockSource>(
    ctx: &mut CreateCtx<'_, S>,
    dir_slot: usize,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    mode: u16,
    owner: u16,
    group: u16,
    first_zone: u64,
) -> Result<usize, OpenError> {
    let (dir_nlinks, dir_size, dir_last, dir_mode) = {
        let dir = ctx.table.slot(dir_slot);
        (dir.nlinks, dir.size as u64, dir.scan_hint, dir.mode)
    };
    if dir_nlinks == NO_LINK {
        return Err(OpenError::Invalid);
    }
    if mode as u32 & TYPE_MASK == TYPE_DIRECTORY && dir_nlinks as u32 >= LINK_CEILING {
        return Err(OpenError::LinkCeiling);
    }
    // Advance: is the name free?
    let mut found = 0u32;
    match lookup_name(
        dir_mode,
        dir_nlinks,
        dir_blocks,
        ctx.block_size(),
        dir_size,
        name,
        &mut found,
    ) {
        Ok(()) => return Err(OpenError::Exists),
        Err(DirError::NotFound) => {}
        Err(error) => return Err(error.into()),
    }
    // Missing: allocate, link once, stamp the zone, force to disk.
    let child = ctx.table.allocate(
        ctx.cache,
        ctx.superblock,
        ctx.bitmap,
        mode,
        owner,
        group,
        ctx.device,
    )?;
    ctx.table.slot_mut(child).nlinks += 1;
    ctx.table.slot_mut(child).zones[0] = first_zone;
    ctx.table
        .write_back(ctx.cache, child, &ctx.io)
        .map_err(OpenError::from)?;
    // Enter the name; roll everything back on failure.
    let child_number = ctx.table.slot(child).number;
    let mut scan = DirScan {
        size: dir_size,
        last_dpos: dir_last,
    };
    let mut number = child_number as u32;
    let entered = search_blocks(
        dir_blocks,
        ctx.block_size(),
        &mut scan,
        true,
        ctx.read_only,
        SearchOp::Enter,
        name,
        &mut number,
    );
    {
        let dir = ctx.table.slot_mut(dir_slot);
        dir.size = scan.size as i64;
        dir.scan_hint = scan.last_dpos;
        dir.dirty = true;
    }
    if let Err(error) = entered {
        let number = ctx.table.slot(child).number;
        ctx.table.slot_mut(child).nlinks -= 1;
        ctx.table.slot_mut(child).dirty = true;
        let _ = ctx.table.put(ctx.cache, child, &ctx.io);
        ctx.table.free_number(ctx.superblock, ctx.bitmap, number);
        return Err(error.into());
    }
    Ok(child)
}

/// Create a regular file (`fs_create`, `open.c:14-50`).
///
/// Gets the parent, makes the node, reports the descriptor, releases the
/// parent. The child stays referenced for the caller. Errors release the
/// parent and propagate precisely.
pub fn create_file<S: BlockSource>(
    ctx: &mut CreateCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    mode: u16,
    owner: u16,
    group: u16,
) -> Result<(NewFile, usize), OpenError> {
    let dir_slot = get_dir(ctx, dir_number)?;
    let child = new_node(ctx, dir_slot, dir_blocks, name, mode, owner, group, 0);
    release_slot(ctx, dir_slot)?;
    let child = child?;
    Ok((describe(ctx, child), child))
}

/// Create a device node (`fs_mknod`, `open.c:56-71`): like a file with the
/// device number stamped as the first zone. Both slots release before
/// returning, like the C function. Arity mirrors the C signature in C
/// order; the context bundle already compresses five pieces into one.
#[allow(clippy::too_many_arguments)]
pub fn create_device<S: BlockSource>(
    ctx: &mut CreateCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    mode: u16,
    owner: u16,
    group: u16,
    device: u64,
) -> Result<Created, OpenError> {
    let dir_slot = get_dir(ctx, dir_number)?;
    let child = new_node(ctx, dir_slot, dir_blocks, name, mode, owner, group, device);
    release_slot(ctx, dir_slot)?;
    let child = child?;
    let number = ctx.table.slot(child).number;
    release_slot(ctx, child)?;
    Ok(Created {
        slot: child,
        number,
    })
}

/// Create a directory (`fs_mkdir`, `open.c:77-122`).
///
/// Makes the node, enters dot and dot-dot into a fresh image set, bumps
/// both link counts on success. On dot failure the name is deleted from the
/// parent (a failed delete reports corruption instead of panicking like the
/// C code), the child's link is undone, and the bitmap bit returns.
/// Arity mirrors the C signature in C order; see `create_device`.
#[allow(clippy::too_many_arguments)]
pub fn create_dir<S: BlockSource>(
    ctx: &mut CreateCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    mode: u16,
    owner: u16,
    group: u16,
    child_blocks: &mut Vec<Vec<u8>>,
) -> Result<Created, OpenError> {
    let dir_slot = get_dir(ctx, dir_number)?;
    let child = match new_node(ctx, dir_slot, dir_blocks, name, mode, owner, group, 0) {
        Ok(child) => child,
        Err(error) => {
            release_slot(ctx, dir_slot)?;
            return Err(error);
        }
    };
    let (child_number, parent_number) = {
        let child_inode = ctx.table.slot(child);
        let parent_inode = ctx.table.slot(dir_slot);
        (child_inode.number, parent_inode.number)
    };
    let block_size = ctx.block_size();
    let read_only = ctx.read_only;
    let mut scan = DirScan {
        size: 0,
        last_dpos: 0,
    };
    let mut dot = child_number as u32;
    let first = search_blocks(
        child_blocks,
        block_size,
        &mut scan,
        true,
        read_only,
        SearchOp::Enter,
        b".",
        &mut dot,
    );
    let mut dotdot = parent_number as u32;
    let second = search_blocks(
        child_blocks,
        block_size,
        &mut scan,
        true,
        read_only,
        SearchOp::Enter,
        b"..",
        &mut dotdot,
    );
    if first.is_ok() && second.is_ok() {
        ctx.table.slot_mut(child).nlinks += 1;
        ctx.table.slot_mut(dir_slot).nlinks += 1;
        ctx.table.slot_mut(dir_slot).dirty = true;
    } else {
        // Roll back: delete the name, undo the link, free the bit.
        let mut number = 0u32;
        let (size, last) = {
            let dir = ctx.table.slot(dir_slot);
            (dir.size as u64, dir.scan_hint)
        };
        let mut rollback = DirScan {
            size,
            last_dpos: last,
        };
        search_blocks(
            dir_blocks,
            block_size,
            &mut rollback,
            true,
            read_only,
            SearchOp::Delete,
            name,
            &mut number,
        )
        .map_err(|_| OpenError::Invalid)?;
        {
            let dir = ctx.table.slot_mut(dir_slot);
            dir.size = rollback.size as i64;
            dir.scan_hint = rollback.last_dpos;
            dir.dirty = true;
        }
        let number = ctx.table.slot(child).number;
        ctx.table.slot_mut(child).nlinks -= 1;
        ctx.table.slot_mut(child).dirty = true;
        let _ = ctx.table.put(ctx.cache, child, &ctx.io);
        ctx.table.free_number(ctx.superblock, ctx.bitmap, number);
        release_slot(ctx, dir_slot)?;
        // At least one dot-enter failed (guarded above): report the first.
        return Err(first
            .err()
            .or(second.err())
            .map(OpenError::from)
            .unwrap_or(OpenError::Invalid));
    }
    ctx.table.slot_mut(child).dirty = true;
    {
        let child_inode = ctx.table.slot_mut(child);
        child_inode.size = scan.size as i64;
        child_inode.scan_hint = scan.last_dpos;
    }
    release_slot(ctx, dir_slot)?;
    release_slot(ctx, child)?;
    Ok(Created {
        slot: child,
        number: child_number,
    })
}

/// Create a symbolic link (`fs_slink`, `open.c:128-187`).
///
/// Makes the node, allocates one zone for the target, copies the target
/// plus terminator, and sizes by the true string length: an embedded zero
/// reports name-too-long (the C comment apologizes for the code and keeps
/// it anyway). Rollback unlinks, deletes the name, frees the zone and the
/// bitmap bit.
pub fn create_symlink<S: BlockSource>(
    ctx: &mut CreateCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    owner: u16,
    group: u16,
    target: &[u8],
) -> Result<Created, OpenError> {
    let dir_slot = get_dir(ctx, dir_number)?;
    let child = match new_node(
        ctx,
        dir_slot,
        dir_blocks,
        name,
        TYPE_SYMLINK as u16 | 0o777,
        owner,
        group,
        0,
    ) {
        Ok(child) => child,
        Err(error) => {
            release_slot(ctx, dir_slot)?;
            return Err(error);
        }
    };
    let block_size = ctx.block_size();
    if target.len() + 1 > block_size {
        return rollback_symlink(ctx, dir_slot, dir_blocks, name, child, None);
    }
    // Allocate the target zone through the 07 policy.
    let mut space = ZoneSpace {
        first_data_zone: ctx.superblock.first_data_zone,
        zone_count: ctx.superblock.zones,
        zsearch: ctx.superblock.zsearch,
    };
    let zone = {
        let bitmap = &mut *ctx.bitmap;
        match alloc_zone(&mut space, 0, &mut |hint| bitmap.alloc(hint)) {
            Ok(zone) => zone,
            Err(_) => {
                return rollback_symlink(ctx, dir_slot, dir_blocks, name, child, None);
            }
        }
    };
    ctx.superblock.zsearch = space.zsearch;
    // Write target plus terminator into the fresh zone's block. A normal
    // acquire always yields a slot (absence is peek-only by contract).
    let slot = mfs_get_block(
        ctx.cache,
        BlockKey::new(ctx.device, zone),
        AcquireMode::NoRead,
    )
    .map_err(|_| OpenError::Invalid)?
    .ok_or(OpenError::Invalid)?;
    {
        let data = ctx.cache.slot_data_mut(slot);
        data[..target.len()].copy_from_slice(target);
        data[target.len()] = 0;
    }
    ctx.cache.mark_dirty(slot);
    let _ = ctx.cache.release(slot);
    // True length decides: embedded zeros are refused.
    let true_length = target
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(target.len());
    if true_length != target.len() {
        return rollback_symlink(ctx, dir_slot, dir_blocks, name, child, Some(zone));
    }
    ctx.table.slot_mut(child).zones[0] = zone;
    ctx.table.slot_mut(child).size = true_length as i64;
    ctx.table.slot_mut(child).dirty = true;
    release_slot(ctx, dir_slot)?;
    let number = ctx.table.slot(child).number;
    release_slot(ctx, child)?;
    Ok(Created {
        slot: child,
        number,
    })
}

/// Roll back a failed symbolic link: unlink the node, delete the name,
/// free the zone and the bitmap bit.
fn rollback_symlink<S: BlockSource>(
    ctx: &mut CreateCtx<'_, S>,
    dir_slot: usize,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    child: usize,
    zone: Option<u64>,
) -> Result<Created, OpenError> {
    let number = ctx.table.slot(child).number;
    ctx.table.slot_mut(child).nlinks = NO_LINK;
    ctx.table.slot_mut(child).dirty = true;
    let _ = ctx.table.put(ctx.cache, child, &ctx.io);
    ctx.table.free_number(ctx.superblock, ctx.bitmap, number);
    if let Some(zone) = zone {
        let mut space = ZoneSpace {
            first_data_zone: ctx.superblock.first_data_zone,
            zone_count: ctx.superblock.zones,
            zsearch: ctx.superblock.zsearch,
        };
        let bitmap = &mut *ctx.bitmap;
        free_zone(ctx.cache, ctx.device, &mut space, zone, &mut |bit| {
            bitmap.free(bit);
        });
        ctx.superblock.zsearch = space.zsearch;
    }
    let mut zap = 0u32;
    let (size, last) = {
        let dir = ctx.table.slot(dir_slot);
        (dir.size as u64, dir.scan_hint)
    };
    let mut scan = DirScan {
        size,
        last_dpos: last,
    };
    search_blocks(
        dir_blocks,
        ctx.block_size(),
        &mut scan,
        true,
        ctx.read_only,
        SearchOp::Delete,
        name,
        &mut zap,
    )
    .map_err(|_| OpenError::Invalid)?;
    {
        let dir = ctx.table.slot_mut(dir_slot);
        dir.size = scan.size as i64;
        dir.scan_hint = scan.last_dpos;
        dir.dirty = true;
    }
    release_slot(ctx, dir_slot)?;
    Err(OpenError::NameTooLong)
}

/// Mark the seek flag without opening (`fs_seek`, `open.c:263-270`).
///
/// A missing inode is silently ignored: seek is a hint, and hints never
/// fail.
pub fn mark_seek(table: &mut InodeTable, device: u64, number: u64) {
    if let Some(slot) = table.find(device, number) {
        table.slot_mut(slot).seek = true;
    }
}

/// Open the parent directory or report invalid (`get_inode` failure in
/// `fs_create` and siblings maps to "invalid argument").
fn get_dir<S: BlockSource>(ctx: &mut CreateCtx<'_, S>, number: u64) -> Result<usize, OpenError> {
    let io = ctx.io;
    ctx.table
        .get(ctx.cache, ctx.device, number, &io)
        .map_err(|_| OpenError::Invalid)
}

/// Release a slot the maker no longer needs.
///
/// Writeback failures propagate: more precise than the void C call, never
/// less safe to forward.
fn release_slot<S: BlockSource>(ctx: &mut CreateCtx<'_, S>, slot: usize) -> Result<(), OpenError> {
    let io = ctx.io;
    ctx.table
        .put(ctx.cache, slot, &io)
        .map_err(OpenError::from)?;
    Ok(())
}

/// Describe a fresh child the framework way.
fn describe<S: BlockSource>(ctx: &CreateCtx<'_, S>, slot: usize) -> NewFile {
    let inode = ctx.table.slot(slot);
    NewFile {
        ino: inode.number,
        mode: inode.mode,
        size: inode.size,
        owner: inode.owner,
        group: inode.group,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dir::SearchOutcome;
    use crate::inode::InodeTable;
    use crate::superblock::{Bitmap, Superblock};
    use minix_fs::bio::RamDisk;
    use minix_fs::cache::{BlockCache, NoSecondLevel};

    extern crate alloc;
    use alloc::vec::Vec;

    const DEVICE: u64 = 0x301;
    const BLOCK_SIZE: usize = 512;

    struct Fixture {
        table: InodeTable,
        cache: BlockCache<RamDisk>,
        superblock: Superblock,
        bitmap: Bitmap,
        io: InodeIo,
        parent: usize,
    }

    /// Build the pieces as locals first, then assemble: no aliasing, every
    /// borrow sequential. The parent opens through the real allocator, so
    /// bitmap and hints stay consistent.
    fn fixture() -> Fixture {
        let mut cache =
            BlockCache::with_pool(RamDisk::new(64, BLOCK_SIZE).unwrap(), NoSecondLevel, 8).unwrap();
        let mut superblock = Superblock {
            inode_count: 64,
            inode_map_blocks: 1,
            zone_map_blocks: 1,
            flags: 1,
            max_size: 100000,
            zones: 64,
            version: 3,
            block_size: BLOCK_SIZE,
            inodes_per_block: 8,
            direct_zones: 7,
            indirect_per_block: 128,
            first_data_zone: 4,
            zone_total_small: 0,
            first_data_zone_small: 4,
            disk_version: 0,
            device: DEVICE,
            read_only: false,
            isearch: 0,
            zsearch: 0,
        };
        let mut bitmap = Bitmap::new(65);
        let io = InodeIo::from_superblock(&superblock);
        let mut table = InodeTable::new();
        let parent = table
            .allocate(
                &mut cache,
                &mut superblock,
                &mut bitmap,
                0o040755,
                0,
                0,
                DEVICE,
            )
            .unwrap();
        table.slot_mut(parent).nlinks = 2;
        Fixture {
            table,
            cache,
            superblock,
            bitmap,
            io,
            parent,
        }
    }

    /// Borrow the context pieces out of the fixture (disjoint fields).
    fn ctx_of(fixture: &mut Fixture) -> CreateCtx<'_, RamDisk> {
        CreateCtx {
            table: &mut fixture.table,
            cache: &mut fixture.cache,
            superblock: &mut fixture.superblock,
            bitmap: &mut fixture.bitmap,
            device: DEVICE,
            io: fixture.io,
            read_only: false,
        }
    }

    #[test]
    fn test_create_file_roundtrip() {
        let mut fixture = fixture();
        let mut parent_blocks = Vec::new();
        let (file, child) = {
            let mut ctx = ctx_of(&mut fixture);
            create_file(&mut ctx, 1, &mut parent_blocks, b"note", 0o100644, 7, 8).unwrap()
        };
        assert_eq!(file.mode, 0o100644);
        assert_eq!(file.owner, 7);
        assert_eq!(file.size, 0);
        assert_eq!(fixture.table.slot(child).nlinks, 1);
        // The child stays referenced for the caller.
        assert_eq!(fixture.table.slot(child).count, 1);
        // The name resolves through the parent images.
        let mut found = 0u32;
        let mut scan = DirScan {
            size: 64,
            last_dpos: 0,
        };
        assert_eq!(
            search_blocks(
                &mut parent_blocks,
                BLOCK_SIZE,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"note",
                &mut found,
            )
            .unwrap(),
            SearchOutcome::Found(file.ino as u32)
        );
        // Duplicate creation reports exists and releases the parent.
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_file(&mut ctx, 1, &mut parent_blocks, b"note", 0o100644, 0, 0).unwrap_err(),
            OpenError::Exists
        );
    }

    #[test]
    fn test_create_guards() {
        let mut fixture = fixture();
        let mut parent_blocks = Vec::new();
        // Removed parent refuses.
        fixture.table.slot_mut(fixture.parent).nlinks = NO_LINK;
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_file(&mut ctx, 1, &mut parent_blocks, b"x", 0o100644, 0, 0).unwrap_err(),
            OpenError::Invalid
        );
        // Link ceiling refuses directories (files are exempt).
        fixture.table.slot_mut(fixture.parent).nlinks = LINK_CEILING as u16;
        let mut child_blocks = Vec::new();
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_dir(
                &mut ctx,
                1,
                &mut parent_blocks,
                b"d",
                0o040755,
                0,
                0,
                &mut child_blocks
            )
            .unwrap_err(),
            OpenError::LinkCeiling
        );
        // ...but a regular file still passes the ceiling.
        let mut ctx = ctx_of(&mut fixture);
        assert!(create_file(&mut ctx, 1, &mut parent_blocks, b"f", 0o100644, 0, 0).is_ok());
    }

    #[test]
    fn test_mkdir_dots_and_links() {
        let mut fixture = fixture();
        let mut parent_blocks = Vec::new();
        let mut child_blocks = Vec::new();
        let created = {
            let mut ctx = ctx_of(&mut fixture);
            create_dir(
                &mut ctx,
                1,
                &mut parent_blocks,
                b"sub",
                0o040755,
                0,
                0,
                &mut child_blocks,
            )
            .unwrap()
        };
        // Dot and dot-dot landed in the child images.
        let mut found = 0u32;
        let mut scan = DirScan {
            size: 128,
            last_dpos: 0,
        };
        assert_eq!(
            search_blocks(
                &mut child_blocks,
                BLOCK_SIZE,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b".",
                &mut found
            )
            .unwrap(),
            SearchOutcome::Found(created.number as u32)
        );
        let mut scan = DirScan {
            size: 128,
            last_dpos: 0,
        };
        assert_eq!(
            search_blocks(
                &mut child_blocks,
                BLOCK_SIZE,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"..",
                &mut found
            )
            .unwrap(),
            SearchOutcome::Found(1)
        );
        // Link counts: child for dot, parent for dot-dot.
        assert_eq!(fixture.table.slot(created.slot).nlinks, 2);
    }

    #[test]
    fn test_mknod_stamps_device() {
        let mut fixture = fixture();
        let mut parent_blocks = Vec::new();
        let created = {
            let mut ctx = ctx_of(&mut fixture);
            create_device(
                &mut ctx,
                1,
                &mut parent_blocks,
                b"null",
                0o020666,
                0,
                0,
                0x0401,
            )
            .unwrap()
        };
        assert_eq!(fixture.table.slot(created.slot).zones[0], 0x0401);
        assert_eq!(
            fixture.table.slot(created.slot).mode as u32 & TYPE_MASK,
            0o020000
        );
    }

    #[test]
    fn test_slink_target_roundtrip() {
        let mut fixture = fixture();
        let mut parent_blocks = Vec::new();
        let created = {
            let mut ctx = ctx_of(&mut fixture);
            create_symlink(&mut ctx, 1, &mut parent_blocks, b"link", 0, 0, b"target").unwrap()
        };
        assert_eq!(fixture.table.slot(created.slot).size, 6);
        let zone = fixture.table.slot(created.slot).zones[0];
        assert!(zone >= fixture.superblock.first_data_zone);
        // Embedded zeros are refused and rolled back: the name is gone.
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_symlink(&mut ctx, 1, &mut parent_blocks, b"bad", 0, 0, b"a\0b").unwrap_err(),
            OpenError::NameTooLong
        );
        let mut found = 0u32;
        let mut scan = DirScan {
            size: 64,
            last_dpos: 0,
        };
        // Parent holds "link" only; "bad" never landed.
        assert!(
            search_blocks(
                &mut parent_blocks,
                BLOCK_SIZE,
                &mut scan,
                true,
                false,
                SearchOp::Lookup,
                b"bad",
                &mut found
            )
            .is_err()
        );
        // Oversized targets are refused before allocating.
        let big = alloc::vec![b'x'; BLOCK_SIZE];
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_symlink(&mut ctx, 1, &mut parent_blocks, b"big", 0, 0, &big).unwrap_err(),
            OpenError::NameTooLong
        );
    }

    #[test]
    fn test_seek_marks_and_ignores_missing() {
        let mut fixture = fixture();
        mark_seek(&mut fixture.table, DEVICE, 1);
        let slot = fixture.table.find(DEVICE, 1).unwrap();
        assert!(fixture.table.slot(slot).seek);
        mark_seek(&mut fixture.table, DEVICE, 999);
    }

    #[test]
    fn test_error_codes_match_c() {
        use crate::dir::DirError;
        use minix_types::{EEXIST, EFBIG, EINVAL, EMLINK, ENAMETOOLONG};
        assert_eq!(OpenError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(OpenError::Exists.to_errno().to_i32(), EEXIST);
        assert_eq!(OpenError::LinkCeiling.to_errno().to_i32(), EMLINK);
        assert_eq!(OpenError::NameTooLong.to_errno().to_i32(), ENAMETOOLONG);
        assert_eq!(OpenError::TooBig.to_errno().to_i32(), EFBIG);
        assert_eq!(OpenError::from(DirError::NotFound), OpenError::Invalid);
    }
}
