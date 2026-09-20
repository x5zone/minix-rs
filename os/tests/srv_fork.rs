//! E5(c) 前哨段 — RS→PM→VM→VFS 的 `srv_fork` 全链路联调（宿主态，走 wire 契约）。
//!
//! RS 用 `srv_fork` 孵化系统服务，这个调用与普通 `fork` 共享大部分流程却有几处
//! 相反的语义（权限门、特权保留、凭证注入、VFS 载荷、立即双回复）。本文件把这条
//! 链路上除 VM 内部之外的三段真实代码接起来跑：PM 的状态机、跨服务的消息编码、
//! VFS 的分发处理臂。它与 [`pm_vm_fork`](../pm_vm_fork.rs) 是同族件，后者覆盖
//! 普通 `fork`，本文件覆盖 `srv_fork`。
//!
//! # 链路面（三层全真代码，只有 VM 内部以 wire 契约代替）
//!
//! 1. **PM**：`minix_pm::fork::do_srv_fork` 真实状态机跑全步（RS 门 / 容量门 /
//!    槽位扫描 / `vm_fork` / `srv_fork_from` 复制 / pid / `tell_vfs` / 立即回复）。
//!    出站消息由 PM 自带的 `TestIpcTransport` 捕获——该类型为集成测试刻意不做
//!    `cfg(test)` 门控（`servers/pm/src/ipc/transport.rs:165-166` 自证）。
//! 2. **VM**：宿主态不可驱动（`minix-vm` 对外只导出 `VmServer` 与 boot 类型，
//!    `vfs_queue`/`pagetable` 等模块均 `pub(crate)`）。因此 VM 段按 wire 契约参战：
//!    请求用 VM 侧解码器 [`VmForkIn`] 回解、应答用 VM 侧编码器 [`VmForkOut`] 构造
//!    ——PM 的编码与 VM 的解码互为见证，测的是跨服务 wire 对账（不是手抄字节）。
//!    VM 内部的地址空间复制由 `minix-vm` 自身单测覆盖。
//! 3. **VFS**：捕获到的 `VFS_PM_SRV_FORK` 逐域断言后，以消息字段还原
//!    `VfsCall::SrvFork` 并交给真实主线入口 `minix_vfs::VfsPmHandler::handle`
//!    （与 `servers/vfs/src/ipc/dispatcher.rs:187-198` 同臂）消费。该臂在复制
//!    fd 表之后追加 `setuid`/`setgid` 两步（C `misc.c:869` 的 `SRV_FORK` 特例），
//!    所以子 fproc 的凭证必须等于消息里的 `REUID`/`REGID`。
//!
//! # 覆盖
//!
//! - RS 门拒绝：非 RS 端点调用返回 `NotPermitted`，且**一条出站消息都没有**
//!   ——门在 `vm_fork` 之前（`forkexit.c:159-160`，Rust `fork.rs:125-127`）；
//! - 成功链：PM→VM→VFS 全通，三支出站消息（VM_FORK / VFS_PM_SRV_FORK / 给子
//!   进程的 OK）逐项断言，子槽凭证与调度上下文、VFS 侧子 fproc 逐项断言。
//!
//! 真机链路（VM 真实参战、RS 真实发起）挂 edge4 T2 / E5(c)。

use minix_pm::exit::KernelGateway;
use minix_pm::fork::{do_srv_fork, ForkCoordError};
use minix_pm::mproc::{Credentials, Lifecycle, Privilege, ProcTable, SrvForkParams};
use minix_pm::TestIpcTransport;
use minix_vfs::PmHandler;
use minix_types::{
    DecodeFromM1, EncodeToM1, Endpoint, Gid, Message, Uid, UserSlot, VfsCall, VfsReply, VmForkIn,
    VmForkOut, OK, VFS_PM_SRV_FORK, VFS_PM_SRV_FORK_REPLY, VM_FORK,
};

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

/// RS 在 PM 进程表里的槽位。
///
/// `Endpoint::RS` 的裸值是 2，槽位取低 15 位即 2（`types/endpoint.rs:63`）；
/// C 侧 `RS_PROC_NR` 同为 2（`minix3/minix/include/minix/com.h`），PM 的自有
/// 单测也把 RS 放在 `procs[2]`。
const RS_SLOT: usize = 2;

/// RS 的 pid（夹具值，`INIT_PID + 1` 起的真实分配值不参与本测试）。
const RS_PID: i32 = 2;

/// 子端点：槽 1、代 1。
///
/// VM 的 `sys_fork` 使用 PM 传入的槽位并递增代际，故首个子进程落在槽 1、代 1。
/// PM 侧有一条 `debug_assert_eq!` 守卫"VM 回复的槽位必须等于请求的槽位"
/// （`fork.rs:165-169`），本夹具与之一致。
fn child_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, 1)
}

fn child_slot() -> UserSlot {
    UserSlot::new(1)
}

/// 父进程表：RS 占槽 2、Running、系统进程、调度器 `NONE`，其余槽为空。
///
/// RS 是系统服务，C 侧它的 `mp_flags` 含 `PRIV_PROC` 且 `mp_scheduler == NONE`
/// （`04-stage-pm/16-scheduling.md:47-48` 引 `schedule.c:72` 断言），夹具照此。
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

/// VM 侧 fork 应答（OK + 子 endpoint 在 m1i3），用 VM 自己的编码器构造。
///
/// 与 `pm_vm_fork.rs` 的同名夹具一致：`VmForkOut` 是 VM 的发送半编码器，
/// 这里借它构造应答，PM 的解码端是 `vm_fork`（`ipc/dispatcher.rs:137`）。
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

/// 无操作内核网关：srv_fork 链在夹具上不触内核面（未设 tracer，`sys_*` 不可达）。
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

/// 构造一个运行中的用户进程槽（用于负路径：非 RS 端点的调用者）。
fn table_with_user_caller() -> ProcTable {
    let mut table = ProcTable::new();
    let caller = table.get_mut(5).expect("slot 5 exists");
    caller.identity.endpoint = Endpoint::from_generation_slot(1, 5);
    caller.identity.id.pid = 100;
    caller.state.lifecycle = Lifecycle::Running;
    table
}

// ---------------------------------------------------------------------------
// E5(c) 前哨.1 负路径：RS 门 —— 拒绝且零出站
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
    // C 的门是 do_srv_fork 的第一条语句（forkexit.c:159-160），先于容量门、
    // 槽位扫描与 vm_fork（183-185）。因此被拒的调用**不产生任何出站流量**：
    // 既没有 VM_FORK，也没有 VFS 通知，更没有给谁发过回复。
    assert!(
        transport.sent().is_empty(),
        "RS 门在 vm_fork 之前：拒绝路径零出站，实际 {:?}",
        transport.sent()
    );
}

// ---------------------------------------------------------------------------
// E5(c) 前哨.2 成功链：PM 状态机 → VM_FORK（wire 回解）→ VFS_PM_SRV_FORK
//                    （逐域）→ VFS 真处理臂
// ---------------------------------------------------------------------------

#[test]
fn srv_fork_chain_rs_pm_vm_vfs_all_real() {
    // 夹具里的凭证注入值：与 RS 自身凭证不同，才能证明是"注入"而非"继承"。
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

    // ── 出站消息一览：恰三支 ──
    // C 的 do_srv_fork 出站顺序：vm_fork（183-185，同步 sendrec）→ tell_vfs
    // （224-230，异步 send）→ reply(child, OK)（237，同步 send）。
    let sent = transport.sent();
    assert_eq!(
        sent.len(),
        3,
        "srv_fork 出站恰三支：VM_FORK + VFS_PM_SRV_FORK + 给子进程的 OK"
    );

    // ── 出站一：VM_FORK（PM 手写编码 ↔ VM 解码器互证）──
    let (vm_dest, vm_req) = &sent[0];
    assert_eq!(*vm_dest, Endpoint::VM);
    assert_eq!(vm_req.m_type, VM_FORK as i32);
    let req = VmForkIn::decode(unsafe { &vm_req.m_u.m_m1 });
    assert_eq!(req.parent_endpoint, Endpoint::RS, "VMF_ENDPOINT = m1i1");
    assert_eq!(req.child_slot, child_slot(), "VMF_SLOTNO = m1i2");

    // ── PM 侧子槽：占位、endpoint、pid、VFS_CALL ──
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
    // tell_vfs 成功后置 VFS_CALL（utility.c:128）；子进程在被立即唤醒时
    // 仍挂着这个未完成的 VFS 调用（fork.rs:229-231 注释）。
    assert!(
        child.state.block.is_vfs_blocked(),
        "子进程 VFS_CALL 已置位（tell_vfs 的第三步）"
    );

    // ── 凭证：SrvForkParams 注入（对比 do_fork 的父继承）──
    let creds = child.resources.privilege.credentials();
    assert_eq!(creds.user.real, INJECT_UID, "mp_realuid = 注入 uid");
    assert_eq!(creds.user.effective, INJECT_UID, "mp_effuid = 注入 uid");
    assert_eq!(creds.user.saved, INJECT_UID, "mp_svuid = 注入 uid");
    assert_eq!(creds.group.real, INJECT_GID, "mp_realgid = 注入 gid");
    assert_eq!(creds.group.effective, INJECT_GID, "mp_effgid = 注入 gid");
    assert_eq!(creds.group.saved, INJECT_GID, "mp_svgid = 注入 gid");
    // 子进程组随父（`srv_fork_from` 的 procgrp 取自父）。
    assert_eq!(child.identity.procgrp, RS_PID, "进程组随父 RS");
    // 系统服务的调度上下文：`scheduler == NONE` 保留，不经 SCHED 接管
    // （C forkexit.c:199-200 保留 PRIV_PROC 后 scheduler 仍 NONE；设计文档
    // 08-pm-srv-fork.md §2.4 同此）。
    assert_eq!(
        child.resources.scheduler,
        Endpoint::NONE,
        "系统服务 scheduler 保持 NONE（不经 SCHED 接管）"
    );
    // PRIV_PROC 保留（C-28 已落地）：C 的 do_srv_fork 用内联掩码
    // `rmc->mp_flags &= (IN_USE|PRIV_PROC|DELAY_CALL)`（forkexit.c:200）保留
    // `PRIV_PROC`，设计文档 08-pm-srv-fork.md §4.2/§D2 写明子进程
    // `is_kernel_process()==true`。`Process::srv_fork_from`（mproc/fork.rs）
    // 现落 `Privilege::Kernel(注入凭证)`：`Privilege::Kernel` 携带凭证字段，
    // 既保留特权位又安放六字段注入（对应 C 里 PRIV_PROC 与 mp_realuid 等正交）。
    // 上面的凭证断言证明注入值生效，本断言证明特权位保留。
    assert!(
        child.is_kernel_process(),
        "srv_fork 子进程保留 PRIV_PROC（is_kernel_process==true，C-28）"
    );

    // ── 出站二：VFS_PM_SRV_FORK（tell_vfs 载荷 m7 逐域）──
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

    // ── VFS 侧：真实 PM 协议入口消费同一条消息 ──
    // `VfsPmHandler::handle(VfsCall::SrvFork)` 即 VFS 主线对 PM 通知的分发臂
    // （dispatcher.rs:187-198）：先 handle_fork 复制 fd 表，再追加 setuid /
    // setgid 两步（SRV_FORK 在 C 侧的特例，misc.c:869）。三个入参全部取自
    // PM 编入消息的 m7 字段，链路不断。
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
    let reply = minix_vfs::VfsPmHandler { table: &mut fproc }
        .handle(vfs_call)
        .expect("VFS 应接受 PM 的 srv_fork 通知");
    assert!(
        matches!(reply, VfsReply::SrvFork),
        "分发臂回 VfsReply::SrvFork"
    );
    // 回复码即 C 的 VFS_PM_SRV_FORK_REPLY（0x988）；PM 侧的 `handle_vfs_reply`
    // 对该回复走空分支（main.c:398-401 `/* Nothing to do */`），清 VFS_CALL 后
    // 不重启信号——与普通 fork 的 `VFS_PM_FORK_REPLY` 双分支正交。
    assert_eq!(reply.m_type(), VFS_PM_SRV_FORK_REPLY);

    let child_fp = fproc
        .get(child_endpoint().to_user_slot().expect("child endpoint is user proc"))
        .expect("vfs child slot exists");
    assert_eq!(child_fp.endpoint, child_endpoint(), "子 fproc 端点落位");
    assert_eq!(
        child_fp.pid, child_pid,
        "子 pid = PM 编入消息的 m7i3"
    );
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
    assert_eq!(
        child_fp.filps[0],
        Some(7),
        "filp 表共享（同一表项索引）"
    );

    // ── 出站三：给子进程的立即 OK（`srv_fork` 与 `do_fork` 的回复差异）──
    // `do_fork` 返回 SUSPEND，由 VFS_PM_FORK_REPLY 的双分支异步唤醒父与子；
    // `srv_fork` 的子进程 scheduler==NONE 不经 SCHED，所以 PM 在此直接
    // `reply(child, OK)`（forkexit.c:237）后同步返父 pid（239）。
    let (child_dest, child_msg) = &sent[2];
    assert_eq!(*child_dest, child_endpoint(), "立即回复的目标是子进程");
    assert_eq!(child_msg.m_type, OK, "子进程收到 OK（不是 SUSPEND）");
}
