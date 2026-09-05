//! Variable-length directory entries: encode, decode, and walk
//! (`path.c` search side, `type.h` record layout).
//!
//! Unlike the Minix fixed sixty-four-byte record, each second-extended
//! entry carries its own total length (`rec_len`), so entries pack back
//! to back and deletion merges a record into its predecessor by growing
//! the predecessor's length. Names run to two hundred fifty-five bytes,
//! four-byte aligned, with the file type stored alongside (the Minix
//! server pays an inode lookup per listing for the same byte).

use minix_types::{EINVAL, EIO, ENAMETOOLONG, Errno};

/// Longest name (`EXT2_NAME_MAX`, two hundred fifty-five bytes).
pub const NAME_MAX: usize = 255;
/// Fixed header before the name (`MIN_DIR_ENTRY_SIZE`, eight bytes:
/// number, length, name length, type).
pub const HEADER_BYTES: usize = 8;
/// Alignment (`DIR_ENTRY_ALIGN`, four bytes).
pub const ALIGNMENT: usize = 4;
/// Directory file type (`EXT2_FT_DIR`, two).
pub const TYPE_DIRECTORY: u8 = 2;
/// Regular file type (`EXT2_FT_REG_FILE`, one).
pub const TYPE_REGULAR: u8 = 1;
/// Symbolic link type (`EXT2_FT_SYMLINK`, seven).
pub const TYPE_SYMLINK: u8 = 7;
/// Unknown type (`EXT2_FT_UNKNOWN`, zero).
pub const TYPE_UNKNOWN: u8 = 0;

/// Why directory handling failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirError {
    /// Bad record (short buffer, zero length, overrun).
    Invalid,
    /// Name too long (`ENAMETOOLONG`).
    NameTooLong,
    /// Not found.
    NotFound,
    /// Storage failure.
    Io,
}

impl DirError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::NameTooLong => Errno::from_i32(ENAMETOOLONG),
            Self::NotFound => Errno::from_i32(minix_types::ENOENT),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// One decoded entry: number, type, and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Inode number (zero marks a free slot).
    pub number: u32,
    /// File type byte.
    pub file_type: u8,
    /// Name bytes.
    pub name: alloc::vec::Vec<u8>,
}

/// Actual bytes an entry occupies: header plus name, rounded up to the
/// alignment (`DIR_ENTRY_ACTUAL_SIZE`).
pub const fn actual_size(name_len: usize) -> usize {
    let contents = HEADER_BYTES + name_len;
    contents + ((ALIGNMENT - contents % ALIGNMENT) % ALIGNMENT)
}

/// Usable slack inside a record: total length minus actual size
/// (`DIR_ENTRY_SHRINK`). Deletion grows the predecessor by exactly this
/// plus the removed record.
pub fn slack(record_len: usize, name_len: usize) -> Result<usize, DirError> {
    if record_len < HEADER_BYTES {
        return Err(DirError::Invalid);
    }
    Ok(record_len.saturating_sub(actual_size(name_len)))
}

/// Decode one record at the front of `image` (`search_dir` record walk).
/// Returns the entry plus the record length to advance by. Short images,
/// zero lengths, and overruns refuse: a corrupt length must not steer
/// the walk.
pub fn decode(image: &[u8]) -> Result<(Entry, usize), DirError> {
    if image.len() < HEADER_BYTES {
        return Err(DirError::Invalid);
    }
    let number = u32::from_le_bytes([image[0], image[1], image[2], image[3]]);
    let record_len = u16::from_le_bytes([image[4], image[5]]) as usize;
    let name_len = image[6] as usize;
    let file_type = image[7];
    if record_len < HEADER_BYTES || record_len > image.len() {
        return Err(DirError::Invalid);
    }
    if HEADER_BYTES + name_len > record_len {
        return Err(DirError::Invalid);
    }
    Ok((
        Entry {
            number,
            file_type,
            name: image[HEADER_BYTES..HEADER_BYTES + name_len].to_vec(),
        },
        record_len,
    ))
}

/// Encode one entry for insertion: the caller supplies the record length
/// (actual size, or a larger merged length when absorbing slack).
/// Overlong names refuse before touching anything.
pub fn encode(number: u32, file_type: u8, name: &[u8], record_len: usize) -> Result<alloc::vec::Vec<u8>, DirError> {
    if name.len() > NAME_MAX {
        return Err(DirError::NameTooLong);
    }
    if record_len < actual_size(name.len()) {
        return Err(DirError::Invalid);
    }
    let mut out = alloc::vec![0u8; record_len];
    out[0..4].copy_from_slice(&number.to_le_bytes());
    out[4..6].copy_from_slice(&(record_len as u16).to_le_bytes());
    out[6] = name.len() as u8;
    out[7] = file_type;
    out[HEADER_BYTES..HEADER_BYTES + name.len()].copy_from_slice(name);
    Ok(out)
}

/// Find a name in a block image: walk records, skipping free slots
/// (number zero). Returns the inode number.
pub fn lookup(image: &[u8], name: &[u8]) -> Result<u32, DirError> {
    let mut rest = image;
    while !rest.is_empty() {
        let (entry, record_len) = decode(rest)?;
        if entry.number != 0 && entry.name == name {
            return Ok(entry.number);
        }
        rest = &rest[record_len..];
    }
    Err(DirError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_actual_size_alignment() {
        assert_eq!(actual_size(1), 12);
        assert_eq!(actual_size(4), 12);
        assert_eq!(actual_size(5), 16);
        // Name at the maximum still aligns.
        assert_eq!(actual_size(255) % 4, 0);
    }

    #[test]
    fn test_encode_decode_round_trip() {
        let bytes = encode(12, TYPE_REGULAR, b"hello", actual_size(5)).unwrap();
        assert_eq!(bytes.len(), 16);
        let (entry, length) = decode(&bytes).unwrap();
        assert_eq!(entry.number, 12);
        assert_eq!(entry.file_type, TYPE_REGULAR);
        assert_eq!(entry.name, b"hello");
        assert_eq!(length, 16);
    }

    #[test]
    fn test_decode_rejects_corrupt() {
        assert_eq!(decode(&[]).unwrap_err(), DirError::Invalid);
        assert_eq!(decode(&[0u8; 8]).unwrap_err(), DirError::Invalid);
        // Record length past the image end.
        let mut bad = vec![0u8; 12];
        bad[4] = 64;
        assert_eq!(decode(&bad).unwrap_err(), DirError::Invalid);
    }

    #[test]
    fn test_overlong_name_refuses() {
        assert_eq!(
            encode(1, TYPE_REGULAR, &[b'a'; 256], 264).unwrap_err(),
            DirError::NameTooLong
        );
    }

    #[test]
    fn test_lookup_skips_free_slots() {
        let mut image = vec![];
        image.extend(encode(0, TYPE_UNKNOWN, b"old", 16).unwrap());
        image.extend(encode(7, TYPE_DIRECTORY, b"sub", 16).unwrap());
        assert_eq!(lookup(&image, b"sub").unwrap(), 7);
        assert_eq!(lookup(&image, b"old").unwrap_err(), DirError::NotFound);
        assert_eq!(lookup(&image, b"missing").unwrap_err(), DirError::NotFound);
    }

    #[test]
    fn test_slack_math() {
        // Sixteen-byte record holding a one-byte name (actual twelve):
        // four bytes of slack for the next entry to absorb.
        assert_eq!(slack(16, 1).unwrap(), 4);
        assert_eq!(slack(4, 0).unwrap_err(), DirError::Invalid);
    }
}
