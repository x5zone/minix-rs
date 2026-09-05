//! Mount options and permission masks (`sffs_init`, `sffs.h`
//! `struct sffs_params`, `stat.c` `get_mode`).
//!
//! Every file shows the configured owner and group (the host's own
//! identities do not cross the boundary), and permissions are the
//! host's mode masked down: files keep only the file-mask bits,
//! directories only the directory-mask bits, with the type bits forced
//! by the guest-side kind. Case handling is a mount flag: insensitive
//! mounts fold names before comparing.

/// Longest path (`PATH_MAX`, four thousand ninety-six bytes).
pub const PATH_MAX: usize = 4096;
/// Longest single name (`NAME_MAX`, two hundred fifty-five bytes).
pub const NAME_MAX: usize = 255;
/// Arbitrary block size for volume statistics (`BLOCK_SIZE` in the
/// statistics handler: the framework owns no blocks, so any nonzero
/// size decodes the host's byte counts into block counts).
pub const STAT_BLOCK_SIZE: u64 = 4096;
/// Directory type bits forced by the guest kind (`S_IFDIR`).
pub const TYPE_DIRECTORY: u32 = 0o040000;
/// Regular type bits forced by the guest kind (`S_IFREG`).
pub const TYPE_REGULAR: u32 = 0o100000;

/// Mount options (`struct sffs_params`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    /// Host path prefix every guest path hangs below.
    pub prefix: alloc::string::String,
    /// Owner shown for every file.
    pub uid: u16,
    /// Group shown for every file.
    pub gid: u16,
    /// Permission bits kept for files.
    pub file_mask: u32,
    /// Permission bits kept for directories.
    pub dir_mask: u32,
    /// Fold names before comparing.
    pub case_insensitive: bool,
}

impl Params {
    /// Defaults shared by both guests (`vbfs.c:65-71`,
    /// `hgfs.c:41-46`): empty prefix, root ownership, full masks,
    /// case-sensitive.
    pub fn defaults() -> Self {
        Self {
            prefix: alloc::string::String::new(),
            uid: 0,
            gid: 0,
            file_mask: 0o755,
            dir_mask: 0o755,
            case_insensitive: false,
        }
    }

    /// Normalize at startup (`sffs_init`, `main.c:24-27`): trailing
    /// slashes leave the prefix so later joins add exactly one.
    pub fn normalize(&mut self) {
        while self.prefix.ends_with('/') {
            self.prefix.pop();
        }
    }
}

/// Guest-visible mode (`get_mode`, `stat.c:18-29`): the type bits come
/// from the guest-side kind, the permission bits are the host mode
/// masked down by the matching mask.
pub const fn guest_mode(is_directory: bool, host_mode: u32, file_mask: u32, dir_mask: u32) -> u32 {
    if is_directory {
        TYPE_DIRECTORY | (host_mode & dir_mask)
    } else {
        TYPE_REGULAR | (host_mode & file_mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_defaults_match_guests() {
        let params = Params::defaults();
        assert_eq!(params.uid, 0);
        assert_eq!(params.file_mask, 0o755);
        assert_eq!(params.dir_mask, 0o755);
        assert!(!params.case_insensitive);
    }

    #[test]
    fn test_normalize_strips_slashes() {
        let mut params = Params::defaults();
        params.prefix = "/share/".to_string();
        params.normalize();
        assert_eq!(params.prefix, "/share");
        params.prefix = "/".to_string();
        params.normalize();
        assert_eq!(params.prefix, "");
    }

    #[test]
    fn test_guest_mode_masks() {
        assert_eq!(guest_mode(true, 0o777, 0o755, 0o700), 0o040700);
        assert_eq!(guest_mode(false, 0o777, 0o644, 0o755), 0o100644);
        // Type bits survive even a zero host mode.
        assert_eq!(guest_mode(true, 0, 0o755, 0o755), 0o040000);
    }
}
