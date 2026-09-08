//! PM 主循环端到端集成测试（从 crate 外部驱动 `PmServer::run_once`）。
//!
//! # 为什么需要这一层
//!
//! 单元测试验证的是各 handler 的逻辑（给定表状态 → 预期副作用），分发层
//! 单测验证的是 `dispatch_pm_call` 的臂路由；但"从收到 `Message` 到发出
//! `Reply`"的完整链路——`run_once` 的 caller 验证、VFS 回复拦截、单一
//! 分发表（04-ipc-dispatch.md §4.2）、`ReplyIntent` → `reply()` 映射——
//! 只能由本层覆盖。旧跨 crate 测试（`os/tests/pm_vm_fork.rs`）因直接
//! 触碰 VM 内部类型而停用，本层改走消息面（其文件头建议的方向）。
//!
//! # 播种方式
//!
//! `BootParams::placeholder()` 的 boot image 全为空条目（不产生任何进程），
//! 测试进程经 `table_mut()` 手工播种——对应 C 在 main 循环前直接填
//! `mproc` 表的做法。wire 断言全部落在 `TestIpcTransport` 记录的
//! (目标, 消息) 序列上。

use minix_pm::init::{BootParams, RunStep};
use minix_pm::ipc::{IpcStatus, TestIpcTransport};
use minix_pm::mproc::{Lifecycle, WaitTarget};
use minix_types::{Endpoint, Message};

/// 集成测试用内核网关 mock：sys_times 返回脚本化计账值，
/// sys_kill/sys_clear/sys_abort 恒 OK（真实通电挂 edge E1/E6）。
struct MockKernelGateway {
    pub user: minix_types::Clock,
    pub sys: minix_types::Clock,
}

impl minix_pm::exit::KernelGateway for MockKernelGateway {
    fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
    fn copy_to_user(&mut self, bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
    fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
        Ok((self.user, self.sys))
    }
}

/// 构造一台带空参数表的服务器（`placeholder()` 不播种任何进程）。
fn server() -> minix_pm::init::PmServer<TestIpcTransport> {
    minix_pm::init::PmServer::with_kernel_gateway(
        BootParams::placeholder(),
        TestIpcTransport::new(),
        Box::new(MockKernelGateway { user: 0, sys: 0 }),
    )
}

/// 在 `slot` 播种一个 Running 进程（endpoint 带代际，PID 唯一）。
fn seed_running(server: &mut minix_pm::init::PmServer<TestIpcTransport>, slot: usize, pid: i32) -> Endpoint {
    let ep = Endpoint::from_generation_slot(1, slot as i32);
    let table = server.table_mut();
    table.procs[slot].identity.endpoint = ep;
    table.procs[slot].identity.id.pid = pid;
    table.procs[slot].state.lifecycle = Lifecycle::Running;
    ep
}

/// 构造一条来自 `source` 的请求消息。
fn request(m_type: i32, source: Endpoint) -> Message {
    Message {
        m_type,
        m_source: source,
        ..Default::default()
    }
}

#[test]
fn fork_request_runs_full_chain_and_suspends() {
    // C: do_fork（forkexit.c:44-143）——vm_fork（sendrec VM）成功 →
    // tell_vfs（VFS_PM_FORK）→ SUSPEND（主循环不向父回复，forkexit.c:139）。
    // 端到端断言的是 wire 序列：VM_FORK 先于 VFS_PM_FORK，且零 caller 回复。
    let mut srv = server();
    let parent_ep = seed_running(&mut srv, 5, 100);

    // 脚本化 VM_FORK 应答：空表 + 游标 0 → 子槽位 1，子代际 = 父(1)+1。
    let mut vm_reply = Message {
        m_type: minix_types::OK,
        ..Default::default()
    };
    vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
    srv.transport_mut()
        .queue_sendrec_reply(vm_reply);
    srv.transport_mut()
        .queue_receive(request(2, parent_ep), IpcStatus::default()); // PM_FORK = 2

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 2, "expected exactly [VM_FORK, VFS_PM_FORK]");
    assert_eq!(sent[0].0, Endpoint::VM);
    assert_eq!(sent[0].1.m_type, minix_types::VM_FORK as i32);
    assert_eq!(sent[1].0, Endpoint::VFS);
    assert_eq!(sent[1].1.m_type, minix_types::VFS_PM_FORK);
    // SUSPEND：没有任何发往父 endpoint 的回复。
    assert!(
        !sent.iter().any(|(dest, _)| *dest == parent_ep),
        "fork must not sync-reply the parent"
    );
    // 子进程槽位 1 处于 VFS_CALL（延续挂在子进程，forkexit.c:130）。
    assert!(srv.table().procs[1].state.block.is_vfs_blocked());
}

#[test]
fn exit_request_never_replies_and_zombifies_child() {
    // C: do_exit（forkexit.c:245-266）——"beyond the grave"：永不回复；
    // 父进程在 wait → tell_parent 后子进程 TOLD_PARENT，否则保持 ZOMBIE。
    let mut srv = server();
    let _parent_ep = seed_running(&mut srv, 2, 10);
    let child_ep = seed_running(&mut srv, 3, 11);
    // 父进程在等任意子进程（wait_test 命中 → tell_parent 链）。
    srv.table_mut().procs[2].state.wait.waiting = true;
    srv.table_mut().procs[2].state.wait.target = WaitTarget::AnyChild;

    srv.transport_mut()
        .queue_receive(request(1, child_ep), IpcStatus::default()); // PM_EXIT = 1，status 默认 0

    assert_eq!(srv.run_once(), RunStep::Handled);

    // 永不回复退出者本身。
    let sent = srv.transport().sent();
    assert!(
        !sent.iter().any(|(dest, _)| *dest == child_ep),
        "exit must never reply the exiting process"
    );
    // 子进程已完成僵尸链（父在 wait → ToldParent）。
    let child = &srv.table().procs[3];
    assert!(
        matches!(
            child.state.lifecycle,
            Lifecycle::ToldParent { .. } | Lifecycle::Zombie { .. }
        ),
        "child should be reaped or zombie, got {:?}",
        child.state.lifecycle
    );
}

#[test]
fn wait4_zombie_replies_pid_tag_with_status_payload() {
    // D-26 端到端：僵尸子进程 + 等待中的父 → ZOMBIE 环 tell_parent →
    // wire 回复 m_type = 子 pid（tag）、载荷 m_pm_lc_wait4.status =
    // W_EXITCODE（body）。libc 的 wait4 按此布局读取（forkexit.c:707-709）。
    let mut srv = server();
    let parent_ep = seed_running(&mut srv, 5, 100);
    // 僵尸子：exit(7)（W_EXITCODE(7,0) = 0x0700）。
    let table = srv.table_mut();
    table.procs[6].state.lifecycle = Lifecycle::Zombie { exit_code: 7, sig_status: 0 };
    table.procs[6].identity.id.pid = 101;
    table.procs[6].identity.endpoint = Endpoint::from_generation_slot(1, 6);
    table.procs[6].state.guardianship = minix_pm::mproc::Guardianship::Normal {
        parent: minix_types::UserSlot::new(5),
    };
    table.procs[5].state.wait.waiting = true;
    table.procs[5].state.wait.target = minix_pm::mproc::WaitTarget::AnyChild;

    let mut wait_msg = request(3, parent_ep); // PM_WAIT4 = 3
    wait_msg.m_u.m_lc_pm_wait4.pid = -1;
    srv.transport_mut()
        .queue_receive(wait_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, parent_ep);
    assert_eq!(sent[0].1.m_type, 101, "tag = child pid");
    let status = unsafe { sent[0].1.m_u.m_pm_lc_wait4.status };
    assert_eq!(status, 7 << 8, "body = W_EXITCODE(7, 0)");
    // 僵尸已被回收为 TOLD_PARENT。
    assert!(matches!(
        srv.table().procs[6].state.lifecycle,
        Lifecycle::ToldParent { .. }
    ));
}

#[test]
fn wait4_without_children_replies_echild() {
    // C: do_wait4 尾段（forkexit.c:550-563）——无匹配子进程 → ECHILD，
    // 与 WNOHANG 无关。
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);

    let mut wait_msg = request(3, caller_ep); // PM_WAIT4 = 3
    wait_msg.m_u.m_lc_pm_wait4.pid = -1; // 任意子进程
    wait_msg.m_u.m_lc_pm_wait4.options = 0;
    wait_msg.m_u.m_lc_pm_wait4.addr = 0;
    srv.transport_mut()
        .queue_receive(wait_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, caller_ep);
    assert_eq!(sent[0].1.m_type, minix_types::ECHILD);
}

#[test]
fn kill_unknown_pid_replies_esrch() {
    // C: do_kill → check_sig（signal.c:597+）——无匹配进程 → ESRCH，
    // 经主循环 reply(caller, errno)。
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);

    let mut kill_msg = request(11, caller_ep); // PM_KILL = 11
    kill_msg.m_u.m_lc_pm_kill.pid = 9999; // 不存在的 PID
    kill_msg.m_u.m_lc_pm_kill.signo = 15; // SIGTERM
    srv.transport_mut()
        .queue_receive(kill_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, caller_ep);
    assert_eq!(sent[0].1.m_type, minix_types::ESRCH);
}

#[test]
fn kill_on_fork_suspended_child_stops_it_and_records_pending() {
    // V3-P1-3：fork 挂起窗口（子槽 VFS_CALL，forkexit.c:130）内 kill(子)
    // → sig_proc 的 VFS_CALL 分支置 pending 并经 sys_delay_stop 停住子
    // 进程（C signal.c:425-443）——PM 复查信号前子进程不得再发起调用。
    let mut srv = server();
    let parent_ep = seed_running(&mut srv, 5, 100);

    // fork 全链：VM_FORK 脚本化 OK → 子槽 1 挂 VFS_CALL。
    let mut vm_reply = Message {
        m_type: minix_types::OK,
        ..Default::default()
    };
    vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
    srv.transport_mut().queue_sendrec_reply(vm_reply);
    srv.transport_mut()
        .queue_receive(request(2, parent_ep), IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);
    assert!(srv.table().procs[1].state.block.is_vfs_blocked());

    // 父进程 kill(子 pid, SIGTERM)。
    let child_pid = srv.table().procs[1].identity.id.pid;
    let mut kill_msg = request(11, parent_ep);
    kill_msg.m_u.m_lc_pm_kill.pid = child_pid;
    kill_msg.m_u.m_lc_pm_kill.signo = 15; // SIGTERM
    srv.transport_mut()
        .queue_receive(kill_msg, IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);

    // 子进程：SIGTERM pending + PROC_STOPPED（C signal.c:426/441-443）。
    let bit = 1u64 << (15 - 1);
    assert_eq!(srv.table().procs[1].resources.signals.pending & bit, bit);
    assert!(srv.table().procs[1].state.block.stopped, "stop_proc must stop the VFS-suspended child");
}

#[test]
fn unwired_call_replies_enosys() {
    // 04-ipc-dispatch.md §3.6 D6——已注册但 handler 未落地（GetPid=4
    // 属 15-credentials.md）→ ENOSYS 占位，诚实反映接线进度。
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);

    srv.transport_mut()
        .queue_receive(request(4, caller_ep), IpcStatus::default()); // PM_GETPID = 4

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, caller_ep);
    assert_eq!(sent[0].1.m_type, minix_types::ENOSYS);
}

#[test]
#[should_panic(expected = "handle_vfs_reply failed")]
fn corrupt_vfs_reply_is_fail_fast_not_enosys() {
    // C: main.c:84-87——IS_VFS_PM_RS && source==VFS 走第一路拦截，进入
    // handle_vfs_reply 状态机；状态损坏（此处：FORK_REPLY 无对应 VFS_CALL
    // 挂起）时 C 以 panic 兜底（main.c:317-319/324-325），Rust 同构。
    // 关键契约：VFS 回复被第一路消费，绝不落入 PM 调用分发表的 ENOSYS
    // 兜底（0x98x 不在 0..=0xff 调用族，落进去只会得到误导性的 ENOSYS）。
    let mut srv = server();
    // VFS 自身必须已在表中（run_once 的 pm_isokendpt 对 source==VFS 校验）。
    let vfs_ep = Endpoint::VFS;
    srv.table_mut().procs[1].identity.endpoint = vfs_ep;
    srv.table_mut().procs[1].identity.id.pid = 1;
    srv.table_mut().procs[1].state.lifecycle = Lifecycle::Running;

    // VFS_PM_FORK_REPLY（0x980+7）——无对应挂起的 VFS_CALL → 状态机报错
    // → run_once panic（fail-fast，04-ipc-dispatch.md §1.5）。
    srv.transport_mut()
        .queue_receive(request(0x980 + 7, vfs_ep), IpcStatus::default());
    let _ = srv.run_once();
}

#[test]
fn kill_termination_tells_vfs_exit() {
    // V2-P0-1（todo.md §11）：kill 成功终止目标后必须经服务器真实通道
    // 向 VFS 发 VFS_PM_EXIT（C forkexit.c:350-358 无条件 tell_vfs）——
    // 修复前该消息被 sig_proc_exit 内部构造的一次性 mock 吞掉，VFS
    // 永远不知道进程死亡。SIGKILL 默认处置 → 普通终止 → zombify +
    // VFS_PM_EXIT；终止链同时经同一通道告知 VM（vm_willexit）。
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);
    seed_running(&mut srv, 6, 101);

    let mut kill_msg = request(11, caller_ep); // PM_KILL = 11
    kill_msg.m_u.m_lc_pm_kill.pid = 101;
    kill_msg.m_u.m_lc_pm_kill.signo = 9; // SIGKILL
    srv.transport_mut()
        .queue_receive(kill_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert!(
        sent.iter()
            .any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
        "kill must notify VFS with VFS_PM_EXIT, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
    // 终止链还经同一通道告知 VM（vm_willexit，D-15/Fix #11）。
    assert!(
        sent.iter()
            .any(|(ep, m)| *ep == Endpoint::VM && m.m_type == minix_types::VM_WILLEXIT as i32),
        "exit chain must tell VM_WILLEXIT, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
    // 调用者收到 kill(2) 成功语义回复 0（dispatch Kill 臂的固定回复值）。
    assert!(
        sent.iter().any(|(ep, m)| *ep == caller_ep && m.m_type == 0),
        "caller must receive success reply 0, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
}
