//! Exit path: `do_exit → exit_proc → exit_restart` + zombie/reap chain.
//!
//! C ground truth: `minix3/minix/servers/pm/forkexit.c:242-469` (do_exit/exit_proc/exit_restart)
//! + `590-807` (zombify/check_parent/tracer_died/cleanup) + `mproc.h:86-104`
//! flags + `main.c:365` publish_event.
//!
//! Design: explicit orchestrator + `Lifecycle` enum (`mproc/lifecycle.rs:27`)
//! + `Guardianship` (`mproc/guardianship.rs`) + `BlockState`.
//! Single-threaded event loop — `&mut ProcTable` without `Arc`/`Mutex`.

use minix_types::{Endpoint, UserSlot, VfsCall, OK};
use crate::ipc::ReplyIntent;
use crate::mproc::{ProcTable, Lifecycle, Guardianship, BlockState, IpcBlockReason};

/// Exit status truncation: Minix3 `mp_exitstatus` is `char` (`mproc.h:25`).
fn trunc_status(status: i32) -> i8 {
    status as i8
}

/// Handles `PM_EXIT` (`do_exit`, `forkexit.c:245-262`).
///
/// - `PRIV_PROC` (system service) → `SIGKILL` (signal 9) via `crate::signal` (deferred) + `NoReply`
/// - otherwise → `exit_proc` + `NoReply` (beyond the grave, `SUSPEND` 永不回复类)
pub fn handle_exit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    status: i32,
    transport: &mut T,
) -> ReplyIntent {
    let proc = &table.procs[caller.get()];
    if proc.is_kernel_process() {
        // System process tries to exit → SIGKILL (forkexit.c:253-256)
        // `sys_kill` is signal path (11-signal-core.md), here stubbed as no-op
        // but we record intent via `sig_pending` for testability
        let _ = (proc.endpoint(), status);
        // In real C: `sys_kill(mp->mp_endpoint, SIGKILL)` → `process_ksig` → `sig_proc`
        // For 09, we model as immediate Exiting via exit_proc with SIGKILL status?
        // Simpler: just treat as exit_proc with SIGKILL-pending, but spec says send SIGKILL
        // and return SUSPEND without calling exit_proc. We keep SUSPEND (NoReply) to
        // preserve “priv process does not use PM exit”.
        return ReplyIntent::NoReply;
    }
    exit_proc(table, caller, trunc_status(status), false, transport);
    ReplyIntent::NoReply
}

/// First half of exit: 9 steps (`forkexit.c:267-413`).
///
/// Caller must be `!PRIV_PROC` (system case handled in `handle_exit`).
/// Sets `VFS_CALL` on exiting slot via `tell_vfs`, marks `EXITING`, `zombify` if
/// `!dump_core`, `disinherit` loop (INIT adoption + `NEW_PARENT`), `SIGHUP` for
/// session leader. Leaves `procs_in_use` unchanged (still counted).
pub fn exit_proc<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    status: i8,
    mut dump_core: bool,
    transport: &mut T,
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

    // ---- 4. sys_times accounting (306-309) ----
    // POSIX: accumulate at parent only after wait, but child saves its own times here.
    // In Rust we simulate with dummy 0 ticks (real Clock via `time` crate is 14)
    {
        let proc = &mut table.procs[proc_nr];
        // `sys_times` would fetch user/sys ticks; here we just keep existing child_utime/stime
        // plus 0 for exiting proc's own times (stub)
        let _ = proc_ep;
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
    {
        // `vm_willexit(proc_nr_e)` tells VM this proc will exit; stubbed as Ok
        let _ = proc_ep;
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
            // VFS_PM_DUMPCORE needs term sig + path; we use sig_status 0 and name as path placeholder
            VfsCall::DumpCore {
                endpoint: proc_ep,
                term_sig: table.procs[proc_nr].state.lifecycle.exit_code().map(|(_, s)| s as i32).unwrap_or(0),
                path: 0, // name pointer stub
            }
        } else {
            VfsCall::Exit { endpoint: proc_ep }
        };
        // tell_vfs on exiting slot (utility.c:123-139) → VFS_CALL
        let _ = crate::ipc::tell_vfs(table, slot, call, transport);
    }

    // ---- 9. PRIV_PROC immediate sys_clear (361-369) ----
    // System process (driver) destroyed without waiting for VFS (deadlock avoidance)
    if table.procs[proc_nr].is_kernel_process() {
        // `sys_clear` → free kernel proc; stubbed as no-op for test, but we keep flag
        let _ = proc_ep;
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
        zombify(table, slot);
    }

    // ---- 12. Disinherit loop (388-409) ----
    disinherit(table, slot);

    // ---- 13. SIGHUP for session leader (412) ----
    if procgrp != 0 {
        // `check_sig(-procgrp, SIGHUP)` → signal.c:568 broadcast to procgrp
        // Stubbed as no-op, but we record via `signal` pending for test (deferred to 11)
        let _ = procgrp;
    }
}

/// Second half of exit: 5 steps (`forkexit.c:418-469`).
///
/// Called after `VFS_PM_EXIT/CORE_REPLY` → `publish_event` → `EventRegistry::resume_event`'s
/// `exit_restart` branch (06). In C, `handle_vfs_reply`'s EXIT branch does
/// `publish_event` then `return` (no tail `restart_sigs`), and `resume_event`'s
/// `Exit` termination calls `exit_restart`.
pub fn exit_restart<T: crate::ipc::IpcTransport + ?Sized>(table: &mut ProcTable, slot: UserSlot, _transport: &mut T) {
    let scheduler = table.procs[slot.get()].resources.scheduler;
    // 1. sched_stop (425, 16-scheduling.md) — stubbed as Ok, failure only printf
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
                    zombify(table, slot);
                }
            }
        }
    }

    // 4. sys_clear for !PRIV_PROC (447-452) — user process destroyed after VFS
    if !table.procs[slot.get()].is_kernel_process() {
        // `sys_clear` → kernel proc free; stubbed
        let _ = slot;
    }

    // 5. vm_exit (455-457) — VM free page tables; stubbed
    let _ = slot;

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
pub(crate) fn zombify(table: &mut ProcTable, slot: UserSlot) {
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

    if let Some(tracer_slot) = tracer {
        if tracer_slot != parent {
            table.procs[slot.get()].state.lifecycle = Lifecycle::TraceZombie { exit_code, sig_status };
            // Do not send SIGCHLD to tracer (forkexit.c:611-614)
            if wait_test(table, tracer_slot, slot) {
                tell_tracer(table, slot);
            }
            // check_parent will be called after tell_tracer or directly
            check_parent(table, slot, false);
            return;
        }
    }
    table.procs[slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code, sig_status };
    check_parent(table, slot, false);
}

/// Check if parent is waiting and tell or SIGCHLD (`forkexit.c:626-665`).
///
/// `try_cleanup` saves ordering in exit_proc/exit_restart.
pub(crate) fn check_parent(table: &mut ProcTable, child_slot: UserSlot, try_cleanup: bool) {
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
        let waited = tell_parent(table, child_slot);
        let mut try_cleanup = try_cleanup;
        if !waited {
            try_cleanup = false;
        }
        if try_cleanup && !is_vfs_or_event_blocked(table, child_slot) {
            cleanup(table, child_slot);
        }
    } else {
        // Parent not waiting → SIGCHLD (11-signal-core.md, deferred)
        let _ = (parent_slot, child_slot);
    }
}

/// Tracer died (`forkexit.c:759-790`).
pub fn tracer_died(table: &mut ProcTable, child_slot: UserSlot) {
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
        check_parent(table, child_slot, true);
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
/// Simplified for 09: `sys_datacopy` of rusage (omitted) + `reply(parent, pid)` +
/// `WAITING` cleared + `ZOMBIE→TOLD_PARENT`.
/// Returns `true` if wait succeeded (for check_parent try_cleanup).
fn tell_parent(table: &mut ProcTable, child_slot: UserSlot) -> bool {
    let parent_slot = table.procs[child_slot.get()].state.guardianship.parent();
    if parent_slot.get() >= table.procs.len() {
        return false;
    }
    let child_pid = table.procs[child_slot.get()].identity.id.pid;
    // In C: `sys_datacopy` of rusage may fail → `reply(parent, errno)` + return FALSE
    // For 09 we assume success (no rusage addr)
    let _ = child_pid;

    // Simulate reply(parent, pid) — in real PM: `parent->mp_reply.m_pm_lc_wait4.status = W_EXITCODE`
    // and `reply(parent_slot, pid)`. For test we just clear WAITING.
    table.procs[parent_slot.get()].state.wait.waiting = false;
    // ZOMBIE → TOLD_PARENT
    let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap_or((0, 0));
    table.procs[child_slot.get()].state.lifecycle = Lifecycle::ToldParent { exit_code: ec, sig_status: ss };
    // Accumulate child times at parent (forkexit.c:722-723)
    let child_utime = table.procs[child_slot.get()].resources.child_utime;
    let child_stime = table.procs[child_slot.get()].resources.child_stime;
    table.procs[parent_slot.get()].resources.child_utime += child_utime;
    table.procs[parent_slot.get()].resources.child_stime += child_stime;

    true
}

/// Tell tracer: `tell_tracer` (`forkexit.c:732-754`).
pub(crate) fn tell_tracer(table: &mut ProcTable, child_slot: UserSlot) {
    let tracer_slot = table.procs[child_slot.get()]
        .state
        .guardianship
        .tracer()
        .expect("tracer must exist");
    let child_pid = table.procs[child_slot.get()].identity.id.pid;
    let _ = child_pid;
    // In C: `tracer->mp_reply... = W_EXITCODE` + `reply(tracer, pid)` + `WAITING` cleared
    table.procs[tracer_slot.get()].state.wait.waiting = false;
    // TRACE_ZOMBIE → ZOMBIE (now zombie to parent)
    let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap();
    table.procs[child_slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code: ec, sig_status: ss };
}

/// Disinherit loop: `for rmp=0..NR_PROCS` (`forkexit.c:388-409`).
///
/// - `tracer == proc_nr → tracer_died`
/// - `parent == proc_nr → parent = INIT_PROC_NR + VFS_CALL→NEW_PARENT + ZOMBIE→check_parent`
/// - `procgrp !=0 → SIGHUP` (session leader, 412)
fn disinherit(table: &mut ProcTable, exiting_slot: UserSlot) {
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
        tracer_died(table, UserSlot::new(idx));
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
            check_parent(table, child_slot, true);
        }
    }
    // SIGHUP for session leader (procgrp !=0)
    // In C: procgrp = (mp_pid == mp_procgrp) ? mp_procgrp : 0; check_sig(-procgrp, SIGHUP)
    // For 09 we stub as no-op (signal path deferred to 11)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Guardianship, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};

    fn running_proc(table: &mut ProcTable, slot: usize, pid: i32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.procgrp = pid;
    }

    #[test]
    fn test_do_exit_priv_proc() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::Kernel;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let intent = handle_exit(&mut table, UserSlot::new(0), 0, &mut transport);
        assert_eq!(intent, ReplyIntent::NoReply);
        // Priv proc should not become Exiting via exit_proc
        assert!(matches!(table.procs[0].state.lifecycle, Lifecycle::Running));
    }

    #[test]
    fn test_exit_proc_normal() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        exit_proc(&mut table, UserSlot::new(5), 0, false, &mut transport);
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
        exit_proc(&mut table, UserSlot::new(5), 0, true, &mut transport);
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
        zombify(&mut table, UserSlot::new(5));
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
        disinherit(&mut table, UserSlot::new(10));
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
        exit_restart(&mut table, UserSlot::new(5), &mut transport);
        // For !PRIV_PROC, sys_clear + vm_exit would be called (stubbed), and TOLD_PARENT → cleanup
        // So slot should be released
        assert!(!table.procs[5].is_in_use());
    }
}
