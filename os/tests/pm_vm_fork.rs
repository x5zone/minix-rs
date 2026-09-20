//! E5(a) — PM↔VM↔VFS fork 全链路联调（宿主态，走 wire 契约）。
//!
//! 本文件曾整体注释停用（自注 "DEPRECATED: permanently disabled"），停用
//! 注释给出的复活条件是：等 E1（minix-sys 用户态 trap 层）与 E2（`SYS_*`
//! wrapper）落地后，改走 minix-sys 消息层重写——两者已于 2026-09-16/17
//! 闭环（edge_todo.md E1/E2），本文件即 E5(a) 的宿主半（edge4.md §5）。
//!
//! # 链路面（三层全真代码，只有 VM 内部以 wire 契约代替）
//!
//! 1. **PM**：`minix_pm::fork::do_fork` 真实状态机跑全步（槽位扫描 / 容量门 /
//!    `vm_fork` / 拷贝 / pid / `tell_vfs`）。出站消息由 PM 自带的
//!    `TestIpcTransport` 捕获——该类型按集成测试需要刻意不做 `cfg(test)`
//!    门控（`servers/pm/src/ipc/transport.rs:165-166` 自证）。
//! 2. **VM**：宿主态不可驱动（`minix-vm` 模块全 `pub(crate)`，`new_for_test`
//!    为 `cfg(test)`）。按停用注释指定的 "mock IPC + wire 契约" 路径：请求用
//!    VM 侧解码器 [`VmForkIn`] 回解、应答用 VM 侧编码器 [`VmForkOut`] 构造
//!    ——PM 手写编码与 VM 解码器互为见证，测的是跨服务 wire 对账（而非
//!    手抄字节）。VM 内部 fork 处理（CoW/内存面）由 `minix-vm` 自身单测覆盖。
//! 3. **VFS**：捕获到的 `VFS_PM_FORK` 逐域断言后，以消息字段还原
//!    `VfsCall::Fork` 并交给真实主线入口 `minix_vfs::VfsPmHandler::handle`
//!    （与 `dispatcher.rs:179-185` 同臂）消费，断言子 fproc 的拷贝语义
//!    （端点 / `child_pid` / 凭证 / filp 表共享）——`child_pid` 取自 PM
//!    编入消息的 m7i3，闭环不断。
//!
//! # 覆盖
//!
//! - 成功链：PM→VM→VFS 全通，三侧产物逐项断言；
//! - VM 拒绝回滚：`vm_fork` 失败不占槽、不通知 VFS（forkexit.c:78-79 语义）。
//!
//! 真机链路（VM 真实参战）挂 edge4 T2 / E5；旧冗余死壳
//! `pm_vm_fork_test.rs` 随本批删除（同内容注释件，复活产物即本文件）。

use minix_pm::exit::KernelGateway;
use minix_pm::fork::{do_fork, ForkCoordError};
use minix_pm::mproc::{Lifecycle, ProcTable};
use minix_pm::TestIpcTransport;
use minix_vfs::PmHandler;
use minix_types::{
    DecodeFromM1, EncodeToM1, Endpoint, Message, UserSlot, VmForkIn, VmForkOut, ENOMEM, OK,
    VFS_PM_FORK, VM_FORK,
};

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

/// 父进程 pid（夹具值，与 PM 内部 `create_test_table_with_parent` 同形）。
const PARENT_PID: i32 = 100;

/// 父进程夹具：槽 0 占用、Running，其余槽为空。
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

/// 子端点：槽 1、代 1——VM 的 `sys_fork` 使用 PM 传入的槽位并递增代际。
fn child_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, 1)
}

fn child_slot() -> UserSlot {
    UserSlot::new(1)
}

/// VM 侧 fork 应答（OK + 子 endpoint 在 m1i3），用 VM 自己的编码器构造。
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

/// 无操作内核网关：fork 链在夹具上不触内核面（tracer 未设，sys_* 不可达）。
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
    fn get_ksig(&mut self) -> Result<Option<(Endpoint, u64)>, i32> {
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
}

// ---------------------------------------------------------------------------
// E5(a).1 成功链：PM 状态机 → VM_FORK（wire 回解）→ VFS_PM_FORK（逐域）→ VFS 真处理
// ---------------------------------------------------------------------------

#[test]
fn fork_chain_pm_vm_vfs_all_real() {
    let mut table = table_with_parent();
    let mut transport = TestIpcTransport::new();
    transport.queue_sendrec_reply(vm_fork_ok_reply(child_endpoint()));
    let mut kern = NoopKernelGateway;

    let child_pid = do_fork(&mut table, parent_endpoint(), &mut transport, &mut kern)
        .expect("VM 应答 OK：全链应成功");

    // ── 出站消息一：VM_FORK（PM 手写编码 ↔ VM 解码器互证）──
    let sent = transport.sent();
    assert_eq!(sent.len(), 2, "do_fork 出站恰两条：VM_FORK + VFS_PM_FORK");
    let (vm_dest, vm_req) = &sent[0];
    assert_eq!(*vm_dest, Endpoint::VM);
    assert_eq!(vm_req.m_type, VM_FORK as i32);
    let req = VmForkIn::decode(unsafe { &vm_req.m_u.m_m1 });
    assert_eq!(req.parent_endpoint, parent_endpoint(), "VMF_ENDPOINT = m1i1");
    assert_eq!(req.child_slot, child_slot(), "VMF_SLOTNO = m1i2");

    // ── 子进程槽位（PM 侧）──
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

    // ── 出站消息二：VFS_PM_FORK（tell_vfs 载荷 m7 逐域）──
    let (vfs_dest, vfs_msg) = &sent[1];
    assert_eq!(*vfs_dest, Endpoint::VFS);
    assert_eq!(vfs_msg.m_type, VFS_PM_FORK);
    let m7 = unsafe { vfs_msg.m_u.m_m7 };
    assert_eq!(m7.m7i1, child_endpoint().get(), "m7i1 = 子 endpoint");
    assert_eq!(m7.m7i2, parent_endpoint().get(), "m7i2 = 父 endpoint");
    assert_eq!(m7.m7i3, child_pid, "m7i3 = 子 pid");
    assert_eq!(m7.m7i4, -1, "m7i4 = -1（Fork 变体固定值）");
    assert_eq!(m7.m7i5, -1, "m7i5 = -1（Fork 变体固定值）");

    // ── VFS 侧：真实 PM 协议入口消费同一条消息（m7 字段还原 VfsCall）──
    // `VfsPmHandler::handle(VfsCall::Fork)` 即 VFS 主线对 PM 通知的分发臂
    // （dispatcher.rs:179-185），child_pid 取自 PM 编入消息的 m7i3。
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
    let reply = minix_vfs::VfsPmHandler { table: &mut fproc }
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
// E5(a).2 VM 拒绝回滚：不占槽、不通知 VFS
// ---------------------------------------------------------------------------

#[test]
fn fork_chain_vm_failure_rolls_back() {
    let mut table = table_with_parent();
    let mut transport = TestIpcTransport::new();
    // C 语义：taskcall 返回值非 OK → errno 直接传播（forkexit.c:78-79），
    // 且此时尚未进入 forkexit.c:83 的"不可失败窗口"——PM 侧不得留痕。
    transport.queue_sendrec_reply(Message {
        m_type: -ENOMEM,
        ..Message::default()
    });
    let mut kern = NoopKernelGateway;

    let err = do_fork(&mut table, parent_endpoint(), &mut transport, &mut kern)
        .expect_err("VM 拒绝时 fork 必须失败");
    assert!(matches!(err, ForkCoordError::VmError));

    // 回滚断言：只发出 VM_FORK 一条；子槽未占、pid 未分配。
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
