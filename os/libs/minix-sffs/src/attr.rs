//! Guest-visible file attributes (`struct sffs_attr`, sffs.h:13-21).
//!
//! The mask says which fields the hypervisor side filled or accepts; the
//! Rust shape carries the same four time pairs and the mode/size pair as
//! optional fields so callers cannot accidentally read a field the
//! backend never set.

/// Attribute masks (`SFFS_ATTR_*`, sffs.h:23-31).
pub const ATTR_SIZE: u32 = 0x01;
/// See [`ATTR_SIZE`].
pub const ATTR_CRTIME: u32 = 0x02;
/// See [`ATTR_SIZE`].
pub const ATTR_ATIME: u32 = 0x04;
/// See [`ATTR_SIZE`].
pub const ATTR_MTIME: u32 = 0x08;
/// See [`ATTR_SIZE`].
pub const ATTR_CTIME: u32 = 0x10;
/// See [`ATTR_SIZE`].
pub const ATTR_MODE: u32 = 0x20;

/// One attribute snapshot. Fields the backend did not report stay
/// `None` (the C mask bit cleared).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SffsAttr {
    /// Type and permission bits (`SFFS_ATTR_MODE`).
    pub mode: Option<u32>,
    /// File size in bytes (`SFFS_ATTR_SIZE`).
    pub size: Option<u64>,
    /// Creation time, seconds + nanoseconds (`SFFS_ATTR_CRTIME`).
    pub crtime: Option<(i64, i64)>,
    /// Access time (`SFFS_ATTR_ATIME`).
    pub atime: Option<(i64, i64)>,
    /// Modification time (`SFFS_ATTR_MTIME`).
    pub mtime: Option<(i64, i64)>,
    /// Change time (`SFFS_ATTR_CTIME`).
    pub ctime: Option<(i64, i64)>,
}

impl SffsAttr {
    /// The mode's type bits (`S_IFMT` slice), when reported.
    pub const fn type_bits(&self) -> Option<u32> {
        match self.mode {
            Some(mode) => Some(mode & 0o170000),
            None => None,
        }
    }

    /// Whether the reported node is a directory.
    pub fn is_directory(&self) -> Option<bool> {
        self.type_bits().map(|t| t == 0o040000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_bits_and_directory() {
        let mut attr = SffsAttr::default();
        assert_eq!(attr.type_bits(), None);
        assert_eq!(attr.is_directory(), None);
        attr.mode = Some(0o040755);
        assert_eq!(attr.type_bits(), Some(0o040000));
        assert_eq!(attr.is_directory(), Some(true));
        attr.mode = Some(0o100644);
        assert_eq!(attr.is_directory(), Some(false));
    }

    #[test]
    fn test_masks_match_c() {
        assert_eq!(ATTR_SIZE, 0x01);
        assert_eq!(ATTR_CRTIME, 0x02);
        assert_eq!(ATTR_ATIME, 0x04);
        assert_eq!(ATTR_MTIME, 0x08);
        assert_eq!(ATTR_CTIME, 0x10);
        assert_eq!(ATTR_MODE, 0x20);
    }
}
