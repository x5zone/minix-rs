//! Rust-side translation of the credential tests in minix3/minix/tests
//! (test11/89).
//!
//! # Translation map
//!
//! | C test | Semantic home |
//! |---|---|
//! | test89 (saved-ID dance) | [`seteuid_saved_id_dance_lets_root_drop_and_regain`] in this file |
//! | test89 (setuid permission gate) | [`setuid_requires_match_or_superuser`] in this file |
//! | test46 (getgroups/setgroups specifics) | [`setgroups_superuser_only_and_getgroups_roundtrip`] in this file |
//! | setsid and process groups (C carriers are test42/77; the numbered suite has no dedicated test) | [`setsid_binds_procgrp_and_getsid_resolves`] in this file (taking the implementation semantics of `getset.c:205-207`) |
//! | test11 (UID/GID semantics on the exec face) | the credential injection face is already covered by `srv_fork.rs` (the setuid/setgid suffix of SRV_FORK, C vfs main.c:867-870); file permission decisions belong to `t_fs_dir_ops.rs` and are not repeated here |
//!
//! C ground truth: `minix3/minix/servers/pm/getset.c` (223 lines, 13 call
//! sites). errno assertions compare against the C values
//! (EPERM/EINVAL/ESRCH). Delivery gate = compilation; every test is
//! `#[ignore]`, and each test attribute states the precondition for
//! lighting it up (all four tests passed an informational run of this
//! file).

use minix_pm::credentials::{
    CopyGroups, GetOp, GetResult, SetError, SetOp, VfsForwarder, do_get, do_set,
};
use minix_pm::mproc::{Lifecycle, ProcTable};
use minix_types::{Endpoint, UserSlot, VirBytes};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn ep_of(slot: usize) -> Endpoint {
    Endpoint::from_generation_slot(1, slot as i32)
}

/// Seed a Running process with all three credential IDs set to `uid`
/// (real/effective/saved share the value).
fn seed_with_uid(table: &mut ProcTable, slot: usize, pid: i32, uid: u32) {
    let p = &mut table.procs[slot];
    p.identity.endpoint = ep_of(slot);
    p.identity.id.pid = pid;
    p.state.lifecycle = Lifecycle::Running;
    let cred = p.resources.privilege.credentials_mut();
    cred.user = minix_types::IdSet {
        real: uid,
        effective: uid,
        saved: uid,
    };
    cred.group = minix_types::IdSet {
        real: uid,
        effective: uid,
        saved: uid,
    };
}

/// Scripted VFS forwarder: records every forwarded SetOp and always
/// answers ReplyLater (the SUSPEND semantics of C's `tell_vfs`,
/// getset.c:219).
struct ScriptedForwarder {
    forwarded: Vec<SetOp>,
}

impl ScriptedForwarder {
    fn new() -> Self {
        Self {
            forwarded: Vec::new(),
        }
    }
}

impl VfsForwarder for ScriptedForwarder {
    fn forward_set(
        &mut self,
        _table: &mut ProcTable,
        _slot: UserSlot,
        _ep: Endpoint,
        op: &SetOp,
    ) -> Result<minix_pm::ipc::ReplyIntent, SetError> {
        self.forwarded.push(op.clone());
        Ok(minix_pm::ipc::ReplyIntent::ReplyLater)
    }
}

/// Scripted getgroups copy seam: records the gid list written back into
/// the user buffer.
struct ScriptedGroups {
    written: Vec<Vec<u32>>,
}

impl ScriptedGroups {
    fn new() -> Self {
        Self {
            written: Vec::new(),
        }
    }
}

impl CopyGroups for ScriptedGroups {
    fn copy_to_user(&mut self, gids: &[u32], _ptr: VirBytes) -> Result<(), SetError> {
        self.written.push(gids.to_vec());
        Ok(())
    }
    fn copy_from_user(&mut self, _ptr: VirBytes, _ngroups: usize) -> Result<Vec<u32>, SetError> {
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// test89 — saved ID dance: root drops privilege, then regains it via the
// saved id
// ---------------------------------------------------------------------------

/// C semantics (getset.c:134: seteuid changes only the effective id; the
/// permission gate at 131-133 requires the target to equal real or saved,
/// or the caller to be the superuser): (0,0,0) → seteuid(100) yields
/// (0,100,0) — saved stays 0; regaining root with seteuid(0) through the
/// saved id succeeds; after setuid(100) lowers all three IDs (117-119),
/// seteuid(0) is refused (saved is already 100).
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn seteuid_saved_id_dance_lets_root_drop_and_regain() {
    let mut table = ProcTable::new();
    let caller = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 0); // root:(0,0,0)
    let mut vfs = ScriptedForwarder::new();
    let mut copier = ScriptedGroups::new();

    // root lowers the effective id to 100: saved keeps 0 (getset.c:134).
    do_set(
        &mut table,
        caller,
        SetOp::SetEUid(100),
        &mut copier,
        &mut vfs,
    )
    .expect("root seteuid(100) 合法");
    let read = do_get(&table, caller, GetOp::GetUid, &mut copier).expect("getuid 可读");
    match read {
        GetResult::Uid { real, eff } => {
            assert_eq!((real, eff), (0, 100), "root 降权后 (real, eff) = (0, 100)");
        }
        other => panic!("getuid 应返回 Uid,得 {other:?}"),
    }

    // Regain root through saved (0).
    do_set(&mut table, caller, SetOp::SetEUid(0), &mut copier, &mut vfs)
        .expect("saved id 允许收回 root");
    match do_get(&table, caller, GetOp::GetUid, &mut copier).expect("getuid 可读") {
        GetResult::Uid { real, eff } => assert_eq!((real, eff), (0, 0), "收回后 (0, 0)"),
        other => panic!("getuid 应返回 Uid,得 {other:?}"),
    }

    // setuid(100) lowers all three IDs (BSD semantics, getset.c:117-119).
    do_set(
        &mut table,
        caller,
        SetOp::SetUid(100),
        &mut copier,
        &mut vfs,
    )
    .expect("root setuid(100) 合法");
    // After the drop, seteuid(0) is refused: 0 equals neither real (100)
    // nor saved (100).
    let err = do_set(&mut table, caller, SetOp::SetEUid(0), &mut copier, &mut vfs)
        .expect_err("降权后收不回 root");
    assert_eq!(err.to_errno(), minix_types::EPERM, "seteuid(0) → EPERM");

    // Every successful set is forwarded to VFS through tell_vfs (C
    // getset.c:219; the PM_SETUID message is filled in at 121-125).
    assert_eq!(vfs.forwarded.len(), 3, "seteuid/seteuid/setuid 三次转发");
}

// ---------------------------------------------------------------------------
// test89 — setuid permission gate
// ---------------------------------------------------------------------------

/// C semantics (getset.c:113-116): setuid to an ID that is neither the
/// caller's own real id nor granted by superuser rights → EPERM; setuid
/// to the caller's own real id → allowed.
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn setuid_requires_match_or_superuser() {
    let mut table = ProcTable::new();
    let caller = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 100);
    let mut vfs = ScriptedForwarder::new();
    let mut copier = ScriptedGroups::new();

    let err = do_set(
        &mut table,
        caller,
        SetOp::SetUid(200),
        &mut copier,
        &mut vfs,
    )
    .expect_err("普通用户 setuid(200) 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EPERM);

    do_set(
        &mut table,
        caller,
        SetOp::SetUid(100),
        &mut copier,
        &mut vfs,
    )
    .expect("setuid 到自身 real 合法");
    assert!(vfs.forwarded.len() == 1, "合法 set 转发一次");
}

// ---------------------------------------------------------------------------
// test89 — setgroups/getgroups round trip
// ---------------------------------------------------------------------------

/// C semantics (getset.c:172-197): setgroups is superuser-only (EPERM);
/// getgroups with count==0 queries the group count without copying; the
/// copy path goes through the `sys_datacopy` seam.
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn setgroups_superuser_only_and_getgroups_roundtrip() {
    let mut table = ProcTable::new();
    let user = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 100);
    let mut vfs = ScriptedForwarder::new();

    // Ordinary user: setgroups → EPERM.
    let mut copier = ScriptedGroups::new();
    let err = do_set(
        &mut table,
        user,
        SetOp::SetGroups { gids: vec![10, 11] },
        &mut copier,
        &mut vfs,
    )
    .expect_err("普通用户 setgroups 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EPERM);

    // Superuser: set three groups.
    let root = UserSlot::new(2);
    seed_with_uid(&mut table, 2, 101, 0);
    let mut copier = ScriptedGroups::new();
    do_set(
        &mut table,
        root,
        SetOp::SetGroups {
            gids: vec![10, 11, 12],
        },
        &mut copier,
        &mut vfs,
    )
    .expect("root setgroups 合法");

    // count==0 query: return the group count, copy nothing (getset.c:34-37).
    match do_get(
        &table,
        root,
        GetOp::GetGroups {
            count: 0,
            ptr: VirBytes::new(0),
        },
        &mut copier,
    )
    .expect("查询组数合法")
    {
        GetResult::Groups { count } => assert_eq!(count, 3, "组数 3"),
        other => panic!("应返回 Groups,得 {other:?}"),
    }
    assert!(copier.written.is_empty(), "count==0 不拷贝");

    // Copy path: an undersized buffer (count < ngroups) → EINVAL
    // (getset.c:39-41); a sufficient one writes the list back in order.
    let too_small = do_get(
        &table,
        root,
        GetOp::GetGroups {
            count: 2,
            ptr: VirBytes::new(0x1000),
        },
        &mut copier,
    )
    .expect_err("缓冲不足必须拒绝");
    assert_eq!(too_small.to_errno(), minix_types::EINVAL);

    do_get(
        &table,
        root,
        GetOp::GetGroups {
            count: 3,
            ptr: VirBytes::new(0x1000),
        },
        &mut copier,
    )
    .expect("拷贝合法");
    assert_eq!(
        copier.written.last(),
        Some(&vec![10, 11, 12]),
        "组列表按序写回"
    );
}

// ---------------------------------------------------------------------------
// test89 — setsid and getsid
// ---------------------------------------------------------------------------

/// C semantics (getset.c:205-207): setsid returns EPERM if the caller is
/// already a process group leader (procgrp == pid); otherwise procgrp is
/// bound to the caller's own pid. getsid: pid==0 queries the caller,
/// an unknown pid → ESRCH.
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn setsid_binds_procgrp_and_getsid_resolves() {
    let mut table = ProcTable::new();
    let caller = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 100);
    let mut vfs = ScriptedForwarder::new();
    let mut copier = ScriptedGroups::new();

    // First setsid: procgrp (0) ≠ pid (100) → binds successfully.
    do_set(&mut table, caller, SetOp::SetSid, &mut copier, &mut vfs).expect("首次 setsid 合法");
    match do_get(&table, caller, GetOp::GetPgrp, &mut copier).expect("getpgrp 可读") {
        GetResult::Pgrp(pgrp) => assert_eq!(pgrp, 100, "procgrp 绑为自身 pid"),
        other => panic!("应返回 Pgrp,得 {other:?}"),
    }

    // Second setsid: already a leader → EPERM (getset.c:206).
    let err = do_set(&mut table, caller, SetOp::SetSid, &mut copier, &mut vfs)
        .expect_err("组长重复 setsid 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EPERM);

    // getsid: pid==0 queries the caller; an unknown pid → ESRCH.
    match do_get(&table, caller, GetOp::GetSid { pid: 0 }, &mut copier).expect("getsid(0) 合法") {
        GetResult::Sid(sid) => assert_eq!(sid, 100, "自身会话 id = procgrp"),
        other => panic!("应返回 Sid,得 {other:?}"),
    }
    let err = do_get(&table, caller, GetOp::GetSid { pid: 777 }, &mut copier)
        .expect_err("未知 pid 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::ESRCH);
}
