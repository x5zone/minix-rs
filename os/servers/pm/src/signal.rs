//! Signal core: `do_kill`/`do_srv_kill` → `check_sig` → `sig_proc` → `sig_proc_exit` + `process_ksig`.
//!
//! C ground truth: `minix3/minix/servers/pm/signal.c:197-646` (do_kill/check_sig/sig_proc/process_ksig)
//! Design: explicit `SignalTarget` + `SignalState` (`mproc/signal.rs`) + `SignalClass`.
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, UserSlot, Pid, EINVAL, ESRCH, EPERM, EDEADEPT};
use crate::mproc::{ProcTable, _NSIG};

/// Signal numbers (subset, `sys/signal.h`).
pub const SIGKILL: i32 = 9;
pub const SIGTERM: i32 = 15;
pub const SIGCHLD: i32 = 20;
/// Lethal signal family（`SIGS_IS_LETHAL`，`sys/signal.h:279-281` 的成员，
/// 值见 `sys/signal.h:55-65`）。
pub const SIGILL: i32 = 4;
pub const SIGABRT: i32 = 6;
pub const SIGEMT: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGPIPE: i32 = 13;
pub const SIGBUS: i32 = 10;
pub const SIGSEGV: i32 = 11;

pub const SIGSTOP: i32 = 17;
/// Hangup（`sys/sys/signal.h:52`）——会话首领死亡时向其进程组广播
///（`check_sig(-procgrp, SIGHUP)`，forkexit.c:412）。
pub const SIGHUP: i32 = 1;
/// 内核信号：SIGSNDELAY（`sys/sys/signal.h:264`，值 70——早前
/// `sys_delay_stop` 的 EBUSY 延迟结束，可恢复当初搁置的信号处置）。
/// 旧代码误写 42 并自注 "Actually SIGSNDELAY is 41?"——从未命中过。
pub const SIGSNDELAY: i32 = 70;
/// 内核信号：SIGKSIG（`sys/sys/signal.h:274`，值 74）——内核为信号
/// 管理器积累了待处理信号（SIGS 范围内的位图经 `sys_getksig` 拉取）。
pub const SIGKSIG: i32 = 74;

/// Kill error, maps to `errno`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillError {
    InvalidSignal,
    NoSuchProcess,
    PermissionDenied,
    InvalidEndpoint,
}

impl KillError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::InvalidSignal => EINVAL,
            Self::NoSuchProcess => ESRCH,
            Self::PermissionDenied => EPERM,
            Self::InvalidEndpoint => EDEADEPT,
        }
    }
}

/// Handles `PM_KILL` (`do_kill`, `signal.c:197-202`).
///
/// `ksig = false` — user `kill(2)`, `PRIV_PROC` lethal protection applies.
pub fn do_kill(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    check_sig(table, caller, pid, signo, false, kern, transport)
}

/// Handles `PM_SRV_KILL` (`do_srv_kill`, `204-221`).
///
/// Only `RS` may call; `ksig = true` so `PRIV_PROC` can be killed.
pub fn do_srv_kill(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<usize, KillError> {
    if table.procs[caller.get()].endpoint() != Endpoint::RS {
        return Err(KillError::PermissionDenied);
    }
    check_sig(table, caller, pid, signo, true, kern, transport)
}

/// Checks which processes to signal (`check_sig`, `568-646`).
///
/// `pid` four meanings: `>0` one, `0` process group (caller procgrp), `-1` all (except `INIT_PID`), `<-1` group `-pid`.
/// `signo == 0` is existence probe (no `sig_proc`, just count).
/// Returns `Ok(count)` or `Err(errno)`, and `SUSPEND` is modeled as `Err` with `caller` now `EXITING` (self-kill).
pub fn check_sig<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    pid: Pid,
    signo: i32,
    ksig: bool,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<usize, KillError> {
    if signo < 0 || signo >= _NSIG as i32 {
        return Err(KillError::InvalidSignal);
    }
    if pid == 1 && signo == SIGKILL {
        return Err(KillError::InvalidSignal); // EINVAL for INIT+KILL
    }
    // Broadcast SIGTERM: RS first（signal.c:588-589）——经 sys_kill 内核
    // 回环产生 ksig，PM 不直接投递系统进程（sig_proc 的 PRIV_PROC !ksig
    // 分支是空操作，直接调它 RS 实际收不到通知，V2-P2-4）。C 不检查返回值。
    if pid == -1 && signo == SIGTERM {
        let _ = kern.sys_kill(Endpoint::RS, SIGTERM);
    }

    let mut count = 0;
    let mut error_code = KillError::NoSuchProcess;
    // Iterate reverse (NR_PROCS-1..0) to hit system procs at end first (597)
    for idx in (0..minix_types::NR_PROCS).rev() {
        let proc = &table.procs[idx];
        if !proc.is_in_use() {
            continue;
        }
        // Selection (601-604)
        if pid > 0 && pid != proc.identity.id.pid {
            continue;
        }
        if pid == 0 && table.procs[caller.get()].identity.procgrp != proc.identity.procgrp {
            continue;
        }
        if pid == -1 && proc.identity.id.pid <= 1 {
            continue;
        }
        if pid < -1 && proc.identity.procgrp != -pid {
            continue;
        }
        // Broadcast SIGKILL skip PRIV_PROC (607-608)
        if pid == -1 && signo == SIGKILL && proc.is_kernel_process() {
            continue;
        }
        // VM skip (613)
        if proc.endpoint() == Endpoint::VM {
            continue;
        }
        // Lethal protection (616-618)
        if !ksig && is_lethal(signo) && proc.is_kernel_process() {
            error_code = KillError::PermissionDenied;
            continue;
        }
        // Permission (622-628): SUPER_USER or real/eff match
        if !can_signal(table, caller, UserSlot::new(idx)) {
            error_code = KillError::PermissionDenied;
            continue;
        }
        count += 1;
        if signo == 0 || proc.state.lifecycle.is_exiting() {
            continue;
        }
        let _ = sig_proc(table, UserSlot::new(idx), signo, true, ksig, kern, transport);
        if pid > 0 {
            break;
        }
    }
    // Self-kill SUSPEND (644)
    if table.procs[caller.get()].state.lifecycle.is_exiting() {
        // Caller killed itself → SUSPEND (beyond grave)
        // In Rust we model as Ok(count) but caller lifecycle is Exiting; dispatcher will map to ReplyLater
        // For check_sig we return count, caller will handle SUSPEND via lifecycle check
        // Here we just return count; the SUSPEND is observed via caller's Exiting flag
    }
    if count > 0 {
        Ok(count)
    } else {
        Err(error_code)
    }
}

/// Whether signal is lethal (`SIGS_IS_LETHAL`, `sys/signal.h:279-281`):
/// SIGILL | SIGBUS | SIGFPE | SIGSEGV | SIGEMT | SIGABRT.
fn is_lethal(signo: i32) -> bool {
    matches!(signo, SIGILL | SIGBUS | SIGFPE | SIGSEGV | SIGEMT | SIGABRT)
}

/// Whether signal needs a stacktrace (`SIGS_IS_STACKTRACE`,
/// `sys/signal.h:286`): lethal 且非 SIGABRT。C 对 PRIV_PROC 的系统信号先
/// `sys_diagctl_stacktrace`（`signal.c:455-457`）。
fn is_stacktrace(signo: i32) -> bool {
    is_lethal(signo) && signo != SIGABRT
}

/// Whether signal is termination (`SIGS_IS_TERMINATION`,
/// `sys/signal.h:282-284`): lethal 或 SIGKILL/SIGPIPE。终止类走
/// `sig_proc_exit`；非终止类转 `SIGS_SIGNAL_RECEIVED` 消息
///（`signal.c:459-470`）。
fn is_termination(signo: i32) -> bool {
    is_lethal(signo) || signo == SIGKILL || signo == SIGPIPE
}

/// Checks permission (`signal.c:622-628`).
fn can_signal(table: &ProcTable, caller: UserSlot, target: UserSlot) -> bool {
    let caller_creds = table.procs[caller.get()].resources.privilege.credentials();
    let target_creds = table.procs[target.get()].resources.privilege.credentials();
    if let (Some(c), Some(t)) = (caller_creds, target_creds) {
        if c.user.effective == 0 {
            return true; // SUPER_USER
        }
        c.user.real == t.user.real
            || c.user.effective == t.user.real
            || c.user.real == t.user.effective
            || c.user.effective == t.user.effective
    } else {
        // Kernel processes (no creds) — treat as superuser for signal?
        // In C, `mp_effuid` for PRIV_PROC is still 0, so true
        // For Rust, Kernel privilege is considered superuser
        table.procs[caller.get()].is_kernel_process() || table.procs[target.get()].is_kernel_process()
    }
}

/// 把 [`sig_proc`]（trace=FALSE 的重投形态）适配为 [`crate::signal_flow::
/// SignalDeliver`] 注入口——check_pending/handle_sigsn_delay 的投递尾巴
///（C 直接调模块级函数，Rust 以桥接保持注入可测性）。
struct SigProcDeliver<'a, T: crate::ipc::IpcTransport + ?Sized> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    transport: &'a mut T,
}

impl<T: crate::ipc::IpcTransport + ?Sized> crate::signal_flow::SignalDeliver for SigProcDeliver<'_, T> {
    fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, signo: i32, ksig: bool) {
        let _ = sig_proc(table, target, signo, false, ksig, self.kern, self.transport);
    }
}

/// 把 [`crate::exit::KernelGateway`] 的 `sys_delay_stop` 适配为
/// [`crate::signal_flow::KernelStop`] seam（13-design D1 的窄接口）。
/// 用独立桥接而非 supertrait/dyn 上转：signal_flow 不感知中央网关类型，
/// 也不依赖 trait upcasting 的工具链版本。
struct GatewayStopBridge<'a>(&'a mut dyn crate::exit::KernelGateway);

impl crate::signal_flow::KernelStop for GatewayStopBridge<'_> {
    fn delay_stop(&mut self, ep: Endpoint) -> i32 {
        match self.0.sys_delay_stop(ep) {
            Ok(()) => 0, // OK
            Err(e) => e,
        }
    }
}

/// Sends signal to process (`sig_proc`, `384-540`).
///
/// 9-step chain: TRACE→VFS|EVENT→PRIV_PROC→badignore→ignore→block→TRACE_STOPPED→caught→terminate.
/// Returns `Ok(())` or `Err` (for sig_send failure).
pub fn sig_proc<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    trace: bool,
    ksig: bool,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<(), KillError> {
    let proc = &table.procs[target.get()];
    if !proc.is_in_use() || proc.state.lifecycle.is_exiting() {
        // panic in C (407-409) — but for Rust we return Err
        return Err(KillError::NoSuchProcess);
    }
    // TRACE 先行（411-422）：调试器信号先入 sigtrace 缓冲，未停止则
    // trace_stop 停住——内核 sys_trace(T_STOP) 真停 + tracer 在 wait 时
    // 收到 W_STOPCODE 回复。V3-P1-1 之前此分支直接置 stopped 标志，
    // 内核与 tracer 均无感知。
    if trace && proc.state.guardianship.tracer().is_some() && signo != SIGKILL {
        table.procs[target.get()].resources.signals.trace_mask |= crate::init::sig_bit(signo);
        if !table.procs[target.get()].state.trace.stopped {
            crate::trace::trace_stop(table, target, signo, kern, transport);
        }
        return Ok(());
    }
    // VFS|EVENT pending（425-444）：置 pending 位后停住进程——防止它在
    // VFS/事件回复到达之后、PM 复查信号之前再次发起调用；PROC_STOPPED
    // 兼作 restart_sigs 复查的指示位（C signal.c:430-443 注释语义）。
    // V3-P1-3 之前该分支只置位不停止，check_pending 的 VFS|EVENT 不变式
    //（必已 stopped，signal_flow.rs:254）随之断裂。
    // 分支条件按 C signal.c:425 用 VFS_CALL|EVENT_CALL——旧代码的
    // `ipc_blocked.is_some()` 会把 C 不含的 DELAY_CALL 也拦进来。
    if table.procs[target.get()].state.block.is_vfs_blocked()
        || table.procs[target.get()].state.block.is_event_blocked()
    {
        table.procs[target.get()].resources.signals.pending |= 1u64 << (signo - 1);
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= 1u64 << (signo - 1);
        }
        // C: `if (!(PROC_STOPPED | DELAY_CALL)) stop_proc(rmp, FALSE)`——
        // FALSE = 不可延迟，内核回 EBUSY 时 stop_proc 内部 panic（C 同型）。
        // C 守卫的 DELAY_CALL 一半在 Rust 不可表示：IpcBlockReason 三变体
        // 互斥（block.rs:50-84，ARCH A-2），VFS_CALL/EVENT_CALL 与
        // DELAY_CALL 不能共存，故此处守卫只剩 STOPPED 一半。
        if !table.procs[target.get()].state.block.stopped {
            let mut bridge = GatewayStopBridge(kern);
            let _ = crate::signal_flow::stop_proc(
                table,
                target,
                crate::signal_flow::MayDelay::MustStop,
                &mut bridge,
            );
        }
        return Ok(());
    }
    // PRIV_PROC system signals (448-480)
    if table.procs[target.get()].is_kernel_process() {
        if table.procs[target.get()].endpoint() == Endpoint::PM {
            return Ok(());
        }
        if !ksig {
            // Forward to kernel signal manager
            let _ = (target, signo);
            return Ok(());
        }
        if is_stacktrace(signo) {
            let _ = target;
        }
        if !is_termination(signo) {
            // Message SIGS_SIGNAL_RECEIVED to system process
            let _ = (target, signo);
            return Ok(());
        } else {
            return sig_proc_exit(table, target, signo, transport, kern);
        }
    }
    // User process: badignore / ignore / block / TRACE_STOPPED / caught / terminate
    // 位基统一经 `init::sig_bit`（对齐 C `__sigmask`），本函数不再出现裸移位。
    let sigmask = crate::init::sig_bit(signo);
    let state = &table.procs[target.get()].resources.signals;
    // badignore（C signal.c:483-486）：仅当内核信号本身属于 noign 集合、
    // 且该信号又被 ignore 或 mask 时，才强制默认处理（不可通过设置吞掉）。
    let badignore = ksig
        && (crate::init::NOIGN_SIGSET & sigmask != 0)
        && (state.ignored & sigmask != 0 || state.mask & sigmask != 0);
    if !badignore && (state.ignored & sigmask != 0) {
        return Ok(());
    }
    if !badignore && (state.mask & sigmask != 0) {
        table.procs[target.get()].resources.signals.pending |= sigmask;
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= sigmask;
        }
        return Ok(());
    }
    if table.procs[target.get()].state.trace.stopped && signo != SIGKILL {
        table.procs[target.get()].resources.signals.pending |= sigmask;
        if ksig {
            table.procs[target.get()].resources.signals.kernel_pending |= sigmask;
        }
        return Ok(());
    }
    if !badignore && (state.caught & sigmask != 0) {
        // Try unpause then sig_send
        if !unpause(table, target) {
            table.procs[target.get()].resources.signals.pending |= sigmask;
            if ksig {
                table.procs[target.get()].resources.signals.kernel_pending |= sigmask;
            }
            return Ok(());
        }
        if sig_send(table, target, signo).is_ok() {
            return Ok(());
        }
        // Fall through to terminate on sig_send failure（C 同：printf 后
        // 落入终止，signal.c:531-534）
    } else if !badignore && (crate::init::IGN_SIGSET & sigmask != 0) {
        // Signal defaults to being ignored（C signal.c:535-539，注意与
        // caught 分支互斥：捕获失败的默认忽略信号必须终止而非忽略）
        return Ok(());
    }
    sig_proc_exit(table, target, signo, transport, kern)
}

/// Terminates process via signal (`sig_proc_exit`, `546-563`).
fn sig_proc_exit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    target: UserSlot,
    signo: i32,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) -> Result<(), KillError> {
    let is_core = crate::init::CORE_SIGSET & crate::init::sig_bit(signo) != 0;
    // C: exit_proc(rmp, 0, dump_core=is_core)——exit_proc 尾部无条件
    // tell_vfs（DUMPCORE 或 EXIT，forkexit.c:350-358），所以 transport
    // 必须是调用者的真实通道。V2-P0-1：曾在此构造一次性 mock，信号
    // 终止的 VFS 告知全部丢失（core 路径进程永久卡 EXITING）。
    // 内核出口（step 9 sys_clear）用生产网关：本路径目标为用户进程
    //（PRIV_PROC 在前置分支已返回），step 9 不会触发；若未来语义变化
    // 命中，pre-E1 诚实 panic（C 失败语义同型）。
    let status = 0;
    crate::exit::exit_proc(table, target, status as i8, is_core, transport, kern);
    Ok(())
}

/// Unpauses a process blocked on WAIT or SIGSUSPEND or VFS (`unpause`, `719-770`).
///
/// Returns `true` if already unpaused or successfully unpaused, `false` if delayed.
fn unpause(table: &mut ProcTable, target: UserSlot) -> bool {
    // Simplified for 11: if VFS_CALL|EVENT_CALL → tell_vfs UNPAUSE, else if WAITING|SIGSUSPENDED → stop_proc
    let proc = &table.procs[target.get()];
    if proc.state.block.ipc_blocked.is_some() {
        // In C: tell_vfs UNPAUSE or stop_proc with may_delay
        return false;
    }
    if proc.state.wait.waiting || proc.resources.signals.suspended {
        // In C: stop_proc(FALSE) for WAITING|SIGSUSPENDED
        table.procs[target.get()].state.block.stopped = true;
        return true;
    }
    // Not paused in PM, try VFS unpause
    true
}

/// Sends signal via handler (`sig_send`, `772-855`).
///
/// Returns `true` if handler setup succeeded.
fn sig_send(table: &mut ProcTable, target: UserSlot, signo: i32) -> Result<(), KillError> {
    // Simplified: set pending cleared, mask updated, and mark as needing sigframe
    let _ = (table, target, signo);
    Ok(())
}

/// Processes kernel signal (`process_ksig`, `294-378`).
pub fn process_ksig(
    table: &mut ProcTable,
    endpoint: Endpoint,
    signo: i32,
    vctl: &mut dyn crate::timer::VTimerCtl,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) -> Result<(), KillError> {
    let slot = table
        .pm_isokendpt(endpoint)
        .map(|s| s.get())
        .map_err(|_| KillError::InvalidEndpoint)?;
    let proc = &table.procs[slot];
    if !proc.is_in_use() || proc.state.lifecycle.is_exiting() {
        return Err(KillError::InvalidEndpoint);
    }
    // Pretend PM is sender (312)
    let _ = proc.identity.procgrp;
    let pid = proc.identity.id.pid;
    // SIGVTALRM/SIGPROF → 重置虚拟计时器（C: signal.c:326-328 的
    // check_vtimer + fall-through 到单播 default 分支）。注意 SIGVTALRM
    // = 26：旧代码误写 12（SIGSYS），该分支从未命中过——真实 bug，随
    // 本条 D-23 修复。VTimerCtl 的生产实现 = 内核 sys_vtimer（E6）。
    if signo == crate::timer::SIGVTALRM || signo == crate::timer::SIGPROF {
        crate::timer::check_vtimer(table, slot, signo, vctl);
    }
    // Broadcast vs single (320-332)
    let target_pid = match signo {
        2 | 3 | 28 | 29 => 0, // INT, QUIT, WINCH, INFO → group broadcast
        _ => pid,
    };
    // check_sig 的返回值 C 不检查（signal.c:337 语句调用）——对
    // SIGSNDELAY(70) 这类超出 _NSIG 的内核信号，check_sig 的越界门
    // 返回 EINVAL 属预期；旧代码用 `?` 传播导致 SIGSNDELAY 尾部死路。
    let _ = check_sig(table, UserSlot::new(0), target_pid, signo, true, kern, transport);
    // SIGSNDELAY（C signal.c:344-369）：更早的 stop_proc 因进程在途
    // 发送而 EBUSY，内核在发送完成后投递 SIGSNDELAY——恢复当初搁置
    // 的处置（清 DELAY_CALL → VFS|EVENT 在途则 stop_proc，否则
    // check_pending）。13 的 handle_sigsn_delay 是该段的全语义移植。
    if signo == SIGSNDELAY {
        // 与 13 的 handle_sigsn_delay 同语义（signal_flow.rs:76-110）；
        // 在此顺序内联是因为 stop 桥与投递桥不同时借用 kern。
        let delayed = matches!(
            table.procs[slot].state.block.ipc_blocked,
            Some(crate::mproc::IpcBlockReason::DelayedSignal)
        );
        if delayed {
            // 清 DELAY_CALL（351）+ assert 未停止（353）。
            table.procs[slot].state.block.ipc_blocked = None;
            assert!(
                !table.procs[slot].state.block.stopped,
                "SIGSNDELAY: DELAY without PROC_STOPPED"
            );
            // VFS|EVENT 在途 → stop_proc(MustStop)，等 VFS 回复后再查（359-363）。
            if table.procs[slot].state.block.is_vfs_blocked()
                || table.procs[slot].state.block.is_event_blocked()
            {
                let mut stop_bridge = GatewayStopBridge(&mut *kern);
                let _ = crate::signal_flow::stop_proc(
                    table,
                    UserSlot::new(slot),
                    crate::signal_flow::MayDelay::MustStop,
                    &mut stop_bridge,
                );
                return Ok(());
            }
            // 尽可能多处置常规信号（366）。
            let mut deliver = SigProcDeliver { kern: &mut *kern, transport: &mut *transport };
            let _ = crate::signal_flow::check_pending(table, UserSlot::new(slot), &mut deliver);
        }
    }
    if table.procs[slot].state.lifecycle.is_exiting() {
        return Err(KillError::InvalidEndpoint);
    }
    Ok(())
}

/// C `process_sigmgr_signals`（`sef_signal.c:27-63`）：SIGKSIG 通知到达
/// 后的拉取循环——内核为信号管理器积累了待处理内核信号时，逐个取回
/// （`sys_getksig`，取回即消费 RTS_SIGNALED）、逐信号确认（`sys_endksig`）
/// 并驱动 `process_ksig` 处置，直到内核报告无更多。取回失败 C panic
///（"SEF: sys_getksig failed"）；单个信号的目标已消亡（EDEADEPT）则
/// 继续循环（C 同）。
pub fn process_sigmgr_signals(
    table: &mut ProcTable,
    vctl: &mut dyn crate::timer::VTimerCtl,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut dyn crate::ipc::IpcTransport,
) {
    loop {
        let found = kern
            .get_ksig()
            .unwrap_or_else(|r| panic!("SEF: sys_getksig failed: {}", r));
        let (target, set) = match found {
            Some(pair) => pair,
            None => break,
        };
        // SIGS_LAST = SIGSNDELAY(70) 超出 u64 位图（kernel syscall_signal.rs:
        // 88-94 的 SigSet(u64) 已知限制）——位图内可表达的只有 1..=64。
        for signo in 1..=64 {
            if set >> (signo - 1) & 1 != 0 {
                if let Err(r) = kern.end_ksig(target, signo) {
                    panic!("sys_endksig failed: {}", r);
                }
                let _ = process_ksig(table, target, signo, vctl, kern, transport);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    /// signal.rs 测试用内核网关 mock（sys_kill/sys_clear/sys_abort 恒 OK；
    /// proc_times 可脚本化计账值供 D-14 累加断言）。
    struct TestKernel {
        pub user: minix_types::Clock,
        pub sys: minix_types::Clock,
    }
    impl crate::exit::KernelGateway for TestKernel {
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, u64)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            Ok((self.user, self.sys))
        }
    }
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Clock, Endpoint, UserSlot};

    struct NopVTimer;
    impl crate::timer::VTimerCtl for NopVTimer {
        fn vtimer(&mut self, _ep: Endpoint, _which: crate::timer::ItimerWhich, _set: Option<Clock>, _get: Option<&mut Clock>) -> i32 { 0 }
    }

    fn mk_proc(table: &mut ProcTable, slot: usize, pid: i32, procgrp: i32, is_kernel: bool) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.procgrp = procgrp;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        if is_kernel {
            table.procs[slot].resources.privilege = Privilege::Kernel;
        } else {
            table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        }
    }

    #[test]
    fn test_kill_single() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, false, &mut kern_rec, &mut t);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);
    }

    // ---- 信号集合与 badignore 行为（todo.md V2-P0-2 回归锚点）----

    /// C signal.h:71/80（sys/sys/signal.h）。
    const SIGCONT_TEST: i32 = 19;
    const SIGWINCH_TEST: i32 = 28;

    /// C signal.c:535-539：SIGCONT ∈ ign_sset，默认忽略——干净进程收到
    /// SIGCONT 必须存活。位序错位（V2-P0-2）曾使本路径落入终止分支。
    #[test]
    fn test_sigproc_default_ignores_sigcont() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernel { user: 0, sys: 0 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, SIGCONT_TEST, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 1);
        assert!(table.procs[5].is_in_use() && !table.procs[5].is_exiting());
    }

    /// C signal.c:483-486：ksig + 信号 ∈ noign_sset + 被进程 ignore →
    /// badignore 强制默认处理（SIGSEGV ∈ core_sigs → dump 终止路径），
    /// ignore 设置被穿透。
    #[test]
    fn test_badignore_forces_default_on_ignored_lethal_ksig() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = crate::init::sig_bit(crate::signal::SIGSEGV);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernel { user: 0, sys: 0 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, true, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(table.procs[5].is_exiting());
    }

    /// ksig 但信号 ∉ noign_sset（SIGWINCH）且被 ignore → 正常忽略：
    /// badignore 只穿透 noign 集合内的信号（C signal.c:483-491）。
    #[test]
    fn test_ignored_non_noign_ksig_still_ignored() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = crate::init::sig_bit(SIGWINCH_TEST);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = TestKernel { user: 0, sys: 0 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, SIGWINCH_TEST, true, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(table.procs[5].is_in_use() && !table.procs[5].is_exiting());
    }

    /// V2-P0-1：信号终止必须经调用者的真实通道告知 VFS（forkexit.c
    /// :350-358 无条件 tell_vfs）——SIGKILL → VFS_PM_EXIT，SIGSEGV
    /// （∈ core_sigs）→ VFS_PM_DUMPCORE。修复前这些消息进一次性 mock
    /// 黑洞，core 路径进程永久卡 EXITING。
    #[test]
    fn test_signal_termination_tells_vfs() {
        let mut kern = TestKernel { user: 0, sys: 0 };

        // SIGKILL：普通终止 → VFS_PM_EXIT
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        check_sig(&mut table, UserSlot::new(0), 42, SIGKILL, false, &mut kern, &mut t).unwrap();
        assert!(
            t.sent().iter().any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
            "SIGKILL termination must tell VFS_PM_EXIT, sent={:?}",
            t.sent().iter().map(|(_, m)| m.m_type).collect::<Vec<_>>()
        );

        // SIGSEGV：core 信号 → VFS_PM_DUMPCORE
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, false, &mut kern, &mut t).unwrap();
        assert!(
            t.sent().iter().any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_DUMPCORE),
            "core signal must send VFS_PM_DUMPCORE, sent={:?}",
            t.sent().iter().map(|(_, m)| m.m_type).collect::<Vec<_>>()
        );
    }

    /// V2-P2-4：广播 SIGTERM 先经 sys_kill(RS) 内核回环通知 RS（C
    /// signal.c:588-588），而非直接 sig_proc（PRIV_PROC !ksig 空转）。
    #[test]
    fn test_broadcast_sigterm_notifies_rs_via_kernel() {
        struct RecordKill {
            killed: Option<(Endpoint, i32)>,
        }
        impl crate::exit::KernelGateway for RecordKill {
            fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, u64)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
            fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
            fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
            fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
                self.killed = Some((ep, sig));
                Ok(())
            }
            fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
            fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
            fn copy_to_user(&mut self, _b: &[u8], _e: Endpoint, _a: u64) -> Result<(), i32> { Ok(()) }
            fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
            fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
                Ok((0, 0))
            }
        }

        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = RecordKill { killed: None };
        check_sig(&mut table, UserSlot::new(0), -1, SIGTERM, false, &mut kern, &mut t).unwrap();
        assert_eq!(kern.killed, Some((Endpoint::RS, SIGTERM)));
    }

    /// 记录型 VTimerCtl mock：捕获 (which, set 值)。
    struct RecVTimer { pub which: Option<crate::timer::ItimerWhich>, pub set: Option<Clock> }
    impl crate::timer::VTimerCtl for RecVTimer {
        fn vtimer(&mut self, _ep: Endpoint, which: crate::timer::ItimerWhich, set: Option<Clock>, _get: Option<&mut Clock>) -> i32 {
            self.which = Some(which);
            self.set = set;
            0
        }
    }

    #[test]
    fn test_process_ksig_sigvtalrm_restarts_vtimer() {
        // C: signal.c:326-328 + alarm.c:222-241 — ksig SIGVTALRM →
        // check_vtimer：interval>0 时向内核重设 ITIMER_VIRTUAL。
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 100, true); // 内核进程（ksig 路径）
        table.procs[5].resources.intervals[crate::timer::ItimerWhich::Virtual as usize] = 50;
        let mut vt = RecVTimer { which: None, set: None };
        let mut t = crate::ipc::TestIpcTransport::default();

        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(1, 5), 26, &mut vt, &mut kern_rec, &mut t);
        assert!(res.is_ok());
        assert_eq!(vt.which, Some(crate::timer::ItimerWhich::Virtual));
        assert_eq!(vt.set, Some(50));
    }

    #[test]
    fn test_process_ksig_sigsys_does_not_touch_vtimer() {
        // 回归守卫：旧代码误写 `signo == 12`（SIGSYS 非 SIGVTALRM=26）——
        // SIGSYS 不应触碰虚拟计时器。
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 100, true);
        let mut vt = RecVTimer { which: None, set: None };
        let mut t = crate::ipc::TestIpcTransport::default();

        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(1, 5), 12, &mut vt, &mut kern_rec, &mut t);
        assert!(res.is_ok());
        assert!(vt.which.is_none(), "SIGSYS must not restart a vtimer");
        assert!(vt.set.is_none());
    }

    #[test]
    fn test_stacktrace_and_termination_match_c_macros() {
        // C: sys/signal.h:279-286 — SIGS_IS_STACKTRACE = LETHAL && !=ABRT；
        // SIGS_IS_TERMINATION = LETHAL || KILL || PIPE。
        assert!(is_stacktrace(SIGILL));
        assert!(is_stacktrace(SIGBUS));
        assert!(is_stacktrace(SIGFPE));
        assert!(is_stacktrace(SIGSEGV));
        assert!(is_stacktrace(SIGEMT));
        assert!(!is_stacktrace(SIGABRT), "ABRT lethal but no stacktrace");
        assert!(!is_stacktrace(9), "SIGKILL not lethal");
        assert!(!is_stacktrace(13), "SIGPIPE not lethal");

        assert!(is_termination(SIGABRT));
        assert!(is_termination(SIGSEGV));
        assert!(is_termination(9));  // SIGKILL
        assert!(is_termination(13)); // SIGPIPE
        assert!(!is_termination(15), "SIGTERM not in C's termination set");
        assert!(!is_termination(2), "SIGINT not termination");
        assert!(!is_termination(SIGSTOP), "SIGSTOP not termination");
    }

    #[test]
    fn test_kill_eperm_for_lethal_priv() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, true); // PRIV_PROC
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut t = crate::ipc::TestIpcTransport::default();
        // D-22 语义修正：SIGS_IS_LETHAL（sys/signal.h:279-281）= ILL|BUS|FPE|
        // SEGV|EMT|ABRT，不含 SIGKILL——旧近似列表把 9 计入 lethal 是与 C 的
        // 真实偏差（SIGKILL 对 PRIV_PROC 经 kill(2) 在 C 中合法）。改用真
        // lethal 的 SIGSEGV 验证 EPERM 保护。
        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        let res = check_sig(&mut table, UserSlot::new(0), 42, crate::signal::SIGSEGV, false, &mut kern_rec, &mut t); // SIGSEGV lethal, !ksig, PRIV_PROC → EPERM
        assert_eq!(res.unwrap_err(), KillError::PermissionDenied);
    }

    #[test]
    fn test_kill_broadcast() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 100, false);
        mk_proc(&mut table, 6, 43, 100, false);
        mk_proc(&mut table, 7, 44, 200, false);
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0)); // root
        table.procs[0].identity.procgrp = 100;
        let mut kern = TestKernel { user: 0, sys: 0 };
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = check_sig(&mut table, UserSlot::new(0), 0, 15, false, &mut kern, &mut t); // pid 0 → procgrp 100
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 3); // two in group 100 + caller itself (kill(0) includes caller)
    }

    #[test]
    fn test_process_ksig_edeadept() {
        let mut table = ProcTable::new();
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut novt = NopVTimer;
        let mut kern_rec = TestKernel { user: 0, sys: 0 };
        let res = process_ksig(&mut table, Endpoint::from_generation_slot(9, 9), 15, &mut novt, &mut kern_rec, &mut t);
        let mut kern_rec = TestKernel { user: 30, sys: 12 };
        assert_eq!(res.unwrap_err(), KillError::InvalidEndpoint);
    }

    // ---- 内核信号拉取循环（todo.md V3-P1-2 回归锚点，批次 H）----

    /// 脚本化 get_ksig 的内核网关 mock：`ksig_script` 逐次出队，空则
    /// None（= 内核侧无更多）；记录 end_ksig 确认序列。
    struct KsigRecorder {
        ksig_script: Vec<Option<(minix_types::Endpoint, u64)>>,
        endksig_calls: Vec<(minix_types::Endpoint, i32)>,
    }
    impl crate::exit::KernelGateway for KsigRecorder {
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
        fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, u64)>, i32> {
            Ok(if self.ksig_script.is_empty() { None } else { self.ksig_script.remove(0) })
        }
        fn end_ksig(&mut self, ep: minix_types::Endpoint, sig: i32) -> Result<(), i32> {
            self.endksig_calls.push((ep, sig));
            Ok(())
        }
    }

    /// C sef_signal.c:27-63：SIGKSIG 到达后的拉取循环——getksig 取回
    /// （endpt, set），逐信号 endksig 确认 + process_ksig 处置，endpt
    /// NONE 终止。SIGTERM(15) 经 process_ksig → check_sig(ksig=TRUE) →
    /// 未捕获未忽略 → 终止链。
    #[test]
    fn test_process_sigmgr_signals_drains_and_delivers() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        let target_ep = table.procs[5].identity.endpoint;
        let mut kern = KsigRecorder {
            ksig_script: vec![
                Some((target_ep, 1u64 << (SIGTERM - 1))),
                None,
            ],
            endksig_calls: Vec::new(),
        };
        let mut novt = NopVTimer;
        let mut t = crate::ipc::TestIpcTransport::default();
        process_sigmgr_signals(&mut table, &mut novt, &mut kern, &mut t);
        // 逐信号确认消费（C：sys_endksig 先于 process_ksig）。
        assert_eq!(kern.endksig_calls, vec![(target_ep, SIGTERM)]);
        // SIGTERM 默认处置终止：进程走完整 exit 链到僵尸。
        assert!(
            matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. }),
            "SIGTERM ksig must terminate the clean process, got {:?}",
            table.procs[5].state.lifecycle
        );
    }

    /// C signal.c:344-369：SIGSNDELAY 到达时清 DELAY_CALL 并恢复搁置的
    /// 处置——常规信号重查（此处验证 DELAY_CALL 清除与 pending 投递）。
    #[test]
    fn test_process_ksig_sigsn_delay_resumes_check_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked = Some(crate::mproc::IpcBlockReason::DelayedSignal);
        // 搁置期间积累的未阻塞信号。
        table.procs[5].resources.signals.pending = 1u64 << (SIGTERM - 1);
        let mut kern = KsigRecorder { ksig_script: Vec::new(), endksig_calls: Vec::new() };
        let mut novt = NopVTimer;
        let mut t = crate::ipc::TestIpcTransport::default();
        let target_ep = table.procs[5].identity.endpoint;
        let res = process_ksig(&mut table, target_ep, SIGSNDELAY, &mut novt, &mut kern, &mut t);
        assert!(res.is_ok());
        // DELAY_CALL 必须清（C signal.c:351）；exit_proc 的 tell_vfs 随后
        // 以 VFS_CALL 占位（exit.rs step 8），不能断言整个 ipc_blocked 为空。
        assert!(
            !matches!(
                table.procs[5].state.block.ipc_blocked,
                Some(crate::mproc::IpcBlockReason::DelayedSignal)
            ),
            "DELAY_CALL must clear, got {:?}",
            table.procs[5].state.block.ipc_blocked
        );
        // SIGTERM 已处置（终止链）。
        assert!(table.procs[5].state.lifecycle.is_exiting() || matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. }));
    }

    #[test]
    fn test_sig_proc_ignored() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].resources.signals.ignored = 1u64 << (SIGCHLD - 1);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kr = TestKernel { user: 0, sys: 0 };
        let res = sig_proc(&mut table, UserSlot::new(5), SIGCHLD, false, false, &mut kr, &mut t);
        assert!(res.is_ok());
        // ignored → no pending
        assert_eq!(table.procs[5].resources.signals.pending & (1u64 << (SIGCHLD - 1)), 0);
    }

    // ---- VFS_CALL 分支的 stop_proc 接线（todo.md V3-P1-3 回归锚点）----

    /// 记录 `sys_delay_stop` 调用并支持脚本化返回值的内核网关 mock：
    /// `reply == 0` 时成功，否则原样返回该负 errno（如 EBUSY = 16）。
    struct StopRecorder {
        calls: Vec<Endpoint>,
        reply: i32,
    }
    impl Default for StopRecorder {
        fn default() -> Self {
            Self { calls: Vec::new(), reply: 0 }
        }
    }
    impl crate::exit::KernelGateway for StopRecorder {
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, u64)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, ep: minix_types::Endpoint) -> Result<(), i32> {
            self.calls.push(ep);
            if self.reply == 0 { Ok(()) } else { Err(self.reply) }
        }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
    }

    /// C signal.c:425-443：VFS_CALL 挂起的进程收到信号 → pending 置位 +
    /// `stop_proc(rmp, FALSE)` 停住（PROC_STOPPED 兼作 restart_sigs 复查
    /// 指示）。V3-P1-3 之前只置位不停。
    #[test]
    fn test_sig_proc_vfs_call_stops_and_sets_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        let bit = 1u64 << (SIGTERM - 1);
        assert_eq!(table.procs[5].resources.signals.pending & bit, bit);
        assert!(table.procs[5].state.block.stopped, "stop_proc must set PROC_STOPPED");
        assert_eq!(kern.calls, vec![table.procs[5].identity.endpoint]);
    }

    /// ksig=TRUE 时同步置 `mp_ksigpending`（C signal.c:428-429）。
    #[test]
    fn test_sig_proc_vfs_call_ksig_sets_kernel_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, true, &mut kern, &mut t);
        assert!(res.is_ok());
        let bit = 1u64 << (SIGTERM - 1);
        assert_eq!(table.procs[5].resources.signals.kernel_pending & bit, bit);
        assert!(table.procs[5].state.block.stopped);
    }

    /// C 守卫 `!(PROC_STOPPED | DELAY_CALL)`：已停进程不再重复调内核
    ///（stop_proc 的 assert 也不允许），只置 pending。
    #[test]
    fn test_sig_proc_vfs_call_skips_stop_when_already_stopped() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        table.procs[5].state.block.stopped = true;
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(kern.calls.is_empty(), "must not re-stop an already-stopped process");
        let bit = 1u64 << (SIGTERM - 1);
        assert_eq!(table.procs[5].resources.signals.pending & bit, bit);
    }

    /// DELAY_CALL（无 VFS_CALL/EVENT_CALL）不进入本分支——C signal.c:425
    /// 只测两个 flag。SIGCONT 默认忽略，进程存活且零内核调用。
    #[test]
    fn test_sig_proc_delay_call_bypasses_vfs_branch() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked = Some(crate::mproc::IpcBlockReason::DelayedSignal);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder::default();
        let res = sig_proc(&mut table, UserSlot::new(5), SIGCONT_TEST, false, false, &mut kern, &mut t);
        assert!(res.is_ok());
        assert!(kern.calls.is_empty());
        assert_eq!(table.procs[5].resources.signals.pending, 0);
        assert_eq!(table.procs[5].state.lifecycle, Lifecycle::Running);
    }

    /// `stop_proc(rmp, FALSE)` = 不可延迟：内核回 EBUSY 必须 panic
    ///（C signal.c:248 "stop_proc: unexpected delay call"）。
    #[test]
    fn test_sig_proc_vfs_call_ebusy_must_stop_panics() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 42, 42, false);
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = StopRecorder { calls: Vec::new(), reply: 16 }; // EBUSY
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = sig_proc(&mut table, UserSlot::new(5), SIGTERM, false, false, &mut kern, &mut t);
        }));
        assert!(res.is_err(), "MustStop + EBUSY must panic");
    }
}
