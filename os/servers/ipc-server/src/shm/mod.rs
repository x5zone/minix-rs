//! Shared memory segments: creation (document 07) and attach/reference
//! counting (document 08).
//!
//! C: `minix3/minix/servers/ipc/shm.c` (469 lines).
//!
//! Split by semantic unit like `sem/`: `segment` owns the 1024-slot table
//! and the birth half of the life cycle, `attach` owns mapping, detach
//! lookup and control commands, `refcount` owns the lazy sweep. Memory and
//! cross-service queries stay at the boundary — pure judgement here,
//! effects as return values.

pub mod attach;
pub mod refcount;
pub mod segment;

pub use attach::{ShmIdView, ShmInfoAgg, ShmctlCommand, align_addr, find_by_phys};
pub use refcount::{RefQuery, SweepPlan, UnmapReq, sweep};
pub use segment::{
    Backing, CreateParams, ShmSegment, ShmSlot, ShmTable, encode_id, next_seq, round_up,
};

use minix_types::{EACCES, EINVAL, ENOMEM, ENOSPC, EPERM};

// ============================================================================
// Shared error type
// ============================================================================

/// Shared-memory errors, one variant per Minix3 failure site.
///
/// Mirrors `sem::SemError` so both halves read alike; [`to_errno`] maps
/// back to the wire values (documents 07/08; constants collected in 99).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShmError {
    /// Bad identifier, address, size, or unknown command. C: `EINVAL`.
    Invalid,
    /// Permission bits fall short. C: `EACCES`.
    Access,
    /// Not the owner on a remove/change path. C: `EPERM`.
    Ownership,
    /// Exclusive-create collision. C: `EEXIST`.
    Exists,
    /// Lookup by key without create flag. C: `ENOENT`.
    Missing,
    /// Table full. C: `ENOSPC`.
    NoSpace,
    /// Anonymous mapping failed. C: `ENOMEM`.
    NoMemory,
}

impl ShmError {
    /// Map to the Minix3 error code.
    pub const fn to_errno(self) -> i32 {
        match self {
            Self::Invalid => EINVAL,
            Self::Access => EACCES,
            Self::Ownership => EPERM,
            Self::Exists => minix_types::EEXIST,
            Self::Missing => minix_types::ENOENT,
            Self::NoSpace => ENOSPC,
            Self::NoMemory => ENOMEM,
        }
    }
}
