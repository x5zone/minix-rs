//! Wait4: `do_wait4` + `wait_test` + `tell_parent`/`tell_tracer` + `cleanup`.
//!
//! C ground truth: `minix3/minix/servers/pm/forkexit.c:471-807` (do_wait4/wait_test/tell_parent/tell_tracer/cleanup)
//! + `utility.c:92-106` set_rusage_times + `mproc.h:86-92` WAITING/ZOMBIE/TOLD_PARENT.
//!   Design: explicit `WaitTarget` enum (`mproc/wait.rs:30`) + `WaitState` + `Lifecycle`.
//!   Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{UserSlot, Pid, VirBytes, Message, ECHILD};
use crate::ipc::ReplyIntent;
use crate::mproc::{ProcTable, Lifecycle, WaitTarget};

fn w_stopcode(sig: i32) -> i32 {
    (sig << 8) | 0x7F
}
pub(crate) fn w_exitcode(exit: i32, sig: i32) -> i32 {
    (exit << 8) | sig
}

/// Wait4 outcome for `do_wait4` dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    /// Synchronously replied with `pid` (W_STOPCODE path, `forkexit.c:531`).
    Replied(Pid),
    /// Would block, but `WNOHANG` → 0 (553-554).
    WouldBlock,
    /// No child → `ECHILD` (560-562).
    NoChild,
    /// Asynchronous `SUSPEND` (tell_parent/tell_tracer already replied, or WAITING set).
    Suspended,
}

/// Handles `PM_WAIT4` (`do_wait4`, `forkexit.c:471-564`).
///
/// `pidarg` may be 0 (normalized to `-procgrp`), `options` may contain `WNOHANG`,
/// `rusage_addr` is `VirBytes` for `sys_datacopy`.
/// Returns `ReplyIntent` for `init.rs` dispatcher: `Replied(pid)` → `Reply(pid)`,
/// `WouldBlock` → `Reply(0)`, `NoChild` → `Reply(ECHILD)`, `Suspended` → `ReplyLater`.
pub fn do_wait4<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    mut pidarg: Pid,
    options: u32,
    rusage_addr: VirBytes,
    transport: &mut T,
) -> ReplyIntent {
    const WNOHANG: u32 = 0x01;
    // Normalize pidarg==0 → -procgrp (493)
    if pidarg == 0 {
        let procgrp = table.procs[caller.get()].identity.procgrp;
        pidarg = -procgrp;
    }

    // Main scan: children counting + three rings (500-548)
    let mut children = 0;
    // Collect matching children indices for later decision
    let mut candidates = alloc::vec::Vec::new();
    for (idx, proc) in table.procs.iter().enumerate() {
        if proc.state.lifecycle.is_in_use() && !matches!(proc.state.lifecycle, Lifecycle::ToldParent { .. }) {
            // IN_USE and not TOLD_PARENT (502)
        } else {
            continue;
        }
        let parent = proc.state.guardianship.parent();
        let tracer = proc.state.guardianship.tracer();
        if parent != caller && tracer != Some(caller) {
            continue;
        }
        // If parent != caller and ZOMBIE, skip non-tracer zombie not owned by caller (504)
        if parent != caller && matches!(proc.state.lifecycle, Lifecycle::Zombie { .. }) {
            continue;
        }
        // pidarg filtering (507-508)
        let target = WaitTarget::from_pidarg(pidarg, table.procs[caller.get()].identity.procgrp);
        let matches_pid = match target {
            WaitTarget::AnyChild => true,
            WaitTarget::SpecificChild(p) => p == proc.identity.id.pid,
            WaitTarget::Group(p) => -p == proc.identity.procgrp,
        };
        if !matches_pid {
            continue;
        }
        children += 1;
        candidates.push(idx);
    }

    // Three rings in priority order: TRACE_ZOMBIE → TRACE_STOPPED → ZOMBIE
    for &idx in &candidates {
        let proc = &table.procs[idx];
        if proc.state.guardianship.tracer() == Some(caller) && matches!(proc.state.lifecycle, Lifecycle::TraceZombie { .. }) {
            // 512-517: TRACE_ZOMBIE → tell_tracer + check_parent + SUSPEND
            crate::exit::tell_tracer(table, UserSlot::new(idx), transport);
            crate::exit::check_parent(table, UserSlot::new(idx), true, transport);
            return ReplyIntent::ReplyLater;
        }
    }
    for &idx in &candidates {
        let proc = &table.procs[idx];
        if proc.state.guardianship.tracer() == Some(caller) && proc.state.trace.stopped {
            // C forkexit.c:519-531 — TRACE_STOPPED 子进程：扫描 mp_sigtrace
            // 取最低位的待报告停止信号，sigdelset 消费之，回复载荷
            // W_STOPCODE(i)、返回值 pid。sigtrace 为空时与 C 一致地落到
            // 下一个环（ZOMBIE），不虚构停止码。
            let trace_mask = table.procs[idx].resources.signals.trace_mask;
            let reported = (1..crate::mproc::_NSIG as i32)
                .find(|&i| trace_mask & (1u64 << (i - 1)) != 0);
            if let Some(signo) = reported {
                table.procs[idx].resources.signals.trace_mask &= !(1u64 << (signo - 1));
                let status = w_stopcode(signo);
                // C: forkexit.c:524-531 — mp_reply.m_pm_lc_wait4.status =
                // W_STOPCODE(i)（载荷，D-26 wire 契约），返回值 pid 作 m_type；
                // caller 的回复缓冲供挂起后异步回复路径（check_parent →
                // tell_parent）复用。
                let mut reply = Message::default();
                reply.m_u.m_pm_lc_wait4.status = status;
                table.procs[caller.get()].ipc.reply = Some(reply);
                return ReplyIntent::Reply(table.procs[idx].identity.id.pid);
            }
        }
    }
    for &idx in &candidates {
        let proc = &table.procs[idx];
        if proc.state.guardianship.parent() == caller && matches!(proc.state.lifecycle, Lifecycle::Zombie { .. }) {
            // 537-545: ZOMBIE → tell_parent + cleanup if not VFS|EVENT。
            // tell_parent（exit.rs，D-26 wire 契约）负责 reply(parent, pid)
            // + 载荷 W_EXITCODE + WAITING 清 + TOLD_PARENT + 时间累计；
            // 本环只补 cleanup（VFS|EVENT 挂起时延迟到 reply 之后）。
            // [DEFERRED: D-21] sys_datacopy(rusage)——内核 CPU 时间读取需
            // SYS_TIMES（edge E6）+ E1；C 侧自身仅填 utime/stime。
            let child_slot = UserSlot::new(idx);
            let _ = rusage_addr;
            crate::exit::tell_parent(table, child_slot, transport);
            if !is_vfs_or_event_blocked(table, child_slot) {
                crate::exit::cleanup(table, child_slot);
            }
            return ReplyIntent::ReplyLater;
        }
    }

    // Tail: no qualifying exited child (550-563)
    if children > 0 {
        if options & WNOHANG != 0 {
            return ReplyIntent::Reply(0);
        }
        // WAITING + mp_wpid/mp_waddr (556-558)
        table.procs[caller.get()].state.wait.waiting = true;
        table.procs[caller.get()].state.wait.target = WaitTarget::from_pidarg(pidarg, table.procs[caller.get()].identity.procgrp);
        table.procs[caller.get()].state.wait.rusage_addr = rusage_addr;
        ReplyIntent::ReplyLater
    } else {
        ReplyIntent::Reply(ECHILD)
    }
}

/// Helper: is child blocked on VFS or EVENT (for try_cleanup guard, 658)
fn is_vfs_or_event_blocked(table: &ProcTable, slot: UserSlot) -> bool {
    table.procs[slot.get()].state.block.ipc_blocked.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Guardianship, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot, VirBytes};

    fn running_child(table: &mut ProcTable, slot: usize, pid: i32, parent: usize) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].state.guardianship = Guardianship::Normal { parent: UserSlot::new(parent) };
    }

    #[test]
    fn test_wait4_echild_no_children() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].state.guardianship = Guardianship::Normal { parent: UserSlot::new(11) };
        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0, VirBytes(0), &mut transport);
        assert_eq!(intent, ReplyIntent::Reply(ECHILD));
    }

    #[test]
    fn test_wait4_wnohang() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        running_child(&mut table, 5, 100, 0);
        // child is Running, not Zombie, so children>0 but no exited child
        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0x01, VirBytes(0), &mut transport); // WNOHANG=1
        assert_eq!(intent, ReplyIntent::Reply(0));
    }

    #[test]
    fn test_wait4_suspend_when_child_running() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        running_child(&mut table, 5, 100, 0);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0, VirBytes(0), &mut transport);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        assert!(table.procs[0].state.wait.waiting);
    }

    #[test]
    fn test_wait4_zombie_tell_parent() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].state.wait.waiting = false;
        // zombie child
        table.procs[5].state.lifecycle = Lifecycle::Zombie { exit_code: 0, sig_status: 0 };
        table.procs[5].identity.id.pid = 100;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.guardianship = Guardianship::Normal { parent: UserSlot::new(0) };
        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0, VirBytes(0), &mut transport);
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // After tell_parent, child should be ToldParent and parent WAITING cleared if it was waiting
        // In this test parent was not WAITING, so tell_parent was via zombify path? Actually wait4's ZOMBIE branch calls tell_parent directly
        // So child should be ToldParent
        assert!(matches!(table.procs[5].state.lifecycle, crate::mproc::Lifecycle::ToldParent { .. }));
    }

    #[test]
    fn test_wait_target_from_pidarg_zero() {
        // pidarg==0 → -procgrp
        let target = WaitTarget::from_pidarg(0, 42);
        assert_eq!(target, WaitTarget::Group(-42));
        let mut state = crate::mproc::WaitState { waiting: true, target, rusage_addr: VirBytes(0) };
        assert!(state.is_waiting_for(100, 42));
        assert!(!state.is_waiting_for(100, 43));
    }

    // ── D-20：TRACE_STOPPED 环的真实 sigtrace 扫描（forkexit.c:519-531）──

    /// 播种一个被 `tracer=0` 跟踪且处于 ptrace 停止态的子进程。
    fn traced_stopped_child(table: &mut ProcTable, slot: usize, pid: i32, trace_mask: u64) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(11),
            tracer: UserSlot::new(0),
            trace_exit: false,
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        table.procs[slot].state.trace.stopped = true;
        table.procs[slot].resources.signals.trace_mask = trace_mask;
    }

    #[test]
    fn test_wait4_trace_stopped_reports_lowest_signal_and_consumes_bit() {
        // C: forkexit.c:519-531 — sigtrace 扫描取最低信号位，sigdelset
        // 消费之，回复载荷 W_STOPCODE(i)、返回 pid。
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        // 两个待报告信号：SIGTRAP(5) 与 SIGSTOP(17)——最低位 SIGTRAP 先报。
        let two_pending = (1u64 << (5 - 1)) | (1u64 << (17 - 1));
        traced_stopped_child(&mut table, 5, 100, two_pending);

        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0, VirBytes(0), &mut transport);

        assert_eq!(intent, ReplyIntent::Reply(100)); // 子进程 pid
        // D-26 wire 契约：状态在 m_pm_lc_wait4.status 载荷（C forkexit.c:528）
        let reply = table.procs[0].ipc.reply.expect("reply buffer must carry W_STOPCODE");
        assert_eq!(
            unsafe { reply.m_u.m_pm_lc_wait4.status },
            w_stopcode(5),
            "lowest pending signal first (payload)"
        );
        assert_eq!(
            table.procs[5].resources.signals.trace_mask,
            1u64 << (17 - 1),
            "consumed bit must be cleared, the other retained"
        );
    }

    #[test]
    fn test_wait4_trace_stopped_empty_sigtrace_falls_through() {
        // C: 519-531 的 for 未命中即落出 if——停止态但 sigtrace 为空时
        // 不虚构停止码，继续 ZOMBIE 环/尾部（此处无僵尸 → children>0 且
        // 无 WNOHANG → SUSPEND 等待）。
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        traced_stopped_child(&mut table, 5, 100, 0);

        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0, VirBytes(0), &mut transport);

        assert_eq!(intent, ReplyIntent::ReplyLater);
        assert!(table.procs[0].ipc.reply.is_none(), "no fabricated stop code");
    }

    #[test]
    fn test_tell_parent_async_writes_status_payload() {
        // D-26 异步路径：父未 wait → 子保持 Zombie；父随后 wait（SUSPEND）
        // 由 zombify→check_parent→tell_parent 补发 reply(parent, pid) +
        // m_pm_lc_wait4.status 载荷（forkexit.c:707-709）。
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].identity.id.pid = 50;
        // 僵尸子：exit(7)（无信号）。
        table.procs[5].state.lifecycle = Lifecycle::Zombie { exit_code: 7, sig_status: 0 };
        table.procs[5].identity.id.pid = 100;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.guardianship = Guardianship::Normal { parent: UserSlot::new(0) };
        table.procs[5].state.wait.waiting = false;

        let mut transport = crate::ipc::TestIpcTransport::default();
        let told = crate::exit::tell_parent(&mut table, UserSlot::new(5), &mut transport);

        assert!(told);
        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, Endpoint::from_generation_slot(1, 0));
        assert_eq!(sent[0].1.m_type, 100, "m_type = child pid (tag)");
        assert_eq!(
            unsafe { sent[0].1.m_u.m_pm_lc_wait4.status },
            w_exitcode(7, 0),
            "payload = W_EXITCODE (body)"
        );
        assert!(matches!(
            table.procs[5].state.lifecycle,
            Lifecycle::ToldParent { .. }
        ));
    }

    #[test]
    fn test_wait4_zombie_status_carries_wcoreflag_byte() {
        // D-07 状态级断言：僵尸子进程的 sig_status 带 WCOREFLAG（bit7）→
        // ZOMBIE 环后 ToldParent 原样保留位型（bit 运算全程 u8 域，i8 符号
        // 扩展不得破坏 bit7）。wire 载荷建模（C m_pm_lc_wait4.status）缺
        // minix-types 成员，登记为 todo.md §6 D-26（挂 E7）。
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].identity.id.pid = 50;
        table.procs[0].state.wait.waiting = true;
        // 僵尸子：SIGABRT(6) 终止且 core dumped（0o200|6 = 134 → i8 -122）。
        table.procs[5].state.lifecycle = Lifecycle::Zombie {
            exit_code: 0,
            sig_status: (0o200u8 | 6u8) as i8,
        };
        table.procs[5].identity.id.pid = 100;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].state.guardianship = Guardianship::Normal { parent: UserSlot::new(0) };

        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = do_wait4(&mut table, UserSlot::new(0), -1, 0, VirBytes(0), &mut transport);

        assert_eq!(intent, ReplyIntent::ReplyLater); // tell_parent 已在 wire 回复父
        match table.procs[5].state.lifecycle {
            Lifecycle::ToldParent { exit_code, sig_status } => {
                assert_eq!(exit_code, 0);
                assert_eq!(sig_status as u8, 0o200 | 6, "WCOREFLAG bit must survive intact");
            }
            ref other => panic!("unexpected lifecycle {:?}", other),
        }
        assert!(!table.procs[0].state.wait.waiting, "WAITING must clear");
        // D-26 wire 契约：m_type = 子 pid，载荷 m_pm_lc_wait4.status =
        // W_EXITCODE(0, 0o200|6)——状态在载荷而非 m_type。
        let wire = transport
            .sent()
            .iter()
            .find(|(ep, m)| *ep == Endpoint::from_generation_slot(1, 0) && m.m_type == 100)
            .expect("reply(parent, pid) must be sent");
        assert_eq!(
            unsafe { wire.1.m_u.m_pm_lc_wait4.status },
            0o200 | 6,
            "WCOREFLAG must travel in the typed payload"
        );
    }
}
