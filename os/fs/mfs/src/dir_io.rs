//! Directory block load and store: the bridge between the block cache and
//! the image-consuming directory walk.
//!
//! C correspondence: the `lmfs_get_block` reads that feed `search_dir`
//! (`path.c:92-240`) and the `new_block` + `write_map` writes that extend a
//! directory past its current blocks (`path.c:216-238`). The Rust walk
//! ([`crate::dir::search_blocks`)] consumes plain block images on purpose,
//! so the codec and the four search modes stay testable without a cache;
//! this module feeds it real bytes. Loading maps each file block through
//! the inode zones and copies it out of the cache. Storing overwrites every
//! image back — whole blocks, so the no-read acquire applies — and
//! allocates plus maps a zone for any image the walk appended.
//!
//! Two policies for damaged or growing directories:
//! - A hole in the mapping is corruption: both directions report an
//!   input-output error, matching the directory enumeration rule that a
//!   directory should never have holes (`crate::read` treats them the same
//!   way). Nothing aborts; the caller sees the error.
//! - Storing treats an unmapped file block past the old size as an appended
//!   block: the zone is allocated, mapped through
//!   [`crate::write::write_map`], and the image written.

use alloc::vec::Vec;

use minix_types::{EIO, EINVAL, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, NoSecondLevel};

use crate::inode::TOTAL_ZONES;
use crate::mfs_cache::{alloc_zone, mfs_get_block, ZoneSpace};
use crate::read::{map_file_block, MapParams, ZoneRange};
use crate::write::write_map;

/// Load a directory's content from the cache into ordered block images.
///
/// The walk size comes from the caller's `DirScan.size`; every file block
/// below `size.div_ceil(block_size)` is mapped through the inode zones and
/// copied out of the cache. A hole (unmapped block) yields a zeroed image.
/// The images are plain copies: the cache slots are released before the
/// walk sees them, so a long directory cannot pin the whole pool.
pub fn load_dir_blocks<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zones: &[u64; TOTAL_ZONES],
    params: MapParams,
    range: ZoneRange,
    size: u64,
    block_size: usize,
) -> Result<Vec<Vec<u8>>, Errno> {
    let mut blocks = Vec::new();
    if size == 0 {
        return Ok(blocks);
    }
    if block_size == 0 {
        return Err(Errno::from_i32(EINVAL));
    }
    let count = size.div_ceil(block_size as u64);
    for file_block in 0..count {
        let mapped =
            map_file_block(cache, device, zones, params, range, file_block).map_err(|error| error.to_errno())?;
        let mut image = alloc::vec![0u8; block_size];
        if let Some(zone) = mapped {
            let slot = mfs_get_block(cache, BlockKey::new(device, zone), AcquireMode::Normal)?;
            let slot = slot.ok_or_else(|| Errno::from_i32(EIO))?;
            image.copy_from_slice(cache.slot_data(slot));
            cache
                .release(slot)
                .map_err(|_| Errno::from_i32(EIO))?;
        } else {
            // A directory never has holes: an unmapped block inside the size
            // is a corrupt image (`list_dir_entries` reports it the same way).
            return Err(Errno::from_i32(EIO));
        }
        blocks.push(image);
    }
    Ok(blocks)
}

/// Store ordered block images back through the cache and the zone map.
///
/// Every image is exactly one block: the write overwrites the whole cached
/// copy, so the no-read acquire skips a storage read the old contents could
/// not inform. `old_size` is the directory size before the walk ran; an
/// image at a file block it covers must already be mapped, and an image
/// past it counts as appended by the walk: a zone is allocated near the
/// previous block's zone (the search hint when there is none), mapped with
/// [`crate::write::write_map`], and the image written into it. Directories
/// never shrink in MFS, so images that disappeared from the slice are left
/// mapped and dirty — their bytes are still on disk, only unreferenced by
/// the walk.
///
/// The inode metadata (size, zone array, dirty mark, write-back) stays with
/// the caller: this module moves block bytes and zone numbers, and the
/// caller copies `DirScan.size` back into the slot like the C callers do
/// around `search_dir`.
#[allow(clippy::too_many_arguments)]
pub fn store_dir_blocks<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zones: &mut [u64; TOTAL_ZONES],
    params: MapParams,
    range: ZoneRange,
    space: &mut ZoneSpace,
    alloc_bit: &mut dyn FnMut(u64) -> Option<u64>,
    free_bit: &mut dyn FnMut(u64),
    old_size: u64,
    blocks: &[Vec<u8>],
    block_size: usize,
) -> Result<(), Errno> {
    if block_size == 0 {
        return Err(Errno::from_i32(EINVAL));
    }
    let old_count = old_size.div_ceil(block_size as u64);
    let mut previous_zone = 0u64;
    for (file_block, image) in blocks.iter().enumerate() {
        if image.len() != block_size {
            return Err(Errno::from_i32(EINVAL));
        }
        let index = file_block as u64;
        let position = index * block_size as u64;
        let mapped = map_file_block(cache, device, zones, params, range, index)
            .map_err(|error| error.to_errno())?;
        let zone = match mapped {
            Some(zone) => zone,
            None if index >= old_count => {
                let near = if previous_zone != 0 {
                    previous_zone
                } else {
                    space.first_data_zone
                };
                let zone = alloc_zone(space, near, alloc_bit)?;
                write_map(
                    zones,
                    params,
                    space,
                    alloc_bit,
                    free_bit,
                    cache,
                    device,
                    range,
                    position,
                    block_size,
                    zone,
                    false,
                )
                .map_err(|error| error.to_errno())?;
                zone
            }
            None => {
                // An unmapped block inside the old size is a corrupt image,
                // the same rule loading applies.
                return Err(Errno::from_i32(EIO));
            }
        };
        let slot = mfs_get_block(cache, BlockKey::new(device, zone), AcquireMode::NoRead)?;
        let slot = slot.ok_or_else(|| Errno::from_i32(EIO))?;
        cache
            .write_slot(slot, image)
            .map_err(|_| Errno::from_i32(EIO))?;
        cache.mark_dirty(slot);
        cache
            .release(slot)
            .map_err(|_| Errno::from_i32(EIO))?;
        previous_zone = zone;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dir::{DirEntry, ENTRY_NAME_SIZE, ENTRY_SIZE};
    use minix_fs::bio::RamDisk;

    const DEVICE: u64 = 1;
    const BLOCK_SIZE: usize = 64;
    const PARAMS: MapParams = MapParams {
        direct_zones: 7,
        indirect_per_block: 16,
    };
    /// Zones 10..=16 valid; the bit closure hands out 12, 13, ... on demand.
    const RANGE: ZoneRange = ZoneRange {
        first: 10,
        count: 20,
    };

    /// Entry builder: number plus a short name, zero-padded by `to_bytes`.
    fn entry(ino: u32, name: &str) -> [u8; ENTRY_SIZE] {
        let mut slot = DirEntry {
            ino,
            name: [0; ENTRY_NAME_SIZE],
        };
        slot.name[..name.len()].copy_from_slice(name.as_bytes());
        slot.to_bytes()
    }

    fn cache_over(disk: RamDisk) -> BlockCache<RamDisk, NoSecondLevel> {
        BlockCache::with_pool(disk, NoSecondLevel, 8).unwrap()
    }

    fn space() -> ZoneSpace {
        ZoneSpace {
            first_data_zone: 10,
            zone_count: 30,
            zsearch: 10,
        }
    }

    /// Bitmap double: hands out bit numbers 3, 4, ... (zone = first - 1 + bit,
    /// so bit 3 is zone 12) and counts calls.
    struct CountingBits {
        next: u64,
        allocated: u32,
    }

    impl CountingBits {
        fn new() -> Self {
            Self {
                next: 3,
                allocated: 0,
            }
        }

        fn alloc(&mut self, _hint: u64) -> Option<u64> {
            self.allocated += 1;
            let bit = self.next;
            self.next += 1;
            Some(bit)
        }
    }

    fn noop_free(_bit: u64) {}

    #[test]
    fn test_load_roundtrips_mapped_blocks() {
        let mut disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        disk.block_mut(10).unwrap()[..ENTRY_SIZE].copy_from_slice(&entry(2, "a"));
        disk.block_mut(11).unwrap()[..ENTRY_SIZE].copy_from_slice(&entry(3, "bb"));
        let mut cache = cache_over(disk);
        let zones = [10, 11, 0, 0, 0, 0, 0, 0, 0, 0];
        let blocks = load_dir_blocks(
            &mut cache,
            DEVICE,
            &zones,
            PARAMS,
            RANGE,
            2 * BLOCK_SIZE as u64,
            BLOCK_SIZE,
        )
        .unwrap();
        assert_eq!(blocks.len(), 2);
        let parsed = DirEntry::from_bytes(&blocks[0]).unwrap();
        assert_eq!(parsed.name_len(), 1);
        assert_eq!(&parsed.name[..1], b"a");
        let parsed = DirEntry::from_bytes(&blocks[1]).unwrap();
        assert_eq!(&parsed.name[..2], b"bb");
    }

    #[test]
    fn test_load_hole_reports_io_error() {
        let disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        let mut cache = cache_over(disk);
        let zones = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let error = load_dir_blocks(
            &mut cache,
            DEVICE,
            &zones,
            PARAMS,
            RANGE,
            2 * BLOCK_SIZE as u64,
            BLOCK_SIZE,
        )
        .unwrap_err();
        // A directory never has holes; the enumeration path reports the
        // same corruption class.
        assert_eq!(error.to_i32(), EIO);
    }

    #[test]
    fn test_load_empty_directory_is_empty_vec() {
        let disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        let mut cache = cache_over(disk);
        let zones = [0; TOTAL_ZONES];
        let blocks =
            load_dir_blocks(&mut cache, DEVICE, &zones, PARAMS, RANGE, 0, BLOCK_SIZE).unwrap();
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_store_overwrites_mapped_block_without_allocating() {
        let disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        let mut cache = cache_over(disk);
        let mut zones = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut bits = CountingBits::new();
        let image = entry(5, "x").to_vec();
        store_dir_blocks(
            &mut cache,
            DEVICE,
            &mut zones,
            PARAMS,
            RANGE,
            &mut space(),
            &mut |hint| bits.alloc(hint),
            &mut noop_free,
            BLOCK_SIZE as u64,
            &[image],
            BLOCK_SIZE,
        )
        .unwrap();
        assert_eq!(bits.allocated, 0);
        // The bytes must survive a flush: storage itself holds them now.
        cache.flush_device(DEVICE).unwrap();
        let mut stored = [0u8; BLOCK_SIZE];
        cache
            .source()
            .read_block(BlockKey::new(DEVICE, 10), &mut stored)
            .unwrap();
        let mut expected = [0u8; BLOCK_SIZE];
        expected[..ENTRY_SIZE].copy_from_slice(&entry(5, "x"));
        assert_eq!(stored, expected);
    }

    #[test]
    fn test_store_appended_block_allocates_and_maps() {
        let disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        let mut cache = cache_over(disk);
        let mut zones = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut bits = CountingBits::new();
        let first = entry(1, ".").to_vec();
        let second = entry(2, "new").to_vec();
        store_dir_blocks(
            &mut cache,
            DEVICE,
            &mut zones,
            PARAMS,
            RANGE,
            &mut space(),
            &mut |hint| bits.alloc(hint),
            &mut noop_free,
            BLOCK_SIZE as u64,
            &[first, second],
            BLOCK_SIZE,
        )
        .unwrap();
        // One allocation, mapped into direct slot one; locality picks zone 12.
        assert_eq!(bits.allocated, 1);
        assert_eq!(zones[1], 12);
        cache.flush_device(DEVICE).unwrap();
        let mut stored = [0u8; BLOCK_SIZE];
        cache
            .source()
            .read_block(BlockKey::new(DEVICE, 12), &mut stored)
            .unwrap();
        let mut expected = [0u8; BLOCK_SIZE];
        expected[..ENTRY_SIZE].copy_from_slice(&entry(2, "new"));
        assert_eq!(stored, expected);
    }

    #[test]
    fn test_store_rejects_oversized_image() {
        let disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        let mut cache = cache_over(disk);
        let mut zones = [0; TOTAL_ZONES];
        let mut bits = CountingBits::new();
        let mut image = alloc::vec![0u8; BLOCK_SIZE];
        image.push(0);
        let error = store_dir_blocks(
            &mut cache,
            DEVICE,
            &mut zones,
            PARAMS,
            RANGE,
            &mut space(),
            &mut |hint| bits.alloc(hint),
            &mut noop_free,
            0,
            &[image],
            BLOCK_SIZE,
        )
        .unwrap_err();
        assert_eq!(error.to_i32(), EINVAL);
        assert_eq!(bits.allocated, 0);
    }

    #[test]
    fn test_store_hole_inside_old_size_reports_io_error() {
        let disk = RamDisk::new(32, BLOCK_SIZE).unwrap();
        let mut cache = cache_over(disk);
        // Zone slot one is zero while the old size covers two blocks: a hole
        // inside a directory is corruption, not an append.
        let mut zones = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut bits = CountingBits::new();
        let first = entry(1, ".").to_vec();
        let second = entry(2, "b").to_vec();
        let error = store_dir_blocks(
            &mut cache,
            DEVICE,
            &mut zones,
            PARAMS,
            RANGE,
            &mut space(),
            &mut |hint| bits.alloc(hint),
            &mut noop_free,
            2 * BLOCK_SIZE as u64,
            &[first, second],
            BLOCK_SIZE,
        )
        .unwrap_err();
        assert_eq!(error.to_i32(), EIO);
        assert_eq!(bits.allocated, 0);
    }
}
