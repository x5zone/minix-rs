//! Optical volume descriptor recognition (ISO 9660 primary volume).
//!
//! Ground truth: the `CD001` identifier (`minix3/minix/commands/isoread/isoread.c:41`).
//! A primary volume descriptor is one 2048 byte sector: type byte 1 at
//! offset 0, the five byte identifier at offsets 1 to 5, version 1 at
//! offset 6, the 32 byte volume label at offsets 40 to 71, and the logical
//! block size at offsets 128 to 131 (little endian) mirrored big endian at
//! 132 to 135. Numbers throughout the format repeat in both endiannesses
//! ("both byte orders"); this module reads the little endian copy and
//! cross checks the big endian copy, rejecting mismatches (a torn or
//! foreign sector must not parse as a volume).

use crate::ImageError;

/// Volume descriptor sector size in bytes.
pub const SECTOR_SIZE: usize = 2048;
/// Primary volume type byte.
pub const PRIMARY_TYPE: u8 = 1;
/// Expected identifier bytes.
pub const IDENTIFIER: &[u8; 5] = b"CD001";
/// Expected version byte.
pub const VERSION: u8 = 1;
/// Volume label range within the sector.
pub const LABEL_RANGE: (usize, usize) = (40, 72);
/// Logical block size range, little endian (big endian mirror follows).
pub const BLOCK_SIZE_RANGE: (usize, usize) = (128, 132);

/// One recognised primary volume: label plus block size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimaryVolume<'a> {
    /// Volume label, trailing blanks trimmed.
    pub label: &'a str,
    /// Logical block size in bytes (usually 2048).
    pub block_size: u32,
}

/// Recognise a primary volume descriptor sector: type, identifier,
/// version, and endian mirrored block size must all agree.
pub fn parse_primary(sector: &[u8]) -> Result<PrimaryVolume<'_>, ImageError> {
    if sector.len() != SECTOR_SIZE {
        return Err(ImageError::InvalidArgument);
    }
    if sector[0] != PRIMARY_TYPE {
        return Err(ImageError::InvalidArgument);
    }
    if sector[1..6] != *IDENTIFIER {
        return Err(ImageError::InvalidArgument);
    }
    if sector[6] != VERSION {
        return Err(ImageError::InvalidArgument);
    }
    let little = u32::from_le_bytes([
        sector[BLOCK_SIZE_RANGE.0],
        sector[BLOCK_SIZE_RANGE.0 + 1],
        sector[BLOCK_SIZE_RANGE.0 + 2],
        sector[BLOCK_SIZE_RANGE.0 + 3],
    ]);
    let big = u32::from_be_bytes([
        sector[BLOCK_SIZE_RANGE.1],
        sector[BLOCK_SIZE_RANGE.1 + 1],
        sector[BLOCK_SIZE_RANGE.1 + 2],
        sector[BLOCK_SIZE_RANGE.1 + 3],
    ]);
    if little != big || little == 0 {
        return Err(ImageError::InvalidArgument);
    }
    let label = core::str::from_utf8(&sector[LABEL_RANGE.0..LABEL_RANGE.1])
        .map_err(|_| ImageError::InvalidArgument)?;
    Ok(PrimaryVolume {
        label: label.trim_end(),
        block_size: little,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sector() -> [u8; SECTOR_SIZE] {
        let mut sector = [0u8; SECTOR_SIZE];
        sector[0] = 1;
        sector[1..6].copy_from_slice(b"CD001");
        sector[6] = 1;
        let label = b"MINIX3-INSTALL-DISC";
        sector[40..72].fill(b' ');
        sector[40..40 + label.len()].copy_from_slice(label);
        sector[128..132].copy_from_slice(&2048u32.to_le_bytes());
        sector[132..136].copy_from_slice(&2048u32.to_be_bytes());
        sector
    }

    #[test]
    fn test_primary_recognised() {
        let image = sector();
        let volume = parse_primary(&image).unwrap();
        assert_eq!(volume.label, "MINIX3-INSTALL-DISC");
        assert_eq!(volume.block_size, 2048);
    }

    #[test]
    fn test_wrong_type_rejected() {
        let image = sector();
        let mut bad = image;
        bad[0] = 2;
        assert_eq!(parse_primary(&bad).map(|_| ()), Err(ImageError::InvalidArgument));
    }

    #[test]
    fn test_bad_identifier_rejected() {
        let image = sector();
        let mut bad = image;
        bad[1] = b'X';
        assert_eq!(parse_primary(&bad).map(|_| ()), Err(ImageError::InvalidArgument));
    }

    #[test]
    fn test_endian_mismatch_rejected() {
        let image = sector();
        let mut bad = image;
        bad[132] = 0x08;
        assert_eq!(parse_primary(&bad).map(|_| ()), Err(ImageError::InvalidArgument));
    }

    #[test]
    fn test_short_sector_rejected() {
        assert_eq!(parse_primary(&[0u8; 100]).map(|_| ()), Err(ImageError::InvalidArgument));
    }
}
