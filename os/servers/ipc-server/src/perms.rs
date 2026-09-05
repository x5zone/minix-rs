//! System V permission model: who may touch an object.
//!
//! Every handler asks one question before doing anything: does the caller
//! have the right to touch this object. This module answers it.
//!
//! C: `check_perm` / `prepare_mib_perm` (utility.c:4-49), permission bits
//! and structures (sys/sys/ipc.h:54-92).
//! Document `04-ipc-permissions.md` §3 (decisions D1-D5).
//!
//! The judgement is split in two on purpose: looking up *who the caller
//! is* needs a cross-service query (only the process manager knows which
//! user owns an endpoint), while comparing *what they may do* is pure
//! logic. This module owns the pure half — [`check_perm`] takes the
//! identity as a parameter ([`Identity`]) so all of it is unit-testable
//! without a process manager. The mask tables ([`resolve_semctl_mask`]
//! etc.) pin down what each call site asks for (document 04 §2.4).

use minix_types::{
    EACCES, EPERM, IPC_INFO, IPC_M, IPC_R, IPC_RMID, IPC_SET, IPC_W, SEM_INFO, SETALL, SETVAL,
    SHM_INFO, SHM_RDONLY,
};

// ============================================================================
// Structures and identity
// ============================================================================

/// Permission record of one object (semaphore set or shared memory segment).
///
/// C: `struct ipc_perm` — sys/ipc.h:54-68, minus the private `_sem_base`
/// pointer (a C implementation detail; the Rust owner holds the data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcPerm {
    /// Lookup or create key. C: `_key`.
    pub key: i32,
    /// Owning user. C: `uid`.
    pub uid: u32,
    /// Owning group. C: `gid`.
    pub gid: u32,
    /// Creating user. C: `cuid`.
    pub creator_uid: u32,
    /// Creating group. C: `cgid`.
    pub creator_gid: u32,
    /// Nine permission bits plus status bits (`SEM_ALLOC` etc.). C: `mode`.
    pub mode: u32,
    /// Sequence number (identifier encoding). C: `_seq`.
    pub seq: u16,
}

/// Caller identity, looked up by the caller before judging.
///
/// C: `getnuid(who)` / `getngid(who)` inside `check_perm` (utility.c:10-11)
/// — a cross-service query to the process manager. Taking it as a parameter
/// keeps the judgement pure (document 04 §3 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Caller user id.
    pub uid: u32,
    /// Caller group id.
    pub gid: u32,
}

/// Snapshot record for management-information output.
///
/// C: `struct ipc_perm_sysctl` — sys/ipc.h:72-81. Same seven fields, wider
/// key; built by construction so it cannot be half-copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcPermSysctl {
    /// Lookup or create key. C: `_key` (widened to 64 bits).
    pub key: u64,
    /// Owning user. C: `uid`.
    pub uid: u32,
    /// Owning group. C: `gid`.
    pub gid: u32,
    /// Creating user. C: `cuid`.
    pub creator_uid: u32,
    /// Creating group. C: `cgid`.
    pub creator_gid: u32,
    /// Permission and status bits. C: `mode`.
    pub mode: u32,
    /// Sequence number. C: `_seq`.
    pub seq: i16,
}

impl IpcPermSysctl {
    /// Copy all seven fields (C: `prepare_mib_perm` — utility.c:38-49:
    /// zero first, then copy each field).
    pub const fn from_perm(perm: &IpcPerm) -> Self {
        Self {
            key: perm.key as u64,
            uid: perm.uid,
            gid: perm.gid,
            creator_uid: perm.creator_uid,
            creator_gid: perm.creator_gid,
            mode: perm.mode,
            seq: perm.seq as i16,
        }
    }
}

// ============================================================================
// Judgement
// ============================================================================

/// Whether the wanted bits survive the three-lane comparison.
///
/// C: `check_perm` — utility.c:4-32, line for line:
/// mask to the top lane (:12), superuser passes (:15-16), same user takes
/// the top lane (:18-20), same group takes the middle lane and shifts the
/// wanted bits (:21-24), everyone else takes the bottom lane (:25-29), and
/// the wanted bits must be non-empty and fully present (:31).
pub const fn check_perm(perm: &IpcPerm, caller: Identity, mode: u32) -> bool {
    let wanted = mode & 0o700;

    // Root is allowed to do anything (utility.c:15-16).
    if caller.uid == 0 {
        return true;
    }

    let (lane, shift) = if caller.uid == perm.uid || caller.uid == perm.creator_uid {
        // Same user (utility.c:18-20).
        (perm.mode & 0o700, 0)
    } else if caller.gid == perm.gid || caller.gid == perm.creator_gid {
        // Same group (utility.c:21-24).
        (perm.mode & 0o070, 3)
    } else {
        // Other user and group (utility.c:25-29).
        (perm.mode & 0o007, 6)
    };
    let aligned = wanted >> shift;

    // (mode && ((mode & req_mode) == mode)) — utility.c:31.
    aligned != 0 && (aligned & lane) == aligned
}

/// Outcome of an access check, with the two failures told apart.
///
/// C scatters two failure codes: `EACCES` when the permission bits fall
/// short, `EPERM` when the *identity* is wrong (remove/change-owner paths).
/// The enum makes the difference a type (document 04 §3 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessVerdict {
    /// Allowed. C: `OK`.
    Allow,
    /// Permission bits fall short. C: `EACCES`.
    DenyAccess,
    /// Not the owner (remove/change-owner paths). C: `EPERM`.
    DenyOwnership,
}

impl AccessVerdict {
    /// Map to the Minix3 error code.
    pub const fn to_errno(self) -> i32 {
        match self {
            Self::Allow => 0,
            Self::DenyAccess => EACCES,
            Self::DenyOwnership => EPERM,
        }
    }
}

/// Owner-or-root identity check for the remove/change-owner paths.
///
/// C: `uid != cuid && uid != uid && uid != 0 → EPERM` (sem.c:526-529,
/// shm.c:315-319/333-337). Note it ignores the group and the permission
/// bits entirely — holding the write bit without being the owner still
/// fails (document 04 §2.4).
pub const fn is_owner_or_root(perm: &IpcPerm, uid: u32) -> bool {
    uid == perm.creator_uid || uid == perm.uid || uid == 0
}

// ============================================================================
// Mask tables: what each call site asks for
// ============================================================================
// C spreads one mask expression per call site (sem.c:107/:520/:536/:705,
// shm.c:65/:150-156/:305). Centralised here so the eight sites are
// reviewable in one place (document 04 §3 D3).

/// What a semaphore-control command needs.
///
/// C: the permission switch in `do_semctl` (sem.c:516-538): set-value
/// commands need the write bit, remove/change-owner need the owner
/// identity, the two information commands are free, everything else needs
/// the read bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemctlAccess {
    /// Check the write bit (`EACCES` on failure).
    CheckWrite,
    /// Check the owner identity (`EPERM` on failure).
    CheckOwner,
    /// Free for general use (no check).
    Free,
    /// Check the read bit (`EACCES` on failure).
    CheckRead,
}

/// Classify a semaphore-control command.
///
/// Unknown commands fall into `CheckRead` here; the command dispatch itself
/// (`ctl.rs`) rejects them with `EINVAL` before any check runs, so the
/// classification never grants anything unreachable.
pub const fn resolve_semctl_mask(cmd: i32) -> SemctlAccess {
    match cmd {
        SETVAL | SETALL => SemctlAccess::CheckWrite,
        IPC_SET | IPC_RMID => SemctlAccess::CheckOwner,
        IPC_INFO | SEM_INFO => SemctlAccess::Free,
        _ => SemctlAccess::CheckRead,
    }
}

/// Mask for a semaphore-operation call: write bit if any operation is
/// non-zero, read bit if all are zero-wait operations.
///
/// C: the mask loop in `do_semop` (sem.c:697-706). Takes the precomputed
/// answer (not the array) so the pure judgement stays decoupled from the
/// operation storage.
pub const fn resolve_semop_mask(has_nonzero: bool) -> u32 {
    if has_nonzero { IPC_W } else { IPC_R }
}

/// Mask for a shared-memory attach: read bit for read-only attaches,
/// read-plus-write otherwise.
///
/// C: `shm.c:150-156` (`SHM_RDONLY → IPC_R`, else `IPC_R | IPC_W`).
/// `SHM_RDONLY 010000` — sys/shm.h:76.
pub const fn resolve_shmat_mask(flag: u32) -> u32 {
    if flag & (SHM_RDONLY as u32) != 0 {
        IPC_R
    } else {
        IPC_R | IPC_W
    }
}

/// Whether a shared-memory-control command needs the owner identity
/// (remove/change-owner) rather than the read bit (status queries).
///
/// C: `do_shmctl` (shm.c:303-337): `IPC_STAT`/`SHM_STAT` check the read
/// bit, `IPC_SET`/`IPC_RMID` check the owner identity, the two information
/// commands are free. Mirrors [`SemctlAccess`] so both tables read alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShmctlAccess {
    /// Check the owner identity (`EPERM` on failure).
    CheckOwner,
    /// Free for general use (no check).
    Free,
    /// Check the read bit (`EACCES` on failure).
    CheckRead,
}

/// Classify a shared-memory-control command (see [`ShmctlAccess`]).
///
/// C: `do_shmctl` (shm.c:303-337): `IPC_STAT`/`SHM_STAT` check the read
/// bit, `IPC_SET`/`IPC_RMID` check the owner identity, the two information
/// commands are free. Mirrors [`SemctlAccess`] so both tables read alike.
pub const fn resolve_shmctl_access(cmd: i32) -> ShmctlAccess {
    match cmd {
        IPC_SET | IPC_RMID => ShmctlAccess::CheckOwner,
        IPC_INFO | SHM_INFO => ShmctlAccess::Free,
        _ => ShmctlAccess::CheckRead,
    }
}

/// Read permission bit (re-export for mask-table readers).
pub const READ_BIT: u32 = IPC_R;
/// Write permission bit (re-export for mask-table readers).
pub const WRITE_BIT: u32 = IPC_W;
/// Control-information bit (re-exported for completeness).
pub const CONTROL_BIT: u32 = IPC_M;
/// Read-plus-write mask used by non-read-only attaches.
pub const READ_WRITE: u32 = IPC_R | IPC_W;

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{GETALL, GETNCNT, GETPID, GETVAL, GETZCNT};

    fn perm() -> IpcPerm {
        // Owner 100, group 200, lanes rwx/r--/--- (0740): owner all,
        // group read-only, others nothing.
        IpcPerm {
            key: 0x1234,
            uid: 100,
            gid: 200,
            creator_uid: 100,
            creator_gid: 200,
            mode: 0o740,
            seq: 5,
        }
    }

    #[test]
    fn root_bypasses_everything() {
        // C: utility.c:15-16 — uid 0 passes regardless of bits.
        let p = perm();
        let root = Identity { uid: 0, gid: 999 };
        assert!(check_perm(&p, root, IPC_R));
        assert!(check_perm(&p, root, IPC_W));
        assert!(check_perm(&p, root, IPC_R | IPC_W));
    }

    #[test]
    fn owner_group_other_lanes() {
        // C: utility.c:18-29 — each lane reads its own three bits.
        let p = perm();
        let owner = Identity { uid: 100, gid: 999 };
        assert!(check_perm(&p, owner, IPC_R));
        assert!(check_perm(&p, owner, IPC_W));
        let group = Identity { uid: 101, gid: 200 };
        assert!(check_perm(&p, group, IPC_R));
        assert!(!check_perm(&p, group, IPC_W));
        let other = Identity { uid: 101, gid: 201 };
        assert!(!check_perm(&p, other, IPC_R));
        assert!(!check_perm(&p, other, IPC_W));
    }

    #[test]
    fn empty_mask_denied() {
        // C: utility.c:31 first half — wanting nothing is not permission.
        let p = perm();
        let owner = Identity { uid: 100, gid: 200 };
        assert!(!check_perm(&p, owner, 0));
    }

    #[test]
    fn partial_hit_denied() {
        // C: utility.c:31 second half — every wanted bit must be present.
        // Lane 0700 of 0o440 has read but not write.
        let p = IpcPerm {
            mode: 0o440,
            ..perm()
        };
        let owner = Identity { uid: 100, gid: 200 };
        assert!(check_perm(&p, owner, IPC_R));
        assert!(!check_perm(&p, owner, IPC_R | IPC_W));
    }

    #[test]
    fn creator_counts_as_owner() {
        // C: utility.c:18 — creator matches even when owner changed
        // (IPC_SET rewrites uid but not cuid).
        let p = IpcPerm {
            uid: 300,
            creator_uid: 100,
            mode: 0o700,
            ..perm()
        };
        let creator = Identity { uid: 100, gid: 999 };
        assert!(check_perm(&p, creator, IPC_W));
        assert!(is_owner_or_root(&p, 100));
        assert!(!is_owner_or_root(&p, 300 - 1));
    }

    #[test]
    fn semctl_mask_matrix() {
        // C: sem.c:516-538.
        assert_eq!(resolve_semctl_mask(SETVAL), SemctlAccess::CheckWrite);
        assert_eq!(resolve_semctl_mask(SETALL), SemctlAccess::CheckWrite);
        assert_eq!(resolve_semctl_mask(IPC_SET), SemctlAccess::CheckOwner);
        assert_eq!(resolve_semctl_mask(IPC_RMID), SemctlAccess::CheckOwner);
        assert_eq!(resolve_semctl_mask(IPC_INFO), SemctlAccess::Free);
        assert_eq!(resolve_semctl_mask(SEM_INFO), SemctlAccess::Free);
        assert_eq!(resolve_semctl_mask(GETVAL), SemctlAccess::CheckRead);
        assert_eq!(resolve_semctl_mask(GETALL), SemctlAccess::CheckRead);
        assert_eq!(resolve_semctl_mask(GETNCNT), SemctlAccess::CheckRead);
        assert_eq!(resolve_semctl_mask(GETPID), SemctlAccess::CheckRead);
        assert_eq!(resolve_semctl_mask(GETZCNT), SemctlAccess::CheckRead);
        assert_eq!(
            AccessVerdict::DenyAccess.to_errno(),
            EACCES,
            "denied bits map to EACCES"
        );
        assert_eq!(
            AccessVerdict::DenyOwnership.to_errno(),
            EPERM,
            "wrong identity maps to EPERM"
        );
        assert_eq!(AccessVerdict::Allow.to_errno(), 0);
    }

    #[test]
    fn semop_mask_matrix() {
        // C: sem.c:697-706 — any non-zero operation wants the write bit.
        assert_eq!(resolve_semop_mask(true), IPC_W);
        assert_eq!(resolve_semop_mask(false), IPC_R);
    }

    #[test]
    fn shmat_mask_matrix() {
        // C: shm.c:150-156.
        assert_eq!(resolve_shmat_mask(SHM_RDONLY as u32), IPC_R);
        assert_eq!(resolve_shmat_mask(0), IPC_R | IPC_W);
    }

    #[test]
    fn owner_check_matrix() {
        // C: sem.c:526-529 — owner, creator, or root pass; group membership
        // and write bits do not help.
        let p = perm();
        assert!(is_owner_or_root(&p, 100));
        assert!(is_owner_or_root(&p, 0));
        assert!(!is_owner_or_root(&p, 200));
        let group_member = Identity { uid: 500, gid: 200 };
        assert!(check_perm(&p, group_member, IPC_R));
        assert!(!is_owner_or_root(&p, 500));
    }

    #[test]
    fn mib_perm_copies_all_fields() {
        // C: utility.c:41-48 — all seven fields, nothing else.
        let p = perm();
        let snap = IpcPermSysctl::from_perm(&p);
        assert_eq!(
            (
                snap.key,
                snap.uid,
                snap.gid,
                snap.creator_uid,
                snap.creator_gid,
                snap.mode,
                snap.seq
            ),
            (0x1234, 100, 200, 100, 200, 0o740, 5)
        );
    }
}
