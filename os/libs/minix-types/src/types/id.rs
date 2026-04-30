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
