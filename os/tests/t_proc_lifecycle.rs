//! Rust-leg translation of minix3/minix/tests — process lifecycle domain
//! (test1/2/12/13/70).
//!
//! # Translation mapping
//!
//! | C test | Semantic home |
//! |---|---|
//! | test70 (message-field race regression where two processes lseek their own temp files concurrently) | [`lseek_alternating_processes_keep_shared_offset_consistent`] in this file (C gives each process its own file; Rust VFS is a single-threaded event loop with no shared race object, so what is pinned is field-by-field accounting; the test additionally covers the POSIX inheritance semantics of a single offset after a fork shares the filp) |
//! | test13 (descriptor-inheritance throughput face of pipe + fork) | [`fork_shares_filp_entries_and_bumps_counts`] in this file (the fd inheritance face; pipe semantics themselves live in `t_pipe_select_locks.rs`) |
//! | test1/2/12 (fork/wait/zombie) | Already covered by `pm_vm_fork.rs` (the whole fork chain) and `servers/pm/tests/run_once_integration.rs` (exit without reply then zombify, wait4 reaping with status, ECHILD); not asserted again |
//!
//! # C<->Rust semantic gap previously pinned (fixed)
//!
//! C `misc.c:616-617`: pm_fork runs `filp_count++` for every non-null
//! `fp_filp[i]`; C `misc.c:632-633`: runs `dup_vnode` for each of `fp_rd`/`fp_wd`.
//! `copy_fproc` used to copy only the indices without incrementing (the original
//! dispatcher.rs docs marked it DEFERRED to 04/05); the gap is now closed:
//! `VfsPmHandler` holds all three tables and really lands the counts in its fork
//! arm.
//!
//! Delivery gate = compilation; every test is `#[ignore]`; lighting-up
//! preconditions are on each test's attribute.

use minix_types::{Endpoint, Message, MessageM7, MessageUnion, UserSlot};
use minix_vfs::PmHandler;
use minix_vfs::call_table::{SyscallResult, VfsCallNum};
use minix_vfs::main_loop::VfsState;
use minix_vfs::vnode::{FsCtl, VnodeId};

/// No-op vnode return channel (the host stand-in for REQ_PUTNODE when the reference
/// count crosses zero).
struct NopCtl;
impl FsCtl for NopCtl {
    fn put_node(
        &mut self,
        _fs: Endpoint,
        _ino: u64,
        _count: usize,
    ) -> Result<(), minix_vfs::vnode::VnodeError> {
        Ok(())
    }
}
use minix_vfs::worker::WorkerFunc;

// ---------------------------------------------------------------------------
// Fixture: reproduces the public API shape of the crate's `seeded` (syscalls.rs:5006)
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

/// Build one lseek request (C `ipc.h:725-731`: the 64-bit offset splits into
/// m7i1/m7i2, fd @m7i3, whence @m7i4; SEEK_SET = 0).
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
// test13 + C misc.c:616-617/:632-633 — fork's descriptor inheritance and shared
// counts
// ---------------------------------------------------------------------------

/// C semantics: the child inherits the parent's fd table (the same filp indices);
/// each inherited entry gets `filp_count++`, and the root/working directory vnodes
/// each get `dup_vnode` (reference count +1).
#[test]
fn fork_shares_filp_entries_and_bumps_counts() {
    let mut state = seeded_vfs_state();
    let fid = state.filp_table.alloc_filp(0o644).expect("filp 表有空位");
    state.filp_table.inc_count(fid); // parent opened it: count = 1
    {
        let fp = state.fproc_table.get_mut(UserSlot::new(0)).expect("slot 0");
        fp.filps[3] = Some(fid.get());
        fp.root_dir = Some(0);
        fp.work_dir = Some(0);
    }
    {
        let v = state.vnode_table.get_mut(VnodeId(0)).expect("vnode slot 0");
        // The parent's rd/wd both point at vnode 0: one reference each, 2 total.
        v.ref_count = 2;
        v.mode = 0o040755; // directory
    }

    let mut ctl = NopCtl;
    let reply = minix_vfs::VfsPmHandler {
        table: &mut state.fproc_table,
        filp_table: &mut state.filp_table,
        vnode_table: &mut state.vnode_table,
        fs_ctl: &mut ctl,
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

/// C semantics (misc.c:651-660): child exit decrements each fd through `close_fd`
/// and calls `put_vnode` for rd/wd — the shared counts bumped at fork are fully
/// returned after exit.
#[test]
fn exit_releases_inherited_counts_back_to_pre_fork() {
    let mut state = seeded_vfs_state();
    let fid = state.filp_table.alloc_filp(0o644).expect("filp 表有空位");
    state.filp_table.inc_count(fid);
    {
        let fp = state.fproc_table.get_mut(UserSlot::new(0)).expect("slot 0");
        fp.filps[3] = Some(fid.get());
        fp.root_dir = Some(0);
        fp.work_dir = Some(0);
    }
    {
        let v = state.vnode_table.get_mut(VnodeId(0)).expect("vnode slot 0");
        v.ref_count = 2;
        v.mode = 0o040755;
    }

    let mut ctl = NopCtl;
    let reply = minix_vfs::VfsPmHandler {
        table: &mut state.fproc_table,
        filp_table: &mut state.filp_table,
        vnode_table: &mut state.vnode_table,
        fs_ctl: &mut ctl,
    }
    .handle(minix_types::VfsCall::Fork {
        child: child_ep(),
        parent: parent_ep(),
        child_pid: CHILD_PID,
    })
    .expect("fork 通知接受");
    assert!(matches!(reply, minix_types::VfsReply::Fork));
    assert_eq!(state.filp_table.get(fid).expect("filp 存在").count, 2);
    assert_eq!(
        state
            .vnode_table
            .get(VnodeId(0))
            .expect("vnode 存在")
            .ref_count,
        4
    );

    // Child exit: the inherited counts are fully returned (C misc.c:651-660).
    minix_vfs::VfsPmHandler {
        table: &mut state.fproc_table,
        filp_table: &mut state.filp_table,
        vnode_table: &mut state.vnode_table,
        fs_ctl: &mut NopCtl,
    }
    .handle(minix_types::VfsCall::Exit {
        endpoint: child_ep(),
    })
    .expect("VFS 接受 PM 的 exit 通知");

    assert_eq!(
        state.filp_table.get(fid).expect("filp 存在").count,
        1,
        "子退出归还继承的 filp 引用"
    );
    assert_eq!(
        state
            .vnode_table
            .get(VnodeId(0))
            .expect("vnode 存在")
            .ref_count,
        2,
        "子退出租还 rd/wd 的 vnode 引用"
    );
}

// ---------------------------------------------------------------------------
// test70 — parent and child alternating lseek on the same file
// ---------------------------------------------------------------------------

/// The motivation of C test70 (`minix3/minix/tests/test70.c`): when two processes
/// lseek concurrently, VFS's m_out message fields get overwritten by the competing
/// thread and wrong values are returned — in C the parent and child each `mkstemp`
/// an independent temp file (child [0,1000), parent [1000,2000), 5000 rounds). Rust
/// VFS is a single-threaded event loop, so the race does not exist by construction;
/// this test pins that motivation as two executable semantics: each request's m7
/// fields are parsed independently, and the process awaiting the answer is the
/// process that made the request. It also uses a fork sharing one filp to cover the
/// single `filp.pos` under POSIX inheritance semantics (C carries that through
/// pm_fork's filp co-inheritance in `misc.c`; the numbered suite has no
/// shared-offset case of its own). The scale is reduced to 64 rounds for the host
/// budget (with no race object in a single thread, the round count only affects how
/// many times the accounting runs).
#[test]
#[ignore = "点亮前提:VFS 挂起路径续接(REQ_INHIBREAD → FS 回复 → 携新位置回信)真链复核"]
fn lseek_alternating_processes_keep_shared_offset_consistent() {
    let mut state = seeded_vfs_state();
    let fid = state.filp_table.alloc_filp(0o644).expect("filp 表有空位");
    state.filp_table.inc_count(fid);

    // Two fprocs, parent and child, sharing one filp at fd4 — the carrier of the
    // shared offset after fork.
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
        // C test70's ftruncate'd temp file: a regular file, seekable, large enough.
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
    // The position-change path must go through a mount window to send REQ_INHIBREAD
    // (dev must not be NO_DEV).
    let vmnt = state.vmnt_table.alloc().expect("vmnt 表有空位");
    {
        let m = state.vmnt_table.get_mut(vmnt).expect("vmnt 存在");
        m.fs = Endpoint::MFS;
        m.dev = 7;
    }

    // Alternating rounds: child range [1,65), parent range [1001,1065).
    // The first offset starts at base+1, so every step counts as a "position
    // change" (the suspend path).
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

            // Cleanup: on real hardware the FS reply drives continuation; here each
            // round is independent.
            state.pending_fs = None;
            state.current_worker = None;
            state.worker_pool.release(worker);
        }
    }
}
