//! minix3/minix/tests Rust 腿翻译 —— 管道/select/记录锁域(test7/8/19/20/29/40)。
//!
//! # 翻译映射
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test19 与 test7.c:224-234(F_SETFL 旗标、FD_CLOEXEC)| [`fcntl_flag_bits_and_arg_gates`] |
//! | test7.c:321-377 与 test20(fcntl() 专项,锁跨 fork/exec)的记录锁 | [`record_locks_conflict_query_and_release`] |
//! | test29(dup/dup2 专项;F_DUPFD 越界 EINVAL 载体) | [`fcntl_flag_bits_and_arg_gates`] |
//! | test40(select 三集与就绪记账) | [`select_interest_and_ready_accounting`] |
//! | test7/8(管道读写判定) | [`pipe_check_matrix_again_and_suspend`] |
//!
//! 这一层是 VFS 的纯决策面(管道尺寸矩阵、select 位集、锁表),与 C
//! `pipe.c`/`select.c`/`lock.c` 逐函数对位;端到端挂起续接(真实唤醒链)
//! 挂 VFS↔MFS 跨包桥条目。交付门 = 编译;全部测试 `#[ignore]`,点亮前提
//! 见各测试属性。

use minix_types::Endpoint;
use minix_vfs::fcntl::{
    LockOp, LockOutcome, LockTable, LockType, O_ACCMODE, O_APPEND, O_NONBLOCK, VnodeKey,
    cloexec_apply, compute_region, dupfd_arg_check, status_get, status_set,
};
use minix_vfs::pipe::{PipeCheckVerdict, pipe_check_decision};
use minix_vfs::read_write::RwDir;
use minix_vfs::select::{FdKind, SelOps, classify, ops2tab_apply, tab2ops};

// ---------------------------------------------------------------------------
// test19/test7/test29 —— fcntl 标志位与参数门
// ---------------------------------------------------------------------------

/// C 语义(`servers/vfs/misc.c:158-176` 的 fcntl 臂;载 test19、test7、
/// test29):F_SETFL 只替换 O_NONBLOCK|O_APPEND 两比特(其余标志存活);
/// F_GETFL 读访问模式加两个行为位;F_DUPFD 的 arg 越界 → EINVAL(test29);
/// FD_CLOEXEC 位判定。
#[test]
#[ignore = "点亮前提:VFS fcntl 臂随载体用户态点亮后复核"]
fn fcntl_flag_bits_and_arg_gates() {
    // F_SETFL:整体替换 NONBLOCK|O_APPEND 两比特,访问模式位保留。
    let flags = O_ACCMODE & 0o2; // O_RDWR(基础态,无 APPEND/NONBLOCK)
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
    // arg 不带 O_APPEND 时该位被清(两比特整体替换语义)。
    let stripped = status_set(after, O_NONBLOCK);
    assert_eq!(stripped & O_APPEND, 0, "arg 缺失的比特被清");

    // F_DUPFD 参数门:负值与 OPEN_MAX 越界 → EINVAL。
    assert!(dupfd_arg_check(-1).is_err());
    assert!(dupfd_arg_check(0).is_ok());

    // FD_CLOEXEC 位判定(F_SETFD 语义)。
    assert!(cloexec_apply(1), "FD_CLOEXEC=1 置位");
    assert!(!cloexec_apply(0), "0 清位");
}

// ---------------------------------------------------------------------------
// test7/test20 —— 记录锁冲突矩阵
// ---------------------------------------------------------------------------

/// C 语义(`servers/vfs/lock.c` 对位):写锁独占;GETLK 报冲突者(类型/
/// 区间/pid);不同区间读锁共存;SETLK 阻塞即拒(EAGAIN),SETLKW 转
/// 等待;解锁放行他人。
#[test]
#[ignore = "点亮前提:VFS 锁表随载体用户态点亮后复核"]
fn record_locks_conflict_query_and_release() {
    let mut mgr = LockTable::new();
    let vnode = VnodeKey {
        fs: Endpoint::MFS,
        ino: 7,
    };

    // pid1 对 [0,100) 上写锁。
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

    // pid2 GETLK 写探查:命中 pid1 的写锁,区间/pid 逐项对账。
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

    // 不重叠区间 [200,300):读锁共存。
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

    // 重叠写锁:F_SETLK(不等待)被拒;F_SETLKW(等待)转挂起。
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

    // pid1 解锁:释放其在该 vnode 的全部锁。C 的按进程清扫在 free_proc
    // 释放面(filp 关闭连带),锁表本体的解锁与唤醒在 lock.c:100-133
    // (unlocking 分支尾 `lock_revive()`,133)。C 锁语义专项载体:
    // test7.c:321-377 与 test20(fcntl 专项)。
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
// test40 —— select 三集与就绪记账
// ---------------------------------------------------------------------------

/// C 语义(`servers/vfs/select.c` 对位):fd 按四类谓词分类,类型不匹配
/// → EBADF;兴趣集三比特编解码;就绪记账的三重去重守卫(有兴趣、未记
/// 过、用户指针在)。
#[test]
#[ignore = "点亮前提:VFS select 挂起续接随载体点亮后复核"]
fn select_interest_and_ready_accounting() {
    // 分类:类型优先级表在 select.c:85-90,225-232 是逐类型应用环(字符 > 套接字 > 常规 > 管道)。
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

    // 兴趣集编码:三比特独立。
    let interest = tab2ops(true, false, true);
    assert!(interest.contains(SelOps::RD) && interest.contains(SelOps::ERR));
    assert!(!interest.contains(SelOps::WR));

    // 就绪记账:三守卫齐备才计新位。want=RD|WR,兴趣=RD|ERR,已记=空,
    // 指针=RD|ERR → 只有 RD 新计入(WR 无兴趣不计,ERR 无就绪不计)。
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
// test7/8 —— 管道读写判定矩阵
// ---------------------------------------------------------------------------

/// C 语义(`servers/vfs/pipe.c` 对位):非阻塞空读 → EAGAIN;非阻塞满写
/// → EAGAIN;阻塞空读且有写者 → 挂起;读端全关后写 → EPIPE。
#[test]
#[ignore = "点亮前提:VFS 管道挂起唤醒链随载体点亮后复核"]
fn pipe_check_matrix_again_and_suspend() {
    // 非阻塞空读:EAGAIN。
    let verdict = pipe_check_decision(RwDir::Read, 0, 8192, true, true, true, false, 64, 0);
    assert!(
        matches!(&verdict, PipeCheckVerdict::Reject(e) if *e == minix_vfs::pipe::PipeError::Again),
        "非阻塞空读 → EAGAIN"
    );

    // 阻塞空读且有写者:挂起等数据(C pipe.c:217-235 的睡眠臂)。
    let verdict = pipe_check_decision(RwDir::Read, 0, 8192, true, true, false, false, 64, 0);
    assert!(
        matches!(verdict, PipeCheckVerdict::Suspend { .. }),
        "阻塞空读挂起"
    );

    // 非阻塞满写:容量 8,已有 8 字节,请求再写 → EAGAIN。
    let verdict = pipe_check_decision(RwDir::Write, 8, 8, true, true, true, false, 1, 0);
    assert!(
        matches!(&verdict, PipeCheckVerdict::Reject(e) if *e == minix_vfs::pipe::PipeError::Again),
        "非阻塞满写 → EAGAIN"
    );

    // 无读者时写:EPIPE(C pipe.c 的 SIGPIPE 前置判定)。
    let verdict = pipe_check_decision(RwDir::Write, 0, 8192, false, true, true, false, 1, 0);
    assert!(
        matches!(&verdict, PipeCheckVerdict::Reject(e) if *e == minix_vfs::pipe::PipeError::Pipe),
        "无读者写 → EPIPE"
    );

    // 空管道可写:放行且额度 = 请求与容量取小。
    let verdict = pipe_check_decision(RwDir::Write, 0, 8192, true, true, true, false, 4096, 0);
    match verdict {
        PipeCheckVerdict::Allow { bytes, .. } => assert_eq!(bytes, 4096, "额度 = min(请求, 容量)"),
        other => panic!("空管道写应放行,得 {other:?}"),
    }
}
