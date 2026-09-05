//! Primary volume descriptor: discovery scan and consistency checks
//! (`super.c`).
//!
//! An ISO9660 disc carries a sequence of volume descriptors starting at
//! byte thirty-two thousand seven hundred sixty-eight, each in its own
//! two-thousand-forty-eight-byte sector. The server walks at most twenty
//! sectors: a primary descriptor is parsed when met (replacing any
//! earlier one), and the walk stops at the set terminator. Success needs
//! both a terminator and a valid primary (`super.c:117-120`).
//!
//! A primary counts as valid with the five-byte signature `CD001`,
//! version one, and a little-endian block size of at least two thousand
//! forty-eight (`super.c:41-45`). Both-endian numbers are read
//! little-endian first: the big-endian twin exists for foreign readers,
//! and the little-endian assert at startup (`main.c` of the sibling
//! disk server) makes the choice total.

use minix_types::{EINVAL, EIO, Errno};

/// First descriptor byte offset (`ISO9660_SUPER_BLOCK_POSITION`, 32768:
/// sixteen sectors of two thousand forty-eight bytes).
pub const DESCRIPTOR_START: u64 = 32768;
/// One descriptor per sector (`ISO9660_MIN_BLOCK_SIZE`, 2048 bytes).
pub const SECTOR_BYTES: usize = 2048;
/// Maximum sectors walked (`MAX_ATTEMPTS`, twenty).
pub const MAX_SECTORS: usize = 20;
/// Primary descriptor tag (`VD_PRIMARY`, one).
pub const TAG_PRIMARY: u8 = 1;
/// Set-terminator tag (`VD_SET_TERM`, two hundred fifty-five).
pub const TAG_TERMINATOR: u8 = 255;
/// Signature (`CD001`, five bytes).
pub const SIGNATURE: &[u8; 5] = b"CD001";
/// Signature field width.
pub const SIGNATURE_BYTES: usize = 5;
/// Descriptor version this server reads (one).
pub const DESCRIPTOR_VERSION: u8 = 1;
/// Minimum block size (two thousand forty-eight bytes).
pub const MIN_BLOCK_SIZE: u32 = 2048;

/// Why volume handling failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeError {
    /// Bad descriptor (signature, version, or block size).
    Invalid,
    /// No usable primary found in the scan.
    NotFound,
    /// Storage failure.
    Io,
}

impl VolumeError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::NotFound => Errno::from_i32(EINVAL),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// Validated primary descriptor facts the mounter needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimaryFacts {
    /// Logical block size in bytes.
    pub block_size: u32,
    /// Total sectors on the disc.
    pub sector_count: u32,
    /// Byte offset of the root directory record inside the primary.
    pub root_record_offset: usize,
    /// Byte length of the root directory record.
    pub root_record_length: usize,
}

/// Check the three consistency gates (`create_vol_pri_desc`,
/// `super.c:41-45`).
pub const fn check_consistency(signature: &[u8; SIGNATURE_BYTES], version: u8, block_size: u32) -> Result<(), VolumeError> {
    if signature[0] != b'C'
        || signature[1] != b'D'
        || signature[2] != b'0'
        || signature[3] != b'0'
        || signature[4] != b'1'
    {
        return Err(VolumeError::Invalid);
    }
    if version != DESCRIPTOR_VERSION {
        return Err(VolumeError::Invalid);
    }
    if block_size < MIN_BLOCK_SIZE {
        return Err(VolumeError::Invalid);
    }
    Ok(())
}

/// Scan descriptor tags in order (`read_vds`, `super.c:87-120`): parse
/// primaries when met (each replacing the last), stop at the first
/// terminator, and succeed only with both a terminator and a valid
/// primary. Tags past the sector budget never run.
pub fn scan_tags(tags: &[u8], valid_primary: &[bool]) -> Result<usize, VolumeError> {
    let mut primary_at: Option<usize> = None;
    let mut terminated = false;
    for (position, tag) in tags.iter().enumerate().take(MAX_SECTORS) {
        if *tag == TAG_PRIMARY && valid_primary.get(position).copied().unwrap_or(false) {
            primary_at = Some(position);
        }
        if *tag == TAG_TERMINATOR {
            terminated = true;
            break;
        }
    }
    match (primary_at, terminated) {
        (Some(at), true) => Ok(at),
        _ => Err(VolumeError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistency_gates() {
        assert!(check_consistency(b"CD001", 1, 2048).is_ok());
        assert_eq!(
            check_consistency(b"CD002", 1, 2048).unwrap_err(),
            VolumeError::Invalid
        );
        assert_eq!(
            check_consistency(b"CD001", 2, 2048).unwrap_err(),
            VolumeError::Invalid
        );
        assert_eq!(
            check_consistency(b"CD001", 1, 1024).unwrap_err(),
            VolumeError::Invalid
        );
    }

    #[test]
    fn test_scan_needs_both() {
        // Primary then terminator: primary sector index reported.
        assert_eq!(scan_tags(&[TAG_PRIMARY, TAG_TERMINATOR], &[true, false]).unwrap(), 0);
        // No terminator: not found, however valid the primary.
        assert_eq!(
            scan_tags(&[TAG_PRIMARY], &[true]).unwrap_err(),
            VolumeError::NotFound
        );
        // Terminator without primary: not found.
        assert_eq!(
            scan_tags(&[TAG_TERMINATOR], &[false]).unwrap_err(),
            VolumeError::NotFound
        );
        // Later primary replaces the earlier one.
        assert_eq!(
            scan_tags(&[TAG_PRIMARY, TAG_PRIMARY, TAG_TERMINATOR], &[true, true, false]).unwrap(),
            1
        );
        // Invalid primary does not count.
        assert_eq!(
            scan_tags(&[TAG_PRIMARY, TAG_TERMINATOR], &[false, false]).unwrap_err(),
            VolumeError::NotFound
        );
    }

    #[test]
    fn test_layout_constants() {
        assert_eq!(DESCRIPTOR_START, 32768);
        assert_eq!(SECTOR_BYTES, 2048);
        assert_eq!(MAX_SECTORS, 20);
        assert_eq!(TAG_TERMINATOR, 255);
    }
}
