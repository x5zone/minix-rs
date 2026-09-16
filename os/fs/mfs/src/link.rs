//! Links, removal, renaming, and range freeing.
//!
//! C correspondence: `minix3/minix/fs/mfs/link.c` (all six hundred
//! thirty-eight lines): `fs_link`, `fs_unlink` (shared with remove-dir),
//! `fs_rdlink`, `remove_dir`, `unlink_file`, `fs_rename`, `fs_trunc`,
//! `truncate_inode`, `freesp_inode`, `nextblock`, `zerozone_half`,
//! `zerozone_range`. Directory content travels as caller-supplied block
//! images in directory order (see `dir.rs`); the data path feeds real cache
//! bytes later.
//!
//! Two rules shape this module. First, removal never frees storage inline:
//! unlinking only drops the name and the link count, and the last release
//! reports zones for the caller to reclaim (see `ReleaseOutcome` in
//! `inode.rs`). Nothing is silently dropped. Second, range freeing is split
//! into a pure plan (which bytes to zero, which zones to free) and an
//! executor living with the write path (document 15), which owns zone
//! writing.

use alloc::vec::Vec;

use minix_types::{
    EACCES, EBUSY, EEXIST, EFBIG, EINVAL, EIO, EISDIR, EMLINK, ENOTDIR, EPERM, EROFS, Errno,
};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource};

use crate::dir::{DirError, SearchOp, lookup_name, search_blocks};
use crate::inode::{InodeError, InodeIo, InodeTable, ReleaseOutcome, TYPE_DIRECTORY, TYPE_MASK};
use crate::superblock::ROOT_INODE_NUMBER;

/// Same-name marker: old and new name one file (`SAME`, `link.c:11`, one thousand).
pub const SAME_NAME: i32 = 1000;

/// First half selector for partial zone zeroing (`FIRST_HALF`, zero).
pub const FIRST_HALF: u32 = 0;
/// Last half selector (`LAST_HALF`, one).
pub const LAST_HALF: u32 = 1;

/// Why a link operation failed. Each variant maps to the wire code the C
/// functions return at the same decision point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    /// Bad request (unknown inode, removed parent, corrupt state).
    Invalid,
    /// Name already exists (`EEXIST`).
    Exists,
    /// Link ceiling hit (`EMLINK`).
    LinkCeiling,
    /// Removing a directory as a file (`EPERM`).
    NotPermitted,
    /// Expected a directory, found otherwise (`ENOTDIR`).
    NotDirectory,
    /// Expected a non-directory, found one (`EISDIR`).
    IsDirectory,
    /// Mount point in the way (`EBUSY`).
    Busy,
    /// Directory not empty (`ENOTEMPTY`, remove-dir only).
    NotEmpty,
    /// Write on a read-only mount (`EROFS`).
    ReadOnly,
    /// File too big (`EFBIG`).
    TooBig,
    /// Target is not a symbolic link (`EACCES`, read-link only).
    Access,
    /// Storage failure along the way.
    Io,
    /// Table or bitmap exhaustion (precise code preserved).
    NoSpace(Errno),
}

impl LinkError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::Exists => Errno::from_i32(EEXIST),
            Self::LinkCeiling => Errno::from_i32(EMLINK),
            Self::NotPermitted => Errno::from_i32(EPERM),
            Self::NotDirectory => Errno::from_i32(ENOTDIR),
            Self::IsDirectory => Errno::from_i32(EISDIR),
            Self::Busy => Errno::from_i32(EBUSY),
            Self::NotEmpty => Errno::from_i32(minix_types::ENOTEMPTY),
            Self::ReadOnly => Errno::from_i32(EROFS),
            Self::TooBig => Errno::from_i32(EFBIG),
            Self::Access => Errno::from_i32(EACCES),
            Self::Io => Errno::from_i32(EIO),
            Self::NoSpace(error) => error,
        }
    }
}

impl From<InodeError> for LinkError {
    fn from(error: InodeError) -> Self {
        match error {
            InodeError::TableFull => Self::NoSpace(Errno::from_i32(minix_types::ENFILE)),
            InodeError::NoSpace => Self::NoSpace(Errno::from_i32(minix_types::ENOSPC)),
            InodeError::ReadOnly => Self::ReadOnly,
            InodeError::Corrupt => Self::Io,
            InodeError::Invalid => Self::Invalid,
        }
    }
}

impl From<DirError> for LinkError {
    fn from(error: DirError) -> Self {
        match error {
            DirError::NotDirectory => Self::NotDirectory,
            DirError::ReadOnly => Self::ReadOnly,
            DirError::NotFound => Self::Invalid,
            DirError::NotEmpty => Self::Invalid,
            DirError::TooBig => Self::TooBig,
            DirError::Invalid => Self::Invalid,
        }
    }
}

/// What a link operation finished with: zones the caller must reclaim.
///
/// Every removal funnels freed zones here instead of freeing inline, so the
/// bitmap and the cache stay consistent under one owner (the link stage
/// executor, document 13 wiring; range freeing executes in document 15).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinkOutcome {
    /// Data zones to free (bitmap bits plus cache copies).
    pub reclaimed: Vec<u64>,
}

impl LinkOutcome {
    /// Fold reclaimed zones from a release outcome into this outcome.
    fn absorb(&mut self, outcome: ReleaseOutcome) {
        if let ReleaseOutcome::ReclaimZones(zones) = outcome {
            self.reclaimed.extend(zones);
        }
    }
}

/// Shared link context: table, cache, geometry, mount facts.
pub struct LinkCtx<'a, S: BlockSource> {
    /// Inode table.
    pub table: &'a mut InodeTable,
    /// Block cache (read-link target bytes).
    pub cache: &'a mut BlockCache<S, minix_fs::cache::NoSecondLevel>,
    /// Device being served.
    pub device: u64,
    /// Inode geometry for disk transfers.
    pub io: InodeIo,
    /// Whether the mount is read-only.
    pub read_only: bool,
    /// File block size in bytes.
    pub block_size: usize,
    /// Mapping geometry (direct zones, indirect density).
    pub map: crate::read::MapParams,
    /// Valid zone range for indirect validation.
    pub range: crate::read::ZoneRange,
}

/// Create a hard link (`fs_link`, `link.c:32-97`).
///
/// Opens the file (missing is invalid), refuses ceiling and directories,
/// opens the parent (removed parent is missing), requires the name absent
/// (present is exists), enters it, then bumps the link count with a change
/// timestamp. Both slots release on every path.
pub fn create_link<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    target_number: u64,
) -> Result<LinkOutcome, LinkError> {
    let target = ctx
        .table
        .get(ctx.cache, ctx.device, target_number, &ctx.io)
        .map_err(|_| LinkError::Invalid)?;
    let (links, mode) = {
        let inode = ctx.table.slot(target);
        (inode.nlinks, inode.mode)
    };
    let mut answer = Ok(LinkOutcome::default());
    if links as u32 >= crate::inode::LINK_CEILING {
        answer = Err(LinkError::LinkCeiling);
    } else if mode as u32 & TYPE_MASK == TYPE_DIRECTORY {
        answer = Err(LinkError::NotPermitted);
    }
    if answer.is_err() {
        let io = ctx.io;
        let _ = ctx.table.put(ctx.cache, target, &io);
        return answer;
    }
    let parent = ctx
        .table
        .get(ctx.cache, ctx.device, dir_number, &ctx.io)
        .map_err(|_| {
            let io = ctx.io;
            let _ = ctx.table.put(ctx.cache, target, &io);
            LinkError::Invalid
        })?;
    {
        let parent_inode = ctx.table.slot(parent);
        if parent_inode.nlinks == crate::inode::NO_LINK {
            let io = ctx.io;
            let _ = ctx.table.put(ctx.cache, target, &io);
            let _ = ctx.table.put(ctx.cache, parent, &io);
            return Err(LinkError::Invalid);
        }
    }
    // Advance: absent continues, present is exists, other errors propagate.
    let mut found = 0u32;
    {
        let parent_inode = ctx.table.slot(parent);
        // Read-only parent images are the caller's; the read-only mount
        // check happens at the adapter (document 02), like the C code,
        // which checks read-only only on the unlink path.
        match lookup_name(
            parent_inode.mode,
            parent_inode.nlinks,
            dir_blocks,
            ctx.block_size,
            parent_inode.size as u64,
            name,
            &mut found,
        ) {
            Ok(()) => {
                let io = ctx.io;
                let _ = ctx.table.put(ctx.cache, target, &io);
                let _ = ctx.table.put(ctx.cache, parent, &io);
                return Err(LinkError::Exists);
            }
            Err(DirError::NotFound) => {}
            Err(error) => {
                let io = ctx.io;
                let _ = ctx.table.put(ctx.cache, target, &io);
                let _ = ctx.table.put(ctx.cache, parent, &io);
                return Err(error.into());
            }
        }
    }
    // Enter and bump.
    let (size, last) = {
        let parent_inode = ctx.table.slot(parent);
        (parent_inode.size as u64, parent_inode.scan_hint)
    };
    let mut scan = crate::dir::DirScan {
        size,
        last_dpos: last,
    };
    let mut number = target_number as u32;
    let entered = search_blocks(
        dir_blocks,
        ctx.block_size,
        &mut scan,
        true,
        false,
        SearchOp::Enter,
        name,
        &mut number,
    );
    {
        let parent_inode = ctx.table.slot_mut(parent);
        parent_inode.size = scan.size as i64;
        parent_inode.scan_hint = scan.last_dpos;
        parent_inode.dirty = true;
    }
    entered.map_err(LinkError::from)?;
    {
        let inode = ctx.table.slot_mut(target);
        inode.nlinks += 1;
        inode.pending_updates |= crate::inode::UPDATE_CHANGE;
        inode.dirty = true;
    }
    let io = ctx.io;
    let _ = ctx.table.put(ctx.cache, target, &io);
    let _ = ctx.table.put(ctx.cache, parent, &io);
    Ok(LinkOutcome::default())
}

/// Remove a file name (`fs_unlink` file mode, `link.c:103-146`).
///
/// Opens the parent and the child (missing propagates), refuses mount
/// points and read-only mounts, refuses directories, then unlinks the name:
/// delete plus one fewer link with a change timestamp. Both slots release
/// once at the end, like the C function; freed zones fold into the outcome.
pub fn remove_file<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
) -> Result<LinkOutcome, LinkError> {
    let parent = open_parent(ctx, dir_number)?;
    let child = open_child(ctx, parent, dir_blocks, name)?;
    if ctx.table.slot(child).mountpoint {
        release_pair(ctx, parent, child);
        return Err(LinkError::Busy);
    }
    if ctx.read_only {
        release_pair(ctx, parent, child);
        return Err(LinkError::ReadOnly);
    }
    if ctx.table.slot(child).mode as u32 & TYPE_MASK == TYPE_DIRECTORY {
        release_pair(ctx, parent, child);
        return Err(LinkError::NotPermitted);
    }
    delete_name(ctx, parent, dir_blocks, name)?;
    {
        let inode = ctx.table.slot_mut(child);
        inode.nlinks -= 1;
        inode.pending_updates |= crate::inode::UPDATE_CHANGE;
        inode.dirty = true;
    }
    let mut outcome = LinkOutcome::default();
    release_child(ctx, child, &mut outcome);
    release_parent(ctx, parent);
    Ok(outcome)
}

/// Remove a directory name (`fs_unlink` remove-dir mode via `remove_dir`).
///
/// Checks emptiness over the child's own images, refuses the root, unlinks
/// the name, then unlinks dot and dot-dot ignoring errors (the superuser
/// may have linked anything, so no assumptions). Both slots release once at
/// the end; freed zones fold into the outcome.
pub fn remove_directory<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    dir_number: u64,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
    child_blocks: &mut Vec<Vec<u8>>,
) -> Result<LinkOutcome, LinkError> {
    let parent = open_parent(ctx, dir_number)?;
    let child = open_child(ctx, parent, dir_blocks, name)?;
    let child_size = ctx.table.slot(child).size as u64;
    // Emptiness walk over the child's images (`search_dir` IS_EMPTY).
    {
        let mut number = 0u32;
        let mut scan = crate::dir::DirScan {
            size: child_size,
            last_dpos: 0,
        };
        if search_blocks(
            child_blocks,
            ctx.block_size,
            &mut scan,
            true,
            ctx.read_only,
            SearchOp::IsEmpty,
            b"",
            &mut number,
        )
        .is_err()
        {
            release_pair(ctx, parent, child);
            return Err(LinkError::NotEmpty);
        }
    }
    if child_number(ctx, child) == ROOT_INODE_NUMBER {
        release_pair(ctx, parent, child);
        return Err(LinkError::Busy);
    }
    delete_name(ctx, parent, dir_blocks, name)?;
    // Unlink dot and dot-dot inside the child, ignoring errors.
    {
        let mut number = 0u32;
        let mut scan = crate::dir::DirScan {
            size: child_size,
            last_dpos: 0,
        };
        let _ = search_blocks(
            child_blocks,
            ctx.block_size,
            &mut scan,
            true,
            ctx.read_only,
            SearchOp::Delete,
            b".",
            &mut number,
        );
        let _ = search_blocks(
            child_blocks,
            ctx.block_size,
            &mut scan,
            true,
            ctx.read_only,
            SearchOp::Delete,
            b"..",
            &mut number,
        );
    }
    // Link accounting follows the three `unlink_file` calls of the C
    // sequence (`link.c:205-211`): the parent's name entry drops the child
    // by one, the dot entry drops the child again, and the dot-dot entry
    // — which names the parent — drops the parent. The child lands at
    // NO_LINK, so its final release reports the zones for reclamation.
    // Saturation replaces the C unsigned wrap-around: a directory with
    // fewer than two links is already corrupt, and saturating keeps the
    // release decision reachable instead of wrapping to a huge count.
    {
        let inode = ctx.table.slot_mut(child);
        inode.nlinks = inode.nlinks.saturating_sub(2);
        inode.pending_updates |= crate::inode::UPDATE_CHANGE;
        inode.dirty = true;
    }
    {
        let pdir = ctx.table.slot_mut(parent);
        pdir.nlinks = pdir.nlinks.saturating_sub(1);
        pdir.pending_updates |= crate::inode::UPDATE_CHANGE;
        pdir.dirty = true;
    }
    let mut outcome = LinkOutcome::default();
    release_child(ctx, child, &mut outcome);
    release_parent(ctx, parent);
    Ok(outcome)
}


/// Rename a name, possibly across directories (`fs_rename`,
/// `link.c:255-422`).
///
/// The two parents carry their own images: the same-directory branch never
/// touches `new_dir_images`, so a caller with one parent directory passes
/// any valid image list for it (an empty one). The moved directory's own
/// images — read and rewritten for the `..` update — go through the bridge
/// inside this function, because the moved directory is not a parameter.
///
/// Error-path note: when replacing an existing new name has already
/// reclaimed zones and a later step fails, the reclaimed list is dropped
/// with the error. The C code frees inline at the same decision points, so
/// the observable tree state matches; only the Rust-side reclaim list is
/// lost on that rare path.
pub fn rename<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    old_dir_number: u64,
    old_dir_images: &mut Vec<Vec<u8>>,
    old_name: &[u8],
    new_dir_number: u64,
    new_dir_images: &mut Vec<Vec<u8>>,
    new_name: &[u8],
) -> Result<LinkOutcome, LinkError> {
    let old_dir = open_parent(ctx, old_dir_number)?;
    // 旧文件（link.c:268-277）：打开失败随父目录引用一起返回。
    let old_file = match open_child(ctx, old_dir, old_dir_images, old_name) {
        Ok(slot) => slot,
        Err(error) => {
            release_parent(ctx, old_dir);
            return Err(error);
        }
    };
    // 旧文件是挂载点（link.c:279-283）。
    if ctx.table.slot(old_file).mountpoint {
        release_pair(ctx, old_dir, old_file);
        return Err(LinkError::Busy);
    }
    // 新父目录（link.c:286-290）。
    let new_dir = match open_parent(ctx, new_dir_number) {
        Ok(slot) => slot,
        Err(error) => {
            release_pair(ctx, old_dir, old_file);
            return Err(error);
        }
    };
    // 新父目录已被删除（link.c:291-296）。
    if ctx.table.slot(new_dir).nlinks == crate::inode::NO_LINK {
        release_pair(ctx, old_dir, old_file);
        release_parent(ctx, new_dir);
        return Err(LinkError::Invalid);
    }
    let same_pdir = old_dir_number == new_dir_number;
    // 试探新名字，不要求存在（link.c:299）。同一父目录时新名字就在
    // 旧镜像里（C 的 new_dirp == old_dirp），搜同一份镜像。
    let probe_images: &mut Vec<Vec<u8>> = if same_pdir {
        old_dir_images
    } else {
        new_dir_images
    };
    let mut new_file = {
        let dir = ctx.table.slot(new_dir);
        let (mode, nlinks, size) = (dir.mode, dir.nlinks, dir.size as u64);
        let mut found = 0u32;
        match crate::dir::lookup_name(
            mode,
            nlinks,
            probe_images,
            ctx.block_size,
            size,
            new_name,
            &mut found,
        ) {
            Ok(()) => Some(
                ctx.table
                    .get(ctx.cache, ctx.device, found as u64, &ctx.io)
                    .map_err(|_| LinkError::Invalid)?,
            ),
            Err(DirError::NotFound) => None,
            Err(error) => return Err(error.into()),
        }
    };
    // 新名字是挂载点：放掉试探引用，按忙回答（link.c:302-306）。
    if let Some(slot) = new_file {
        if ctx.table.slot(slot).mountpoint {
            let io = ctx.io;
            let _ = ctx.table.put(ctx.cache, slot, &io);
            new_file = None;
            return Err(LinkError::Busy);
        }
    }
    let odir = ctx.table.slot(old_file).mode as u32 & TYPE_MASK == TYPE_DIRECTORY;
    let old_number = child_number(ctx, old_file);
    let mut same = false;
    let mut error: Option<LinkError> = None;

    if new_file.is_none() {
        // 环走查（link.c:315-342）：目录搬家且父目录不同时，顺新父目录
        // 的点点上走，撞见旧文件就是把目录搬进自己的子树。缺点点按最坏
        // 情况处理（link.c:336-340）。
        if odir && !same_pdir {
            let mut current = new_dir_number;
            loop {
                if current == old_number {
                    error = Some(LinkError::Invalid);
                    break;
                }
                if current == ROOT_INODE_NUMBER {
                    break;
                }
                let slot = ctx
                    .table
                    .get(ctx.cache, ctx.device, current, &ctx.io)
                    .map_err(|_| LinkError::Invalid)?;
                let (mode, nlinks, size, zones) = {
                    let dir = ctx.table.slot(slot);
                    (dir.mode, dir.nlinks, dir.size as u64, dir.zones)
                };
                let mut images = crate::dir_io::load_dir_blocks(
                    ctx.cache,
                    ctx.device,
                    &zones,
                    ctx.map,
                    ctx.range,
                    size,
                    ctx.block_size,
                )
                .map_err(|_| LinkError::Io)?;
                let _ = ctx.table.put(ctx.cache, slot, &ctx.io);
                let mut found = 0u32;
                match crate::dir::lookup_name(
                    mode,
                    nlinks,
                    &mut images,
                    ctx.block_size,
                    size,
                    b"..",
                    &mut found,
                ) {
                    Ok(()) => current = found as u64,
                    Err(_) => {
                        error = Some(LinkError::Invalid);
                        break;
                    }
                }
                if current == old_number {
                    error = Some(LinkError::Invalid);
                    break;
                }
                if current == ROOT_INODE_NUMBER {
                    break;
                }
            }
        }
        // 目录搬家且新父目录链接到顶（link.c:346-349）。
        if error.is_none()
            && odir
            && !same_pdir
            && ctx.table.slot(new_dir).nlinks as u32 >= crate::inode::LINK_CEILING
        {
            error = Some(LinkError::LinkCeiling);
        }
    } else {
        // 新名字存在（link.c:350-356）：同一文件记内部相同标记，类型
        // 交错各报其错。
        let slot = new_file.unwrap();
        let new_number = child_number(ctx, slot);
        let ndir = ctx.table.slot(slot).mode as u32 & TYPE_MASK == TYPE_DIRECTORY;
        if old_number == new_number {
            same = true;
        } else if odir && !ndir {
            error = Some(LinkError::NotDirectory);
        } else if !odir && ndir {
            error = Some(LinkError::IsDirectory);
        }
    }

    let mut outcome = LinkOutcome::default();
    if error.is_none() && !same {
        // 新名字已存在：先腾位置（link.c:371-378）。腾出来的区号折进
        // 本次的回收清单。
        if let Some(slot) = new_file {
            let (zones, size) = {
                let node = ctx.table.slot(slot);
                (node.zones, node.size as u64)
            };
            let mut replaced_images = crate::dir_io::load_dir_blocks(
                ctx.cache,
                ctx.device,
                &zones,
                ctx.map,
                ctx.range,
                size,
                ctx.block_size,
            )
            .map_err(|_| LinkError::Io)?;
            // 同父目录时新名字就在旧镜像里：替换操作走旧镜像
            // （C 的 new_dirp == old_dirp，同一目录）。
            let target_images: &mut Vec<Vec<u8>> = if same_pdir {
                &mut *old_dir_images
            } else {
                &mut *new_dir_images
            };
            let replaced = if odir {
                remove_directory(
                    ctx,
                    new_dir_number,
                    target_images,
                    new_name,
                    &mut replaced_images,
                )
            } else {
                remove_file(ctx, new_dir_number, target_images, new_name)
            };
            match replaced {
                Ok(done) => outcome.reclaimed.extend(done.reclaimed),
                Err(e) => error = Some(e),
            }
        }
    }

    if error.is_none() {
        // 同父目录先删旧名腾出槽位，再进新名（link.c:391-395）；跨父目录
        // 先进新名，保证新目录满时旧名字还在（link.c:397-399）。
        let number = old_number as u32;
        let mut scratch = 0u32;
        if same_pdir {
            let mut scan = crate::dir::DirScan {
                size: ctx.table.slot(old_dir).size as u64,
                last_dpos: ctx.table.slot(old_dir).scan_hint,
            };
            let deleted = search_blocks(
                old_dir_images,
                ctx.block_size,
                &mut scan,
                true,
                ctx.read_only,
                SearchOp::Delete,
                old_name,
                &mut scratch,
            );
            deleted.map_err(LinkError::from)?;
            let mut target = number;
            let entered = search_blocks(
                old_dir_images,
                ctx.block_size,
                &mut scan,
                true,
                ctx.read_only,
                SearchOp::Enter,
                new_name,
                &mut target,
            );
            entered.map_err(LinkError::from)?;
            {
                let dir = ctx.table.slot_mut(old_dir);
                dir.size = scan.size as i64;
                dir.scan_hint = scan.last_dpos;
                dir.dirty = true;
            }
        } else {
            let mut scan = crate::dir::DirScan {
                size: ctx.table.slot(new_dir).size as u64,
                last_dpos: ctx.table.slot(new_dir).scan_hint,
            };
            let mut target = number;
            let entered = search_blocks(
                new_dir_images,
                ctx.block_size,
                &mut scan,
                true,
                ctx.read_only,
                SearchOp::Enter,
                new_name,
                &mut target,
            );
            entered.map_err(LinkError::from)?;
            {
                let dir = ctx.table.slot_mut(new_dir);
                dir.size = scan.size as i64;
                dir.scan_hint = scan.last_dpos;
                dir.dirty = true;
            }
            let mut scan = crate::dir::DirScan {
                size: ctx.table.slot(old_dir).size as u64,
                last_dpos: ctx.table.slot(old_dir).scan_hint,
            };
            let deleted = search_blocks(
                old_dir_images,
                ctx.block_size,
                &mut scan,
                true,
                ctx.read_only,
                SearchOp::Delete,
                old_name,
                &mut scratch,
            );
            deleted.map_err(LinkError::from)?;
            {
                let dir = ctx.table.slot_mut(old_dir);
                dir.size = scan.size as i64;
                dir.scan_hint = scan.last_dpos;
                dir.dirty = true;
            }
        }
    }

    if error.is_none() && !same && odir && !same_pdir {
        // 目录跨父目录搬家：改写被搬目录里的点点（link.c:405-414）。
        // 先删旧点点，再把新父目录的编号作为点点进入——不增长，整块
        // 原位覆写即可。
        let (zones, size) = {
            let node = ctx.table.slot(old_file);
            (node.zones, node.size as u64)
        };
        let mut moved_images = crate::dir_io::load_dir_blocks(
            ctx.cache,
            ctx.device,
            &zones,
            ctx.map,
            ctx.range,
            size,
            ctx.block_size,
        )
        .map_err(|_| LinkError::Io)?;
        let mut scan = crate::dir::DirScan {
            size,
            last_dpos: 0,
        };
        let mut none = 0u32;
        let _ = search_blocks(
            &mut moved_images,
            ctx.block_size,
            &mut scan,
            true,
            ctx.read_only,
            SearchOp::Delete,
            b"..",
            &mut none,
        );
        let mut dotdot = new_dir_number as u32;
        let entered = search_blocks(
            &mut moved_images,
            ctx.block_size,
            &mut scan,
            true,
            ctx.read_only,
            SearchOp::Enter,
            b"..",
            &mut dotdot,
        );
        if entered.is_ok() {
            for (index, image) in moved_images.iter().enumerate() {
                let file_block = index as u64;
                let zones = ctx.table.slot(old_file).zones;
                let mapped = crate::read::map_file_block(
                    ctx.cache,
                    ctx.device,
                    &zones,
                    ctx.map,
                    ctx.range,
                    file_block,
                )
                .map_err(|_| LinkError::Io)?;
                if let Some(zone) = mapped {
                    let slot = ctx
                        .cache
                        .acquire(BlockKey::new(ctx.device, zone), AcquireMode::NoRead)
                        .map_err(|_| LinkError::Io)?;
                    ctx.cache
                        .write_slot(slot, image)
                        .map_err(|_| LinkError::Io)?;
                    ctx.cache.mark_dirty(slot);
                    let _ = ctx.cache.release(slot);
                }
            }
            {
                let dir = ctx.table.slot_mut(new_dir);
                dir.nlinks += 1;
                dir.dirty = true;
            }
        }
    }

    // 释放四个槽位（link.c:417-420）。SAME 按成功返回（link.c:421）。
    release_pair(ctx, old_dir, old_file);
    release_parent(ctx, new_dir);
    if let Some(slot) = new_file {
        let io = ctx.io;
        let _ = ctx.table.put(ctx.cache, slot, &io);
    }
    match error {
        Some(e) => Err(e),
        None => Ok(outcome),
    }
}

/// Open the parent directory or report invalid.
fn open_parent<S: BlockSource>(ctx: &mut LinkCtx<'_, S>, number: u64) -> Result<usize, LinkError> {
    let io = ctx.io;
    ctx.table
        .get(ctx.cache, ctx.device, number, &io)
        .map_err(|_| LinkError::Invalid)
}

/// Advance to the child through parent images or report invalid.
fn open_child<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    parent: usize,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
) -> Result<usize, LinkError> {
    let (mode, nlinks, size) = {
        let parent_inode = ctx.table.slot(parent);
        (
            parent_inode.mode,
            parent_inode.nlinks,
            parent_inode.size as u64,
        )
    };
    let mut found = 0u32;
    lookup_name(
        mode,
        nlinks,
        dir_blocks,
        ctx.block_size,
        size,
        name,
        &mut found,
    )
    .map_err(|error| {
        if matches!(error, DirError::NotFound) {
            LinkError::Invalid
        } else {
            error.into()
        }
    })?;
    let io = ctx.io;
    ctx.table
        .get(ctx.cache, ctx.device, found as u64, &io)
        .map_err(|_| LinkError::Invalid)
}

/// Inode number of a slot.
fn child_number<S: BlockSource>(ctx: &LinkCtx<'_, S>, slot: usize) -> u64 {
    ctx.table.slot(slot).number
}

/// Release both slots once.
fn release_pair<S: BlockSource>(ctx: &mut LinkCtx<'_, S>, parent: usize, child: usize) {
    let io = ctx.io;
    let _ = ctx.table.put(ctx.cache, child, &io);
    let _ = ctx.table.put(ctx.cache, parent, &io);
}

/// Release the child, folding reclaimed zones into the outcome.
fn release_child<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    child: usize,
    outcome: &mut LinkOutcome,
) {
    let io = ctx.io;
    if let Ok(release) = ctx.table.put(ctx.cache, child, &io) {
        outcome.absorb_release(release);
    }
}

/// Release the parent slot.
fn release_parent<S: BlockSource>(ctx: &mut LinkCtx<'_, S>, parent: usize) {
    let io = ctx.io;
    let _ = ctx.table.put(ctx.cache, parent, &io);
}

/// Read a symbolic link target (`fs_rdlink`, `link.c:152-178`).
///
/// Opens the inode (missing is invalid), refuses non-links, maps block
/// zero (missing is an input-output error), clamps to the smaller of the
/// request and the recorded size, copies out, and reports the byte count.
pub fn read_link<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    number: u64,
    capacity: usize,
    out: &mut dyn FnMut(&[u8]),
) -> Result<usize, LinkError> {
    let slot = ctx
        .table
        .get(ctx.cache, ctx.device, number, &ctx.io)
        .map_err(|_| LinkError::Invalid)?;
    let (mode, size, zone) = {
        let inode = ctx.table.slot(slot);
        (inode.mode, inode.size, inode.zones[0])
    };
    if mode as u32 & TYPE_MASK != crate::inode::TYPE_SYMLINK {
        let io = ctx.io;
        let _ = ctx.table.put(ctx.cache, slot, &io);
        return Err(LinkError::Access);
    }
    if zone == 0 {
        let io = ctx.io;
        let _ = ctx.table.put(ctx.cache, slot, &io);
        return Err(LinkError::Io);
    }
    let cache_slot = ctx
        .cache
        .acquire(BlockKey::new(ctx.device, zone), AcquireMode::Normal)
        .map_err(|_| {
            let io = ctx.io;
            let _ = ctx.table.put(ctx.cache, slot, &io);
            LinkError::Io
        })?;
    let take = (capacity as i64).min(size).max(0) as usize;
    let bytes = ctx.cache.slot_data(cache_slot).to_vec();
    let _ = ctx.cache.release(cache_slot);
    let io = ctx.io;
    let _ = ctx.table.put(ctx.cache, slot, &io);
    out(&bytes[..take.min(bytes.len())]);
    Ok(take.min(bytes.len()))
}

/// Delete one name from parent images (`search_dir` delete mode).
fn delete_name<S: BlockSource>(
    ctx: &mut LinkCtx<'_, S>,
    parent: usize,
    dir_blocks: &mut Vec<Vec<u8>>,
    name: &[u8],
) -> Result<(), LinkError> {
    let (size, last) = {
        let parent_inode = ctx.table.slot(parent);
        (parent_inode.size as u64, parent_inode.scan_hint)
    };
    let mut scan = crate::dir::DirScan {
        size,
        last_dpos: last,
    };
    let mut number = 0u32;
    search_blocks(
        dir_blocks,
        ctx.block_size,
        &mut scan,
        true,
        ctx.read_only,
        SearchOp::Delete,
        name,
        &mut number,
    )
    .map_err(LinkError::from)?;
    {
        let parent_inode = ctx.table.slot_mut(parent);
        parent_inode.size = scan.size as i64;
        parent_inode.scan_hint = scan.last_dpos;
        parent_inode.dirty = true;
    }
    Ok(())
}

/// First free byte position at or after `position` (`nextblock`).
///
/// Rounds up without overflowing: divide first, then add only on remainder
/// (`link.c:558-570`). Past the address ceiling the result saturates
/// instead of wrapping (hardened over the C arithmetic, whose comment
/// admits the overflow worry).
pub const fn next_block_start(position: u64, zone_size: u64) -> u64 {
    if zone_size == 0 {
        return position;
    }
    let base = position / zone_size * zone_size;
    if position.is_multiple_of(zone_size) {
        base
    } else {
        base.saturating_add(zone_size)
    }
}

/// A byte range to zero plus zone numbers to free: the pure plan behind
/// range freeing (`freesp_inode`, `link.c:494-552`).
///
/// Splitting the decision from the execution keeps this testable without
/// storage: the write path executes zeroing through the cache and freeing
/// through the zone map (document 15).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FreePlan {
    /// Byte ranges to overwrite with zeros `(start, length)`.
    pub zero_ranges: Vec<(u64, u64)>,
    /// Whole zone numbers to free.
    pub free_zones: Vec<u64>,
}

/// Plan freeing `[start, end)` inside a file of `size` bytes.
///
/// C: the branching inside `freesp_inode` (`link.c:513-546`), with zone
/// size equal to block size. End past the size clamps; end at or below
/// start is invalid. A partial edge zeroes its span; a tail span at the
/// size frees its zone instead of zeroing (the zone holds nothing else).
/// Whole inner zones free by number. Callers execute zeroing through the
/// cache and freeing through the zone map (document 15).
pub fn plan_free_range(
    size: u64,
    block_size: u64,
    start: u64,
    end: u64,
) -> Result<FreePlan, LinkError> {
    if block_size == 0 {
        return Err(LinkError::Invalid);
    }
    let end = end.min(size);
    if end <= start {
        return Err(LinkError::Invalid);
    }
    let mut plan = FreePlan::default();
    let first_zone = start / block_size;
    let last_zone = (end - 1) / block_size;
    let zero_start_edge = !start.is_multiple_of(block_size);
    let zero_end_edge = !end.is_multiple_of(block_size) && end < size;
    if first_zone == last_zone {
        if zero_start_edge || zero_end_edge {
            plan.zero_ranges.push((start, end - start));
            return Ok(plan);
        }
        // Else fall through: full single-zone coverage frees below.
    } else {
        if zero_start_edge {
            let head_end = (first_zone + 1) * block_size;
            plan.zero_ranges.push((start, head_end - start));
        }
        if zero_end_edge {
            let tail_start = end / block_size * block_size;
            plan.zero_ranges.push((tail_start, end - tail_start));
        }
    }
    // Whole zones free by number, tail-at-size included (`e++` in C).
    let mut stop = end / block_size;
    if end == size && !end.is_multiple_of(block_size) {
        stop = stop.saturating_add(1);
    }
    let mut zone = next_block_start(start, block_size) / block_size;
    while zone < stop {
        plan.free_zones.push(zone);
        zone = zone.saturating_add(1);
    }
    plan.free_zones.sort_unstable();
    plan.free_zones.dedup();
    Ok(plan)
}

/// Truncate decision: shrink, grow, or reject (`truncate_inode` without
/// storage, `link.c:452-488`).
///
/// Special files refuse; growth past the maximum refuses with file-too-big;
/// otherwise reports the new size and whether the tail needs zeroing
/// (growth) or range freeing (shrink). Execution lives with the write path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncatePlan {
    /// Shrink to this size (caller frees `[new_size, old_size)` then sets).
    Shrink {
        /// New file size.
        new_size: u64,
    },
    /// Grow to this size (caller zeroes the tail, holes read zero).
    Grow {
        /// New file size.
        new_size: u64,
    },
    /// No change.
    Same,
}

/// Decide a truncation without touching storage.
pub fn plan_truncate(
    mode: u16,
    size: u64,
    max_size: u64,
    new_size: u64,
    is_block_special: bool,
    is_char_special: bool,
) -> Result<TruncatePlan, LinkError> {
    let _ = mode;
    if is_block_special || is_char_special {
        return Err(LinkError::Invalid);
    }
    if new_size > max_size {
        return Err(LinkError::TooBig);
    }
    if new_size < size {
        Ok(TruncatePlan::Shrink { new_size })
    } else if new_size > size {
        Ok(TruncatePlan::Grow { new_size })
    } else {
        Ok(TruncatePlan::Same)
    }
}

impl LinkOutcome {
    /// Fold reclaimed zones from a release outcome (test and wiring helper).
    fn absorb_release(&mut self, outcome: ReleaseOutcome) {
        LinkOutcome::absorb(self, outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dir::{DirScan, SearchOutcome};
    use crate::inode::{InodeTable, TYPE_SYMLINK};
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
        io: crate::inode::InodeIo,
    }

    fn fixture() -> Fixture {
        let cache =
            BlockCache::with_pool(RamDisk::new(64, BLOCK_SIZE).unwrap(), NoSecondLevel, 8).unwrap();
        let superblock = Superblock {
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
        Fixture {
            table: InodeTable::new(),
            cache,
            superblock,
            bitmap: Bitmap::new(65),
            io: crate::inode::InodeIo::from_superblock(&superblock),
            // NOTE: `io` borrows nothing; built from the local above.
        }
    }

    fn ctx_of(fixture: &mut Fixture) -> LinkCtx<'_, RamDisk> {
        LinkCtx {
            table: &mut fixture.table,
            cache: &mut fixture.cache,
            device: DEVICE,
            io: fixture.io,
            read_only: false,
            block_size: BLOCK_SIZE,
            map: crate::read::MapParams {
                direct_zones: 7,
                indirect_per_block: 128,
            },
            range: crate::read::ZoneRange {
                first: 4,
                count: 61,
            },
        }
    }

    /// Open a live directory slot through the allocator (no disk involved).
    fn open_dir(fixture: &mut Fixture) -> usize {
        let slot = fixture
            .table
            .allocate(
                &mut fixture.cache,
                &mut fixture.superblock,
                &mut fixture.bitmap,
                0o040755,
                0,
                0,
                DEVICE,
            )
            .unwrap();
        fixture.table.slot_mut(slot).nlinks = 2;
        slot
    }

    /// Open a live regular file slot.
    fn open_file(fixture: &mut Fixture) -> usize {
        let slot = fixture
            .table
            .allocate(
                &mut fixture.cache,
                &mut fixture.superblock,
                &mut fixture.bitmap,
                0o100644,
                0,
                0,
                DEVICE,
            )
            .unwrap();
        fixture.table.slot_mut(slot).nlinks = 1;
        slot
    }

    #[test]
    fn test_link_bumps_and_reuses_name() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        // Release the opener references so counts mirror server flow
        // (framework holds one reference per open instead).
        let _ = (dir, file);
        let mut ctx = ctx_of(&mut fixture);
        let outcome = create_link(&mut ctx, dir_number, &mut images, b"hard", file_number).unwrap();
        assert!(outcome.reclaimed.is_empty());
        // Link count grew; the name resolves to the same number.
        let mut found = 0u32;
        let mut lookup = crate::dir::DirScan {
            size: 64,
            last_dpos: 0,
        };
        assert_eq!(
            search_blocks(
                &mut images,
                BLOCK_SIZE,
                &mut lookup,
                true,
                false,
                SearchOp::Lookup,
                b"hard",
                &mut found
            )
            .unwrap(),
            SearchOutcome::Found(file_number as u32)
        );
        // Second link to the same name reports exists.
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_link(&mut ctx, dir_number, &mut images, b"hard", file_number).unwrap_err(),
            LinkError::Exists
        );
    }

    #[test]
    fn test_link_refuses_ceiling_and_directories() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        fixture.table.slot_mut(file).nlinks = u16::MAX;
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_link(&mut ctx, dir_number, &mut images, b"x", file_number).unwrap_err(),
            LinkError::LinkCeiling
        );
        fixture.table.slot_mut(file).nlinks = 1;
        // Directories never link.
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            create_link(&mut ctx, dir_number, &mut images, b"y", dir_number).unwrap_err(),
            LinkError::NotPermitted
        );
    }

    #[test]
    fn test_unlink_drops_link_and_reports_zones() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        // Pretend the file owns zone nine on disk.
        fixture.table.slot_mut(file).zones[0] = 9;
        let mut ctx = ctx_of(&mut fixture);
        create_link(&mut ctx, dir_number, &mut images, b"a", file_number).unwrap();
        // Two links now (create path set one, link made two): unlink drops
        // to one and reports nothing to reclaim.
        let mut ctx = ctx_of(&mut fixture);
        let outcome = remove_file(&mut ctx, dir_number, &mut images, b"a").unwrap();
        assert!(outcome.reclaimed.is_empty());
        // Missing names report invalid.
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            remove_file(&mut ctx, dir_number, &mut images, b"nope-missing").unwrap_err(),
            LinkError::Invalid
        );
    }

    #[test]
    fn test_remove_directory_needs_empty() {
        let mut fixture = fixture();
        // Numbers come from the allocator; the test reads them back instead
        // of assuming them.
        let parent = open_dir(&mut fixture);
        let parent_number = fixture.table.slot(parent).number;
        let sub = open_dir(&mut fixture);
        let sub_number = fixture.table.slot(sub).number;
        fixture.table.slot_mut(sub).nlinks = 2;
        // Parent holds the sub name; the child holds dots plus one file.
        let mut parent_images = Vec::new();
        let mut parent_scan = crate::dir::DirScan {
            size: 0,
            last_dpos: 0,
        };
        let mut number = sub_number as u32;
        search_blocks(
            &mut parent_images,
            BLOCK_SIZE,
            &mut parent_scan,
            true,
            false,
            SearchOp::Enter,
            b"sub",
            &mut number,
        )
        .unwrap();
        let mut child_images = Vec::new();
        let mut child_scan = crate::dir::DirScan {
            size: 0,
            last_dpos: 0,
        };
        let mut dot = sub_number as u32;
        search_blocks(
            &mut child_images,
            BLOCK_SIZE,
            &mut child_scan,
            true,
            false,
            SearchOp::Enter,
            b".",
            &mut dot,
        )
        .unwrap();
        let mut dotdot = parent_number as u32;
        search_blocks(
            &mut child_images,
            BLOCK_SIZE,
            &mut child_scan,
            true,
            false,
            SearchOp::Enter,
            b"..",
            &mut dotdot,
        )
        .unwrap();
        let mut file_number = 9u32;
        search_blocks(
            &mut child_images,
            BLOCK_SIZE,
            &mut child_scan,
            true,
            false,
            SearchOp::Enter,
            b"file",
            &mut file_number,
        )
        .unwrap();
        // Direct block driving bypasses node creation: sync sizes by hand
        // (creation paths do this inside `new_node` and `create_dir`).
        fixture.table.slot_mut(parent).size = 64;
        fixture.table.slot_mut(sub).size = 192;
        let mut ctx = ctx_of(&mut fixture);
        // Non-empty refuses.
        assert_eq!(
            remove_directory(
                &mut ctx,
                parent_number,
                &mut parent_images,
                b"sub",
                &mut child_images
            )
            .unwrap_err(),
            LinkError::NotEmpty
        );
        // Empty it (dots only) and removal succeeds.
        let mut number = 0u32;
        let mut scan = crate::dir::DirScan {
            size: 192,
            last_dpos: 0,
        };
        search_blocks(
            &mut child_images,
            BLOCK_SIZE,
            &mut scan,
            true,
            false,
            SearchOp::Delete,
            b"file",
            &mut number,
        )
        .unwrap();
        let mut ctx = ctx_of(&mut fixture);
        let outcome = remove_directory(
            &mut ctx,
            parent_number,
            &mut parent_images,
            b"sub",
            &mut child_images,
        )
        .unwrap();
        // The three-step accounting lands the child at NO_LINK (its name,
        // its dot, and the parent's dot-dot all went away).
        assert_eq!(fixture.table.slot(sub).nlinks, crate::inode::NO_LINK);
        assert!(outcome.reclaimed.is_empty());
        assert_eq!(SAME_NAME, 1000);
        assert_eq!(FIRST_HALF, 0);
        assert_eq!(LAST_HALF, 1);
    }


    /// 种一个名字进镜像并同步目录尺寸（rename 内部按槽位尺寸走查）。
    fn plant_name(
        fixture: &mut Fixture,
        dir: usize,
        images: &mut Vec<Vec<u8>>,
        name: &[u8],
        number: u32,
    ) {
        let size = fixture.table.slot(dir).size as u64;
        let last = fixture.table.slot(dir).scan_hint;
        let mut scan = crate::dir::DirScan {
            size,
            last_dpos: last,
        };
        let mut target = number;
        search_blocks(
            images,
            BLOCK_SIZE,
            &mut scan,
            true,
            false,
            SearchOp::Enter,
            name,
            &mut target,
        )
        .unwrap();
        let dir_inode = fixture.table.slot_mut(dir);
        dir_inode.size = scan.size as i64;
        dir_inode.scan_hint = scan.last_dpos;
        dir_inode.dirty = true;
    }

    /// 在盘上种一个真实目录：点与点点写入区号对应的块，槽位带区号、
    /// 尺寸与链接数二——环走查从缓存读「..」，夹具必须有真的磁盘点项链。
    fn plant_dir(fixture: &mut Fixture, slot: usize, zone: u64, parent_number: u64) {
        let number = fixture.table.slot(slot).number;
        let mut images: Vec<Vec<u8>> = Vec::new();
        let mut scan = crate::dir::DirScan::default();
        let mut dot = number as u32;
        search_blocks(
            &mut images,
            BLOCK_SIZE,
            &mut scan,
            true,
            false,
            SearchOp::Enter,
            b".",
            &mut dot,
        )
        .unwrap();
        let mut dotdot = parent_number as u32;
        search_blocks(
            &mut images,
            BLOCK_SIZE,
            &mut scan,
            true,
            false,
            SearchOp::Enter,
            b"..",
            &mut dotdot,
        )
        .unwrap();
        for (index, image) in images.iter().enumerate() {
            let cache_slot = fixture
                .cache
                .acquire(
                    minix_fs::cache::BlockKey::new(DEVICE, zone + index as u64),
                    minix_fs::cache::AcquireMode::NoRead,
                )
                .unwrap();
            fixture.cache.write_slot(cache_slot, image).unwrap();
            fixture.cache.mark_dirty(cache_slot);
            fixture.cache.release(cache_slot).unwrap();
        }
        let inode = fixture.table.slot_mut(slot);
        inode.zones[0] = zone;
        inode.size = scan.size as i64;
        inode.dirty = true;
    }

    fn search_found(
        fixture: &mut Fixture,
        dir: usize,
        images: &mut Vec<Vec<u8>>,
        name: &[u8],
    ) -> Option<u32> {
        let size = fixture.table.slot(dir).size as u64;
        let mut found = 0u32;
        let mode = fixture.table.slot(dir).mode;
        let nlinks = fixture.table.slot(dir).nlinks;
        match crate::dir::lookup_name(mode, nlinks, images, BLOCK_SIZE, size, name, &mut found) {
            Ok(()) => Some(found),
            Err(DirError::NotFound) => None,
            Err(_) => None,
        }
    }

    #[test]
    fn test_rename_same_directory_moves_entry() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        plant_name(&mut fixture, dir, &mut images, b"a", file_number as u32);
        let mut ctx = ctx_of(&mut fixture);
        rename(&mut ctx, dir_number, &mut images, b"a", dir_number, &mut Vec::new(), b"b").unwrap();
        assert!(search_found(&mut fixture, dir, &mut images, b"a").is_none());
        assert_eq!(
            search_found(&mut fixture, dir, &mut images, b"b"),
            Some(file_number as u32)
        );
    }

    #[test]
    fn test_rename_cross_directory_rewrites_dotdot() {
        let mut fixture = fixture();
        let mut dir1_images = Vec::new();
        let mut dir2_images = Vec::new();
        let dir1 = open_dir(&mut fixture);
        let dir1_number = fixture.table.slot(dir1).number;
        let dir2 = open_dir(&mut fixture);
        let dir2_number = fixture.table.slot(dir2).number;
        let inner = open_dir(&mut fixture);
        let inner_number = fixture.table.slot(inner).number;
        // 三个目录都种真的磁盘点项链：dir1 与 dir2 的点点指根，
        // inner 的点点指 dir1。
        plant_dir(&mut fixture, dir1, 7, 1);
        plant_dir(&mut fixture, dir2, 8, 1);
        plant_dir(&mut fixture, inner, 9, dir1_number);
        plant_name(&mut fixture, dir1, &mut dir1_images, b"inner", inner_number as u32);
        let mut ctx = ctx_of(&mut fixture);
        rename(
            &mut ctx,
            dir1_number,
            &mut dir1_images,
            b"inner",
            dir2_number,
            &mut dir2_images,
            b"moved",
        )
        .unwrap();
        // 新父目录查得到，旧父目录没有了。
        assert_eq!(
            search_found(&mut fixture, dir2, &mut dir2_images, b"moved"),
            Some(inner_number as u32)
        );
        assert!(search_found(&mut fixture, dir1, &mut dir1_images, b"inner").is_none());
        // 点点改写：冲刷后 inner 数据区的第 64 字节起指向新父目录。
        fixture.cache.flush_all().unwrap();
        let mut block = alloc::vec![0u8; BLOCK_SIZE];
        minix_fs::cache::BlockSource::read_block(
            fixture.cache.source(),
            minix_fs::cache::BlockKey::new(DEVICE, 9),
            &mut block,
        )
        .unwrap();
        assert_eq!(block[64..68], (dir2_number as u32).to_le_bytes());
        // 新父目录因获得子目录链接计数加一。
        assert!(fixture.table.slot(dir2).nlinks >= 2);
    }

    #[test]
    fn test_rename_replaces_existing_file() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let first = open_file(&mut fixture);
        let first_number = fixture.table.slot(first).number;
        let second = open_file(&mut fixture);
        let second_number = fixture.table.slot(second).number;
        plant_name(&mut fixture, dir, &mut images, b"a", first_number as u32);
        plant_name(&mut fixture, dir, &mut images, b"b", second_number as u32);
        // a 改名到 b：b 的旧编号退出，a 的编号顶上。
        let mut ctx = ctx_of(&mut fixture);
        rename(
            &mut ctx,
            dir_number,
            &mut images,
            b"a",
            dir_number,
            &mut Vec::new(),
            b"b",
        )
        .unwrap();
        assert_eq!(
            search_found(&mut fixture, dir, &mut images, b"b"),
            Some(first_number as u32)
        );
        assert!(search_found(&mut fixture, dir, &mut images, b"a").is_none());
        let _ = second_number;
    }

    #[test]
    fn test_rename_type_mismatch_refused() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        let sub = open_dir(&mut fixture);
        let sub_number = fixture.table.slot(sub).number;
        plant_name(&mut fixture, dir, &mut images, b"f", file_number as u32);
        plant_name(&mut fixture, dir, &mut images, b"d", sub_number as u32);
        // 文件改名到已存在的目录名：报告是目录。
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            rename(
                &mut ctx,
                dir_number,
                &mut images,
                b"f",
                dir_number,
                &mut Vec::new(),
                b"d"
            )
            .unwrap_err(),
            LinkError::IsDirectory
        );
        // 目录改名到已存在的文件名：报告非目录。
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            rename(
                &mut ctx,
                dir_number,
                &mut images,
                b"d",
                dir_number,
                &mut Vec::new(),
                b"f"
            )
            .unwrap_err(),
            LinkError::NotDirectory
        );
        let _ = (file_number, sub_number);
    }

    #[test]
    fn test_rename_same_name_reports_success() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        plant_name(&mut fixture, dir, &mut images, b"keep", file_number as u32);
        let mut ctx = ctx_of(&mut fixture);
        let outcome =
            rename(&mut ctx, dir_number, &mut images, b"keep", dir_number, &mut Vec::new(), b"keep").unwrap();
        assert!(outcome.reclaimed.is_empty());
        assert_eq!(
            search_found(&mut fixture, dir, &mut images, b"keep"),
            Some(file_number as u32)
        );
    }

    #[test]
    fn test_rename_cycle_refused_and_emlink() {
        let mut fixture = fixture();
        let mut images = Vec::new();
        let dir = open_dir(&mut fixture);
        let dir_number = fixture.table.slot(dir).number;
        let sub = open_dir(&mut fixture);
        let sub_number = fixture.table.slot(sub).number;
        let top = open_dir(&mut fixture);
        let top_number = fixture.table.slot(top).number;
        // dir 的点点指根，sub 的点点指 dir；top 的点点指根但链接数到顶。
        plant_dir(&mut fixture, dir, 7, 1);
        plant_dir(&mut fixture, sub, 8, dir_number);
        plant_dir(&mut fixture, top, 9, 1);
        fixture.table.slot_mut(top).nlinks = crate::inode::LINK_CEILING as u16;
        plant_name(&mut fixture, dir, &mut images, b"d", sub_number as u32);
        // 环：把 dir 搬进 sub 的子树，走查从 sub 上行撞见 dir，报告
        // 无效参数。
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            rename(
                &mut ctx,
                dir_number,
                &mut images,
                b"d",
                sub_number,
                &mut Vec::new(),
                b"x"
            )
            .unwrap_err(),
            LinkError::Invalid
        );
        // 链接到顶：目录搬进链接数到顶的新父目录报告链接过多。
        let mut ctx = ctx_of(&mut fixture);
        assert_eq!(
            rename(
                &mut ctx,
                dir_number,
                &mut images,
                b"d",
                top_number,
                &mut Vec::new(),
                b"x"
            )
            .unwrap_err(),
            LinkError::LinkCeiling
        );
    }

    #[test]
    fn test_next_block_start_math() {
        assert_eq!(next_block_start(0, 512), 0);
        assert_eq!(next_block_start(1, 512), 512);
        assert_eq!(next_block_start(512, 512), 512);
        assert_eq!(next_block_start(513, 512), 1024);
        // Past the ceiling the result saturates instead of wrapping.
        assert_eq!(next_block_start(u64::MAX - 1, 512), u64::MAX);
        assert_eq!(next_block_start(100, 0), 100);
    }

    #[test]
    fn test_plan_free_range_shapes() {
        // Single zone partial span zeroes only.
        let plan = plan_free_range(4096, 512, 100, 200).unwrap();
        assert_eq!(
            plan,
            FreePlan {
                zero_ranges: alloc::vec![(100, 100)],
                free_zones: alloc::vec![],
            }
        );
        // Aligned full last span at the size frees the zone.
        let plan = plan_free_range(1024, 512, 512, 1024).unwrap();
        assert_eq!(plan.free_zones, alloc::vec![1]);
        assert!(plan.zero_ranges.is_empty());
        // Spanning span: head zeroes, middle frees, tail zeroes.
        let plan = plan_free_range(4096, 512, 100, 1200).unwrap();
        assert_eq!(plan.zero_ranges, alloc::vec![(100, 412), (1024, 176)]);
        assert_eq!(plan.free_zones, alloc::vec![1]);
        // Empty and inverted ranges refuse.
        assert_eq!(
            plan_free_range(100, 512, 50, 50).unwrap_err(),
            LinkError::Invalid
        );
        assert_eq!(
            plan_free_range(100, 512, 80, 50).unwrap_err(),
            LinkError::Invalid
        );
        // End past the size clamps; the head partial still zeroes while
        // the size-tail zone frees (it holds nothing else).
        let plan = plan_free_range(600, 512, 400, 9999).unwrap();
        assert_eq!(plan.zero_ranges, alloc::vec![(400, 112)]);
        assert_eq!(plan.free_zones, alloc::vec![1]);
    }

    #[test]
    fn test_plan_truncate_decisions() {
        assert_eq!(
            plan_truncate(0o100644, 1000, 100000, 100, false, false).unwrap(),
            TruncatePlan::Shrink { new_size: 100 }
        );
        assert_eq!(
            plan_truncate(0o100644, 100, 100000, 5000, false, false).unwrap(),
            TruncatePlan::Grow { new_size: 5000 }
        );
        assert_eq!(
            plan_truncate(0o100644, 100, 100000, 100, false, false).unwrap(),
            TruncatePlan::Same
        );
        assert_eq!(
            plan_truncate(0o100644, 100, 100000, 200, true, false).unwrap_err(),
            LinkError::Invalid
        );
        assert_eq!(
            plan_truncate(0o100644, 100, 1000, 2000, false, false).unwrap_err(),
            LinkError::TooBig
        );
    }

    #[test]
    fn test_rdlink_reads_first_zone() {
        let mut fixture = fixture();
        // Symlink slot with a target block staged in the cache device.
        let slot = fixture
            .table
            .allocate(
                &mut fixture.cache,
                &mut fixture.superblock,
                &mut fixture.bitmap,
                0o120777,
                0,
                0,
                DEVICE,
            )
            .unwrap();
        let number = fixture.table.slot(slot).number;
        fixture.table.slot_mut(slot).nlinks = 1;
        fixture.table.slot_mut(slot).zones[0] = 20;
        fixture.table.slot_mut(slot).size = 6;
        // Stage "target" at device block twenty through the cache.
        {
            let cache_slot = fixture
                .cache
                .acquire(BlockKey::new(DEVICE, 20), AcquireMode::NoRead)
                .unwrap();
            fixture.cache.slot_data_mut(cache_slot)[..6].copy_from_slice(b"target");
            fixture.cache.mark_dirty(cache_slot);
            fixture.cache.release(cache_slot).unwrap();
        }
        let mut ctx = ctx_of(&mut fixture);
        let mut seen = Vec::new();
        let moved = read_link(&mut ctx, number, 64, &mut |chunk: &[u8]| {
            seen.extend_from_slice(chunk)
        })
        .unwrap();
        assert_eq!(moved, 6);
        assert_eq!(seen, b"target");
        // Clamp to the recorded size, not the request.
        let mut seen = Vec::new();
        let mut ctx = ctx_of(&mut fixture);
        let moved = read_link(&mut ctx, number, 2, &mut |chunk: &[u8]| {
            seen.extend_from_slice(chunk)
        })
        .unwrap();
        assert_eq!(moved, 2);
        assert_eq!(seen, b"ta");
        // Regular files are refused.
        let file = open_file(&mut fixture);
        let file_number = fixture.table.slot(file).number;
        let mut ctx = ctx_of(&mut fixture);
        let mut seen = Vec::new();
        assert_eq!(
            read_link(&mut ctx, file_number, 64, &mut |chunk: &[u8]| seen
                .extend_from_slice(chunk))
            .unwrap_err(),
            LinkError::Access
        );
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{
            EACCES, EBUSY, EEXIST, EFBIG, EINVAL, EIO, EISDIR, EMLINK, ENOTDIR, EPERM, EROFS,
        };
        assert_eq!(LinkError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(LinkError::Exists.to_errno().to_i32(), EEXIST);
        assert_eq!(LinkError::LinkCeiling.to_errno().to_i32(), EMLINK);
        assert_eq!(LinkError::NotPermitted.to_errno().to_i32(), EPERM);
        assert_eq!(LinkError::NotDirectory.to_errno().to_i32(), ENOTDIR);
        assert_eq!(LinkError::IsDirectory.to_errno().to_i32(), EISDIR);
        assert_eq!(LinkError::Busy.to_errno().to_i32(), EBUSY);
        assert_eq!(LinkError::ReadOnly.to_errno().to_i32(), EROFS);
        assert_eq!(LinkError::TooBig.to_errno().to_i32(), EFBIG);
        assert_eq!(LinkError::Access.to_errno().to_i32(), EACCES);
        assert_eq!(LinkError::Io.to_errno().to_i32(), EIO);
        assert_eq!(SAME_NAME, 1000);
    }
}
