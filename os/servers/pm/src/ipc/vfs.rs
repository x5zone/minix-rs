//! PM ↔ VFS 异步协议：请求发送（`tell_vfs`）与回复状态机（`handle_vfs_reply`）。
//!
//! 对应 Minix3 C 源码：
//! - `minix3/minix/servers/pm/utility.c:120-139` — `tell_vfs`（置位 VFS_CALL + 异步发送）
//! - `minix3/minix/servers/pm/main.c:294-424` — `handle_vfs_reply`（11 路回复状态机）
//!
//! 设计契约见 `04-stage-pm/.design/05-design.v1.md`（D2~D8）。
//!
//! # 与微内核事件模型的对照
//!
//! Minix3 的 PM 是**单线程事件循环**（`04-ipc-dispatch` 已确立：handler 阻塞 =
//! 整个服务器停摆）。它向 VFS 发起的请求（exec 装载镜像、fork 复制 fd、setuid、
//! exit 等）耗时且不可预测（要读盘、写盘、等待 tty/pipe）。若同步等待，PM 会停摆；
//! 若此时 VFS 又经 `ipc_send(PM_PROC_NR, ...)`（阻塞）反向向 PM 发消息，则双方互
//! 相等待 → 死锁。因此 PM 用 `asynsend3`（非阻塞、`AMF_NOREPLY`）把请求"投出"，
//! 然后立刻回到主循环；VFS 处理完（可能经 worker 线程，见 `vfs/main.c:895-899`
//! 注释）才异步回送回复，PM 在主循环第一路（`main.c:84-87`）收口。
//!
//! 这与 Redox 的 `Scheme` 模型同构而异构：
//! - **同构**：请求被"投出"后，调用方不立即得到结果；结果由被调用方在"稍后"回复。
//! - **异构**：Redox 把延迟完成挂在 `handle` 表上（内核挂起调用者，`doc.redox-os.org/
//!   book/scheme-operation.html`），其 unit 是 *scheme handle*；Minix3 把延续挂在
//!   *进程*（`mproc`）的标志位上，unit 是 *进程*，因为 PM 的请求本质都是进程状态迁移。
//!   Rust 侧保留"以进程为单位"的语义，但把 C 的隐式标志位延续提升为显式端口调用
//!   （[`VfsReplyServices`]），消除 `main.c`/`event.c`/`signal.c` 四处散落、靠 `panic`
//!   兜底的状态推断（ARCH A-2 / A-6 延伸）。

use core::fmt;

use minix_types::{
    Endpoint, IpcError, Message, Pid, UserSlot, VfsCall, VfsReply, VfsReplyError,
    VFS_PM_REBOOT_REPLY,
};

use crate::ipc::transport::IpcTransport;
use crate::mproc::{IpcBlockReason, ProcTable};

/// Minix3 成功码（`minix3/sys/sys/errno.h:190` `#define OK 0`）。
pub const OK: i32 = 0;

/// core dump 标志位（`minix3/sys/sys/wait.h:63`，值 `0200`）。
///
/// 仅 `VfsReply::Core` 成功时置位（main.c:357-358），当前 `set_core_flag`
/// 为 DEFERRED（见 [`VfsReplyServices::set_core_flag`]）。
#[allow(dead_code)]
const WCOREFLAG: i32 = 0o200;

/// `tell_vfs` 的发送错误。
///
/// C 在对应位置直接 `panic`（`utility.c:131-132` 的 not-idle、`135-136` 的发送失败）。
/// Rust 侧改为返回 `Result`：生产路径由调用方按 C 语义 fail-fast，测试路径可断言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsCallError {
    /// 目标进程正在进行另一次 VFS/事件调用（C: `VFS_CALL | EVENT_CALL` 已置位）。
    ///
    /// C: `utility.c:122-123` panic("tell_vfs: not idle: %d")。
    NotIdle,
    /// IPC 发送失败（C: `asynsend3` 失败 panic）。
    SendFailed(IpcError),
}

impl fmt::Display for VfsCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotIdle => write!(f, "tell_vfs: target process is not idle (already VFS/EVENT blocked)"),
            Self::SendFailed(e) => write!(f, "tell_vfs: IPC send to VFS failed: {:?}", e),
        }
    }
}

/// 进程事件类型（供 [`VfsReplyServices::publish_event`] 使用）。
///
/// 对应 C `event.c:86-91` 中由 `EXITING` / `UNPAUSED` 标志推断的两类事件；此处把推断
/// 上提到状态机（[`VfsReplyServices`] 的调用点），消除 `publish_event` 内部的
/// `panic("unknown event for flags")`（D6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcEvent {
    /// 进程退出事件（EXITING 置位）。
    Exit,
    /// 信号/解暂停事件（UNPAUSED 置位）。
    Signal,
}

/// `exec_restart` 的参数（来自 `VFS_PM_EXEC_REPLY`，main.c:350-353）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecRestartArgs {
    /// 执行状态（OK 或失败，`VFS_PM_STATUS = m7_i2`）。
    pub status: i32,
    /// 程序计数器（`VFS_PM_PC = m7_p1`）。
    pub pc: u64,
    /// 可能更新的用户栈指针（`VFS_PM_NEWSP = m7_p2`）。
    pub newsp: u64,
    /// 可能更新的 ps_strings 指针（`VFS_PM_NEWPS_STR = m7_i5`）。
    pub newps_str: i32,
}

/// PM 回复 VFS 时的效果端口（六边形架构：状态机是纯逻辑，效果经此端口施加）。
///
/// 读取方法供状态机做分支决策；效果方法即时施加。两个实现：
/// - [`PmServices`]：生产实现（部分方法在 06/09/13/16/17 落地前为 `unimplemented!`）。
/// - [`RecordingServices`]：测试实现（录制调用序列，供断言）。
pub trait VfsReplyServices {
    // ── 读取（状态机决策用）──

    /// 将回复中的 endpoint 解析为槽位（C: `pm_isokendpt`，main.c:317-319）。
    ///
    /// 返回 `None` 时状态机产生 [`VfsReplyError::BadEndpoint`]。
    fn slot_of_endpoint(&self, endpoint: Endpoint) -> Option<UserSlot>;

    /// 抽取并清除 `VFS_CALL | NEW_PARENT`（main.c:327-328）。
    ///
    /// 返回 `NEW_PARENT` 旧值；若 `VFS_CALL` 未置位则 panic（main.c:324-325
    /// "reply without request"）。清除必须在读取之后、使用之前，与 C 顺序一致。
    fn take_vfs_call(&mut self, slot: UserSlot) -> bool;

    /// `UNPAUSED` 是否在进入时为置位（main.c:330-331 入口不变式）。
    fn is_unpaused(&self, slot: UserSlot) -> bool;

    /// 进程是否正在退出（用于尾部 `restart_sigs` 条件，main.c:422）。
    fn is_exiting(&self, slot: UserSlot) -> bool;

    /// 进程是否在使用中（用于尾部 `restart_sigs` 条件，main.c:422）。
    fn is_in_use(&self, slot: UserSlot) -> bool;

    /// 进程组号（SETSID 回复码，main.c:345 → `reply(caller, mp_procgrp)`）。
    fn procgrp_of(&self, slot: UserSlot) -> Pid;

    /// 进程 PID（FORK 成功回复父进程时作为回复码，main.c:393 → `rmp->mp_pid`）。
    fn pid_of(&self, slot: UserSlot) -> Pid;

    /// 父进程槽位（`reply_to_guardian` 内部使用）。
    fn parent_slot(&self, slot: UserSlot) -> UserSlot;

    // ── 效果（即时施加）──

    /// 回复进程本体（main.c:339/345/389）。
    fn reply(&mut self, slot: UserSlot, code: i32);

    /// 回复父进程；当 `suppress`（`new_parent` 为真）时抑制回复（main.c:384/392）。
    fn reply_to_guardian(&mut self, slot: UserSlot, suppress: bool, code: i32);

    /// 启动新进程调度（main.c:373）。返回 `Err` 表示调度失败（main.c:378-386）。
    fn sched_start_user(&mut self, slot: UserSlot) -> Result<(), i32>;

    /// 推进进程退出（main.c:381/379 → `exit_proc(rmp, -1, FALSE)` 或 CORE/EXIT 路径）。
    fn exit_proc(&mut self, slot: UserSlot, status: i32, dump_core: bool);

    /// 置 `WCOREFLAG`（仅 `Core` 回复且 status==OK，main.c:357-358）。
    fn set_core_flag(&mut self, slot: UserSlot);

    /// 断言进程正在退出（main.c:362 `assert(mp_flags & EXITING)`，EXIT/CORE 路径）。
    fn assert_exiting(&mut self, slot: UserSlot);

    /// 断言进程已停止（main.c:407 `assert(mp_flags & PROC_STOPPED)`，UNPAUSE 路径）。
    fn assert_stopped(&mut self, slot: UserSlot);

    /// 发布进程事件（main.c:365/413 `publish_event`，06 档落地具体订阅）。
    fn publish_event(&mut self, slot: UserSlot, event: ProcEvent);

    /// 重启执行（main.c:350 `exec_restart`，17 档落地）。
    fn exec_restart(&mut self, slot: UserSlot, args: ExecRestartArgs);

    /// 重投挂起信号（尾部，main.c:422-423 `restart_sigs`）。
    fn restart_signals(&mut self, slot: UserSlot);

    /// 置 `UNPAUSED`（main.c:410，UNPAUSE 路径）。
    fn set_unpaused(&mut self, slot: UserSlot);

    /// 请求内核中止（main.c:309 `sys_abort(abort_flag)`，REBOOT 特例）。
    fn sys_abort(&mut self);
}

/// 发送 VFS 请求（C: `utility.c:120-139` `tell_vfs`）。
///
/// 顺序与 C **一致**：① not-idle 检查 → ② 异步发送 → ③ 成功后才置 `VFS_CALL`。
/// 若先置位后发送，发送失败会留下永不清除的 `VFS_CALL`，该进程从此再无法发起
/// VFS 调用。
///
/// `call` 携带目标进程 endpoint（[`VfsCall::endpoint`]），`slot` 是 PM 侧的进程槽位，
/// 二者通常对应同一进程；对 `Reboot` 这种不与进程关联的请求，`slot` 由调用方按
/// C 约定借 `VFS` 槽位（misc.c:230）。
pub fn tell_vfs<T: IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    call: VfsCall,
    transport: &mut T,
) -> Result<(), VfsCallError> {
    // ① not-idle（utility.c:122-123）：VFS_CALL 或 EVENT_CALL 均不可
    if table.procs[slot.get()].state.block.ipc_blocked.is_some() {
        return Err(VfsCallError::NotIdle);
    }

    // ② 异步发送（utility.c:127 asynsend3(VFS_PROC_NR, AMF_NOREPLY)）
    let msg = call.encode();
    transport
        .send(Endpoint::VFS, &msg)
        .map_err(VfsCallError::SendFailed)?;

    // ③ 置 VFS_CALL（utility.c:128）——必须在发送成功后
    table.procs[slot.get()].state.block.ipc_blocked =
        Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
    Ok(())
}

/// 处理 VFS 回复（C: `main.c:294-424` `handle_vfs_reply`）。
///
/// 四段结构：① REBOOT 特例 → ② endpoint 解析 → ③ 不变式与 `NEW_PARENT` 抽取 →
/// ④ 11 路 switch + 尾部 `restart_sigs`。纯逻辑，效果与读取全部经 [`VfsReplyServices`]。
pub fn handle_vfs_reply<S: VfsReplyServices>(
    svc: &mut S,
    msg: &Message,
) -> Result<(), VfsReplyError> {
    // ① REBOOT 特例：不与任何进程关联，直接请求内核中止（main.c:304-312）
    if msg.m_type == VFS_PM_REBOOT_REPLY {
        svc.sys_abort();
        return Ok(());
    }

    // ② 解码回复（含 RS 族校验；非 RS 族 → NotAReply）
    let reply = VfsReply::decode(msg)?;

    // ③ endpoint 解析（main.c:315-321）
    let endpt_raw = unsafe { msg.m_u.m_m7.m7i1 };
    let endpt = Endpoint(endpt_raw);
    let slot = match svc.slot_of_endpoint(endpt) {
        Some(s) => s,
        None => return Err(VfsReplyError::BadEndpoint(endpt)),
    };

    // ④ 不变式 + NEW_PARENT 抽取（main.c:324-328）
    let new_parent = svc.take_vfs_call(slot);
    if svc.is_unpaused(slot) {
        // main.c:330-331
        panic!(
            "handle_vfs_reply: UNPAUSED set on entry for reply code {}",
            msg.m_type
        );
    }

    // ⑤ 11 路（main.c:334-419）
    match reply {
        VfsReply::SetUid | VfsReply::SetGid | VfsReply::SetGroups => {
            // main.c:339：唤醒原始调用者
            svc.reply(slot, OK);
        }
        VfsReply::SetSid => {
            // main.c:345：回复码为进程组号
            svc.reply(slot, svc.procgrp_of(slot));
        }
        VfsReply::Exec { status, pc, newsp, newps_str } => {
            // main.c:350-353：exec_restart 内部负责回复调用者
            svc.exec_restart(slot, ExecRestartArgs { status, pc, newsp, newps_str });
        }
        VfsReply::Core { status } => {
            // main.c:357-360：status==OK 置 WCOREFLAG，然后 fallthrough 到 EXIT
            if status == OK {
                svc.set_core_flag(slot);
            }
            svc.assert_exiting(slot);
            svc.publish_event(slot, ProcEvent::Exit);
            return Ok(()); // 提前 return，不走尾部
        }
        VfsReply::Exit => {
            // main.c:362-367：发布退出事件后 return（不做尾部 restart_sigs）
            svc.assert_exiting(slot);
            svc.publish_event(slot, ProcEvent::Exit);
            return Ok(());
        }
        VfsReply::Fork => {
            // main.c:369-396：调度新进程，再回复子进程与父进程
            if svc.sched_start_user(slot).is_err() {
                // 调度失败：拆除新进程，向父进程回复失败（除非父已死）
                svc.exit_proc(slot, -1, false);
                svc.reply_to_guardian(slot, new_parent, -1);
                return Ok(()); // 提前 return
            }
            // 调度成功：回复子进程本体（caller slot），再回复父进程（除非父已死）
            svc.reply(slot, OK);
            svc.reply_to_guardian(slot, new_parent, svc.pid_of(slot));
            // 走到尾部 restart_sigs
        }
        VfsReply::SrvFork => {
            // main.c:398-401：服务进程 fork，无事可做
        }
        VfsReply::Unpause => {
            // main.c:403-415：置 UNPAUSED，发布信号事件后 return（不做尾部）
            svc.assert_stopped(slot);
            svc.set_unpaused(slot);
            svc.publish_event(slot, ProcEvent::Signal);
            return Ok(());
        }
        VfsReply::Reboot => unreachable!("REBOOT_REPLY 已在特例分支处理"),
    }

    // ⑥ 尾部：restart_sigs（main.c:421-423）
    // 条件 (IN_USE|EXITING) == IN_USE：在使用中且未退出。
    if svc.is_in_use(slot) && !svc.is_exiting(slot) {
        svc.restart_signals(slot);
    }
    Ok(())
}

/// 生产实现（DEFERRED 方法在对应文档落地前为 `unimplemented!`，自说明字符串标明归属）。
pub struct PmServices<'a, T: IpcTransport> {
    table: &'a mut ProcTable,
    transport: &'a mut T,
    event_registry: &'a mut crate::event::EventRegistry,
    abort_flag: i32,
}

impl<'a, T: IpcTransport> PmServices<'a, T> {
    /// 构造生产端口。
    pub fn new(
        table: &'a mut ProcTable,
        transport: &'a mut T,
        event_registry: &'a mut crate::event::EventRegistry,
        abort_flag: i32,
    ) -> Self {
        Self {
            table,
            transport,
            event_registry,
            abort_flag,
        }
    }

    fn send_reply(&mut self, slot: UserSlot, code: i32) {
        let ep = self.table.procs[slot.get()].endpoint();
        let msg = Message {
            m_type: code,
            ..Default::default()
        };
        // C 的 reply() 在发送失败时仅告警不 panic；这里保持一致。
        if let Err(e) = self.transport.send(ep, &msg) {
            eprintln!("PM: vfs reply to slot {} failed: {:?}", slot.get(), e);
        }
    }
}

impl<'a, T: IpcTransport> VfsReplyServices for PmServices<'a, T> {
    fn slot_of_endpoint(&self, endpoint: Endpoint) -> Option<UserSlot> {
        self.table.pm_isokendpt(endpoint).ok()
    }

    fn take_vfs_call(&mut self, slot: UserSlot) -> bool {
        // main.c:324-328：先校验 VFS_CALL 置位，再抽取 NEW_PARENT 并清除。
        let proc = &mut self.table.procs[slot.get()];
        match proc.state.block.ipc_blocked.take() {
            Some(IpcBlockReason::VfsCall { reply_to_new_parent }) => reply_to_new_parent,
            _ => panic!(
                "handle_vfs_reply: reply without request (slot {})",
                slot.get()
            ),
        }
    }

    fn is_unpaused(&self, slot: UserSlot) -> bool {
        self.table.procs[slot.get()].state.block.unpaused
    }

    fn is_exiting(&self, slot: UserSlot) -> bool {
        self.table.procs[slot.get()].is_exiting()
    }

    fn is_in_use(&self, slot: UserSlot) -> bool {
        self.table.procs[slot.get()].is_in_use()
    }

    fn procgrp_of(&self, slot: UserSlot) -> Pid {
        self.table.procs[slot.get()].procgrp()
    }

    fn pid_of(&self, slot: UserSlot) -> Pid {
        self.table.procs[slot.get()].pid()
    }

    fn parent_slot(&self, slot: UserSlot) -> UserSlot {
        self.table.procs[slot.get()].parent()
    }

    fn reply(&mut self, slot: UserSlot, code: i32) {
        self.send_reply(slot, code);
    }

    fn reply_to_guardian(&mut self, slot: UserSlot, suppress: bool, code: i32) {
        if suppress {
            // main.c:384/392：父进程已死（被 INIT 收养），抑制回复。
            return;
        }
        let parent = self.table.procs[slot.get()].parent();
        self.send_reply(parent, code);
    }

    fn sched_start_user(&mut self, slot: UserSlot) -> Result<(), i32> {
        let sched = self.table.procs[slot.get()].resources.scheduler;
        // main.c:372：调度器为内核或 NONE 时无需显式启动。
        if sched == Endpoint::KERNEL || sched == Endpoint::NONE {
            return Ok(());
        }
        // [ARCH A-3] 调度器非内核：需在 16-scheduling.md 落地 sched_start_user 系统调用。
        unimplemented!("DEFERRED: sched_start_user for non-kernel scheduler — 见 16-scheduling.md")
    }

    fn exit_proc(&mut self, slot: UserSlot, status: i32, dump_core: bool) {
        // [ARCH A-9] 进程退出推进 — 见 09-pm-exit.md。
        let _ = (slot, status, dump_core);
        unimplemented!("DEFERRED: exit_proc — 见 09-pm-exit.md")
    }

    fn set_core_flag(&mut self, slot: UserSlot) {
        // [ARCH A-9] core flag 落位 — 见 09-pm-exit.md（当前 Process 尚无 sigstatus 字段）。
        let _ = slot;
        unimplemented!("DEFERRED: set_core_flag (WCOREFLAG) — 见 09-pm-exit.md")
    }

    fn assert_exiting(&mut self, slot: UserSlot) {
        assert!(
            self.table.procs[slot.get()].is_exiting(),
            "handle_vfs_reply: EXIT/CORE reply but process is not EXITING (slot {})",
            slot.get()
        );
    }

    fn assert_stopped(&mut self, slot: UserSlot) {
        assert!(
            self.table.procs[slot.get()].state.block.stopped,
            "handle_vfs_reply: UNPAUSE reply but process not PROC_STOPPED (slot {})",
            slot.get()
        );
    }

    fn publish_event(&mut self, slot: UserSlot, event: ProcEvent) {
        // [ARCH A-9] 事件订阅/发布 — 见 06-event-subscription.md。
        // 校验传入事件与目标进程标志推断一致，再经 EventRegistry 发布
        // （复用 `publish_event` 的 `PRIV_PROC|EXITING` 清理与串行化）。
        let inferred = {
            let proc = &self.table.procs[slot.get()];
            if proc.is_exiting() {
                ProcEvent::Exit
            } else if proc.state.block.unpaused {
                ProcEvent::Signal
            } else {
                panic!(
                    "publish_event: unknown event for slot {} flags {:?} unpaused {}",
                    slot.get(),
                    proc.state.lifecycle,
                    proc.state.block.unpaused
                );
            }
        };
        assert_eq!(
            inferred, event,
            "publish_event: caller event {:?} != inferred {:?} for slot {}",
            event, inferred, slot.get()
        );
        // 通过 EventRegistry 发布（借用拆分：table / transport / registry 为不相交字段）
        // 使用原始指针拆分以满足 borrow checker 对 &mut self 的不相交借用
        let table_ptr = self.table as *mut ProcTable;
        let transport_ptr = self.transport as *mut T;
        let registry_ptr = self.event_registry as *mut crate::event::EventRegistry;
        unsafe {
            (*registry_ptr).publish_event(slot, &mut *table_ptr, &mut *transport_ptr);
        }
    }

    fn exec_restart(&mut self, slot: UserSlot, args: ExecRestartArgs) {
        // [ARCH A-6] 执行重启 — 见 17-exec.md。
        let _ = (slot, args);
        unimplemented!("DEFERRED: exec_restart — 见 17-exec.md")
    }

    fn restart_signals(&mut self, slot: UserSlot) {
        // [ARCH A-2] 信号重投 — 见 13-signal-flow.md。
        //
        // DEFERRED 脚手架：当前 PM 尚未建模挂起信号，且此方法是每条成功路径的
        // 尾部清理，若 `unimplemented!()` 会让整个协议在集成层无法端到端验证。
        // 13 落地后由真实实现替换（投递 pending 信号）。
        let _ = slot;
    }

    fn set_unpaused(&mut self, slot: UserSlot) {
        self.table.procs[slot.get()].state.block.unpaused = true;
    }

    fn sys_abort(&mut self) {
        // [ARCH A-3] 内核中止原语 — 见 01-stage-kernel。
        let _ = self.abort_flag;
        unimplemented!("DEFERRED: sys_abort — 见 01-stage-kernel.md")
    }
}

/// 测试用录制实现：把端口调用序列存进 [`RecordedEffect`]，供断言。
///
/// 读取方法返回构造时配置的状态（`endpoint_slot` / `unpaused` / `exiting` / `in_use` /
/// `procgrp` / `pid` / `parent` / `next_new_parent` / `next_sched_result`），使状态机
/// 的每条分支都可确定性断言，而无需真实 `ProcTable` / `IpcTransport`。
#[cfg(test)]
pub struct RecordingServices {
    pub effects: Vec<RecordedEffect>,
    pub endpoint_slot: Option<(Endpoint, UserSlot)>,
    pub unpaused: bool,
    pub exiting: bool,
    pub in_use: bool,
    pub procgrp: Pid,
    pub pid: Pid,
    pub parent: UserSlot,
    pub next_new_parent: bool,
    pub next_sched_result: Result<(), i32>,
}

/// [`RecordingServices`] 录制的端口效果。
///
/// 读取类变体（SlotOfEndpoint/IsUnpaused/...）仅由纯读取方法产生；录制实现
/// 当前不记录纯读取调用（见 `VfsReplyServices` 读取方法签名 `&self`），故标注
/// `allow(dead_code)`。
#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedEffect {
    SlotOfEndpoint { endpoint: Endpoint },
    TakeVfsCall { slot: UserSlot, new_parent: bool },
    IsUnpaused { slot: UserSlot },
    IsExiting { slot: UserSlot },
    IsInUse { slot: UserSlot },
    ProcgrpOf { slot: UserSlot },
    PidOf { slot: UserSlot },
    ParentSlot { slot: UserSlot },
    Reply { slot: UserSlot, code: i32 },
    ReplyToGuardian { slot: UserSlot, suppressed: bool, code: i32 },
    SchedStartUser { slot: UserSlot, result: Result<(), i32> },
    ExitProc { slot: UserSlot, status: i32, dump_core: bool },
    SetCoreFlag { slot: UserSlot },
    AssertExiting { slot: UserSlot },
    AssertStopped { slot: UserSlot },
    PublishEvent { slot: UserSlot, event: ProcEvent },
    ExecRestart { slot: UserSlot, args: ExecRestartArgs },
    RestartSignals { slot: UserSlot },
    SetUnpaused { slot: UserSlot },
    SysAbort,
}

#[cfg(test)]
impl RecordingServices {
    /// 构造默认配置：若 endpoint 命中则解析为 `slot(1)`，其余状态为典型运行值。
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            endpoint_slot: Some((Endpoint::from_generation_slot(1, 1), UserSlot::new(1))),
            unpaused: false,
            exiting: false,
            in_use: true,
            procgrp: 7,
            pid: 100,
            parent: UserSlot::new(0),
            next_new_parent: false,
            next_sched_result: Ok(()),
        }
    }
}

#[cfg(test)]
impl VfsReplyServices for RecordingServices {
    fn slot_of_endpoint(&self, endpoint: Endpoint) -> Option<UserSlot> {
        self.endpoint_slot
            .filter(|(ep, _)| *ep == endpoint)
            .map(|(_, s)| s)
    }

    fn take_vfs_call(&mut self, slot: UserSlot) -> bool {
        let np = self.next_new_parent;
        self.effects.push(RecordedEffect::TakeVfsCall { slot, new_parent: np });
        np
    }

    fn is_unpaused(&self, slot: UserSlot) -> bool {
        self.unpaused
    }

    fn is_exiting(&self, slot: UserSlot) -> bool {
        self.exiting
    }

    fn is_in_use(&self, slot: UserSlot) -> bool {
        self.in_use
    }

    fn procgrp_of(&self, slot: UserSlot) -> Pid {
        self.procgrp
    }

    fn pid_of(&self, slot: UserSlot) -> Pid {
        self.pid
    }

    fn parent_slot(&self, slot: UserSlot) -> UserSlot {
        self.parent
    }

    fn reply(&mut self, slot: UserSlot, code: i32) {
        self.effects.push(RecordedEffect::Reply { slot, code });
    }

    fn reply_to_guardian(&mut self, slot: UserSlot, suppress: bool, code: i32) {
        self.effects
            .push(RecordedEffect::ReplyToGuardian { slot, suppressed: suppress, code });
    }

    fn sched_start_user(&mut self, slot: UserSlot) -> Result<(), i32> {
        let r = self.next_sched_result;
        self.effects.push(RecordedEffect::SchedStartUser { slot, result: r });
        r
    }

    fn exit_proc(&mut self, slot: UserSlot, status: i32, dump_core: bool) {
        self.effects
            .push(RecordedEffect::ExitProc { slot, status, dump_core });
    }

    fn set_core_flag(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::SetCoreFlag { slot });
    }

    fn assert_exiting(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::AssertExiting { slot });
    }

    fn assert_stopped(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::AssertStopped { slot });
    }

    fn publish_event(&mut self, slot: UserSlot, event: ProcEvent) {
        self.effects.push(RecordedEffect::PublishEvent { slot, event });
    }

    fn exec_restart(&mut self, slot: UserSlot, args: ExecRestartArgs) {
        self.effects.push(RecordedEffect::ExecRestart { slot, args });
    }

    fn restart_signals(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::RestartSignals { slot });
    }

    fn set_unpaused(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::SetUnpaused { slot });
    }

    fn sys_abort(&mut self) {
        self.effects.push(RecordedEffect::SysAbort);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{
        Message, VFS_PM_CORE_REPLY, VFS_PM_EXIT_REPLY, VFS_PM_EXEC_REPLY, VFS_PM_FORK_REPLY,
        VFS_PM_SETGID_REPLY, VFS_PM_SETGROUPS_REPLY, VFS_PM_SETSID_REPLY, VFS_PM_SETUID_REPLY,
        VFS_PM_SRV_FORK_REPLY, VFS_PM_UNPAUSE_REPLY,
    };

    fn reply_msg(m_type: i32, endpt: Endpoint) -> Message {
        let mut m = Message {
            m_type,
            ..Default::default()
        };
        unsafe {
            m.m_u.m_m7.m7i1 = endpt.get();
        }
        m
    }

    #[test]
    fn test_reboot_reply_is_special_cased() {
        let mut svc = RecordingServices::new();
        let msg = reply_msg(VFS_PM_REBOOT_REPLY, Endpoint::VFS);
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert_eq!(svc.effects, vec![RecordedEffect::SysAbort]);
    }

    #[test]
    fn test_setuid_setgid_setgroups_wake_caller() {
        for m_type in [VFS_PM_SETUID_REPLY, VFS_PM_SETGID_REPLY, VFS_PM_SETGROUPS_REPLY] {
            let mut svc = RecordingServices::new();
            let msg = reply_msg(m_type, Endpoint::from_generation_slot(1, 1));
            handle_vfs_reply(&mut svc, &msg).unwrap();
            // slot_of_endpoint → take_vfs_call → reply(OK) → is_in_use → restart_signals
            assert_eq!(
                svc.effects.last(),
                Some(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) })
            );
            assert!(svc.effects.contains(&RecordedEffect::Reply {
                slot: UserSlot::new(1),
                code: OK
            }));
        }
    }

    #[test]
    fn test_setsid_replies_procgrp() {
        let mut svc = RecordingServices::new();
        svc.procgrp = 42;
        let msg = reply_msg(VFS_PM_SETSID_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: 42
        }));
    }

    #[test]
    fn test_exec_restart_receives_args() {
        let mut svc = RecordingServices::new();
        let mut msg = reply_msg(VFS_PM_EXEC_REPLY, Endpoint::from_generation_slot(1, 1));
        unsafe {
            msg.m_u.m_m7.m7i2 = 0; // status OK
            msg.m_u.m_m7.m7p1 = 0x1000;
            msg.m_u.m_m7.m7p2 = 0x2000;
            msg.m_u.m_m7.m7i5 = 0x3000;
        }
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::ExecRestart {
            slot: UserSlot::new(1),
            args: ExecRestartArgs { status: 0, pc: 0x1000, newsp: 0x2000, newps_str: 0x3000 },
        }));
    }

    #[test]
    fn test_core_sets_flag_then_exit_event_early_return() {
        let mut svc = RecordingServices::new();
        svc.exiting = true; // EXITING 置位，供 assert_exiting 通过
        let mut msg = reply_msg(VFS_PM_CORE_REPLY, Endpoint::from_generation_slot(1, 1));
        unsafe {
            msg.m_u.m_m7.m7i2 = 0; // status OK
        }
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // Core: set_core_flag → assert_exiting → publish(Exit) → 提前 return（无 restart_signals）
        assert!(svc.effects.contains(&RecordedEffect::SetCoreFlag { slot: UserSlot::new(1) }));
        assert!(svc.effects.contains(&RecordedEffect::PublishEvent {
            slot: UserSlot::new(1),
            event: ProcEvent::Exit,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_exit_publishes_event_early_return() {
        let mut svc = RecordingServices::new();
        svc.exiting = true;
        let msg = reply_msg(VFS_PM_EXIT_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::PublishEvent {
            slot: UserSlot::new(1),
            event: ProcEvent::Exit,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_fork_success_replies_child_and_parent_then_tail() {
        let mut svc = RecordingServices::new();
        svc.next_sched_result = Ok(());
        svc.pid = 1234;
        let msg = reply_msg(VFS_PM_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // sched OK → reply(child, OK) + reply_to_guardian(parent, suppress=false, pid) + tail
        assert!(svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: OK,
        }));
        assert!(svc.effects.contains(&RecordedEffect::ReplyToGuardian {
            slot: UserSlot::new(1),
            suppressed: false,
            code: 1234,
        }));
        assert!(svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_fork_success_suppressed_guardian_when_new_parent() {
        let mut svc = RecordingServices::new();
        svc.next_sched_result = Ok(());
        svc.next_new_parent = true; // 父进程已死
        svc.pid = 1234; // 即便抑制，传入的回复码仍是子进程 PID
        let msg = reply_msg(VFS_PM_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: OK,
        }));
        assert!(svc.effects.contains(&RecordedEffect::ReplyToGuardian {
            slot: UserSlot::new(1),
            suppressed: true,
            code: 1234, // pid 配置默认 100，但被 suppress 忽略
        }));
    }

    #[test]
    fn test_fork_sched_failure_tears_down_and_replies_parent() {
        let mut svc = RecordingServices::new();
        svc.next_sched_result = Err(-1);
        let msg = reply_msg(VFS_PM_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // sched 失败 → exit_proc(slot, -1, false) + reply_to_guardian(parent, false, -1) + return
        assert!(svc.effects.contains(&RecordedEffect::ExitProc {
            slot: UserSlot::new(1),
            status: -1,
            dump_core: false,
        }));
        assert!(svc.effects.contains(&RecordedEffect::ReplyToGuardian {
            slot: UserSlot::new(1),
            suppressed: false,
            code: -1,
        }));
        // 没有回复子进程、没有 restart_signals
        assert!(!svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: OK,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_srv_fork_is_noop_then_tail() {
        let mut svc = RecordingServices::new();
        let msg = reply_msg(VFS_PM_SRV_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // 无任何 reply/exit，仅尾部 restart_signals
        assert!(svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
        assert!(!svc.effects.iter().any(|e| matches!(e, RecordedEffect::Reply { .. })));
    }

    #[test]
    fn test_unpause_sets_flag_publishes_signal_early_return() {
        let mut svc = RecordingServices::new();
        let msg = reply_msg(VFS_PM_UNPAUSE_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::SetUnpaused { slot: UserSlot::new(1) }));
        assert!(svc.effects.contains(&RecordedEffect::PublishEvent {
            slot: UserSlot::new(1),
            event: ProcEvent::Signal,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_bad_endpoint_replies_with_error() {
        let mut svc = RecordingServices::new();
        svc.endpoint_slot = None; // 任何 endpoint 都解析失败
        let msg = reply_msg(VFS_PM_SETUID_REPLY, Endpoint::from_generation_slot(1, 1));
        let err = handle_vfs_reply(&mut svc, &msg).unwrap_err();
        assert_eq!(err, VfsReplyError::BadEndpoint(Endpoint::from_generation_slot(1, 1)));
    }

    #[test]
    fn test_unpaused_on_entry_panics() {
        let mut svc = RecordingServices::new();
        svc.unpaused = true; // 入口即 UNPAUSED → 入口不变式失败
        let msg = reply_msg(VFS_PM_SETUID_REPLY, Endpoint::from_generation_slot(1, 1));
        let result = std::panic::catch_unwind(move || {
            let mut svc = svc;
            let _ = handle_vfs_reply(&mut svc, &msg);
        });
        assert!(result.is_err(), "入口 UNPAUSED 必须 panic");
    }

    #[test]
    fn test_decode_error_propagates() {
        // 非 RS 族的回复 → VfsReply::decode 返回 NotAReply，但 run_once 仅在 is_vfs_pm_rs
        // 时调用本函数；这里直接验证函数对 NotAReply 的传播（用普通 PM 调用码）。
        let mut svc = RecordingServices::new();
        let msg = reply_msg(0 /* 非 RS 族 */, Endpoint::from_generation_slot(1, 1));
        // 0 不在 RS 族 → decode 失败。但 slot_of_endpoint 仍会被调用（先解码再解析）。
        let err = handle_vfs_reply(&mut svc, &msg).unwrap_err();
        // 0 的族判定：is_vfs_pm_rs(0) = (0 & !0x7f)==0x980? 否 → NotAReply(0)
        assert_eq!(err, VfsReplyError::NotAReply(0));
    }

    #[test]
    fn test_tell_vfs_sets_vfs_call_on_success() {
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::ProcTable;
        use minix_types::VfsCall;

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let slot = UserSlot::new(1);
        let call = VfsCall::SetUid {
            endpoint: Endpoint::from_generation_slot(1, 1),
            eid: 0,
            rid: 0,
        };
        let r = tell_vfs(&mut table, slot, call, &mut transport);
        assert!(r.is_ok());
        assert_eq!(
            table.procs[slot.get()].state.block.ipc_blocked,
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: false })
        );
        // TestIpcTransport 应记录一条发往 VFS 的消息
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.last_sent_dest(), Some(Endpoint::VFS));
    }

    #[test]
    fn test_tell_vfs_not_idle_when_blocked() {
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::ProcTable;
        use minix_types::VfsCall;

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let slot = UserSlot::new(1);
        // 预先置位（模拟已在进行 VFS 调用或事件调用）
        table.procs[slot.get()].state.block.ipc_blocked =
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let call = VfsCall::SetUid {
            endpoint: Endpoint::from_generation_slot(1, 1),
            eid: 0,
            rid: 0,
        };
        let r = tell_vfs(&mut table, slot, call, &mut transport);
        assert_eq!(r, Err(VfsCallError::NotIdle));
        assert_eq!(transport.sent().len(), 0);
    }
}
