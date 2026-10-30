//! E5(a) — full-chain PM↔VM↔VFS fork integration (hosted form, over the wire contract).
//!
//! This file was once commented out wholesale (self-labelled "DEPRECATED:
//! permanently disabled"); the revival condition given in that comment was: once
//! E1 (the minix-sys userland trap layer) and E2 (the `SYS_*` wrappers) landed,
//! rewrite it on the minix-sys message layer — both closed on 2026-09-16/17, and
//! this file is the hosted half of E5(a).
//!
//! # Chain surface (all three layers are real code; only VM internals are
//! replaced by the wire contract)
//!
//! 1. **PM**: `minix_pm::fork::do_fork` runs the real state machine through every
//!    step (slot scan / capacity gate / `vm_fork` / copy / pid / `tell_vfs`).
//!    Outbound messages are captured by PM's own `TestIpcTransport` — that type
//!    deliberately carries no `cfg(test)` gate because integration tests need it
//!    (`servers/pm/src/ipc/transport.rs:165-166` attests this).
//! 2. **VM**: cannot be driven in the hosted form (`minix-vm` is all `pub(crate)`
//!    and `new_for_test` is `cfg(test)`). Following the "mock IPC + wire
//!    contract" path the disabled comment prescribed: requests are re-decoded
//!    with the VM-side decoder [`VmForkIn`] and replies are built with the
//!    VM-side encoder [`VmForkOut`] — PM's hand-written encoding and VM's decoder
//!    witness each other, so what is tested is cross-service wire agreement, not
//!    hand-copied bytes. VM's internal fork work (CoW / memory face) is covered
//!    by `minix-vm`'s own unit tests.
//! 3. **VFS**: after asserting the captured `VFS_PM_FORK` field by field, the
//!    message fields reconstruct `VfsCall::Fork` and hand it to the real mainline
//!    entry `minix_vfs::VfsPmHandler::handle` (the same arm as
//!    `dispatcher.rs:179-185`) to consume, asserting the child fproc's copy
//!    semantics (endpoint / `child_pid` / credentials / shared filp table) — the
//!    `child_pid` comes from the m7i3 PM encoded into the message, so the loop
//!    stays closed.
//!
//! # Coverage
//!
//! - Success chain: PM→VM→VFS all the way through, with each side's products
//!   asserted item by item;
//! - VM refusal rollback: a failed `vm_fork` takes no slot and does not notify
//!   VFS (forkexit.c:78-79 semantics).
//!
//! The real-machine chain (with VM actually participating) is pursued separately
//! as T2 / E5; the old redundant dead shell `pm_vm_fork_test.rs` was deleted with
//! this batch (a commented-out copy of the same content; the revived product is
//! this file).

use minix_pm::TestIpcTransport;
use minix_pm::exit::KernelGateway;
use minix_vfs::vnode::FsCtl;

/// No-op vnode return channel (the hosted stand-in for REQ_PUTNODE at the zero-reference boundary crossing).
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
use minix_pm::fork::{ForkCoordError, do_fork};
use minix_pm::mproc::{Lifecycle, ProcTable};
use minix_types::{
    DecodeFromM1, ENOMEM, EncodeToM1, Endpoint, Message, OK, UserSlot, VFS_PM_FORK, VM_FORK,
    VmForkIn, VmForkOut,
};
use minix_vfs::PmHandler;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Parent pid (fixture value, same shape as PM's internal `create_test_table_with_parent`).
const PARENT_PID: i32 = 100;

/// Parent fixture: slot 0 is in use and Running; the remaining slots are empty.
fn table_with_parent() -> ProcTable {
    let mut table = ProcTable::new();
    let parent = table.get_mut(0).expect("slot 0 exists");
    parent.identity.endpoint = parent_endpoint();
    parent.identity.id.pid = PARENT_PID;
    parent.state.lifecycle = Lifecycle::Running;
    table
}

fn parent_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, 0)
}

/// Child endpoint: slot 1, generation 1 — VM's `sys_fork` uses the slot PM passed in and bumps the generation.
fn child_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, 1)
}

fn child_slot() -> UserSlot {
    UserSlot::new(1)
}

/// VM-side fork reply (OK plus the child endpoint in m1i3), built with VM's own encoder.
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

/// No-op kernel gateway: on this fixture the fork chain never reaches the kernel face (no tracer is set, so the sys_* calls are unreachable).
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

// ---------------------------------------------------------------------------
// E5(a).1 Success chain: PM state machine → VM_FORK (wire re-decode) → VFS_PM_FORK (field by field) → real VFS handling
// ---------------------------------------------------------------------------

#[test]
fn fork_chain_pm_vm_vfs_all_real() {
    let mut table = table_with_parent();
    let mut transport = TestIpcTransport::new();
    transport.queue_sendrec_reply(vm_fork_ok_reply(child_endpoint()));
    let mut kern = NoopKernelGateway;

    let child_pid = do_fork(&mut table, parent_endpoint(), &mut transport, &mut kern)
        .expect("VM 应答 OK：全链应成功");

    // ── Outbound message one: VM_FORK (PM's hand-written encoding ↔ VM decoder witness each other) ──
    let sent = transport.sent();
    assert_eq!(sent.len(), 2, "do_fork 出站恰两条：VM_FORK + VFS_PM_FORK");
    let (vm_dest, vm_req) = &sent[0];
    assert_eq!(*vm_dest, Endpoint::VM);
    assert_eq!(vm_req.m_type, VM_FORK as i32);
    let req = VmForkIn::decode(unsafe { &vm_req.m_u.m_m1 });
    assert_eq!(
        req.parent_endpoint,
        parent_endpoint(),
        "VMF_ENDPOINT = m1i1"
    );
    assert_eq!(req.child_slot, child_slot(), "VMF_SLOTNO = m1i2");

    // ── Child slot (PM side) ──
    let child = table.get(child_slot().get()).expect("child slot exists");
    assert!(matches!(child.state.lifecycle, Lifecycle::Running));
    assert_eq!(child.identity.endpoint, child_endpoint());
    assert_eq!(child.identity.id.pid, child_pid);
    assert_ne!(child_pid, PARENT_PID, "子 pid 必须是新分配");
    assert_eq!(
        table.get(0).expect("parent slot").identity.id.pid,
        PARENT_PID,
        "父槽 pid 不被改写"
    );

    // ── Outbound message two: VFS_PM_FORK (the tell_vfs payload's m7 field by field) ──
    let (vfs_dest, vfs_msg) = &sent[1];
    assert_eq!(*vfs_dest, Endpoint::VFS);
    assert_eq!(vfs_msg.m_type, VFS_PM_FORK);
    let m7 = unsafe { vfs_msg.m_u.m_m7 };
    assert_eq!(m7.m7i1, child_endpoint().get(), "m7i1 = 子 endpoint");
    assert_eq!(m7.m7i2, parent_endpoint().get(), "m7i2 = 父 endpoint");
    assert_eq!(m7.m7i3, child_pid, "m7i3 = 子 pid");
    assert_eq!(m7.m7i4, -1, "m7i4 = -1（Fork 变体固定值）");
    assert_eq!(m7.m7i5, -1, "m7i5 = -1（Fork 变体固定值）");

    // ── VFS side: the real PM protocol entry consumes the same message (the m7
    // fields reconstruct VfsCall) ──
    // `VfsPmHandler::handle(VfsCall::Fork)` is VFS mainline's dispatch arm for PM
    // notifications (dispatcher.rs:179-185); child_pid comes from the m7i3 PM
    // encoded into the message.
    let mut fproc = minix_vfs::fproc::FProcTable::new();
    {
        let parent = fproc
            .get_mut(UserSlot::new(0))
            .expect("vfs parent slot exists");
        parent.pid = PARENT_PID;
        parent.endpoint = parent_endpoint();
        parent.real_uid = 1001;
        parent.umask = 0o022;
        parent.filps[0] = Some(7);
    }
    let child_ep = Endpoint(m7.m7i1);
    let mut filps = minix_vfs::filp::FilpTable::new();
    let mut vnodes = minix_vfs::vnode::VnodeTable::new();
    let mut ctl = NopCtl;
    let reply = minix_vfs::VfsPmHandler {
        table: &mut fproc,
        filp_table: &mut filps,
        vnode_table: &mut vnodes,
        fs_ctl: &mut ctl,
    }
    .handle(minix_types::VfsCall::Fork {
        child: child_ep,
        parent: Endpoint(m7.m7i2),
        child_pid: m7.m7i3,
    })
    .expect("VFS 应接受 PM 的 fork 通知");
    assert!(
        matches!(reply, minix_types::VfsReply::Fork),
        "分发臂回 VfsReply::Fork"
    );

    let child_fp = fproc
        .get(UserSlot::new(child_ep.slot() as usize))
        .expect("vfs child slot exists");
    assert_eq!(child_fp.endpoint, child_endpoint(), "子 fproc 端点落位");
    assert_eq!(
        child_fp.pid, child_pid,
        "子 pid = PM 编入消息的 m7i3（真链路 cpid，非 legacy 沿用父 pid）"
    );
    assert_eq!(child_fp.real_uid, 1001, "凭证块拷贝");
    assert_eq!(child_fp.umask, 0o022, "umask 拷贝");
    assert_eq!(child_fp.filps[0], Some(7), "filp 表共享（同一表项索引）");
}

// ---------------------------------------------------------------------------
// E5(a).2 VM refusal rollback: no slot taken, no VFS notification
// ---------------------------------------------------------------------------

#[test]
fn fork_chain_vm_failure_rolls_back() {
    let mut table = table_with_parent();
    let mut transport = TestIpcTransport::new();
    // C semantics: a taskcall return value other than OK → the errno propagates
    // directly (forkexit.c:78-79), and the "cannot fail" window of forkexit.c:83
    // has not been entered yet — the PM side must leave no trace.
    transport.queue_sendrec_reply(Message {
        m_type: -ENOMEM,
        ..Message::default()
    });
    let mut kern = NoopKernelGateway;

    let err = do_fork(&mut table, parent_endpoint(), &mut transport, &mut kern)
        .expect_err("VM 拒绝时 fork 必须失败");
    assert!(matches!(err, ForkCoordError::VmError));

    // Rollback assertions: only one VM_FORK goes out; the child slot is unoccupied and no pid was allocated.
    let sent = transport.sent();
    assert_eq!(sent.len(), 1, "失败链不得通知 VFS");
    assert_eq!(sent[0].0, Endpoint::VM);
    let child = table.get(child_slot().get()).expect("slot 1 exists");
    assert!(
        matches!(child.state.lifecycle, Lifecycle::Unused),
        "vm_fork 失败不占槽（成功后才 procs_in_use++）"
    );
    assert_eq!(child.identity.id.pid, 0, "未分配 pid（PID_FREE 语义）");
}
