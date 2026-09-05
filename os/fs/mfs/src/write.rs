//! File write path: zone mapping writes, block ensuring, truncation.
//!
//! C correspondence: `minix3/minix/fs/mfs/write.c` (all three hundred
//! nineteen lines): `write_map`, `wr_indir`, `empty_indir`, `clear_zone`,
//! `new_block`, `zero_block`, plus the write half of `fs_readwrite` in
//! `read.c:48-111`. Writing mirrors reading with one asymmetry: a missing
//! block is created (zone allocated, mapping stored) instead of reading as
//! zeros. Truncation shrinks by freeing and grows by doing nothing (holes
//! read zero through the mapping).
//!
//! Zone allocation policy comes from document 07 (`alloc_zone`); range
//! decisions come from document 13 (`plan_truncate`, `plan_free_range`).
//! This module executes both against the cache.

use alloc::vec::Vec;

use minix_types::{EFBIG, EINVAL, EIO, EROFS, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, NoSecondLevel};

use crate::inode::{InodeTable, TYPE_DIRECTORY, TYPE_MASK, TYPE_REGULAR};
use crate::mfs_cache::{ZoneSpace, alloc_zone, free_zone};
use crate::read::{MapParams, ZoneRange};

/// Free-mode flag: free instead of storing (`WMAP_FREE`, `const.h:40`).
pub const WRITE_FREE: u32 = 1;

/// Why a write failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    /// Bad request (unknown inode, corrupt image, bad argument).
    Invalid,
    /// Growth past the maximum (`EFBIG`).
    TooBig,
    /// Allocation exhausted (precise code preserved).
    NoSpace(Errno),
    /// Write on a read-only mount (`EROFS`).
    ReadOnly,
    /// Storage failure along the way.
    Io,
}

impl WriteError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::TooBig => Errno::from_i32(EFBIG),
            Self::NoSpace(error) => error,
            Self::ReadOnly => Errno::from_i32(EROFS),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// Store or free one zone mapping (`write_map`, `write.c:28-185`).
///
/// Position selects the zone slot (direct, single, or double indirect,
/// like the read mapping). Storing writes the zone number, allocating
/// indirect blocks on demand (zeroed, through the cache). Freeing clears a
/// data zone and then drops indirect blocks left empty, including the
/// double block when its last single goes. Double-indirect indexes past the
/// table refuse with file-too-big (`write.c:95`). The inode stays dirty
/// throughout (the C code marks it on entry); callers write it back.
#[allow(clippy::too_many_arguments)]
pub fn write_map<S: BlockSource>(
    zones: &mut [u64; crate::inode::TOTAL_ZONES],
    params: MapParams,
    space: &mut ZoneSpace,
    alloc_bit: &mut dyn FnMut(u64) -> Option<u64>,
    free_bit: &mut dyn FnMut(u64),
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    range: ZoneRange,
    position: u64,
    block_size: usize,
    new_zone: u64,
    free_mode: bool,
) -> Result<(), WriteError> {
    if block_size == 0 || params.indirect_per_block == 0 {
        return Err(WriteError::Invalid);
    }
    let direct = params.direct_zones as u64;
    let per_block = params.indirect_per_block as u64;
    let zone = position / block_size as u64;
    if zone < direct {
        let index = zone as usize;
        if free_mode {
            if zones[index] != 0 {
                free_zone(cache, device, space, zones[index], free_bit);
                zones[index] = 0;
            }
        } else {
            zones[index] = new_zone;
        }
        return Ok(());
    }
    let mut excess = zone - direct;
    // Double-indirect leg first so the single zone below is known. The
    // first zone seeds indirect allocation like the C code.
    let first_zone_hint = zones[0];
    let mut double_zone = 0u64;
    let mut double_index = 0u64;
    let mut via_double = false;
    let mut single_zone;
    if excess < per_block {
        single_zone = zones[direct as usize];
    } else {
        excess -= per_block;
        let index = excess / per_block;
        if index >= per_block {
            return Err(WriteError::TooBig);
        }
        double_zone = zones[direct as usize + 1];
        if double_zone == 0 && !free_mode {
            double_zone = alloc_zone(space, first_zone_hint, alloc_bit)
                .map_err(|_| WriteError::NoSpace(Errno::from_i32(minix_types::ENOSPC)))?;
            zones[direct as usize + 1] = double_zone;
            let slot = cache
                .acquire(BlockKey::new(device, double_zone), AcquireMode::NoRead)
                .map_err(|_| WriteError::Io)?;
            zero_slot(cache, slot);
            let _ = cache.release(slot);
        }
        if double_zone == 0 {
            // Freeing without a double block: no single block either.
            return Ok(());
        }
        double_index = index;
        let entries = read_indirect_entries(cache, device, double_zone, range)?;
        single_zone = entries.get(index as usize).copied().unwrap_or(0);
        excess %= per_block;
        via_double = true;
    }
    if single_zone == 0 && !free_mode {
        single_zone = alloc_zone(space, first_zone_hint, alloc_bit)
            .map_err(|_| WriteError::NoSpace(Errno::from_i32(minix_types::ENOSPC)))?;
        if via_double {
            write_indirect_entry(cache, device, double_zone, double_index, single_zone)?;
        } else {
            zones[direct as usize] = single_zone;
        }
        let slot = cache
            .acquire(BlockKey::new(device, single_zone), AcquireMode::NoRead)
            .map_err(|_| WriteError::Io)?;
        zero_slot(cache, slot);
        let _ = cache.release(slot);
    }
    if single_zone == 0 {
        // Freeing without a single block: nothing below exists.
        return Ok(());
    }
    if free_mode {
        // Free the data zone, then drop the single block when emptied.
        let mut entries = read_indirect_entries(cache, device, single_zone, range)?;
        let old = entries.get(excess as usize).copied().unwrap_or(0);
        if old != 0 {
            free_zone(cache, device, space, old, free_bit);
            write_indirect_entry(cache, device, single_zone, excess, 0)?;
            entries[excess as usize] = 0;
        }
        if entries.iter().all(|&entry| entry == 0) {
            free_zone(cache, device, space, single_zone, free_bit);
            if double_zone != 0 {
                write_indirect_entry(cache, device, double_zone, double_index, 0)?;
                // Drop the double block when its last single goes: re-read
                // after the clearing above (never trust a stale snapshot).
                let entries = read_indirect_entries(cache, device, double_zone, range)?;
                if indirect_is_empty(&entries) {
                    free_zone(cache, device, space, double_zone, free_bit);
                    zones[direct as usize + 1] = 0;
                }
            } else {
                zones[direct as usize] = 0;
            }
        }
    } else {
        write_indirect_entry(cache, device, single_zone, excess, new_zone)?;
    }
    Ok(())
}

/// Read one indirect block as validated zone numbers.
fn read_indirect_entries<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zone: u64,
    range: ZoneRange,
) -> Result<Vec<u64>, WriteError> {
    let slot = cache
        .acquire(BlockKey::new(device, zone), AcquireMode::Normal)
        .map_err(|_| WriteError::Io)?;
    let bytes = cache.slot_data(slot).to_vec();
    let _ = cache.release(slot);
    if !bytes.len().is_multiple_of(4) {
        return Err(WriteError::Io);
    }
    let mut entries = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        entries.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as u64);
    }
    // Same range rule as the read walk: forged pointers are corruption.
    for entry in &entries {
        if *entry != 0 && (*entry < range.first || *entry >= range.count) {
            return Err(WriteError::Io);
        }
    }
    Ok(entries)
}

/// Write one indirect entry (`wr_indir`, `write.c:191-208`).
///
/// Bounds-checked: the C code aborts on a null block, which cannot happen
/// here (blocks arrive validated), and indexes stay inside by construction.
fn write_indirect_entry<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zone: u64,
    index: u64,
    value: u64,
) -> Result<(), WriteError> {
    let slot = cache
        .acquire(BlockKey::new(device, zone), AcquireMode::Normal)
        .map_err(|_| WriteError::Io)?;
    {
        let data = cache.slot_data_mut(slot);
        let at = index as usize * 4;
        if at + 4 > data.len() {
            let _ = cache.release(slot);
            return Err(WriteError::Io);
        }
        data[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
    }
    cache.mark_dirty(slot);
    let _ = cache.release(slot);
    Ok(())
}

/// Whether an indirect image holds only absent entries (`empty_indir`).
pub fn indirect_is_empty(entries: &[u64]) -> bool {
    entries.iter().all(|&entry| entry == 0)
}

/// Zero a whole cache slot and mark it dirty (`zero_block`).
pub fn zero_slot<S: BlockSource>(cache: &mut BlockCache<S, NoSecondLevel>, slot: usize) {
    cache.slot_data_mut(slot).fill(0);
    cache.mark_dirty(slot);
}

/// Ensure the block for a file position, allocating on a miss (`new_block`,
/// `write.c:254-305`).
///
/// A mapped position acquires its block normally. Otherwise the zone hint
/// (the file's own hint, else its first zone, else the filesystem's first
/// data zone) seeds allocation; allocation failure reports no space;
/// mapping failure frees the zone again and the stored file hint updates
/// for the next lookup. A fresh block arrives zeroed without a storage
/// read. Returns the cache slot index, still held: the caller releases it
/// after use.
#[allow(clippy::too_many_arguments)]
pub fn ensure_block<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    space: &mut ZoneSpace,
    alloc_bit: &mut dyn FnMut(u64) -> Option<u64>,
    free_bit: &mut dyn FnMut(u64),
    device: u64,
    slot: usize,
    params: MapParams,
    range: ZoneRange,
    block_size: usize,
    position: u64,
) -> Result<usize, WriteError> {
    let zones = table.slot(slot).zones;
    let file_block = position / block_size as u64;
    if let Some(device_block) =
        crate::read::map_file_block(cache, device, &zones, params, range, file_block)
            .map_err(|_| WriteError::Io)?
    {
        return cache
            .acquire(BlockKey::new(device, device_block), AcquireMode::Normal)
            .map_err(|_| WriteError::Io);
    }
    // Miss: seed the hint like the C code, allocate, and map it in.
    let hint = {
        let inode = table.slot(slot);
        if inode.zone_hint != 0 {
            inode.zone_hint
        } else if inode.zones[0] != 0 {
            inode.zones[0]
        } else {
            space.first_data_zone
        }
    };
    let zone = alloc_zone(space, hint, alloc_bit)
        .map_err(|_| WriteError::NoSpace(Errno::from_i32(minix_types::ENOSPC)))?;
    // Re-read zones after the (immutable) hint lookup above.
    let mut zones = table.slot(slot).zones;
    let outcome = write_map(
        &mut zones, params, space, alloc_bit, free_bit, cache, device, range, position, block_size,
        zone, false,
    );
    if let Err(error) = outcome {
        free_zone(cache, device, space, zone, free_bit);
        return Err(error);
    }
    // Publish the mapping and remember the hint for next time.
    {
        let inode = table.slot_mut(slot);
        inode.zones = zones;
        inode.zone_hint = zone;
        inode.dirty = true;
    }
    let cache_slot = cache
        .acquire(BlockKey::new(device, zone), AcquireMode::NoRead)
        .map_err(|_| WriteError::Io)?;
    zero_slot(cache, cache_slot);
    Ok(cache_slot)
}

/// Write bytes to a file (`fs_readwrite` write half, `read.c:48-111`).
///
/// Refuses read-only mounts and oversized growth up front. Each block-sized
/// chunk maps or allocates through [`ensure_block`]; blocks that already
/// exist at or past the old end are zeroed first when the write starts
/// block-aligned (otherwise stale tails would survive past the new end of
/// file, `read.c:187-190`). Regular files and directories grow; the seek
/// flag clears; change and modification timestamps stamp on writable
/// mounts. Reports bytes moved.
#[allow(clippy::too_many_arguments)]
pub fn write_file<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    space: &mut ZoneSpace,
    alloc_bit: &mut dyn FnMut(u64) -> Option<u64>,
    free_bit: &mut dyn FnMut(u64),
    device: u64,
    slot: usize,
    params: MapParams,
    range: ZoneRange,
    block_size: usize,
    max_size: u64,
    read_only: bool,
    position: i64,
    data: &[u8],
) -> Result<usize, WriteError> {
    if position < 0 || data.len() > isize::MAX as usize {
        return Err(WriteError::Invalid);
    }
    if read_only {
        return Err(WriteError::ReadOnly);
    }
    if block_size == 0 {
        return Err(WriteError::Invalid);
    }
    // Growth past the maximum refuses up front, in signed arithmetic like
    // the C check (`read.c:53`): lengths past the maximum fail even from
    // position zero, which saturating subtraction would miss.
    if data.len() as u64 > max_size || (position as u64) > max_size - data.len() as u64 {
        return Err(WriteError::TooBig);
    }
    let old_size = table.slot(slot).size as u64;
    let mode = table.slot(slot).mode;
    let mut position = position as u64;
    let mut remaining = data.len();
    let mut moved = 0usize;
    while remaining > 0 {
        let offset = (position % block_size as u64) as usize;
        let chunk = (block_size - offset).min(remaining);
        let cache_slot = ensure_block(
            table, cache, space, alloc_bit, free_bit, device, slot, params, range, block_size,
            position,
        )?;
        // Existing block at or past the old end, block-aligned start,
        // partial chunk: clear first so no stale tail survives.
        if position >= old_size && offset == 0 && chunk != block_size {
            let data = cache.slot_data_mut(cache_slot);
            data.fill(0);
        }
        {
            let bytes = cache.slot_data_mut(cache_slot);
            bytes[offset..offset + chunk].copy_from_slice(&data[moved..moved + chunk]);
        }
        cache.mark_dirty(cache_slot);
        let _ = cache.release(cache_slot);
        moved += chunk;
        remaining -= chunk;
        position += chunk as u64;
        if chunk == 0 {
            break;
        }
    }
    {
        let inode = table.slot_mut(slot);
        let growable =
            mode as u32 & TYPE_MASK == TYPE_REGULAR || mode as u32 & TYPE_MASK == TYPE_DIRECTORY;
        if growable && position > old_size {
            inode.size = position as i64;
        }
        inode.seek = false;
        if !read_only {
            inode.pending_updates |= crate::inode::UPDATE_CHANGE | crate::inode::UPDATE_MODIFY;
            inode.dirty = true;
        }
    }
    Ok(moved)
}

/// Truncate a file to a new size (`truncate_inode`, `link.c:452-488`,
/// executing the plan from document 13).
///
/// Special files refuse; growth past the maximum refuses; shrinking frees
/// whole zones and zeroes partial edges through the cache; growth only
/// moves the size (holes read zero through the mapping). Timestamps stamp
/// and the slot dirties on success.
#[allow(clippy::too_many_arguments)]
pub fn truncate_file<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    space: &mut ZoneSpace,
    alloc_bit: &mut dyn FnMut(u64) -> Option<u64>,
    free_bit: &mut dyn FnMut(u64),
    device: u64,
    slot: usize,
    params: MapParams,
    range: ZoneRange,
    block_size: usize,
    max_size: u64,
    new_size: u64,
) -> Result<(), WriteError> {
    use crate::link::{plan_free_range, plan_truncate};
    let (mode, size) = {
        let inode = table.slot(slot);
        (inode.mode, inode.size as u64)
    };
    let is_block = mode as u32 & TYPE_MASK == crate::inode::TYPE_BLOCK;
    let is_char = mode as u32 & TYPE_MASK == crate::inode::TYPE_CHARACTER;
    match plan_truncate(mode, size, max_size, new_size, is_block, is_char).map_err(|error| {
        match error {
            crate::link::LinkError::TooBig => WriteError::TooBig,
            _ => WriteError::Invalid,
        }
    })? {
        crate::link::TruncatePlan::Same => Ok(()),
        crate::link::TruncatePlan::Grow { new_size } => {
            let inode = table.slot_mut(slot);
            inode.size = new_size as i64;
            inode.pending_updates |= crate::inode::UPDATE_CHANGE | crate::inode::UPDATE_MODIFY;
            inode.dirty = true;
            Ok(())
        }
        crate::link::TruncatePlan::Shrink { new_size } => {
            let plan = plan_free_range(size, block_size as u64, new_size, size)
                .map_err(|_| WriteError::Invalid)?;
            // Zero partial edges through mapped blocks.
            for (start, length) in &plan.zero_ranges {
                let mut remaining = *length;
                let mut position = *start;
                while remaining > 0 {
                    let offset = (position % block_size as u64) as usize;
                    let chunk = (block_size - offset).min(remaining as usize);
                    let zones = table.slot(slot).zones;
                    let file_block = position / block_size as u64;
                    let mapped = crate::read::map_file_block(
                        cache, device, &zones, params, range, file_block,
                    )
                    .map_err(|_| WriteError::Io)?;
                    if let Some(device_block) = mapped {
                        let cache_slot = cache
                            .acquire(BlockKey::new(device, device_block), AcquireMode::Normal)
                            .map_err(|_| WriteError::Io)?;
                        {
                            let bytes = cache.slot_data_mut(cache_slot);
                            bytes[offset..offset + chunk].fill(0);
                        }
                        cache.mark_dirty(cache_slot);
                        let _ = cache.release(cache_slot);
                    }
                    remaining -= chunk as u64;
                    position += chunk as u64;
                }
            }
            // Free whole zones by position through the mapping writer.
            for zone in &plan.free_zones {
                let mut zones = table.slot(slot).zones;
                write_map(
                    &mut zones,
                    params,
                    space,
                    alloc_bit,
                    free_bit,
                    cache,
                    device,
                    range,
                    zone * block_size as u64,
                    block_size,
                    0,
                    true,
                )?;
                table.slot_mut(slot).zones = zones;
            }
            {
                let inode = table.slot_mut(slot);
                inode.size = new_size as i64;
                inode.pending_updates |= crate::inode::UPDATE_CHANGE | crate::inode::UPDATE_MODIFY;
                inode.dirty = true;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inode::{InodeTable, TYPE_REGULAR};
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
        space: ZoneSpace,
        bitmap: Bitmap,
        superblock: Superblock,
        params: MapParams,
        range: ZoneRange,
        freed: Vec<u64>,
    }

    fn fixture() -> Fixture {
        let superblock = Superblock {
            inode_count: 64,
            inode_map_blocks: 1,
            zone_map_blocks: 1,
            flags: 1,
            max_size: 1_000_000,
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
            space: ZoneSpace {
                first_data_zone: 4,
                zone_count: 64,
                zsearch: 0,
            },
            bitmap: Bitmap::new(65),
            superblock,
            params: MapParams {
                direct_zones: 7,
                indirect_per_block: 128,
            },
            range: ZoneRange {
                first: 4,
                count: 64,
            },
            freed: Vec::new(),
        }
    }

    fn open_file(fixture: &mut Fixture, zones: [u64; 10], size: i64) -> usize {
        let slot = fixture
            .table
            .allocate(
                &mut fixture.cache,
                &mut fixture.superblock,
                &mut fixture.bitmap,
                TYPE_REGULAR as u16,
                0,
                0,
                DEVICE,
            )
            .unwrap();
        {
            let inode = fixture.table.slot_mut(slot);
            inode.zones = zones;
            inode.size = size;
            inode.nlinks = 1;
        }
        slot
    }

    #[test]
    fn test_write_map_direct_store_and_free() {
        let mut fixture = fixture();
        let mut zones = [0u64; 10];
        let mut freed = Vec::new();
        // Store into direct slot two.
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        write_map(
            &mut zones,
            fixture.params,
            &mut fixture.space,
            &mut alloc,
            &mut |zone| freed.push(zone),
            &mut fixture.cache,
            DEVICE,
            fixture.range,
            2 * BLOCK_SIZE as u64,
            BLOCK_SIZE,
            44,
            false,
        )
        .unwrap();
        assert_eq!(zones[2], 44);
        // Free it back.
        write_map(
            &mut zones,
            fixture.params,
            &mut fixture.space,
            &mut alloc,
            &mut |zone| freed.push(zone),
            &mut fixture.cache,
            DEVICE,
            fixture.range,
            2 * BLOCK_SIZE as u64,
            BLOCK_SIZE,
            0,
            true,
        )
        .unwrap();
        assert_eq!(zones[2], 0);
        // Bit number, not zone: the release converts zone forty-four.
        assert_eq!(freed, alloc::vec![41]);
    }

    #[test]
    fn test_write_map_single_indirect_create_and_cascade() {
        let mut fixture = fixture();
        let mut zones = [0u64; 10];
        let position = 7 * BLOCK_SIZE as u64;
        // Store through the single-indirect leg: allocates a single block
        // plus records the entry.
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        write_map(
            &mut zones,
            fixture.params,
            &mut fixture.space,
            &mut alloc,
            &mut |_| {},
            &mut fixture.cache,
            DEVICE,
            fixture.range,
            position,
            BLOCK_SIZE,
            50,
            false,
        )
        .unwrap();
        let single = zones[7];
        assert!(single >= 4);
        // The entry landed in the single block.
        let entries =
            read_indirect_entries(&mut fixture.cache, DEVICE, single, fixture.range).unwrap();
        assert_eq!(entries[0], 50);
        // Freeing the only entry cascades: data, then the single block.
        let mut freed = Vec::new();
        write_map(
            &mut zones,
            fixture.params,
            &mut fixture.space,
            &mut alloc,
            &mut |zone| freed.push(zone),
            &mut fixture.cache,
            DEVICE,
            fixture.range,
            position,
            BLOCK_SIZE,
            0,
            true,
        )
        .unwrap();
        assert_eq!(zones[7], 0);
        // Bitmap bits: data zone fifty and the single block itself.
        assert!(freed.contains(&47));
        assert!(freed.contains(&(single - 3)));
    }

    #[test]
    fn test_write_map_double_indirect_cap() {
        let mut fixture = fixture();
        let mut zones = [0u64; 10];
        // Past double-indirect capacity (7 + 128 + 128*128 zones): refuse.
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        let far = (7 + 128 + 128 * 128) * BLOCK_SIZE as u64;
        assert_eq!(
            write_map(
                &mut zones,
                fixture.params,
                &mut fixture.space,
                &mut alloc,
                &mut |_| {},
                &mut fixture.cache,
                DEVICE,
                fixture.range,
                far,
                BLOCK_SIZE,
                60,
                false,
            )
            .unwrap_err(),
            WriteError::TooBig
        );
    }

    #[test]
    fn test_ensure_block_allocates_and_zeroes() {
        let mut fixture = fixture();
        let slot = open_file(&mut fixture, [0u64; 10], 0);
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        let cache_slot = ensure_block(
            &mut fixture.table,
            &mut fixture.cache,
            &mut fixture.space,
            &mut alloc,
            &mut |_| {},
            DEVICE,
            slot,
            fixture.params,
            fixture.range,
            BLOCK_SIZE,
            0,
        )
        .unwrap();
        // Fresh block reads zero.
        assert!(
            fixture
                .cache
                .slot_data(cache_slot)
                .iter()
                .all(|&byte| byte == 0)
        );
        fixture.cache.release(cache_slot).unwrap();
        // Mapping recorded and hint stored.
        assert!(fixture.table.slot(slot).zones[0] >= 4);
        assert!(fixture.table.slot(slot).zone_hint >= 4);
        // Second ensure hits the mapping (no second allocation).
        let before = fixture.superblock.zsearch;
        let cache_slot = ensure_block(
            &mut fixture.table,
            &mut fixture.cache,
            &mut fixture.space,
            &mut alloc,
            &mut |_| {},
            DEVICE,
            slot,
            fixture.params,
            fixture.range,
            BLOCK_SIZE,
            0,
        )
        .unwrap();
        fixture.cache.release(cache_slot).unwrap();
        assert_eq!(fixture.superblock.zsearch, before);
    }

    #[test]
    fn test_write_file_grows_with_holes() {
        let mut fixture = fixture();
        let slot = open_file(&mut fixture, [0u64; 10], 0);
        let number = fixture.table.slot(slot).number;
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        // Write past the end: bytes 1000..1010 in a 512-byte world straddle
        // blocks one and two, leaving a hole from zero.
        let data = alloc::vec![9u8; 10];
        let moved = write_file(
            &mut fixture.table,
            &mut fixture.cache,
            &mut fixture.space,
            &mut alloc,
            &mut |_| {},
            DEVICE,
            slot,
            fixture.params,
            fixture.range,
            BLOCK_SIZE,
            1_000_000,
            false,
            1000,
            &data,
        )
        .unwrap();
        assert_eq!(moved, 10);
        assert_eq!(fixture.table.slot(slot).size, 1010);
        // Read back through the read path: hole zeros, data intact.
        let read_params = crate::read::FileParams {
            map: fixture.params,
            range: fixture.range,
            block_size: BLOCK_SIZE,
            read_only: false,
        };
        let mut seen = Vec::new();
        let moved = crate::read::read_file(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            number,
            0,
            1010,
            read_params,
            &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
        )
        .unwrap();
        assert_eq!(moved, 1010);
        assert!(seen[..1000].iter().all(|&byte| byte == 0));
        assert!(seen[1000..].iter().all(|&byte| byte == 9));
        // Oversized growth refuses; read-only refuses.
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        assert_eq!(
            write_file(
                &mut fixture.table,
                &mut fixture.cache,
                &mut fixture.space,
                &mut alloc,
                &mut |_| {},
                DEVICE,
                slot,
                fixture.params,
                fixture.range,
                BLOCK_SIZE,
                100,
                false,
                0,
                &[0u8; 200],
            )
            .unwrap_err(),
            WriteError::TooBig
        );
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        assert_eq!(
            write_file(
                &mut fixture.table,
                &mut fixture.cache,
                &mut fixture.space,
                &mut alloc,
                &mut |_| {},
                DEVICE,
                slot,
                fixture.params,
                fixture.range,
                BLOCK_SIZE,
                1_000_000,
                true,
                0,
                &[0u8; 1],
            )
            .unwrap_err(),
            WriteError::ReadOnly
        );
    }

    #[test]
    fn test_truncate_shrink_and_grow() {
        let mut fixture = fixture();
        let mut zones = [0u64; 10];
        zones[0] = 10;
        zones[1] = 11;
        let slot = open_file(&mut fixture, zones, 1024);
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        let mut freed = Vec::new();
        truncate_file(
            &mut fixture.table,
            &mut fixture.cache,
            &mut fixture.space,
            &mut alloc,
            &mut |zone| freed.push(zone),
            DEVICE,
            slot,
            fixture.params,
            fixture.range,
            BLOCK_SIZE,
            1_000_000,
            100,
        )
        .unwrap();
        assert_eq!(fixture.table.slot(slot).size, 100);
        assert_eq!(fixture.table.slot(slot).zones[1], 0);
        // Bitmap bit for zone eleven.
        assert_eq!(freed, alloc::vec![8]);
        assert!(fixture.table.slot(slot).dirty);
        // Grow only moves the size.
        truncate_file(
            &mut fixture.table,
            &mut fixture.cache,
            &mut fixture.space,
            &mut alloc,
            &mut |zone| freed.push(zone),
            DEVICE,
            slot,
            fixture.params,
            fixture.range,
            BLOCK_SIZE,
            1_000_000,
            900,
        )
        .unwrap();
        assert_eq!(fixture.table.slot(slot).size, 900);
        // Special files refuse; oversized refuses.
        fixture.table.slot_mut(slot).mode = 0o020000;
        let mut alloc = |hint: u64| fixture.bitmap.alloc(hint);
        assert_eq!(
            truncate_file(
                &mut fixture.table,
                &mut fixture.cache,
                &mut fixture.space,
                &mut alloc,
                &mut |zone| freed.push(zone),
                DEVICE,
                slot,
                fixture.params,
                fixture.range,
                BLOCK_SIZE,
                1_000_000,
                50,
            )
            .unwrap_err(),
            WriteError::Invalid
        );
    }

    #[test]
    fn test_indirect_helpers() {
        assert!(indirect_is_empty(&[0u64; 4]));
        assert!(!indirect_is_empty(&[0, 0, 5, 0]));
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EFBIG, EINVAL, EIO, ENOSPC, EROFS};
        assert_eq!(WriteError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(WriteError::TooBig.to_errno().to_i32(), EFBIG);
        assert_eq!(
            WriteError::NoSpace(Errno::from_i32(ENOSPC))
                .to_errno()
                .to_i32(),
            ENOSPC
        );
        assert_eq!(WriteError::ReadOnly.to_errno().to_i32(), EROFS);
        assert_eq!(WriteError::Io.to_errno().to_i32(), EIO);
    }
}
