//! Exit path: `do_exit → exit_proc → exit_restart` + zombie/reap chain.
//!
//! C ground truth: `minix3/minix/servers/pm/forkexit.c:242-469` (do_exit/exit_proc/exit_restart)
//! + `590-807` (zombify/check_parent/tracer_died/cleanup) + `mproc.h:86-104`
//!   flags + `main.c:365` publish_event.
//!
//! Design: explicit orchestrator + `Lifecycle` enum (`mproc/lifecycle.rs:27`)
//! + `Guardianship` (`mproc/guardianship.rs`) + `BlockState`.
//!   Single-threaded event loop — `&mut ProcTable` without `Arc`/`Mutex`.

use minix_types::{Endpoint, Message, UserSlot, VfsCall, VirBytes};
use crate::ipc::ReplyIntent;
use crate::mproc::{ProcTable, Lifecycle};
use alloc::vec::Vec;

/// Exit status truncation: Minix3 `mp_exitstatus` is `char` (`mproc.h:25`).
fn trunc_status(status: i32) -> i8 {
    status as i8
}

/// PM 的内核调用出口（`do_exit` 的 PRIV_PROC 违规分支，2026-09-06 D-13 落地）。
///
/// 与 VM 侧 `kernel_gateway.rs` 的 `KernelGateway` 同型：handler 面向 trait
/// 编程，生产实现走真实内核调用 wire（pre-E1 由 trap 桩诚实回 `-EIO`），
/// 测试注入脚本化 mock。E6 后续的 SYS_TIMES/SYS_CLEAR 等按同模式扩展。
pub trait KernelGateway {
    /// C: `sys_kill(proc_ep, signr)`（libsys `sys_kill.c:8-17`）——
    /// `_kernel_call(SYS_KILL, &m)`，载荷 `m_sigcalls.{endpt,sig}`，
    /// 返回值 = 内核回复（OK 或负 errno）。
    fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32>;

    /// C: `sys_clear(proc_ep)`（libsys `sys_clear.c:8-14`）——
    /// `_kernel_call(SYS_CLEAR, &m)`，载荷 m1i1 = 目标 endpoint，无回复
    /// 载荷；返回值 = 内核回复（OK 或负 errno）。
    fn sys_clear(&mut self, ep: Endpoint) -> Result<(), i32>;

    /// C: `sys_statectl(request, address, length)`（libsys
    /// `sys_statectl.c:3-11`）——状态控制面（清/挂 IPC 过滤、状态表登记）。
    /// 出生协议段一（PD-34 `process_init`）经此发出清过滤请求。默认实现
    /// 模拟接受型内核（测试替身无需逐一补写）；生产
    /// [`TrapKernelGateway`] 走 trap 直连真接线。
    fn statectl(&mut self, _request: i32, _address: u64, _length: i32) -> Result<i32, i32> {
        Ok(minix_types::OK)
    }

    /// C: `sys_abort(how)`（libsys `sys_abort.c:8-13`）——
    /// `_kernel_call(SYS_ABORT, &m)`，载荷 m1i1 = `how`（RB_* 位组）。
    /// 成功时机器直接停机；失败返回负 errno（C 调用方忽略）。
    fn sys_abort(&mut self, how: i32) -> Result<(), i32>;

    /// C: `sys_times(proc_ep, &user, &sys, NULL, NULL)`（libsys
    /// `sys_times.c:8-24`）——读取目标进程的 user/system CPU ticks。
    /// 失败返回负 errno。
    fn proc_times(&mut self, ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32>;

    /// C: `sys_datacopy(src_ep, src, dst_ep, dst, len)`（libsys
    /// `sys_datacopy.c`；kernel `dispatch_vircopy` = `Syscall::Vircopy = 15`）
    /// ——把 `bytes` 写入 `dst_ep` 进程虚地址 `dst_addr` 处。
    /// 失败返回负 errno（如父进程缓冲非法）。
    fn copy_to_user(&mut self, bytes: &[u8], dst_ep: Endpoint, dst_addr: u64) -> Result<(), i32>;

    /// C: `sys_resume(proc_ep)`（`syslib.h:48` = `sys_runctl(ep, RC_RESUME, 0)`）
    /// ——清除内核侧 `PROC_STOPPED`，恢复被停止的进程。返回原始内核回复
    ///（OK = 0 / 负 errno），调用方（signal.c:285 `try_resume_proc`）对
    /// 非 OK panic。
    fn sys_resume(&mut self, ep: Endpoint) -> Result<(), i32>;

    /// C: `sys_delay_stop(proc_ep)`（libsys `delay_stop.c`；`signal.c:239`
    /// `stop_proc` 内）——内核侧停住目标进程（`PROC_STOPPED`），返回
    /// OK = 已停 / `EBUSY` = 内核延迟调用在途（仅 `may_delay=TRUE` 时
    /// 可接受）/ 其它负 errno = 失败（`stop_proc` 对失败 panic，C 同型）。
    /// 生产实现依赖 minix-sys wrapper（edge_todo.md E6 清单已登记）。
    fn sys_delay_stop(&mut self, ep: Endpoint) -> Result<(), i32>;

    /// 恢复信号上下文(C: `sys_sigreturn(ep, ctx)`——libsys
    /// `sys_sigreturn.c`;S3 批次 B 接线)。默认 pre-wire 诚实失败,
    /// 生产 `TrapKernelGateway` 委托 minix-sys `sys_sigreturn`。
    fn sys_sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32> {
        let _ = (ep, ctx);
        Err(-minix_types::ENOSYS)
    }

    /// PM 诊断输出(D-31):一行诊断经 sys_diagctl code 1 抵达内核控制台
    /// (C printf 的 Rust 对应物)。默认 pre-wire 诚实失败;生产
    /// TrapKernelGateway 委托 minix-sys sys_diagctl_write。
    fn diag_write(&mut self, text: &str) -> Result<(), i32> {
        let _ = text;
        Err(-minix_types::ENOSYS)
    }

    /// C: `sys_trace(req, proc_ep, addr, &data)`（libsys `sys_trace.c:8-22`）
    /// ——内核 SYS_TRACE 通道：trace_stop 的 T_STOP（trace.c:263）、
    /// do_trace 的命令透传（trace.c:244-248）与 READB/WRITEB_INS
    ///（trace.c:106/129）。读值由内核写回 `data`。失败返回负 errno
    ///（调用方按 C 透传，不折叠——V3-P2-6 规约）。
    fn sys_trace(&mut self, req: i32, ep: Endpoint, addr: u64, data: &mut i64) -> Result<(), i32>;

    /// C: `sys_vircopy(src_ep, src, dst_ep, dst, len)`（libsys
    /// `sys_vircopy.c:8-16`）——跨进程虚地址复制，方向由调用方给定：
    /// T_GETRANGE/T_SETRANGE 的被跟踪进程 ↔ 调试器缓冲区搬运
    ///（trace.c:176-183）。与 [`Self::copy_to_user`] 的差异：本方法
    /// 不隐含 SELF 端。
    fn sys_vircopy(
        &mut self,
        src_ep: Endpoint,
        src: u64,
        dst_ep: Endpoint,
        dst: u64,
        len: u64,
    ) -> Result<(), i32>;

    /// C: `sys_datacopy(src_ep, src, SELF, dst, len)`（libsys
    /// `sys_datacopy.c`；内核侧与 VIRCOPY 同型，Fix #27 先例）——把
    /// `src_ep` 进程虚地址处的字节读入 PM 本地缓冲。T_GETRANGE 的
    /// `ptrace_range` 参数块即经此通道取出（trace.c:169-171）。
    fn copy_from_user(&mut self, src_ep: Endpoint, src: u64, bytes: &mut [u8]) -> Result<(), i32>;

    /// C: `sys_getksig(&target, &set)`（libsys `sys_getksig.c:8-30`）——
    /// 取回一个有待处理内核信号的进程（endpoint + 位图）；`None` =
    /// 内核侧无更多待处理信号（C：endpt == NONE）。取回即消费内核的
    /// RTS_SIGNALED。SIGKSIG 拉取循环（sef_signal.c:27-63）的取半边。
    fn get_ksig(&mut self) -> Result<Option<(Endpoint, u64)>, i32>;

    /// C: `sys_endksig(proc_nr_e, sig_nr)`（libsys `sys_endksig.c:8-20`）
    /// ——确认消费一个内核信号（清 SIG_PENDING）。调用方必须是目标
    /// 进程的信号管理器（否则 EPERM）。拉取循环的确认半边。
    fn end_ksig(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32>;

    /// C: `sys_sigsend(proc_nr_e, smp)`（libsys `sys_sigsend.c:8-18`）——
    /// 内核按 `sigmsg` 建立 sigframe 并唤醒目标进程（被捕获信号的投递
    /// 动作，signal.c:818）。EFAULT/ENOMEM = 进程内存装不下 handler
    ///（合法失败）；其它负 errno = PM/内核失配（调用方 panic，C 同型）。
    fn sys_sigsend(&mut self, ep: Endpoint, sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32>;

    /// C: `sys_diagctl_stacktrace(proc_nr_e)`（`syslib.h:167` =
    /// `sys_diagctl(DIAGCTL_CODE_STACKTRACE, NULL, ep)`）——请求内核打印
    /// 目标进程的栈回溯。C 调用方（`signal.c:464-466`、RS `main.c:681-683`）
    /// 不检查返回值；诊断通道失败静默。
    fn sys_diagctl_stacktrace(&mut self, ep: Endpoint) -> Result<(), i32>;
}

/// 生产实现：内核调用经 minix-sys 的 trap 通道（pre-E1 回 `-EIO`）。
pub struct TrapKernelGateway<T: minix_sys::syscall::KernelCallTransport> {
    pub transport: T,
}

impl<T: minix_sys::syscall::KernelCallTransport> TrapKernelGateway<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<T: minix_sys::syscall::KernelCallTransport> KernelGateway for TrapKernelGateway<T> {
    fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_kill(&self.transport, ep.0, sig);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_clear(&mut self, ep: Endpoint) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_clear(&self.transport, ep.0);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_abort(&mut self, how: i32) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_abort(&self.transport, how);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn statectl(&mut self, request: i32, address: u64, length: i32) -> Result<i32, i32> {
        let r = minix_sys::syscall::sys_statectl(&self.transport, request, address, length);
        if r < 0 { Err(r) } else { Ok(r) }
    }

    fn proc_times(&mut self, ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
        let times = minix_sys::syscall::sys_times(&self.transport, ep.0)?;
        Ok((times.user_time as minix_types::Clock, times.system_time as minix_types::Clock))
    }

    fn copy_to_user(&mut self, bytes: &[u8], dst_ep: Endpoint, dst_addr: u64) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_vircopy(
            &self.transport,
            minix_sys::syscall::SELF,
            bytes.as_ptr() as u64,
            dst_ep.0,
            dst_addr,
            bytes.len() as u64,
        );
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_resume(&mut self, ep: Endpoint) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_runctl(&self.transport, ep.0, minix_sys::syscall::RC_RESUME, 0);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> {
        // pre-E6 诚实占位：minix-sys 的 sys_delay_stop wrapper 尚未落地
        //（edge_todo.md E6 清单），任何调用都以 -EIO 失败——`stop_proc`
        // 对失败 panic（C signal.c:245 "sys_delay_stop failed" 同型），
        // 不伪造停止状态。
        Err(-minix_types::EIO)
    }

    fn sys_trace(&mut self, req: i32, ep: Endpoint, addr: u64, data: &mut i64) -> Result<(), i32> {
        // SYS_TRACE 真实通道（kernel 对端 dispatch_trace 已实现；
        // minix-sys sys_trace wrapper = E6 切片，2026-09-09 随 V3-P1-1 落地）。
        minix_sys::syscall::sys_trace(&self.transport, req, ep.0, addr, data)
    }

    fn sys_vircopy(
        &mut self,
        src_ep: Endpoint,
        src: u64,
        dst_ep: Endpoint,
        dst: u64,
        len: u64,
    ) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_vircopy(
            &self.transport,
            src_ep.0,
            src,
            dst_ep.0,
            dst,
            len,
        );
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn copy_from_user(&mut self, src_ep: Endpoint, src: u64, bytes: &mut [u8]) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_vircopy(
            &self.transport,
            src_ep.0,
            src,
            minix_sys::syscall::SELF,
            bytes.as_mut_ptr() as u64,
            bytes.len() as u64,
        );
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn get_ksig(&mut self) -> Result<Option<(Endpoint, u64)>, i32> {
        // SYS_GETKSIG 真实通道（kernel dispatch_getksig，syscall_signal.rs:393）。
        let (endpt, map) = minix_sys::syscall::sys_getksig(&self.transport)?;
        if endpt == Endpoint::NONE.0 {
            Ok(None)
        } else {
            Ok(Some((Endpoint(endpt), map)))
        }
    }

    fn end_ksig(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
        minix_sys::syscall::sys_endksig(&self.transport, ep.0, sig)
    }

    fn sys_sigsend(&mut self, ep: Endpoint, sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> {
        minix_sys::syscall::sys_sigsend(&self.transport, ep.0, sigmsg)
    }

    fn sys_diagctl_stacktrace(&mut self, ep: Endpoint) -> Result<(), i32> {
        minix_sys::syscall::sys_diagctl_stacktrace(&self.transport, ep.0)
    }

    fn sys_sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32> {
        minix_sys::syscall::sys_sigreturn(&self.transport, ep.0, ctx.0)
    }

    fn diag_write(&mut self, text: &str) -> Result<(), i32> {
        minix_sys::syscall::sys_diagctl_write(&self.transport, text)
    }
}

/// Handles `PM_EXIT` (`do_exit`, `forkexit.c:245-262`).
///
/// - `PRIV_PROC` (system service) → `sys_kill(endpoint, SIGKILL)` + `NoReply`
/// - otherwise → `exit_proc` + `NoReply` (beyond the grave, `SUSPEND` 永不回复类)
pub fn do_exit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    status: i32,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) -> ReplyIntent {
    let proc = &table.procs[caller.get()];
    if proc.is_kernel_process() {
        // C: forkexit.c:250-256 — 系统进程不得经 PM 的 exit() 终止
        //（"System processes do not use PM's exit()"）：printf 警告后
        // `sys_kill(mp->mp_endpoint, SIGKILL)`，返回值 C 不予检查——
        // 真正的终止由内核信号路径稍后经 process_ksig（11）回到 PM 完成。
        // 因此这里**不**调 exit_proc：违规进程在 PM 表中保持 Running，
        // 等待 SIGKILL 的内核信号回环。C 254-255 的 printf 是可观测
        // 面的一部分（真机上运维可见），经 pm_diag! 走内核 diagctl 通道。
        let name_raw = proc.identity.name;
        let name_len = name_raw.iter().position(|&b| b == 0).unwrap_or(name_raw.len());
        let name_str = core::str::from_utf8(&name_raw[..name_len]).unwrap_or("");
        pm_diag!(
            "PM: system process {} ({}) tries to exit(), sending SIGKILL",
            proc.endpoint().get(),
            name_str
        );
        let _ = kern.sys_kill(proc.endpoint(), crate::signal::SIGKILL);
        return ReplyIntent::NoReply;
    }
    exit_proc(
        table,
        caller,
        trunc_status(status),
        0,
        false,
        transport,
        kern,
    );
    ReplyIntent::NoReply
}

/// First half of exit: 9 steps (`forkexit.c:267-413`).
///
/// Caller must be `!PRIV_PROC` (system case handled in `do_exit`).
/// Sets `VFS_CALL` on exiting slot via `tell_vfs`, marks `EXITING`, `zombify` if
/// `!dump_core`, `disinherit` loop (INIT adoption + `NEW_PARENT`), `SIGHUP` for
/// session leader. Leaves `procs_in_use` unchanged (still counted).
///
/// `sig_status` 是 C `mp_sigstatus` 的对位（`signal.c:552`——`sig_proc_exit`
/// 在进 `exit_proc` 之前写入终止信号号；`forkexit.c:354` 作 `VFS_PM_TERM_SIG`
/// 随 DUMPCORE 告知 VFS，之后随 EXITING 保留到 zombify/wait）。非信号死亡
///（`do_exit`、VFS 驱动的销毁、trace 终止）传 0。信号号必须在调用前确定：
/// EXITING 生命周期在 step 10 才建立，若此处从 lifecycle 读回永远是 0
///（2026-10-05 §续-383：term_sig 恒 0 → VFS 拒 DUMPCORE → 退出链停摆）。
pub fn exit_proc<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    status: i8,
    sig_status: i8,
    mut dump_core: bool,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    // ---- 1. dump_core double gate (285-292) ----
    {
        let proc = &table.procs[slot.get()];
        // forkexit.c:285-286：setuid 程序（real uid ≠ effective uid）不 dump core。
        // 该判据对所有 mproc 统一（含 PRIV_PROC），凭证恒存在。
        if dump_core {
            let creds = proc.resources.privilege.credentials();
            if creds.user.real != creds.user.effective {
                dump_core = false;
            }
        }
        // forkexit.c:291-292：PRIV_PROC 系统进程不 dump core。
        if dump_core && proc.is_kernel_process() {
            dump_core = false;
        }
    }

    let proc_nr = slot.get();
    let proc_ep = table.procs[proc_nr].endpoint();
    // ---- 2. session leader procgrp memory (298) ----
    let procgrp = {
        let proc = &table.procs[proc_nr];
        if proc.identity.id.pid == proc.identity.procgrp {
            proc.identity.procgrp
        } else {
            0
        }
    };

    // ---- 3. ALARM_ON → set_alarm(0) (301) ----
    {
        let proc = &mut table.procs[proc_nr];
        if proc.resources.flags.contains(crate::mproc::RemainingFlags::ALARM_ON) {
            proc.resources.flags.remove(crate::mproc::RemainingFlags::ALARM_ON);
            proc.resources.timer = None;
        }
    }

    // ---- 4. sys_times accounting (305-310) ----
    // C: 取死亡进程自身的 user/system CPU ticks，累加进它的 child 桶
    //（`rmp->mp_child_utime += user_time`），父进程 wait 时再并入
    //（tell_parent，forkexit.c:722-723）。失败 panic——计账缺失不可恢复。
    match kern.proc_times(proc_ep) {
        Ok((user, sys)) => {
            table.procs[proc_nr].resources.child_utime += user;
            table.procs[proc_nr].resources.child_stime += sys;
        }
        Err(r) => panic!("exit_proc: sys_times failed: {}", r),
    }

    // ---- 5. PROC_STOPPED forced (326-330) ----
    {
        let proc = &mut table.procs[proc_nr];
        if !proc.state.block.stopped {
            // `sys_stop` would stop scheduling; we set flag and rely on main.c:80-82 EXITING drop
            proc.state.block.stopped = true;
        }
    }

    // ---- 6. vm_willexit (332-334) ----
    // C: `if((r=vm_willexit(proc_nr_e)) != OK) panic("exit_proc: vm_willexit
    // failed: %d", r);`——VM 的内存记账依赖该通知，失败不可恢复。
    if let Err(r) = crate::ipc::vm_willexit(transport, proc_ep) {
        panic!("exit_proc: vm_willexit failed: {}", r);
    }

    // ---- 7. INIT/VFS special (336-345) ----
    // In C: INIT dies → stacktrace + return (no VFS); VFS dies → panic
    // For Rust, we handle via early return for INIT (slot of INIT_PROC_NR) and panic for VFS
    const INIT_PROC_NR: usize = 11;
    const VFS_PROC_NR: i32 = 1;
    if proc_ep == Endpoint::from_generation_slot(0, INIT_PROC_NR as i32) || proc_nr == INIT_PROC_NR {
        // INIT died — C 336-341：printf + sys_diagctl_stacktrace + return
        //（不走 VFS：VFS 侧无 INIT 的 fproc）。诊断行经 pm_diag! 抵达内核
        // 控制台（D-31），栈回溯失败不阻断——C 就不检查返回值。
        // 与 C 的差异：C 对 INIT 不改 mp_flags（表里留在运行态无人管），
        // Rust 额外标 Exiting 使生命周期状态机可观测（见 09 文档差异表）。
        pm_diag!(
            "PM: INIT died with exit status {}; showing stacktrace",
            status
        );
        let _ = kern.sys_diagctl_stacktrace(proc_ep);
        table.procs[proc_nr].state.lifecycle = Lifecycle::Exiting { exit_code: status, sig_status };
        return;
    }
    if proc_ep.get() == VFS_PROC_NR {
        panic!("exit_proc: VFS died");
    }

    // ---- 8. VFS tell (350-359) ----
    {
        let call = if dump_core {
            // C: forkexit.c:354-357 — `m.VFS_PM_PATH = rmp->mp_name`（m7p1
            // 指针，VFS 稍后 safecopy）。本线按 OQ-5 裁决（new_edge4 §2 C-6）
            // 改为**按值携带**：`mp_name` 的 Rust 对位 `identity.name`
            // （[u8; PROC_NAME_LEN] NUL 填充）直接进载荷尾 40..56，
            // name_len = 首个 NUL 前的字节数（m7_i3 槽）——Rust 侧无可移动
            // 表行上的稳定裸指针，按值协议同时消除跨异步指针稳定性问题。
            let name = table.procs[proc_nr].identity.name;
            let name_len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            // C: forkexit.c:354 — m.VFS_PM_TERM_SIG = rmp->mp_sigstatus。
            // 信号号来自 sig_proc_exit 的写入（本函数 sig_status 参数），
            // 不从 lifecycle 读：EXITING 到 step 10 才建立，读回必是 0，
            // VFS 的 DUMPCORE 臂对 term_sig=0 拒服务（BadEndpoint），
            // 退出链自此停摆（§续-383 死亡序列的静态根因）。
            VfsCall::DumpCore {
                endpoint: proc_ep,
                term_sig: sig_status as i32,
                name_len: name_len as u32,
                name,
            }
        } else {
            VfsCall::Exit { endpoint: proc_ep }
        };
        // tell_vfs on exiting slot (utility.c:123-139) → VFS_CALL
        // tell_vfs 失败即 panic（utility.c 同型，V2-P2-3）——进程死亡时
        // VFS 告知丢失不可接受，静默继续会让 VFS 永远保留死进程状态。
        crate::ipc::tell_vfs(table, slot, call, transport);
    }

    // ---- 9. PRIV_PROC immediate sys_clear (361-369) ----
    // System process (driver) destroyed without waiting for VFS (deadlock avoidance)
    if table.procs[proc_nr].is_kernel_process() {
        // C: forkexit.c:366-368 — 失败即 panic（进程已终结而内核侧未回收
        // 即永久泄漏，不可恢复）。
        if let Err(r) = kern.sys_clear(proc_ep) {
            panic!("exit_proc: sys_clear failed: {}", r);
        }
    }

    // ---- 10. Mark EXITING (374-375) — retain IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED
    {
        let proc = &mut table.procs[proc_nr];
        // Preserve VFS_CALL (set by tell_vfs), PROC_STOPPED, TRACE_EXIT, PRIV_PROC, IN_USE
        // In Rust, IN_USE is lifecycle != Unused, VFS_CALL is BlockState, etc.
        // We just set lifecycle to Exiting, keeping other states as is
        // C 只置 EXITING 位、不动 mp_sigstatus（forkexit.c:374-375）；
        // 我方 sigstatus 字段寄生在生命周期里，构造时必须携带调用方
        // 给定的 sig_status（信号死亡的终止信号号），否则 zombify/wait
        // 上报的将是 0（§续-383 同源缺陷：信号死亡的 wait 状态错报）。
        proc.state.lifecycle = Lifecycle::Exiting {
            exit_code: status,
            sig_status,
        };
        // mp_exitstatus char truncation already via status param
    }

    // ---- 11. Zombify if !dump_core (384-385) ----
    if !dump_core {
        zombify(table, slot, transport, kern);
    }

    // ---- 12. Disinherit loop (388-409) ----
    disinherit(table, slot, transport, kern);

    // ---- 13. SIGHUP for session leader (411-412) ----
    if procgrp != 0 {
        // 会话首领死亡 → 向其进程组广播 SIGHUP（D-27）。C 复用 check_sig
        // 的负 pid 组扫描（signal.c:601-604 的 mp_procgrp 匹配），caller
        // 是死亡的首领本人（权限判定与 C 一致）；首领自身已 EXITING，
        // sig_proc 的退出守卫跳过投递。返回值 C 不检查（412）。
        let _ = crate::signal::check_sig(
            table,
            slot,
            -procgrp,
            crate::signal::SIGHUP,
            false,
            kern,
            transport,
        );
    }
}

/// Second half of exit: 5 steps (`forkexit.c:418-469`).
///
/// Called after `VFS_PM_EXIT/CORE_REPLY` → `publish_event` → `EventRegistry::resume_event`'s
/// `exit_restart` branch (06). In C, `handle_vfs_reply`'s EXIT branch does
/// `publish_event` then `return` (no tail `restart_sigs`), and `resume_event`'s
/// `Exit` termination calls `exit_restart`.
pub fn exit_restart<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    let scheduler = table.procs[slot.get()].resources.scheduler;
    // 1. sched_stop (forkexit.c:425-441)：用户态调度器在管才发
    //    SCHEDULING_STOP；调度器拒绝只 printf（C 的可观测面），进程照常
    //    交还内核调度。
    if scheduler != Endpoint::KERNEL && scheduler != Endpoint::NONE {
        let mut msg = Message::default();
        msg.m_u.m_lsys_sched_scheduling_stop =
            minix_types::ipc::MessLsysSchedSchedulingStop {
                endpoint: table.procs[slot.get()].endpoint().0,
                _padding: [0; 52],
            };
        msg.m_type = minix_types::SCHEDULING_STOP;
        let rv = match transport.sendrec(scheduler, &mut msg) {
            Ok(()) => msg.m_type,
            Err(_) => -minix_types::EIO,
        };
        if rv != 0 {
            // C forkexit.c:429-433：只 printf，继续走僵尸化。
            // 宿主构建下传输不可达时同样走这里（-EIO），行为一致。
        }
    }
    // 2. scheduler = NONE (441)
    table.procs[slot.get()].resources.scheduler = Endpoint::NONE;

    // 3. Core dump first zombify (444-445) — if not yet ZOMBIE|TRACE_ZOMBIE|TOLD_PARENT
    {
        let lc = table.procs[slot.get()].state.lifecycle;
        match lc {
            Lifecycle::TraceZombie { .. } | Lifecycle::Zombie { .. } | Lifecycle::ToldParent { .. } => {}
            _ => {
                // For dump_core path, this is first zombify
                // For normal path, already zombified, but we check again for safety
                // In Rust we only zombify if currently Exiting
                if matches!(lc, Lifecycle::Exiting { .. }) {
                    zombify(table, slot, transport, kern);
                }
            }
        }
    }

    // 4. sys_clear for !PRIV_PROC (447-452) — user process destroyed after VFS
    // C: forkexit.c:449-451 — 失败即 panic（同 exit_proc step 9 的不可恢复语义）。
    if !table.procs[slot.get()].is_kernel_process() {
        let ep = table.procs[slot.get()].endpoint();
        if let Err(r) = kern.sys_clear(ep) {
            panic!("exit_restart: sys_clear failed: {}", r);
        }
    }

    // 5. vm_exit (455-457) — VM free page tables
    // C: `if((r=vm_exit(rmp->mp_endpoint)) != OK) panic("exit_restart:
    // vm_exit failed: %d", r);`——页表随进程终结，VM 不回收即永久泄漏。
    {
        let ep = table.procs[slot.get()].endpoint();
        if let Err(r) = crate::ipc::vm_exit(transport, ep) {
            panic!("exit_restart: vm_exit failed: {}", r);
        }
    }

    // 6. TRACE_EXIT → reply(tracer, OK) (459-464, 18-trace.md)
    // C：`mproc[mp_tracer].mp_reply.m_pm_lc_ptrace.data = 0; reply(tracer, OK)`
    // ——唤醒阻塞在 ptrace(T_EXIT) 上的 tracer，补完该调用的回复。
    // Rust 对位：TRACE_EXIT 建模为 `TraceState::exit_pending`（mproc/trace.rs），
    // 经 transport.send 直接异步回复（与 tell_tracer/tell_parent 同型）。
    {
        if let Some(tracer_slot) = table.procs[slot.get()]
            .state
            .guardianship
            .tracer()
            && table.procs[slot.get()].state.trace.exit_pending
        {
            let tracer_ep = table.procs[tracer_slot.get()].endpoint();
            let mut reply_msg = Message {
                m_type: minix_types::OK,
                ..Default::default()
            };
            reply_msg.m_u.m_pm_lc_ptrace.data = 0;
            let _ = transport.send(tracer_ep, &reply_msg);
        }
    }

    // 7. TOLD_PARENT → cleanup (467-468) — parent already reaped
    if matches!(table.procs[slot.get()].state.lifecycle, Lifecycle::ToldParent { .. }) {
        cleanup(table, slot);
    }
}

/// Zombify a process (`forkexit.c:593-624`).
///
/// - `TRACE_ZOMBIE|ZOMBIE` already → panic
/// - `tracer != NO_TRACER && tracer != parent` → `TRACE_ZOMBIE` else `ZOMBIE`
/// - `!wait_test(tracer) → return` else `tell_tracer` + `check_parent(FALSE)`
pub(crate) fn zombify<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    let lc = table.procs[slot.get()].state.lifecycle;
    if matches!(lc, Lifecycle::TraceZombie { .. } | Lifecycle::Zombie { .. }) {
        panic!("zombify: process was already a zombie");
    }
    let (parent, tracer) = {
        let proc = &table.procs[slot.get()];
        (proc.state.guardianship.parent(), proc.state.guardianship.tracer())
    };
    let (exit_code, sig_status) = match table.procs[slot.get()].state.lifecycle {
        Lifecycle::Exiting { exit_code, sig_status } => (exit_code, sig_status),
        _ => (0, 0),
    };

    if let Some(tracer_slot) = tracer
        && tracer_slot != parent {
            table.procs[slot.get()].state.lifecycle = Lifecycle::TraceZombie { exit_code, sig_status };
            // Do not send SIGCHLD signals to tracers (forkexit.c:611-614)
            if !wait_test(table, tracer_slot, slot) {
                // C 613-614：tracer 未在 wait → 直接 return，不通知真父——
                // “先 tracer 后真父”的顺序由 tracer 收割后的后续链路维持
                //（10 的 wait4 收割时再走 tell_tracer→check_parent）。
                return;
            }
            tell_tracer(table, slot, transport);
            // tracer 已收到死讯，真父接着处理（forkexit.c:623）
            check_parent(table, slot, false, transport, kern);
            return;
        }
    table.procs[slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code, sig_status };
    check_parent(table, slot, false, transport, kern);
}

/// Check if parent is waiting and tell or SIGCHLD (`forkexit.c:626-665`).
///
/// `try_cleanup` saves ordering in exit_proc/exit_restart.
pub(crate) fn check_parent<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    try_cleanup: bool,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    let parent_slot = table.procs[child_slot.get()].state.guardianship.parent();
    if parent_slot.get() >= table.procs.len() {
        return;
    }
    let parent = &table.procs[parent_slot.get()];
    if parent.state.lifecycle.is_exiting() {
        // child of dead parent → INIT will reassigned, do nothing (646-650)
        return;
    }
    if wait_test(table, parent_slot, child_slot) {
        let addr = table.procs[parent_slot.get()].state.wait.rusage_addr;
        let waited = tell_parent(table, child_slot, addr, transport, kern);
        let mut try_cleanup = try_cleanup;
        if !waited {
            try_cleanup = false;
        }
        if try_cleanup && !is_vfs_or_event_blocked(table, child_slot) {
            cleanup(table, child_slot);
        }
    } else {
        // Parent not waiting → SIGCHLD（D-28）。C check_parent 尾部：
        // `sig_proc(p_mp, SIGCHLD, TRUE /*trace*/, FALSE /*ksig*/)`——
        // 默认处置下 SIGCHLD ∈ ign_sset 被忽略，装了 handler 的父进程
        // 收到通知（sigframe 交付链依赖 V2-P2-8，随批次 B 落地）。
        let _ = child_slot;
        let _ = crate::signal::sig_proc(
            table,
            parent_slot,
            crate::signal::SIGCHLD,
            true,
            false,
            kern,
            transport,
        );
    }
}

/// Tracer died (`forkexit.c:759-790`).
pub fn tracer_died<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) {
    let old = table.procs[child_slot.get()].state.guardianship.clone();
    table.procs[child_slot.get()].state.guardianship = match old {
        crate::mproc::Guardianship::Traced { parent, .. } => crate::mproc::Guardianship::Normal { parent },
        other => other,
    };
    // TRACE_EXIT cleared (768-769)：`mp_flags &= ~TRACE_EXIT` 的 Rust 对位
    // 是摘 `TraceState::exit_pending`——tracer 已死，无人再等 T_EXIT 回复。
    table.procs[child_slot.get()].state.trace.exit_pending = false;
    // If !EXITING → SIGKILL cascade (775-777)：tracer 在子进程还在跑/停着时
    // 死了，状态不可知（C 注释 "we have no idea what state the child is
    // in"），只能杀掉避免 trainwreck；可能引发级联退出（C 775-777）。
    // C: `sig_proc(child, SIGKILL, TRUE /*trace*/, FALSE /*ksig*/)`。
    if !table.procs[child_slot.get()].state.lifecycle.is_exiting() {
        let _ = crate::signal::sig_proc(
            table,
            child_slot,
            crate::signal::SIGKILL,
            true,
            false,
            kern,
            transport,
        );
        return;
    }
    // TRACE_ZOMBIE → ZOMBIE + check_parent (784-788)
    if matches!(
        table.procs[child_slot.get()].state.lifecycle,
        Lifecycle::TraceZombie { .. }
    ) {
        let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap();
        table.procs[child_slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code: ec, sig_status: ss };
        check_parent(table, child_slot, true, transport, kern);
    }
}

/// Cleanup: release slot (`forkexit.c:795-806`).
///
/// `mp_pid=0`, `mp_flags=0`, `child_utime/stime=0`, `procs_in_use--`.
/// In Rust: `ProcTable::release_slot` (table.rs:172) does `Process::default()` + `procs_in_use--`.
pub fn cleanup(table: &mut ProcTable, slot: UserSlot) {
    table.release_slot(slot.get());
}

/// Helper: is child blocked on VFS or EVENT (for check_parent try_cleanup)
fn is_vfs_or_event_blocked(table: &ProcTable, slot: UserSlot) -> bool {
    table.procs[slot.get()].state.block.ipc_blocked.is_some()
}

/// Wait test: `wait_test` (`forkexit.c:569-588`).
///
/// `parent_waiting && right_child` where `right_child` is pid/podgrp match.
/// For 09, we simplify to `WAITING && parent == child.parent` (10 will refine with pidarg).
fn wait_test(table: &ProcTable, parent_slot: UserSlot, child_slot: UserSlot) -> bool {
    let parent = &table.procs[parent_slot.get()];
    let child = &table.procs[child_slot.get()];
    if !parent.state.wait.waiting {
        return false;
    }
    // 10's wait_test includes pidarg matching (pid, pgrp, -1). For 09 we assume -1 (any child)
    // and check parent relationship already via guardianship.
    let _ = child;
    true
}

/// Tell parent: `tell_parent` (`forkexit.c:670-726`).
///
/// rusage（144 字节，仅 utime/stime）经 `KernelGateway::copy_to_user`
/// 真实写入父进程用户内存（D-21/Fix #27），随后 `reply(parent, pid)`、
/// 清 `WAITING`、`ZOMBIE→TOLD_PARENT`、子时间并入父桶（722-723）。
/// Returns `true` if wait succeeded (for check_parent try_cleanup).
pub(crate) fn tell_parent<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    addr: VirBytes,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) -> bool {
    let parent_slot = table.procs[child_slot.get()].state.guardianship.parent();
    if parent_slot.get() >= table.procs.len() {
        return false;
    }
    let child_pid = table.procs[child_slot.get()].identity.id.pid;
    let parent_ep = table.procs[parent_slot.get()].endpoint();
    // C: forkexit.c:692-704 — 先经 sys_datacopy 把 rusage 写入父进程用户
    // 内存（仅 ru_utime/ru_stime 两字段，utility.c set_rusage_times）；
    // 失败 → reply(parent, errno) + FALSE，子进程保持 ZOMBIE 可重试。

    // set_rusage_times（utility.c:144-157）：ticks → usec 按 system_hz。
    let hz: u64 = u64::from(table.system_hz);
    let (child_utime, child_stime) = {
        let child = &table.procs[child_slot.get()];
        (child.resources.child_utime, child.resources.child_stime)
    };
    let mut rusage = [0u8; 144]; // C: sizeof(struct rusage) x86-64
    {
        let u_usec = (child_utime.max(0) as u64 * 1_000_000) / hz;
        let s_usec = (child_stime.max(0) as u64 * 1_000_000) / hz;
        // ru_utime: tv_sec @0, tv_usec @8；ru_stime: tv_sec @16, tv_usec @24
        rusage[0..8].copy_from_slice(&(u_usec / 1_000_000).to_ne_bytes());
        rusage[8..16].copy_from_slice(&(u_usec % 1_000_000).to_ne_bytes());
        rusage[16..24].copy_from_slice(&(s_usec / 1_000_000).to_ne_bytes());
        rusage[24..32].copy_from_slice(&(s_usec % 1_000_000).to_ne_bytes());
    }
    // B25（NK4-C 1.48，真机 c37-c41 根因）：C 在 forkexit.c:692 用
    // `if (addr)` 守卫整段 rusage 拷贝——waitpid(pid, NULL, 0) 的
    // rusage_addr==0 是合法输入，必须跳过拷贝直接回复。旧实现无守卫，
    // 对父进程地址 0x0 发 SYS_VIRCOPY → 内核挂起 PM 问 VM 要页 → VM
    // 拒绝 → 本函数失败臂向 INIT 投 errno 消息 → INIT 的 DELIVERMSG
    // 写自己陈旧 p_delivermsg_vir=0x0 → memreq 再失败 → SIGSEGV →
    // 全系统崩（真机 c41 现场：susp-krn mt=15 tgt=0xb st=0x0 ln=0x90）。
    if addr.0 != 0 {
        if let Err(r) = kern.copy_to_user(&rusage, parent_ep, addr.0) {
            // datacopy 失败：reply(parent, errno) + FALSE（forkexit.c:699-701），
            // 子进程保持 ZOMBIE，父进程可重试 wait。
            let _ = transport.send(
                parent_ep,
                &Message {
                    m_type: r,
                    ..Default::default()
                },
            );
            return false;
        }
    }

    // C: forkexit.c:707-709 — 状态写 mp_reply.m_pm_lc_wait4.status（载荷，
    // D-26 wire 契约），pid 作返回值走 reply(parent, pid)。
    let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap_or((0, 0));
    let mut reply_msg = Message {
        m_type: child_pid,
        ..Default::default()
    };
    reply_msg.m_u.m_pm_lc_wait4.status = crate::wait::w_exitcode(ec as u8 as i32, ss as u8 as i32);
    let _ = transport.send(parent_ep, &reply_msg);

    table.procs[parent_slot.get()].state.wait.waiting = false;
    // ZOMBIE → TOLD_PARENT
    table.procs[child_slot.get()].state.lifecycle = Lifecycle::ToldParent { exit_code: ec, sig_status: ss };
    // Accumulate child times at parent (forkexit.c:722-723)
    table.procs[parent_slot.get()].resources.child_utime += child_utime;
    table.procs[parent_slot.get()].resources.child_stime += child_stime;

    true
}

/// Tell tracer: `tell_tracer` (`forkexit.c:732-754`).
pub(crate) fn tell_tracer<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    transport: &mut T,
) {
    let tracer_slot = table.procs[child_slot.get()]
        .state
        .guardianship
        .tracer()
        .expect("tracer must exist");
    let child_pid = table.procs[child_slot.get()].identity.id.pid;
    // C: forkexit.c:748-749 — `tracer->mp_reply.m_pm_lc_wait4.status =
    // W_EXITCODE(ec, sigstatus & 0377)` 后 `reply(tracer, pid)`（D-26 载荷契约）。
    let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap();
    let mut reply_msg = Message {
        m_type: child_pid,
        ..Default::default()
    };
    reply_msg.m_u.m_pm_lc_wait4.status =
        crate::wait::w_exitcode(ec as u8 as i32, ss as u8 as i32 & 0o377);
    let tracer_ep = table.procs[tracer_slot.get()].endpoint();
    let _ = transport.send(tracer_ep, &reply_msg);

    table.procs[tracer_slot.get()].state.wait.waiting = false;
    // TRACE_ZOMBIE → ZOMBIE (now zombie to parent)
    table.procs[child_slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code: ec, sig_status: ss };
}

/// Disinherit loop: `for rmp=0..NR_PROCS` (`forkexit.c:388-409`).
///
/// - `tracer == proc_nr → tracer_died`
/// - `parent == proc_nr → parent = INIT_PROC_NR + VFS_CALL→NEW_PARENT + ZOMBIE→check_parent`
///
/// C 的 `SIGHUP` 广播（411-412）在循环之后、仍在 `exit_proc` 内，由调用方执行。
fn disinherit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    exiting_slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) {
    let proc_nr = exiting_slot.get();
    // Collect affected slots first to avoid borrow conflicts
    let mut to_adopt = Vec::new();
    let mut tracer_died_slots = Vec::new();
    for (idx, proc) in table.procs.iter().enumerate() {
        if !proc.is_in_use() {
            continue;
        }
        if proc.state.guardianship.tracer() == Some(UserSlot::new(proc_nr)) {
            tracer_died_slots.push(idx);
        }
        if proc.state.guardianship.parent() == UserSlot::new(proc_nr) {
            to_adopt.push(idx);
        }
    }
    for idx in tracer_died_slots {
        tracer_died(table, UserSlot::new(idx), transport, kern);
    }
    for idx in to_adopt {
        let child_slot = UserSlot::new(idx);
        // Adopt to INIT
        {
            let child = &mut table.procs[idx];
            child.state.guardianship = match child.state.guardianship {
                crate::mproc::Guardianship::Normal { .. } => crate::mproc::Guardianship::Normal {
                    parent: UserSlot::new(11), // INIT_PROC_NR
                },
                crate::mproc::Guardianship::Traced { tracer, .. } => crate::mproc::Guardianship::Traced {
                    parent: UserSlot::new(11),
                    tracer,
                                        trace_options: crate::mproc::TraceOptions::empty(),
                },
            };
            if child.state.block.ipc_blocked.is_some() {
                // VFS_CALL → NEW_PARENT (block.rs:52)
                if let Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent }) =
                    child.state.block.ipc_blocked
                {
                    let _ = reply_to_new_parent;
                }
                // For test we set reply_to_new_parent true via VfsCall
                if let Some(crate::mproc::IpcBlockReason::VfsCall { .. }) = child.state.block.ipc_blocked {
                    child.state.block.ipc_blocked =
                        Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: true });
                }
            }
        }
        // If already ZOMBIE, check_parent for INIT（C 406-407 仅检 `ZOMBIE` 位：
        // TraceZombie 的孩子死父时不通知 INIT——它的死讯要先给 tracer，
        // 由 tracer 收割后经 tell_tracer→check_parent 链路自行处理）。
        if matches!(table.procs[idx].state.lifecycle, Lifecycle::Zombie { .. }) {
            check_parent(table, child_slot, true, transport, kern);
        }
    }
    // SIGHUP 不在本函数：C 的 411-412 在 disinherit 循环之后（仍是
    // exit_proc 主体），由 exit_proc 尾部执行（D-27）。
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Guardianship, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};
    use crate::mproc::IpcBlockReason;

    fn running_proc(table: &mut ProcTable, slot: usize, pid: i32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.procgrp = pid;
    }


    /// D-27/V2-P1-2：会话首领死亡 → 进程组广播 SIGHUP（forkexit.c:411-412，
    /// check_sig(-procgrp) 负 pid 组扫描）。同组成员默认处置终止，异组进程
    /// 存活；首领自身已 EXITING，sig_proc 的退出守卫跳过重复投递。
    #[test]
    fn test_session_leader_death_broadcasts_sighup() {
        let mut table = ProcTable::new();
        // 首领 slot 1：pid == procgrp == 100（会话首领）。
        running_proc(&mut table, 1, 100);
        // 同组成员 slot 2；异组进程 slot 3。
        running_proc(&mut table, 2, 101);
        table.procs[2].identity.procgrp = 100;
        running_proc(&mut table, 3, 102);
        table.procs[3].identity.procgrp = 200;

        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        do_exit(&mut table, UserSlot::new(1), 0, &mut t, &mut kern);

        // 同组成员被 SIGHUP 默认处置终止（EXITING → zombify）。
        assert!(
            table.procs[2].is_exiting()
                || matches!(
                    table.procs[2].state.lifecycle,
                    Lifecycle::Zombie { .. } | Lifecycle::ToldParent { .. }
                ),
            "group member must be terminated by SIGHUP, got {:?}",
            table.procs[2].state.lifecycle
        );
        // 异组进程存活。
        assert!(table.procs[3].is_in_use() && !table.procs[3].is_exiting());
    }

    /// D-28/V2-P2-5：父进程未等待时 check_parent 向其投递 SIGCHLD
    /// （C check_parent 尾部 sig_proc(p_mp, SIGCHLD, TRUE, FALSE)）。
    /// 用 mask 阻塞使投递可观察：pending 位被置位；父进程不退出。
    #[test]
    fn test_check_parent_sends_sigchld_when_parent_not_waiting() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // 父：未等待
        running_proc(&mut table, 2, 101);
        table.procs[2].state.guardianship = Guardianship::Normal { parent: UserSlot::new(1) };
        table.procs[2].state.lifecycle = Lifecycle::Zombie { exit_code: 0, sig_status: 0 };
        // 父进程阻塞 SIGCHLD → 投递落入 pending（可观察）。
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);

        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        check_parent(&mut table, UserSlot::new(2), false, &mut t, &mut kern);

        assert!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD) != 0,
            "SIGCHLD must be pending on the blocked parent"
        );
        assert!(table.procs[1].is_in_use() && !table.procs[1].is_exiting());
    }

    /// D-13：记录 sys_kill 调用的网关 mock。
    #[derive(Default)]
    struct KillRecorder {
        killed: Option<(Endpoint, i32)>,
        copied_bytes: Option<alloc::vec::Vec<u8>>,
    }
    impl KernelGateway for KillRecorder {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
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
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> {
            Ok(())
        }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            Ok((30, 12)) // 脚本化计账值（D-14 验证累加）
        }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_user(&mut self, bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            self.copied_bytes = Some(bytes.to_vec());
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> {
            Ok(())
        }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    }

    #[test]
    fn test_do_exit_priv_proc() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::Kernel(Credentials::default());
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        let intent = do_exit(&mut table, UserSlot::new(0), 0, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::NoReply);
        // Priv proc should not become Exiting via exit_proc
        assert!(matches!(table.procs[0].state.lifecycle, Lifecycle::Running));
        // D-13：违规退出经 sys_kill(endpoint, SIGKILL) 交内核信号路径处置
        assert_eq!(kern.killed, Some((Endpoint::from_generation_slot(1, 0), crate::signal::SIGKILL)));
    }

    #[test]
    fn test_do_exit_user_process_skips_sys_kill() {
        // C: forkexit.c:258-260 — 非 PRIV_PROC 走 exit_proc，不碰 sys_kill
        //（sys_kill 是 PRIV_PROC 违规分支的专属处置）。
        let mut table = ProcTable::new();
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].identity.id.pid = 200;
        table.procs[5].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();

        let intent = do_exit(&mut table, UserSlot::new(5), 0, &mut transport, &mut kern);

        assert_eq!(intent, ReplyIntent::NoReply);
        assert!(kern.killed.is_none(), "user exit must not go through sys_kill");
        // exit_proc 走完 zombify 后：父未 wait → Zombie（非 ToldParent）。
        assert!(matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. }));
    }

    #[test]
    fn test_tell_parent_delivers_rusage_via_datacopy() {
        // D-21：tell_parent 经 VIRCOPY 把 144 字节 rusage 写入父进程用户
        // 内存（forkexit.c:692-704），仅 ru_utime/ru_stime 两 timeval 有值
        //（set_rusage_times，utility.c:144-157）；datacopy 失败 →
        // reply(parent, errno) + FALSE，子保持 ZOMBIE。
        let mut table = ProcTable::new();
        table.system_hz = 100; // 显式 hz 隔离断言
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        // 子进程桶：(30 ticks, 12 ticks) @ hz=100 → (300000 usec, 120000 usec)
        table.procs[5].resources.child_utime = 30;
        table.procs[5].resources.child_stime = 12;
        table.procs[5].state.lifecycle = Lifecycle::Zombie { exit_code: 7, sig_status: 0 };
        let addr = VirBytes(0x7000);

        let told = tell_parent(&mut table, UserSlot::new(5), addr, &mut transport, &mut kern);

        assert!(told);
        let copied = kern.copied_bytes.as_ref().expect("rusage must be datacopied");
        assert_eq!(copied.len(), 144);
        let u_sec = u64::from_ne_bytes(copied[0..8].try_into().unwrap());
        let u_usec = u64::from_ne_bytes(copied[8..16].try_into().unwrap());
        let s_sec = u64::from_ne_bytes(copied[16..24].try_into().unwrap());
        let s_usec = u64::from_ne_bytes(copied[24..32].try_into().unwrap());
        assert_eq!((u_sec, u_usec), (0, 300000));
        assert_eq!((s_sec, s_usec), (0, 120000));
        // 其余 112 字节保持零（C 同样 memset 后只填两字段）
        assert!(copied[32..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_tell_parent_null_rusage_addr_skips_datacopy() {
        // B25（真机 c37-c41 根因）：C forkexit.c:692 的 `if (addr)` 守卫——
        // waitpid(pid, NULL, 0) 时 rusage_addr==0 是合法输入，必须跳过
        // rusage 拷贝直接回复；若仍对 0x0 发 VIRCOPY，内核挂起 PM 问 VM
        // 要页 → 失败臂向 INIT 投 errno → INIT DELIVERMSG 写陈旧 pdmv=0
        // → SIGSEGV 全系统崩。
        let mut table = ProcTable::new();
        table.system_hz = 100;
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        table.procs[5].resources.child_utime = 30;
        table.procs[5].resources.child_stime = 12;
        table.procs[5].state.lifecycle = Lifecycle::Zombie {
            exit_code: 7,
            sig_status: 0,
        };

        let told = tell_parent(
            &mut table,
            UserSlot::new(5),
            VirBytes(0),
            &mut transport,
            &mut kern,
        );

        assert!(told, "NULL rusage must still tell parent");
        assert!(
            kern.copied_bytes.is_none(),
            "addr==0 must not issue any datacopy (C: if (addr) guard, forkexit.c:692)"
        );
        // 父进程收到 reply(pid) 且退出状态载荷照常交付（C: forkexit.c:707-709）。
        let pid = table.procs[5].identity.id.pid;
        assert!(
            transport.sent().iter().any(|(_, m)| m.m_type == pid),
            "parent must receive the pid reply"
        );
    }

    #[test]
    fn test_exit_proc_accumulates_sys_times() {
        // D-14：exit_proc 取死亡进程自身 CPU ticks 累加进它的 child 桶
        //（forkexit.c:305-310），父进程 wait 时再并入（tell_parent 722-723）。
        // KillRecorder 的 proc_times 脚本值 = (30, 12)。
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        // KillRecorder 的 proc_times 恒 (30, 12)（脚本化计账值）。

        exit_proc(
            &mut table,
            UserSlot::new(5),
            0,
            0,
            false,
            &mut transport,
            &mut kern,
        );

        // 父进程未 wait → Zombie 持桶；wait 时桶值并入父。
        assert_eq!(table.procs[5].resources.child_utime, 30);
        assert_eq!(table.procs[5].resources.child_stime, 12);
    }

    #[test]
    fn test_exit_proc_normal() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_proc(
            &mut table,
            UserSlot::new(5),
            0,
            0,
            false,
            &mut transport,
            &mut kern,
        );
        assert!(matches!(
            table.procs[5].state.lifecycle,
            Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. } | Lifecycle::Exiting { .. }
        ));
        // VFS_CALL should be set (tell_vfs)
        assert!(table.procs[5].state.block.is_vfs_blocked());
    }

    #[test]
    fn test_exit_proc_dump_core_suppressed_for_priv() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs[5].resources.privilege = Privilege::Kernel(Credentials::default());
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_proc(
            &mut table,
            UserSlot::new(5),
            0,
            0,
            true,
            &mut transport,
            &mut kern,
        );
        // dump_core is suppressed for PRIV_PROC, so should still be Zombie not waiting for core
        // In C: dump_core && PRIV_PROC → FALSE, so !dump_core → zombify
        assert!(table.procs[5].state.lifecycle.is_zombie() || matches!(table.procs[5].state.lifecycle, Lifecycle::TraceZombie { .. }));
    }

    #[test]
    fn test_zombify_trace_zombie() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
                        trace_options: crate::mproc::TraceOptions::empty(),
        };
        // tracer at 2 is NOT waiting → stays TraceZombie (wait_test false → return)
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].state.wait.waiting = false;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern_rec = KillRecorder::default();
        zombify(&mut table, UserSlot::new(5), &mut t, &mut kern_rec);
        assert!(matches!(
            table.procs[5].state.lifecycle,
            Lifecycle::TraceZombie { .. }
        ));
    }

    #[test]
    fn test_disinherit_new_parent() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 10, 100);
        table.procs[10].state.lifecycle = Lifecycle::Running;
        // child at 11 with parent 10 and VFS_CALL
        running_proc(&mut table, 11, 101);
        table.procs[11].state.guardianship = Guardianship::Normal { parent: UserSlot::new(10) };
        table.procs[11].state.block.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        table.procs[10].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        disinherit(&mut table, UserSlot::new(10), &mut t, &mut kern);
        assert_eq!(table.procs[11].state.guardianship.parent(), UserSlot::new(11));
        assert!(matches!(
            table.procs[11].state.block.ipc_blocked,
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: true })
        ));
    }

    #[test]
    fn test_cleanup_releases_slot() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs_in_use.set(1);
        table.procs[5].state.lifecycle = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        cleanup(&mut table, UserSlot::new(5));
        assert!(!table.procs[5].is_in_use());
        assert_eq!(table.procs_in_use.get(), 0);
    }

    #[test]
    fn test_exit_restart_cleans_priv() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs[5].state.lifecycle = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        table.procs[5].resources.scheduler = Endpoint::SCHED;
        table.procs_in_use.set(1);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_restart(&mut table, UserSlot::new(5), &mut transport, &mut kern);
        // For !PRIV_PROC, sys_clear + vm_exit would be called (stubbed), and TOLD_PARENT → cleanup
        // So slot should be released
        assert!(!table.procs[5].is_in_use());
    }

    /// 回归保护（forkexit.c:613-614）：tracer 未在 wait 时 `zombify` 直接 return
    ///（C forkexit.c:613-614），不得越过 tracer 先通知真父——真父的
    /// SIGCHLD 必须保持未投递（mask 阻塞使投递可观察，D-28 手法）。
    #[test]
    fn test_zombify_tracer_not_waiting_defers_parent() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // 真父：Running
        running_proc(&mut table, 2, 101); // tracer：未在 wait
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);
        running_proc(&mut table, 5, 105);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 7, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        zombify(&mut table, UserSlot::new(5), &mut t, &mut kern);
        assert!(matches!(table.procs[5].state.lifecycle, Lifecycle::TraceZombie { .. }));
        assert_eq!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD),
            0,
            "真父不得在 tracer 之前收到死讯（C 613-614 先序）"
        );
        assert!(t.sent().is_empty(), "tracer 未等待时不应有任何异步回复发出");
    }

    /// 回归保护：`exit_restart` 步 6 对 TRACE_EXIT
    ///（`TraceState::exit_pending`）回复 `reply(tracer, OK)` 且
    /// `m_pm_lc_ptrace.data = 0`（C forkexit.c:459-464），唤醒阻塞在
    /// ptrace(T_EXIT) 上的 tracer。
    #[test]
    fn test_exit_restart_trace_exit_replies_tracer() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // 真父
        running_proc(&mut table, 2, 101); // tracer
        running_proc(&mut table, 5, 105);
        table.procs[5].state.lifecycle = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        table.procs[5].state.trace.exit_pending = true;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_restart(&mut table, UserSlot::new(5), &mut transport, &mut kern);
        let tracer_ep = table.procs[2].identity.endpoint;
        let reply = transport
            .sent()
            .iter()
            .find(|(dst, _)| *dst == tracer_ep)
            .expect("必须向 tracer 发出回复");
        assert_eq!(reply.1.m_type, minix_types::OK, "C: reply(tracer, OK)");
        // SAFETY: 读 Message 联合体成员 m_pm_lc_ptrace.data（i64）——所有成员
        // 均初始化可读（发送前刚写入同一成员），与 trace.rs 测试同一手法。
        assert_eq!(
            unsafe { reply.1.m_u.m_pm_lc_ptrace.data },
            0,
            "C: m_pm_lc_ptrace.data = 0"
        );
    }

    /// 回归保护（forkexit.c:775-777）：tracer 在子进程还在跑时死了 → SIGKILL
    /// 级联（C forkexit.c:775-777）；同时 TRACE_EXIT 位被摘
    ///（C 768-769）。子进程走默认处置终止（D-27 SIGHUP 测试同型观察）。
    #[test]
    fn test_tracer_died_kills_running_child() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // 真父
        running_proc(&mut table, 2, 101); // tracer（已死，由调用方语义保证）
        running_proc(&mut table, 5, 105); // 被跟踪子：仍 Running
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        table.procs[5].state.trace.exit_pending = true;
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        tracer_died(&mut table, UserSlot::new(5), &mut t, &mut kern);
        assert!(!table.procs[5].state.trace.exit_pending, "C 768-769：TRACE_EXIT 摘位");
        assert_eq!(
            table.procs[5].state.guardianship.tracer(),
            None,
            "tracer 死亡后监护回到 Normal"
        );
        assert!(
            table.procs[5].is_exiting(),
            "SIGKILL 默认处置必须终止还在运行的被跟踪子（C 775-777）"
        );
    }

    /// 回归保护（forkexit.c:784-788）：tracer 死在子进程报死途中
    ///（TRACE_ZOMBIE）→ 降为 ZOMBIE 并对真父重试 check_parent
    ///（C 784-788），不得误发 SIGKILL。
    #[test]
    fn test_tracer_died_trace_zombie_becomes_zombie() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100);
        running_proc(&mut table, 2, 101);
        running_proc(&mut table, 5, 105);
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);
        table.procs[5].state.lifecycle = Lifecycle::TraceZombie { exit_code: 3, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        tracer_died(&mut table, UserSlot::new(5), &mut t, &mut kern);
        assert!(
            matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { exit_code: 3, sig_status: 0 }),
            "TRACE_ZOMBIE → ZOMBIE（C 784-788）"
        );
        assert_ne!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD),
            0,
            "降级后真父未等待 → SIGCHLD 重试投递"
        );
    }
}
