//! Superblock: on-disk layout, validation, and bitmaps.
//!
//! C correspondence: `minix3/minix/fs/mfs/super.c` (all three hundred
//! sixty-five lines) for the read-write-validate-bitmap logic and
//! `minix3/minix/fs/mfs/super.h` plus `const.h` for the layout and the
//! constants. The disk layout is parsed with explicit little-endian reads:
//! the format is defined little-endian and the target is little-endian, so
//! conversion is the identity spelled out field by field (never a blind
//! memory copy of a packed struct).
//!
//! Only the third format version is accepted, like the C code: older magic
//! numbers are refused at parse time with a dedicated error.

use alloc::vec::Vec;

use minix_types::{EINVAL, ENOSPC, EROFS, Errno};

/// Magic of the first format version (`const.h:22`, `0x137F`). Refused: only
/// the third version is supported.
pub const MAGIC_V1: u16 = 0x137F;
/// Magic of the second format version (`const.h:24`, `0x2468`). Refused.
pub const MAGIC_V2: u16 = 0x2468;
/// Magic of the third format version (`const.h:26`, `0x4D5A`). Accepted.
pub const MAGIC_V3: u16 = 0x4D5A;

/// Version number of the second format (`const.h:28`).
pub const VERSION_V2: i32 = 2;
/// Version number of the third format (`const.h:29`).
pub const VERSION_V3: i32 = 3;

/// First block of the file system area, skipping boot and superblock area
/// (`const.h:51`, value two). Bitmap block numbers count from here.
pub const START_BLOCK: u64 = 2;
/// Byte offset of the superblock inside block zero (`const.h:50`, one
/// thousand twenty-four). Block zero starts with the boot block; the
/// superblock follows it.
pub const SUPER_BLOCK_OFFSET: usize = 1024;
/// Block number of the boot block (`const.h:49`, value zero).
pub const BOOT_BLOCK: u64 = 0;
/// Inode number of the root directory (`const.h:48`, value one).
pub const ROOT_INODE_NUMBER: u64 = 1;

/// Inode bitmap selector (`super.h:62`).
pub const MAP_INODE: u32 = 0;
/// Zone bitmap selector (`super.h:63`).
pub const MAP_ZONE: u32 = 1;

/// Cleanly-unmounted flag (`super.h:68`): zero means dirty, one means the
/// file system was unmounted cleanly.
pub const FLAG_CLEAN: u16 = 1;
/// Mandatory feature-flag mask (`super.h:75`): any unknown bit in this range
/// refuses the mount, so newer formats fail gracefully instead of
/// corrupting.
pub const FLAG_MANDATORY_MASK: u16 = 0xFF00;

/// Direct zones in a second-version inode (`const.h:5`, value seven).
pub const DIRECT_ZONE_COUNT: usize = 7;
/// Total zone slots in a second-version inode (`const.h:6`, value ten).
pub const TOTAL_ZONE_COUNT: usize = 10;
/// On-disk second-version inode size in bytes (two plus two plus two plus
/// two plus four times four plus ten times four: sixty-four).
pub const INODE_DISK_SIZE: usize = 64;

/// In-core inode table slots (`const.h:8`, value five hundred twelve, kept
/// near the virtual file system service vnode count by convention).
pub const INODE_TABLE_SLOTS: usize = 512;

/// Smallest accepted block size in bytes: one page (`super.c:288-290`).
/// Smaller blocks cannot host a whole page, which the cache assumes.
pub const MIN_BLOCK_SIZE: usize = 4096;

/// On-disk superblock: the thirteen fields up to and including the disk
/// version (`super.c:184`, `LAST_ONDISK_FIELD`). Everything after the disk
/// version exists only in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskSuperblock {
    /// Usable inodes, not counting inode zero (always set, never used).
    pub inode_count: u32,
    /// Total device size in small zones (superseded by `zones` in v2+).
    pub zone_total_small: u16,
    /// Blocks in the inode bitmap.
    pub inode_map_blocks: i16,
    /// Blocks in the zone bitmap.
    pub zone_map_blocks: i16,
    /// First data zone, small form (zero when too large to fit).
    pub first_data_zone_small: u16,
    /// Log base two of blocks per zone; must be zero (one block per zone).
    pub log_zone_size: i16,
    /// File-system state flags (clean bit and feature bits).
    pub flags: u16,
    /// Maximum file size on this device.
    pub max_size: i32,
    /// Zone count (replaces the small total in v2+).
    pub zones: u32,
    /// Magic number recognizing the superblock.
    pub magic: u16,
    /// Padding against compiler-dependent layout (`super.h:38`).
    pub pad: i16,
    /// Block size in bytes.
    pub block_size: u16,
    /// Format sub-version; last field stored on disk.
    pub disk_version: u8,
}

impl DiskSuperblock {
    /// Stored size in bytes: four plus five times two plus two plus four
    /// plus four plus three times two plus one: thirty-one total.
    pub const STORED_BYTES: usize = 31;

    /// Parse the thirteen fields from little-endian bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SuperError> {
        if bytes.len() < Self::STORED_BYTES {
            return Err(SuperError::ShortBuffer);
        }
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let i16_at = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let u32_at = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let i32_at = |at: usize| {
            i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        Ok(Self {
            inode_count: u32_at(0),
            zone_total_small: u16_at(4),
            inode_map_blocks: i16_at(6),
            zone_map_blocks: i16_at(8),
            first_data_zone_small: u16_at(10),
            log_zone_size: i16_at(12),
            flags: u16_at(14),
            max_size: i32_at(16),
            zones: u32_at(20),
            magic: u16_at(24),
            pad: i16_at(26),
            block_size: u16_at(28),
            disk_version: bytes[30],
        })
    }

    /// Serialize the thirteen fields to little-endian bytes.
    pub fn to_bytes(&self) -> [u8; Self::STORED_BYTES] {
        let mut out = [0u8; Self::STORED_BYTES];
        out[0..4].copy_from_slice(&self.inode_count.to_le_bytes());
        out[4..6].copy_from_slice(&self.zone_total_small.to_le_bytes());
        out[6..8].copy_from_slice(&self.inode_map_blocks.to_le_bytes());
        out[8..10].copy_from_slice(&self.zone_map_blocks.to_le_bytes());
        out[10..12].copy_from_slice(&self.first_data_zone_small.to_le_bytes());
        out[12..14].copy_from_slice(&self.log_zone_size.to_le_bytes());
        out[14..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.max_size.to_le_bytes());
        out[20..24].copy_from_slice(&self.zones.to_le_bytes());
        out[24..26].copy_from_slice(&self.magic.to_le_bytes());
        out[26..28].copy_from_slice(&self.pad.to_le_bytes());
        out[28..30].copy_from_slice(&self.block_size.to_le_bytes());
        out[30] = self.disk_version;
        out
    }
}

/// Why a superblock was rejected.
///
/// Every variant maps to "invalid argument" on the wire except
/// [`SuperError::ReadOnly`], which maps to "read-only file system": the C
/// code aborts the server on a read-only write, this code reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuperError {
    /// Input shorter than the stored thirteen fields.
    ShortBuffer,
    /// First or second version magic: only the third is supported
    /// (`super.c:252-260`).
    UnsupportedVersion,
    /// Unrecognized magic (`super.c:258-260`).
    BadMagic,
    /// Blocks per zone is not one (`super.c:278-281`).
    SplitZones,
    /// Block size below one page, not a multiple of five hundred twelve, or
    /// smaller than the superblock itself (`super.c:288-321`).
    BadBlockSize,
    /// Geometry sanity check failed (`super.c:333-342`).
    BadGeometry,
    /// Unknown mandatory feature flags (`super.c:348-352`).
    UnsupportedFlags,
    /// Write to a read-only file system (`write_super`, `super.c:360-364`).
    ReadOnly,
    /// No free bit (bitmap exhaustion, `alloc_bit` returning `NO_BIT`).
    NoSpace,
}

impl SuperError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::ReadOnly => Errno::from_i32(EROFS),
            Self::NoSpace => Errno::from_i32(ENOSPC),
            _ => Errno::from_i32(EINVAL),
        }
    }
}

/// In-memory superblock: disk fields plus computed and runtime fields.
///
/// Mirrors `struct super_block` (`super.h:24-60`): the first thirteen fields
/// come from disk, the computed group is derived at parse time, and the
/// runtime group is set by the mounter (device, read-only flag). Search
/// hints start at zero (`super.c:327-328`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Superblock {
    /// Usable inodes.
    pub inode_count: u32,
    /// Blocks in the inode bitmap.
    pub inode_map_blocks: i16,
    /// Blocks in the zone bitmap.
    pub zone_map_blocks: i16,
    /// State and feature flags.
    pub flags: u16,
    /// Maximum file size, clamped to the address ceiling.
    pub max_size: i64,
    /// Zone count.
    pub zones: u64,
    /// Format version (always three here).
    pub version: i32,
    /// Block size in bytes.
    pub block_size: usize,
    /// Inodes per block, derived from the block size.
    pub inodes_per_block: u32,
    /// Direct zones per inode (seven).
    pub direct_zones: u32,
    /// Zones per indirect block, derived from the block size.
    pub indirect_per_block: u32,
    /// First data zone (computed when the small field is zero).
    pub first_data_zone: u64,
    /// Mounted device.
    pub device: u64,
    /// Mounted read-only.
    pub read_only: bool,
    /// Inode search hint.
    pub isearch: u64,
    /// Zone search hint.
    pub zsearch: u64,
}

/// Parse and validate block zero into an in-memory superblock.
///
/// C: `read_super` (`super.c:241-355`) after `rw_super` fetches the bytes.
/// `block_zero` is the whole block-zero image; the superblock is read at
/// [`SUPER_BLOCK_OFFSET`]. `device` and `read_only` are the mounter's
/// runtime facts. Checks run in C order: magic and version first, then zone
/// size, block size chain, size clamp, first-data-zone computation, geometry
/// sanity, mandatory flags.
pub fn parse_superblock(
    block_zero: &[u8],
    device: u64,
    read_only: bool,
) -> Result<Superblock, SuperError> {
    if block_zero.len() < SUPER_BLOCK_OFFSET + DiskSuperblock::STORED_BYTES {
        return Err(SuperError::ShortBuffer);
    }
    let disk = DiskSuperblock::from_bytes(&block_zero[SUPER_BLOCK_OFFSET..])?;

    if disk.magic == MAGIC_V1 || disk.magic == MAGIC_V2 {
        return Err(SuperError::UnsupportedVersion);
    }
    if disk.magic != MAGIC_V3 {
        return Err(SuperError::BadMagic);
    }
    if disk.log_zone_size != 0 {
        return Err(SuperError::SplitZones);
    }

    let block_size = disk.block_size as usize;
    if block_size < MIN_BLOCK_SIZE
        || !block_size.is_multiple_of(512)
        || DiskSuperblock::STORED_BYTES > block_size
        || !block_size.is_multiple_of(INODE_DISK_SIZE)
    {
        return Err(SuperError::BadBlockSize);
    }

    // Thirty-two-bit sizes always fit sixty-four-bit maxima: the C clamp to
    // LONG_MAX only bites where a long is thirty-two bits wide, never on
    // this target.
    let max_size = disk.max_size as i64;
    let inodes_per_block = (block_size / INODE_DISK_SIZE) as u32;
    let first_data_zone = if disk.first_data_zone_small == 0 {
        // Too large to fit in sixteen bits: recompute from the layout
        // (`super.c:299-308`).
        let mut offset = START_BLOCK + disk.inode_map_blocks as u64 + disk.zone_map_blocks as u64;
        offset += (disk.inode_count as u64).div_ceil(inodes_per_block as u64);
        offset
    } else {
        disk.first_data_zone_small as u64
    };

    if disk.inode_map_blocks < 1
        || disk.zone_map_blocks < 1
        || disk.inode_count < 1
        || (disk.zones as u64) < 1
        || first_data_zone <= 4
        || first_data_zone >= disk.zones as u64
        || disk.log_zone_size > 4
    {
        return Err(SuperError::BadGeometry);
    }
    if disk.flags & FLAG_MANDATORY_MASK != 0 {
        return Err(SuperError::UnsupportedFlags);
    }

    Ok(Superblock {
        inode_count: disk.inode_count,
        inode_map_blocks: disk.inode_map_blocks,
        zone_map_blocks: disk.zone_map_blocks,
        flags: disk.flags,
        max_size,
        zones: disk.zones as u64,
        version: VERSION_V3,
        block_size,
        inodes_per_block,
        direct_zones: DIRECT_ZONE_COUNT as u32,
        indirect_per_block: (block_size / core::mem::size_of::<u32>()) as u32,
        first_data_zone,
        device,
        read_only,
        isearch: 0,
        zsearch: 0,
    })
}

/// Refuse a superblock write on a read-only file system.
///
/// C: `write_super` aborts here (`super.c:362-363`); this code reports
/// instead. The actual block copy runs through the cache in the mount stage
/// (document 10).
pub const fn check_writable(read_only: bool) -> Result<(), SuperError> {
    if read_only {
        return Err(SuperError::ReadOnly);
    }
    Ok(())
}

/// Whether the clean flag marks a clean unmount (`super.h:68`).
pub const fn is_clean(flags: u16) -> bool {
    flags & FLAG_CLEAN != 0
}

/// Bitmap over thirty-two-bit words, little-endian word order.
///
/// Backs both the inode and the zone maps. Bit numbers count from zero;
/// number zero is reserved and never allocated (it means failure, `NO_BIT`
/// in `const.h:32`). Words past `bit_count` are treated as fully used so
/// allocation never escapes the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    words: Vec<u32>,
    bit_count: u64,
}

impl Bitmap {
    /// Empty map of `bit_count` bits, all free.
    pub fn new(bit_count: u64) -> Self {
        let words = alloc::vec![0u32; bit_count.div_ceil(32) as usize];
        Self { words, bit_count }
    }

    /// Whether a bit is set (out-of-range bits read as set: unusable).
    pub fn test(&self, bit: u64) -> bool {
        if bit >= self.bit_count {
            return true;
        }
        self.words[bit as usize / 32] & (1 << (bit % 32)) != 0
    }

    /// Allocate a bit at or after `origin`, wrapping once around the map.
    ///
    /// C: `alloc_bit` (`super.c:29-106`) minus storage: origin past the end
    /// restarts at zero, each word is byte-swapped around the read on
    /// foreign-endian images (identity here, spelled out at the call site),
    /// the first free bit wins, allocation marks dirty. Returns `None` when
    /// the map is full.
    pub fn alloc(&mut self, origin: u64) -> Option<u64> {
        if self.bit_count == 0 {
            return None;
        }
        let mut bit = if origin >= self.bit_count { 0 } else { origin };
        for _ in 0..self.bit_count {
            if !self.test(bit) {
                self.words[bit as usize / 32] |= 1 << (bit % 32);
                // Bit zero is failure, never allocation: skip it.
                if bit == 0 {
                    continue;
                }
                return Some(bit);
            }
            bit += 1;
            if bit >= self.bit_count {
                bit = 0;
            }
        }
        None
    }

    /// Clear a bit. Returns false for a double free (the C code aborts here:
    /// `super.c:141-144`); the caller decides whether that is corruption or
    /// a stale number.
    pub fn free(&mut self, bit: u64) -> bool {
        if bit >= self.bit_count || bit == 0 {
            return false;
        }
        let word = &mut self.words[bit as usize / 32];
        let mask = 1 << (bit % 32);
        if *word & mask == 0 {
            return false;
        }
        *word &= !mask;
        true
    }

    /// Bits in the map.
    pub const fn bit_count(&self) -> u64 {
        self.bit_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_disk() -> DiskSuperblock {
        DiskSuperblock {
            inode_count: 100,
            zone_total_small: 0,
            inode_map_blocks: 1,
            zone_map_blocks: 1,
            first_data_zone_small: 0,
            log_zone_size: 0,
            flags: FLAG_CLEAN,
            max_size: i32::MAX,
            zones: 2000,
            magic: MAGIC_V3,
            pad: 0,
            block_size: 4096,
            disk_version: 0,
        }
    }

    fn block_with(disk: &DiskSuperblock) -> Vec<u8> {
        let mut block = alloc::vec![0u8; 4096];
        block[SUPER_BLOCK_OFFSET..SUPER_BLOCK_OFFSET + DiskSuperblock::STORED_BYTES]
            .copy_from_slice(&disk.to_bytes());
        block
    }

    #[test]
    fn test_parse_valid_image() {
        let block = block_with(&valid_disk());
        let parsed = parse_superblock(&block, 0x301, false).unwrap();
        assert_eq!(parsed.version, VERSION_V3);
        assert_eq!(parsed.block_size, 4096);
        assert_eq!(parsed.inodes_per_block, 4096 / 64);
        assert_eq!(parsed.direct_zones, 7);
        assert_eq!(parsed.indirect_per_block, 4096 / 4);
        // First data zone recomputed: 2 + 1 + 1 + ceil(100 / 64) = 6.
        assert_eq!(parsed.first_data_zone, 6);
        assert_eq!(parsed.isearch, 0);
        assert_eq!(parsed.zsearch, 0);
        assert_eq!(parsed.device, 0x301);
        assert!(!parsed.read_only);
        assert!(is_clean(parsed.flags));
    }

    #[test]
    fn test_old_magics_rejected() {
        for magic in [MAGIC_V1, MAGIC_V2, 0x1234] {
            let mut disk = valid_disk();
            disk.magic = magic;
            let block = block_with(&disk);
            let error = parse_superblock(&block, 0, false).unwrap_err();
            if magic == 0x1234 {
                assert_eq!(error, SuperError::BadMagic);
            } else {
                assert_eq!(error, SuperError::UnsupportedVersion);
            }
            assert_eq!(error.to_errno().to_i32(), minix_types::EINVAL);
        }
    }

    #[test]
    fn test_block_size_chain() {
        let mut disk = valid_disk();
        disk.log_zone_size = 1;
        assert_eq!(
            parse_superblock(&block_with(&disk), 0, false).unwrap_err(),
            SuperError::SplitZones
        );
        let mut disk = valid_disk();
        disk.block_size = 1024;
        assert_eq!(
            parse_superblock(&block_with(&disk), 0, false).unwrap_err(),
            SuperError::BadBlockSize
        );
        let mut disk = valid_disk();
        disk.block_size = 4096 + 256;
        assert_eq!(
            parse_superblock(&block_with(&disk), 0, false).unwrap_err(),
            SuperError::BadBlockSize
        );
    }

    #[test]
    fn test_geometry_and_flags() {
        let mut disk = valid_disk();
        disk.inode_map_blocks = 0;
        assert_eq!(
            parse_superblock(&block_with(&disk), 0, false).unwrap_err(),
            SuperError::BadGeometry
        );
        let mut disk = valid_disk();
        disk.flags = 0x0100;
        assert_eq!(
            parse_superblock(&block_with(&disk), 0, false).unwrap_err(),
            SuperError::UnsupportedFlags
        );
        // Explicit small first zone is honored instead of recomputed.
        let mut disk = valid_disk();
        disk.first_data_zone_small = 9;
        disk.zones = 2000;
        let parsed = parse_superblock(&block_with(&disk), 0, false).unwrap();
        assert_eq!(parsed.first_data_zone, 9);
    }

    #[test]
    fn test_short_buffer_rejected() {
        assert_eq!(
            parse_superblock(&[0u8; 100], 0, false).unwrap_err(),
            SuperError::ShortBuffer
        );
    }

    #[test]
    fn test_write_guard() {
        assert!(check_writable(false).is_ok());
        assert_eq!(check_writable(true).unwrap_err(), SuperError::ReadOnly);
        assert_eq!(SuperError::ReadOnly.to_errno().to_i32(), minix_types::EROFS);
        assert_eq!(SuperError::NoSpace.to_errno().to_i32(), minix_types::ENOSPC);
    }

    #[test]
    fn test_disk_roundtrip() {
        let disk = valid_disk();
        let back = DiskSuperblock::from_bytes(&disk.to_bytes()).unwrap();
        assert_eq!(disk, back);
        assert_eq!(DiskSuperblock::STORED_BYTES, 31);
    }

    #[test]
    fn test_bitmap_alloc_free_wrap() {
        // Ten bits: zero reserved, one to nine usable.
        let mut map = Bitmap::new(10);
        assert_eq!(map.alloc(0), Some(1));
        assert_eq!(map.alloc(1), Some(2));
        // Origin past the end restarts at zero (skipping reserved zero).
        assert_eq!(map.alloc(99), Some(3));
        for bit in 4..10u64 {
            assert_eq!(map.alloc(0), Some(bit));
        }
        assert_eq!(map.alloc(0), None);
        // Double free is reported, not fatal.
        assert!(map.free(4));
        assert!(!map.free(4));
        assert!(!map.free(0));
        assert!(!map.free(99));
        assert_eq!(map.alloc(0), Some(4));
        assert!(map.test(4));
        // Out-of-range bits read as set: unusable by construction.
        assert!(map.test(99));
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(MAGIC_V1, 0x137F);
        assert_eq!(MAGIC_V2, 0x2468);
        assert_eq!(MAGIC_V3, 0x4D5A);
        assert_eq!(START_BLOCK, 2);
        assert_eq!(SUPER_BLOCK_OFFSET, 1024);
        assert_eq!(BOOT_BLOCK, 0);
        assert_eq!(ROOT_INODE_NUMBER, 1);
        assert_eq!(FLAG_CLEAN, 1);
        assert_eq!(FLAG_MANDATORY_MASK, 0xFF00);
        assert_eq!(DIRECT_ZONE_COUNT, 7);
        assert_eq!(TOTAL_ZONE_COUNT, 10);
        assert_eq!(INODE_DISK_SIZE, 64);
        assert_eq!(INODE_TABLE_SLOTS, 512);
    }
}
