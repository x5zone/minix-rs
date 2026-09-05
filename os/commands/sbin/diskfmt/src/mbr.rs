//! Master boot record parsing.
//!
//! Ground truth: `minix3/sys/sys/bootblock.h` (table at offset 446,
//! `MBR_PART_OFFSET`; magic `0xAA55` at 510, `MBR_MAGIC`; four entries,
//! `MBR_PART_COUNT`; active flag `0x80`, `MBR_PFLAG_ACTIVE`; type codes
//! including Minix `0x80`/`0x81`, Linux native `0x83`, 386BSD `0xA5`) and
//! the 16 byte entry layout (`mbrp_flag`, three start geometry bytes,
//! `mbrp_type`, three end geometry bytes, `mbrp_start`, `mbrp_size`, the
//! last two little endian).
//!
//! Only the little endian start/size fields carry geometry the tools
//! trust (cylinder/head/sector bytes are legacy approximations the C
//! tools display but never compute from); this module parses the same
//! fields plus the flag and type, and names the common types.

use crate::DiskError;

/// Byte offset of the partition table in a 512 byte sector.
pub const TABLE_OFFSET: usize = 446;
/// Byte offset of the magic number.
pub const MAGIC_OFFSET: usize = 510;
/// Expected magic, little endian (`0x55` then `0xAA` on disk).
pub const MAGIC: u16 = 0xAA55;
/// Entries per table.
pub const ENTRY_COUNT: usize = 4;
/// Entry size in bytes.
pub const ENTRY_SIZE: usize = 16;
/// Sector size in bytes.
pub const SECTOR_SIZE: usize = 512;
/// Active (bootable) flag value.
pub const FLAG_ACTIVE: u8 = 0x80;

/// One parsed partition entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionEntry {
    /// True when the active flag (`0x80`) is set.
    pub active: bool,
    /// Partition type byte (`0x81` Minix, `0x83` Linux native, ...).
    pub partition_type: u8,
    /// First sector, absolute (little endian on disk).
    pub start: u32,
    /// Sector count (little endian on disk).
    pub size: u32,
}

impl PartitionEntry {
    /// True when the type byte marks the slot unused.
    pub fn empty(self) -> bool {
        self.partition_type == 0x00
    }

    /// Last sector (inclusive), or `None` for empty slots and wrapping
    /// ranges (overflow is an error, never a wraparound).
    pub fn last_sector(self) -> Option<u32> {
        if self.empty() || self.size == 0 {
            return None;
        }
        self.start.checked_add(self.size - 1)
    }
}

/// Human name for the common type bytes (`bootblock.h` lines 279 to 328
/// plus the Minix pair at 299 to 300); unknown types report `None` rather
/// than a guessed label.
pub fn type_name(partition_type: u8) -> Option<&'static str> {
    match partition_type {
        0x00 => Some("unused"),
        0x01 => Some("FAT12"),
        0x04 => Some("FAT16 small"),
        0x05 => Some("extended"),
        0x06 => Some("FAT16 big"),
        0x07 => Some("NTFS"),
        0x0B => Some("FAT32"),
        0x0C => Some("FAT32 LBA"),
        0x0E => Some("FAT16 LBA"),
        0x0F => Some("extended LBA"),
        0x80 => Some("Minix until 1.4a"),
        0x81 => Some("Minix"),
        0x82 => Some("Linux swap"),
        0x83 => Some("Linux native"),
        0xA5 => Some("386BSD"),
        0xEB => Some("BeOS"),
        0xEE => Some("protective"),
        _ => None,
    }
}

/// Parse one 16 byte entry image.
pub fn parse_entry(image: &[u8]) -> Result<PartitionEntry, DiskError> {
    if image.len() != ENTRY_SIZE {
        return Err(DiskError::InvalidArgument);
    }
    let flag = image[0];
    if flag != 0x00 && flag != FLAG_ACTIVE {
        return Err(DiskError::InvalidArgument);
    }
    let start = u32::from_le_bytes([image[8], image[9], image[10], image[11]]);
    let size = u32::from_le_bytes([image[12], image[13], image[14], image[15]]);
    Ok(PartitionEntry {
        active: flag == FLAG_ACTIVE,
        partition_type: image[4],
        start,
        size,
    })
}

/// Parse a 512 byte boot sector: check the magic, decode four entries.
/// Returns the entries in slot order (empty slots included: slot 3 of an
/// empty disk is unused, not corrupt).
pub fn parse_sector(sector: &[u8]) -> Result<[PartitionEntry; ENTRY_COUNT], DiskError> {
    if sector.len() != SECTOR_SIZE {
        return Err(DiskError::InvalidArgument);
    }
    let magic = u16::from_le_bytes([sector[MAGIC_OFFSET], sector[MAGIC_OFFSET + 1]]);
    if magic != MAGIC {
        return Err(DiskError::InvalidArgument);
    }
    let mut entries = [PartitionEntry {
        active: false,
        partition_type: 0,
        start: 0,
        size: 0,
    }; ENTRY_COUNT];
    for (index, entry) in entries.iter_mut().enumerate() {
        let start = TABLE_OFFSET + index * ENTRY_SIZE;
        *entry = parse_entry(&sector[start..start + ENTRY_SIZE])?;
    }
    Ok(entries)
}

/// Read only partition table behind one trait: the executor's live disk
/// and the test memory image answer the same queries.
pub trait PartitionTable {
    /// Decode slot `index` (0 to 3), or `None` for an empty slot.
    fn entry(&self, index: usize) -> Option<PartitionEntry>;
    /// How many slots the table holds (always 4 for a master boot record,
    /// but the trait does not assume it).
    fn slot_count(&self) -> usize;
}

/// A table with no partitions: every slot misses. The honest starting
/// point until block device access lands.
pub struct EmptyPartitionTable;

impl PartitionTable for EmptyPartitionTable {
    fn entry(&self, _index: usize) -> Option<PartitionEntry> {
        None
    }

    fn slot_count(&self) -> usize {
        0
    }
}

/// A table over a decoded four entry array.
pub struct SlicePartitionTable {
    /// Entries in slot order (empty slots hold type zero).
    pub entries: [PartitionEntry; ENTRY_COUNT],
}

impl PartitionTable for SlicePartitionTable {
    fn entry(&self, index: usize) -> Option<PartitionEntry> {
        let entry = *self.entries.get(index)?;
        if entry.empty() {
            None
        } else {
            Some(entry)
        }
    }

    fn slot_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sector() -> [u8; SECTOR_SIZE] {
        let mut sector = [0u8; SECTOR_SIZE];
        // Slot 0: bootable Minix from sector 63, one million sectors.
        sector[446] = 0x80;
        sector[446 + 4] = 0x81;
        sector[446 + 8..446 + 12].copy_from_slice(&63u32.to_le_bytes());
        sector[446 + 12..446 + 16].copy_from_slice(&1_000_000u32.to_le_bytes());
        // Slot 1: Linux native, right after.
        sector[462 + 4] = 0x83;
        sector[462 + 8..462 + 12].copy_from_slice(&1_000_063u32.to_le_bytes());
        sector[462 + 12..462 + 16].copy_from_slice(&2_000_000u32.to_le_bytes());
        sector[510] = 0x55;
        sector[511] = 0xAA;
        sector
    }

    #[test]
    fn test_two_partitions() {
        let entries = parse_sector(&sector()).unwrap();
        assert!(entries[0].active);
        assert_eq!(entries[0].partition_type, 0x81);
        assert_eq!((entries[0].start, entries[0].size), (63, 1_000_000));
        assert!(!entries[1].active);
        assert_eq!(entries[1].partition_type, 0x83);
        assert!(entries[2].empty());
        assert!(entries[3].empty());
    }

    #[test]
    fn test_type_names() {
        assert_eq!(type_name(0x81), Some("Minix"));
        assert_eq!(type_name(0x83), Some("Linux native"));
        assert_eq!(type_name(0xA5), Some("386BSD"));
        assert_eq!(type_name(0x42), None);
    }

    #[test]
    fn test_last_sector_math() {
        let entries = parse_sector(&sector()).unwrap();
        assert_eq!(entries[0].last_sector(), Some(1_000_062));
        assert_eq!(entries[2].last_sector(), None);
    }

    #[test]
    fn test_bad_magic_rejected() {
        let mut bad = sector();
        bad[511] = 0x00;
        assert_eq!(parse_sector(&bad).map(|_| ()), Err(DiskError::InvalidArgument));
    }

    #[test]
    fn test_bad_flag_rejected() {
        let mut bad = sector();
        bad[446] = 0x01;
        assert_eq!(parse_sector(&bad).map(|_| ()), Err(DiskError::InvalidArgument));
    }

    #[test]
    fn test_table_trait() {
        let entries = parse_sector(&sector()).unwrap();
        let table = SlicePartitionTable { entries };
        assert_eq!(table.slot_count(), 4);
        assert_eq!(table.entry(0).unwrap().start, 63);
        assert_eq!(table.entry(2), None);
        assert_eq!(EmptyPartitionTable.entry(0), None);
        assert_eq!(EmptyPartitionTable.slot_count(), 0);
    }
}
