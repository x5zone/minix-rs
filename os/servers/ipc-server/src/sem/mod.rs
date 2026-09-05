//! Semaphore sets: table management (document 05) and atomic operations
//! (document 06).
//!
//! C: `minix3/minix/servers/ipc/sem.c` (888 lines).
//!
//! The module is split by semantic unit, not by C file order: `table`
//! owns the ten-slot set table and the set life cycle, `ctl` owns the
//! thirteen control commands, `op` owns atomic trial execution and retry,
//! `waiter` owns the waiter slots and completion. Cross-module effects
//! (event subscription, wake-up messages) travel as return values, never
//! as direct calls — the service layer executes them.

pub mod ctl;
pub mod op;
pub mod table;
pub mod waiter;

pub use ctl::{CtlReply, SemIdView, SemInfo, SemctlCommand, assemble_mib_info, fill_info};
pub use op::{SemOp, TryOutcome, retry, try_ops, validate_ops};
pub use table::{SemSet, Semaphore, SemaphoreTable, TableEffect, Wakeup};
pub use waiter::{Waiter, WaiterTable};

use minix_types::{
    E2BIG, EACCES, EAGAIN, EDONTREPLY, EEXIST, EFBIG, EIDRM, EINTR, EINVAL, ENOENT, ENOSPC, EPERM,
    ERANGE,
};

// ============================================================================
// Shared error type
// ============================================================================

/// Semaphore errors, one variant per Minix3 failure site.
///
/// C returns bare `errno.h` integers from a dozen places; the enum keeps
/// them distinct so tests can assert *which* check fired, and [`to_errno`]
/// maps back to the wire values (documents 05/06,收口 99).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemError {
    /// Bad identifier, index, count, or unknown command. C: `EINVAL`.
    Invalid,
    /// Permission bits fall short. C: `EACCES`.
    Access,
    /// Not the owner on a remove/change-owner path. C: `EPERM`.
    Ownership,
    /// Exclusive-create collision. C: `EEXIST`.
    Exists,
    /// Lookup by non-private key without create flag. C: `ENOENT`.
    Missing,
    /// Table full. C: `ENOSPC`.
    NoSpace,
    /// Value out of range. C: `ERANGE`.
    Range,
    /// More than `SEMOPM` operations. C: `E2BIG`.
    TooManyOps,
    /// Semaphore number past the set size. C: `EFBIG`.
    BadNumber,
    /// Non-blocking operation would wait. C: `EAGAIN`.
    Again,
    /// Set removed while waited on. C: `EIDRM`.
    Removed,
    /// Wait interrupted by a signal. C: `EINTR`.
    Interrupted,
}

impl SemError {
    /// Map to the Minix3 error code.
    pub const fn to_errno(self) -> i32 {
        match self {
            Self::Invalid => EINVAL,
            Self::Access => EACCES,
            Self::Ownership => EPERM,
            Self::Exists => EEXIST,
            Self::Missing => ENOENT,
            Self::NoSpace => ENOSPC,
            Self::Range => ERANGE,
            Self::TooManyOps => E2BIG,
            Self::BadNumber => EFBIG,
            Self::Again => EAGAIN,
            Self::Removed => EIDRM,
            Self::Interrupted => EINTR,
        }
    }
}

/// Suppression marker: the caller is gone, send nothing.
///
/// C: `EDONTREPLY 203` — sys/errno.h:199. Used on the process-exit path
/// (`waiter.rs`); the main loop already knows the sibling marker `SUSPEND`
/// (document 01).
pub const NO_REPLY: i32 = EDONTREPLY;
