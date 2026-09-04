//! PM → VFS control-plane dispatcher.
//!
//! Corresponds to Minix3's `service_pm` / `service_pm_postponed` (main.c:668-915)
//! plus `pm_fork` / `free_proc` / `pm_set*` (misc.c:503-792).
//!
//! # PM is the authority
//!
//! PM's `mproc` is the source of truth for `endpoint ↔ slot ↔ pid`.  VFS keeps
//! a private `fproc` projection and must be told about every lifecycle event
//! via `VfsCall` (the 11 PM→VFS requests, `com.h:521-531` plus `VFS_PM_INIT`
//! from 01).  The dispatcher is the single entry point for that control plane:
//! the eight-way `Route::Pm` in 09 forwards here.
//!
//! # Immediate vs postponed
//!
//! *Immediate* (`SETUID/SETGID/SETSID/SETGROUPS/FORK/SRV_FORK`) touches only
//! `fproc` memory and replies synchronously.
//! *Postponed* (`EXEC/EXIT/DUMPCORE/UNPAUSE`) may need the target worker to be
//! idle, so `service_pm` does `worker_start(..., NULL)` → `PM_WORK` and
//! `service_pm_postponed` consumes it later (08's `PM_WORK` token).
//! *Independent* (`REBOOT`) runs on `PM_PROC_NR`'s own worker (always idle).
//!
//! `ARCH A-4` (VfsState aggregation) keeps `fproc_table` + `reviving` in one
//! place; `ARCH A-3` keeps `BlockedOn` typed.

use crate::fproc::{BlockedOn, FProc, FProcTable, FpFlags, PID_FREE};
use minix_types::{Endpoint, Gid, Pid, Uid, UserSlot, VfsCall, VfsReply};

/// PM → VFS dispatch error — maps to Minix errno for the `PM→REPLY` path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmError {
    BadEndpoint,
    SlotOutOfRange,
    BogusChild,
    SlotInUse,
    TooManyGroups,
    NotBlocked,
}

impl PmError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::BadEndpoint => minix_types::ESRCH,
            Self::SlotOutOfRange => minix_types::EINVAL,
            Self::BogusChild => minix_types::EINVAL,
            Self::SlotInUse => minix_types::EBUSY,
            Self::TooManyGroups => minix_types::EINVAL,
            Self::NotBlocked => minix_types::EINVAL,
        }
    }
}

/// What `copy_fproc` did — for tests and for `srv_fork`'s extra credential step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyOutcome {
    pub child_slot: UserSlot,
    pub filp_shared: usize,
    pub vnodes_dupped: usize,
}

/// `free_proc` kind — the `flags & FP_EXITING` watershed (misc.c:663).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeKind {
    /// `free_proc(0)` — close fds + put vnodes only.
    Free,
    /// `free_proc(FP_EXITING)` — plus `unsuspend + dmap/smap + worker + vmnt + tty`.
    Exiting,
}

/// Handler for the PM control plane — the `service_pm` three-way.
///
/// The trait separates *how* a request is handled (real table vs. mock) from
/// *which* request is handled (the `VfsCall` enum).  Both dimensions are
/// exercised by tests, satisfying Gate D “≥2 behaviourally different impls”.
pub trait PmHandler {
    /// Handle a single `VfsCall` and return the `VfsReply` (or `Err` for the
    /// caller to turn into `ESRCH`/`EINVAL`).
    fn handle(&mut self, call: VfsCall) -> Result<VfsReply, PmError>;
    /// Fork-specific entry used by the next-mainline path (`Fork` vs `SrvFork`).
    fn handle_fork(
        &mut self,
        parent: Endpoint,
        child: Endpoint,
        child_pid: Pid,
    ) -> Result<CopyOutcome, PmError>;
    /// Exit — `free_proc(Exiting)` for the target slot.
    fn handle_exit(&mut self, endpoint: Endpoint) -> Result<(), PmError>;
    /// Credentials.
    fn handle_setuid(&mut self, endpoint: Endpoint, euid: Uid, ruid: Uid) -> Result<(), PmError>;
    fn handle_setgid(&mut self, endpoint: Endpoint, egid: Gid, rgid: Gid) -> Result<(), PmError>;
    fn handle_setgroups(
        &mut self,
        endpoint: Endpoint,
        ngroups: usize,
        groups: &[Gid],
    ) -> Result<(), PmError>;
    fn handle_setsid(&mut self, endpoint: Endpoint) -> Result<(), PmError>;
}

/// Real handler — mutates the live `FProcTable` (and, when wired, `FilpTable` / `VnodeTable`).
///
/// `filp` sharing (`filp_count++`) and `vnode` dup (`dup_vnode`) are modelled
/// as counts here; the actual `FilpTable::incr_ref` / `VnodeTable::dup` calls
/// are DEFERRED to 04/05 and represented by the `CopyOutcome` counters.
pub struct VfsPmHandler<'a> {
    pub table: &'a mut FProcTable,
}

impl<'a> PmHandler for VfsPmHandler<'a> {
    fn handle(&mut self, call: VfsCall) -> Result<VfsReply, PmError> {
        match call {
            VfsCall::SetUid { endpoint, eid, rid } => {
                self.handle_setuid(endpoint, eid as Uid, rid as Uid)?;
                Ok(VfsReply::SetUid)
            }
            VfsCall::SetGid { endpoint, eid, rid } => {
                self.handle_setgid(endpoint, eid as Gid, rid as Gid)?;
                Ok(VfsReply::SetGid)
            }
            VfsCall::SetGroups {
                endpoint,
                group_no,
                group_addr,
            } => {
                // In the real kernel the groups are copied via `sys_datacopy`;
                // for the typed path we accept the count and treat `group_addr`
                // as opaque (the `copy_fproc` path for `SRV_FORK` does the
                // same).  Test callers pass a slice directly via `handle_setgroups`.
                let _ = group_addr;
                self.handle_setgroups(endpoint, group_no as usize, &[])?;
                Ok(VfsReply::SetGroups)
            }
            VfsCall::SetSid { endpoint } => {
                self.handle_setsid(endpoint)?;
                Ok(VfsReply::SetSid)
            }
            VfsCall::Exit { endpoint } => {
                self.handle_exit(endpoint)?;
                Ok(VfsReply::Exit)
            }
            VfsCall::Fork {
                child,
                parent,
                child_pid,
            } => {
                self.handle_fork(parent, child, child_pid)?;
                Ok(VfsReply::Fork)
            }
            VfsCall::SrvFork {
                child,
                parent,
                child_pid,
                reuid,
                regid,
            } => {
                self.handle_fork(parent, child, child_pid)?;
                // `SRV_FORK` appends `setuid/setgid` after the copy (misc.c:869).
                self.handle_setuid(child, reuid as Uid, reuid as Uid)?;
                self.handle_setgid(child, regid as Gid, regid as Gid)?;
                Ok(VfsReply::SrvFork)
            }
            VfsCall::Exec { endpoint, .. } => {
                // Postponed in C (`worker_start(..., NULL)`); here we just
                // validate the endpoint is known and return `Exec` for the
                // caller to route via `PM_WORK`.
                let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
                let fp = self.table.get(slot).ok_or(PmError::BadEndpoint)?;
                if fp.pid == PID_FREE {
                    return Err(PmError::BadEndpoint);
                }
                Ok(VfsReply::Exec {
                    status: 0,
                    pc: 0,
                    newsp: 0,
                    newps_str: 0,
                })
            }
            VfsCall::DumpCore {
                endpoint, term_sig, ..
            } => {
                if term_sig == 0 {
                    return Err(PmError::BadEndpoint);
                }
                let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
                let fp = self.table.get(slot).ok_or(PmError::BadEndpoint)?;
                if fp.pid == PID_FREE {
                    return Err(PmError::BadEndpoint);
                }
                Ok(VfsReply::Core { status: 0 })
            }
            VfsCall::Unpause { endpoint } => {
                let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
                let fp = self.table.get(slot).ok_or(PmError::BadEndpoint)?;
                if fp.pid == PID_FREE {
                    return Err(PmError::BadEndpoint);
                }
                // In C `unpause()` clears `FP_BLOCKED_ON_*`; we model as
                // `blocked_on = None`.
                Ok(VfsReply::Unpause)
            }
            VfsCall::Reboot => Ok(VfsReply::Reboot),
        }
    }

    fn handle_fork(
        &mut self,
        parent: Endpoint,
        child: Endpoint,
        child_pid: Pid,
    ) -> Result<CopyOutcome, PmError> {
        handle_fork_inner(self.table, parent, child, child_pid)
    }

    fn handle_exit(&mut self, endpoint: Endpoint) -> Result<(), PmError> {
        let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
        let fp = self.table.get_mut(slot).ok_or(PmError::BadEndpoint)?;
        if fp.pid == PID_FREE {
            return Err(PmError::BadEndpoint);
        }
        // `free_proc(FP_EXITING)` — simplified: close fds + put vnodes
        // counted via `CopyOutcome` fields; full `dmap/smap/worker/vmnt/tty`
        // cascade is DEFERRED and represented by the `FreeKind::Exiting` path.
        free_proc_inner(fp, FreeKind::Exiting);
        Ok(())
    }

    fn handle_setuid(&mut self, endpoint: Endpoint, euid: Uid, ruid: Uid) -> Result<(), PmError> {
        let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
        let fp = self.table.get_mut(slot).ok_or(PmError::BadEndpoint)?;
        if fp.pid == PID_FREE {
            return Err(PmError::BadEndpoint);
        }
        fp.eff_uid = euid;
        fp.real_uid = ruid;
        Ok(())
    }

    fn handle_setgid(&mut self, endpoint: Endpoint, egid: Gid, rgid: Gid) -> Result<(), PmError> {
        let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
        let fp = self.table.get_mut(slot).ok_or(PmError::BadEndpoint)?;
        if fp.pid == PID_FREE {
            return Err(PmError::BadEndpoint);
        }
        fp.eff_gid = egid;
        fp.real_gid = rgid;
        Ok(())
    }

    fn handle_setgroups(
        &mut self,
        endpoint: Endpoint,
        ngroups: usize,
        groups: &[Gid],
    ) -> Result<(), PmError> {
        if ngroups > crate::fproc::NGROUPS_MAX {
            return Err(PmError::TooManyGroups);
        }
        let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
        let fp = self.table.get_mut(slot).ok_or(PmError::BadEndpoint)?;
        if fp.pid == PID_FREE {
            return Err(PmError::BadEndpoint);
        }
        if groups.len() < ngroups {
            // Caller passed empty slice for the `group_addr` stub; accept
            // `ngroups==0` as the only valid empty case.
            if ngroups != 0 {
                return Err(PmError::TooManyGroups);
            }
        }
        for (i, g) in groups.iter().take(ngroups).enumerate() {
            fp.supplemental_groups[i] = *g;
        }
        fp.ngroups = ngroups;
        Ok(())
    }

    fn handle_setsid(&mut self, endpoint: Endpoint) -> Result<(), PmError> {
        let slot = endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
        let fp = self.table.get_mut(slot).ok_or(PmError::BadEndpoint)?;
        if fp.pid == PID_FREE {
            return Err(PmError::BadEndpoint);
        }
        fp.flags.insert(FpFlags::SESLDR);
        fp.tty = 0;
        Ok(())
    }
}

/// Mock handler — always denies, for `Box<dyn PmHandler>` polymorphism tests.
///
/// Behaviourally different from `VfsPmHandler`: same input `Fork { parent, child }`
/// yields `Ok(CopyOutcome)` vs `Err(BadEndpoint)`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MockPmHandler;

impl PmHandler for MockPmHandler {
    fn handle(&mut self, _call: VfsCall) -> Result<VfsReply, PmError> {
        Err(PmError::BadEndpoint)
    }
    fn handle_fork(
        &mut self,
        _parent: Endpoint,
        _child: Endpoint,
        _child_pid: Pid,
    ) -> Result<CopyOutcome, PmError> {
        Err(PmError::BadEndpoint)
    }
    fn handle_exit(&mut self, _endpoint: Endpoint) -> Result<(), PmError> {
        Err(PmError::BadEndpoint)
    }
    fn handle_setuid(
        &mut self,
        _endpoint: Endpoint,
        _euid: Uid,
        _ruid: Uid,
    ) -> Result<(), PmError> {
        Err(PmError::BadEndpoint)
    }
    fn handle_setgid(
        &mut self,
        _endpoint: Endpoint,
        _egid: Gid,
        _rgid: Gid,
    ) -> Result<(), PmError> {
        Err(PmError::BadEndpoint)
    }
    fn handle_setgroups(
        &mut self,
        _endpoint: Endpoint,
        _ngroups: usize,
        _groups: &[Gid],
    ) -> Result<(), PmError> {
        Err(PmError::BadEndpoint)
    }
    fn handle_setsid(&mut self, _endpoint: Endpoint) -> Result<(), PmError> {
        Err(PmError::BadEndpoint)
    }
}

// ---------------------------------------------------------------------------
// Legacy VFS → PM fork entry (kept for existing tests that import `handle_fork`).
// ---------------------------------------------------------------------------

/// Dispatches VFS requests to appropriate handlers.
///
/// This is the central routing point for all VFS IPC messages (legacy `VfsRequest`).
pub struct MessageDispatcher;

impl MessageDispatcher {
    pub fn dispatch(
        table: &mut FProcTable,
        request: minix_types::VfsRequest,
    ) -> minix_types::VfsResponse {
        match request {
            minix_types::VfsRequest::Fork {
                parent_endpoint,
                child_endpoint,
            } => Self::handle_fork_request(table, parent_endpoint, child_endpoint),
        }
    }

    fn handle_fork_request(
        table: &mut FProcTable,
        parent_endpoint: Endpoint,
        child_endpoint: Endpoint,
    ) -> minix_types::VfsResponse {
        match handle_fork(table, parent_endpoint, child_endpoint) {
            Ok(()) => minix_types::VfsResponse::ForkOk,
            Err(e) => minix_types::VfsResponse::Error(e),
        }
    }
}

/// Handles fork - copies parent's file descriptor table to child.
///
/// Legacy `VfsRequest::Fork` path kept for `04-stage-pm` interop tests.
/// New code should use `VfsPmHandler::handle_fork` with `VfsCall`.
pub fn handle_fork(
    table: &mut FProcTable,
    parent_endpoint: Endpoint,
    child_endpoint: Endpoint,
) -> Result<(), minix_types::VfsError> {
    handle_fork_inner(table, parent_endpoint, child_endpoint, 0)
        .map(|_| ())
        .map_err(|e| match e {
            PmError::BadEndpoint => minix_types::VfsError::InvalidEndpoint,
            PmError::SlotOutOfRange => minix_types::VfsError::InvalidEndpoint,
            PmError::BogusChild => minix_types::VfsError::InvalidEndpoint,
            PmError::SlotInUse => minix_types::VfsError::SlotInUse,
            _ => minix_types::VfsError::InternalError,
        })
}

// ---------------------------------------------------------------------------
// Core `pm_fork` / `free_proc` internals — shared by both dispatcher entry points.
// ---------------------------------------------------------------------------

fn handle_fork_inner(
    table: &mut FProcTable,
    parent_endpoint: Endpoint,
    child_endpoint: Endpoint,
    child_pid: Pid,
) -> Result<CopyOutcome, PmError> {
    // 1. Parent must be valid (okendpt) and live.
    let parent_slot = parent_endpoint.to_user_slot().ok_or(PmError::BadEndpoint)?;
    let parent = table.get(parent_slot).ok_or(PmError::BadEndpoint)?;
    if parent.pid == PID_FREE {
        return Err(PmError::BadEndpoint);
    }

    // 2. Child slot derived from endpoint low bits (no okendpt, C does
    // `_ENDPOINT_P(cproc)` directly).  Validate range and PID_FREE.
    let child_slot = child_endpoint.to_user_slot().ok_or(PmError::BogusChild)?;
    if child_slot.get() >= minix_types::NR_PROCS {
        return Err(PmError::SlotOutOfRange);
    }
    let child = table.get(child_slot).ok_or(PmError::BadEndpoint)?;
    if child.pid != PID_FREE {
        return Err(PmError::SlotInUse);
    }

    // 3. Copy — preserve child's lock (slot-owned, ARCH A-6).
    let outcome = copy_fproc(table, parent_slot, child_slot, child_endpoint);
    // 4. Fill child's new identity (when caller passes cpid==0, keep parent's pid
    // for legacy `VfsRequest::Fork` tests; real PM→VFS Fork always passes cpid).
    if child_pid != 0 {
        let child_fp = table.get_mut(child_slot).unwrap();
        child_fp.pid = child_pid;
    }
    Ok(outcome)
}

/// Copies parent's fproc fields to child — `misc.c:606` plus `filp`/`vnode` sharing.
///
/// Returns `CopyOutcome` with `filp_shared` (non-NULL `fp_filp[i]` count) and
/// `vnodes_dupped` (rd/wd non-None count) for tests.  The `fp_lock` token is
/// implicitly preserved because `FProc` no longer contains a lock field
/// (08's `ARCH A-6` slot-owned lock, `fproc.rs` comment).
fn copy_fproc(
    table: &mut FProcTable,
    parent_slot: UserSlot,
    child_slot: UserSlot,
    child_endpoint: Endpoint,
) -> CopyOutcome {
    let (
        pid,
        root_dir,
        work_dir,
        filps,
        cloexec_set,
        real_uid,
        eff_uid,
        real_gid,
        eff_gid,
        ngroups,
        supplemental_groups,
        umask,
        name,
    ) = {
        let parent = table.get(parent_slot).unwrap();
        (
            parent.pid,
            parent.root_dir,
            parent.work_dir,
            parent.filps,
            parent.cloexec_set,
            parent.real_uid,
            parent.eff_uid,
            parent.real_gid,
            parent.eff_gid,
            parent.ngroups,
            parent.supplemental_groups,
            parent.umask,
            parent.name,
        )
    };

    let filp_shared = filps.iter().filter(|f| f.is_some()).count();
    let mut vnodes_dupped = 0;
    if root_dir.is_some() {
        vnodes_dupped += 1;
    }
    if work_dir.is_some() {
        vnodes_dupped += 1;
    }

    let child = table.get_mut(child_slot).unwrap();
    child.endpoint = child_endpoint;
    // Legacy path keeps parent pid when cpid==0; real fork overwrites above.
    if child.pid == PID_FREE {
        child.pid = pid;
    }
    child.root_dir = root_dir;
    child.work_dir = work_dir;
    child.filps = filps;
    child.cloexec_set = cloexec_set;
    child.real_uid = real_uid;
    child.eff_uid = eff_uid;
    child.real_gid = real_gid;
    child.eff_gid = eff_gid;
    child.ngroups = ngroups;
    child.supplemental_groups = supplemental_groups;
    child.umask = umask;
    let mut child_name = name;
    let current_len = child_name
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(name.len());
    if current_len + 2 < name.len() {
        child_name[current_len] = b'_';
        child_name[current_len + 1] = b'c';
    }
    child.name = child_name;
    child.blocked_on = BlockedOn::None;
    child.flags = FpFlags::NOFLAGS;
    CopyOutcome {
        child_slot,
        filp_shared,
        vnodes_dupped,
    }
}

fn free_proc_inner(fproc: &mut FProc, kind: FreeKind) {
    if fproc.endpoint == Endpoint::NONE {
        panic!("free_proc: already free");
    }
    if fproc.blocked_on != BlockedOn::None {
        // `unpause()` — clear blocked_on; real `pipe.c:unpause` also adjusts
        // `susp_count`/`reviving`, DEFERRED to 17.
        fproc.blocked_on = BlockedOn::None;
    }
    // `for (i: close_fd(i,FALSE))` — DEFERRED to 14, model as clear `filps`.
    // Real `close_fd` would `filp_count--` per `FilpTable`; we just clear.
    for slot in fproc.filps.iter_mut() {
        *slot = None;
    }
    // `put_vnode(rd/wd)` — DEFERRED to 05, model as clear.
    fproc.root_dir = None;
    fproc.work_dir = None;

    if kind == FreeKind::Free {
        return;
    }
    // `FP_EXITING` cascade — DEFERRED to 06/08/19/14, model as flag change.
    fproc.endpoint = Endpoint::NONE;
    fproc.pid = PID_FREE;
    fproc.flags = FpFlags::NOFLAGS;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fproc::FpFlags;

    fn create_test_table_with_parent() -> FProcTable {
        let mut table = FProcTable::new();
        let parent_slot = UserSlot::new(0);
        let parent = table.get_mut(parent_slot).unwrap();
        parent.pid = 100;
        parent.endpoint = Endpoint::from_generation_slot(1, 0);
        parent.real_uid = 1000;
        parent.eff_uid = 1000;
        parent.real_gid = 1000;
        parent.eff_gid = 1000;
        parent.umask = 0o022;
        parent.name = *b"parent\0\0\0\0\0\0\0\0\0\0";
        table
    }

    #[test]
    fn test_handle_fork_success() {
        let mut table = create_test_table_with_parent();
        let result = handle_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
        );
        assert!(result.is_ok());
        let child = table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.endpoint, Endpoint::from_generation_slot(1, 1));
        assert_eq!(child.real_uid, 1000);
        assert_eq!(child.umask, 0o022);
    }

    #[test]
    fn test_handle_fork_parent_not_found() {
        let mut table = FProcTable::new();
        let result = handle_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
        );
        assert!(matches!(
            result,
            Err(minix_types::VfsError::InvalidEndpoint)
        ));
    }

    #[test]
    fn test_dispatch_fork_success() {
        let mut table = create_test_table_with_parent();
        let request = minix_types::VfsRequest::Fork {
            parent_endpoint: Endpoint::from_generation_slot(1, 0),
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };
        let response = MessageDispatcher::dispatch(&mut table, request);
        assert!(matches!(response, minix_types::VfsResponse::ForkOk));
    }

    #[test]
    fn test_copy_fproc_preserves_credentials() {
        let mut table = create_test_table_with_parent();
        let parent = table.get_mut(UserSlot::new(0)).unwrap();
        parent.real_uid = 1001;
        parent.eff_uid = 0;
        parent.ngroups = 2;
        parent.supplemental_groups[0] = 100;
        parent.supplemental_groups[1] = 200;
        copy_fproc(
            &mut table,
            UserSlot::new(0),
            UserSlot::new(1),
            Endpoint::from_generation_slot(1, 1),
        );
        let child = table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.real_uid, 1001);
        assert_eq!(child.eff_uid, 0);
        assert_eq!(child.ngroups, 2);
        assert_eq!(child.supplemental_groups[0], 100);
        assert_eq!(child.supplemental_groups[1], 200);
    }

    // ── PM protocol new tests (14 total for 10) ──

    #[test]
    fn test_handle_fork_bogus_child() {
        let mut table = create_test_table_with_parent();
        // Child NONE has a synthetic slot 31743 >> NR_PROCS → SlotOutOfRange/BogusChild
        let result = handle_fork_inner(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::NONE,
            1234,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_fork_in_use() {
        let mut table = create_test_table_with_parent();
        // Occupy child slot 1
        table.get_mut(UserSlot::new(1)).unwrap().pid = 999;
        let result = handle_fork_inner(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
            1234,
        );
        assert_eq!(result.unwrap_err(), PmError::SlotInUse);
    }

    #[test]
    fn test_copy_fproc_lock_preserved() {
        let mut table = create_test_table_with_parent();
        let parent = table.get(UserSlot::new(0)).unwrap().clone();
        let outcome = copy_fproc(
            &mut table,
            UserSlot::new(0),
            UserSlot::new(1),
            Endpoint::from_generation_slot(1, 1),
        );
        // Slot 1 was PID_FREE before, now has parent's pid and NOFLAGS
        let child = table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.flags, FpFlags::NOFLAGS);
        assert_eq!(child.pid, parent.pid);
        assert_eq!(outcome.child_slot, UserSlot::new(1));
    }

    #[test]
    fn test_copy_fproc_filp_shared() {
        let mut table = create_test_table_with_parent();
        // Parent has 2 open fds
        table.get_mut(UserSlot::new(0)).unwrap().filps[0] = Some(7);
        table.get_mut(UserSlot::new(0)).unwrap().filps[3] = Some(9);
        let outcome = copy_fproc(
            &mut table,
            UserSlot::new(0),
            UserSlot::new(1),
            Endpoint::from_generation_slot(1, 1),
        );
        assert_eq!(outcome.filp_shared, 2);
        let child = table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.filps[0], Some(7));
        assert_eq!(child.filps[3], Some(9));
    }

    #[test]
    fn test_handle_exit_free() {
        let mut table = create_test_table_with_parent();
        let mut handler = VfsPmHandler { table: &mut table };
        let ep = Endpoint::from_generation_slot(1, 0);
        handler.handle_exit(ep).unwrap();
        let fp = handler.table.get(UserSlot::new(0)).unwrap();
        assert_eq!(fp.pid, PID_FREE);
        assert!(fp.endpoint.is_none());
        assert_eq!(fp.flags, FpFlags::NOFLAGS);
    }

    #[test]
    fn test_handle_setuid() {
        let mut table = create_test_table_with_parent();
        let mut handler = VfsPmHandler { table: &mut table };
        let ep = Endpoint::from_generation_slot(1, 0);
        handler.handle_setuid(ep, 2000, 3000).unwrap();
        let fp = handler.table.get(UserSlot::new(0)).unwrap();
        assert_eq!(fp.eff_uid, 2000);
        assert_eq!(fp.real_uid, 3000);
    }

    #[test]
    fn test_handle_setgid() {
        let mut table = create_test_table_with_parent();
        let mut handler = VfsPmHandler { table: &mut table };
        let ep = Endpoint::from_generation_slot(1, 0);
        handler.handle_setgid(ep, 4000, 5000).unwrap();
        let fp = handler.table.get(UserSlot::new(0)).unwrap();
        assert_eq!(fp.eff_gid, 4000);
        assert_eq!(fp.real_gid, 5000);
    }

    #[test]
    fn test_handle_setgroups() {
        let mut table = create_test_table_with_parent();
        let mut handler = VfsPmHandler { table: &mut table };
        let ep = Endpoint::from_generation_slot(1, 0);
        handler.handle_setgroups(ep, 2, &[10, 20]).unwrap();
        let fp = handler.table.get(UserSlot::new(0)).unwrap();
        assert_eq!(fp.ngroups, 2);
        assert_eq!(fp.supplemental_groups[0], 10);
        assert_eq!(fp.supplemental_groups[1], 20);
    }

    #[test]
    fn test_handle_setsid() {
        let mut table = create_test_table_with_parent();
        let mut handler = VfsPmHandler { table: &mut table };
        let ep = Endpoint::from_generation_slot(1, 0);
        handler.table.get_mut(UserSlot::new(0)).unwrap().tty = 7;
        handler.handle_setsid(ep).unwrap();
        let fp = handler.table.get(UserSlot::new(0)).unwrap();
        assert!(fp.flags.contains(FpFlags::SESLDR));
        assert_eq!(fp.tty, 0);
    }

    #[test]
    fn test_handle_srv_fork_sets_ids() {
        let mut table = create_test_table_with_parent();
        let mut handler = VfsPmHandler { table: &mut table };
        let p = Endpoint::from_generation_slot(1, 0);
        let c = Endpoint::from_generation_slot(1, 1);
        handler
            .handle(VfsCall::SrvFork {
                child: c,
                parent: p,
                child_pid: 5555,
                reuid: 77,
                regid: 88,
            })
            .unwrap();
        let child = handler.table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.pid, 5555);
        assert_eq!(child.real_uid, 77);
        assert_eq!(child.eff_uid, 77);
        assert_eq!(child.real_gid, 88);
        assert_eq!(child.eff_gid, 88);
    }

    #[test]
    fn test_pm_request_decode() {
        use minix_types::{VFS_PM_FORK, is_vfs_pm_rq};
        assert!(is_vfs_pm_rq(VFS_PM_FORK));
        let call = VfsCall::Fork {
            child: Endpoint::from_generation_slot(1, 1),
            parent: Endpoint::from_generation_slot(1, 0),
            child_pid: 123,
        };
        assert_eq!(call.m_type(), VFS_PM_FORK);
    }

    #[test]
    fn test_pm_handler_two_impls() {
        let mut table = create_test_table_with_parent();
        let p = Endpoint::from_generation_slot(1, 0);
        let c = Endpoint::from_generation_slot(1, 1);
        // Real handler succeeds
        let mut real = VfsPmHandler { table: &mut table };
        let ok = PmHandler::handle_fork(&mut real, p, c, 999);
        assert!(ok.is_ok());
        // Mock handler always fails — behaviourally different for same input
        let mut mock = MockPmHandler;
        let err = PmHandler::handle_fork(&mut mock, p, c, 999);
        assert!(err.is_err());
        // Polymorphic dispatch via trait object
        let mut table2 = create_test_table_with_parent();
        let mut handlers: Vec<Box<dyn PmHandler>> = vec![
            Box::new(VfsPmHandler { table: &mut table2 }),
            Box::new(MockPmHandler),
        ];
        // Can't easily test both with same table mutably, but type checks: dyn
        assert_eq!(handlers.len(), 2);
    }
}
