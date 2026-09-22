//! minix3/minix/tests Rust 腿翻译 —— 进程生命周期域(test1/2/12/13/70)。
//!
//! # 翻译映射
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test70(两进程对各自临时文件并发 lseek 的消息字段竞争回归) | 本文件 [`lseek_alternating_processes_keep_shared_offset_consistent`](C 的两进程各持独立文件;Rust VFS 单线程事件循环无竞争对象,钉的是字段逐条对账;测试额外覆盖 fork 共享 filp 后单一偏移的 POSIX 继承语义) |
//! | test13(pipe 加 fork 的描述符继承吞吐面) | 本文件 [`fork_shares_filp_entries_and_bumps_counts`](fd 继承面;pipe 语义本体归 `t_pipe_select_locks.rs`) |
//! | test1/2/12(fork/wait/僵尸) | 已由 `pm_vm_fork.rs`(fork 全链)与 `servers/pm/tests/run_once_integration.rs`(exit 不回复僵尸化、wait4 回收带状态、ECHILD)覆盖,不重复断言 |
//!
//! # 翻译即发现的 C↔Rust 语义偏差(记录,不改产品代码)
//!
//! C `misc.c:616-617`:pm_fork 对每个非空 `fp_filp[i]` 执行 `filp_count++`;
//! C `misc.c:632-633`:对 `fp_rd`/`fp_wd` 各执行 `dup_vnode`。Rust 侧
//! `servers/vfs/src/ipc/dispatcher.rs` 的 `copy_fproc` 只拷贝索引,两处计数
//! 均未递增。测试 A 把 C 语义钉为断言——点亮时若失败,即该偏差的复核点,
//! 须先修产品代码或经评审裁决,不得改断言迁就实现。
//!
//! 交付门 = 编译;全部测试 `#[ignore]`,点亮前提见各测试属性。

use minix_types::{Endpoint, Message, MessageM7, MessageUnion, UserSlot};
use minix_vfs::PmHandler;
use minix_vfs::call_table::{SyscallResult, VfsCallNum};
use minix_vfs::main_loop::VfsState;
use minix_vfs::vnode::VnodeId;
use minix_vfs::worker::WorkerFunc;

// ---------------------------------------------------------------------------
// 夹具:复刻 crate 内 `seeded` 的公开 API 形态(syscalls.rs:5006)
// ---------------------------------------------------------------------------

const PARENT_PID: i32 = 100;
const CHILD_PID: i32 = 101;

fn ep_of(slot: usize) -> Endpoint {
    Endpoint::from_generation_slot(1, slot as i32)
}

fn parent_ep() -> Endpoint {
    ep_of(0)
}

fn child_ep() -> Endpoint {
    ep_of(1)
}

fn seeded_vfs_state() -> VfsState {
    let mut state = VfsState::new();
    {
        let fp = state.fproc_table.get_mut(UserSlot::new(0)).expect("slot 0");
        fp.pid = PARENT_PID;
        fp.endpoint = parent_ep();
    }
    state.current_fp_slot = Some(UserSlot::new(0));
    state.initialized = true;
    state
}

/// 组一条 lseek 请求(C `ipc.h:725-731`:offset 64 位拆 m7i1/m7i2,fd @m7i3,
/// whence @m7i4;SEEK_SET = 0)。
fn lseek_msg(slot: usize, offset: i64, fd: i32) -> Message {
    Message {
        m_source: ep_of(slot),
        m_type: VfsCallNum::Lseek as i32,
        m_u: MessageUnion {
            m_m7: MessageM7 {
                m7i1: offset as i32,
                m7i2: (offset >> 32) as i32,
                m7i3: fd,
                m7i4: 0,
                ..Default::default()
            },
        },
    }
}

// ---------------------------------------------------------------------------
// test13 + C misc.c:616-617/:632-633 —— fork 的描述符继承与共享计数
// ---------------------------------------------------------------------------

/// C 语义:子进程继承父的 fd 表(同一 filp 索引),每个继承项 `filp_count++`,
/// root/working 目录 vnode 各 `dup_vnode`(引用计数 +1)。
#[test]
#[ignore = "点亮前提:copy_fproc 按 C misc.c:617/:632-633 补两处计数递增(当前缺,见文件头差异记录)"]
fn fork_shares_filp_entries_and_bumps_counts() {
    let mut state = seeded_vfs_state();
    let fid = state.filp_table.alloc_filp(0o644).expect("filp 表有空位");
    state.filp_table.inc_count(fid); // 父进程打开:count = 1
    {
        let fp = state.fproc_table.get_mut(UserSlot::new(0)).expect("slot 0");
        fp.filps[3] = Some(fid.get());
        fp.root_dir = Some(0);
        fp.work_dir = Some(0);
    }
    {
        let v = state.vnode_table.get_mut(VnodeId(0)).expect("vnode slot 0");
        // 父进程 rd/wd 同指 vnode 0:引用计数各持一次,合计 2。
        v.ref_count = 2;
        v.mode = 0o040755; // 目录
    }

    let reply = minix_vfs::VfsPmHandler {
        table: &mut state.fproc_table,
    }
    .handle(minix_types::VfsCall::Fork {
        child: child_ep(),
        parent: parent_ep(),
        child_pid: CHILD_PID,
    })
    .expect("VFS 接受 PM 的 fork 通知");
    assert!(matches!(reply, minix_types::VfsReply::Fork));

    let parent = state.fproc_table.get(UserSlot::new(0)).expect("slot 0");
    let child = state.fproc_table.get(UserSlot::new(1)).expect("slot 1");
    assert_eq!(
        child.filps[3], parent.filps[3],
        "继承 = 同一 filp 索引(C misc.c:604-608 整体拷贝 + 616-617 计数)"
    );
    assert_eq!(child.filps[3], Some(fid.get()));
    assert_eq!(
        state.filp_table.get(fid).expect("filp 存在").count,
        2,
        "C misc.c:617:父开 1 次 + 子继承 1 次 = filp_count 2"
    );
    assert_eq!(
        state
            .vnode_table
            .get(VnodeId(0))
            .expect("vnode 存在")
            .ref_count,
        4,
        "C misc.c:632-633:父持 2 + rd/wd 各 dup 一次 = 4"
    );
}

// ---------------------------------------------------------------------------
// test70 —— 父子进程对同一文件的交替 lseek
// ---------------------------------------------------------------------------

/// C test70(`minix3/minix/tests/test70.c`)的动机:两进程并发 lseek 时
/// VFS 的 m_out 消息字段被竞争线程覆写导致返回错值——C 里父子各自 mkstemp
/// 独立临时文件(子 [0,1000)、父 [1000,2000),5000 轮)。Rust VFS 是单线程
/// 事件循环,竞争从架构上不存在;本测试把该动机钉成两条可执行语义:每条
/// 请求的 m7 字段独立解析、待答进程身份与请求进程一致;并以 fork 共享同一
/// filp 的夹具一并覆盖 POSIX 继承语义下的单一 `filp.pos`(C 由
/// `misc.c` pm_fork 的 filp 共继承承载,编号套件无共享偏移专项)。规模按
/// 宿主预算缩为 64 轮(单线程下无竞争对象,轮数只影响对账次数)。
#[test]
#[ignore = "点亮前提:VFS 挂起路径续接(REQ_INHIBREAD → FS 回复 → 携新位置回信)真链复核"]
fn lseek_alternating_processes_keep_shared_offset_consistent() {
    let mut state = seeded_vfs_state();
    let fid = state.filp_table.alloc_filp(0o644).expect("filp 表有空位");
    state.filp_table.inc_count(fid);

    // 父子双 fproc,fd4 共享同一 filp——fork 后共享偏移的载体。
    for (slot, pid) in [(0usize, PARENT_PID), (1usize, CHILD_PID)] {
        let fp = state
            .fproc_table
            .get_mut(UserSlot::new(slot))
            .expect("slot");
        fp.pid = pid;
        fp.endpoint = ep_of(slot);
        fp.filps[4] = Some(fid.get());
    }
    {
        // C test70 的 ftruncate 临时文件:常规文件、可 seek、足够大。
        let v = state.vnode_table.get_mut(VnodeId(0)).expect("vnode slot 0");
        v.mode = 0o100644;
        v.size = 4000;
        v.fs = Endpoint::MFS;
    }
    {
        let f = state.filp_table.get_mut(fid).expect("filp 存在");
        f.vnode = Some(0);
        f.pos = 0;
    }
    // 位置变化路径要经挂载窗口发 REQ_INHIBREAD(dev 须非 NO_DEV)。
    let vmnt = state.vmnt_table.alloc().expect("vmnt 表有空位");
    {
        let m = state.vmnt_table.get_mut(vmnt).expect("vmnt 存在");
        m.fs = Endpoint::MFS;
        m.dev = 7;
    }

    // 交替轮转:子进程区间 [1,65)、父进程区间 [1001,1065)。
    // 首轮偏移从 base+1 起,保证每步都"位置变化"(挂起路径)。
    for round in 0..64i64 {
        for (slot, base) in [(1usize, 0i64), (0usize, 1000i64)] {
            let offset = base + round + 1;
            state.current_fp_slot = Some(UserSlot::new(slot));
            state.current_message = lseek_msg(slot, offset, 4);
            let worker = state
                .worker_pool
                .assign_first_fit(
                    UserSlot::new(slot),
                    WorkerFunc::DoWork,
                    &state.current_message,
                )
                .expect("worker 空闲");
            state.current_worker = Some(worker);

            let r = minix_vfs::syscalls::dispatch_syscall(&mut state, VfsCallNum::Lseek);
            assert_eq!(
                r,
                SyscallResult::Suspend,
                "位置变化 → 挂起等 REQ_INHIBREAD 续接(C open.c:642-645)"
            );
            let pos = state.filp_table.get(fid).expect("filp 存在").pos;
            assert_eq!(
                pos, offset,
                "共享 filp.pos 落位为本次请求偏移(slot {slot}, round {round})"
            );
            let pending = state.pending_fs.as_ref().expect("抑制预读已登记");
            assert_eq!(
                pending.user,
                ep_of(slot),
                "待答进程 = 请求进程(test70 的字段对账语义)"
            );
            assert_eq!(pending.req.m_type, minix_types::REQ_INHIBREAD);

            // 清场:真机上由 FS 回复驱动续接;测试里逐轮独立。
            state.pending_fs = None;
            state.current_worker = None;
            state.worker_pool.release(worker);
        }
    }
}
