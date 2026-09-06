//! PM 主循环三路分发（C: `main.c:84-103`）。
//!
//! 文档: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/04-ipc-dispatch.md`。
//!
//! # 与 C 的对应
//!
//! - `IS_VFS_PM_RS(call_nr) && who_e == VFS_PROC_NR` → `handle_vfs_reply()`
//!   （main.c:84-87）——此拦截在 `PmServer::run_once` 主循环第一路完成
//!   （见 `crate::init`），不进入本三路分发；状态机归 05-vfs-interaction.md。
//! - `call_nr == PROC_EVENT_REPLY` → `do_proc_event_reply()`
//!   （main.c:88-89）——本层只建钩子，语义归 06-event-subscription.md。
//! - `IS_PM_CALL(call_nr)` → `call_vec[call_nr - PM_BASE]()`（main.c:90-101）
//!   ——见 [`crate::ipc::calls::dispatch_pm_call`]。
//! - 其余 → ENOSYS（main.c:102-103）。
//!
//! `ReplyIntent` 建模 C 的 `result != SUSPEND → reply()`（main.c:106）：
//! SUSPEND（com.h:1151，值 -998）被显式化为 [`ReplyIntent::ReplyLater`] /
//! [`ReplyIntent::NoReply`]（ARCH A-6，plan.md §7.3）。

use super::transport::IpcTransport;
use crate::ipc::calls::{PmCall, dispatch_pm_call};
use crate::mproc::ProcTable;
use minix_types::{ENOSYS, Endpoint, Message, UserSlot};

/// VFS→PM 回复消息类型基址。C: `VFS_PM_RS_BASE` — com.h:514。
///
/// 收敛到 minix-types（与 com.h:514 单一真相），见 05-design.v1.md D1。
pub use minix_types::VFS_PM_RS_BASE;

/// 进程事件订阅者回复消息类型。C: `PROC_EVENT_REPLY` —
/// `COMMON_RS_BASE + 0` — com.h:619（COMMON_RS_BASE = 0xE80，com.h:598）。
pub const PROC_EVENT_REPLY: i32 = 0xE80;

/// 主循环对一次分发的回复意图（ARCH A-6：C `SUSPEND` 显式化）。
///
/// C 中 handler 返回 `result`，主循环 `if (result != SUSPEND) reply(...)`
/// （main.c:106）；SUSPEND 是 -998 魔法数，同时覆盖三种子情形
/// （plan.md §7.3）：等待中回复（do_wait4 → tell_parent）、异步回复
/// （do_exec/do_set → handle_vfs_reply）、永不回复（do_exit）。Rust 用
/// 枚举区分：
///
/// - [`ReplyIntent::Reply`]：主循环立即 `reply(slot, code)`（C: result
///   非 SUSPEND）。
/// - [`ReplyIntent::ReplyLater`]：本次不回复，稍后由异步路径回复
///   （C: result == SUSPEND 且存在后续回复者，如 05 的 VFS 回复）。
/// - [`ReplyIntent::NoReply`]：永不回复（C: do_exit 返回 SUSPEND 且
///   没有后续回复者，forkexit.c:246-266；04 层暂不产生，供 09 使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyIntent {
    /// 立即以 `code` 作为回复 `m_type` 发送（code 可为 OK=0 / errno /
    /// 载荷值如 pid）。
    Reply(i32),
    /// 本次不回复，稍后由异步路径回复（C: SUSPEND）。
    ReplyLater,
    /// 永不回复（C: do_exit 子情形；04 层暂无生产者）。
    NoReply,
}

/// C: `IS_VFS_PM_RS(type)` — com.h:517：`((type) & ~0x7f) == VFS_PM_RS_BASE`。
///
/// 委托到 minix-types 同名函数（单一真相），见 05-design.v1.md D1。
pub fn is_vfs_pm_rs(nr: i32) -> bool {
    minix_types::is_vfs_pm_rs(nr)
}

/// C: `IS_PM_CALL(type)` — callnr.h:11：`((type) & ~0xff) == PM_BASE`。
pub fn is_pm_call(nr: i32) -> bool {
    (nr & !0xff) == 0
}

/// 主循环三路分发（C: main.c:84-103）。
///
/// 前置条件（调用方 `PmServer::run_once` 已保证）：消息非 notification、
/// `msg.m_source` 已通过 `pm_isokendpt` 验证、caller 非 EXITING。
///
/// # 注意
///
/// VFS→PM 异步回复（`IS_VFS_PM_RS && source == VFS`）已在 `run_once` 入口
/// 拦截并交由 `handle_vfs_reply` 状态机处理（main.c:84-87 在主循环第一路），
/// 因此**不会**进入本分发。本函数处理事件回复与 PM 调用两路——PM 调用
/// 全部经 [`dispatch_pm_call`] 的单一分发表（ARCH A-5），主循环不得内联
/// 拦截。
///
/// # 返回
///
/// [`ReplyIntent`]：主循环据此决定是否回复（main.c:106 等价）。
pub fn dispatch_message<T: IpcTransport>(
    table: &mut ProcTable,
    events: &mut crate::event::EventRegistry,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
    caller: UserSlot,
    msg: &Message,
) -> ReplyIntent {
    let call_nr = msg.m_type;

    if call_nr == PROC_EVENT_REPLY {
        // C: main.c:88-89 — do_proc_event_reply()（06-event-subscription.md）。
        events.do_proc_event_reply(msg, caller, table, transport, kern)
    } else if is_pm_call(call_nr) {
        // C: main.c:90-101 — call_index = call_nr - PM_BASE；越界/NULL →
        // ENOSYS。
        match PmCall::from_call_nr(call_nr) {
            Some(call) => dispatch_pm_call(call, table, events, transport, kern, caller, msg),
            None => ReplyIntent::Reply(ENOSYS),
        }
    } else {
        // C: main.c:102-103 — 非 PM 调用 → ENOSYS。
        ReplyIntent::Reply(ENOSYS)
    }
}

// ── PM → VM 服务间调用（同步任务调用，对齐 C libsys）──

/// 发送 VM_FORK 请求到 VM 服务（同步）。
///
/// C: `vm_fork`（libsys，`minix3/minix/lib/libsys/vm_fork.c:16-25`）——
/// `_taskcall(VM_PROC_NR, VM_FORK, &m)`：请求 m1 载荷 `VMF_ENDPOINT`
/// （父 endpoint，m1i1）与 `VMF_SLOTNO`（子槽位，m1i2），回复的子进程
/// endpoint 在 `VMF_CHILD_ENDPOINT`（m1i3）。C 的 do_fork 在 vm_fork
/// 失败时把 errno 直接传播给 fork 调用者（`forkexit.c:77-79`），且一旦
/// vm_fork 成功，fork 不允许再失败（`forkexit.c:83` "PM may not fail
/// fork after call to vm_fork()"）——因此本函数是 fork 链路上唯一的
/// VM 依赖点，其失败必须先于任何不可回滚的进程表变更被发现。
///
/// # 错误
///
/// 传输失败或 VM 回复非 OK（负 errno）都收敛为
/// [`ForkCoordError::VmError`]。errno 细粒度传播依赖 `PmError` 增加
/// 载体变体（minix-types 共享层，登记于 edge_todo.md E7），当前以
/// 统一 VmError 失败——失败语义正确，粒度待共享层支持。
pub fn vm_fork<T: IpcTransport>(
    transport: &mut T,
    parent: Endpoint,
    child_slot: UserSlot,
) -> Result<Endpoint, crate::fork::ForkCoordError> {
    // 请求编码：m1i1 = VMF_ENDPOINT，m1i2 = VMF_SLOTNO（C 的 VMF_*
    // 宏即 m1 字段别名；VmForkIn 只实现了 VM 侧的 DecodeFromM1，
    // PM 侧发送端按同一布局手写，对应 ipc/vm.rs:841-849 的解码序）。
    let mut msg = Message {
        m_type: minix_types::VM_FORK as i32,
        ..Default::default()
    };
    msg.m_u.m_m1.m1i1 = parent.0;
    msg.m_u.m_m1.m1i2 = child_slot.get() as i32;
    transport
        .sendrec(Endpoint::VM, &mut msg)
        .map_err(|_| crate::fork::ForkCoordError::VmError)?;
    // C: taskcall 返回值非 OK → errno 传播（forkexit.c:78-79）。负数
    // errno 直接视为 VM 拒绝；m_type == OK 才读取 m1i3。
    if msg.m_type != minix_types::OK {
        return Err(crate::fork::ForkCoordError::VmError);
    }
    // 回复解码：VMF_CHILD_ENDPOINT 在 m1i3
    //（minix-types EncodeToM1 for VmForkOut，ipc/vm.rs:852-856）。
    Ok(Endpoint(unsafe { msg.m_u.m_m1 }.m1i3))
}

/// 发送 VM_WILLEXIT 通知到 VM 服务（同步）。
///
/// C: `vm_willexit`（libsys，`minix3/minix/lib/libsys/vm_willexit.c:11-21`）
/// —— `_taskcall(VM_PROC_NR, VM_WILLEXIT, &m)`：载荷 `VMWE_ENDPOINT`
/// （m1i1，`com.h:644`），无回复载荷。taskcall 返回值即结果——非 OK
/// 时 C 的调用方 panic（`forkexit.c:332-334` "exit_proc: vm_willexit
/// failed"），因为 VM 的内存记账依赖该通知，缺失将永久失衡。
///
/// # 错误
///
/// 传输失败或 VM 回复非 OK → `Err(errno 值)`；调用方（`exit.rs` 的
/// `exit_proc` 步骤 6）以同文案 panic 对齐 C。
pub fn vm_willexit<T: IpcTransport + ?Sized>(transport: &mut T, endpoint: Endpoint) -> Result<(), i32> {
    let mut msg = Message {
        m_type: minix_types::VM_WILLEXIT as i32,
        ..Default::default()
    };
    msg.m_u.m_m1.m1i1 = endpoint.0;
    transport
        .sendrec(Endpoint::VM, &mut msg)
        .map_err(|_| minix_types::EIO)?;
    if msg.m_type != minix_types::OK {
        return Err(msg.m_type);
    }
    Ok(())
}

/// 发送 VM_EXIT 通知到 VM 服务（同步）。
///
/// C: `vm_exit`（libsys，`minix3/minix/lib/libsys/vm_exit.c`）——
/// `_taskcall(VM_PROC_NR, VM_EXIT, &m)`：载荷 `VME_ENDPOINT`（m1i1，
/// `com.h:631`），无回复载荷。失败时 C 的调用方 panic
///（`forkexit.c:455-457` "exit_restart: vm_exit failed"）——页表已随
/// 进程终结，VM 不回收即永久泄漏。
///
/// # 错误
///
/// 传输失败或 VM 回复非 OK → `Err(errno 值)`；调用方（`exit.rs` 的
/// `exit_restart` 步骤 5）以同文案 panic 对齐 C。
pub fn vm_exit<T: IpcTransport + ?Sized>(transport: &mut T, endpoint: Endpoint) -> Result<(), i32> {
    let mut msg = Message {
        m_type: minix_types::VM_EXIT as i32,
        ..Default::default()
    };
    msg.m_u.m_m1.m1i1 = endpoint.0;
    transport
        .sendrec(Endpoint::VM, &mut msg)
        .map_err(|_| minix_types::EIO)?;
    if msg.m_type != minix_types::OK {
        return Err(msg.m_type);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventRegistry;
    use crate::ipc::TestIpcTransport;
    use crate::mproc::Lifecycle;
    use minix_types::Message;


    /// 测试用内核网关 mock（sys_kill/sys_clear 恒 OK）。
    #[derive(Default)]
    struct NoopKernel;
    impl crate::exit::KernelGateway for NoopKernel {
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
    }
    fn msg_with(m_type: i32, source: Endpoint) -> Message {
        let mut m = Message::default();
        m.m_type = m_type;
        m.m_source = source;
        m
    }

    /// 构造 dispatch_message 测试环境：`slot` 处注册 Running 进程
    ///（`kernel=true` 时置 Privilege::Kernel，供事件回复的内核门）。
    fn setup(slot: usize, ep: Endpoint, kernel: bool) -> (ProcTable, EventRegistry, TestIpcTransport) {
        let mut table = ProcTable::new();
        table.procs[slot].identity.endpoint = ep;
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        if kernel {
            table.procs[slot].resources.privilege = crate::mproc::Privilege::Kernel;
        }
        (table, EventRegistry::new(), TestIpcTransport::new())
    }

    #[test]
    fn test_is_vfs_pm_rs_matches_c() {
        // com.h:517 — 0x980 族内任意低 7 位都命中。
        assert!(is_vfs_pm_rs(0x980 + 7)); // VFS_PM_FORK_REPLY
        assert!(is_vfs_pm_rs(0x980));
        assert!(!is_vfs_pm_rs(0x980 + 0x80)); // 超出低 7 位
        assert!(!is_vfs_pm_rs(0x900)); // VFS_PM_RQ
        assert!(!is_vfs_pm_rs(0xE80)); // COMMON_RS
    }

    #[test]
    fn test_is_pm_call_matches_c() {
        // callnr.h:11 — 0x00~0xff 内任意类型都"是 PM 调用族"（解码后
        // 未注册号仍 ENOSYS，与 C 越界槽位一致）。
        assert!(is_pm_call(2)); // PM_FORK
        assert!(is_pm_call(47)); // PM_GETSYSINFO
        assert!(is_pm_call(0)); // 保留值：族内但未注册
        assert!(!is_pm_call(0x100)); // 超出低 8 位
        assert!(!is_pm_call(-1));
    }

    #[test]
    fn test_vfs_pm_rs_from_non_vfs_source_is_enosys() {
        // VFS→PM 异步回复（IS_VFS_PM_RS && source == VFS）已由 `run_once`
        // 在主循环第一路（main.c:84-87）拦截，不进入本三路分发；此处仅当
        // 来源非 VFS 时才会落入 dispatch_message，此时它不属于 PM 调用族
        // → ENOSYS（main.c:102-103 兜底）。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel::default();
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            UserSlot::new(3),
            &msg_with(0x980 + 7, Endpoint::RS),
        );
        assert_eq!(intent, ReplyIntent::Reply(ENOSYS));
    }

    #[test]
    fn test_proc_event_reply_routes_to_reply_later() {
        // C: main.c:88-89 — PROC_EVENT_REPLY → do_proc_event_reply()。
        // 内核调用者 + 默认消息（event 位为 0 → endpoint 解析失败）
        // → SUSPEND 前置（event.c:241-245 → ReplyLater）。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, true);
        let mut kern = NoopKernel::default();
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            UserSlot::new(3),
            &msg_with(PROC_EVENT_REPLY, Endpoint::RS),
        );
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_proc_event_reply_from_user_process_is_enosys() {
        // event.c:232-233 — 仅系统服务可回复；普通进程误用 → ENOSYS 回复。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel::default();
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            UserSlot::new(3),
            &msg_with(PROC_EVENT_REPLY, Endpoint::RS),
        );
        assert_eq!(intent, ReplyIntent::Reply(ENOSYS));
    }

    #[test]
    fn test_pm_call_routes_to_dispatch_pm_call() {
        // C: main.c:90-101 — IS_PM_CALL → call_vec（单一分发表）。
        // 未接线调用（GetPid）→ ENOSYS（DEFERRED，40 个）。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel::default();
        assert_eq!(
            dispatch_message(
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                UserSlot::new(3),
                &msg_with(4, ep)
            ),
            ReplyIntent::Reply(ENOSYS)
        );
    }

    #[test]
    fn test_unknown_type_returns_enosys() {
        // C: main.c:102-103 — 非 PM 调用 → ENOSYS。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel::default();
        assert_eq!(
            dispatch_message(
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                UserSlot::new(3),
                &msg_with(0x100, ep)
            ),
            ReplyIntent::Reply(ENOSYS)
        );
    }

    // ── vm_willexit（PM → VM 退出预告）──

    #[test]
    fn test_vm_willexit_encodes_endpoint_and_ok() {
        // C: libsys vm_willexit.c:11-21 — 载荷 VMWE_ENDPOINT（m1i1，
        // com.h:644），无回复载荷；taskcall 返回 OK。
        let mut transport = crate::ipc::TestIpcTransport::new();
        let got = vm_willexit(&mut transport, Endpoint::from_generation_slot(1, 7));
        assert!(got.is_ok());
        let (dest, sent_msg) = &transport.sent()[0];
        assert_eq!(*dest, Endpoint::VM);
        assert_eq!(sent_msg.m_type, minix_types::VM_WILLEXIT as i32);
        assert_eq!(unsafe { sent_msg.m_u.m_m1 }.m1i1, Endpoint::from_generation_slot(1, 7).0);
    }

    #[test]
    fn test_vm_willexit_refusal_is_err() {
        let mut transport = crate::ipc::TestIpcTransport::new();
        let mut refusal = Message::default();
        refusal.m_type = -12; // VM 拒绝（负 errno）
        transport.queue_sendrec_reply(refusal);
        let got = vm_willexit(&mut transport, Endpoint::from_generation_slot(1, 7));
        assert_eq!(got.unwrap_err(), -12);
    }

    // ── vm_exit（PM → VM 页表回收通知）──

    #[test]
    fn test_vm_exit_encodes_endpoint_and_ok() {
        // C: libsys vm_exit.c — 载荷 VME_ENDPOINT（m1i1，com.h:631）。
        let mut transport = crate::ipc::TestIpcTransport::new();
        let got = vm_exit(&mut transport, Endpoint::from_generation_slot(3, 9));
        assert!(got.is_ok());
        let (dest, sent_msg) = &transport.sent()[0];
        assert_eq!(*dest, Endpoint::VM);
        assert_eq!(sent_msg.m_type, minix_types::VM_EXIT as i32);
        assert_eq!(unsafe { sent_msg.m_u.m_m1 }.m1i1, Endpoint::from_generation_slot(3, 9).0);
    }

    #[test]
    fn test_vm_exit_refusal_is_err() {
        let mut transport = crate::ipc::TestIpcTransport::new();
        let mut refusal = Message::default();
        refusal.m_type = -19;
        transport.queue_sendrec_reply(refusal);
        let got = vm_exit(&mut transport, Endpoint::from_generation_slot(3, 9));
        assert_eq!(got.unwrap_err(), -19);
    }

    // ── vm_fork（PM → VM 任务调用）──

    /// 构造一条 VM_FORK 脚本化应答（OK + 子 endpoint 在 m1i3）。
    fn vm_fork_ok_reply(child: Endpoint) -> Message {
        let mut m = Message::default();
        m.m_type = minix_types::OK;
        unsafe {
            m.m_u.m_m1.m1i3 = child.0;
        }
        m
    }

    #[test]
    fn test_vm_fork_encodes_request_and_decodes_reply() {
        // C: libsys vm_fork.c:16-25 — _taskcall(VM_PROC_NR, VM_FORK)，
        // 请求 VMF_ENDPOINT/VMF_SLOTNO（m1i1/m1i2），回复 VMF_CHILD_ENDPOINT（m1i3）。
        let mut transport = crate::ipc::TestIpcTransport::new();
        let child = Endpoint::from_generation_slot(2, 7);
        transport.queue_sendrec_reply(vm_fork_ok_reply(child));

        let got = vm_fork(
            &mut transport,
            Endpoint::from_generation_slot(1, 3),
            UserSlot::new(7),
        )
        .expect("vm_fork should succeed on scripted OK reply");
        assert_eq!(got, child);

        // 请求 wire：发往 VM，m_type = VM_FORK，载荷 m1i1/m1i2。
        let (dest, sent_msg) = &transport.sent()[0];
        assert_eq!(*dest, Endpoint::VM);
        assert_eq!(sent_msg.m_type, minix_types::VM_FORK as i32);
        let m1 = unsafe { sent_msg.m_u.m_m1 };
        assert_eq!(m1.m1i1, Endpoint::from_generation_slot(1, 3).0);
        assert_eq!(m1.m1i2, 7);
    }

    #[test]
    fn test_vm_fork_vm_refusal_is_error() {
        // C: forkexit.c:78-79 — vm_fork 返回非 OK → errno 传播为失败。
        let mut transport = crate::ipc::TestIpcTransport::new();
        let mut refusal = Message::default();
        refusal.m_type = -12; // 负 errno（ENOMEM）
        transport.queue_sendrec_reply(refusal);

        let result = vm_fork(
            &mut transport,
            Endpoint::from_generation_slot(1, 3),
            UserSlot::new(7),
        );
        assert_eq!(result.unwrap_err(), crate::fork::ForkCoordError::VmError);
    }

    #[test]
    fn test_vm_fork_transport_failure_is_error() {
        // 传输层失败（真实内核 IPC 未落地时 sendrec 会 Err）同样收敛为
        // VmError——绝不伪造成功（原 send_vm_fork 假成功接缝的回归守卫）。
        struct FailingSendrec;
        impl IpcTransport for FailingSendrec {
            fn receive(
                &mut self,
            ) -> Result<(Message, crate::ipc::IpcStatus), minix_types::IpcError> {
                Err(minix_types::IpcError::WouldBlock)
            }
            fn send(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), minix_types::IpcError> {
                Ok(())
            }
            fn sendrec(
                &mut self,
                _dest: Endpoint,
                _msg: &mut Message,
            ) -> Result<(), minix_types::IpcError> {
                Err(minix_types::IpcError::NoPerm)
            }
        }

        let mut transport = FailingSendrec;
        let result = vm_fork(
            &mut transport,
            Endpoint::from_generation_slot(1, 3),
            UserSlot::new(7),
        );
        assert_eq!(result.unwrap_err(), crate::fork::ForkCoordError::VmError);
    }
}
