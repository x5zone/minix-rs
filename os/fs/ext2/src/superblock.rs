//! Second extended filesystem superblock: validation, derived geometry,
//! and feature gates (`super.c`, `super.h`, `const.h`).
//!
//! The on-disk superblock always lives at byte offset one thousand
//! twenty-four and always reads as little-endian (`super.c:90-108`:
//! magic check, `main.c:38-41`: little-endian assert). Everything else
//! derives from it: the block size from the base-two logarithm field,
//! the group count from the block and inode totals, and the mount
//! decision from the revision level plus the three feature sets.

use minix_types::{EINVAL, EIO, Errno};

/// Magic number of a second extended filesystem (`SUPER_MAGIC`, 0xEF53).
pub const MAGIC: u16 = 0xEF53;
/// Byte offset of the on-disk superblock (`SUPER_BLOCK_BYTES`, 1024).
pub const SUPER_OFFSET: usize = 1024;
/// On-disk superblock size (`SUPER_SIZE_D`, 1024 bytes).
pub const SUPER_STORED_BYTES: usize = 1024;
/// Old revision: fixed inode size and fixed first inode
/// (`EXT2_GOOD_OLD_REV`, zero).
pub const REVISION_OLD: u32 = 0;
/// Dynamic revision: sized inodes and movable first inode
/// (`EXT2_DYNAMIC_REV`, one).
pub const REVISION_DYNAMIC: u32 = 1;
/// Old-revision inode size (`EXT2_GOOD_OLD_INODE_SIZE`, 128 bytes).
pub const OLD_INODE_SIZE: u32 = 128;
/// Old-revision first usable inode (`EXT2_GOOD_OLD_FIRST_INO`, eleven:
/// one through ten stay reserved).
pub const OLD_FIRST_INODE: u32 = 11;
/// Root inode number (`ROOT_INODE`, two: unlike Minix, one is reserved).
pub const ROOT_INODE_NUMBER: u32 = 2;
/// Cleanly unmounted state (`EXT2_VALID_FS`, one).
pub const STATE_CLEAN: u16 = 1;
/// Error state (`EXT2_ERROR_FS`, two): mounting for writing refuses.
pub const STATE_ERROR: u16 = 2;
/// Compatible features this server understands
/// (`SUPPORTED_INCOMPAT_FEATURES` is only the directory file type; the
/// mask below lists every incompatible bit instead, and validation
/// reports anything outside the supported set).
pub const INCOMPAT_FILETYPE: u32 = 0x0002;
/// Read-only compatible bits this server tolerates mounting
/// (`SUPPORTED_RO_COMPAT_FEATURES`: sparse superblocks plus large
/// files; anything else mounts read-only at best, and this server
/// refuses outright, `mount.c:66-82`).
pub const RO_COMPAT_SPARSE_SUPER: u32 = 0x0001;
pub const RO_COMPAT_LARGE_FILE: u32 = 0x0002;
pub const RO_COMPAT_BTREE_DIR: u32 = 0x0004;

/// Why superblock handling failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuperError {
    /// Bad request (short buffer, corrupt sizes).
    Invalid,
    /// Not a second extended filesystem (magic mismatch).
    NotExt2,
    /// Needs features this server does not implement.
    Unsupported,
    /// Error state on a write mount (`STATE_ERROR`).
    Unclean,
    /// Storage failure.
    Io,
}

impl SuperError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::NotExt2 => Errno::from_i32(EINVAL),
            Self::Unsupported => Errno::from_i32(EINVAL),
            Self::Unclean => Errno::from_i32(EINVAL),
            Self::Io => Errno::from_i32(EIO),
        }
    }
}

/// Raw on-disk fields this server reads (little-endian, subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawSuperblock {
    /// Magic number.
    pub magic: u16,
    /// Base-two logarithm of the block size over one kilobyte.
    pub log_block_size: u32,
    /// Revision level.
    pub revision: u32,
    /// Total inode count.
    pub inode_count: u32,
    /// Total block count.
    pub block_count: u32,
    /// Reserved block count.
    pub reserved_count: u32,
    /// Free block count.
    pub free_blocks: u32,
    /// Free inode count.
    pub free_inodes: u32,
    /// First data block number.
    pub first_data_block: u32,
    /// Blocks per group.
    pub blocks_per_group: u32,
    /// Inodes per group.
    pub inodes_per_group: u32,
    /// Inode size for the dynamic revision.
    pub inode_size: u16,
    /// First usable inode for the dynamic revision.
    pub first_inode: u32,
    /// Compatible feature set.
    pub feature_compat: u32,
    /// Incompatible feature set.
    pub feature_incompat: u32,
    /// Read-only compatible feature set.
    pub feature_ro_compat: u32,
    /// Filesystem state.
    pub state: u16,
}

/// Validated in-memory geometry plus mount inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// Filesystem block size in bytes.
    pub block_size: u32,
    /// Inode size in bytes.
    pub inode_size: u32,
    /// First usable inode number.
    pub first_inode: u32,
    /// Block group count.
    pub group_count: u32,
    /// Group descriptors per block.
    pub descriptors_per_block: u32,
    /// Inode-table blocks per group.
    pub table_blocks_per_group: u32,
    /// Inodes per block.
    pub inodes_per_block: u32,
    /// Whether the mount is read-only.
    pub read_only: bool,
}

/// Validate the magic number (`read_super`, `super.c:107-108`).
pub const fn check_magic(magic: u16) -> Result<(), SuperError> {
    if magic == MAGIC {
        Ok(())
    } else {
        Err(SuperError::NotExt2)
    }
}

/// Derive the block size (`super.c:110-122`): one kilobyte shifted by
/// the logarithm field, refused below the page size, refused when not a
/// multiple of five hundred twelve, refused when smaller than the stored
/// superblock itself.
pub const fn derive_block_size(log_block_size: u32, page_size: u32) -> Result<u32, SuperError> {
    if log_block_size > 8 {
        return Err(SuperError::Invalid);
    }
    let size = 1024u32 << log_block_size;
    if size < page_size || size % 512 != 0 || SUPER_STORED_BYTES as u32 > size {
        return Err(SuperError::Invalid);
    }
    Ok(size)
}

/// Inode size for a revision (`EXT2_INODE_SIZE`): the old revision fixes
/// one hundred twenty-eight bytes, the dynamic revision reads the field.
/// The size must be a power of two and fit in one block
/// (`super.c:129-133`).
pub const fn inode_size_for(revision: u32, field: u16, block_size: u32) -> Result<u32, SuperError> {
    let size = if revision == REVISION_OLD { OLD_INODE_SIZE } else { field as u32 };
    if size == 0 || size > block_size || (size & (size - 1)) != 0 {
        return Err(SuperError::Invalid);
    }
    Ok(size)
}

/// First usable inode for a revision (`EXT2_FIRST_INO`).
pub const fn first_inode_for(revision: u32, field: u32) -> u32 {
    if revision == REVISION_OLD {
        OLD_FIRST_INODE
    } else {
        field
    }
}

/// Group count (`super.c:146-147`): blocks past the first data block,
/// minus one, across blocks-per-group, plus one.
pub const fn group_count(block_count: u32, first_data_block: u32, blocks_per_group: u32) -> Result<u32, SuperError> {
    if blocks_per_group == 0 || block_count <= first_data_block {
        return Err(SuperError::Invalid);
    }
    Ok((block_count - first_data_block - 1) / blocks_per_group + 1)
}

/// Validate the whole superblock and derive geometry (`read_super`).
pub fn validate(raw: &RawSuperblock, page_size: u32, read_only: bool) -> Result<Geometry, SuperError> {
    check_magic(raw.magic)?;
    let block_size = derive_block_size(raw.log_block_size, page_size)?;
    let inode_size = inode_size_for(raw.revision, raw.inode_size, block_size)?;
    if block_size / inode_size == 0 || raw.inodes_per_group == 0 {
        return Err(SuperError::Invalid);
    }
    let groups = group_count(raw.block_count, raw.first_data_block, raw.blocks_per_group)?;
    Ok(Geometry {
        block_size,
        inode_size,
        first_inode: first_inode_for(raw.revision, raw.first_inode),
        group_count: groups,
        descriptors_per_block: 0,
        table_blocks_per_group: 0,
        inodes_per_block: block_size / inode_size,
        read_only,
    })
}

/// Gate incompatible features (`mount.c:50-65`): anything outside the
/// directory-file-type bit refuses, with the caller's diagnosis naming
/// the offending bit.
pub const fn gate_incompatible(features: u32) -> Result<(), SuperError> {
    if features & !INCOMPAT_FILETYPE == 0 {
        Ok(())
    } else {
        Err(SuperError::Unsupported)
    }
}

/// Gate read-only compatible features (`mount.c:66-82`): anything outside
/// sparse-super plus large-file refuses outright (the C server prints and
/// remounts read-only at best; this decision function reports, and the
/// mounter chooses).
pub const fn gate_ro_compatible(features: u32) -> Result<(), SuperError> {
    if features & !(RO_COMPAT_SPARSE_SUPER | RO_COMPAT_LARGE_FILE) == 0 {
        Ok(())
    } else {
        Err(SuperError::Unsupported)
    }
}

/// Gate the error state (`mount.c:84-89`): an unclean filesystem refuses
/// a write mount and accepts a read mount.
pub const fn gate_state(state: u16, read_only: bool) -> Result<(), SuperError> {
    if state == STATE_ERROR && !read_only {
        return Err(SuperError::Unclean);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> RawSuperblock {
        RawSuperblock {
            magic: MAGIC,
            log_block_size: 0,
            revision: REVISION_OLD,
            inode_count: 1024,
            block_count: 4096,
            reserved_count: 0,
            free_blocks: 4000,
            free_inodes: 1000,
            first_data_block: 1,
            blocks_per_group: 8192,
            inodes_per_group: 1024,
            inode_size: 128,
            first_inode: 11,
            feature_compat: 0,
            feature_incompat: INCOMPAT_FILETYPE,
            feature_ro_compat: 0,
            state: STATE_CLEAN,
        }
    }

    #[test]
    fn test_magic_gate() {
        assert!(check_magic(MAGIC).is_ok());
        assert_eq!(check_magic(0x137F).unwrap_err(), SuperError::NotExt2);
    }

    #[test]
    fn test_block_size_derivation() {
        assert_eq!(derive_block_size(0, 4096).unwrap_err(), SuperError::Invalid);
        assert_eq!(derive_block_size(2, 4096).unwrap(), 4096);
        assert_eq!(derive_block_size(9, 4096).unwrap_err(), SuperError::Invalid);
    }

    #[test]
    fn test_inode_size_rules() {
        assert_eq!(inode_size_for(REVISION_OLD, 0, 1024).unwrap(), 128);
        assert_eq!(inode_size_for(REVISION_DYNAMIC, 256, 1024).unwrap(), 256);
        assert_eq!(
            inode_size_for(REVISION_DYNAMIC, 100, 1024).unwrap_err(),
            SuperError::Invalid
        );
        assert_eq!(first_inode_for(REVISION_OLD, 99), 11);
        assert_eq!(first_inode_for(REVISION_DYNAMIC, 99), 99);
    }

    #[test]
    fn test_group_count_math() {
        assert_eq!(group_count(8193, 1, 8192).unwrap(), 1);
        assert_eq!(group_count(16385, 1, 8192).unwrap(), 2);
        assert_eq!(group_count(1, 1, 8192).unwrap_err(), SuperError::Invalid);
    }

    #[test]
    fn test_validate_end_to_end() {
        // One-kilobyte blocks fail the page-size floor, so widen the log.
        let mut wide = raw();
        wide.log_block_size = 2;
        let geometry = validate(&wide, 4096, false).unwrap();
        assert_eq!(geometry.block_size, 4096);
        assert_eq!(geometry.inodes_per_block, 32);
        assert_eq!(geometry.first_inode, 11);
        assert_eq!(geometry.group_count, 1);
    }

    #[test]
    fn test_feature_gates() {
        assert!(gate_incompatible(INCOMPAT_FILETYPE).is_ok());
        assert_eq!(gate_incompatible(0x0004).unwrap_err(), SuperError::Unsupported);
        assert!(gate_ro_compatible(RO_COMPAT_SPARSE_SUPER).is_ok());
        assert_eq!(gate_ro_compatible(RO_COMPAT_BTREE_DIR).unwrap_err(), SuperError::Unsupported);
        assert!(gate_state(STATE_ERROR, true).is_ok());
        assert_eq!(gate_state(STATE_ERROR, false).unwrap_err(), SuperError::Unclean);
    }
}
