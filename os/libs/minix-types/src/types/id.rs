//! User and group ID type definitions.
//!
//! Provides UID/GID and their triplets (real/effective/saved) types.

use core::fmt;

/// User ID (32-bit unsigned integer).
pub type Uid = u32;

/// Group ID (32-bit unsigned integer).
pub type Gid = u32;

/// ID triplet (real / effective / saved).
///
/// Stores three states of UID or GID:
/// - `real`: Real ID, identifies the actual owner of the process.
/// - `effective`: Effective ID, used for permission checks.
/// - `saved`: Saved ID, used for setuid/setgid restoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct IdSet<T> {
    pub real: T,
    pub effective: T,
    pub saved: T,
}

impl<T: Default> Default for IdSet<T> {
    fn default() -> Self {
        Self {
            real: T::default(),
            effective: T::default(),
            saved: T::default(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for IdSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IdSet {{ real: {}, effective: {}, saved: {} }}",
            self.real, self.effective, self.saved
        )
    }
}

/// User ID triplet.
pub type UidSet = IdSet<Uid>;

/// Group ID triplet.
pub type GidSet = IdSet<Gid>;

/// Device number (64-bit unsigned integer).
///
/// Corresponds to C's `dev_t` (`minix3/sys/sys/types.h:187`). Encodes
/// major/minor device numbers; `NO_DEV` (0) means "no device".
pub type DevId = u64;

/// Absence of a device number.
///
/// Corresponds to `#define NO_DEV ((dev_t) 0)` (`minix3/minix/include/minix/const.h:132`).
pub const NO_DEV: DevId = 0;

/// File permission mode bits (64-bit host, 32-bit wire).
///
/// Corresponds to C's `mode_t` (`minix3/sys/sys/ansi.h:41`: `__uint32_t`).
/// Used for `umask` and file creation modes.
pub type Mode = u32;

/// Grant ID.
///
/// Corresponds to C's `cp_grant_id_t` (`minix3/minix/include/minix/type.h`:
/// `int32_t`). Identifies a safecopy grant issued by VFS for driver I/O.
/// `GRANT_INVALID` (-1) is represented as `Option::None` at use sites.
pub type GrantId = i32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_no_dev_sentinel() {
        assert_eq!(NO_DEV, 0);
        assert_eq!(NO_DEV as DevId, DevId::default());
    }

    #[test]
    fn test_mode_is_32bit() {
        // mode_t must fit 32 bits (sys/ansi.h:41) so umask arithmetic
        // (e.g. `!0`) does not silently truncate on the wire.
        assert_eq!(core::mem::size_of::<Mode>(), 4);
    }

    #[test]
    fn test_grant_id_is_32bit() {
        assert_eq!(core::mem::size_of::<GrantId>(), 4);
    }
}
