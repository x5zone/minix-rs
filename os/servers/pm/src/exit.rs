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
        // 等待 SIGKILL 的内核信号回环。
        #[cfg(test)]
        eprintln!(
            "PM: system process {} tries to exit(), sending SIGKILL",
            proc.endpoint().get()
        );
        let _ = kern.sys_kill(proc.endpoint(), crate::signal::SIGKILL);
        return ReplyIntent::NoReply;
    }
    exit_proc(table, caller, trunc_status(status), false, transport, kern);
    ReplyIntent::NoReply
}

/// First half of exit: 9 steps (`forkexit.c:267-413`).
///
/// Caller must be `!PRIV_PROC` (system case handled in `do_exit`).
/// Sets `VFS_CALL` on exiting slot via `tell_vfs`, marks `EXITING`, `zombify` if
/// `!dump_core`, `disinherit` loop (INIT adoption + `NEW_PARENT`), `SIGHUP` for
/// session leader. Leaves `procs_in_use` unchanged (still counted).
pub fn exit_proc<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    status: i8,
    mut dump_core: bool,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    // ---- 1. dump_core double gate (285-292) ----
    {
        let proc = &table.procs[slot.get()];
        if dump_core && proc.resources.privilege.credentials().is_some() {
            let creds = proc.resources.privilege.credentials().unwrap();
            if creds.user.real != creds.user.effective {
                dump_core = false;
            }
        }
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
        // INIT died — in C: printf + stacktrace + return (no VFS)
        // For testability we just mark Exiting and return without VFS
        table.procs[proc_nr].state.lifecycle = Lifecycle::Exiting { exit_code: status, sig_status: 0 };
        return;
    }
    if proc_ep.get() == VFS_PROC_NR {
        panic!("exit_proc: VFS died");
    }

    // ---- 8. VFS tell (350-359) ----
    {
        let call = if dump_core {
            // C: forkexit.c:354-357 — `m.VFS_PM_PATH = rmp->mp_name`（m7p1）：
            // 指向 PM 静态 mproc 表内进程名的指针，VFS 稍后经 safecopy 从
            // PM 内存读取。Rust 无法对可移动的表数据形成跨异步的稳定裸指针，
            // 且 minix-types 的 `VfsCall::DumpCore.path` 为 i32（容不下 64 位
            // 指针）——wire 契约需与 05-stage-vfs 协同重新设计（按值携带
            // [u8;16] 名字，或 minix-types 增加 path+len 成员，挂 edge E7）。
            // [DEFERRED: D-16] 阻塞依赖：跨服务 core-name 契约决策。
            VfsCall::DumpCore {
                endpoint: proc_ep,
                term_sig: table.procs[proc_nr].state.lifecycle.exit_code().map(|(_, s)| s as i32).unwrap_or(0),
                path: 0, // [DEFERRED: D-16] 见上
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
        proc.state.lifecycle = Lifecycle::Exiting { exit_code: status, sig_status: 0 };
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
    // 1. sched_stop (425, 16-scheduling.md) — [DEFERRED: D-17] SCHED 服务器（16-stage，A-8）不存在，无对端可通话；C 对失败仅 printf，no-op 与 C 可观测行为一致
    let _ = scheduler;
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
    {
        let proc = &table.procs[slot.get()];
        if proc.resources.flags.contains(crate::mproc::RemainingFlags::from_bits_truncate(0)) {
            // TRACE_EXIT is in RemainingFlags? Actually TRACE_EXIT 0x08000 is not in RemainingFlags
            // In Rust, TRACE_EXIT is in TraceState? We check via guardianship trace flag
        }
        // For 09, we check if trace flag indicates TRACE_EXIT — simplified as no-op
        let _ = proc.state.trace.stopped;
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
            // Do not send SIGCHLD to tracer (forkexit.c:611-614)
            if wait_test(table, tracer_slot, slot) {
                tell_tracer(table, slot, transport);
            }
            // check_parent will be called after tell_tracer or directly
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
    // TRACE_EXIT cleared (768-769)
    // If !EXITING → SIGKILL cascade (775-777)
    if !table.procs[child_slot.get()].state.lifecycle.is_exiting() {
        // signal::sig_proc(SIGKILL) — deferred
        let _ = child_slot;
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
    if let Err(r) = kern.copy_to_user(&rusage, parent_ep, addr.0) {
        // datacopy 失败：reply(parent, errno) + FALSE（forkexit.c:699-701），
        // 子进程保持 ZOMBIE，父进程可重试 wait。
        let _ = transport.send(parent_ep, &Message { m_type: r, ..Default::default() });
        return false;
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
                    trace_exit: false,
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
        // If already ZOMBIE, check_parent for INIT
        if matches!(
            table.procs[idx].state.lifecycle,
            Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. }
        ) {
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
    use crate::mproc::{BlockState, IpcBlockReason};

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
    }

    #[test]
    fn test_do_exit_priv_proc() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::Kernel;
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
    fn test_exit_proc_accumulates_sys_times() {
        // D-14：exit_proc 取死亡进程自身 CPU ticks 累加进它的 child 桶
        //（forkexit.c:305-310），父进程 wait 时再并入（tell_parent 722-723）。
        // KillRecorder 的 proc_times 脚本值 = (30, 12)。
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        // KillRecorder 的 proc_times 恒 (30, 12)（脚本化计账值）。

        exit_proc(&mut table, UserSlot::new(5), 0, false, &mut transport, &mut kern);

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
        exit_proc(&mut table, UserSlot::new(5), 0, false, &mut transport, &mut kern);
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
        table.procs[5].resources.privilege = Privilege::Kernel;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_proc(&mut table, UserSlot::new(5), 0, true, &mut transport, &mut kern);
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
            trace_exit: false,
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
}
