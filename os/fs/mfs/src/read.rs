//! File read path: block mapping, chunked reads, readahead, and directory
//! listing.
//!
//! C correspondence: `minix3/minix/fs/mfs/read.c` (all five hundred
//! fifty-six lines): `fs_readwrite` (read half), `rw_chunk`, `read_map`,
//! `get_block_map`, `rd_indir`, `rahead`, `fs_getdents`. Position mapping
//! turns a file offset into a device block through direct and double
//! indirect zones; holes read as zeros; reads never extend the file; the
//! seek flag clears on every transfer.
//!
//! Metadata reads (indirect blocks) always go through the cache normally:
//! the C opportunistic-peek mode only serves queue building for scattered
//! prefetch, which the block layer already covers (document 05), so there
//! is nothing to save by peeking here.

use alloc::vec::Vec;

use minix_types::{EINVAL, EIO, ENOENT, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, NoSecondLevel};
use minix_fs::data::{DataChannel, MemoryBackend};
use minix_fs::dentry::{DentryEncoder, DirentType};

use crate::inode::{InodeTable, TYPE_MASK};

/// Minimum prefetch on sequential reads (`BLOCKS_MINIMUM`, `read.c:344`,
/// thirty-two), skipped after a seek.
pub const PREFETCH_MINIMUM: usize = 32;

/// Block-mapping geometry: direct count plus indirect density.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapParams {
    /// Direct zones in the inode (seven).
    pub direct_zones: u32,
    /// Zones per indirect block (block size over four).
    pub indirect_per_block: u32,
}

/// Valid zone range for indirect entries (first data zone to zone count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneRange {
    /// First data zone number.
    pub first: u64,
    /// Zone count (exclusive upper bound).
    pub count: u64,
}

/// Why mapping or reading failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// Bad request (unknown inode, corrupt image, bad argument).
    Invalid,
    /// Storage failure along the way.
    Io,
    /// Directory position misaligned (`ENOENT` in the C code: a misaligned
    /// resume position means the caller lost track, reported as missing).
    Unaligned,
}

impl ReadError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::Io => Errno::from_i32(EIO),
            Self::Unaligned => Errno::from_i32(ENOENT),
        }
    }
}

/// Map a file-relative block number to a device block number.
///
/// C: `read_map` (`read.c:210-279`) with opportunistic mode off. Direct
/// zones answer immediately; deeper positions walk single then double
/// indirect blocks through the cache. Absent zones report empty (holes);
/// out-of-range double-indirect indexes report empty; illegal zone numbers
/// in indirect blocks report an input-output error (the C code aborts the
/// server here: a forged pointer on disk).
pub fn map_file_block<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zones: &[u64; crate::inode::TOTAL_ZONES],
    params: MapParams,
    range: ZoneRange,
    file_block: u64,
) -> Result<Option<u64>, ReadError> {
    let direct = params.direct_zones as u64;
    let per_block = params.indirect_per_block as u64;
    if per_block == 0 {
        return Err(ReadError::Invalid);
    }
    if file_block < direct {
        let zone = zones[file_block as usize];
        return Ok(if zone == 0 { None } else { Some(zone) });
    }
    let mut excess = file_block - direct;
    // Single indirect?
    let single = if excess < per_block {
        zones[direct as usize]
    } else {
        // Double indirect: index into it, then into the single it names.
        let dbl = zones[direct as usize + 1];
        if dbl == 0 {
            return Ok(None);
        }
        excess -= per_block;
        let index = excess / per_block;
        if index >= per_block {
            return Ok(None);
        }
        let dbl_block = read_indirect(cache, device, dbl, range)?;
        let single_zone = dbl_block[index as usize];
        excess %= per_block;
        if single_zone == 0 {
            return Ok(None);
        }
        // Re-read the single block named by the double entry.
        let single_block = read_indirect(cache, device, single_zone as u64, range)?;
        return entry_to_block(&single_block, excess, range);
    };
    if single == 0 {
        return Ok(None);
    }
    let single_block = read_indirect(cache, device, single, range)?;
    entry_to_block(&single_block, excess, range)
}

/// Read one indirect block as zone numbers, validated.
fn read_indirect<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zone: u64,
    range: ZoneRange,
) -> Result<Vec<u32>, ReadError> {
    let slot = cache
        .acquire(BlockKey::new(device, zone), AcquireMode::Normal)
        .map_err(|_| ReadError::Io)?;
    let bytes = cache.slot_data(slot).to_vec();
    let _ = cache.release(slot);
    if !bytes.len().is_multiple_of(4) {
        return Err(ReadError::Io);
    }
    let mut zones = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        zones.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    // Validate like `rd_indir`: forged pointers are corruption, not holes.
    for zone in &zones {
        if *zone != 0 && ((*zone as u64) < range.first || (*zone as u64) >= range.count) {
            return Err(ReadError::Io);
        }
    }
    Ok(zones)
}

/// Resolve one indirect entry to a device block number.
fn entry_to_block(
    entries: &[u32],
    index: u64,
    _range: ZoneRange,
) -> Result<Option<u64>, ReadError> {
    let zone = entries.get(index as usize).copied().unwrap_or(0);
    Ok(if zone == 0 { None } else { Some(zone as u64) })
}

/// Read bytes from a file (`fs_readwrite` read half, `read.c:23-111`).
///
/// Finds the slot without opening (reads do not take references in the C
/// code either: `find_inode`), clamps to the file size, walks block-aligned
/// chunks, fills holes with zeros, clears the seek flag, stamps access time
/// on writable mounts, and reports the byte count. Reading never changes
/// the size and never fails past end of file: it stops.
#[allow(clippy::too_many_arguments)]
pub fn read_file<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    number: u64,
    position: i64,
    length: usize,
    params: FileParams,
    out: &mut dyn FnMut(&[u8]),
) -> Result<usize, ReadError> {
    if position < 0 || length > isize::MAX as usize {
        return Err(ReadError::Invalid);
    }
    let slot = table.find(device, number).ok_or(ReadError::Invalid)?;
    let (size, block_size) = {
        let inode = table.slot(slot);
        (inode.size, params.block_size)
    };
    if block_size == 0 {
        return Err(ReadError::Invalid);
    }
    let mut position = position as u64;
    let mut remaining = length;
    // Clamp to end of file like the chunk loop does (`read.c:72-74`).
    if position >= size as u64 {
        table.slot_mut(slot).seek = false;
        return Ok(0);
    }
    remaining = remaining.min((size as u64 - position) as usize);
    let mut moved = 0usize;
    let mut first_block = true;
    while remaining > 0 {
        let offset = (position % block_size as u64) as usize;
        let mut chunk = (block_size - offset).min(remaining);
        let file_block = position / block_size as u64;
        let mapped = map_position(table, cache, device, slot, &params, file_block)?;
        if let Some(device_block) = mapped {
            if first_block {
                // Warm what follows (see below); only once per call.
                readahead_file(table, cache, device, slot, &params, file_block);
                first_block = false;
            }
            let cache_slot = cache
                .acquire(BlockKey::new(device, device_block), AcquireMode::Normal)
                .map_err(|_| ReadError::Io)?;
            let bytes = cache.slot_data(cache_slot).to_vec();
            let _ = cache.release(cache_slot);
            chunk = chunk.min(bytes.len().saturating_sub(offset));
            out(&bytes[offset..offset + chunk]);
        } else {
            // Holes read as zeros (`rw_chunk`, `read.c:148-155`).
            out(&alloc::vec![0u8; chunk]);
        }
        moved += chunk;
        remaining -= chunk;
        position += chunk as u64;
        if chunk == 0 {
            break;
        }
    }
    {
        let inode = table.slot_mut(slot);
        inode.seek = false;
        if !params.read_only {
            inode.pending_updates |= crate::inode::UPDATE_ACCESS;
            inode.dirty = true;
        }
    }
    Ok(moved)
}

/// File parameters for transfers: geometry, block size, mount facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileParams {
    /// Mapping geometry.
    pub map: MapParams,
    /// Valid zone range for indirect validation.
    pub range: ZoneRange,
    /// File block size in bytes.
    pub block_size: usize,
    /// Whether the mount is read-only (skips timestamp updates).
    pub read_only: bool,
}

/// Map one file block through the slot's zones.
fn map_position<S: BlockSource>(
    table: &InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    slot: usize,
    params: &FileParams,
    file_block: u64,
) -> Result<Option<u64>, ReadError> {
    let zones = table.slot(slot).zones;
    map_file_block(cache, device, &zones, params.map, params.range, file_block)
}

/// Warm the blocks following a read (`rahead` policy, `read.c:330-448`).
///
/// Maps up to the prefetch minimum ahead (skipped after a seek, like the C
/// code) and warms the mapped device blocks through the block layer.
/// Holes (unmapped) stop the run: only contiguous mapped runs warm, because
/// sparse regions need no cache. Returns warmed blocks; failures are silent
/// because demand reads follow.
fn readahead_file<S: BlockSource>(
    table: &InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    slot: usize,
    params: &FileParams,
    file_block: u64,
) -> usize {
    if table.slot(slot).seek {
        return 0;
    }
    // Copy the zones out first: mapping borrows the cache mutably per
    // call, so numbers cross the boundary by value.
    let zones = table.slot(slot).zones;
    let map_params = params.map;
    let range = params.range;
    let mut run = Vec::new();
    let mut block = file_block + 1;
    for _ in 0..PREFETCH_MINIMUM {
        // NOTE: mapping reads indirect blocks through the cache; each call
        // is self-contained, so sequential calls stay borrow-clean.
        match map_one(cache, device, &zones, map_params, range, block) {
            Ok(Some(device_block)) => run.push(device_block),
            _ => break,
        }
        block += 1;
    }
    let warmed = run.len().min(cache.readahead_limit());
    minix_fs::bio::prefetch_blocks(cache, device, &run[..warmed]);
    warmed
}

/// Map one file block without a table borrow (readahead helper).
fn map_one<S: BlockSource>(
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    zones: &[u64; crate::inode::TOTAL_ZONES],
    params: MapParams,
    range: ZoneRange,
    file_block: u64,
) -> Result<Option<u64>, ReadError> {
    map_file_block(cache, device, zones, params, range, file_block)
}

/// List directory entries (`fs_getdents`, `read.c:454-556`).
///
/// Position must be entry-aligned (sixty-four); anything else reports
/// missing, because a misaligned resume lost track (`read.c:471-472`).
/// Walks directory blocks from the resume point, skipping free slots,
/// resolving each target's type through the table, and staging through the
/// directory-entry encoder. A full caller buffer stops the walk with the
/// position rewound to the blocking entry; otherwise the position advances
/// to the size. Access time stamps on writable mounts. Eight parameters
/// mirror the C signature plus the parameter bundle; callers pass them in
/// C order (inode, position, capacity, geometry).
#[allow(clippy::too_many_arguments)]
pub fn list_dir_entries<S: BlockSource>(
    table: &mut InodeTable,
    cache: &mut BlockCache<S, NoSecondLevel>,
    device: u64,
    number: u64,
    position: &mut u64,
    capacity: usize,
    params: &FileParams,
    out: &mut dyn FnMut(&[u8]),
) -> Result<usize, ReadError> {
    let block_size = params.block_size;
    if !(*position).is_multiple_of(crate::inode::DIRECTORY_ENTRY_SIZE as u64) {
        return Err(ReadError::Unaligned);
    }
    let slot = table.find(device, number).ok_or(ReadError::Invalid)?;
    let (size, is_dir) = {
        let inode = table.slot(slot);
        (
            inode.size as u64,
            inode.mode as u32 & TYPE_MASK == crate::inode::TYPE_DIRECTORY,
        )
    };
    if !is_dir {
        return Err(ReadError::Invalid);
    }
    // Stage through the directory-entry encoder like the C code stages
    // through its static buffer (`read.c:482-483`); the encoder owns the
    // caller and staging buffers while the loop below only touches the
    // cache and the table, so borrows stay disjoint.
    let mut caller = alloc::vec![0u8; capacity.max(1)];
    let mut staging = alloc::vec![0u8; 512];
    let mut new_position = size;
    let total = {
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let channel = DataChannel::Present {
            backend: &mut backend,
            size: capacity,
        };
        let mut encoder = DentryEncoder::new(channel, capacity, &mut staging);
        let start = *position;
        let mut block_pos = start - start % block_size as u64;
        let mut outcome: Result<usize, ReadError> = Ok(0);
        let mut stopped = false;
        while block_pos < size && !stopped {
            // Map and copy the block image out first: holding cache bytes
            // across table opens would alias borrows.
            let file_block = block_pos / block_size as u64;
            let image = match map_position(table, cache, device, slot, params, file_block)? {
                Some(device_block) => {
                    let cache_slot = cache
                        .acquire(BlockKey::new(device, device_block), AcquireMode::Normal)
                        .map_err(|_| ReadError::Io)?;
                    let bytes = cache.slot_data(cache_slot).to_vec();
                    let _ = cache.release(cache_slot);
                    bytes
                }
                None => {
                    // Directories have no holes; a gap is corruption.
                    outcome = Err(ReadError::Io);
                    break;
                }
            };
            let entries = image.len() / crate::inode::DIRECTORY_ENTRY_SIZE;
            let mut index = if block_pos < *position {
                ((*position - block_pos) / crate::inode::DIRECTORY_ENTRY_SIZE as u64) as usize
            } else {
                0
            };
            while index < entries {
                let base = index * crate::inode::DIRECTORY_ENTRY_SIZE;
                let entry = crate::dir::DirEntry::from_bytes(
                    &image[base..base + crate::inode::DIRECTORY_ENTRY_SIZE],
                )
                .map_err(|_| ReadError::Io)?;
                if entry.ino == 0 {
                    index += 1;
                    continue;
                }
                // Name length up to the first zero, capped at sixty.
                let name_length = entry.name_len().min(crate::dir::ENTRY_NAME_SIZE);
                // Resolve the target type through the table (`read.c:516`).
                // Cold entries may need loading; try the table hit only (no
                // disk in listing: matches "seriously expensive" comment by
                // degrading to unknown).
                let raw_type = match table.find(device, entry.ino as u64) {
                    Some(target_slot) => file_type_byte(table.slot(target_slot).mode),
                    None => dirent_unknown(),
                };
                let entry_pos = block_pos + (index * crate::inode::DIRECTORY_ENTRY_SIZE) as u64;
                match encoder.add(entry.ino as u64, &entry.name[..name_length], raw_type) {
                    Ok(0) => {
                        // Caller buffer full: resume here next time.
                        new_position = entry_pos;
                        stopped = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => {
                        outcome = Err(ReadError::Io);
                        stopped = true;
                        break;
                    }
                }
                index += 1;
            }
            if !stopped {
                block_pos += block_size as u64;
            }
        }
        outcome?;
        encoder.finish().map_err(|_| ReadError::Io)?
    };
    *position = new_position;
    if !params.read_only {
        let inode = table.slot_mut(slot);
        inode.pending_updates |= crate::inode::UPDATE_ACCESS;
        inode.dirty = true;
    }
    out(&caller[..total.min(caller.len())]);
    Ok(total.min(caller.len()))
}

/// File type byte the C `IFTODT` macro computes (`dirent.h:125`).
fn file_type_byte(mode: u16) -> DirentType {
    DirentType::from_raw((((mode as u32) & 0o170000) >> 12) as u8)
}

/// Unknown type for targets not worth opening.
fn dirent_unknown() -> DirentType {
    DirentType::Unknown
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
        cache: BlockCache<CountingDisk>,
        superblock: Superblock,
        bitmap: Bitmap,
        io: InodeIo,
    }

    /// RamDisk that counts storage reads (prefetch-bounded assertions).
    struct CountingDisk {
        inner: RamDisk,
        reads: core::cell::Cell<usize>,
    }

    impl BlockSource for CountingDisk {
        fn block_size(&self) -> usize {
            self.inner.block_size()
        }

        fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
            self.reads.set(self.reads.get() + 1);
            self.inner.read_block(key, out)
        }

        fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
            self.inner.write_block(key, data)
        }
    }

    fn file_params() -> FileParams {
        FileParams {
            map: MapParams {
                direct_zones: 7,
                indirect_per_block: 128,
            },
            range: ZoneRange {
                first: 4,
                count: 64,
            },
            block_size: BLOCK_SIZE,
            read_only: false,
        }
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
            cache: BlockCache::with_pool(
                CountingDisk {
                    inner: RamDisk::new(64, BLOCK_SIZE).unwrap(),
                    reads: core::cell::Cell::new(0),
                },
                NoSecondLevel,
                8,
            )
            .unwrap(),
            bitmap: Bitmap::new(65),
            io: InodeIo::from_superblock(&superblock),
            superblock,
        }
    }

    /// Open a file slot through the real allocator, then shape it: zones,
    /// size, link count. Every slot the tests touch is hashed and
    /// reference-counted exactly like server flow.
    fn open_file(
        fixture: &mut Fixture,
        zones: [u64; crate::inode::TOTAL_ZONES],
        size: i64,
        mode: u16,
    ) -> usize {
        let slot = fixture
            .table
            .allocate(
                &mut fixture.cache,
                &mut fixture.superblock,
                &mut fixture.bitmap,
                mode,
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

    fn write_device_block(fixture: &mut Fixture, block: u64, fill: u8) {
        let slot = fixture
            .cache
            .acquire(BlockKey::new(DEVICE, block), AcquireMode::NoRead)
            .unwrap();
        fixture.cache.slot_data_mut(slot).fill(fill);
        fixture.cache.mark_dirty(slot);
        fixture.cache.release(slot).unwrap();
        fixture.cache.flush_device(DEVICE).unwrap();
    }

    fn write_indirect(fixture: &mut Fixture, zone: u64, entries: &[u32]) {
        let slot = fixture
            .cache
            .acquire(BlockKey::new(DEVICE, zone), AcquireMode::NoRead)
            .unwrap();
        {
            let data = fixture.cache.slot_data_mut(slot);
            for byte in data.iter_mut() {
                *byte = 0;
            }
            for (index, entry) in entries.iter().enumerate() {
                data[index * 4..index * 4 + 4].copy_from_slice(&entry.to_le_bytes());
            }
        }
        fixture.cache.mark_dirty(slot);
        fixture.cache.release(slot).unwrap();
        fixture.cache.flush_device(DEVICE).unwrap();
    }

    #[test]
    fn test_map_direct_and_hole() {
        let mut fixture = fixture();
        let mut zones = [0u64; 10];
        zones[0] = 10;
        zones[3] = 13;
        let params = file_params();
        let range = params.range;
        let map = params.map;
        assert_eq!(
            map_file_block(&mut fixture.cache, DEVICE, &zones, map, range, 0).unwrap(),
            Some(10)
        );
        assert_eq!(
            map_file_block(&mut fixture.cache, DEVICE, &zones, map, range, 1).unwrap(),
            None
        );
        assert_eq!(
            map_file_block(&mut fixture.cache, DEVICE, &zones, map, range, 3).unwrap(),
            Some(13)
        );
    }

    #[test]
    fn test_map_single_and_double_indirect() {
        let mut fixture = fixture();
        // Indirect block at zone 20 naming zones 30 and 31.
        write_indirect(&mut fixture, 20, &[30, 31]);
        // Double indirect block at zone 21 naming single block 22, which
        // names zone 32.
        write_indirect(&mut fixture, 22, &[32]);
        write_indirect(&mut fixture, 21, &[22]);
        let mut zones = [0u64; 10];
        zones[7] = 20;
        zones[8] = 21;
        let params = file_params();
        // File block seven is the first single-indirect position.
        assert_eq!(
            map_file_block(
                &mut fixture.cache,
                DEVICE,
                &zones,
                params.map,
                params.range,
                7
            )
            .unwrap(),
            Some(30)
        );
        assert_eq!(
            map_file_block(
                &mut fixture.cache,
                DEVICE,
                &zones,
                params.map,
                params.range,
                8
            )
            .unwrap(),
            Some(31)
        );
        // File block 7+128 enters double-indirect territory.
        assert_eq!(
            map_file_block(
                &mut fixture.cache,
                DEVICE,
                &zones,
                params.map,
                params.range,
                135
            )
            .unwrap(),
            Some(32)
        );
        // Missing single block maps empty.
        let bare = [0u64; 10];
        assert_eq!(
            map_file_block(
                &mut fixture.cache,
                DEVICE,
                &bare,
                params.map,
                params.range,
                7
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn test_map_rejects_forged_pointers() {
        let mut fixture = fixture();
        // Indirect entry pointing below the data range is corruption.
        write_indirect(&mut fixture, 20, &[1]);
        let mut zones = [0u64; 10];
        zones[7] = 20;
        let params = file_params();
        assert_eq!(
            map_file_block(
                &mut fixture.cache,
                DEVICE,
                &zones,
                params.map,
                params.range,
                7
            )
            .unwrap_err(),
            ReadError::Io
        );
    }

    #[test]
    fn test_read_spans_blocks_and_holes() {
        let mut fixture = fixture();
        write_device_block(&mut fixture, 10, 0xAA);
        write_device_block(&mut fixture, 12, 0xBB);
        let mut zones = [0u64; 10];
        zones[0] = 10;
        zones[2] = 12;
        let slot = open_file(
            &mut fixture,
            zones,
            (BLOCK_SIZE * 3) as i64,
            TYPE_REGULAR as u16,
        );
        let params = file_params();
        let number = fixture.table.slot(slot).number;
        // Span the hole in block one: pattern, zeros, then pattern.
        let mut seen = Vec::new();
        let moved = read_file(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            number,
            0,
            BLOCK_SIZE * 3,
            params,
            &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
        )
        .unwrap();
        assert_eq!(moved, BLOCK_SIZE * 3);
        assert!(seen[..BLOCK_SIZE].iter().all(|&byte| byte == 0xAA));
        assert!(
            seen[BLOCK_SIZE..2 * BLOCK_SIZE]
                .iter()
                .all(|&byte| byte == 0)
        );
        assert!(seen[2 * BLOCK_SIZE..].iter().all(|&byte| byte == 0xBB));
        // Seek flag cleared, access stamped dirty.
        assert!(!fixture.table.slot(slot).seek);
        assert!(fixture.table.slot(slot).dirty);
    }

    #[test]
    fn test_read_clamps_at_end_of_file() {
        let mut fixture = fixture();
        write_device_block(&mut fixture, 10, 0xCC);
        let mut zones = [0u64; 10];
        zones[0] = 10;
        let slot = open_file(&mut fixture, zones, 100, TYPE_REGULAR as u16);
        let params = file_params();
        let number = fixture.table.slot(slot).number;
        let mut seen = Vec::new();
        let moved = read_file(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            number,
            0,
            1000,
            params,
            &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
        )
        .unwrap();
        assert_eq!(moved, 100);
        assert_eq!(seen.len(), 100);
        // Past the end reports zero, not an error.
        let mut seen = Vec::new();
        let moved = read_file(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            number,
            100,
            50,
            params,
            &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
        )
        .unwrap();
        assert_eq!(moved, 0);
        // Negative positions refuse.
        let mut seen = Vec::new();
        assert_eq!(
            read_file(
                &mut fixture.table,
                &mut fixture.cache,
                DEVICE,
                number,
                -1,
                10,
                params,
                &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
            )
            .unwrap_err(),
            ReadError::Invalid
        );
    }

    #[test]
    fn test_readahead_is_bounded() {
        let mut fixture = fixture();
        for block in 10..20u64 {
            write_device_block(&mut fixture, block, block as u8);
        }
        let mut zones = [0u64; 10];
        for (index, zone) in (10..20u64).enumerate() {
            if index < 7 {
                zones[index] = zone;
            }
        }
        // Seven direct zones: blocks ten to sixteen contiguous.
        let slot = open_file(
            &mut fixture,
            zones,
            (BLOCK_SIZE * 7) as i64,
            TYPE_REGULAR as u16,
        );
        let params = file_params();
        let number = fixture.table.slot(slot).number;
        fixture.cache.source().reads.set(0);
        let mut seen = Vec::new();
        read_file(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            number,
            0,
            BLOCK_SIZE,
            params,
            &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
        )
        .unwrap();
        // One demand block plus bounded prefetch, never the whole file.
        let reads = fixture.cache.source().reads.get();
        assert!(reads >= 1 && reads <= 1 + fixture.cache.readahead_limit());
    }

    #[test]
    fn test_list_dir_entries_types_and_resume() {
        use crate::dir::{DirEntry, ENTRY_NAME_SIZE};
        let mut fixture = fixture();
        // Targets first, so their real numbers land in the images.
        let file = open_file(&mut fixture, [0u64; 10], 0, TYPE_REGULAR as u16);
        let file_number = fixture.table.slot(file).number;
        let sub = open_file(&mut fixture, [0u64; 10], 0, TYPE_DIRECTORY as u16);
        let sub_number = fixture.table.slot(sub).number;
        // Directory block at zone forty: live file entry, free slot, dir.
        let mut image = alloc::vec![0u8; BLOCK_SIZE];
        let mut first = DirEntry {
            ino: file_number as u32,
            name: [0u8; ENTRY_NAME_SIZE],
        };
        first.name[..4].copy_from_slice(b"file");
        image[..64].copy_from_slice(&first.to_bytes());
        let mut second = DirEntry {
            ino: sub_number as u32,
            name: [0u8; ENTRY_NAME_SIZE],
        };
        second.name[..3].copy_from_slice(b"sub");
        image[64..128].copy_from_slice(&second.to_bytes());
        {
            let slot = fixture
                .cache
                .acquire(BlockKey::new(DEVICE, 40), AcquireMode::NoRead)
                .unwrap();
            fixture.cache.slot_data_mut(slot).copy_from_slice(&image);
            fixture.cache.mark_dirty(slot);
            fixture.cache.release(slot).unwrap();
            fixture.cache.flush_device(DEVICE).unwrap();
        }
        let mut zones = [0u64; 10];
        zones[0] = 40;
        let dir = open_file(&mut fixture, zones, 192, TYPE_DIRECTORY as u16);
        let dir_number = fixture.table.slot(dir).number;
        // Tiny capacity forces a mid-walk stop with resume position.
        let mut position = 0u64;
        let mut first_batch = Vec::new();
        let params = file_params();
        let moved = list_dir_entries(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            dir_number,
            &mut position,
            24,
            &params,
            &mut |chunk: &[u8]| first_batch.extend_from_slice(chunk),
        )
        .unwrap();
        assert!(moved > 0);
        assert_eq!(position, 64);
        // Resume lists the rest and parks at the size.
        let mut rest = Vec::new();
        let moved = list_dir_entries(
            &mut fixture.table,
            &mut fixture.cache,
            DEVICE,
            dir_number,
            &mut position,
            1024,
            &params,
            &mut |chunk: &[u8]| rest.extend_from_slice(chunk),
        )
        .unwrap();
        assert!(moved > 0);
        assert_eq!(position, 192);
        // Misaligned resume reports missing.
        let mut bad = 10u64;
        assert_eq!(
            list_dir_entries(
                &mut fixture.table,
                &mut fixture.cache,
                DEVICE,
                dir_number,
                &mut bad,
                1024,
                &params,
                &mut |_| {},
            )
            .unwrap_err(),
            ReadError::Unaligned
        );
    }

    #[test]
    fn test_type_byte_matches_iftodt() {
        assert_eq!(file_type_byte(0o040755) as u8, 4);
        assert_eq!(file_type_byte(0o100644) as u8, 8);
        assert_eq!(file_type_byte(0o120777) as u8, 10);
        assert_eq!(dirent_unknown() as u8, 0);
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EINVAL, EIO, ENOENT};
        assert_eq!(ReadError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(ReadError::Io.to_errno().to_i32(), EIO);
        assert_eq!(ReadError::Unaligned.to_errno().to_i32(), ENOENT);
        assert_eq!(PREFETCH_MINIMUM, 32);
    }
}
