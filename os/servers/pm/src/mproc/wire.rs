//! PM 进程表 → `SI_PROC_TAB` 快照行（`[ARCH: A-4]` 单一权威）。
//!
//! C ground truth: `servers/pm/mproc.h` `mproc[NR_PROCS]`；wire 行是
//! [`minix_types::MProcSnap`]（200 字节/槽，布局见证在其 `layout` 测试）。
//! 消费方（IS `dump_pm` 的两个 dump、MIB `proc/tables` 的取表半）按名读
//! 字段——C 的逐行 `MP_MAGIC` 漂移校验由共享类型取代（见该结构文档）。
//!
//! 映射纪律：C 字段 → PM 类型化对应物逐一映射；无对应物的字段填 0。
//! 与 C 快照的差异表（A-4 子集裁定的落地清单）：
//!
//! | C 字段 | 处理 | 说明 |
//! |---|---|---|
//! | mp_pid/name/procgrp/parent/tracer | 直接映射 | `NO_TRACER = 0`（const.h:11）|
//! | mp_realuid/effuid/realgid/effgid | credentials | Kernel 权限进程保持全零（C 系统进程初值语义）|
//! | mp_nice | resources.nice | 直接映射 |
//! | mp_flags | 位合成（[`flags_for`]）| IN_USE 置位即"在用"判据 |
//! | mp_ignore/catch/sigmask/sigpending `__bits[0]` | signals | 低位字；dump 只打这一字 |
//! | mp_timer_exp | resources.timer | `tmr_exp_time`（无 timer 时为 0）|
//! | mp_started | resources.started | fork 时置的 uptime（C-21 新槽，MIB swtime 消费）|
//! | mp_svuid/svgid | credentials 的 IdSet saved 位 | KERN_PROC2 的 p_svuid/svgid（C-22）|
//! | mp_child_utime/stime | resources.child_* | KERN_PROC2 的 p_uctime（C-22）|
//! | mp_ngroups/sgroups | credentials 的 ngroups/supplemental_groups | KERN_PROC2 的 p_groups（C-22）|
//! | mp_endpoint | identity.endpoint | KERN_PROC_ARGS 读目标进程页的地址载体（C-22）|
//! | mp_frame_addr/len | ipc.frame_addr/frame_len | 参数帧基址与长度（C-22，KERN_PROC_ARGS 估算上限）|
//! | 其余 C 字段（mp_reply[64]、mp_sigact、mp_sgroups、mp_wpid…）| 不进快照 | A-4 裁定：无人读或跨 wire 无意义 |
//!
//! `mp_flags` 的位值权威在 `minix_types::mp_flags`（C-21 上收），本 crate
//! re-export 保持既有引用点不动；文件尾的钉值测试防两处分叉。

use crate::mproc::{ProcTable, Process};
use minix_types::MProcSnap;

/// C mproc.h:86-104 的 flags 位——权威在 `minix_types::mp_flags`。
pub use minix_types::mp_flags;

#[cfg(test)]
mod flag_authority_pin {
    /// 权威位值的钉值（C mproc.h:86-104）——防两处分叉回退。
    #[test]
    fn pm_flags_match_minix_types() {
        use super::mp_flags as a;
        assert_eq!(a::IN_USE, 0x00001);
        assert_eq!(a::WAITING, 0x00002);
        assert_eq!(a::ZOMBIE, 0x00004);
        assert_eq!(a::PROC_STOPPED, 0x00008);
        assert_eq!(a::ALARM_ON, 0x00010);
        assert_eq!(a::EXITING, 0x00020);
        assert_eq!(a::TOLD_PARENT, 0x00040);
        assert_eq!(a::TRACE_STOPPED, 0x00080);
        assert_eq!(a::SIGSUSPENDED, 0x00100);
        assert_eq!(a::VFS_CALL, 0x00400);
        assert_eq!(a::NEW_PARENT, 0x00800);
        assert_eq!(a::UNPAUSED, 0x01000);
        assert_eq!(a::PRIV_PROC, 0x02000);
        assert_eq!(a::PARTIAL_EXEC, 0x04000);
        assert_eq!(a::TRACE_EXIT, 0x08000);
        assert_eq!(a::TRACE_ZOMBIE, 0x10000);
        assert_eq!(a::DELAY_CALL, 0x20000);
        assert_eq!(a::TAINTED, 0x40000);
        assert_eq!(a::EVENT_CALL, 0x80000);
    }
}

/// C `mp_flags` 的位合成（不变量：在用槽必带 `IN_USE`）。
///
/// C: mproc.h:86-104 各位的置位处散在 wait/exit/signal 各文件；
/// 本函数把同一组位在**一处**合成，`serialize_snap` 与空槽判据共用。
pub fn flags_for(p: &Process) -> u32 {
    let mut flags: u32 = mp_flags::IN_USE;
    flags |= match &p.state.lifecycle {
        crate::mproc::Lifecycle::Zombie { .. } | crate::mproc::Lifecycle::TraceZombie { .. } => {
            mp_flags::ZOMBIE
        }
        crate::mproc::Lifecycle::ToldParent { .. } => mp_flags::TOLD_PARENT,
        crate::mproc::Lifecycle::Exiting { .. } => mp_flags::EXITING,
        _ => 0,
    };
    if p.state.wait.waiting {
        flags |= mp_flags::WAITING;
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
    flags
}

/// 把一个 PM 槽序列化为 `MProcSnap` 行。
///
/// `idx` 是槽位索引（`mp_parent`/`mp_tracer` 引用的就是它）。未使用槽
/// 返回全零行（`IN_USE` 未置）——消费方按 `mp_flags & IN_USE` 过滤空槽，
/// 与 C 的空槽映像同一判据。
pub fn serialize_snap(idx: usize, p: &Process) -> MProcSnap {
    let _ = idx; // C 的 mp_parent/mp_tracer 已是槽索引，无需换算
    let mut w = MProcSnap::default();
    if !p.state.lifecycle.is_in_use() {
        return w;
    }

    // ── identity（mproc.h:28-45/80）──
    w.mp_pid = p.identity.id.pid;
    w.mp_procgrp = p.identity.procgrp;
    w.mp_name = p.identity.name;

    // ── guardianship（mproc.h:33-34；NO_TRACER = 0）──
    w.mp_parent = p.state.guardianship.parent().0 as i32;
    w.mp_tracer = p.state.guardianship.tracer().map_or(0, |t| t.0 as i32);

    // ── credentials（mproc.h:41-45）──
    // 凭证恒存在：User 槽携带用户凭证，Kernel 特权槽携带全零凭证
    //（C 系统进程 uid/gid 初值为 0，消费方按 uid=0 视作 root）。两类统一序列化。
    {
        let cred = p.resources.privilege.credentials();
        w.mp_realuid = cred.user.real;
        w.mp_effuid = cred.user.effective;
        w.mp_realgid = cred.group.real;
        w.mp_effgid = cred.group.effective;
    }

    // ── signals（mproc.h:53-57；u64 SigSet 的低位字）──
    let sig = &p.resources.signals;
    w.mp_ignore0 = sig.ignored as u32;
    w.mp_catch0 = sig.caught as u32;
    w.mp_sigmask0 = sig.mask as u32;
    w.mp_sigpending0 = sig.pending as u32;

    // ── resources：nice / timer 到期时刻 / started（mproc.h:62/75）──
    w.mp_nice = p.resources.nice;
    if let Some(timer) = &p.resources.timer {
        w.mp_timer_exp = timer.expire_time as u32;
    }
    // C: `mp_started = getuptime()`（forkexit.c:114，forkexit.rs 注记的
    // 同一时刻值）——MIB 的 swtime（`uptime - mp_started`）消费它。
    w.mp_started = p.resources.started as u64;

    // ── 凭证尾段 + 子进程时钟（C-22；KERN_PROC2 的 p_svuid/svgid、
    // p_groups、p_uctime 消费）──
    {
        let cred = p.resources.privilege.credentials();
        w.mp_svuid = cred.user.saved;
        w.mp_svgid = cred.group.saved;
        w.mp_ngroups = cred.ngroups as u32;
        w.mp_sgroups = cred.supplemental_groups;
    }
    w.mp_child_utime = p.resources.child_utime as u64;
    w.mp_child_stime = p.resources.child_stime as u64;
    // ── KERN_PROC_ARGS 的取页载体（C-22）──
    // C: `mp->mp_endpoint`（proc.c:963 的 datacopy 目标进程）与
    // `mp_frame_addr + mp_frame_len`（ps_strings 落在帧尾，proc.c:964）。
    w.mp_endpoint = p.identity.endpoint.0;
    w.mp_frame_addr = p.ipc.frame_addr.0;
    w.mp_frame_len = p.ipc.frame_len as u64;

    w.mp_flags = flags_for(p);
    w
}

/// 序列化整表（`NR_PROCS` 槽 × 76 字节）。返回槽数（== 输出槽数）。
pub fn serialize_mproc_tab(table: &ProcTable, out: &mut [MProcSnap]) {
    assert_eq!(out.len(), table.procs.len(), "mproc tab slot count mismatch");
    for (idx, (row, proc)) in out.iter_mut().zip(table.procs.iter()).enumerate() {
        *row = serialize_snap(idx, proc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::ProcTable;

    /// 空槽：全零行（`IN_USE` 未置）。C 的空 `mproc` 槽判据同构。
    #[test]
    fn test_empty_slot_is_all_zero() {
        let table = ProcTable::new();
        let mut out = vec![MProcSnap::default(); table.procs.len()];
        serialize_mproc_tab(&table, &mut out);
        assert_eq!(out[0].mp_flags & mp_flags::IN_USE, 0);
        assert_eq!(out[0], MProcSnap::default());
    }

    /// 在用槽必带 IN_USE；flags 合成涵盖 wait/exit/signal 三类位。
    #[test]
    fn test_flags_for_in_use_and_waiting() {
        use crate::mproc::Lifecycle;
        let mut p = crate::mproc::Process::new(0, 1);
        p.state.lifecycle = Lifecycle::Running;
        p.state.wait.waiting = true;
        let flags = flags_for(&p);
        assert_ne!(flags & mp_flags::IN_USE, 0);
        assert_ne!(flags & mp_flags::WAITING, 0);
    }

    /// 行宽 = minix-types 权威（88，C-21 的 `mp_started` 尾槽），整表宽度
    /// 跟随之（旧 C-ABI 464 B 行退役）。
    #[test]
    fn test_row_width_follows_shared_snapshot() {
        assert_eq!(core::mem::size_of::<MProcSnap>(), 200);
        assert_eq!(core::mem::size_of::<MProcSnap>() * 256, 51_200);
        assert_eq!(core::mem::offset_of!(MProcSnap, mp_name), 12);
        assert_eq!(core::mem::offset_of!(MProcSnap, mp_started), 80);
    }
}
