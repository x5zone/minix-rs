//! E5(c) forward section — full-chain `srv_fork` integration across RS→PM→VM→VFS (hosted form, over the wire contract).
//!
//! RS uses `srv_fork` to spawn system services. That call shares most of the flow
//! with an ordinary `fork` yet has several opposite semantics (permission gate,
//! privilege retention, credential injection, VFS payload, immediate double
//! reply). This file connects and runs the three real-code segments of that chain
//! other than VM internals: PM's state machine, the cross-service message
//! encoding, and VFS's dispatch arm. It is a sibling of
//! [`pm_vm_fork`](../pm_vm_fork.rs), which covers an ordinary `fork`; this file
//! covers `srv_fork`.
//!
//! # Chain surface (all three layers are real code; only VM internals are
//! replaced by the wire contract)
//!
//! 1. **PM**: `minix_pm::fork::do_srv_fork` runs the real state machine through
//!    every step (RS gate / capacity gate / slot scan / `vm_fork` /
//!    `srv_fork_from` copy / pid / `tell_vfs` / immediate reply). Outbound
//!    messages are captured by PM's own `TestIpcTransport` — that type
//!    deliberately carries no `cfg(test)` gate for integration tests
//!    (`servers/pm/src/ipc/transport.rs:165-166` attests this).
//! 2. **VM**: cannot be driven in the hosted form (`minix-vm` exports only
//!    `VmServer` and the boot types; modules such as `vfs_queue`/`pagetable` are
//!    all `pub(crate)`). The VM segment therefore takes part through the wire
//!    contract: requests are re-decoded with the VM-side decoder [`VmForkIn`] and
//!    replies are built with the VM-side encoder [`VmForkOut`] — PM's encoding
//!    and VM's decoding witness each other, so what is tested is cross-service
//!    wire agreement, not hand-copied bytes. VM's internal address-space copy is
//!    covered by `minix-vm`'s own unit tests.
//! 3. **VFS**: after asserting the captured `VFS_PM_SRV_FORK` field by field, the
//!    message fields reconstruct `VfsCall::SrvFork` and hand it to the real
//!    mainline entry `minix_vfs::VfsPmHandler::handle` (the same arm as
//!    `servers/vfs/src/ipc/dispatcher.rs:187-198`) to consume. That arm appends
//!    the two steps `setuid`/`setgid` after copying the fd table (the `SRV_FORK`
//!    special case at C `misc.c:869`), so the child fproc's credentials must
//!    equal the message's `REUID`/`REGID`.
//!
//! # Coverage
//!
//! - RS gate rejection: a call from a non-RS endpoint returns `NotPermitted`,
//!   and **there is not a single outbound message** — the gate runs before
//!   `vm_fork` (`forkexit.c:159-160`, Rust `fork.rs:125-127`);
//! - Success chain: PM→VM→VFS all the way through, with the three outbound
//!   messages (VM_FORK / VFS_PM_SRV_FORK / the OK to the child process) asserted
//!   item by item, along with the child slot's credentials and scheduling
//!   context and the VFS-side child fproc.
//!
//! The real-machine chain (VM actually participating, RS actually initiating) is
//! pursued separately as T2 / E5(c).

use minix_pm::TestIpcTransport;
/// No-op vnode return channel (the hosted stand-in for REQ_PUTNODE at the zero-reference boundary crossing).
struct NopCtl;
impl minix_vfs::vnode::FsCtl for NopCtl {
    fn put_node(
        &mut self,
        _fs: minix_types::Endpoint,
        _ino: u64,
        _count: usize,
    ) -> Result<(), minix_vfs::vnode::VnodeError> {
        Ok(())
    }
}
use minix_pm::exit::KernelGateway;
use minix_pm::fork::{ForkCoordError, do_srv_fork};
use minix_pm::mproc::{Credentials, Lifecycle, Privilege, ProcTable, SrvForkParams};
use minix_types::{
    DecodeFromM1, EncodeToM1, Endpoint, Gid, Message, OK, Uid, UserSlot, VFS_PM_SRV_FORK,
    VFS_PM_SRV_FORK_REPLY, VM_FORK, VfsCall, VfsReply, VmForkIn, VmForkOut,
};
use minix_vfs::PmHandler;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// RS's slot in the PM process table.
///
/// The raw value of `Endpoint::RS` is 2, and taking the low 15 bits gives slot 2
/// (`types/endpoint.rs:63`); C's `RS_PROC_NR` is likewise 2
/// (`minix3/minix/include/minix/com.h`), and PM's own unit tests also put RS at
/// `procs[2]`.
const RS_SLOT: usize = 2;

/// RS's pid (fixture value; the real allocation starting at `INIT_PID + 1` does not take part in this test).
const RS_PID: i32 = 2;

/// Child endpoint: slot 1, generation 1.
///
/// VM's `sys_fork` uses the slot PM passed in and bumps the generation, so the
/// first child process lands in slot 1, generation 1. On the PM side a
/// `debug_assert_eq!` guards that "the slot VM replies with must equal the
/// requested slot" (`fork.rs:165-169`), and this fixture agrees with it.
fn child_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, 1)
}

fn child_slot() -> UserSlot {
    UserSlot::new(1)
}

/// Parent process table: RS occupies slot 2 as Running, a system process, with
/// scheduler `NONE`; the remaining slots are empty.
///
/// RS is a system service; in C its `mp_flags` contains `PRIV_PROC` and
/// `mp_scheduler == NONE` (`04-stage-pm/16-scheduling.md:47-48` cites the
/// `schedule.c:72` assertion), so the fixture follows that.
fn table_with_rs() -> ProcTable {
    let mut table = ProcTable::new();
    let rs = table.get_mut(RS_SLOT).expect("slot 2 exists");
    rs.identity.endpoint = Endpoint::RS;
    rs.identity.id.pid = RS_PID;
    rs.identity.procgrp = RS_PID;
    rs.state.lifecycle = Lifecycle::Running;
    rs.resources.privilege = Privilege::Kernel(Credentials::default());
    rs.resources.scheduler = Endpoint::NONE;
    table
}

/// VM-side fork reply (OK plus the child endpoint in m1i3), built with VM's own encoder.
///
/// Same as the same-named fixture in `pm_vm_fork.rs`: `VmForkOut` is VM's
/// send-half encoder, borrowed here to build the reply; PM's decoding end is
/// `vm_fork` (`ipc/dispatcher.rs:137`).
fn vm_fork_ok_reply(child: Endpoint) -> Message {
    let mut reply = Message {
        m_type: OK,
        ..Message::default()
    };
    VmForkOut {
        child_endpoint: child,
    }
    .encode(unsafe { &mut reply.m_u.m_m1 });
    reply
}

/// No-op kernel gateway: on this fixture the srv_fork chain never reaches the kernel face (no tracer is set, so the `sys_*` calls are unreachable).
struct NoopKernelGateway;

impl KernelGateway for NoopKernelGateway {
    fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> {
        Ok(())
    }
    fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
    fn sys_abort(&mut self, _how: i32) -> Result<(), i32> {
        Ok(())
    }
    fn proc_times(
        &mut self,
        _ep: Endpoint,
    ) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
        Ok((0, 0))
    }
    fn copy_to_user(
        &mut self,
        _bytes: &[u8],
        _dst_ep: Endpoint,
        _dst_addr: u64,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
    fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
    fn sys_trace(
        &mut self,
        _req: i32,
        _ep: Endpoint,
        _addr: u64,
        _data: &mut i64,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn sys_vircopy(
        &mut self,
        _src_ep: Endpoint,
        _src: u64,
        _dst_ep: Endpoint,
        _dst: u64,
        _len: u64,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn copy_from_user(
        &mut self,
        _src_ep: Endpoint,
        _src: u64,
        _bytes: &mut [u8],
    ) -> Result<(), i32> {
        Ok(())
    }
    fn get_ksig(&mut self) -> Result<Option<(Endpoint, minix_types::SigSetBits)>, i32> {
        Ok(None)
    }
    fn end_ksig(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> {
        Ok(())
    }
    fn sys_sigsend(
        &mut self,
        _ep: Endpoint,
        _sigmsg: &minix_sys::syscall::SigMsgWire,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn sys_sigreturn(&mut self, _ep: Endpoint, _ctx: minix_types::VirBytes) -> Result<(), i32> {
        Ok(())
    }
    fn diag_write(&mut self, _text: &str) -> Result<(), i32> {
        Ok(())
    }
    fn sys_diagctl_stacktrace(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
}

/// Build a running user process slot (for the negative path: a caller at a non-RS endpoint).
fn table_with_user_caller() -> ProcTable {
    let mut table = ProcTable::new();
    let caller = table.get_mut(5).expect("slot 5 exists");
    caller.identity.endpoint = Endpoint::from_generation_slot(1, 5);
    caller.identity.id.pid = 100;
    caller.state.lifecycle = Lifecycle::Running;
    table
}

// ---------------------------------------------------------------------------
// E5(c) forward part 1, negative path: the RS gate — rejection with zero outbound traffic
// ---------------------------------------------------------------------------

#[test]
fn srv_fork_gate_rejects_non_rs_caller_silently() {
    let mut table = table_with_user_caller();
    let mut transport = TestIpcTransport::new();
    let mut kern = NoopKernelGateway;

    let err = do_srv_fork(
        &mut table,
        Endpoint::from_generation_slot(1, 5),
        SrvForkParams { uid: 0, gid: 0 },
        &mut transport,
        &mut kern,
    )
    .expect_err("非 RS 端点不得使用 srv_fork");

    assert!(matches!(err, ForkCoordError::NotPermitted));
    // C's gate is the first statement of do_srv_fork (forkexit.c:159-160), ahead
    // of the capacity gate, slot scan and vm_fork (183-185). A rejected call
    // therefore **produces no outbound traffic at all**: no VM_FORK, no VFS
    // notification, and no reply sent to anyone.
    assert!(
        transport.sent().is_empty(),
        "RS 门在 vm_fork 之前：拒绝路径零出站，实际 {:?}",
        transport.sent()
    );
}

// ---------------------------------------------------------------------------
// E5(c) forward part 2, success chain: PM state machine → VM_FORK (wire re-decode) → VFS_PM_SRV_FORK
//                    (field by field) → the real VFS handling arm
// ---------------------------------------------------------------------------

#[test]
fn srv_fork_chain_rs_pm_vm_vfs_all_real() {
    // The credential injection values in this fixture differ from RS's own
    // credentials; only then does the test prove "injection" rather than
    // "inheritance".
    const INJECT_UID: Uid = 1001;
    const INJECT_GID: Gid = 55;

    let mut table = table_with_rs();
    let mut transport = TestIpcTransport::new();
    transport.queue_sendrec_reply(vm_fork_ok_reply(child_endpoint()));
    let mut kern = NoopKernelGateway;

    let child_pid = do_srv_fork(
        &mut table,
        Endpoint::RS,
        SrvForkParams {
            uid: INJECT_UID,
            gid: INJECT_GID,
        },
        &mut transport,
        &mut kern,
    )
    .expect("VM 应答 OK：RS 发起的 srv_fork 全链应成功");

    // ── Outbound message overview: exactly three ──
    // C's do_srv_fork outbound order: vm_fork (183-185, synchronous sendrec) →
    // tell_vfs (224-230, asynchronous send) → reply(child, OK) (237, synchronous
    // send).
    let sent = transport.sent();
    assert_eq!(
        sent.len(),
        3,
        "srv_fork 出站恰三支：VM_FORK + VFS_PM_SRV_FORK + 给子进程的 OK"
    );

    // ── Outbound one: VM_FORK (PM's hand-written encoding ↔ VM decoder witness each other) ──
    let (vm_dest, vm_req) = &sent[0];
    assert_eq!(*vm_dest, Endpoint::VM);
    assert_eq!(vm_req.m_type, VM_FORK as i32);
    let req = VmForkIn::decode(unsafe { &vm_req.m_u.m_m1 });
    assert_eq!(req.parent_endpoint, Endpoint::RS, "VMF_ENDPOINT = m1i1");
    assert_eq!(req.child_slot, child_slot(), "VMF_SLOTNO = m1i2");

    // ── Child slot on the PM side: occupancy, endpoint, pid, VFS_CALL ──
    let child = table.get(child_slot().get()).expect("child slot exists");
    assert!(matches!(child.state.lifecycle, Lifecycle::Running));
    assert_eq!(child.identity.endpoint, child_endpoint());
    assert_eq!(child.identity.id.pid, child_pid);
    assert_ne!(child_pid, RS_PID, "子 pid 必须是新分配");
    assert_eq!(
        table.get(RS_SLOT).expect("rs slot").identity.id.pid,
        RS_PID,
        "父槽 pid 不被改写"
    );
    // VFS_CALL is set after tell_vfs succeeds (utility.c:128); the child still
    // carries this unfinished VFS call when it is woken immediately (the comment
    // at fork.rs:229-231).
    assert!(
        child.state.block.is_vfs_blocked(),
        "子进程 VFS_CALL 已置位（tell_vfs 的第三步）"
    );

    // ── Credentials: injected from SrvForkParams (versus do_fork's inheritance from the parent) ──
    let creds = child.resources.privilege.credentials();
    assert_eq!(creds.user.real, INJECT_UID, "mp_realuid = 注入 uid");
    assert_eq!(creds.user.effective, INJECT_UID, "mp_effuid = 注入 uid");
    assert_eq!(creds.user.saved, INJECT_UID, "mp_svuid = 注入 uid");
    assert_eq!(creds.group.real, INJECT_GID, "mp_realgid = 注入 gid");
    assert_eq!(creds.group.effective, INJECT_GID, "mp_effgid = 注入 gid");
    assert_eq!(creds.group.saved, INJECT_GID, "mp_svgid = 注入 gid");
    // The child process group follows the parent (`srv_fork_from` takes procgrp from the parent).
    assert_eq!(child.identity.procgrp, RS_PID, "进程组随父 RS");
    // The scheduling context of a system service: `scheduler == NONE` is kept and
    // SCHED never takes over (C forkexit.c:199-200 keeps PRIV_PROC and the
    // scheduler stays NONE; design document 08-pm-srv-fork.md §2.4 agrees).
    assert_eq!(
        child.resources.scheduler,
        Endpoint::NONE,
        "系统服务 scheduler 保持 NONE（不经 SCHED 接管）"
    );
    // PRIV_PROC is retained: C's do_srv_fork keeps `PRIV_PROC` with the inline
    // mask `rmc->mp_flags &= (IN_USE|PRIV_PROC|DELAY_CALL)` (forkexit.c:200), and
    // design document 08-pm-srv-fork.md §4.2/§D2 states that the child has
    // `is_kernel_process()==true`. `Process::srv_fork_from` (mproc/fork.rs) now
    // produces `Privilege::Kernel(injected credentials)`: `Privilege::Kernel`
    // carries the credential fields, so it keeps the privilege bit and holds the
    // six-field injection (orthogonal in C, where PRIV_PROC and mp_realuid et al.
    // are separate). The credential assertions above prove the injection took
    // effect; this assertion proves the privilege bit was retained.
    assert!(
        child.is_kernel_process(),
        "srv_fork 子进程保留 PRIV_PROC（is_kernel_process==true，C-28）"
    );

    // ── Outbound two: VFS_PM_SRV_FORK (the tell_vfs payload's m7 field by field) ──
    let (vfs_dest, vfs_msg) = &sent[1];
    assert_eq!(*vfs_dest, Endpoint::VFS);
    assert_eq!(vfs_msg.m_type, VFS_PM_SRV_FORK);
    let m7 = unsafe { vfs_msg.m_u.m_m7 };
    assert_eq!(m7.m7i1, child_endpoint().get(), "m7i1 = 子 endpoint");
    assert_eq!(m7.m7i2, Endpoint::RS.get(), "m7i2 = 父 endpoint（RS）");
    assert_eq!(m7.m7i3, child_pid, "m7i3 = 子 pid");
    assert_eq!(
        m7.m7i4, INJECT_UID as i32,
        "m7i4 = REUID 携真实 uid（do_fork 在此处固定为 -1）"
    );
    assert_eq!(
        m7.m7i5, INJECT_GID as i32,
        "m7i5 = REGID 携真实 gid（do_fork 在此处固定为 -1）"
    );

    // ── VFS side: the real PM protocol entry consumes the same message ──
    // `VfsPmHandler::handle(VfsCall::SrvFork)` is VFS mainline's dispatch arm for
    // PM notifications (dispatcher.rs:187-198): it copies the fd table in
    // handle_fork, then appends the two steps setuid / setgid (the SRV_FORK
    // special case on the C side, misc.c:869). All three inputs come from the m7
    // fields PM encoded into the message, so the chain stays closed.
    let mut fproc = minix_vfs::fproc::FProcTable::new();
    {
        let rs = fproc
            .get_mut(UserSlot::new(RS_SLOT))
            .expect("vfs rs slot exists");
        rs.pid = RS_PID;
        rs.endpoint = Endpoint::RS;
        rs.real_uid = 3;
        rs.eff_uid = 3;
        rs.real_gid = 4;
        rs.eff_gid = 4;
        rs.umask = 0o022;
        rs.filps[0] = Some(7);
    }
    let vfs_call = VfsCall::SrvFork {
        child: Endpoint(m7.m7i1),
        parent: Endpoint(m7.m7i2),
        child_pid: m7.m7i3,
        reuid: m7.m7i4,
        regid: m7.m7i5,
    };
    let mut filps = minix_vfs::filp::FilpTable::new();
    let mut vnodes = minix_vfs::vnode::VnodeTable::new();
    let mut ctl = NopCtl;
    let reply = minix_vfs::VfsPmHandler {
        table: &mut fproc,
        filp_table: &mut filps,
        vnode_table: &mut vnodes,
        fs_ctl: &mut ctl,
    }
    .handle(vfs_call)
    .expect("VFS 应接受 PM 的 srv_fork 通知");
    assert!(
        matches!(reply, VfsReply::SrvFork),
        "分发臂回 VfsReply::SrvFork"
    );
    // The reply code is exactly C's VFS_PM_SRV_FORK_REPLY (0x988); PM's
    // `handle_vfs_reply` takes the empty branch for this reply (main.c:398-401
    // `/* Nothing to do */`) and, after clearing VFS_CALL, does not restart the
    // signal — orthogonal to the two branches of the ordinary fork's
    // `VFS_PM_FORK_REPLY`.
    assert_eq!(reply.m_type(), VFS_PM_SRV_FORK_REPLY);

    let child_fp = fproc
        .get(
            child_endpoint()
                .to_user_slot()
                .expect("child endpoint is user proc"),
        )
        .expect("vfs child slot exists");
    assert_eq!(child_fp.endpoint, child_endpoint(), "子 fproc 端点落位");
    assert_eq!(child_fp.pid, child_pid, "子 pid = PM 编入消息的 m7i3");
    assert_eq!(
        child_fp.real_uid, INJECT_UID,
        "SRV_FORK 臂的追加 setuid：real_uid = REUID"
    );
    assert_eq!(
        child_fp.eff_uid, INJECT_UID,
        "SRV_FORK 臂的追加 setuid：eff_uid = REUID"
    );
    assert_eq!(
        child_fp.real_gid, INJECT_GID,
        "SRV_FORK 臂的追加 setgid：real_gid = REGID"
    );
    assert_eq!(
        child_fp.eff_gid, INJECT_GID,
        "SRV_FORK 臂的追加 setgid：eff_gid = REGID"
    );
    assert_eq!(
        child_fp.umask, 0o022,
        "fd 表复制带入父 umask（复制先于 setuid/setgid 两步）"
    );
    assert_eq!(child_fp.filps[0], Some(7), "filp 表共享（同一表项索引）");

    // ── Outbound three: the immediate OK to the child process (the reply
    // difference between `srv_fork` and `do_fork`) ──
    // `do_fork` returns SUSPEND and the two branches of VFS_PM_FORK_REPLY wake
    // parent and child asynchronously; a `srv_fork` child has scheduler==NONE and
    // never goes through SCHED, so PM sends `reply(child, OK)` directly here
    // (forkexit.c:237) and then synchronously returns the parent pid (239).
    let (child_dest, child_msg) = &sent[2];
    assert_eq!(*child_dest, child_endpoint(), "立即回复的目标是子进程");
    assert_eq!(child_msg.m_type, OK, "子进程收到 OK（不是 SUSPEND）");
}
