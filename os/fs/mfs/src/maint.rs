//! Filesystem maintenance: synchronous write-back, free-bit counting, and
//! the dirty-marking guard.
//!
//! C correspondence: `minix3/minix/fs/mfs/misc.c` (twenty-three lines:
//! `fs_sync`), `minix3/minix/fs/mfs/stats.c` (eighty-nine lines:
//! `count_free_bits`), `minix3/minix/fs/mfs/clean.h` (fourteen lines: the
//! dirty-marking macro), `minix3/minix/fs/mfs/glo.h` (twenty-two lines:
//! shared globals). The constant table (`const.h`, sixty-eight lines) is
//! catalogued in document 17; each constant's authoritative Rust definition
//! lives with its owning module (superblock, inode, directory), not here.
//!
//! Two rules shape this module. First, write-back order is a contract, not
//! an accident: dirty inodes go to disk before dirty blocks, because
//! writing an inode leaves its results in the block cache (`misc.c:10-14`).
//! Second, counting never reads storage itself: the caller supplies a
//! bitmap image, this module counts within bounds (`stats.c:73-76` clips
//! bits past the end of the map).

use minix_types::{EIO, EINVAL, EROFS, Errno};

/// Why a maintenance call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintError {
    /// Bad request (empty image, corrupt size).
    Invalid,
    /// Marking dirty on a read-only mount (`EROFS`; the C macro prints and
    /// dumps the stack, which a library must not do).
    ReadOnly,
    /// Storage failure while writing back.
    Io,
}

impl MaintError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::ReadOnly => Errno::from_i32(EROFS),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// What a synchronization pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncReport {
    /// Dirty inodes written back.
    pub inodes_written: u64,
    /// Whether the block cache was flushed (always true on success: the
    /// block pass runs even when no inode was dirty).
    pub blocks_flushed: bool,
}

/// Write back dirty inodes, then flush dirty blocks (`fs_sync`,
/// `misc.c:8-23`).
///
/// The caller supplies the dirty inode numbers in table order plus two
/// executors: one writes a single inode by number, one flushes the whole
/// block cache. Inodes go first and blocks go last, because writing an
/// inode stages bytes in the block cache that the block pass must carry to
/// disk. The block pass runs even when the inode list is empty: an empty
/// inode pass does not imply a clean block cache. The first failure stops
/// the pass and reports, with the counts so far preserved in the report.
pub fn sync_filesystem(
    dirty_inodes: &[u64],
    write_inode: &mut dyn FnMut(u64) -> Result<(), MaintError>,
    flush_blocks: &mut dyn FnMut() -> Result<(), MaintError>,
) -> Result<SyncReport, MaintError> {
    let mut report = SyncReport {
        inodes_written: 0,
        blocks_flushed: false,
    };
    for number in dirty_inodes {
        write_inode(*number)?;
        report.inodes_written += 1;
    }
    flush_blocks()?;
    report.blocks_flushed = true;
    Ok(report)
}

/// Count clear bits in a bitmap image (`count_free_bits`, `stats.c:13-89`
/// without storage).
///
/// The image holds the raw bitmap blocks back to back; `total_bits` is the
/// number of meaningful bits (`s_ninodes + 1` for the inode map, zones past
/// the data start for the zone map). Bits at positions at or past
/// `total_bits` are padding and are never counted, even when clear
/// (`stats.c:73-76`). An image shorter than the bit count only contributes
/// the bits it actually holds; missing tail bytes count as nothing, never
/// as free.
pub fn count_free_bits(image: &[u8], total_bits: u64) -> u64 {
    let mut free = 0u64;
    let mut bit = 0u64;
    for byte in image {
        for position in 0..8u64 {
            if bit >= total_bits {
                return free;
            }
            if byte & (1 << position) == 0 {
                free += 1;
            }
            bit += 1;
        }
    }
    free
}

/// Decide whether a dirty mark is legal (`MARKDIRTY`, `clean.h:5-12`).
///
/// On a writable mount the mark proceeds. On a read-only mount a dirty
/// mark means the logic is corrupt (read-only images must never get
/// dirty), and the C macro prints the file and line plus a stack dump.
/// A library must not print, so this guard reports read-only instead and
/// the caller converts it to its own diagnostic.
pub const fn check_dirty_mark(read_only: bool) -> Result<(), MaintError> {
    if read_only {
        return Err(MaintError::ReadOnly);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    extern crate alloc;

    #[test]
    fn test_sync_writes_inodes_before_blocks() {
        use core::cell::RefCell;
        let order = RefCell::new(Vec::new());
        let mut write_inode = |number: u64| {
            order.borrow_mut().push(number);
            Ok(())
        };
        let mut flush_blocks = || {
            order.borrow_mut().push(u64::MAX);
            Ok(())
        };
        let report = sync_filesystem(&[3, 7], &mut write_inode, &mut flush_blocks).unwrap();
        assert_eq!(report.inodes_written, 2);
        assert!(report.blocks_flushed);
        // Inodes first in table order, blocks last.
        assert_eq!(*order.borrow(), alloc::vec![3, 7, u64::MAX]);
    }

    #[test]
    fn test_sync_flushes_blocks_with_no_dirty_inode() {
        let mut flushed = false;
        let mut write_inode = |_: u64| Ok(());
        let mut flush_blocks = || {
            flushed = true;
            Ok(())
        };
        let report = sync_filesystem(&[], &mut write_inode, &mut flush_blocks).unwrap();
        assert_eq!(report.inodes_written, 0);
        assert!(flushed);
        assert!(report.blocks_flushed);
    }

    #[test]
    fn test_sync_stops_at_first_failure() {
        let mut flushed = false;
        let mut write_inode = |number: u64| {
            if number == 7 {
                return Err(MaintError::Io);
            }
            Ok(())
        };
        let mut flush_blocks = || {
            flushed = true;
            Ok(())
        };
        assert_eq!(
            sync_filesystem(&[3, 7, 9], &mut write_inode, &mut flush_blocks).unwrap_err(),
            MaintError::Io
        );
        // The block pass never ran: inode failure stops the pass.
        assert!(!flushed);
    }

    #[test]
    fn test_count_free_bits_clips_padding() {
        // One full byte clear plus a second byte: only the first ten bits
        // are meaningful, the rest is padding.
        assert_eq!(count_free_bits(&[0x00, 0x00], 10), 10);
        // Set bits are not free: 0b11 has six free bits in a full byte.
        assert_eq!(count_free_bits(&[0b11], 8), 6);
        // Short images contribute only what they hold.
        assert_eq!(count_free_bits(&[0x00], 64), 8);
        // Zero meaningful bits means zero free whatever the image holds.
        assert_eq!(count_free_bits(&[0x00], 0), 0);
        // Sixty-five bits across nine bytes, two set (reserved zero plus
        // one used): sixty-three free, matching the volume test shape.
        let mut image = alloc::vec![0u8; 9];
        image[0] = 0b11;
        assert_eq!(count_free_bits(&image, 65), 63);
    }

    #[test]
    fn test_dirty_guard_refuses_read_only() {
        assert!(check_dirty_mark(false).is_ok());
        assert_eq!(check_dirty_mark(true).unwrap_err(), MaintError::ReadOnly);
    }

    #[test]
    fn test_error_codes_match_c() {
        use minix_types::{EIO, EINVAL, EROFS};
        assert_eq!(MaintError::Invalid.to_errno().to_i32(), EINVAL);
        assert_eq!(MaintError::ReadOnly.to_errno().to_i32(), EROFS);
        assert_eq!(MaintError::Io.to_errno().to_i32(), EIO);
    }
}
