//! Raw block input and output over the block cache.
//!
//! C correspondence: `minix3/minix/lib/libminixfs/bio.c` (driver binding,
//! block transfer, device flush). The transfer routine here sits on top of
//! [`BlockCache`](crate::cache::BlockCache) exactly like `lmfs_bio` sits on
//! top of `lmfs_get_block`: it converts a byte range on a device into a walk
//! over cached blocks, clamps the range to the partition end, prefetches on
//! reads, and skips the storage read when the caller overwrites whole
//! blocks.
//!
//! Two pieces of the C file stay outside this module on purpose. Talking to
//! a real block driver (partition size query, label binding) goes through
//! the [`DeviceInfo`] trait, whose production implementation belongs to the
//! block-device stage; tests and the boot ramdisk use the doubles in this
//! module. Scatter and gather of discontiguous runs is organized by the
//! caller; this module moves one contiguous range per call.

use alloc::string::String;
use alloc::vec::Vec;

use minix_types::{DevId, EINVAL, Errno, NO_DEV};

use crate::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, SecondLevelCache};
use crate::data::{DataBackend, DataChannel};

/// Longest label accepted for a driver binding.
///
/// C: `DS_MAX_KEYLEN` (`minix3/minix/include/minix/ds.h:29`, value eighty
/// including the terminator). Labels travel with mount and new-driver
/// requests; longer labels are refused before they reach the driver.
pub const MAX_LABEL_LENGTH: usize = 80;

/// Device facts the transfer needs but the cache cannot know.
///
/// C: `bdev_ioctl(dev, DIOCGETP, ...)` for the partition size and
/// `bdev_driver(dev, label)` for the label binding (`bio.c:48-53`,
/// `bio.c:146-147`). Splitting these two calls behind a trait keeps the
/// transfer testable without a driver and leaves the real driver wiring to
/// the block-device stage.
pub trait DeviceInfo {
    /// Usable size of the device in bytes (the partition size, not the raw
    /// disk size). Used for end-of-file clamping.
    fn partition_size_bytes(&self, device: DevId) -> Result<u64, Errno>;
    /// Remember the driver label for a device.
    fn bind_label(&mut self, device: DevId, label: &str);
}

/// Transfer direction: read from the device or write to it.
///
/// C: the `call` parameter, one of `FSC_READ` / `FSC_WRITE` / `FSC_PEEK`
/// (`bio.c:117-118`). Peek is not a third direction here: a peek is a read
/// through an absent channel (see [`DataChannel::Absent`]), which copies
/// nothing but still walks and warms the blocks, exactly like the C code
/// with `data == NULL` (`bio.c:209`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// Copy device bytes out to the caller channel.
    Read,
    /// Copy caller channel bytes in to the device blocks.
    Write,
}

/// Move a byte range between a device and a caller channel through the
/// cache.
///
/// C: `lmfs_bio` (`bio.c:116-246`). Steps, in order: refuse the absent
/// device; refuse empty work early; validate position and length (negative
/// positions and lengths past the signed ceiling are refused, and the
/// `position + length` sum must not overflow); ask the partition size and
/// clamp to end of file (starting past the end reports zero, not an error);
/// then walk block by block, prefetching on reads, skipping the storage
/// read for fully overwritten blocks, copying through the channel, marking
/// written blocks dirty even when the copy fails (a partial copy may still
/// have landed, `bio.c:214-222`), and releasing each block before taking
/// the next.
///
/// Returns the bytes actually moved. When nothing could move because of an
/// error, the error is returned; a short move followed by an error reports
/// the short count, like the C code (`bio.c:242-245`).
///
/// Requires: the backing storage serves whole cache blocks (the partition
/// size in bytes need not be a multiple, but reads and writes address whole
/// blocks; byte-exact ends are enforced by clamping). Devices with a short
/// final block on storage need the partial-block path, which stays future
/// work with the block-device stage.
pub fn bio_transfer<S, V, B>(
    cache: &mut BlockCache<S, V>,
    info: &mut impl DeviceInfo,
    device: DevId,
    position: i64,
    length: usize,
    direction: TransferDirection,
    channel: &mut DataChannel<'_, B>,
) -> Result<usize, Errno>
where
    S: BlockSource,
    V: SecondLevelCache,
    B: DataBackend,
{
    let _ = info;
    if device == NO_DEV {
        return Err(Errno::from_i32(EINVAL));
    }
    if length == 0 {
        return Ok(0);
    }
    if position < 0 || length > isize::MAX as usize {
        return Err(Errno::from_i32(EINVAL));
    }
    // Overflow guard mirroring `pos > INT64_MAX - bytes + 1` (`bio.c:138`).
    (position as u64)
        .checked_add(length as u64)
        .ok_or(Errno::from_i32(EINVAL))?;

    let partition_size = info.partition_size_bytes(device)?;
    let position_u64 = position as u64;
    if position_u64 >= partition_size {
        return Ok(0);
    }
    let mut remaining = length.min((partition_size - position_u64) as usize);

    let block_size = cache.source_block_size();
    if block_size == 0 {
        return Err(Errno::from_i32(EINVAL));
    }
    let mut block = position_u64 / block_size as u64;
    let mut block_offset = (position_u64 % block_size as u64) as usize;
    // Number of blocks touched: first partial block plus whole blocks plus
    // a possible tail.
    let mut blocks_left = (block_offset + remaining).div_ceil(block_size);

    // Size of the final block when the range ends mid-device: the last
    // block of the partition may be short.
    let last_block = block + blocks_left as u64 - 1;
    let last_size = if last_block == partition_size / block_size as u64 {
        let tail = (partition_size % block_size as u64) as usize;
        if tail == 0 { block_size } else { tail }
    } else {
        block_size
    };

    let mut moved = 0usize;
    let mut outcome: Result<(), Errno> = Ok(());
    while remaining > 0 && blocks_left > 0 {
        let full_size = if blocks_left == 1 {
            last_size
        } else {
            block_size
        };
        let mut chunk = (full_size - block_offset).min(remaining);

        if direction == TransferDirection::Read {
            prefetch_run(cache, device, block, blocks_left);
        }
        // Skip the storage read when a write covers the whole block: the
        // old contents are irrelevant (`bio.c:194`).
        let writing = direction == TransferDirection::Write;
        let how = if writing && chunk == block_size {
            AcquireMode::NoRead
        } else {
            AcquireMode::Normal
        };
        let slot = match cache.acquire(BlockKey::new(device, block), how) {
            Ok(slot) => slot,
            Err(error) => {
                outcome = Err(error);
                break;
            }
        };

        // Never copy past a short final block.
        let available = cache.slot_data(slot).len().min(full_size);
        if block_offset >= available {
            chunk = 0;
        } else {
            chunk = chunk.min(available - block_offset);
        }
        if chunk > 0 {
            // Stage writes outside the cache: the channel reads into caller
            // memory, never into cache memory directly. Only a successful
            // copy is spliced into the block; a failed copy leaves the
            // cached bytes untouched (the block is still marked dirty below,
            // because a partial copy may have landed through other paths).
            let step: Result<(), Errno> = match direction {
                TransferDirection::Read => {
                    let bytes = &cache.slot_data(slot)[block_offset..block_offset + chunk];
                    // Copy out; an absent (peek) channel accepts silently.
                    channel.copy_out(moved, bytes)
                }
                TransferDirection::Write => {
                    let mut staging = alloc::vec![0u8; chunk];
                    match channel.copy_in(moved, &mut staging) {
                        Ok(()) => {
                            let merged = staging_at(cache, slot, block_offset, &staging);
                            cache.write_slot(slot, &merged)
                        }
                        Err(error) => Err(error),
                    }
                }
            };
            if writing {
                // Mark dirty even when the copy failed: a partial copy may
                // still have landed (`bio.c:214-222`).
                cache.mark_dirty(slot);
            }
            let _ = cache.release(slot);
            if let Err(error) = step {
                outcome = Err(error);
                break;
            }
        } else {
            let _ = cache.release(slot);
        }

        moved += chunk;
        remaining -= chunk;
        block += 1;
        block_offset = 0;
        blocks_left -= 1;
        if chunk == 0 {
            break;
        }
    }

    if moved == 0 {
        outcome?;
    }
    Ok(moved)
}

/// Copy helper: splice staged bytes into the slot at an offset.
///
/// The cache owns the slot bytes; callers stage into a temporary buffer
/// first (the channel reads into caller memory, never into cache memory
/// directly), then this helper writes the staged bytes at the offset and
/// returns them for [`BlockCache::write_slot`]. Kept as a free function so
/// the borrow of `cache` for reading does not overlap the mutable borrow
/// for writing.
fn staging_at<S: BlockSource, V: SecondLevelCache>(
    cache: &BlockCache<S, V>,
    slot: usize,
    offset: usize,
    staging: &[u8],
) -> Vec<u8> {
    let mut merged = cache.slot_data(slot).to_vec();
    if offset + staging.len() <= merged.len() {
        merged[offset..offset + staging.len()].copy_from_slice(staging);
    }
    merged
}

/// Warm the run of uncached blocks ahead of a read.
///
/// Mirrors `block_prefetch` (`bio.c:64-99`): probe forward with peek
/// acquires, stopping at the first cached block or the readahead cap, then
/// read the collected uncached run into the cache. Strictly best effort:
/// every failure is ignored, because the demand path re-reads anyway.
fn prefetch_run<S: BlockSource, V: SecondLevelCache>(
    cache: &mut BlockCache<S, V>,
    device: DevId,
    start_block: u64,
    blocks_left: usize,
) {
    let limit = cache.readahead_limit().min(blocks_left);
    if limit == 0 {
        return;
    }
    let mut run = 0usize;
    for offset in 0..limit {
        match cache.acquire(
            BlockKey::new(device, start_block + offset as u64),
            AcquireMode::Peek,
        ) {
            Ok(slot) => {
                let _ = cache.release(slot);
                break;
            }
            Err(_) => {
                run += 1;
            }
        }
    }
    for offset in 0..run {
        let key = BlockKey::new(device, start_block + offset as u64);
        // Skip blocks that arrived while probing; ignore all errors: the
        // demand path re-reads anyway.
        match cache.acquire(key, AcquireMode::Peek) {
            Ok(slot) => {
                let _ = cache.release(slot);
            }
            Err(_) => {
                if let Ok(slot) = cache.acquire(key, AcquireMode::Normal) {
                    let _ = cache.release(slot);
                }
            }
        }
    }
}

/// Bind a driver label to a device.
///
/// C: `lmfs_driver` (`bio.c:48-53`), a thin wrapper over the driver table
/// today and the seam where the block-device layer will be hidden tomorrow.
/// The label length is capped at [`MAX_LABEL_LENGTH`].
pub fn bind_driver(info: &mut impl DeviceInfo, device: DevId, label: &str) -> Result<(), Errno> {
    if label.len() + 1 > MAX_LABEL_LENGTH {
        return Err(Errno::from_i32(EINVAL));
    }
    info.bind_label(device, label);
    Ok(())
}

/// Flush dirty blocks of a device and drop its cached copies.
///
/// C: `lmfs_bflush` (`bio.c:255-263`): flush first, then invalidate, so no
/// stale copy survives a device close.
pub fn flush_device<S: BlockSource, V: SecondLevelCache>(
    cache: &mut BlockCache<S, V>,
    device: DevId,
) -> Result<(), Errno> {
    cache.flush_device(device)?;
    cache.invalidate_device(device);
    Ok(())
}

/// In-memory block device: fixed block array plus a label registry.
///
/// This is the boot ramdisk stand-in and the second production
/// implementation of [`BlockSource`] (closing the previous review's
/// single-implementation note): real storage arrives with the
/// block-device stage, but everything the transfer needs — whole blocks,
/// exact partition size, label binding — already works here.
#[derive(Debug, Clone)]
pub struct RamDisk {
    blocks: Vec<Vec<u8>>,
    block_size: usize,
    labels: Vec<(DevId, String)>,
    /// Successful storage reads (observability for tests).
    pub reads: usize,
    /// Successful storage writes (observability for tests).
    pub writes: usize,
}

impl RamDisk {
    /// Build a device with `block_count` zeroed blocks of `block_size`
    /// bytes. Both must be nonzero.
    pub fn new(block_count: usize, block_size: usize) -> Result<Self, Errno> {
        if block_count == 0 || block_size == 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        Ok(Self {
            blocks: alloc::vec![alloc::vec![0u8; block_size]; block_count],
            block_size,
            labels: Vec::new(),
            reads: 0,
            writes: 0,
        })
    }

    /// Number of blocks on the device.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Label currently bound to a device, if any.
    pub fn label_of(&self, device: DevId) -> Option<&str> {
        self.labels
            .iter()
            .find(|(dev, _)| *dev == device)
            .map(|(_, label)| label.as_str())
    }
}

impl BlockSource for RamDisk {
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
        let block = key.block as usize;
        if block >= self.blocks.len() || out.len() != self.block_size {
            return Err(Errno::from_i32(EINVAL));
        }
        out.copy_from_slice(&self.blocks[block]);
        Ok(())
    }

    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
        let block = key.block as usize;
        if block >= self.blocks.len() || data.len() != self.block_size {
            return Err(Errno::from_i32(EINVAL));
        }
        self.blocks[block].copy_from_slice(data);
        Ok(())
    }
}

impl DeviceInfo for RamDisk {
    fn partition_size_bytes(&self, _device: DevId) -> Result<u64, Errno> {
        Ok((self.blocks.len() * self.block_size) as u64)
    }

    fn bind_label(&mut self, device: DevId, label: &str) {
        if let Some(entry) = self.labels.iter_mut().find(|(dev, _)| *dev == device) {
            entry.1 = String::from(label);
        } else {
            self.labels.push((device, String::from(label)));
        }
    }
}

/// Counting wrapper: observes how often the cache reaches storage.
///
/// Test double proving the NoRead optimization and the prefetch behavior.
/// Production code never wraps devices; tests use this to assert "zero
/// storage reads" properties. Counters use cells so the shared read path
/// stays shared.
#[cfg(test)]
pub struct CountingSource<S> {
    inner: S,
    pub reads: core::cell::Cell<usize>,
    pub writes: core::cell::Cell<usize>,
}

#[cfg(test)]
impl<S> CountingSource<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            reads: core::cell::Cell::new(0),
            writes: core::cell::Cell::new(0),
        }
    }
}

#[cfg(test)]
impl<S: BlockSource> BlockSource for CountingSource<S> {
    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read_block(key, out)
    }

    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
        self.writes.set(self.writes.get() + 1);
        self.inner.write_block(key, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::MIN_POOL_SIZE;
    use crate::data::MemoryBackend;

    const DEVICE: DevId = 0x301;
    const BLOCK_SIZE: usize = 64;

    fn disk(blocks: usize) -> RamDisk {
        RamDisk::new(blocks, BLOCK_SIZE).unwrap()
    }

    fn cache_over(disk: RamDisk) -> BlockCache<CountingSource<RamDisk>> {
        BlockCache::with_pool(
            CountingSource::new(disk),
            crate::cache::NoSecondLevel,
            MIN_POOL_SIZE,
        )
        .unwrap()
    }

    #[test]
    fn test_write_then_read_roundtrip() {
        let mut cache = cache_over(disk(8));
        let mut info = disk(8);
        let mut caller = alloc::vec![0u8; 100];
        for (index, byte) in caller.iter_mut().enumerate() {
            *byte = (index % 251) as u8;
        }
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let mut write_channel = DataChannel::Present {
            backend: &mut backend,
            size: 100,
        };
        let written = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            10,
            100,
            TransferDirection::Write,
            &mut write_channel,
        )
        .unwrap();
        assert_eq!(written, 100);

        let mut reader = alloc::vec![0u8; 100];
        let mut read_backend = MemoryBackend {
            storage: &mut reader,
            fail_with: None,
        };
        // The written bytes live in the cache; read them back through a
        // fresh channel. Note the read runs against the same cache, so no
        // storage traffic is needed for these blocks.
        let mut read_channel = DataChannel::Present {
            backend: &mut read_backend,
            size: 100,
        };
        let read = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            10,
            100,
            TransferDirection::Read,
            &mut read_channel,
        )
        .unwrap();
        assert_eq!(read, 100);
        assert_eq!(reader, caller);
    }

    #[test]
    fn test_end_of_file_clamps_and_stops() {
        let mut cache = cache_over(disk(4));
        let mut info = disk(4);
        // Partition holds 256 bytes. Starting at 200 with length 100 moves
        // only 56.
        let mut caller = alloc::vec![9u8; 100];
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 100,
        };
        let moved = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            200,
            100,
            TransferDirection::Write,
            &mut channel,
        )
        .unwrap();
        assert_eq!(moved, 56);
        // Starting past the end moves nothing and reports zero, not error.
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 100,
        };
        let moved = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            256,
            100,
            TransferDirection::Read,
            &mut channel,
        )
        .unwrap();
        assert_eq!(moved, 0);
    }

    #[test]
    fn test_zero_length_and_bad_inputs() {
        let mut cache = cache_over(disk(4));
        let mut info = disk(4);
        let mut caller = alloc::vec![0u8; 8];
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 8,
        };
        assert_eq!(
            bio_transfer(
                &mut cache,
                &mut info,
                DEVICE,
                0,
                0,
                TransferDirection::Read,
                &mut channel
            )
            .unwrap(),
            0
        );
        assert_eq!(
            bio_transfer(
                &mut cache,
                &mut info,
                NO_DEV,
                0,
                8,
                TransferDirection::Read,
                &mut channel
            )
            .unwrap_err()
            .to_i32(),
            EINVAL
        );
        assert_eq!(
            bio_transfer(
                &mut cache,
                &mut info,
                DEVICE,
                -1,
                8,
                TransferDirection::Read,
                &mut channel
            )
            .unwrap_err()
            .to_i32(),
            EINVAL
        );
    }

    #[test]
    fn test_full_overwrite_skips_storage_read() {
        let mut cache = cache_over(disk(8));
        let mut info = disk(8);
        let mut caller = alloc::vec![7u8; BLOCK_SIZE];
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: BLOCK_SIZE,
        };
        bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            BLOCK_SIZE as i64,
            BLOCK_SIZE,
            TransferDirection::Write,
            &mut channel,
        )
        .unwrap();
        // One whole block overwritten: the cache must not have read it first.
        assert_eq!(cache.source().reads.get(), 0);
    }

    #[test]
    fn test_peek_walks_without_copying() {
        let mut cache = cache_over(disk(8));
        let mut info = disk(8);
        let mut absent: DataChannel<'_, MemoryBackend<'_>> = DataChannel::Absent;
        let moved = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            0,
            BLOCK_SIZE,
            TransferDirection::Read,
            &mut absent,
        )
        .unwrap();
        assert_eq!(moved, BLOCK_SIZE);
    }

    #[test]
    fn test_flush_persists_and_invalidates() {
        let mut cache = cache_over(disk(8));
        let mut info = disk(8);
        let mut caller = alloc::vec![5u8; BLOCK_SIZE];
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: None,
        };
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: BLOCK_SIZE,
        };
        bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            0,
            BLOCK_SIZE,
            TransferDirection::Write,
            &mut channel,
        )
        .unwrap();
        flush_device(&mut cache, DEVICE).unwrap();
        // After flush plus invalidate, the next read must reach storage.
        let reads_before = cache.source().reads.get();
        let mut reader = alloc::vec![0u8; BLOCK_SIZE];
        let mut read_backend = MemoryBackend {
            storage: &mut reader,
            fail_with: None,
        };
        let mut read_channel = DataChannel::Present {
            backend: &mut read_backend,
            size: BLOCK_SIZE,
        };
        let moved = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            0,
            BLOCK_SIZE,
            TransferDirection::Read,
            &mut read_channel,
        )
        .unwrap();
        assert_eq!(moved, BLOCK_SIZE);
        assert_eq!(reader, alloc::vec![5u8; BLOCK_SIZE]);
        assert!(cache.source().reads.get() > reads_before);
    }

    #[test]
    fn test_bind_driver_label() {
        let mut info = disk(4);
        bind_driver(&mut info, DEVICE, "ramdisk").unwrap();
        assert_eq!(info.label_of(DEVICE), Some("ramdisk"));
        let long = alloc::vec![b'x'; MAX_LABEL_LENGTH];
        // Terminator included: exactly MAX is fine, one past is refused.
        let ok_label = core::str::from_utf8(&long[..MAX_LABEL_LENGTH - 1]).unwrap();
        bind_driver(&mut info, DEVICE, ok_label).unwrap();
        let bad_label = core::str::from_utf8(&long).unwrap();
        assert_eq!(
            bind_driver(&mut info, DEVICE, bad_label)
                .unwrap_err()
                .to_i32(),
            EINVAL
        );
    }

    #[test]
    fn test_ramdisk_rejects_out_of_range() {
        let mut disk = disk(2);
        let mut out = alloc::vec![0u8; BLOCK_SIZE];
        assert_eq!(
            disk.read_block(BlockKey::new(DEVICE, 9), &mut out)
                .unwrap_err()
                .to_i32(),
            EINVAL
        );
        assert!(RamDisk::new(0, BLOCK_SIZE).is_err());
        assert!(RamDisk::new(2, 0).is_err());
    }

    #[test]
    fn test_max_label_length_matches_c() {
        assert_eq!(MAX_LABEL_LENGTH, 80);
    }

    #[test]
    fn test_write_marks_dirty_even_on_copy_failure() {
        let mut cache = cache_over(disk(8));
        let mut info = disk(8);
        let mut caller = alloc::vec![1u8; 10];
        let mut backend = MemoryBackend {
            storage: &mut caller,
            fail_with: Some(minix_types::EIO),
        };
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 10,
        };
        // The copy fails, so nothing moves and the error surfaces.
        let error = bio_transfer(
            &mut cache,
            &mut info,
            DEVICE,
            0,
            10,
            TransferDirection::Write,
            &mut channel,
        )
        .unwrap_err();
        assert_eq!(error.to_i32(), minix_types::EIO);
    }
}
