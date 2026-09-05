//! Directory entry listing: the getdents staging encoder.
//!
//! C correspondence: `minix3/minix/lib/libfsdriver/dentry.c` (init, add,
//! finish) with the record layout from `minix3/sys/sys/dirent.h`.
//!
//! Listing a directory cannot write each entry straight to the caller: one
//! entry rarely fills a message, and each kernel copy has a cost. The C code
//! therefore stages entries in a small server-side buffer and flushes it to
//! the caller whenever it fills up. This module keeps that exact staging
//! machine — same states, same flush points, same return contract — but the
//! staging buffer is borrowed from the caller instead of living on the C
//! stack, and the two C panics become typed errors (a library must not abort
//! the server on bad input; see [`DentryError`]).

use minix_types::{EINVAL, ENAMETOOLONG, Errno};

use crate::data::{DataBackend, DataChannel};

/// Longest single name accepted in a listing.
///
/// C: `MAXNAMLEN` (`minix3/sys/sys/dirent.h:55`, value 511, kept in sync with
/// `NAME_MAX`). The C code panics past this limit ("should never happen",
/// `dentry.c:36-37`); here it is a typed error instead.
pub const MAX_NAME_LENGTH: usize = 511;

/// File types reported in directory entries.
///
/// C: `DT_UNKNOWN` / `DT_FIFO` / `DT_CHR` / `DT_DIR` / `DT_BLK` and friends
/// (`minix3/sys/sys/dirent.h:66-80`). Only the values used by file servers
/// are listed; unknown types round-trip as zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DirentType {
    /// Unknown type. C: `DT_UNKNOWN` (0).
    Unknown = 0,
    /// First-in first-out. C: `DT_FIFO` (1).
    Fifo = 1,
    /// Character device. C: `DT_CHR` (2).
    Character = 2,
    /// Directory. C: `DT_DIR` (4).
    Directory = 4,
    /// Block device. C: `DT_BLK` (6).
    Block = 6,
    /// Regular file. C: `DT_REG` (8). Added when the disk read path needed
    /// faithful type reporting (`IFTODT` yields eight for regular files).
    Regular = 8,
    /// Symbolic link. C: `DT_LNK` (10).
    Symlink = 10,
    /// Socket. C: `DT_SOCK` (12).
    Socket = 12,
    /// Any other value (regular files land here in this protocol).
    Other = 255,
}

impl DirentType {
    /// Decode a raw type byte; unlisted values become `Other`.
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::Unknown,
            1 => Self::Fifo,
            2 => Self::Character,
            4 => Self::Directory,
            6 => Self::Block,
            8 => Self::Regular,
            10 => Self::Symlink,
            12 => Self::Socket,
            _ => Self::Other,
        }
    }
}

/// Why a listing step failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DentryError {
    /// A name longer than [`MAX_NAME_LENGTH`]. The C code panics here; this
    /// library reports "name too long" instead so the server survives.
    NameTooLong,
    /// The staging buffer cannot hold even one entry. The C code panics for
    /// the staging buffer ("getdents buffer too small", `dentry.c:49-50`)
    /// and reports invalid for the caller buffer (`dentry.c:42-43`); both
    /// surface here as invalid input.
    BufferTooSmall,
    /// The caller transport failed, carrying its error code.
    Transport(Errno),
}

impl DentryError {
    /// The wire error code for this failure.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::NameTooLong => Errno::from_i32(ENAMETOOLONG),
            Self::BufferTooSmall => Errno::from_i32(EINVAL),
            Self::Transport(error) => error,
        }
    }
}

/// Alignment of directory records.
///
/// C: `_DIRENT_ALIGN` (`minix3/sys/sys/dirent.h:88`): one less than the size
/// of the inode number field. The inode number is sixty-four bits
/// (`minix3/sys/sys/types.h:197`), so records align to eight-byte
/// boundaries.
pub const DIRENT_ALIGN: usize = 8;

/// Offset of the name field inside a record: inode number (eight bytes) plus
/// record length, name length, and type fields (two plus two plus one
/// bytes). C: `_DIRENT_NAMEOFF` (`minix3/sys/sys/dirent.h:94-99`); the name
/// array needs no padding, so the offset is thirteen.
pub const DIRENT_NAME_OFFSET: usize = 13;

/// Length of one record holding a name of `name_length` bytes: the name
/// offset plus the name plus its terminating zero, rounded up to the
/// alignment. C: `_DIRENT_RECLEN` (`minix3/sys/sys/dirent.h:105-107`).
pub const fn record_length(name_length: usize) -> usize {
    (DIRENT_NAME_OFFSET + name_length + 1 + (DIRENT_ALIGN - 1)) & !(DIRENT_ALIGN - 1)
}

/// Staging encoder for one directory listing.
///
/// Mirrors `struct fsdriver_dentry` (`minix3/minix/include/minix/fsdriver.h:28-36`):
/// the caller channel and its total capacity, the bytes flushed so far, and
/// the staging buffer with its fill level. The staging buffer is borrowed so
/// servers of any size can use the encoder without heap allocation.
#[derive(Debug)]
pub struct DentryEncoder<'a, 'b, B: DataBackend> {
    channel: DataChannel<'a, B>,
    capacity: usize,
    flushed: usize,
    staging: &'b mut [u8],
    staged: usize,
}

impl<'a, 'b, B: DataBackend> DentryEncoder<'a, 'b, B> {
    /// Start a listing: no bytes flushed, staging buffer empty.
    /// C: `fsdriver_dentry_init` (`dentry.c:9-20`).
    pub fn new(channel: DataChannel<'a, B>, capacity: usize, staging: &'b mut [u8]) -> Self {
        Self {
            channel,
            capacity,
            flushed: 0,
            staging,
            staged: 0,
        }
    }

    /// Add one entry. Returns the record length on success, zero when the
    /// listing must stop because the entry no longer fits, or an error.
    ///
    /// C: `fsdriver_dentry_add` (`dentry.c:27-79`). The three outcomes match
    /// exactly:
    /// - The entry fits nowhere and nothing was written yet: invalid
    ///   (`dentry.c:42-43`).
    /// - The entry fits in the caller buffer but not in the staging buffer:
    ///   flush the staging buffer first, then stage the entry.
    /// - The entry fits nowhere but something was already written: stop
    ///   with zero (`dentry.c:44-46`).
    pub fn add(
        &mut self,
        inode: u64,
        name: &[u8],
        entry_type: DirentType,
    ) -> Result<usize, DentryError> {
        if name.len() > MAX_NAME_LENGTH {
            return Err(DentryError::NameTooLong);
        }
        let length = record_length(name.len());

        if self.flushed + self.staged + length > self.capacity {
            if self.flushed == 0 && self.staged == 0 {
                return Err(DentryError::BufferTooSmall);
            }
            return Ok(0);
        }

        if self.staged + length > self.staging.len() {
            if self.staged == 0 {
                return Err(DentryError::BufferTooSmall);
            }
            self.flush_staging().map_err(DentryError::Transport)?;
        }

        let base = self.staged;
        self.staging[base..base + 8].copy_from_slice(&inode.to_ne_bytes());
        self.staging[base + 8..base + 10].copy_from_slice(&(length as u16).to_ne_bytes());
        self.staging[base + 10..base + 12].copy_from_slice(&(name.len() as u16).to_ne_bytes());
        self.staging[base + 12] = entry_type as u8;
        self.staging[base + DIRENT_NAME_OFFSET..base + DIRENT_NAME_OFFSET + name.len()]
            .copy_from_slice(name);
        // Null-terminate the name and clear the alignment padding so no
        // server memory leaks to the caller (dentry.c:67-74).
        for slot in &mut self.staging[base + DIRENT_NAME_OFFSET + name.len()..base + length] {
            *slot = 0;
        }
        self.staged += length;
        Ok(length)
    }

    /// Finish the listing: flush the staging remainder and report the total
    /// bytes handed to the caller. C: `fsdriver_dentry_finish`
    /// (`dentry.c:85-99`).
    pub fn finish(mut self) -> Result<usize, Errno> {
        if self.staged > 0 {
            self.flush_staging()?;
        }
        Ok(self.flushed)
    }

    /// Copy the staged bytes to the caller and advance both offsets. Staged
    /// bytes always occupy the buffer prefix, so flushing is a prefix copy
    /// followed by resetting the fill level.
    fn flush_staging(&mut self) -> Result<(), Errno> {
        let staged = self.staged;
        self.channel
            .copy_out(self.flushed, &self.staging[..staged])?;
        self.flushed += staged;
        self.staged = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MemoryBackend;

    /// Run a listing function against plain local buffers and hand the
    /// filled caller buffer back for inspection. All borrows end when the
    /// encoder is finished, so tests can read the buffers afterwards with
    /// no aliasing tricks.
    fn run_listing(
        caller_size: usize,
        staging_size: usize,
        steps: impl FnOnce(&mut DentryEncoder<'_, '_, MemoryBackend<'_>>) -> Result<(), DentryError>,
    ) -> (Result<usize, Errno>, [u8; 256], [u8; 64]) {
        let mut caller = [0u8; 256];
        let mut staging = [0u8; 64];
        let mut backend = MemoryBackend {
            storage: &mut caller[..],
            fail_with: None,
        };
        let result = {
            let channel = DataChannel::Present {
                backend: &mut backend,
                size: caller_size,
            };
            let mut encoder =
                DentryEncoder::new(channel, caller_size, &mut staging[..staging_size]);
            match steps(&mut encoder) {
                Ok(()) => encoder.finish(),
                Err(error) => Err(error.to_errno()),
            }
        };
        (result, caller, staging)
    }

    #[test]
    fn test_record_length_alignment() {
        // Thirteen bytes of header plus the name plus its terminator,
        // rounded up to eight-byte boundaries.
        assert_eq!(record_length(1), 16);
        assert_eq!(record_length(2), 16);
        assert_eq!(record_length(3), 24);
        assert_eq!(record_length(4), 24);
        assert_eq!(record_length(8), 24);
        // Longest legal name still fits in a sixteen-bit record length.
        assert!(record_length(MAX_NAME_LENGTH) <= u16::MAX as usize);
    }

    #[test]
    fn test_add_and_finish_roundtrip() {
        let (result, caller, _staging) = run_listing(128, 64, |enc| {
            let first = enc.add(7, b"hello", DirentType::Directory).unwrap();
            assert_eq!(first, record_length(5));
            let second = enc.add(9, b"world!", DirentType::Other).unwrap();
            assert_eq!(second, record_length(6));
            Ok(())
        });
        let total = result.unwrap();
        assert_eq!(total, record_length(5) + record_length(6));
        // First record layout: inode, record length, name length, type, name.
        assert_eq!(&caller[..8], &7u64.to_ne_bytes());
        assert_eq!(&caller[8..10], &(record_length(5) as u16).to_ne_bytes());
        assert_eq!(&caller[10..12], &5u16.to_ne_bytes());
        assert_eq!(caller[12], DirentType::Directory as u8);
        assert_eq!(&caller[13..18], b"hello");
        assert_eq!(caller[18], 0);
    }

    #[test]
    fn test_add_returns_zero_when_full() {
        let (result, caller, _staging) = run_listing(32, 64, |enc| {
            let first = enc.add(1, b"abcdefgh", DirentType::Other).unwrap();
            assert!(first > 0);
            // "abcdefgh" takes 24 bytes; another 24 no longer fits in 32.
            assert_eq!(enc.add(2, b"abcdefgh", DirentType::Other).unwrap(), 0);
            Ok(())
        });
        assert_eq!(result.unwrap(), record_length(8));
        assert_eq!(&caller[..8], &1u64.to_ne_bytes());
    }

    #[test]
    fn test_empty_listing_finishes_at_zero() {
        let (result, _caller, _staging) = run_listing(64, 64, |_enc| Ok(()));
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_oversized_name_is_rejected() {
        let long = [b'x'; MAX_NAME_LENGTH + 1];
        let (result, _caller, _staging) = run_listing(64, 64, |enc| {
            assert_eq!(
                enc.add(1, &long, DirentType::Other).unwrap_err(),
                DentryError::NameTooLong
            );
            Ok(())
        });
        assert!(result.is_ok());
        assert_eq!(DentryError::NameTooLong.to_errno().to_i32(), ENAMETOOLONG);
    }

    #[test]
    fn test_first_entry_without_room_is_invalid() {
        let (result, _caller, _staging) = run_listing(8, 64, |enc| {
            assert_eq!(
                enc.add(1, b"hello", DirentType::Other).unwrap_err(),
                DentryError::BufferTooSmall
            );
            Ok(())
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_staging_flush_mid_listing() {
        // Caller capacity is large but the staging buffer holds one entry.
        let (result, caller, _staging) = run_listing(256, 24, |enc| {
            let first = enc.add(1, b"abcdefgh", DirentType::Other).unwrap();
            assert_eq!(first, 24);
            let second = enc.add(2, b"abcdefgh", DirentType::Other).unwrap();
            assert_eq!(second, 24);
            Ok(())
        });
        assert_eq!(result.unwrap(), 48);
        assert_eq!(&caller[..8], &1u64.to_ne_bytes());
        assert_eq!(&caller[24..32], &2u64.to_ne_bytes());
    }

    #[test]
    fn test_dirent_type_roundtrip() {
        assert_eq!(DirentType::from_raw(4), DirentType::Directory);
        assert_eq!(DirentType::from_raw(8), DirentType::Regular);
        assert_eq!(DirentType::from_raw(10), DirentType::Symlink);
        assert_eq!(DirentType::from_raw(99), DirentType::Other);
    }
}
