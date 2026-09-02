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

use crate::ipc::calls::{PmCall, dispatch_pm_call};
use crate::mproc::ProcTable;
use minix_types::{ENOSYS, Endpoint};

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
/// 因此**不会**进入本分发。本函数仅处理事件回复与 PM 调用两路。
///
/// # 返回
///
/// [`ReplyIntent`]：主循环据此决定是否回复（main.c:106 等价）。
pub fn dispatch_message(table: &mut ProcTable, msg: &minix_types::Message) -> ReplyIntent {
    let call_nr = msg.m_type;

    if call_nr == PROC_EVENT_REPLY {
        // C: main.c:88-89 — do_proc_event_reply()。
        //
        // 钩子：事件订阅语义归 06-event-subscription.md。
        ReplyIntent::ReplyLater
    } else if is_pm_call(call_nr) {
        // C: main.c:90-101 — call_index = call_nr - PM_BASE；越界/NULL →
        // ENOSYS。
        match PmCall::from_call_nr(call_nr) {
            Some(call) => dispatch_pm_call(call, table, msg.m_source),
            None => ReplyIntent::Reply(ENOSYS),
        }
    } else {
        // C: main.c:102-103 — 非 PM 调用 → ENOSYS。
        ReplyIntent::Reply(ENOSYS)
    }
}

// ── 服务间请求占位（07 落地前保留）──

// 以下两个占位函数是 fork 协调器原型（fork.rs）使用的服务间请求
// （VM_FORK / 内核请求）。VFS 请求已统一经 `tell_vfs`（`crate::ipc::vfs`）
// 发送，不再有独立占位。真实异步协议归 07-pm-fork.md 与 05-stage-vfs。

/// 发送 VM_FORK 请求到 VM 服务。
///
/// C: `vm_fork`（libsys）；真实协议见 02-stage-vm/18-vm-fork.md。
pub fn send_vm_fork(
    request: minix_types::VmForkIn,
) -> Result<minix_types::VmForkOut, crate::fork::ForkCoordError> {
    // 测试占位：VM 侧的 sys_fork 会生成 generation 递增的 endpoint；
    // 此处按请求槽位生成 gen=1 的 endpoint，与 C 的 child_ep.slot == next_child 守卫一致
    // （forkexit.c:74-75），02-stage-vm/18 的真实实现将替换。
    Ok(minix_types::VmForkOut {
        child_endpoint: Endpoint::from_generation_slot(1, request.child_slot.get() as i32),
    })
}

/// 发送请求到内核。
///
/// 内核系统调用面归 01-stage-kernel；当前测试占位。
pub fn send_kernel_request(
    _request: minix_types::KernelRequest,
) -> Result<minix_types::KernelResponse, crate::fork::ForkCoordError> {
    // TODO: 内核 IPC 落地；当前测试占位。
    Ok(minix_types::KernelResponse::ForkOk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Message;

    fn msg_with(m_type: i32, source: Endpoint) -> Message {
        let mut m = Message::default();
        m.m_type = m_type;
        m.m_source = source;
        m
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
        let mut table = ProcTable::new();
        let intent = dispatch_message(&mut table, &msg_with(0x980 + 7, Endpoint::RS));
        assert_eq!(intent, ReplyIntent::Reply(ENOSYS));
    }

    #[test]
    fn test_proc_event_reply_routes_to_reply_later() {
        // C: main.c:88-89 — PROC_EVENT_REPLY → do_proc_event_reply()。
        let mut table = ProcTable::new();
        let intent = dispatch_message(&mut table, &msg_with(PROC_EVENT_REPLY, Endpoint::RS));
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_pm_call_routes_to_dispatch_pm_call() {
        // C: main.c:90-101 — IS_PM_CALL → call_vec。
        let mut table = ProcTable::new();
        // 未实现调用 → ENOSYS（DEFERRED 07~20）。
        assert_eq!(
            dispatch_message(
                &mut table,
                &msg_with(11, Endpoint::from_generation_slot(1, 0))
            ),
            ReplyIntent::Reply(ENOSYS)
        );
        // fork → SUSPEND 语义（forkexit.c:139）。
        assert_eq!(
            dispatch_message(
                &mut table,
                &msg_with(2, Endpoint::from_generation_slot(1, 0))
            ),
            ReplyIntent::ReplyLater
        );
    }

    #[test]
    fn test_unknown_type_returns_enosys() {
        // C: main.c:102-103 — 非 PM 调用 → ENOSYS。
        let mut table = ProcTable::new();
        assert_eq!(
            dispatch_message(
                &mut table,
                &msg_with(0x100, Endpoint::from_generation_slot(1, 0))
            ),
            ReplyIntent::Reply(ENOSYS)
        );
    }
}
