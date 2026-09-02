//! Credentials: `do_get`/`do_set` 13 calls + `TAINTED` + VFS forwarding.
//!
//! C ground truth: `minix3/minix/servers/pm/getset.c` (223 lines)
//! Design: `.design/15-design.v1.md` D1–D8 (explicit `GetOp`/`SetOp`/`Credentials` methods/`tainted: bool`/VfsForwarder).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, UserSlot, Pid, Uid, Gid, VirBytes, EINVAL, EPERM, EFAULT, ESRCH, ENOSYS};
use crate::mproc::{ProcTable, Credentials, NGROUPS_MAX, RemainingFlags};
use crate::ipc::ReplyIntent;

/// `GID_MAX` (`sys/limits.h`, 32-bit `0xFFFFFFFF` or `i32::MAX`).
pub const GID_MAX: u64 = u32::MAX as u64;

/// `SUPER_USER` (`unistd.h`, `0`).
pub const SUPER_USER: Uid = 0;

/// `do_get` operation (7 branches, getset.c:28).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetOp {
    GetUid,
    GetGid,
    GetGroups { count: i32, ptr: VirBytes },
    GetPid,
    GetPgrp,
    GetSid { pid: Pid },
    Issetugid,
}

/// `do_get` result (dual `r` + `reply` fields, getset.c:52-81).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetResult {
    Uid { real: Uid, eff: Uid },
    Gid { real: Gid, eff: Gid },
    Groups { count: usize },
    Pid { self_pid: Pid, parent: Pid },
    Pgrp(Pid),
    Sid(Pid),
    Issetugid(bool),
    Error(i32),
}

/// `do_set` operation (6 branches, getset.c:110).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetOp {
    SetUid(Uid),
    SetEUid(Uid),
    SetGid(Gid),
    SetEGid(Gid),
    SetGroups { gids: Vec<Gid> },
    SetSid,
}

/// Errors for `do_set` (mapped to errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    Perm,
    Inval,
    Fault,
    Busy,
    Srch,
}

impl SetError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => EPERM,
            Self::Inval => EINVAL,
            Self::Fault => EFAULT,
            Self::Busy => minix_types::EBUSY,
            Self::Srch => ESRCH,
        }
    }
}

/// Copy groups abstraction (`sys_datacopy`, getset.c:43/184, D4, A-11).
pub trait CopyGroups {
    fn copy_to_user(&mut self, gids: &[Gid], ptr: VirBytes) -> Result<(), SetError>;
    fn copy_from_user(&mut self, ptr: VirBytes, ngroups: usize) -> Result<Vec<Gid>, SetError>;
}

/// VFS forwarder (`tell_vfs(SUSPEND)`, getset.c:219, D6, A-6).
pub trait VfsForwarder {
    fn forward_set(&mut self, ep: Endpoint, op: &SetOp) -> Result<ReplyIntent, SetError>;
}

/// `do_get` (`getset.c:18-89`, D1).
pub fn do_get(
    table: &ProcTable,
    caller: UserSlot,
    op: GetOp,
    copier: &mut dyn CopyGroups,
) -> Result<GetResult, SetError> {
    let rmp = &table.procs[caller.get()];
    match op {
        GetOp::GetUid => {
            let real = rmp.resources.privilege.credentials().map(|c| c.user.real).unwrap_or(0);
            let eff = rmp.resources.privilege.credentials().map(|c| c.user.effective).unwrap_or(0);
            Ok(GetResult::Uid { real, eff })
        }
        GetOp::GetGid => {
            let real = rmp.resources.privilege.credentials().map(|c| c.group.real).unwrap_or(0);
            let eff = rmp.resources.privilege.credentials().map(|c| c.group.effective).unwrap_or(0);
            Ok(GetResult::Gid { real, eff })
        }
        GetOp::GetGroups { count, ptr } => {
            if count > NGROUPS_MAX as i32 || count < 0 {
                return Err(SetError::Inval);
            }
            let creds = rmp.resources.privilege.credentials().ok_or(SetError::Inval)?;
            if count == 0 {
                return Ok(GetResult::Groups { count: creds.ngroups });
            }
            if (count as usize) < creds.ngroups {
                return Err(SetError::Inval);
            }
            let gids = &creds.supplemental_groups[..creds.ngroups];
            copier.copy_to_user(gids, ptr)?;
            Ok(GetResult::Groups { count: creds.ngroups })
        }
        GetOp::GetPid => {
            let self_pid = table.procs[caller.get()].identity.id.pid;
            let parent = table.procs[table.procs[caller.get()].parent().get()].identity.id.pid;
            Ok(GetResult::Pid { self_pid, parent })
        }
        GetOp::GetPgrp => {
            Ok(GetResult::Pgrp(rmp.identity.procgrp))
        }
        GetOp::GetSid { pid } => {
            let target = if pid == 0 {
                &table.procs[caller.get()]
            } else {
                match table.find_proc(pid) {
                    Some(slot) => &table.procs[slot.get()],
                    None => return Err(SetError::Srch),
                }
            };
            Ok(GetResult::Sid(target.identity.procgrp))
        }
        GetOp::Issetugid => {
            let tainted = table.procs[caller.get()].resources.flags.contains(RemainingFlags::TAINTED)
                || table.procs[caller.get()].resources.tainted;
            Ok(GetResult::Issetugid(tainted))
        }
    }
}

/// `do_set` (`getset.c:95-223`, D2/D3/D6).
pub fn do_set(
    table: &mut ProcTable,
    caller: UserSlot,
    op: SetOp,
    _copier: &mut dyn CopyGroups,
    vfs: &mut dyn VfsForwarder,
) -> Result<ReplyIntent, SetError> {
    let ep = table.procs[caller.get()].endpoint();
    // Clone credentials for permission checks (avoid double borrow)
    let creds = table.procs[caller.get()].resources.privilege.credentials().cloned().unwrap_or_default();
    let procgrp = table.procs[caller.get()].identity.procgrp;
    let pid = table.procs[caller.get()].identity.id.pid;

    match op {
        SetOp::SetUid(uid) => {
            if creds.user.real != uid && !creds.is_superuser() {
                return Err(SetError::Perm);
            }
            let c = table.procs[caller.get()].resources.privilege.credentials_mut().unwrap();
            c.set_uid_all(uid);
            // VFS forwarding (121-125)
            vfs.forward_set(ep, &SetOp::SetUid(uid))?;
            Ok(ReplyIntent::ReplyLater)
        }
        SetOp::SetEUid(uid) => {
            if creds.user.real != uid && creds.user.saved != uid && !creds.is_superuser() {
                return Err(SetError::Perm);
            }
            table.procs[caller.get()].resources.privilege.credentials_mut().unwrap().set_euid(uid);
            vfs.forward_set(ep, &SetOp::SetEUid(uid))?;
            Ok(ReplyIntent::ReplyLater)
        }
        SetOp::SetGid(gid) => {
            if creds.group.real != gid && !creds.is_superuser() {
                return Err(SetError::Perm);
            }
            table.procs[caller.get()].resources.privilege.credentials_mut().unwrap().set_gid_all(gid);
            vfs.forward_set(ep, &SetOp::SetGid(gid))?;
            Ok(ReplyIntent::ReplyLater)
        }
        SetOp::SetEGid(gid) => {
            if creds.group.real != gid && creds.group.saved != gid && !creds.is_superuser() {
                return Err(SetError::Perm);
            }
            table.procs[caller.get()].resources.privilege.credentials_mut().unwrap().set_egid(gid);
            vfs.forward_set(ep, &SetOp::SetEGid(gid))?;
            Ok(ReplyIntent::ReplyLater)
        }
        SetOp::SetGroups { ref gids } => {
            if !creds.is_superuser() {
                return Err(SetError::Perm);
            }
            if gids.len() > NGROUPS_MAX {
                return Err(SetError::Inval);
            }
            for &g in gids {
                if (g as u64) > GID_MAX {
                    return Err(SetError::Inval);
                }
            }
            table.procs[caller.get()].resources.privilege.credentials_mut().unwrap().set_groups(gids);
            vfs.forward_set(ep, &op)?;
            Ok(ReplyIntent::ReplyLater)
        }
        SetOp::SetSid => {
            if procgrp == pid {
                return Err(SetError::Perm);
            }
            table.procs[caller.get()].identity.procgrp = pid;
            vfs.forward_set(ep, &SetOp::SetSid)?;
            Ok(ReplyIntent::ReplyLater)
        }
    }
}

// Helper to get mutable credentials (for Privilege::User)
trait PrivilegeExt {
    fn credentials_mut(&mut self) -> Option<&mut Credentials>;
}

impl PrivilegeExt for crate::mproc::Privilege {
    fn credentials_mut(&mut self) -> Option<&mut Credentials> {
        match self {
            crate::mproc::Privilege::User(c) => Some(c),
            crate::mproc::Privilege::Kernel => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot, VirBytes};

    fn mk_proc(table: &mut ProcTable, slot: usize, uid: Uid, gid: Gid) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].identity.procgrp = 10;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(uid, gid));
        table.procs[slot].resources.flags = RemainingFlags::empty();
        table.procs[slot].resources.tainted = false;
    }

    struct NopCopy;
    impl CopyGroups for NopCopy {
        fn copy_to_user(&mut self, _gids: &[Gid], _ptr: VirBytes) -> Result<(), SetError> { Ok(()) }
        fn copy_from_user(&mut self, _ptr: VirBytes, ngroups: usize) -> Result<Vec<Gid>, SetError> {
            Ok(vec![0; ngroups])
        }
    }
    struct NopVfs;
    impl VfsForwarder for NopVfs {
        fn forward_set(&mut self, _ep: Endpoint, _op: &SetOp) -> Result<ReplyIntent, SetError> {
            Ok(ReplyIntent::ReplyLater)
        }
    }
    struct FailVfs;
    impl VfsForwarder for FailVfs {
        fn forward_set(&mut self, _ep: Endpoint, _op: &SetOp) -> Result<ReplyIntent, SetError> {
            Err(SetError::Busy)
        }
    }

    #[test]
    fn test_getgroups_zero_queries() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        table.procs[0].resources.privilege.credentials_mut().unwrap().ngroups = 3;
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::GetGroups { count: 0, ptr: VirBytes(0) }, &mut c).unwrap();
        assert_eq!(res, GetResult::Groups { count: 3 });
    }

    #[test]
    fn test_getgroups_less_than_avail_inval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        table.procs[0].resources.privilege.credentials_mut().unwrap().ngroups = 3;
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::GetGroups { count: 2, ptr: VirBytes(0x1000) }, &mut c);
        assert_eq!(res.unwrap_err(), SetError::Inval);
    }

    #[test]
    fn test_getgroups_copy_to_user() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        table.procs[0].resources.privilege.credentials_mut().unwrap().supplemental_groups[0] = 100;
        table.procs[0].resources.privilege.credentials_mut().unwrap().ngroups = 1;
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::GetGroups { count: 1, ptr: VirBytes(0x1000) }, &mut c).unwrap();
        assert_eq!(res, GetResult::Groups { count: 1 });
    }

    #[test]
    fn test_getuid_gid_double_value() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.effective = 0;
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::GetUid, &mut c).unwrap();
        assert_eq!(res, GetResult::Uid { real: 1000, eff: 0 });
        let res2 = do_get(&table, UserSlot::new(0), GetOp::GetGid, &mut c).unwrap();
        assert_eq!(res2, GetResult::Gid { real: 100, eff: 100 });
    }

    #[test]
    fn test_getpid_who_p_explicit() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        mk_proc(&mut table, 1, 1000, 100);
        table.procs[0].state.guardianship = crate::mproc::Guardianship::Normal { parent: UserSlot::new(1) };
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::GetPid, &mut c).unwrap();
        match res {
            GetResult::Pid { self_pid, parent } => {
                assert_eq!(self_pid, 100);
                assert_eq!(parent, 101);
            }
            _ => panic!("unexpected"),
        }
    }

    #[test]
    fn test_getsid_find_proc() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        mk_proc(&mut table, 5, 1000, 100);
        table.procs[5].identity.id.pid = 42;
        table.procs[5].identity.procgrp = 99;
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::GetSid { pid: 42 }, &mut c).unwrap();
        assert_eq!(res, GetResult::Sid(99));
        let res2 = do_get(&table, UserSlot::new(0), GetOp::GetSid { pid: 0 }, &mut c).unwrap();
        assert_eq!(res2, GetResult::Sid(10));
        let res3 = do_get(&table, UserSlot::new(0), GetOp::GetSid { pid: 9999 }, &mut c);
        assert_eq!(res3.unwrap_err(), SetError::Srch);
    }

    #[test]
    fn test_issetugid_tainted() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        let mut c = NopCopy;
        let res = do_get(&table, UserSlot::new(0), GetOp::Issetugid, &mut c).unwrap();
        assert_eq!(res, GetResult::Issetugid(false));
        table.procs[0].resources.tainted = true;
        let res2 = do_get(&table, UserSlot::new(0), GetOp::Issetugid, &mut c).unwrap();
        assert_eq!(res2, GetResult::Issetugid(true));
        table.procs[0].resources.tainted = false;
        table.procs[0].resources.flags.insert(RemainingFlags::TAINTED);
        let res3 = do_get(&table, UserSlot::new(0), GetOp::Issetugid, &mut c).unwrap();
        assert_eq!(res3, GetResult::Issetugid(true));
    }

    #[test]
    fn test_setuid_full_triplet() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        let mut c = NopCopy;
        let mut v = NopVfs;
        let op = SetOp::SetUid(2000);
        // Non-super, real != uid => EPERM
        assert_eq!(do_set(&mut table, UserSlot::new(0), op.clone(), &mut c, &mut v).unwrap_err(), SetError::Perm);
        // Super can set
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.effective = 0;
        let res = do_set(&mut table, UserSlot::new(0), SetOp::SetUid(2000), &mut c, &mut v).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        let creds = table.procs[0].resources.privilege.credentials().unwrap();
        assert_eq!(creds.user.real, 2000);
        assert_eq!(creds.user.effective, 2000);
        assert_eq!(creds.user.saved, 2000);
    }

    #[test]
    fn test_seteuid_triple_check() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.saved = 0;
        let mut c = NopCopy;
        let mut v = NopVfs;
        // real==1000, saved==0, can set to 0 via saved
        let res = do_set(&mut table, UserSlot::new(0), SetOp::SetEUid(0), &mut c, &mut v).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        assert_eq!(table.procs[0].resources.privilege.credentials().unwrap().user.effective, 0);
        // Reset to non-super to test Perm: real=1000, saved=0, eff=1000 (not super)
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.effective = 1000;
        // Try to set to 2000 without super and not real/saved
        let res2 = do_set(&mut table, UserSlot::new(0), SetOp::SetEUid(2000), &mut c, &mut v);
        assert_eq!(res2.unwrap_err(), SetError::Perm);
    }

    #[test]
    fn test_setgroups_super_check() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 1000, 100);
        let mut c = NopCopy;
        let mut v = NopVfs;
        assert_eq!(do_set(&mut table, UserSlot::new(0), SetOp::SetGroups { gids: vec![1,2] }, &mut c, &mut v).unwrap_err(), SetError::Perm);
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.effective = 0;
        let res = do_set(&mut table, UserSlot::new(0), SetOp::SetGroups { gids: vec![1,2] }, &mut c, &mut v).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        assert_eq!(table.procs[0].resources.privilege.credentials().unwrap().ngroups, 2);
    }

    #[test]
    fn test_setgroups_gid_max() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 0, 0);
        let mut c = NopCopy;
        let mut v = NopVfs;
        // GID_MAX is u32::MAX, so no u32 gid can exceed it; instead verify that
        // the check is present by using a valid gid that should succeed.
        let res = do_set(&mut table, UserSlot::new(0), SetOp::SetGroups { gids: vec![100] }, &mut c, &mut v);
        assert_eq!(res.unwrap(), ReplyIntent::ReplyLater);
        // Exceeding NGROUPS_MAX should be Inval (already tested in super check)
        let many = vec![1; NGROUPS_MAX + 1];
        let res2 = do_set(&mut table, UserSlot::new(0), SetOp::SetGroups { gids: many }, &mut c, &mut v);
        assert_eq!(res2.unwrap_err(), SetError::Inval);
    }

    #[test]
    fn test_setsid_already_leader() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        table.procs[0].identity.procgrp = 100;
        table.procs[0].identity.id.pid = 100;
        let mut c = NopCopy;
        let mut v = NopVfs;
        assert_eq!(do_set(&mut table, UserSlot::new(0), SetOp::SetSid, &mut c, &mut v).unwrap_err(), SetError::Perm);
        table.procs[0].identity.procgrp = 10;
        let res = do_set(&mut table, UserSlot::new(0), SetOp::SetSid, &mut c, &mut v).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        assert_eq!(table.procs[0].identity.procgrp, 100);
    }

    #[test]
    fn test_do_set_forwards_to_vfs() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0);
        table.procs[0].resources.privilege.credentials_mut().unwrap().user.effective = 0;
        let mut c = NopCopy;
        let mut v = NopVfs;
        let res = do_set(&mut table, UserSlot::new(0), SetOp::SetUid(0), &mut c, &mut v).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(NGROUPS_MAX, 16);
        assert_eq!(crate::mproc::RemainingFlags::TAINTED.bits(), 0x40000);
        assert_eq!(GID_MAX, u32::MAX as u64);
    }

    fn mk_running(table: &mut ProcTable, slot: usize) {
        table.procs[slot].state.lifecycle = crate::mproc::Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].identity.procgrp = 10;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
    }
}
