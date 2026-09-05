//! Directory records and extents: the on-disc index plus the run list
//! (`inode.c` record handling, `utility.c` extent helpers).
//!
//! A directory record packs the location, length, flags, name, and (for
//! directories) the system-use tail that Rock Ridge lives in. Numbers
//! come in both byte orders; this server reads the little-endian twin.
//! A file's bytes form a run of extents (location plus length); block
//! `n` of the file is the run holding the `n`th block, so multi-extent
//! files (interleaved mode) walk the list instead of multiplying once.

use minix_types::{EINVAL, EIO, Errno};

/// Record length byte offset (zero) and minimum sane length.
pub const RECORD_LENGTH_OFFSET: usize = 0;
/// Extended-attribute length byte offset (one).
pub const EXT_ATTR_OFFSET: usize = 1;
/// Location field offset, little-endian twin first (two).
pub const LOCATION_OFFSET: usize = 2;
/// Data-length field offset, little-endian twin first (ten).
pub const LENGTH_OFFSET: usize = 10;
/// Flags byte offset (twenty-five): bit one marks a directory, bit zero
/// marks a hidden entry the listing skips.
pub const FLAGS_OFFSET: usize = 25;
/// Directory flag bit (`inode.c` directory test).
pub const FLAG_DIRECTORY: u8 = 0x02;
/// Name-length byte offset (thirty-two).
pub const NAME_LENGTH_OFFSET: usize = 32;
/// Name bytes offset (thirty-three).
pub const NAME_OFFSET: usize = 33;
/// Smallest record that still holds a name length (`check_dir_record`
/// rejects anything shorter than the name header).
pub const MIN_RECORD_BYTES: usize = 33;

/// Why record handling failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordError {
    /// Bad record (short, zero length, overrun, bad flags use).
    Invalid,
    /// Storage failure.
    Io,
}

impl RecordError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// One decoded directory record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRecord {
    /// Total record length in bytes.
    pub record_length: usize,
    /// Extended-attribute record length in blocks.
    pub ext_attributes: u8,
    /// First sector of the file data.
    pub location: u32,
    /// Data length in bytes.
    pub data_length: u32,
    /// Raw flags byte.
    pub flags: u8,
    /// File name bytes.
    pub name: alloc::vec::Vec<u8>,
}

/// Decode one record at the front of `image`. Zero lengths and overruns
/// refuse: the directory walk advances by the length byte, so a zero
/// would stall it forever and an overrun would read past the block.
pub fn decode(image: &[u8]) -> Result<(DirectoryRecord, usize), RecordError> {
    if image.len() < MIN_RECORD_BYTES {
        return Err(RecordError::Invalid);
    }
    let record_length = image[RECORD_LENGTH_OFFSET] as usize;
    if record_length == 0 || record_length > image.len() {
        return Err(RecordError::Invalid);
    }
    let body = &image[..record_length];
    let name_length = body[NAME_LENGTH_OFFSET] as usize;
    if NAME_OFFSET + name_length > record_length {
        return Err(RecordError::Invalid);
    }
    let location = u32::from_le_bytes([body[2], body[3], body[4], body[5]]);
    let data_length = u32::from_le_bytes([body[10], body[11], body[12], body[13]]);
    Ok((
        DirectoryRecord {
            record_length,
            ext_attributes: body[EXT_ATTR_OFFSET],
            location,
            data_length,
            flags: body[FLAGS_OFFSET],
            name: body[NAME_OFFSET..NAME_OFFSET + name_length].to_vec(),
        },
        record_length,
    ))
}

/// Whether a record describes a directory.
pub const fn is_directory(flags: u8) -> bool {
    flags & FLAG_DIRECTORY != 0
}

/// One file run: a sector location plus a block count
/// (`struct dir_extent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    /// First sector of the run.
    pub location: u64,
    /// Run length in blocks.
    pub length: u64,
}

/// Map a file-relative block to its absolute sector
/// (`get_extent_absolute_block_id`, `utility.c`): walk runs until the
/// block falls inside one; past every run reports absent (zero), never
/// a wrapped address.
pub fn map_block(runs: &[Extent], file_block: u64) -> u64 {
    let mut base = 0u64;
    for run in runs {
        if file_block < base + run.length {
            return run.location + (file_block - base);
        }
        base += run.length;
    }
    0
}

/// Root extent from a root record (`super.c:54-61`): the attribute
/// length shifts the start forward, and a partial tail block still
/// counts as one.
pub fn root_extent(location: u64, ext_attributes: u64, data_length: u64, block_size: u64) -> Extent {
    if block_size == 0 {
        return Extent { location, length: 0 };
    }
    Extent {
        location: location + ext_attributes,
        length: data_length.div_ceil(block_size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn record_bytes(name: &[u8], location: u32, length: u32, flags: u8) -> alloc::vec::Vec<u8> {
        let total = 33 + name.len();
        let mut out = vec![0u8; total];
        out[0] = total as u8;
        out[2..6].copy_from_slice(&location.to_le_bytes());
        out[6..10].copy_from_slice(&location.to_be_bytes());
        out[10..14].copy_from_slice(&length.to_le_bytes());
        out[14..18].copy_from_slice(&length.to_be_bytes());
        out[25] = flags;
        out[32] = name.len() as u8;
        out[33..].copy_from_slice(name);
        out
    }

    #[test]
    fn test_decode_round_trip() {
        let bytes = record_bytes(b"HELLO", 100, 2048, FLAG_DIRECTORY);
        let (record, length) = decode(&bytes).unwrap();
        assert_eq!(record.location, 100);
        assert_eq!(record.data_length, 2048);
        assert!(is_directory(record.flags));
        assert_eq!(record.name, b"HELLO");
        assert_eq!(length, bytes.len());
    }

    #[test]
    fn test_decode_rejects() {
        assert_eq!(decode(&[]).unwrap_err(), RecordError::Invalid);
        // Zero length would stall the walk.
        assert_eq!(decode(&[0u8; 40]).unwrap_err(), RecordError::Invalid);
        // Name overruns the record.
        let mut bad = record_bytes(b"AB", 0, 0, 0);
        bad[32] = 30;
        assert_eq!(decode(&bad).unwrap_err(), RecordError::Invalid);
    }

    #[test]
    fn test_map_block_runs() {
        let runs = [Extent { location: 100, length: 4 }, Extent { location: 200, length: 2 }];
        assert_eq!(map_block(&runs, 0), 100);
        assert_eq!(map_block(&runs, 3), 103);
        assert_eq!(map_block(&runs, 4), 200);
        assert_eq!(map_block(&runs, 6), 0);
        assert_eq!(map_block(&[], 0), 0);
    }

    #[test]
    fn test_root_extent_tail_counts() {
        assert_eq!(
            root_extent(100, 0, 2048, 2048),
            Extent { location: 100, length: 1 }
        );
        assert_eq!(
            root_extent(100, 1, 2049, 2048),
            Extent { location: 101, length: 2 }
        );
    }
}
