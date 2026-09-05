//! File metadata: permissions, ownership, times, status, and conversion.
//!
//! C correspondence: `minix3/minix/fs/mfs/protect.c` (fifty-eight lines:
//! `fs_chmod`, `fs_chown`), `stadir.c` (one hundred four lines:
//! `estimate_blocks`, `fs_stat`, `fs_statvfs`), `time.c` (forty-nine lines:
//! `fs_utime`), `utility.c` (thirty-six lines: `conv2`, `conv4`). Metadata
//! reads and writes a slot's fields; only the filesystem-statistics call
//! touches the cache (to count free inode bits), and only the block estimate
//! is arithmetic.
//!
//! Timestamps have second resolution on disk: explicit times round down,
//! and the two sentinel nanosecond values select "now" or "leave alone".

use minix_types::{EINVAL, EROFS, Errno};

use minix_fs::cache::{BlockCache, BlockKey, BlockSource, NoSecondLevel};

use crate::inode::{InodeTable, TYPE_BLOCK, TYPE_CHARACTER, TYPE_MASK};

/// Permission bits preserved across mode changes (`ALL_MODES`,
/// `const.h:115`, octal `0007777`).
pub const MODE_MASK: u32 = 0o7777;
/// Set-user-ID bit (`I_SET_UID_BIT`, octal `0004000`).
pub const SET_USER_ID: u32 = 0o4000;
/// Set-group-ID bit (`I_SET_GID_BIT`, octal `0002000`).
pub const SET_GROUP_ID: u32 = 0o2000;

/// "Now" nanoseconds selector (`UTIME_NOW`, `stat.h:235`).
pub const NSEC_NOW: i64 = (1 << 30) - 1;
/// "Leave alone" nanoseconds selector (`UTIME_OMIT`, `stat.h:236`).
pub const NSEC_OMIT: i64 = (1 << 30) - 2;

/// A timestamp as seconds plus a nanosecond selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSpec {
    /// Whole seconds.
    pub seconds: i64,
    /// Nanoseconds, or one of the two selectors.
    pub nanoseconds: i64,
}

impl TimeSpec {
    /// "Now" selector.
    pub const fn now() -> Self {
        Self {
            seconds: 0,
            nanoseconds: NSEC_NOW,
        }
    }

    /// "Leave alone" selector.
    pub const fn omit() -> Self {
        Self {
            seconds: 0,
            nanoseconds: NSEC_OMIT,
        }
    }

    /// Explicit stamp (subsecond parts round down on disk).
    pub const fn stamp(seconds: i64) -> Self {
        Self {
            seconds,
            nanoseconds: 0,
        }
    }
}

/// Why a metadata call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaError {
    /// Bad request (unknown inode).
    Invalid,
    /// Write on a read-only mount (`EROFS`).
    ReadOnly,
}

impl MetaError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::ReadOnly => Errno::from_i32(EROFS),
        }
    }
}

/// Change permission bits (`fs_chmod`, `protect.c:9-33`).
///
/// Opens the file (missing is invalid), refuses read-only mounts first
/// (releasing on the way out). Keeps the type bits, replaces the permission
/// bits, stamps change time, dirties, releases, and reports the new mode
/// back.
pub fn change_mode<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    device: u64,
    number: u64,
    read_only: bool,
    mode: u16,
) -> Result<u16, MetaError> {
    let slot = find_open(table, cache, io, device, number)?;
    if read_only {
        release(table, cache, io, slot);
        return Err(MetaError::ReadOnly);
    }
    let inode = table.slot_mut(slot);
    inode.mode = (inode.mode & !MODE_MASK as u16) | (mode & MODE_MASK as u16);
    inode.pending_updates |= crate::inode::UPDATE_CHANGE;
    inode.dirty = true;
    let mode = table.slot(slot).mode;
    release(table, cache, io, slot);
    Ok(mode)
}

/// Change owner and group (`fs_chown`, `protect.c:39-58`).
///
/// Opens the file, sets both identifiers, clears both set-ID bits (a
/// changed owner keeps no privileges), stamps change time, dirties,
/// releases, and reports the mode back (it may have changed through the
/// bit clearing).
pub fn change_owner<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    device: u64,
    number: u64,
    owner: u16,
    group: u16,
) -> Result<u16, MetaError> {
    let slot = find_open(table, cache, io, device, number)?;
    {
        let inode = table.slot_mut(slot);
        inode.owner = owner;
        inode.group = group;
        inode.mode &= !(SET_USER_ID as u16 | SET_GROUP_ID as u16);
        inode.pending_updates |= crate::inode::UPDATE_CHANGE;
        inode.dirty = true;
    }
    let mode = table.slot(slot).mode;
    release(table, cache, io, slot);
    Ok(mode)
}

/// Set access and modification times (`fs_utime`, `time.c:11-48`).
///
/// Opens the file, then starts by discarding stale pending flags (change
/// time always stamps). "Now" arms the lazy flag, "omit" skips, explicit
/// stamps round down to seconds (no subsecond resolution on disk). Dirties
/// unconditionally and releases, like the C code marking dirty outside the
/// switch.
pub fn update_times<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    device: u64,
    number: u64,
    accessed: TimeSpec,
    modified: TimeSpec,
) -> Result<(), MetaError> {
    let slot = find_open(table, cache, io, device, number)?;
    {
        let inode = table.slot_mut(slot);
        inode.pending_updates = crate::inode::UPDATE_CHANGE;
        apply_stamp(inode, accessed, true);
        apply_stamp(inode, modified, false);
        inode.dirty = true;
    }
    release(table, cache, io, slot);
    Ok(())
}

/// Apply one timestamp half: flag, skip, or stamp.
fn apply_stamp(inode: &mut crate::inode::Inode, accessed: TimeSpec, is_access: bool) {
    let flag = if is_access {
        crate::inode::UPDATE_ACCESS
    } else {
        crate::inode::UPDATE_MODIFY
    };
    match accessed.nanoseconds {
        nsec if nsec == NSEC_NOW => inode.pending_updates |= flag,
        nsec if nsec == NSEC_OMIT => {}
        _ => {
            if is_access {
                inode.accessed = accessed.seconds;
            } else {
                inode.modified = accessed.seconds;
            }
        }
    }
}

/// File status in framework-neutral fields (`fs_stat`, `stadir.c:43-76`).
///
/// The caller renders these into its own status layout (the system layout
/// belongs to the system-call interface stage). Block count estimates
/// five-hundred-twelve-byte units conservatively, ignoring holes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    /// File mode.
    pub mode: u16,
    /// Link count.
    pub nlinks: u16,
    /// Owner user identifier.
    pub owner: u16,
    /// Owner group identifier.
    pub group: u16,
    /// Device number for specials, zero otherwise.
    pub device: u64,
    /// File size in bytes.
    pub size: i64,
    /// Last access time.
    pub accessed: i64,
    /// Last data change time.
    pub modified: i64,
    /// Last status change time.
    pub changed: i64,
    /// Preferred input-output size (file block size).
    pub block_size: u64,
    /// Blocks used in five-hundred-twelve-byte units (estimated).
    pub blocks: u64,
}

/// Read file status: stamps pending times first, then reports.
#[allow(clippy::too_many_arguments)]
pub fn read_stat<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    device: u64,
    number: u64,
    block_size: u64,
    direct_zones: u32,
    indirect_per_block: u32,
    now: i64,
    read_only: bool,
) -> Result<FileStat, MetaError> {
    let slot = find_open(table, cache, io, device, number)?;
    // Pending times stamp through the shared helper (read-only mounts keep
    // stale flags: stamping would dirty a read-only image).
    table.stamp_times(slot, now, read_only);
    let inode = table.slot(slot);
    let file_type = inode.mode as u32 & TYPE_MASK;
    let special = file_type == TYPE_BLOCK || file_type == TYPE_CHARACTER;
    let stat = FileStat {
        mode: inode.mode,
        nlinks: inode.nlinks,
        owner: inode.owner,
        group: inode.group,
        device: if special { inode.zones[0] } else { 0 },
        size: inode.size,
        accessed: inode.accessed,
        modified: inode.modified,
        changed: inode.changed,
        block_size,
        blocks: estimate_blocks(inode.size, block_size, direct_zones, indirect_per_block),
    };
    release(table, cache, io, slot);
    Ok(stat)
}

/// Estimate five-hundred-twelve-byte blocks for a size (`estimate_blocks`).
///
/// Counts data zones plus single and double indirect blocks from pure
/// arithmetic, ignoring holes (conservative by design: reading indirect
/// blocks for an exact count would cost more than the call is worth,
/// `stadir.c:13-17`).
pub fn estimate_blocks(
    size: i64,
    block_size: u64,
    direct_zones: u32,
    indirect_per_block: u32,
) -> u64 {
    if size <= 0 || block_size == 0 || indirect_per_block == 0 {
        return 0;
    }
    let zone_size = block_size;
    let zones = (size as u64).div_ceil(zone_size);
    let per_block = indirect_per_block as u64;
    // Single indirect blocks beyond the direct run, rounded up; the
    // subtraction-first form goes negative-safe through signed math like
    // the C code (unsigned would wrap below).
    let single =
        (zones as i64 - direct_zones as i64 + per_block as i64 - 1).max(0) as u64 / per_block;
    let square = per_block * per_block;
    let double = if square == 0 {
        0
    } else {
        (single as i64 - 1 + square as i64 - 1).max(0) as u64 / square
    };
    (zones + single + double) * (zone_size / 512)
}

/// File-system statistics in framework-neutral fields (`fs_statvfs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeStat {
    /// Total zones.
    pub blocks: u64,
    /// Free zones.
    pub blocks_free: u64,
    /// Free zones for unprivileged callers (same here: no reserve).
    pub blocks_available: u64,
    /// Zone size in bytes.
    pub block_size: u64,
    /// Fragment size in bytes (block size here).
    pub fragment_size: u64,
    /// Input-output size (fragment size here).
    pub io_size: u64,
    /// Total inodes.
    pub files: u64,
    /// Free inodes (counted live).
    pub files_free: u64,
    /// Free inodes for unprivileged callers (same here).
    pub files_available: u64,
    /// Maximum name length (sixty).
    pub name_max: u64,
}

/// Read volume statistics, counting free inode bits live.
///
/// C: `fs_statvfs` (`stadir.c:82-104`). Free zones derive from the usage
/// counts the mounter maintains; free inodes count live through the
/// bitmap (fresh, not cached). No reserve pool exists, so available equals
/// free on both axes.
pub fn read_volume_stat<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    superblock: &crate::superblock::Superblock,
    zone_total: u64,
    zone_used: u64,
) -> Result<VolumeStat, MetaError> {
    // Free inodes count live through the bitmap (fresh, not cached).
    let free_inodes = count_clear_bits(cache, superblock)?;
    let free_zones = zone_total.saturating_sub(zone_used);
    Ok(VolumeStat {
        blocks: zone_total,
        blocks_free: free_zones,
        blocks_available: free_zones,
        block_size: superblock.block_size as u64,
        fragment_size: superblock.block_size as u64,
        io_size: superblock.block_size as u64,
        files: superblock.inode_count as u64,
        files_free: free_inodes,
        files_available: free_inodes,
        name_max: crate::inode::NAME_CAPACITY as u64,
    })
}

/// Count clear bits across the inode bitmap blocks.
fn count_clear_bits<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    superblock: &crate::superblock::Superblock,
) -> Result<u64, MetaError> {
    let total_bits = superblock.inode_count as u64 + 1;
    let mut free = 0u64;
    let mut bit = 0u64;
    for index in 0..superblock.inode_map_blocks.max(0) as u64 {
        let slot = cache
            .acquire(
                BlockKey::new(superblock.device, crate::superblock::START_BLOCK + index),
                minix_fs::cache::AcquireMode::Normal,
            )
            .map_err(|_| MetaError::Invalid)?;
        let bytes = cache.slot_data(slot).to_vec();
        let _ = cache.release(slot);
        for byte in bytes {
            for position in 0..8u64 {
                if bit >= total_bits {
                    break;
                }
                if byte & (1 << position) == 0 {
                    free += 1;
                }
                bit += 1;
            }
            if bit >= total_bits {
                break;
            }
        }
    }
    Ok(free)
}

/// Swap a sixteen-bit word unless native (`conv2`, `utility.c:10-17`).
pub const fn convert_half(native: bool, word: u16) -> u16 {
    if native { word } else { word.swap_bytes() }
}

/// Swap a thirty-two-bit word unless native (`conv4`, `utility.c:23-36`).
pub const fn convert_word(native: bool, word: u32) -> u32 {
    if native { word } else { word.swap_bytes() }
}

/// Open a slot, loading from disk on a miss (shared prologue).
///
/// Every metadata call opens like the C functions do (`get_inode`), works,
/// then releases. Cold opens read through the cache; the release writes
/// back when dirty.
fn find_open<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    device: u64,
    number: u64,
) -> Result<usize, MetaError> {
    table
        .get(cache, device, number, io)
        .map_err(|_| MetaError::Invalid)
}

/// Release a slot after metadata work.
fn release<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    io: &crate::inode::InodeIo,
    slot: usize,
) {
    let _ = table.put(cache, slot, io);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inode::{InodeIo, InodeTable, TYPE_DIRECTORY, TYPE_REGULAR};
    use crate::superblock::{Bitmap, Superblock};
    use minix_fs::bio::RamDisk;
    use minix_fs::cache::{BlockCache, NoSecondLevel};

    extern crate alloc;

    const DEVICE: u64 = 0x301;
    const BLOCK_SIZE: usize = 512;

    struct Fixture {
        table: InodeTable,
        cache: BlockCache<RamDisk>,
        superblock: Superblock,
        bitmap: Bitmap,
        io: InodeIo,
    }

    fn fixture() -> Fixture {
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
            cache: BlockCache::with_pool(RamDisk::new(64, BLOCK_SIZE).unwrap(), NoSecondLevel, 8)
                .unwrap(),
            bitmap: Bitmap::new(65),
            io: InodeIo::from_superblock(&superblock),
            superblock,
        }
    }

    /// Open a live file slot through the allocator.
    fn open_file(fixture: &mut Fixture, mode: u16) -> usize {
        let slot = fixture
            .table
            .allocate(
                &mut fixture.cache,
                &mut fixture.superblock,
                &mut fixture.bitmap,
                mode,
                100,
                200,
                DEVICE,
            )
            .unwrap();
        fixture.table.slot_mut(slot).nlinks = 1;
        slot
    }

    #[test]
    fn test_chmod_keeps_type_and_reports() {
        let mut fixture = fixture();
        let slot = open_file(&mut fixture, TYPE_REGULAR as u16 | 0o644);
        let number = fixture.table.slot(slot).number;
        // Release the opener reference: metadata calls open by number.
        let io = fixture.io;
        let _ = fixture.table.put(&mut fixture.cache, slot, &io);
        let mode = change_mode(
            &mut fixture.table,
            &mut fixture.cache,
            &fixture.io,
            DEVICE,
            number,
            false,
            0o600,
        )
        .unwrap();
        assert_eq!(mode & 0o7777, 0o600);
        assert_eq!(mode as u32 & crate::inode::TYPE_MASK, 0o100000);
        // Read-only refuses and releases.
        assert_eq!(
            change_mode(
                &mut fixture.table,
                &mut fixture.cache,
                &fixture.io,
                DEVICE,
                number,
                true,
                0o777,
            )
            .unwrap_err(),
            MetaError::ReadOnly
        );
        // Missing inodes report invalid (far past the device: no block).
        assert_eq!(
            change_mode(
                &mut fixture.table,
                &mut fixture.cache,
                &fixture.io,
                DEVICE,
                10000,
                false,
                0o777,
            )
            .unwrap_err(),
            MetaError::Invalid
        );
    }

    #[test]
    fn test_chown_clears_set_id_bits() {
        let mut fixture = fixture();
        let slot = open_file(&mut fixture, TYPE_REGULAR as u16 | 0o4755);
        let number = fixture.table.slot(slot).number;
        let io = fixture.io;
        let _ = fixture.table.put(&mut fixture.cache, slot, &io);
        let mode = change_owner(
            &mut fixture.table,
            &mut fixture.cache,
            &fixture.io,
            DEVICE,
            number,
            7,
            8,
        )
        .unwrap();
        // Set-user-ID cleared, owner and group landed.
        assert_eq!(mode & 0o4000, 0);
        let io = fixture.io;
        let check = fixture
            .table
            .get(&mut fixture.cache, DEVICE, number, &io)
            .unwrap();
        assert_eq!(fixture.table.slot(check).owner, 7);
        assert_eq!(fixture.table.slot(check).group, 8);
    }

    #[test]
    fn test_utime_flags_and_stamps() {
        let mut fixture = fixture();
        let slot = open_file(&mut fixture, TYPE_REGULAR as u16);
        let number = fixture.table.slot(slot).number;
        let io = fixture.io;
        let _ = fixture.table.put(&mut fixture.cache, slot, &io);
        // Explicit stamps land; change time always stamps via pending.
        update_times(
            &mut fixture.table,
            &mut fixture.cache,
            &fixture.io,
            DEVICE,
            number,
            TimeSpec::stamp(5000),
            TimeSpec::omit(),
        )
        .unwrap();
        let io = fixture.io;
        let check = fixture
            .table
            .get(&mut fixture.cache, DEVICE, number, &io)
            .unwrap();
        // Explicit access stamp applied...
        assert_eq!(fixture.table.slot(check).accessed, 5000);
        // ...but modification only flagged (lazy): stamp it through stat.
        assert!(fixture.table.slot(check).pending_updates & crate::inode::UPDATE_MODIFY == 0);
        // "Now" arms flags instead of stamping.
        let _ = fixture.table.put(&mut fixture.cache, check, &io);
        update_times(
            &mut fixture.table,
            &mut fixture.cache,
            &fixture.io,
            DEVICE,
            number,
            TimeSpec::now(),
            TimeSpec::now(),
        )
        .unwrap();
        let io = fixture.io;
        let check = fixture
            .table
            .get(&mut fixture.cache, DEVICE, number, &io)
            .unwrap();
        assert!(fixture.table.slot(check).pending_updates & crate::inode::UPDATE_ACCESS != 0);
    }

    #[test]
    fn test_stat_reports_fields() {
        let mut fixture = fixture();
        let slot = open_file(&mut fixture, TYPE_REGULAR as u16);
        fixture.table.slot_mut(slot).size = 1500;
        fixture.table.slot_mut(slot).zones[0] = 9;
        let number = fixture.table.slot(slot).number;
        let io = fixture.io;
        let _ = fixture.table.put(&mut fixture.cache, slot, &io);
        let stat = read_stat(
            &mut fixture.table,
            &mut fixture.cache,
            &fixture.io,
            DEVICE,
            number,
            BLOCK_SIZE as u64,
            7,
            128,
            7777,
            false,
        )
        .unwrap();
        assert_eq!(stat.mode, TYPE_REGULAR as u16);
        assert_eq!(stat.size, 1500);
        assert_eq!(stat.owner, 100);
        assert_eq!(stat.device, 0);
        assert_eq!(stat.block_size, BLOCK_SIZE as u64);
        // Fifteen hundred bytes need three zones (no indirects yet).
        assert_eq!(stat.blocks, 3 * (BLOCK_SIZE as u64 / 512));
        // Special files report zone zero as the device.
        let slot = open_file(&mut fixture, 0o060000);
        fixture.table.slot_mut(slot).zones[0] = 0x0401;
        fixture.table.slot_mut(slot).nlinks = 1;
        let number = fixture.table.slot(slot).number;
        let io = fixture.io;
        let _ = fixture.table.put(&mut fixture.cache, slot, &io);
        let stat = read_stat(
            &mut fixture.table,
            &mut fixture.cache,
            &fixture.io,
            DEVICE,
            number,
            BLOCK_SIZE as u64,
            7,
            128,
            0,
            false,
        )
        .unwrap();
        assert_eq!(stat.device, 0x0401);
    }

    #[test]
    fn test_estimate_blocks_math() {
        // Zero and negative sizes cost nothing.
        assert_eq!(estimate_blocks(0, 4096, 7, 1024), 0);
        // One zone, no indirects: seven direct cover it.
        assert_eq!(estimate_blocks(4096, 4096, 7, 1024), 8);
        // Eight zones need one single indirect: eight plus one, in units.
        assert_eq!(estimate_blocks(8 * 4096, 4096, 7, 1024), 9 * 8);
    }

    #[test]
    fn test_volume_stat_counts_live() {
        let mut fixture = fixture();
        // The bitmap lives on disk at block two: stage bits zero and one
        // (reserved plus one used) through the cache, then flush.
        {
            use minix_fs::cache::{AcquireMode, BlockKey};
            let slot = fixture
                .cache
                .acquire(BlockKey::new(DEVICE, 2), AcquireMode::NoRead)
                .unwrap();
            fixture.cache.slot_data_mut(slot)[0] = 0b11;
            fixture.cache.mark_dirty(slot);
            fixture.cache.release(slot).unwrap();
            fixture.cache.flush_device(DEVICE).unwrap();
        }
        let stat = read_volume_stat(&mut fixture.cache, &fixture.superblock, 64, 10).unwrap();
        assert_eq!(stat.blocks, 64);
        assert_eq!(stat.blocks_free, 54);
        assert_eq!(stat.blocks_available, 54);
        assert_eq!(stat.block_size, BLOCK_SIZE as u64);
        assert_eq!(stat.files, 64);
        // Sixty-five bits, two set (zero reserved, one used).
        assert_eq!(stat.files_free, 63);
        assert_eq!(stat.name_max, 60);
    }

    #[test]
    fn test_convert_roundtrips() {
        assert_eq!(convert_half(true, 0x1234), 0x1234);
        assert_eq!(convert_half(false, 0x1234), 0x3412);
        assert_eq!(convert_word(true, 0x12345678), 0x12345678);
        assert_eq!(convert_word(false, 0x12345678), 0x78563412);
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EINVAL, EROFS};
        assert_eq!(MetaError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(MetaError::ReadOnly.to_errno().to_i32(), EROFS);
        assert_eq!(NSEC_NOW, (1 << 30) - 1);
        assert_eq!(NSEC_OMIT, (1 << 30) - 2);
        assert_eq!(MODE_MASK, 0o7777);
    }
}
