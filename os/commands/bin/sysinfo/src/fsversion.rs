//! File system version probe.
//!
//! Ground truth: `minix3/minix/lib/libc/gen/fsversion.c`. It opens a device,
//! skips the boot block (`SUPER_BLOCK_BYTES`, 1024 near `minix3/minix/fs/mfs/const.h:50`),
//! reads one super-block-sized buffer, and matches 16-bit magics: MFS v1/v2/v3
//! at `MAGIC_OFFSET_MFS` (0x18), ext2 at `MAGIC_OFFSET_EXT` (0x38), and — only
//! when none of those hit — seeks to the ISO 9660 primary volume descriptor
//! region (`MAGIC_OFFSET_ISO9660`, 0x8000) looking for the `CD001` tag at byte
//! offset 1. The magic comparison is `memcmp(super + offset, &magic, 2)` over an
//! `unsigned short` local, so on the (little endian) host it reads the two bytes
//! of the on-disk value low-first; this module reproduces that with
//! [`core::mem`]-free explicit low/high ordering (see [`read_le16`]).
//!
//! File opening and seeking stay with the execution layer; this module owns the
//! byte-pattern classification so it is host-testable against inline golden
//! buffers.

use crate::SysinfoError;

/// Boot-block size skipped before the super block (C: `SUPER_BLOCK_BYTES`, 1024).
pub const SUPER_BLOCK_BYTES: usize = 1024;

/// Magic offset inside the super block for the Minix file system versions
/// (C: `MAGIC_OFFSET_MFS`, 0x18).
const MAGIC_OFFSET_MFS: usize = 0x18;

/// Magic offset inside the super block for ext2 (C: `MAGIC_OFFSET_EXT`, 0x38).
const MAGIC_OFFSET_EXT: usize = 0x38;

/// Region offset of the ISO 9660 primary volume descriptor (C:
/// `MAGIC_OFFSET_ISO9660`, 0x8000).
pub const ISO9660_VOLUME_OFFSET: i64 = 0x8000;

/// MFS v1 magic (`minix3/minix/fs/mfs/const.h:22`, `SUPER_MAGIC`).
const SUPER_MAGIC_V1: u16 = 0x137f;
/// MFS version 2 magic (`minix3/minix/fs/mfs/const.h:24`).
const SUPER_MAGIC_V2: u16 = 0x2468;
/// MFS version 3 magic (`minix3/minix/fs/mfs/const.h:26`).
const SUPER_MAGIC_V3: u16 = 0x4d5a;
/// ext2 magic (`minix3/minix/fs/ext2/const.h:16`; fsversion's `MAGIC_VALUE_EXT2`).
const MAGIC_VALUE_EXT2: u16 = 0xef53;

/// The file system kind fsversion distinguishes. `Unknown` is what C folds into
/// a `-1` return (no magic matched); `Iso9660` matches only on the second read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsVersion {
    /// Minix file system version 1.
    Mfs1,
    /// Minix file system version 2.
    Mfs2,
    /// Minix file system version 3.
    Mfs3,
    /// ext2 file system.
    Ext2,
    /// ISO 9660 (CD-ROM) primary volume descriptor.
    Iso9660,
}

/// Read a little-endian 16-bit value at `offset` (low byte first), mirroring the
/// `memcmp(super + offset, &magic, 2)` byte order on a little-endian host.
/// Returns `None` when the buffer is too short to cover the two bytes.
fn read_le16(block: &[u8], offset: usize) -> Option<u16> {
    let hi = offset.checked_add(1)?;
    let byte_hi = *block.get(hi)?;
    let byte_lo = *block.get(offset)?;
    Some(byte_lo as u16 | ((byte_hi as u16) << 8))
}

/// Classify a super-block buffer (MFS v1/v2/v3, then ext2) — the first pass of
/// C `fsversion`, before the ISO 9660 fallback. `None` means no magic matched.
pub fn classify_super(super_block: &[u8]) -> Option<FsVersion> {
    let mfs = read_le16(super_block, MAGIC_OFFSET_MFS);
    if mfs == Some(SUPER_MAGIC_V1) {
        return Some(FsVersion::Mfs1);
    } else if mfs == Some(SUPER_MAGIC_V2) {
        return Some(FsVersion::Mfs2);
    } else if mfs == Some(SUPER_MAGIC_V3) {
        return Some(FsVersion::Mfs3);
    }
    if read_le16(super_block, MAGIC_OFFSET_EXT) == Some(MAGIC_VALUE_EXT2) {
        return Some(FsVersion::Ext2);
    }
    None
}

/// True when `descriptor` is an ISO 9660 primary volume descriptor: the five
/// bytes `CD001` sit at byte offset 1 (C `memcmp(super + 1, "CD001", 5)`).
pub fn is_iso9660_descriptor(descriptor: &[u8]) -> bool {
    descriptor.get(1..6) == Some(&b"CD001"[..])
}

/// The display tag C `printroot`'s `done()` writes before ` rw` for a probe
/// result: MFS1/2/3 become `1`/`2`/`3`, ext2 becomes `ext2`, and everything the
/// switch does not name explicitly (`CD001` and no match) falls through to `0`
/// (`printroot.c` `default: write "0 rw\n"`).
pub fn version_tag(version: Option<FsVersion>) -> &'static str {
    match version {
        Some(FsVersion::Mfs1) => "1",
        Some(FsVersion::Mfs2) => "2",
        Some(FsVersion::Mfs3) => "3",
        Some(FsVersion::Ext2) => "ext2",
        Some(FsVersion::Iso9660) | None => "0",
    }
}

/// Build a super-block buffer with the MFS magic written little-endian at
/// `MAGIC_OFFSET_MFS`. Helper for tests and (with the real device read) the
/// execution layer keeps one byte-order rule in this module.
pub fn stamp_mfs_magic(buffer: &mut [u8], magic: u16) -> Result<(), SysinfoError> {
    if buffer.len() < MAGIC_OFFSET_MFS + 2 {
        return Err(SysinfoError::InvalidArgument);
    }
    buffer[MAGIC_OFFSET_MFS] = (magic & 0xff) as u8;
    buffer[MAGIC_OFFSET_MFS + 1] = (magic >> 8) as u8;
    Ok(())
}

/// Build a super-block buffer with the ext2 magic written little-endian at
/// `MAGIC_OFFSET_EXT`.
pub fn stamp_ext2_magic(buffer: &mut [u8]) -> Result<(), SysinfoError> {
    if buffer.len() < MAGIC_OFFSET_EXT + 2 {
        return Err(SysinfoError::InvalidArgument);
    }
    buffer[MAGIC_OFFSET_EXT] = (MAGIC_VALUE_EXT2 & 0xff) as u8;
    buffer[MAGIC_OFFSET_EXT + 1] = (MAGIC_VALUE_EXT2 >> 8) as u8;
    Ok(())
}

/// The MFS magic constants, exposed so the execution layer and tests stamp the
/// exact on-disk values without re-spelling them (single source of truth).
pub const MFS_MAGIC_V1: u16 = SUPER_MAGIC_V1;
/// See the family doc above.
pub const MFS_MAGIC_V2: u16 = SUPER_MAGIC_V2;
/// See the family doc above.
pub const MFS_MAGIC_V3: u16 = SUPER_MAGIC_V3;

#[cfg(test)]
mod tests {
    use super::*;

    // A super block long enough to hold both magic offsets.
    fn super_buf() -> [u8; SUPER_BLOCK_BYTES] {
        [0u8; SUPER_BLOCK_BYTES]
    }

    #[test]
    fn test_classify_mfs_versions() {
        let mut b = super_buf();
        stamp_mfs_magic(&mut b, SUPER_MAGIC_V1).unwrap();
        assert_eq!(classify_super(&b), Some(FsVersion::Mfs1));

        let mut b = super_buf();
        stamp_mfs_magic(&mut b, SUPER_MAGIC_V2).unwrap();
        assert_eq!(classify_super(&b), Some(FsVersion::Mfs2));

        let mut b = super_buf();
        stamp_mfs_magic(&mut b, SUPER_MAGIC_V3).unwrap();
        assert_eq!(classify_super(&b), Some(FsVersion::Mfs3));
    }

    #[test]
    fn test_classify_ext2() {
        let mut b = super_buf();
        stamp_ext2_magic(&mut b).unwrap();
        assert_eq!(classify_super(&b), Some(FsVersion::Ext2));
    }

    #[test]
    fn test_mfs_takes_precedence_over_ext2() {
        // C checks MFS first with an else-if chain, then ext2; a buffer that
        // carries both must report MFS (the comment: "a valid MFS may look like
        // EXT but not vice versa").
        let mut b = super_buf();
        stamp_mfs_magic(&mut b, SUPER_MAGIC_V3).unwrap();
        stamp_ext2_magic(&mut b).unwrap();
        assert_eq!(classify_super(&b), Some(FsVersion::Mfs3));
    }

    #[test]
    fn test_no_magic_is_none() {
        let b = super_buf();
        assert_eq!(classify_super(&b), None);
    }

    #[test]
    fn test_byte_order_is_little_endian() {
        // 0x137f must appear as bytes [0x7f, 0x13] at the MFS offset, matching
        // the host `unsigned short` memcmp in fsversion.c.
        let mut b = super_buf();
        b[MAGIC_OFFSET_MFS] = 0x7f;
        b[MAGIC_OFFSET_MFS + 1] = 0x13;
        assert_eq!(classify_super(&b), Some(FsVersion::Mfs1));
    }

    #[test]
    fn test_iso9660_descriptor() {
        let mut d = [0u8; 8];
        d[1..6].copy_from_slice(b"CD001");
        assert!(is_iso9660_descriptor(&d));

        let mut wrong = [0u8; 8];
        wrong[0..5].copy_from_slice(b"CD001"); // at offset 0, not 1
        assert!(!is_iso9660_descriptor(&wrong));
        assert!(!is_iso9660_descriptor(&[0u8; 4])); // too short
    }

    #[test]
    fn test_version_tag_mapping() {
        assert_eq!(version_tag(Some(FsVersion::Mfs1)), "1");
        assert_eq!(version_tag(Some(FsVersion::Mfs2)), "2");
        assert_eq!(version_tag(Some(FsVersion::Mfs3)), "3");
        assert_eq!(version_tag(Some(FsVersion::Ext2)), "ext2");
        // C's switch has no ISO case: it falls to default `0 rw`, same as None.
        assert_eq!(version_tag(Some(FsVersion::Iso9660)), "0");
        assert_eq!(version_tag(None), "0");
    }
}
