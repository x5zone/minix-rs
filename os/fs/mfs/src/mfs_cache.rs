//! Cache wrappers: the file-system-facing block and zone helpers.
//!
//! C correspondence: `minix3/minix/fs/mfs/cache.c` (all one hundred nine
//! lines): `get_block`, `alloc_zone`, `free_zone`. The block wrapper adds
//! the file-system error policy on top of the cache; the zone helpers
//! implement the allocation policy (search hints, exhaustion, search-position
//! upkeep) over an injected bitmap, whose storage-backed implementation
//! arrives with the superblock stage (document 08).

use minix_types::{EIO, ENOENT, ENOSPC, Errno};

use minix_fs::cache::{AcquireMode, BlockCache, BlockKey, BlockSource, SecondLevelCache};

/// Absent zone number: allocation failure (`NO_ZONE`, `minix3/minix/include/minix/const.h:131`).
pub const NO_ZONE: u64 = 0;

/// Zone-space geometry the allocation policy needs.
///
/// A trimmed view of the superblock fields the wrappers consult
/// (`s_firstdatazone`, `s_zones`, `s_zsearch` in `super.h:51-59`). The full
/// superblock lives in the superblock stage; this struct carries only what
/// allocation policy reads and writes, so the policy stays testable without
/// a mounted image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneSpace {
    /// First data zone number (zones below are metadata).
    pub first_data_zone: u64,
    /// Total zones on the device (`s_zones`).
    pub zone_count: u64,
    /// Search hint: zones below are believed in use (`s_zsearch`).
    pub zsearch: u64,
}

/// Acquire a block with the file-system error policy.
///
/// C: `get_block` (`cache.c:22-37`). The cache distinguishes "not present"
/// from real failures; the file system treats anything but those two as
/// corruption-class (the C code aborts the server here) and reports it.
/// A miss is only legitimate for peek mode, which the C code asserts; a
/// miss in any other mode is reported as an input-output error instead of
/// aborting.
pub fn mfs_get_block<S: BlockSource, V: SecondLevelCache>(
    cache: &mut BlockCache<S, V>,
    key: BlockKey,
    mode: AcquireMode,
) -> Result<Option<usize>, Errno> {
    match cache.acquire(key, mode) {
        Ok(slot) => Ok(Some(slot)),
        Err(error) if error.to_i32() == ENOENT => {
            if mode == AcquireMode::Peek {
                Ok(None)
            } else {
                Err(Errno::from_i32(EIO))
            }
        }
        Err(error) => Err(error),
    }
}

/// Allocate a zone near a hint.
///
/// C: `alloc_zone` (`cache.c:42-80`). Bit numbers and zone numbers convert
/// with `zone = first_data_zone - 1 + bit`, and bit zero is never returned
/// (it means failure). The search starts at the recorded hint when asking
/// near the first data zone, otherwise near the requested zone. Exhaustion
/// reports "no space"; the one-time console warning in the C code is
/// presentation and stays with the caller (a library does not print).
/// The bitmap itself (`alloc_bit`, document 08) arrives as a closure taking
/// the start hint and returning the allocated bit number, if any.
pub fn alloc_zone(
    space: &mut ZoneSpace,
    near: u64,
    alloc_bit: &mut dyn FnMut(u64) -> Option<u64>,
) -> Result<u64, Errno> {
    let hint = if near == space.first_data_zone {
        space.zsearch
    } else {
        near.saturating_sub(space.first_data_zone.saturating_sub(1))
    };
    match alloc_bit(hint) {
        None => Err(Errno::from_i32(ENOSPC)),
        Some(bit) => {
            if bit == 0 {
                return Err(Errno::from_i32(ENOSPC));
            }
            if near == space.first_data_zone {
                space.zsearch = bit;
            }
            Ok(space.first_data_zone.saturating_sub(1).saturating_add(bit))
        }
    }
}

/// Return a zone to the free pool.
///
/// C: `free_zone` (`cache.c:85-109`). Zones outside the data range are
/// silently ignored (the caller passed a stale number; there is nothing to
/// free). Otherwise the bitmap bit is cleared, the search hint rewinds when
/// the freed bit precedes it, and the cache drops any copy of the block so
/// a later reader cannot see stale contents. Zones equal blocks here; the
/// multi-block-zone assertion in the C code is enforced at mount time
/// (document 08 rejects nonzero zone sizes).
pub fn free_zone<S: BlockSource, V: SecondLevelCache>(
    cache: &mut BlockCache<S, V>,
    device: u64,
    space: &mut ZoneSpace,
    zone: u64,
    free_bit: &mut dyn FnMut(u64),
) {
    if zone < space.first_data_zone || zone >= space.zone_count {
        return;
    }
    let bit = zone - (space.first_data_zone - 1);
    free_bit(bit);
    if bit < space.zsearch {
        space.zsearch = bit;
    }
    cache.free_block(BlockKey::new(device, zone));
}

/// Validate a zone number against the data range (test and caller helper).
pub const fn in_data_range(space: &ZoneSpace, zone: u64) -> bool {
    zone >= space.first_data_zone && zone < space.zone_count
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec::Vec;
    use minix_fs::bio::RamDisk;

    const DEVICE: u64 = 0x301;

    fn cache() -> BlockCache<RamDisk> {
        BlockCache::with_pool(
            RamDisk::new(16, 64).unwrap(),
            minix_fs::cache::NoSecondLevel,
            8,
        )
        .unwrap()
    }

    fn zone_space() -> ZoneSpace {
        ZoneSpace {
            first_data_zone: 5,
            zone_count: 16,
            zsearch: 0,
        }
    }

    #[test]
    fn test_get_block_hit_and_peek_miss() {
        let mut cache = cache();
        let key = BlockKey::new(DEVICE, 3);
        let slot = mfs_get_block(&mut cache, key, AcquireMode::Normal)
            .unwrap()
            .expect("present on storage");
        cache.release(slot).unwrap();
        // Peek miss is a legitimate empty, not an error.
        cache.invalidate_device(DEVICE);
        assert_eq!(
            mfs_get_block(&mut cache, key, AcquireMode::Peek).unwrap(),
            None
        );
        // A miss in any other mode violates the peek-only rule: the C code
        // asserts here, this code reports an input-output error. A source
        // that answers "not present" drives the case.
        struct Missing;
        impl BlockSource for Missing {
            fn block_size(&self) -> usize {
                64
            }
            fn read_block(&self, _key: BlockKey, _out: &mut [u8]) -> Result<(), Errno> {
                Err(Errno::from_i32(ENOENT))
            }
            fn write_block(&mut self, _key: BlockKey, _data: &[u8]) -> Result<(), Errno> {
                Err(Errno::from_i32(EIO))
            }
        }
        let mut missing_cache: BlockCache<Missing> =
            BlockCache::with_pool(Missing, minix_fs::cache::NoSecondLevel, 8).unwrap();
        assert_eq!(
            mfs_get_block(&mut missing_cache, key, AcquireMode::Normal)
                .unwrap_err()
                .to_i32(),
            EIO
        );
    }

    #[test]
    fn test_alloc_zone_hint_and_exhaustion() {
        let mut space = zone_space();
        // Bitmap with bits 1..4 taken, bit 5 free.
        let mut taken = [true, true, true, true, true, false];
        let mut alloc = |hint: u64| -> Option<u64> {
            let mut bit = hint.max(1);
            while (bit as usize) < taken.len() {
                if !taken[bit as usize] {
                    taken[bit as usize] = true;
                    return Some(bit);
                }
                bit += 1;
            }
            None
        };
        // Near the first data zone: starts at the recorded hint.
        let zone = alloc_zone(&mut space, 5, &mut alloc).unwrap();
        assert_eq!(zone, 5 - 1 + 5);
        assert_eq!(space.zsearch, 5);
        // Exhaustion reports no space.
        assert_eq!(
            alloc_zone(&mut space, 5, &mut alloc).unwrap_err().to_i32(),
            ENOSPC
        );
        // Near another zone converts zone to bit before delegating.
        let mut space = zone_space();
        let mut seen = 0u64;
        let mut probe = |hint: u64| -> Option<u64> {
            seen = hint;
            Some(2)
        };
        let zone = alloc_zone(&mut space, 9, &mut probe).unwrap();
        assert_eq!(seen, 9 - (5 - 1));
        assert_eq!(zone, (5 - 1) + 2);
    }

    #[test]
    fn test_free_zone_guards_and_rewinds() {
        let mut cache = cache();
        let mut space = ZoneSpace {
            first_data_zone: 5,
            zone_count: 16,
            zsearch: 7,
        };
        // Cache a copy of block 9 first so the detach is observable.
        let key = BlockKey::new(DEVICE, 9);
        let slot = mfs_get_block(&mut cache, key, AcquireMode::Normal)
            .unwrap()
            .unwrap();
        cache.release(slot).unwrap();
        let mut freed = Vec::new();
        let mut free = |bit: u64| freed.push(bit);
        free_zone(&mut cache, DEVICE, &mut space, 9, &mut free);
        // Out-of-range zones are silently ignored: nothing is freed.
        free_zone(&mut cache, DEVICE, &mut space, 3, &mut free);
        free_zone(&mut cache, DEVICE, &mut space, 16, &mut free);
        assert_eq!(freed, alloc::vec![9 - (5 - 1)]);
        // Bit five precedes the hint seven: the hint rewinds to five.
        assert_eq!(space.zsearch, 5);
        // The cached copy is detached: next acquire re-reads storage.
        let slot = mfs_get_block(&mut cache, key, AcquireMode::Normal)
            .unwrap()
            .unwrap();
        cache.release(slot).unwrap();
    }

    #[test]
    fn test_range_helper() {
        let space = zone_space();
        assert!(!in_data_range(&space, 4));
        assert!(in_data_range(&space, 5));
        assert!(in_data_range(&space, 15));
        assert!(!in_data_range(&space, 16));
        assert_eq!(NO_ZONE, 0);
    }
}
