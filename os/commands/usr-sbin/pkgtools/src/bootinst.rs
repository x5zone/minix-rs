//! Boot block installation vocabulary.
//!
//! Ground truth: `minix3/usr.sbin/installboot/installboot.c`. The file system
//! type dispatch sits near line 61, the stage file choice near line 246, and
//! the room requirement (new setups need 32 free sectors because the second
//! stage loader is about 8 kilobytes) near line 276: without room the tool
//! fails instead of writing a truncated loader. File system implementations
//! live in the sibling files (`ffs.c`, `ext2fs.c`, `minixfs3.c`). Writing
//! stays with the execution layer; this module owns the type parsing and the
//! room check.

use crate::PkgError;

/// File system types the installer understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootFs {
    /// Berkeley fast file system.
    Ffs,
    /// Second extended file system.
    Ext2,
    /// Minix third generation file system.
    Minix3,
}

/// Parse a file system type word (`-t` argument).
pub fn parse_boot_fs(word: &str) -> Result<BootFs, PkgError> {
    match word {
        "ffs" => Ok(BootFs::Ffs),
        "ext2fs" => Ok(BootFs::Ext2),
        "minixfs3" => Ok(BootFs::Minix3),
        _ => Err(PkgError::InvalidArgument),
    }
}

/// Stage file name per file system.
pub fn stage_file(fs: BootFs) -> &'static str {
    match fs {
        BootFs::Ffs => "bootxx_ffs",
        BootFs::Ext2 => "bootxx_ext2fs",
        BootFs::Minix3 => "bootxx_minixfs3",
    }
}

/// Reserved sectors required for the second stage loader.
pub const REQUIRED_SECTORS: u64 = 32;

/// Check that `free_sectors` leaves room for the loader.
pub fn check_loader_room(free_sectors: u64) -> Result<(), PkgError> {
    if free_sectors < REQUIRED_SECTORS {
        return Err(PkgError::NoSpace);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_types_parse() {
        assert_eq!(parse_boot_fs("ffs"), Ok(BootFs::Ffs));
        assert_eq!(parse_boot_fs("ext2fs"), Ok(BootFs::Ext2));
        assert_eq!(parse_boot_fs("minixfs3"), Ok(BootFs::Minix3));
        assert_eq!(parse_boot_fs("ntfs"), Err(PkgError::InvalidArgument));
    }

    #[test]
    fn test_stage_files_named() {
        assert_eq!(stage_file(BootFs::Minix3), "bootxx_minixfs3");
        assert_eq!(stage_file(BootFs::Ffs), "bootxx_ffs");
    }

    #[test]
    fn test_room_checked() {
        assert_eq!(check_loader_room(32), Ok(()));
        assert_eq!(check_loader_room(64), Ok(()));
        assert_eq!(check_loader_room(31), Err(PkgError::NoSpace));
    }
}
