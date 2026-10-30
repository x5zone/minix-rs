//! Rust-leg translation of minix3/minix/tests — the pipe/select/record-lock domain
//! (test7/8/19/20/29/40).
//!
//! # Translation map
//!
//! | C test | Semantic home |
//! |---|---|
//! | test19 and test7.c:224-234 (F_SETFL flag bits, FD_CLOEXEC) | [`fcntl_flag_bits_and_arg_gates`] |
//! | record locks of test7.c:321-377 and test20 (the fcntl() special case, locks across fork/exec) | [`record_locks_conflict_query_and_release`] |
//! | test29 (the dup/dup2 special case; carrier for the F_DUPFD out-of-range EINVAL) | [`fcntl_flag_bits_and_arg_gates`] |
//! | test40 (the three select sets and ready accounting) | [`select_interest_and_ready_accounting`] |
//! | test7/8 (pipe read/write decisions) | [`pipe_check_matrix_again_and_suspend`] |
//!
//! This layer is VFS's pure decision surface (the pipe size matrix, the select bit sets,
//! the lock table), matched function by function against C's
//! `pipe.c`/`select.c`/`lock.c`; the end-to-end suspend continuation (the real wakeup
//! chain) belongs to the VFS↔MFS cross-package bridge entry. Delivery gate =
//! compilation; every test is `#[ignore]`, and the lighting prerequisites are in each
//! test's attribute.

use minix_types::Endpoint;
use minix_vfs::fcntl::{
    LockOp, LockOutcome, LockTable, LockType, O_ACCMODE, O_APPEND, O_NONBLOCK, VnodeKey,
    cloexec_apply, compute_region, dupfd_arg_check, status_get, status_set,
};
use minix_vfs::pipe::{PipeCheckVerdict, pipe_check_decision};
use minix_vfs::read_write::RwDir;
use minix_vfs::select::{FdKind, SelOps, classify, ops2tab_apply, tab2ops};

// ---------------------------------------------------------------------------
// test19/test7/test29 — fcntl flag bits and argument gates
// ---------------------------------------------------------------------------

/// C semantics (the fcntl arm of `servers/vfs/misc.c:158-176`; carrying test19, test7,
/// test29): F_SETFL replaces only the O_NONBLOCK|O_APPEND bits (all other flags
/// survive); F_GETFL reads the access mode plus the two behavior bits; an out-of-range
/// arg to F_DUPFD → EINVAL (test29); FD_CLOEXEC bit decision.
/// FD_CLOEXEC bit decision.
#[test]
#[ignore = "点亮前提:VFS fcntl 臂随载体用户态点亮后复核"]
fn fcntl_flag_bits_and_arg_gates() {
    // F_SETFL: replaces the two bits NONBLOCK|O_APPEND wholesale; the access-mode bits are preserved.
    let flags = O_ACCMODE & 0o2; // O_RDWR (base state, no APPEND/NONBLOCK)
    let after = status_set(flags, O_NONBLOCK | O_APPEND);
    assert_eq!(
        status_get(after) & O_NONBLOCK,
        O_NONBLOCK,
        "NONBLOCK 位写入"
    );
    assert_eq!(
        after & O_ACCMODE,
        O_ACCMODE & 0o2,
        "访问模式位不被 F_SETFL 触碰"
    );
    assert_eq!(
        status_get(after) & O_APPEND,
        O_APPEND,
        "O_APPEND 随 arg 写入"
    );
    // When arg does not carry O_APPEND that bit is cleared (whole replacement of the two bits).
    let stripped = status_set(after, O_NONBLOCK);
    assert_eq!(stripped & O_APPEND, 0, "arg 缺失的比特被清");

    // F_DUPFD argument gate: a negative value or one beyond OPEN_MAX → EINVAL.
    assert!(dupfd_arg_check(-1).is_err());
    assert!(dupfd_arg_check(0).is_ok());

    // FD_CLOEXEC bit decision (F_SETFD semantics).
    assert!(cloexec_apply(1), "FD_CLOEXEC=1 置位");
    assert!(!cloexec_apply(0), "0 清位");
}

// ---------------------------------------------------------------------------
// test7/test20 — record-lock conflict matrix
// ---------------------------------------------------------------------------

/// C semantics (matched against `servers/vfs/lock.c`): a write lock is exclusive; GETLK
/// reports the conflicting holder (type/range/pid); read locks on disjoint ranges
/// coexist; SETLK that would block is rejected (EAGAIN), SETLKW becomes a wait; unlocking
/// lets others in.
#[test]
#[ignore = "点亮前提:VFS 锁表随载体用户态点亮后复核"]
fn record_locks_conflict_query_and_release() {
    let mut mgr = LockTable::new();
    let vnode = VnodeKey {
        fs: Endpoint::MFS,
        ino: 7,
    };

    // pid1 takes a write lock on [0,100).
    let region = compute_region(0, 0, 100).expect("区间合法");
    let out = mgr
        .lock_op_decision(
            LockOp::Set {
                ltype: LockType::Write,
                wait: false,
            },
            1,
            vnode,
            region,
            3,
        )
        .expect("空表无冲突");
    assert_eq!(out, LockOutcome::Granted, "首锁授予");

    // pid2's GETLK write probe: hits pid1's write lock; range/pid are reconciled item by item.
    let probe = compute_region(0, 10, 20).expect("区间合法");
    let out = mgr
        .lock_op_decision(
            LockOp::Query {
                ltype: LockType::Write,
            },
            2,
            vnode,
            probe,
            3,
        )
        .expect("探查不阻塞");
    match out {
        LockOutcome::QueryHit(answer) => {
            assert_eq!(answer.pid, 1, "报持有者 pid");
            assert_eq!(answer.lock_type, LockType::Write, "报写锁");
            assert_eq!((answer.first, answer.len), (0, 100), "报持有区间");
        }
        other => panic!("GETLK 应命中,得 {other:?}"),
    }

    // Disjoint range [200,300): read locks coexist.
    let far = compute_region(0, 200, 100).expect("区间合法");
    let out = mgr
        .lock_op_decision(
            LockOp::Set {
                ltype: LockType::Read,
                wait: false,
            },
            2,
            vnode,
            far,
            3,
        )
        .expect("不重叠无冲突");
    assert_eq!(out, LockOutcome::Granted, "共存区间读锁授予");

    // Overlapping write lock: F_SETLK (no wait) is rejected; F_SETLKW (wait) turns into a suspension.
    let overlap = compute_region(0, 50, 10).expect("区间合法");
    assert!(
        mgr.lock_op_decision(
            LockOp::Set {
                ltype: LockType::Write,
                wait: false,
            },
            2,
            vnode,
            overlap,
            3,
        )
        .is_err(),
        "F_SETLK 阻塞即拒(C EAGAIN)"
    );
    let out = mgr
        .lock_op_decision(
            LockOp::Set {
                ltype: LockType::Write,
                wait: true,
            },
            2,
            vnode,
            overlap,
            3,
        )
        .expect("SETLKW 决策本身不报错");
    assert!(matches!(out, LockOutcome::Wait(_)), "F_SETLKW 转等待挂起");

    // pid1 unlocks: releases all its locks on this vnode. C's per-process sweep runs on
    // the free_proc release face (with the filp close); the lock table's own unlock and
    // wakeup are at lock.c:100-133 (the `lock_revive()` at the end of the unlocking
    // branch, 133). C's lock-semantics dedicated carriers: test7.c:321-377 and test20
    // (the fcntl special case).
    assert!(mgr.release_for(vnode, 1), "有锁可放");
    let out = mgr
        .lock_op_decision(
            LockOp::Set {
                ltype: LockType::Write,
                wait: false,
            },
            2,
            vnode,
            overlap,
            3,
        )
        .expect("释放后无冲突");
    assert_eq!(out, LockOutcome::Granted, "原持有者放锁后他人可上");
}

// ---------------------------------------------------------------------------
// test40 — the three select sets and ready accounting
// ---------------------------------------------------------------------------

/// C semantics (matched against `servers/vfs/select.c`): fds are classified by four
/// predicate classes, and a type mismatch → EBADF; the three interest bits are encoded
/// and decoded; ready accounting has a three-way dedup guard (interested, not yet
/// recorded, user pointer present).
#[test]
#[ignore = "点亮前提:VFS select 挂起续接随载体点亮后复核"]
fn select_interest_and_ready_accounting() {
    // Classification: the type priority table is at select.c:85-90, and 225-232 is the per-type application loop (char > socket > regular > pipe).
    assert_eq!(classify(true, false, false, false), Some(FdKind::Char));
    assert_eq!(
        classify(false, true, true, false),
        Some(FdKind::Sock),
        "表序在前"
    );
    assert_eq!(classify(false, false, true, false), Some(FdKind::File));
    assert_eq!(classify(false, false, false, true), Some(FdKind::Pipe));
    assert_eq!(
        classify(false, false, false, false),
        None,
        "无类型 → EBADF 路"
    );

    // Interest-set encoding: the three bits are independent.
    let interest = tab2ops(true, false, true);
    assert!(interest.contains(SelOps::RD) && interest.contains(SelOps::ERR));
    assert!(!interest.contains(SelOps::WR));

    // Ready accounting: a new bit counts only when all three guards hold. want=RD|WR,
    // interest=RD|ERR, already recorded=empty, pointer=RD|ERR → only RD is newly counted
    // (WR is not counted without interest, ERR is not counted without readiness).
    let marked = ops2tab_apply(
        SelOps::RD | SelOps::WR,
        interest,
        SelOps::empty(),
        SelOps::RD | SelOps::ERR,
    );
    assert_eq!(
        marked.newly,
        SelOps::RD,
        "RD 新计入,WR 无兴趣不计,ERR 无就绪不计"
    );
    let none = ops2tab_apply(SelOps::RD, interest, SelOps::RD, SelOps::RD);
    assert_eq!(none.newly, SelOps::empty(), "已记录的不再计");
}

// ---------------------------------------------------------------------------
// test7/8 — pipe read/write decision matrix
// ---------------------------------------------------------------------------

/// C semantics (matched against `servers/vfs/pipe.c`): a non-blocking empty read →
/// EAGAIN; a non-blocking write on a full pipe → EAGAIN; a blocking empty read with a
/// writer present → suspend; a write after all read ends are closed → EPIPE.
#[test]
#[ignore = "点亮前提:VFS 管道挂起唤醒链随载体点亮后复核"]
fn pipe_check_matrix_again_and_suspend() {
    // Non-blocking empty read: EAGAIN.
    let verdict = pipe_check_decision(RwDir::Read, 0, 8192, true, true, true, false, 64, 0);
    assert!(
        matches!(&verdict, PipeCheckVerdict::Reject(e) if *e == minix_vfs::pipe::PipeError::Again),
        "非阻塞空读 → EAGAIN"
    );

    // Blocking empty read with a writer present: suspend waiting for data (the sleep arm of C pipe.c:217-235).
    let verdict = pipe_check_decision(RwDir::Read, 0, 8192, true, true, false, false, 64, 0);
    assert!(
        matches!(verdict, PipeCheckVerdict::Suspend { .. }),
        "阻塞空读挂起"
    );

    // Non-blocking write on a full pipe: capacity 8 with 8 bytes already in it, another write requested → EAGAIN.
    let verdict = pipe_check_decision(RwDir::Write, 8, 8, true, true, true, false, 1, 0);
    assert!(
        matches!(&verdict, PipeCheckVerdict::Reject(e) if *e == minix_vfs::pipe::PipeError::Again),
        "非阻塞满写 → EAGAIN"
    );

    // Write with no readers: EPIPE (C pipe.c's SIGPIPE pre-check).
    let verdict = pipe_check_decision(RwDir::Write, 0, 8192, false, true, true, false, 1, 0);
    assert!(
        matches!(&verdict, PipeCheckVerdict::Reject(e) if *e == minix_vfs::pipe::PipeError::Pipe),
        "无读者写 → EPIPE"
    );

    // An empty pipe is writable: allowed with the quota = min(request, capacity).
    let verdict = pipe_check_decision(RwDir::Write, 0, 8192, true, true, true, false, 4096, 0);
    match verdict {
        PipeCheckVerdict::Allow { bytes, .. } => assert_eq!(bytes, 4096, "额度 = min(请求, 容量)"),
        other => panic!("空管道写应放行,得 {other:?}"),
    }
}
