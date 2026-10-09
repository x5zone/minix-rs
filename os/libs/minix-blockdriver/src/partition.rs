//! MBR primary-partition table parser (U-6 阶段二 enabler).
//!
//! C correspondence: `drvlib.c` `partition()` reads the four-entry primary
//! table at `PART_TABLE_OFF` and rejects a boot sector lacking the `0x55AA`
//! signature. Entry layout is `struct part_entry`
//! (`minix3/minix/include/arch/i386/include/partition.h:7-18`):
//! `bootind` byte 0, `sysind` byte 4, `lowsec` (start LBA) bytes 8-11,
//! `size` (sectors) bytes 12-15, 16 bytes per entry.
//!
//! This is the byte-decode half of partitioning; the sector read that feeds it
//! comes from each driver's block transfer. Keeping the decode here (the shared
//! framework, C's `libblockdriver`) means every disk driver — `at_wini`,
//! `ahci`, ... — parses identically instead of each re-deriving the layout.

/// One decoded primary-partition entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MbrEntry {
    /// The boot-indicator active bit is set. C: `bootind == ACTIVE_FLAG`.
    pub bootable: bool,
    /// System indicator (the partition type byte). C: `sysind`.
    pub sysind: u8,
    /// Logical first sector (LBA). C: `lowsec`.
    pub start_lba: u32,
    /// Size in sectors. C: `size`.
    pub sectors: u32,
}

/// Number of primary partition entries. C: `NR_PARTITIONS 4` — partition.h:21.
pub const NR_PARTITIONS: usize = 4;
/// Offset of the partition table within a 512-byte boot sector.
/// C: `PART_TABLE_OFF 0x1BE` — partition.h:22.
pub const PART_TABLE_OFF: usize = 0x1BE;
/// Boot-indicator value marking the active partition. C: `ACTIVE_FLAG 0x80` —
/// partition.h:20.
pub const ACTIVE_FLAG: u8 = 0x80;
/// Empty system-indicator (unused table slot). C: `NO_PART 0x00` — partition.h:25.
pub const NO_PART: u8 = 0x00;
/// MBR boot-signature byte at offset 510. C: `tmp_buf[510] != 0x55` — drvlib.c:209.
pub const SIGNATURE_LO: u8 = 0x55;
/// MBR boot-signature byte at offset 511. C: `tmp_buf[511] != 0xAA` — drvlib.c:209.
pub const SIGNATURE_HI: u8 = 0xAA;

/// Decode the four primary-partition entries from a 512-byte MBR sector.
///
/// Returns `None` when the `0x55AA` boot signature is absent (C treats that as
/// an "invalid partition table" and reports no partitions). Each slot is
/// `Some(entry)` unless its `sysind` is `NO_PART` or its size is zero, in which
/// case it is `None` (an unused table entry).
pub fn parse_primary_table(mbr: &[u8; 512]) -> Option<[Option<MbrEntry>; NR_PARTITIONS]> {
    if mbr[510] != SIGNATURE_LO || mbr[511] != SIGNATURE_HI {
        return None;
    }
    let mut out = [None; NR_PARTITIONS];
    for (i, slot) in out.iter_mut().enumerate() {
        let e = PART_TABLE_OFF + i * 16;
        let sysind = mbr[e + 4];
        let start_lba = u32::from_le_bytes([mbr[e + 8], mbr[e + 9], mbr[e + 10], mbr[e + 11]]);
        let sectors = u32::from_le_bytes([mbr[e + 12], mbr[e + 13], mbr[e + 14], mbr[e + 15]]);
        if sysind != NO_PART && sectors != 0 {
            *slot = Some(MbrEntry {
                bootable: mbr[e] & ACTIVE_FLAG != 0,
                sysind,
                start_lba,
                sectors,
            });
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minix partition type byte. C: `MINIX_PART 0x81`.
    const MINIX_PART: u8 = 0x81;

    /// A zeroed boot sector with a valid trailing signature.
    fn blank_mbr() -> [u8; 512] {
        let mut m = [0u8; 512];
        m[510] = SIGNATURE_LO;
        m[511] = SIGNATURE_HI;
        m
    }

    /// Write one primary-partition entry into `slot` (0..4) of `m`.
    fn put_entry(
        m: &mut [u8; 512],
        slot: usize,
        bootable: bool,
        sysind: u8,
        start: u32,
        size: u32,
    ) {
        let e = PART_TABLE_OFF + slot * 16;
        m[e] = if bootable { ACTIVE_FLAG } else { 0x00 };
        m[e + 4] = sysind;
        m[e + 8..e + 12].copy_from_slice(&start.to_le_bytes());
        m[e + 12..e + 16].copy_from_slice(&size.to_le_bytes());
    }

    #[test]
    fn decodes_a_minix_primary_partition() {
        let mut m = blank_mbr();
        put_entry(&mut m, 0, true, MINIX_PART, 2048, 100_000);
        let table = parse_primary_table(&m).unwrap();
        assert_eq!(
            table[0],
            Some(MbrEntry {
                bootable: true,
                sysind: MINIX_PART,
                start_lba: 2048,
                sectors: 100_000
            })
        );
        // The other slots stay empty.
        assert!(table[1].is_none() && table[2].is_none() && table[3].is_none());
    }

    #[test]
    fn zero_signature_rejects_the_table() {
        // C: tmp_buf[510]/[511] mismatch → "invalid partition table" → no parts.
        let m = [0u8; 512];
        assert!(parse_primary_table(&m).is_none());
    }

    #[test]
    fn skips_unused_and_zero_length_entries() {
        let mut m = blank_mbr();
        put_entry(&mut m, 1, false, MINIX_PART, 4096, 0); // size 0 → unused
        put_entry(&mut m, 2, false, NO_PART, 0, 500); // sysind 0 → unused
        put_entry(&mut m, 3, false, MINIX_PART, 8192, 1000); // real
        let table = parse_primary_table(&m).unwrap();
        assert!(table[0].is_none());
        assert!(table[1].is_none(), "zero sector count is an empty entry");
        assert!(
            table[2].is_none(),
            "NO_PART system indicator is an empty entry"
        );
        assert_eq!(
            table[3].map(|e| (e.start_lba, e.sectors)),
            Some((8192, 1000))
        );
        assert!(!table[3].unwrap().bootable);
    }

    #[test]
    fn little_endian_field_decoding() {
        // lowsec/size are 32-bit little-endian on the medium.
        let mut m = blank_mbr();
        put_entry(&mut m, 0, false, MINIX_PART, 0x1122_3344, 0xAABB_CCDD);
        let e = parse_primary_table(&m).unwrap()[0].unwrap();
        assert_eq!(e.start_lba, 0x1122_3344);
        assert_eq!(e.sectors, 0xAABB_CCDD);
    }
}
