//! minix3/minix/tests Rust 腿翻译 —— 凭证域(test11/89)。
//!
//! # 翻译映射
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test89(saved IDs 舞步) | 本文件 [`seteuid_saved_id_dance_lets_root_drop_and_regain`] |
//! | test89(setuid 权限门) | 本文件 [`setuid_requires_match_or_superuser`] |
//! | test46(getgroups/setgroups 专项) | 本文件 [`setgroups_superuser_only_and_getgroups_roundtrip`] |
//! | setsid 与进程组(C 载体是 test42/77,编号套件无专项) | 本文件 [`setsid_binds_procgrp_and_getsid_resolves`](取 `getset.c:205-207` 实现语义) |
//! | test11(exec 面的 UID/GID 语义) | 凭证注入面已由 `srv_fork.rs`(SRV_FORK 的 setuid/setgid 后置,C vfs main.c:867-870)覆盖;文件权限判定归 `t_fs_dir_ops.rs`,此处不重复 |
//!
//! C ground truth:`minix3/minix/servers/pm/getset.c`(223 行,13 个调用)。
//! errno 断言对照 C 值(EPERM/EINVAL/ESRCH)。交付门 = 编译;全部测试
//! `#[ignore]`,点亮前提见各测试属性(本文件四测信息性运行均通过)。

use minix_pm::credentials::{
    CopyGroups, GetOp, GetResult, SetError, SetOp, VfsForwarder, do_get, do_set,
};
use minix_pm::mproc::{Lifecycle, ProcTable};
use minix_types::{Endpoint, UserSlot, VirBytes};

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

fn ep_of(slot: usize) -> Endpoint {
    Endpoint::from_generation_slot(1, slot as i32)
}

/// 播种一个 Running 进程,凭证三元组全置 `uid`(real/eff/saved 同值)。
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

/// 脚本化 VFS 转发器:只记录转发来的 SetOp,恒回 ReplyLater(C tell_vfs
/// 的 SUSPEND 语义,getset.c:219)。
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

/// 脚本化 getgroups 拷贝缝:记录写回用户缓冲的 gid 列表。
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
// test89 —— saved ID 舞步:root 降权后凭 saved id 收回
// ---------------------------------------------------------------------------

/// C 语义(getset.c:134 seteuid 只动 effective;权限门 131-133:目标须等于
/// real 或 saved,或超级用户):(0,0,0) → seteuid(100) 得 (0,100,0)——
/// saved 保留 0;凭 saved 收回 seteuid(0) 成功;setuid(100) 全三元组降权
/// (117-119)后 seteuid(0) 被拒(saved 已是 100)。
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn seteuid_saved_id_dance_lets_root_drop_and_regain() {
    let mut table = ProcTable::new();
    let caller = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 0); // root:(0,0,0)
    let mut vfs = ScriptedForwarder::new();
    let mut copier = ScriptedGroups::new();

    // root 降 effective 到 100:saved 保留 0(getset.c:134)。
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

    // 凭 saved(0)收回 root。
    do_set(&mut table, caller, SetOp::SetEUid(0), &mut copier, &mut vfs)
        .expect("saved id 允许收回 root");
    match do_get(&table, caller, GetOp::GetUid, &mut copier).expect("getuid 可读") {
        GetResult::Uid { real, eff } => assert_eq!((real, eff), (0, 0), "收回后 (0, 0)"),
        other => panic!("getuid 应返回 Uid,得 {other:?}"),
    }

    // setuid(100) 全三元组降权(getset.c:117-119 BSD 语义)。
    do_set(
        &mut table,
        caller,
        SetOp::SetUid(100),
        &mut copier,
        &mut vfs,
    )
    .expect("root setuid(100) 合法");
    // 降权后 seteuid(0) 被拒:0 ≠ real(100) 也 ≠ saved(100)。
    let err = do_set(&mut table, caller, SetOp::SetEUid(0), &mut copier, &mut vfs)
        .expect_err("降权后收不回 root");
    assert_eq!(err.to_errno(), minix_types::EPERM, "seteuid(0) → EPERM");

    // 每次成功的 set 都经 tell_vfs 转发 VFS(C getset.c:219;PM_SETUID 报文填充在 121-125)。
    assert_eq!(vfs.forwarded.len(), 3, "seteuid/seteuid/setuid 三次转发");
}

// ---------------------------------------------------------------------------
// test89 —— setuid 权限门
// ---------------------------------------------------------------------------

/// C 语义(getset.c:113-116):setuid 到非自身 real 且非超级用户 → EPERM;
/// 到自身 real → 合法。
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
// test89 —— setgroups/getgroups 往返
// ---------------------------------------------------------------------------

/// C 语义(getset.c:172-197):setgroups 仅超级用户(EPERM);getgroups
/// count==0 查询组数不拷贝;拷贝路径走 sys_datacopy 缝。
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn setgroups_superuser_only_and_getgroups_roundtrip() {
    let mut table = ProcTable::new();
    let user = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 100);
    let mut vfs = ScriptedForwarder::new();

    // 普通用户:setgroups → EPERM。
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

    // 超级用户:置三组。
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

    // count==0 查询:返回组数,不拷贝(getset.c:34-37)。
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

    // 拷贝路径:缓冲不足(count < ngroups)→ EINVAL(getset.c:39-41);
    // 足够则按序写回。
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
// test89 —— setsid 与 getsid
// ---------------------------------------------------------------------------

/// C 语义(getset.c:205-207):setsid 时若调用者已是进程组长(procgrp ==
/// pid)→ EPERM;否则 procgrp 绑为自身 pid。getsid:pid==0 查自身,未知
/// pid → ESRCH。
#[test]
#[ignore = "点亮前提:PM 凭证面随载体用户态点亮后复核"]
fn setsid_binds_procgrp_and_getsid_resolves() {
    let mut table = ProcTable::new();
    let caller = UserSlot::new(1);
    seed_with_uid(&mut table, 1, 100, 100);
    let mut vfs = ScriptedForwarder::new();
    let mut copier = ScriptedGroups::new();

    // 首次 setsid:procgrp(0) ≠ pid(100) → 绑定成功。
    do_set(&mut table, caller, SetOp::SetSid, &mut copier, &mut vfs).expect("首次 setsid 合法");
    match do_get(&table, caller, GetOp::GetPgrp, &mut copier).expect("getpgrp 可读") {
        GetResult::Pgrp(pgrp) => assert_eq!(pgrp, 100, "procgrp 绑为自身 pid"),
        other => panic!("应返回 Pgrp,得 {other:?}"),
    }

    // 再次 setsid:已是组长 → EPERM(getset.c:206)。
    let err = do_set(&mut table, caller, SetOp::SetSid, &mut copier, &mut vfs)
        .expect_err("组长重复 setsid 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EPERM);

    // getsid:pid==0 查自身;未知 pid → ESRCH。
    match do_get(&table, caller, GetOp::GetSid { pid: 0 }, &mut copier).expect("getsid(0) 合法") {
        GetResult::Sid(sid) => assert_eq!(sid, 100, "自身会话 id = procgrp"),
        other => panic!("应返回 Sid,得 {other:?}"),
    }
    let err = do_get(&table, caller, GetOp::GetSid { pid: 777 }, &mut copier)
        .expect_err("未知 pid 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::ESRCH);
}
