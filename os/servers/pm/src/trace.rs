//! Ptrace: `do_trace` + `trace_stop`.
//!
//! C ground truth: `minix3/minix/servers/pm/trace.c` (276 lines)；
//! `T_*` 常量全集 = `sys/sys/ptrace.h:226-250`（PT_* 别名）+ `:238-250`
//! （Minix 专属命令）。`T_OK/T_ATTACH/T_EXIT/T_SETOPT` 在 PM 内闭环，
//! `T_RESUME/T_STEP/T_SYSCALL/T_DETACH` 半本地半内核（break 后落穿
//! `sys_trace` 透传，trace.c:244-249），其余命令整体透传。
//! 设计：`.design/18-design.v1.md` D1–D8；wire = `MessLcPmPtrace`/
//! `MessPmLcPtrace`（minix-types，2026-09-09 V3-P1-1 切片）。
//! Single-threaded — `&mut ProcTable` without `Arc`.

use crate::ipc::ReplyIntent;
use crate::mproc::{ProcTable, Guardianship, TraceOptions, WaitTarget};
use minix_types::{Endpoint, UserSlot, Pid, EINVAL, EPERM, EBUSY, ESRCH, OK};
use minix_types::Message;

use core::mem::size_of;

/// `T_*` 命令全集（`sys/sys/ptrace.h:226-250`，值 = PT_* 别名与
/// Minix 专属段）。**逐值对账 C**（`test_constants_match_c` 全量断言）：
/// V3-P1-1 之前 15/18 的值是自造序号，且 `T_STOP` 与 `T_SETDATA` 同为 6。
pub const T_OK: i32 = 0; // PT_TRACE_ME
pub const T_GETINS: i32 = 1; // PT_READ_I
pub const T_GETDATA: i32 = 2; // PT_READ_D
pub const T_SETINS: i32 = 4; // PT_WRITE_I
pub const T_SETDATA: i32 = 5; // PT_WRITE_D
pub const T_RESUME: i32 = 7; // PT_CONTINUE
pub const T_EXIT: i32 = 8; // PT_KILL
pub const T_ATTACH: i32 = 9; // PT_ATTACH
pub const T_DETACH: i32 = 10; // PT_DETACH
pub const T_SYSCALL: i32 = 14; // PT_SYSCALL
pub const T_STOP: i32 = -1;
pub const T_READB_INS: i32 = 100;
pub const T_WRITEB_INS: i32 = 101;
pub const T_GETUSER: i32 = 102;
pub const T_SETUSER: i32 = 103;
pub const T_STEP: i32 = 104;
pub const T_SETOPT: i32 = 105;
pub const T_GETRANGE: i32 = 106;
pub const T_SETRANGE: i32 = 107;

/// C `mproc.h:100` `TRACE_EXIT`（`mp_trace_flags` 的状态位，非 TO_* 选项位）。
/// C 把它和选项放在同一个字里（T_SETOPT 整字赋值可触达）；Rust 把选项
///（`TraceOptions`）与状态（`TraceState.exit_pending`）分列，本常量用于
/// T_SETOPT 时从整字里摘出状态位。
pub const TRACE_EXIT_FLAG: u32 = 0x8000;

/// C `_NSIG`（`sys/signal.h:45`）——信号号上界（不含）。
const NSIG: i64 = 64;

/// C `sys/ptrace.h:216-225` 的 trace space 取值（`ptrace_range.pr_space`）。
const TS_INS: i32 = 0;
const TS_DATA: i32 = 1;

/// `W_STOPCODE`（`sys/wait.h`）——停止态 wait status 编码。
pub fn w_stopcode(sig: i32) -> i32 {
    (sig << 8) | 0x7f
}

/// Errors for `do_trace`。`Kernel` 携带内核 SYS_TRACE 的原始返回值——
/// C 全程透传 `r`（trace.c:108/131/246），不折叠为 EINVAL（V3-P2-6 规约
/// 在本文件的落地）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceError {
    Busy,
    Srch,
    Perm,
    Inval,
    Kernel(i32),
}

impl TraceError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Busy => EBUSY,
            Self::Srch => ESRCH,
            Self::Perm => EPERM,
            Self::Inval => EINVAL,
            Self::Kernel(r) => r,
        }
    }
}

/// `ptrace` request（`m_lc_pm_ptrace`，`ipc.h:492-501`；wire =
/// `minix_types::MessLcPmPtrace`）。`data` 是 C `long`（有符号）：
/// T_RESUME/T_DETACH 的负值 = "不投递信号"，是合法入参。
#[derive(Debug, Clone, Copy)]
pub struct PtraceReq {
    pub req: i32,
    pub pid: Pid,
    pub addr: u64,
    pub data: i64,
}

/// C `struct ptrace_range`（`sys/ptrace.h:216-225`，x86-64 布局 32 字节）：
/// T_GETRANGE/T_SETRANGE 经 `sys_datacopy` 从调试器内存取出（trace.c:169）。
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PtraceRange {
    pr_space: i32,
    _pad: u32,
    pr_addr: i64,
    pr_ptr: u64,
    pr_size: u64,
}

impl PtraceRange {
    /// 以字节视图取/写结构体（C 的 `sys_datacopy(..., &pr, sizeof(pr))`
    /// 语义：参数块是外来字节，合法性由调用方校验，不需要类型安全）。
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: repr(C) 的 POD 结构（全整数字段），任意字节写入合法。
        unsafe { core::slice::from_raw_parts_mut(self as *mut Self as *mut u8, size_of::<Self>()) }
    }
}

/// 把 `data` 写入 caller 的预填回复载荷（C: `mp_reply.m_pm_lc_ptrace.data`，
/// trace.c:59/92/110/133/164/187/233/248），主循环 `reply(caller, result)`
/// 以 m_type 携带返回码后整体发出（wait4 的 tag + typed body 契约，
/// D-26/Fix #22 同型）。
fn trace_reply_data(table: &mut ProcTable, caller: UserSlot, data: i64) {
    let mut reply = Message::default();
    reply.m_u.m_pm_lc_ptrace.data = data;
    table.procs[caller.get()].ipc.reply = Some(reply);
}

/// 把 `sig_proc`（trace=FALSE 的重投形态）适配为 `check_pending` 的
/// `SignalDeliver` 注入口——T_DETACH/T_RESUME 的 check_pending 尾巴
///（trace.c:213/240 直接调 check_pending）。
struct TraceDeliver<'a, T: crate::ipc::IpcTransport + ?Sized> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    transport: &'a mut T,
}

impl<T: crate::ipc::IpcTransport + ?Sized> crate::signal_flow::SignalDeliver for TraceDeliver<'_, T> {
    fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, signo: i32, ksig: bool) {
        let _ = crate::signal::sig_proc(table, target, signo, false, ksig, self.kern, self.transport);
    }
}

/// C do_trace 的 switch 落穿段（trace.c:244-249）：`sys_trace` 透传，
/// 失败透传原始 errno（不折叠），读值回填 reply 载荷。
fn passthrough(
    kern: &mut dyn crate::exit::KernelGateway,
    table: &mut ProcTable,
    caller: UserSlot,
    child: UserSlot,
    req: PtraceReq,
) -> Result<ReplyIntent, TraceError> {
    let ep = table.procs[child.get()].endpoint();
    let mut data = req.data;
    kern.sys_trace(req.req, ep, req.addr, &mut data)
        .map_err(TraceError::Kernel)?;
    trace_reply_data(table, caller, data);
    Ok(ReplyIntent::Reply(OK))
}

/// `do_trace`（trace.c:42-250，D1–D8）。
///
/// 与 C 的结构对应：T_OK/T_ATTACH/T_STOP/T_READB_INS/T_WRITEB_INS 各自
/// 独立处理；其余命令先过通用守卫（trace.c:140-143：存在/未退出/
/// tracer 匹配/TRACE_STOPPED）再分派；T_DETACH/T_RESUME/T_STEP/
/// T_SYSCALL 尾部落穿 `sys_trace` 透传（C 的 switch `break` 语义）。
pub fn do_trace<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    req: PtraceReq,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<ReplyIntent, TraceError> {
    // caller 是否 root：C 的 `mp_effuid != SUPER_USER`（trace.c:67/74/102/114）。
    // Kernel 特权槽 = C 的系统进程（effuid 恒 0，见 todo.md §12.3 观察 6）。
    let caller_root = table.procs[caller.get()]
        .resources
        .privilege
        .credentials()
        .map(|c| c.user.effective == 0)
        .unwrap_or(true);
    let caller_is_kernel = table.procs[caller.get()].is_kernel_process();

    match req.req {
        // T_OK：tracer 已存在 → EBUSY；tracer = parent；reply.data = 0
        //（trace.c:55-60）。经 D1 承诺的 `try_set_tracer`（18 文档设计基线）。
        T_OK => {
            let parent = table.procs[caller.get()].parent();
            table.procs[caller.get()]
                .state
                .guardianship
                .try_set_tracer(parent)
                .map_err(|_| TraceError::Busy)?;
            trace_reply_data(table, caller, 0);
            Ok(ReplyIntent::Reply(OK))
        }
        // T_ATTACH：attach 到既有进程（trace.c:62-93）。
        T_ATTACH => {
            let child = table.find_proc(req.pid).ok_or(TraceError::Srch)?;
            if table.procs[child.get()].is_exiting() {
                return Err(TraceError::Srch);
            }
            // 非 root：eff uid/gid 匹配 + 目标未 setuid/setgid（trace.c:67-71）。
            let caller_creds = table.procs[caller.get()]
                .resources.privilege.credentials().cloned().unwrap_or_default();
            let child_creds = table.procs[child.get()]
                .resources.privilege.credentials().cloned().unwrap_or_default();
            if !caller_root
                && (caller_creds.user.effective != child_creds.user.effective
                    || caller_creds.group.effective != child_creds.group.effective
                    || child_creds.user.effective != child_creds.user.real
                    || child_creds.group.effective != child_creds.group.real)
            {
                return Err(TraceError::Perm);
            }
            // 仅 root 可 trace 系统进程（trace.c:74-75）。
            if !caller_root && table.procs[child.get()].is_kernel_process() {
                return Err(TraceError::Perm);
            }
            // 系统进程不得 trace 任何人（它们直接用 sys_trace，trace.c:77-78）。
            if caller_is_kernel {
                return Err(TraceError::Perm);
            }
            // 不能 trace 自己/PM/VM（trace.c:81-82）。
            if child == caller
                || table.procs[child.get()].endpoint() == Endpoint::PM
                || table.procs[child.get()].endpoint() == Endpoint::VM
            {
                return Err(TraceError::Perm);
            }
            // 已被 trace → EBUSY（trace.c:85）。
            if table.procs[child.get()].tracer().is_some() {
                return Err(TraceError::Busy);
            }
            let parent = table.procs[child.get()].parent();
            table.procs[child.get()].state.guardianship = Guardianship::Traced {
                parent,
                tracer: caller,
                trace_options: TraceOptions::NOEXEC, // C: mp_trace_flags = TO_NOEXEC（trace.c:88）
            };
            // sig_proc(SIGSTOP, trace=TRUE)（trace.c:90）——经 sig_proc 的
            // TRACE 分支进 trace_stop：内核停住 + tracer 等待时收到 W_STOPCODE。
            let _ = crate::signal::sig_proc(table, child, crate::signal::SIGSTOP, true, false, kern, transport);
            trace_reply_data(table, caller, 0);
            Ok(ReplyIntent::Reply(OK))
        }
        // T_STOP：不暴露给用户程序（效果 = kill + trace），一律 EINVAL
        //（trace.c:95-99）。内部停止走 trace_stop()，不经消息入口。
        T_STOP => Err(TraceError::Inval),
        // READB/WRITEB_INS：root 专属的文本段读写 hack（trace.c:101-134）。
        // 注意：root 门在通用守卫（140-143）**之前**，不需要 tracer/
        // TRACE_STOPPED——C 在 140-143 之前 switch 提前 return。
        preq if preq == T_READB_INS || preq == T_WRITEB_INS => {
            if !caller_root {
                return Err(TraceError::Perm);
            }
            let child = table.find_proc(req.pid).ok_or(TraceError::Srch)?;
            if table.procs[child.get()].is_exiting() {
                return Err(TraceError::Srch);
            }
            let ep = table.procs[child.get()].endpoint();
            let mut data = req.data;
            kern.sys_trace(preq, ep, req.addr, &mut data)
                .map_err(TraceError::Kernel)?;
            trace_reply_data(table, caller, data);
            Ok(ReplyIntent::Reply(OK))
        }
        // 其余命令：通用守卫（trace.c:140-143）。
        _ => {
            let child = table.find_proc(req.pid).ok_or(TraceError::Srch)?;
            if table.procs[child.get()].is_exiting() {
                return Err(TraceError::Srch);
            }
            if table.procs[child.get()].tracer() != Some(caller) {
                return Err(TraceError::Srch);
            }
            if !table.procs[child.get()].state.trace.stopped {
                return Err(TraceError::Busy);
            }
            match req.req {
                // T_EXIT：TRACE_EXIT 置位；有 VFS/EVENT 在途调用则保存
                // exitstatus 等回复，否则立即 exit_proc；SUSPEND 等收尾
                //（trace.c:146-159）。
                T_EXIT => {
                    table.procs[child.get()].state.trace.exit_pending = true;
                    let blocked = table.procs[child.get()].state.block.is_vfs_blocked()
                        || table.procs[child.get()].state.block.is_event_blocked();
                    if blocked {
                        // C: mp_exitstatus = data（char 截断，mproc.h:25）。
                        table.procs[child.get()].state.lifecycle =
                            crate::mproc::Lifecycle::Exiting { exit_code: req.data as i8, sig_status: 0 };
                    } else {
                        crate::exit::exit_proc(table, child, req.data as i8, false, transport, kern);
                    }
                    Ok(ReplyIntent::ReplyLater) // SUSPEND
                }
                // T_SETOPT：整字赋值（trace.c:162）——TO_* 选项位归
                // TraceOptions；TRACE_EXIT 状态位（0x8000）摘给
                // TraceState.exit_pending；其余状态位（STOPPED/ZOMBIE）
                // 属 C 的标志混杂，Rust 由 Lifecycle/TraceState 建模
                //（setopt 触达它们是未定义用法，注释声明差异）。
                T_SETOPT => {
                    let bits = req.data as u32;
                    table.procs[child.get()].state.guardianship.set_trace_options(bits);
                    table.procs[child.get()].state.trace.exit_pending =
                        bits & TRACE_EXIT_FLAG != 0;
                    trace_reply_data(table, caller, 0);
                    Ok(ReplyIntent::Reply(OK))
                }
                // T_GETRANGE/T_SETRANGE：从调试器内存取 ptrace_range 参数块，
                // 校验 space 与 size，vircopy 双向搬运（trace.c:167-188）。
                T_GETRANGE | T_SETRANGE => {
                    let mut pr = PtraceRange::default();
                    let caller_ep = table.procs[caller.get()].endpoint();
                    kern.copy_from_user(caller_ep, req.addr, pr.as_bytes_mut())
                        .map_err(TraceError::Kernel)?;
                    if pr.pr_space != TS_INS && pr.pr_space != TS_DATA {
                        return Err(TraceError::Inval);
                    }
                    // C: `pr_size == 0 || pr_size > LONG_MAX → EINVAL`。
                    if pr.pr_size == 0 || pr.pr_size > i64::MAX as u64 {
                        return Err(TraceError::Inval);
                    }
                    let child_ep = table.procs[child.get()].endpoint();
                    if req.req == T_GETRANGE {
                        // 子 → 调试器（trace.c:176-179）。
                        kern.sys_vircopy(child_ep, pr.pr_addr as u64, caller_ep, pr.pr_ptr, pr.pr_size)
                            .map_err(TraceError::Kernel)?;
                    } else {
                        // 调试器 → 子（trace.c:180-183）。
                        kern.sys_vircopy(caller_ep, pr.pr_ptr, child_ep, pr.pr_addr as u64, pr.pr_size)
                            .map_err(TraceError::Kernel)?;
                    }
                    trace_reply_data(table, caller, 0);
                    Ok(ReplyIntent::Reply(OK))
                }
                // T_DETACH（trace.c:190-215）：清 tracer → sigtrace 重放
                //（check_sig 全语义）→ data 信号（trace=TRUE）→ 清
                // TRACE_STOPPED → check_pending → 落穿内核透传（内核侧
                // 完成 detach）。trace_flags 随 Traced→Normal 变体替换
                // 消失（= C trace.c:211 的清零）。
                T_DETACH => {
                    // data < 0 || >= _NSIG → EINVAL（trace.c:191）——i64
                    // 载荷保留了 C 的负值半边。
                    if req.data < 0 || req.data >= NSIG {
                        return Err(TraceError::Inval);
                    }
                    let child_pid = table.procs[child.get()].identity.id.pid;
                    table.procs[child.get()].state.guardianship =
                        Guardianship::Normal { parent: table.procs[child.get()].parent() };
                    let sigtrace = table.procs[child.get()].resources.signals.trace_mask;
                    for i in 1..NSIG {
                        if sigtrace >> (i - 1) & 1 != 0 {
                            table.procs[child.get()].resources.signals.trace_mask &= !(1u64 << (i - 1));
                            // C: check_sig(child->mp_pid, i, FALSE)（trace.c:200）。
                            let _ = crate::signal::check_sig(table, caller, child_pid, i as i32, false, kern, transport);
                        }
                    }
                    if req.data > 0 {
                        let _ = crate::signal::sig_proc(table, child, req.data as i32, true, false, kern, transport);
                    }
                    table.procs[child.get()].state.trace.stopped = false;
                    {
                        let mut deliver = TraceDeliver { kern, transport };
                        let _ = crate::signal_flow::check_pending(table, child, &mut deliver);
                    }
                    passthrough(kern, table, caller, child, req)
                }
                // T_RESUME/T_STEP/T_SYSCALL（trace.c:217-242）：data 信号
                //（trace=FALSE）→ sigtrace 短路（假成功恢复）→ 清
                // TRACE_STOPPED → check_pending → 落穿内核透传。
                T_RESUME | T_STEP | T_SYSCALL => {
                    if req.data < 0 || req.data >= NSIG {
                        return Err(TraceError::Inval);
                    }
                    if req.data > 0 {
                        let _ = crate::signal::sig_proc(table, child, req.data as i32, false, false, kern, transport);
                    }
                    // sigtrace 还有位 → 假成功（reply.data=0），不真恢复
                    //（trace.c:228-236）。
                    if table.procs[child.get()].resources.signals.trace_mask != 0 {
                        trace_reply_data(table, caller, 0);
                        return Ok(ReplyIntent::Reply(OK));
                    }
                    table.procs[child.get()].state.trace.stopped = false;
                    {
                        let mut deliver = TraceDeliver { kern, transport };
                        let _ = crate::signal_flow::check_pending(table, child, &mut deliver);
                    }
                    passthrough(kern, table, caller, child, req)
                }
                // 其余命令（T_GETINS/T_GETDATA/T_GETUSER/T_SETINS/
                // T_SETDATA/T_SETUSER）：整体内核透传，读值回填
                //（trace.c:244-249）。
                _ => passthrough(kern, table, caller, child, req),
            }
        }
    }
}

/// `trace_stop`（trace.c:255-276）：被 trace 的进程收到信号后停住它。
///
/// `sys_trace(T_STOP)` 失败即 panic（trace.c:264，C 同文案）；置
/// TRACE_STOPPED；tracer 在 wait 且目标匹配时：消费 sigtrace 位、
/// 清 WAITING、以 wait4 载荷（W_STOPCODE）+ pid tag 立即回复 tracer
///（C 经 `reply()`，Rust 走 transport 直发——消息处理中途的异步回复，
/// 与 tell_parent 同型）。
pub fn trace_stop<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child: UserSlot,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) {
    let ep = table.procs[child.get()].endpoint();
    let mut data: i64 = 0;
    kern.sys_trace(T_STOP, ep, 0, &mut data)
        .unwrap_or_else(|r| panic!("sys_trace failed: {}", r));
    table.procs[child.get()].state.trace.stopped = true;

    let tracer_slot = match table.procs[child.get()].tracer() {
        Some(t) => t,
        None => return,
    };
    let child_pid = table.procs[child.get()].identity.id.pid;
    let child_procgrp = table.procs[child.get()].identity.procgrp;
    // wait_test（forkexit.c:580-601 的匹配半边）：WAITING + 目标匹配
    //（AnyChild/SpecificChild/Group）。先取 tracer 侧快照再改 child，
    // 避免同一 ProcTable 上的借用交叠。
    let (waiting, target, tracer_ep) = {
        let rpmp = &table.procs[tracer_slot.get()];
        (rpmp.state.wait.waiting, rpmp.state.wait.target, rpmp.endpoint())
    };
    if waiting {
        let matched = match target {
            WaitTarget::AnyChild => true,
            WaitTarget::SpecificChild(p) => p == child_pid,
            WaitTarget::Group(g) => -g == child_procgrp,
        };
        if matched {
            table.procs[child.get()].resources.signals.trace_mask &= !(1u64 << (signo as u64 - 1));
            table.procs[tracer_slot.get()].state.wait.waiting = false;
            // C: mp_reply.m_pm_lc_wait4.status = W_STOPCODE(signo);
            // reply(tracer, child_pid)（trace.c:273-274）。
            let mut reply = Message { m_type: child_pid, ..Default::default() };
            reply.m_u.m_pm_lc_wait4.status = w_stopcode(signo);
            transport.send(tracer_ep, &reply).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{Lifecycle, Privilege, Credentials};

    /// 脚本化内核 mock：记录 SYS_TRACE/vircopy 调用，可脚本化返回值、
    /// SYS_TRACE 读值与 copy_from_user 的参数块内容。
    struct MockKernel {
        traces: Vec<(i32, i32, u64)>,
        trace_reply: Result<i64, i32>,
        vircopy_calls: Vec<(i32, u64, i32, u64, u64)>,
        range_block: Option<PtraceRange>,
    }
    impl Default for MockKernel {
        fn default() -> Self {
            Self { traces: Vec::new(), trace_reply: Ok(0), vircopy_calls: Vec::new(), range_block: None }
        }
    }

    impl crate::exit::KernelGateway for MockKernel {
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, req: i32, ep: Endpoint, addr: u64, data: &mut i64) -> Result<(), i32> {
            self.traces.push((req, ep.0, addr));
            match self.trace_reply {
                Ok(v) => {
                    *data = v;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        fn sys_vircopy(&mut self, src_ep: Endpoint, src: u64, dst_ep: Endpoint, dst: u64, len: u64) -> Result<(), i32> {
            self.vircopy_calls.push((src_ep.0, src, dst_ep.0, dst, len));
            Ok(())
        }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, u64)>, i32> { Ok(None) }
        fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: Endpoint, _src: u64, bytes: &mut [u8]) -> Result<(), i32> {
            match &self.range_block {
                Some(pr) => {
                    let src = unsafe {
                        core::slice::from_raw_parts(pr as *const PtraceRange as *const u8, size_of::<PtraceRange>())
                    };
                    bytes[..src.len()].copy_from_slice(src);
                }
                None => bytes.fill(0),
            }
            Ok(())
        }
    }

    fn mk_proc(table: &mut ProcTable, slot: usize, pid: Pid) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        table.procs[slot].state.guardianship = Guardianship::Normal { parent: UserSlot::new(0) };
        table.procs[slot].state.trace = crate::mproc::TraceState::default();
    }

    /// 构造"caller(0) 是 slot 5 进程的 tracer 且其已停止"的现场。
    fn traced_stopped(table: &mut ProcTable, caller: usize, target: usize) {
        table.procs[target].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(caller),
            tracer: UserSlot::new(caller),
            trace_options: TraceOptions::empty(),
        };
        table.procs[target].state.trace.stopped = true;
    }

    // ---- 常量全集对账 C（本测试的全量版：V3-P1-1 之前同名测试只断言
    // 恰好正确的 2 个常量，15 个错误值零覆盖——测试名谎报，V3 Rule
    // Discovery 的 CSL 候选模式实例）。----

    #[test]
    fn test_constants_match_c() {
        // sys/sys/ptrace.h:226-250（PT_* 别名 + Minix 专属段）逐值对账。
        assert_eq!(T_OK, 0); // PT_TRACE_ME
        assert_eq!(T_GETINS, 1); // PT_READ_I
        assert_eq!(T_GETDATA, 2); // PT_READ_D
        assert_eq!(T_SETINS, 4); // PT_WRITE_I
        assert_eq!(T_SETDATA, 5); // PT_WRITE_D
        assert_eq!(T_RESUME, 7); // PT_CONTINUE
        assert_eq!(T_EXIT, 8); // PT_KILL
        assert_eq!(T_ATTACH, 9); // PT_ATTACH
        assert_eq!(T_DETACH, 10); // PT_DETACH
        assert_eq!(T_SYSCALL, 14); // PT_SYSCALL
        assert_eq!(T_STOP, -1);
        assert_eq!(T_READB_INS, 100);
        assert_eq!(T_WRITEB_INS, 101);
        assert_eq!(T_GETUSER, 102);
        assert_eq!(T_SETUSER, 103);
        assert_eq!(T_STEP, 104);
        assert_eq!(T_SETOPT, 105);
        assert_eq!(T_GETRANGE, 106);
        assert_eq!(T_SETRANGE, 107);
    }

    #[test]
    fn test_w_stopcode() {
        assert_eq!(w_stopcode(11), (11 << 8) | 0x7f);
        assert_eq!(w_stopcode(5), (5 << 8) | 0x7f);
    }

    // ---- do_trace 分支矩阵 ----

    #[test]
    fn test_t_ok_sets_tracer_and_zero_payload() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_OK, pid: 0, addr: 0, data: 0 };
        // 首次 T_OK 成功（tracer = parent），reply 载荷 data = 0。
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        assert!(table.procs[0].tracer().is_some());
        assert_eq!(
            unsafe { table.procs[0].ipc.reply.take().unwrap().m_u.m_pm_lc_ptrace.data },
            0,
            "C trace.c:59: reply.data = 0"
        );
        // 二次 T_OK → EBUSY（trace.c:56）。
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t);
        assert_eq!(res.unwrap_err(), TraceError::Busy);
    }

    #[test]
    fn test_t_attach_sets_noexec_and_stops_child() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_ATTACH, pid: 42, addr: 0, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        // C trace.c:88：trace_flags = TO_NOEXEC。
        assert_eq!(
            table.procs[5].state.guardianship.trace_options(),
            TraceOptions::NOEXEC,
            "attach must set TO_NOEXEC (C trace.c:88)"
        );
        // C trace.c:90：sig_proc(SIGSTOP, TRUE) → TRACE 分支 → trace_stop：
        // 内核收到 T_STOP 且子进程 stopped。
        assert!(table.procs[5].state.trace.stopped, "attach must stop the child");
        assert!(
            kern.traces.iter().any(|&(req, _, _)| req == T_STOP),
            "trace_stop must issue sys_trace(T_STOP)"
        );
        // sigtrace 记录 SIGSTOP（trace 分支的 sigaddset）。
        assert_eq!(table.procs[5].resources.signals.trace_mask, 1u64 << (17 - 1));
    }

    #[test]
    fn test_t_stop_is_einval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_STOP, pid: 0, addr: 0, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval
        );
    }

    #[test]
    fn test_readb_ins_requires_root_not_tracer() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10); // caller，非 root（uid 1000）
        mk_proc(&mut table, 5, 42);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // 非 root → EPERM（trace.c:102），即使没有 tracer/TRACE_STOPPED——
        // root 门在通用守卫之前。
        let req = PtraceReq { req: T_READB_INS, pid: 42, addr: 0x100, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Perm
        );
        // root：无 tracer 也透传（守卫不适用），读值回填（trace.c:110）。
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        kern.trace_reply = Ok(0x55);
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        assert_eq!(unsafe { table.procs[0].ipc.reply.take().unwrap().m_u.m_pm_lc_ptrace.data }, 0x55);
    }

    #[test]
    fn test_t_exit_defers_when_vfs_blocked_else_exits() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // VFS_CALL 在途 → 保存 exitstatus + SUSPEND（trace.c:150-151/159）。
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let req = PtraceReq { req: T_EXIT, pid: 42, addr: 0, data: 7 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        assert!(table.procs[5].state.trace.exit_pending);
        assert!(table.procs[5].is_exiting(), "exitstatus saved as Exiting");
        // 无阻塞 → exit_proc 直达（C trace.c:153-154）：进程 EXITING 且
        // VFS 收到 VFS_PM_EXIT（exit_proc 尾部 tell_vfs）。
        let mut table2 = ProcTable::new();
        mk_proc(&mut table2, 0, 10);
        mk_proc(&mut table2, 5, 42);
        traced_stopped(&mut table2, 0, 5);
        let mut t2 = crate::ipc::TestIpcTransport::default();
        let mut kern2 = MockKernel::default();
        let req = PtraceReq { req: T_EXIT, pid: 42, addr: 0, data: 7 };
        let res = do_trace(&mut table2, UserSlot::new(0), req, &mut kern2, &mut t2).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        // exit_proc 是完整链：Exiting → zombify（exit.rs step 10-11），
        // 终态 = 僵尸（TRACE_EXIT 语境下由 zombify 处置），不再是 Exiting。
        assert!(
            matches!(
                table2.procs[5].state.lifecycle,
                Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. }
            ),
            "exit_proc must run the full chain to zombify, got {:?}",
            table2.procs[5].state.lifecycle
        );
        assert!(
            t2.sent().iter().any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
            "exit_proc must tell VFS"
        );
    }

    #[test]
    fn test_t_setopt_sets_options_and_exit_flag() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // TO_TRACEFORK|TO_NOEXEC + TRACE_EXIT 状态位（C 整字赋值语义）。
        let req = PtraceReq { req: T_SETOPT, pid: 42, addr: 0, data: (0x1 | 0x4 | TRACE_EXIT_FLAG) as i64 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        assert_eq!(
            table.procs[5].state.guardianship.trace_options(),
            TraceOptions::TRACEFORK | TraceOptions::NOEXEC
        );
        assert!(table.procs[5].state.trace.exit_pending, "TRACE_EXIT bit must reach exit_pending");
    }

    #[test]
    fn test_t_getrange_setrange_validate_and_direction() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // 参数块填零 → pr_space = 0 = TS_INS 合法，pr_size = 0 → EINVAL
        //（trace.c:174 的 size 校验）。
        let req = PtraceReq { req: T_GETRANGE, pid: 42, addr: 0x9000, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval,
            "pr_size == 0 must be EINVAL"
        );
        // 非法 space → EINVAL（trace.c:173）。
        kern.range_block = Some(PtraceRange { pr_space: 9, _pad: 0, pr_addr: 0, pr_ptr: 0, pr_size: 8 });
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval
        );
        // 合法参数块：SETRANGE 方向 = 调试器 → 子（trace.c:180-183）。
        kern.range_block = Some(PtraceRange {
            pr_space: TS_DATA,
            _pad: 0,
            pr_addr: 0x5000,
            pr_ptr: 0x9000,
            pr_size: 64,
        });
        let req = PtraceReq { req: T_SETRANGE, pid: 42, addr: 0x9000, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        let caller_ep = table.procs[0].identity.endpoint;
        let child_ep = table.procs[5].identity.endpoint;
        assert_eq!(kern.vircopy_calls, vec![(caller_ep.0, 0x9000, child_ep.0, 0x5000, 64)]);
    }

    #[test]
    fn test_t_detach_replays_sigtrace_and_falls_through_to_kernel() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        // sigtrace 有 SIGTRAP(5) 与 SIGILL(4)：重放应经 check_sig 投递。
        table.procs[5].resources.signals.trace_mask = (1u64 << (5 - 1)) | (1u64 << (4 - 1));
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_DETACH, pid: 42, addr: 0, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        // tracer 已清（trace.c:194）。
        assert!(table.procs[5].tracer().is_none());
        // 重放：sigtrace 位被消费（check_sig → sig_proc）。
        assert_eq!(table.procs[5].resources.signals.trace_mask, 0);
        // C trace.c:215 break → 244：内核透传必须发生（T_DETACH 到内核）。
        assert!(
            kern.traces.iter().any(|&(req, _, _)| req == T_DETACH),
            "detach must fall through to sys_trace (C trace.c:244)"
        );
        // stopped 已清（trace.c:210）。
        assert!(!table.procs[5].state.trace.stopped);
    }

    #[test]
    fn test_t_resume_feigns_success_when_sigtrace_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        table.procs[5].resources.signals.trace_mask = 1u64 << (5 - 1); // SIGTRAP
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_RESUME, pid: 42, addr: 0, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        // 假成功：不恢复、不透传（trace.c:231-236）。
        assert!(table.procs[5].state.trace.stopped);
        assert!(kern.traces.is_empty());
        // reply.data = 0（trace.c:233）。
        assert_eq!(unsafe { table.procs[0].ipc.reply.take().unwrap().m_u.m_pm_lc_ptrace.data }, 0);
    }

    #[test]
    fn test_t_resume_negative_data_is_einval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // C trace.c:220：data < 0 → EINVAL（i64 载荷保留了负值半边；
        // 旧 u64 建模使该分支不可达）。
        let req = PtraceReq { req: T_RESUME, pid: 42, addr: 0, data: -1 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval
        );
    }

    #[test]
    fn test_generic_guard_tracer_mismatch_is_esrch() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // 不存在的 pid → ESRCH（trace.c:140）。
        let req = PtraceReq { req: T_GETDATA, pid: 43, addr: 0, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Srch
        );
        // tracer 匹配但未停止 → EBUSY（trace.c:143）。
        table.procs[5].state.trace.stopped = false;
        let req = PtraceReq { req: T_GETDATA, pid: 42, addr: 0, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Busy
        );
    }

    // ---- trace_stop ----

    #[test]
    fn test_trace_stop_replies_waiting_tracer_with_payload() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(0),
            trace_options: TraceOptions::empty(),
        };
        table.procs[5].resources.signals.trace_mask = 1u64 << 10; // 信号 11 的位
        table.procs[0].state.wait.waiting = true;
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        trace_stop(&mut table, UserSlot::new(5), 11, &mut kern, &mut t);
        assert!(table.procs[5].state.trace.stopped);
        // D-26 wire 契约：status 在 m_pm_lc_wait4.status 载荷，m_type = pid。
        assert_eq!(t.sent().len(), 1, "tracer waiting → immediate reply");
        let (dest, msg) = &t.sent()[0];
        assert_eq!(*dest, table.procs[0].identity.endpoint);
        assert_eq!(msg.m_type, 42, "reply(tracer, child_pid) — C trace.c:274");
        assert_eq!(unsafe { msg.m_u.m_pm_lc_wait4.status }, w_stopcode(11), "C trace.c:273 payload");
        // sigtrace 消费（trace.c:270）。
        assert_eq!(table.procs[5].resources.signals.trace_mask & (1u64 << 10), 0);
    }

    #[test]
    fn test_trace_stop_not_waiting_only_stops() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(0),
            trace_options: TraceOptions::empty(),
        };
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        trace_stop(&mut table, UserSlot::new(5), 11, &mut kern, &mut t);
        assert!(table.procs[5].state.trace.stopped);
        assert!(t.sent().is_empty(), "tracer not waiting → no reply");
        assert!(kern.traces.iter().any(|&(req, _, _)| req == T_STOP));
    }

    #[test]
    fn test_trace_stop_kernel_failure_panics() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(0),
            trace_options: TraceOptions::empty(),
        };
        let mut kern = MockKernel::default();
        kern.trace_reply = Err(-5); // EIO
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            trace_stop(&mut table, UserSlot::new(5), 11, &mut kern, &mut t);
        }));
        assert!(res.is_err(), "sys_trace(T_STOP) failure must panic (C trace.c:264)");
    }
}
