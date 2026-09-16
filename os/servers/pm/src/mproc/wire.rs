//! PM 进程表 → C `struct mproc` 字节镜像（D-29 数据路径）。
//!
//! C ground truth: `servers/pm/mproc.h` `mproc[NR_PROCS]`；wire 镜像为
//! [`minix_types::MprocWire`]（464 字节/槽，布局见证在其 `layout` 测试）。
//! 消费方（RS live-update 的 SI_PROC_TAB、IS 的 mproc_tab dump）按 C 字段
//! 偏移读取——IN_USE 位为槽有效判据，全零槽即"未使用"。
//!
//! 映射纪律：C 字段 → PM 类型化对应物逐一映射；无对应物的字段填 0 并在
//! 下表登记（与 C 消费方对"零值 = 未使用"的处理一致）：
//!
//! | C 字段 | 来源 | 说明 |
//! |---|---|---|
//! | mp_pid/endpoint/procgrp/name | identity | 直接映射 |
//! | mp_parent/mp_tracer | guardianship | `NO_TRACER = 0`（const.h:11）|
//! | mp_wpid/mp_waddr | wait | `AnyChild → -1`、`SpecificChild → pid`、`Group → pgid` |
//! | mp_exitstatus/sigstatus | lifecycle `exit_code()` | 仅 Zombie/TraceZombie/Exiting/ToldParent 有值 |
//! | mp_flags | lifecycle + block + wait + remaining + trace | 位合成（见 `flags_for`）|
//! | uid/gid ×6/ngroups/sgroups | credentials | 直接映射 |
//! | 7 × sigset | signals（SigSet u64 → LE 双 u32）| bits[2..4] 恒 0 |
//! | mp_sigreturn | signals.sigreturn_addr | 直接映射 |
//! | mp_sigact = 0 | 无对应物 | Rust 用 `Box<[SigAction]>`，无 mpsigact 指针 |
//! | mp_timer/interval | resources.timer/intervals | `tmr_next/tmr_func/tmr_arg = 0` |
//! | mp_reply | ipc.reply | 64 字节消息逐字节 |
//! | mp_frame_addr/len | ipc.frame_addr/len | 直接映射 |
//! | mp_nice/scheduler | resources | 直接映射 |

use crate::mproc::{ProcTable, Process};
use minix_types::{MprocWire, SigSetWire, MP_MAGIC, PROC_NAME_LEN};


/// C mproc.h:86-104 的 flags 位（wire 值）。
pub mod mp_flags {
    pub const IN_USE: u32 = 0x00001;
    pub const WAITING: u32 = 0x00002;
    pub const ZOMBIE: u32 = 0x00004;
    pub const PROC_STOPPED: u32 = 0x00008;
    pub const ALARM_ON: u32 = 0x00010;
    pub const EXITING: u32 = 0x00020;
    pub const TOLD_PARENT: u32 = 0x00040;
    pub const TRACE_STOPPED: u32 = 0x00080;
    pub const SIGSUSPENDED: u32 = 0x00100;
    pub const VFS_CALL: u32 = 0x00400;
    pub const NEW_PARENT: u32 = 0x00800;
    pub const UNPAUSED: u32 = 0x01000;
    pub const PRIV_PROC: u32 = 0x02000;
    pub const PARTIAL_EXEC: u32 = 0x04000;
    pub const TRACE_EXIT: u32 = 0x08000;
    pub const TRACE_ZOMBIE: u32 = 0x10000;
    pub const DELAY_CALL: u32 = 0x20000;
    pub const TAINTED: u32 = 0x40000;
    pub const EVENT_CALL: u32 = 0x80000;
}

/// C `NO_EVENTSUB`（char 槽，无订阅者）。
const NO_EVENTSUB: u8 = u8::MAX;

/// 把一个 PM 槽序列化为 C `struct mproc` wire 形状。
///
/// `idx` 是槽位索引（mp_parent/mp_tracer 等引用的就是它）。未使用槽
/// 返回全零 wire（IN_USE 未置），与 C 的空槽映像一致。
pub fn serialize_slot(idx: usize, p: &Process) -> MprocWire {
    let mut w = MprocWire::new();
    if !p.state.lifecycle.is_in_use() {
        return w;
    }

    // ── identity（mproc.h:39-45）──
    w.mp_pid = p.identity.id.pid;
    w.mp_endpoint = p.identity.endpoint.get();
    w.mp_procgrp = p.identity.procgrp;
    w.mp_name = p.identity.name;

    // ── wait（mproc.h:43/46-47：wpid/waddr，wait.rs）──
    if p.state.wait.waiting {
        w.mp_flags |= mp_flags::WAITING;
    }
    // C waitpid 语义：pid > 0 等特定子进程，pid == -1 等任意子进程。
    w.mp_wpid = match &p.state.wait.target {
        crate::mproc::WaitTarget::AnyChild => -1,
        crate::mproc::WaitTarget::SpecificChild(pid) => *pid,
        crate::mproc::WaitTarget::Group(pgid) => *pgid,
    };
    w.mp_waddr = p.state.wait.rusage_addr.0;

    // ── guardianship（mproc.h:47-48：parent/tracer；NO_TRACER = 0）──
    w.mp_parent = p.state.guardianship.parent().0 as i32;
    w.mp_tracer = p.state.guardianship.tracer().map_or(0, |t| t.0 as i32);
    w.mp_trace_flags = p.state.guardianship.trace_options().bits();

    // ── credentials（mproc.h:53-63）──
    // Privilege::User(Credentials) → uid/gid；Kernel 权限进程的 uid/gid
    // 全零（wire 零值 = C 系统进程初值语义，消费方按 uid=0 视作 root）。
    if let crate::mproc::Privilege::User(cred) = &p.resources.privilege {
        w.mp_realuid = cred.user.real;
        w.mp_effuid = cred.user.effective;
        w.mp_svuid = cred.user.saved;
        w.mp_realgid = cred.group.real;
        w.mp_effgid = cred.group.effective;
        w.mp_svgid = cred.group.saved;
        w.mp_ngroups = cred.ngroups as i32;
        w.mp_sgroups = cred.supplemental_groups;
    }
    // Kernel 权限进程：uid/gid 全零（wire 保持零值，C 消费方按 uid=0
    // 视作 root——与 C 的系统进程 mproc 初值语义一致）。

    // ── signals（mproc.h:64-70；SigSet u64 → LE 双 u32，高位 0）──
    let sig = &p.resources.signals;
    w.mp_ignore = sig_set_wire(sig.ignored);
    w.mp_catch = sig_set_wire(sig.caught);
    w.mp_sigmask = sig_set_wire(sig.mask);
    w.mp_sigmask2 = sig_set_wire(sig.mask_saved);
    w.mp_sigpending = sig_set_wire(sig.pending);
    w.mp_ksigpending = sig_set_wire(sig.kernel_pending);
    w.mp_sigtrace = sig_set_wire(sig.trace_mask);
    w.mp_sigreturn = sig.sigreturn_addr.0;

    // ── resources：时间/timer/nice/scheduler（mproc.h:49-51/72-77）──
    w.mp_child_utime = p.resources.child_utime as u64;
    w.mp_child_stime = p.resources.child_stime as u64;
    w.mp_started = p.resources.started as u64;
    w.mp_nice = p.resources.nice;
    w.mp_scheduler = p.resources.scheduler.get();
    if let Some(timer) = &p.resources.timer {
        w.mp_timer.tmr_exp_time = timer.expire_time as u64;
        // tmr_next/tmr_func/tmr_arg 无对应物（Rust timer 链在 timer.rs），
        // 保持 0 —— C 消费方只在 alarm 活跃时读完整 timer。
    }
    w.mp_interval = [p.resources.intervals[0] as u64, p.resources.intervals[1] as u64, p.resources.intervals[2] as u64];

    // ── ipc（mproc.h:73-76：reply/frame/eventsub）──
    if let Some(reply) = &p.ipc.reply {
        // SAFETY: `Message` 是 64 字节 repr(C) POD，逐字节拷入 wire。
        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::addr_of!(*reply) as *const u8,
                core::ptr::addr_of_mut!(w.mp_reply) as *mut u8,
                64,
            );
        }
    }
    w.mp_frame_addr = p.ipc.frame_addr.0;
    w.mp_frame_len = p.ipc.frame_len as u64;
    w.mp_eventsub = p
        .ipc
        .event_subscriber
        .map_or(NO_EVENTSUB, |slot| slot.0 as u8);

    // ── flags 合成（C mproc.h:86-104 位值）──
    let mut flags: u32 = mp_flags::IN_USE;
    flags |= match &p.state.lifecycle {
        crate::mproc::Lifecycle::Zombie { .. } | crate::mproc::Lifecycle::TraceZombie { .. } => {
            mp_flags::ZOMBIE
        }
        crate::mproc::Lifecycle::ToldParent { .. } => mp_flags::TOLD_PARENT,
        crate::mproc::Lifecycle::Exiting { .. } => mp_flags::EXITING,
        _ => 0,
    };
    if let Some((exit_code, sig_status)) = p.state.lifecycle.exit_code() {
        w.mp_exitstatus = exit_code as u8;
        w.mp_sigstatus = sig_status as u8;
    }
    if p.state.block.stopped {
        flags |= mp_flags::PROC_STOPPED;
    }
    if p.state.block.ipc_blocked.is_some() {
        flags |= mp_flags::VFS_CALL;
    }
    if p.state.block.unpaused {
        flags |= mp_flags::UNPAUSED;
    }
    if p.state.trace.stopped {
        flags |= mp_flags::TRACE_STOPPED;
    }
    if p.state.trace.exit_pending {
        flags |= mp_flags::TRACE_ZOMBIE;
    }
    if p.resources.flags.contains(crate::mproc::RemainingFlags::ALARM_ON) {
        flags |= mp_flags::ALARM_ON;
    }
    if p.resources.flags.contains(crate::mproc::RemainingFlags::PARTIAL_EXEC) {
        flags |= mp_flags::PARTIAL_EXEC;
    }
    if p.resources.flags.contains(crate::mproc::RemainingFlags::TAINTED) || p.resources.tainted {
        flags |= mp_flags::TAINTED;
    }
    if p.resources.signals.suspended {
        flags |= mp_flags::SIGSUSPENDED;
    }
    w.mp_flags = flags;

    w
}

/// 序列化整表（256 槽 × 464 字节 = 118784 字节，C `mproc[NR_PROCS]` 映像）。
pub fn serialize_mproc_tab(table: &ProcTable, out: &mut [MprocWire]) {
    assert_eq!(out.len(), table.procs.len(), "mproc tab slot count mismatch");
    for (idx, (wire, proc)) in out.iter_mut().zip(table.procs.iter()).enumerate() {
        *wire = serialize_slot(idx, proc);
    }
}


/// SigSet u64 → LE 双 u32 wire。
fn sig_set_wire(set: u64) -> SigSetWire {
    SigSetWire {
        bits: [set as u32, (set >> 32) as u32, 0, 0],
    }
}
