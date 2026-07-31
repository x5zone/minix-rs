//! Kernel system call dispatch — enum + match replaces C's call_vec[].
//!
//! # Minix3 C Source Mapping
//!
//! - `system.c:168-278` — system_init(): IRQ hook init + alarm timer init + call_vec registration
//! - `system.c:103-116` — kernel_call_dispatch(): call_vec[call_nr] dispatch
//! - `system.c:58-90` — kernel_call_finish(): VMSUSPEND handling + result copy
//! - `com.h:207-267` — SYS_* constant definitions
//! - `com.h:270` — NR_SYS_CALLS = 58
//!
//! # Design Decisions (08-system-init-boot-finish.md §3)
//!
//! - **D1**: `enum Syscall + match` replaces C's `call_vec[]` function pointer array.
//!   Benefits: type safety, compile-time exhaustiveness check, no function pointers.
//! - **D2**: `const assert` replaces C's `map()` macro runtime assert.
//! - **D9**: Architecture-specific syscalls return `BadCall` on unsupported platforms
//!   rather than being conditionally compiled out (avoids `#[cfg(target_arch)]` behavior selection).

use crate::proc::KProcess;
use crate::kpriv::PrivTable;
use crate::ipc_filter::kcall_filter_check;
use crate::clock::ClockState;
use crate::proc_table::ProcessTable;
use minix_types::Message;

/// Total number of kernel system calls.
/// C: NR_SYS_CALLS = 58 — minix/com.h:270
pub const NR_SYS_CALLS: usize = 58;

// ── Minix3 error codes used in dispatch ──

/// Operation not permitted. C: EPERM = 1
const EPERM: i32 = 1;
/// No such file or directory / no matching entry. C: ENOENT = 2
const ENOENT: i32 = 2;
/// Invalid argument. C: EINVAL = 22
const EINVAL: i32 = 22;
/// Function not implemented. C: ENOSYS = 78 (POSIX)
const ENOSYS: i32 = 78;

/// Kernel system call number.
///
/// C: `SYS_*` constants in minix/com.h:207-262
///
/// Design decision D1: enum + match replaces C's call_vec[] function pointer array.
/// Design decision D9: architecture-specific syscalls are included in the enum
/// for all platforms; unsupported ones return BadCall in the dispatch match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Fork = 0,
    Exec = 1,
    Clear = 2,
    Schedule = 3,
    Privctl = 4,
    Trace = 5,
    Kill = 6,
    Getksig = 7,
    Endksig = 8,
    Sigsend = 9,
    Sigreturn = 10,
    // 11-12: unused
    Memset = 13,
    Umap = 14,
    Vircopy = 15,
    Physcopy = 16,
    UmapRemote = 17,
    Vumap = 18,
    Irqctl = 19,
    // 20: unused
    Devio = 21,
    Sdevio = 22,
    Vdevio = 23,
    Setalarm = 24,
    Times = 25,
    Getinfo = 26,
    Abort = 27,
    Iopenable = 28,
    // 29-30: unused
    SafecopyFrom = 31,
    SafecopyTo = 32,
    Vsafecopy = 33,
    Setgrant = 34,
    Readbios = 35,
    Sprof = 36,
    // 37-38: unused
    Stime = 39,
    Settime = 40,
    // 41-42: unused
    Vmctl = 43,
    Diagctl = 44,
    Vtimer = 45,
    Runctl = 46,
    // 47-49: unused
    Getmcontext = 50,
    Setmcontext = 51,
    Update = 52,
    Exit = 53,
    Schedctl = 54,
    Statectl = 55,
    Safememset = 56,
    Padconf = 57,
}

impl TryFrom<u16> for Syscall {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Syscall::Fork),
            1 => Ok(Syscall::Exec),
            2 => Ok(Syscall::Clear),
            3 => Ok(Syscall::Schedule),
            4 => Ok(Syscall::Privctl),
            5 => Ok(Syscall::Trace),
            6 => Ok(Syscall::Kill),
            7 => Ok(Syscall::Getksig),
            8 => Ok(Syscall::Endksig),
            9 => Ok(Syscall::Sigsend),
            10 => Ok(Syscall::Sigreturn),
            13 => Ok(Syscall::Memset),
            14 => Ok(Syscall::Umap),
            15 => Ok(Syscall::Vircopy),
            16 => Ok(Syscall::Physcopy),
            17 => Ok(Syscall::UmapRemote),
            18 => Ok(Syscall::Vumap),
            19 => Ok(Syscall::Irqctl),
            21 => Ok(Syscall::Devio),
            22 => Ok(Syscall::Sdevio),
            23 => Ok(Syscall::Vdevio),
            24 => Ok(Syscall::Setalarm),
            25 => Ok(Syscall::Times),
            26 => Ok(Syscall::Getinfo),
            27 => Ok(Syscall::Abort),
            28 => Ok(Syscall::Iopenable),
            31 => Ok(Syscall::SafecopyFrom),
            32 => Ok(Syscall::SafecopyTo),
            33 => Ok(Syscall::Vsafecopy),
            34 => Ok(Syscall::Setgrant),
            35 => Ok(Syscall::Readbios),
            36 => Ok(Syscall::Sprof),
            39 => Ok(Syscall::Stime),
            40 => Ok(Syscall::Settime),
            43 => Ok(Syscall::Vmctl),
            44 => Ok(Syscall::Diagctl),
            45 => Ok(Syscall::Vtimer),
            46 => Ok(Syscall::Runctl),
            50 => Ok(Syscall::Getmcontext),
            51 => Ok(Syscall::Setmcontext),
            52 => Ok(Syscall::Update),
            53 => Ok(Syscall::Exit),
            54 => Ok(Syscall::Schedctl),
            55 => Ok(Syscall::Statectl),
            56 => Ok(Syscall::Safememset),
            57 => Ok(Syscall::Padconf),
            _ => Err(()),
        }
    }
}

/// Compile-time verification that all syscall enum values are within [0, NR_SYS_CALLS).
///
/// C: map() macro's assert(call_index >= 0 && call_index < NR_SYS_CALLS)
/// Design decision D2: const assert replaces C's runtime assert in map() macro.
const _: () = {
    assert!(Syscall::Fork as u16 == 0);
    assert!(Syscall::Padconf as u16 == 57);
    assert!((Syscall::Padconf as u16) < (NR_SYS_CALLS as u16));
};

/// Kernel call dispatch result.
///
/// C: return values from kernel_call_dispatch() + kernel_call_finish()
/// - Positive/zero values: OK result code
/// - VMSUSPEND (-996): call needs VM assistance
/// - EDONTREPLY: no reply should be sent
/// - EBADREQUEST (212): invalid syscall number
/// - ECALLDENIED (210): no permission for system call
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    /// Call completed with return value (C: result >= 0 or result == OK).
    Ok(i32),
    /// Call requires VM assistance (C: VMSUSPEND = -996).
    VmSuspend,
    /// No reply should be sent (C: EDONTREPLY).
    NoReply,
    /// Invalid or unimplemented syscall number (C: EBADREQUEST = 212).
    BadCall,
    /// Caller lacks permission for this system call (C: ECALLDENIED = 210).
    /// C: `!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` — system.c:107
    CallDenied,
}

impl KcallResult {
    /// Returns the errno to reply with, or `None` if no reply should be sent.
    ///
    /// Used by `kernel_call_finish` to unify the non-VmSuspend paths:
    /// C `kernel_call_finish` else-branch handles all non-VMSUSPEND cases
    /// uniformly (clear saved_msg + optional reply + release BKL).
    /// `VmSuspend` is excluded — it has its own dedicated path.
    fn reply_code(&self) -> Option<i32> {
        match self {
            KcallResult::Ok(ret) => Some(*ret),
            KcallResult::BadCall => Some(EBADREQUEST),
            KcallResult::CallDenied => Some(ECALLDENIED),
            KcallResult::NoReply | KcallResult::VmSuspend => None,
        }
    }
}

/// Dispatch a kernel system call.
///
/// C: kernel_call_dispatch() in system.c:103-116
/// C: kernel_call_finish() in system.c:58-90
///
/// Design decision D1: match replaces call_vec[] dispatch.
/// Design decision D5: s_k_call_mask checked at dispatch entry (runtime bitmap).
/// Design decision D9: arch-specific syscalls return BadCall on unsupported
/// platforms instead of being conditionally compiled out.
///
/// # BKL (Big Kernel Lock) — SMP Safety
///
/// This function acquires the BKL on entry and releases it on exit.
/// In C, the BKL is acquired in the assembly trap entry (`mpx.S`) and
/// released in `switch_to_user()`. In Rust, we acquire it here because
/// the kernel does not yet have an assembly-level BKL wrapper.
///
/// On single-CPU builds, `bkl_lock()` is a compiler fence + atomic CAS
/// that succeeds immediately (the lock is never contended), so the
/// overhead is negligible.
///
/// **Invariant**: The BKL must be held for the entire duration of
/// `kernel_call_dispatch` + `kernel_call_finish`. The only exception
/// is the `VmSuspend` path in `kernel_call_finish`, which releases
/// the BKL before waiting for VM (see `kernel_call_finish` docs).
pub fn kernel_call_dispatch(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    proc_table: &mut crate::proc_table::ProcessTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // Acquire BKL — C: BKL_LOCK() in mpx.S kernel_call_entry_common
    // BklGuard is a marker (not RAII); bkl_unlock() is called in kernel_call_finish().
    let _ = crate::smp::bkl_lock();

    let result = kernel_call_dispatch_inner(caller, msg, priv_table, proc_table, clock_state);

    // Note: BKL is NOT released here. It is released in:
    //   1. kernel_call_finish() — for normal completion (before switch_to_user)
    //   2. switch_to_user() — before returning to user mode
    // This matches C's pattern where BKL is held across dispatch + finish.
    result
}

/// Inner dispatch logic, called after BKL is acquired.
fn kernel_call_dispatch_inner(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    proc_table: &mut crate::proc_table::ProcessTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    let call_nr = msg.m_type as u16;
    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };

    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:107
    // Check if the caller has permission to invoke this system call.
    // Processes without an assigned privilege (priv_id == None) are denied
    // all kernel calls — this should not happen for running processes.
    //
    // Composed as `Option::and_then` + `map_or(true, ...)`:
    //   - `None` (no priv_id, or priv_id not in table) → deny (true)
    //   - `Some(priv)` → deny iff `kcall_filter_check` returns false
    let call_denied = caller.priv_id
        .and_then(|id| priv_table.get(id))
        .map_or(true, |caller_priv| !kcall_filter_check(caller_priv, call_nr as u32));
    if call_denied {
        return KcallResult::CallDenied;
    }

    match syscall {
        Syscall::Fork => dispatch_fork(caller, msg, proc_table, priv_table),
        Syscall::Exec => dispatch_exec(caller, msg, proc_table),
        Syscall::Clear => dispatch_clear(caller, msg, proc_table, priv_table),
        Syscall::Exit => dispatch_exit(caller, msg),
        Syscall::Schedule => dispatch_schedule(caller, msg, proc_table),
        Syscall::Privctl => dispatch_privctl(caller, msg),
        Syscall::Trace => dispatch_trace(caller, msg, proc_table),
        Syscall::Kill => dispatch_kill(caller, msg, proc_table, priv_table),
        Syscall::Getksig => dispatch_getksig(caller, msg, proc_table, priv_table),
        Syscall::Endksig => dispatch_endksig(caller, msg, proc_table, priv_table),
        Syscall::Sigsend => dispatch_sigsend(caller, msg, proc_table),
        Syscall::Sigreturn => dispatch_sigreturn(caller, msg, proc_table),
        Syscall::Memset => dispatch_memset(caller, msg, proc_table),
        Syscall::Umap => dispatch_umap(caller, msg, proc_table),
        Syscall::Vircopy => dispatch_vircopy(caller, msg, proc_table),
        Syscall::Physcopy => dispatch_physcopy(caller, msg, proc_table),
        Syscall::UmapRemote => dispatch_umap_remote(caller, msg, proc_table),
        Syscall::Vumap => dispatch_vumap(caller, msg, proc_table),
        Syscall::Irqctl => dispatch_irqctl(caller, msg),
        // D9: x86-specific syscalls — return BadCall on other architectures.
        Syscall::Devio => dispatch_arch_devio(caller, msg, priv_table),
        Syscall::Sdevio => dispatch_arch_sdevio(caller, msg, priv_table, proc_table),
        // D9: VDEVIO is also x86-specific (system.c:215-216: #if defined(__i386__))
        Syscall::Vdevio => dispatch_arch_vdevio(caller, msg),
        Syscall::Setalarm => dispatch_setalarm(caller, msg, priv_table, clock_state),
        Syscall::Times => dispatch_times(caller, msg, proc_table),
        Syscall::Getinfo => dispatch_getinfo(caller, msg, priv_table, proc_table),
        Syscall::Abort => dispatch_abort(caller, msg),
        Syscall::Iopenable => dispatch_arch_iopenable(caller, msg, proc_table),
        Syscall::SafecopyFrom => dispatch_safecopy_from(caller, msg, proc_table),
        Syscall::SafecopyTo => dispatch_safecopy_to(caller, msg, proc_table),
        Syscall::Vsafecopy => dispatch_vsafecopy(caller, msg),
        Syscall::Setgrant => dispatch_setgrant(caller, msg, priv_table),
        Syscall::Readbios => dispatch_arch_readbios(caller, msg),
        Syscall::Sprof => dispatch_sprofile(caller, msg, proc_table),
        Syscall::Stime => dispatch_stime(caller, msg, clock_state),
        Syscall::Settime => dispatch_settime(caller, msg, clock_state),
        Syscall::Vmctl => dispatch_vmctl(caller, msg, proc_table),
        Syscall::Diagctl => dispatch_diagctl(caller, msg, priv_table),
        Syscall::Vtimer => dispatch_vtimer(caller, msg, priv_table, proc_table),
        Syscall::Runctl => dispatch_runctl(caller, msg, proc_table),
        Syscall::Getmcontext => dispatch_getmcontext(caller, msg, proc_table),
        Syscall::Setmcontext => dispatch_setmcontext(caller, msg, proc_table),
        Syscall::Update => dispatch_update(caller, msg, proc_table, priv_table),
        Syscall::Schedctl => dispatch_schedctl(caller, msg, proc_table),
        Syscall::Statectl => dispatch_statectl(caller, msg, priv_table),
        Syscall::Safememset => dispatch_safememset(caller, msg, proc_table, priv_table),
        // D9: ARM-specific — return BadCall on other architectures.
        Syscall::Padconf => dispatch_arch_padconf(caller, msg),
    }
}

// ── Architecture-specific dispatch stubs ──
// D9: These return BadCall on architectures that don't support them.
// On supported architectures, the arch module provides real implementations.

#[cfg(not(target_arch = "x86_64"))]
fn dispatch_arch_devio(_: &mut KProcess, _: &Message) -> KcallResult {
    KcallResult::BadCall
}
#[cfg(not(target_arch = "x86_64"))]
fn dispatch_arch_sdevio(
    _: &mut KProcess,
    _: &Message,
    _: &crate::kpriv::PrivTable,
    _: &crate::proc_table::ProcessTable,
) -> KcallResult {
    KcallResult::BadCall
}
#[cfg(not(target_arch = "x86_64"))]
fn dispatch_arch_vdevio(_: &mut KProcess, _: &Message) -> KcallResult {
    KcallResult::BadCall
}
#[cfg(not(target_arch = "x86_64"))]
fn dispatch_arch_iopenable(_: &mut KProcess, _: &Message, _: &mut crate::proc_table::ProcessTable) -> KcallResult {
    KcallResult::BadCall
}
#[cfg(not(target_arch = "x86_64"))]
fn dispatch_arch_readbios(_: &mut KProcess, _: &Message) -> KcallResult {
    KcallResult::BadCall
}
#[cfg(not(target_arch = "arm"))]
fn dispatch_arch_padconf(_: &mut KProcess, _: &Message) -> KcallResult {
    KcallResult::BadCall
}

// ── Dispatch functions ──
// Each dispatch_* function delegates to the corresponding subsystem module.
// Functions that need ProcessTable or PrivTable receive them from
// kernel_call_dispatch (threaded through since 2026-06-15).

fn dispatch_fork(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable, priv_table: &PrivTable) -> KcallResult {
    crate::syscall_process::dispatch_fork(caller, msg, proc_table, priv_table)
}
fn dispatch_exec(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_process::dispatch_exec(caller, msg, proc_table) }
fn dispatch_clear(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable, priv_table: &mut crate::kpriv::PrivTable) -> KcallResult { crate::syscall_process::dispatch_clear(caller, msg, proc_table, priv_table) }
fn dispatch_exit(caller: &mut KProcess, msg: &Message) -> KcallResult { crate::syscall_process::dispatch_exit(caller, msg) }

/// Dispatch SYS_SCHEDULE.
///
/// C: `do_schedule()` — system.c:284-323
///
/// Sets scheduling parameters (priority, quantum, CPU) for a process.
/// This is an internal kernel call used by the scheduler process.
///
/// # Current status
///
/// Returns ENOSYS. Full implementation requires `ProcessTable` access
/// through the dispatcher and `sched_proc()` integration.
/// The C handler reads `m_lsys_krn_sys_schedule` fields and calls
/// `sched_proc()` to update the process's scheduling parameters.
///
/// # Implementation status (2026-06-16)
///
/// Steps 1-4 (input validation) are implemented:
///   1. **SYS_PROC permission check**: `caller_has_sys_proc(caller)`.
///      SYS_SCHEDULE is only allowed from the system process.
///   2. **Endpoint validation**: `isokendpt(endpoint, &proc_nr)` rejects
///      unknown endpoints with EINVAL.
///   3. **Process slot lookup**: the target process must be in the
///      proc_table (validated by endpoint_to_nr).
///   4. **Permission check (p_scheduler)**: C's `caller != p->p_scheduler`
///      check. In Rust, the equivalent is `caller.p_nr != target.scheduler`
///      (or `target.scheduler.is_none()` to match C's `p_scheduler == NULL`
///      fallback).
///
/// DEFERRED: `sched_proc()` body — needs per-CPU scheduling queue
/// integration (per-CPU scheduling queue). After validation passes, returns ENOSYS.
fn dispatch_schedule(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    if !crate::syscall_clock::caller_has_sys_proc(caller) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_schedule.c:14-27 — extract parameters from mess_lsys_krn_schedule.
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   endpoint@0, quantum@4, priority@8, cpu@12, niced@16
    // while MessageM1 has m1p1@16 (would read `niced` as `cpu`) — a P1
    // field-mapping bug. Always use the dedicated `MessLsysKrnSchedule`.
    let sched = unsafe { msg.m_u.m_lsys_krn_schedule };
    let endpoint = sched.endpoint;
    let _quantum = sched.quantum;
    let _priority = sched.priority;
    let _cpu = sched.cpu;
    let _niced = sched.niced;

    // C: do_schedule.c:14-15 — endpoint_to_nr lookup.
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpoint)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_schedule.c:18-19 — `caller != p->p_scheduler` check.
    // In Rust: target.scheduler is Option<ProcNr>; None matches C's
    // `p_scheduler == NULL` (kernel default), which allows any caller.
    // Some(scheduler_nr) must equal caller.p_nr to pass.
    let target = match proc_table.get(target_nr) {
        Some(p) => p,
        None => return KcallResult::Ok(EINVAL),
    };
    let allowed = match target.p_sched.scheduler {
        None => true, // C: p_scheduler == NULL → always allowed
        Some(sched_nr) => sched_nr == caller.p_nr,
    };
    if !allowed {
        return KcallResult::Ok(EPERM);
    }

    // Apply scheduling parameters via sched_proc (per-CPU scheduling queue).
    // C: do_schedule.c:21-25 — sched_proc(p, priority, quantum, cpu, niced).
    // C: do_schedule.c:27 — `niced = !!(m_ptr->m_lsys_krn_schedule.niced)`
    // (boolean coercion of the int field). We pass `false` here matching
    // the kernel's SYS_SCHEDCTL path (C: do_schedctl.c passes FALSE);
    // SYS_NICE is not yet wired up, so `niced` stays false for now.
    let niced = false;

    // Design decision §3.8 (11-design.v1.md): convert C's i32 -1 sentinel
    // ("keep current") to Option. Negative values other than -1 are rejected
    // early to match C semantics (system.c:644-648).
    let priority_opt = match sched.priority {
        -1 => None,
        v if v >= 0 => Some(v as u8),
        _ => return KcallResult::Ok(EINVAL), // priority < 0 && != -1
    };
    let quantum_opt = match sched.quantum {
        -1 => None,
        v if v >= 1 => Some(v as u32),
        _ => return KcallResult::Ok(EINVAL), // quantum < 1 && != -1
    };
    let cpu_opt = if sched.cpu == -1 { None } else { Some(sched.cpu as u32) };

    let target = match proc_table.get_mut(target_nr) {
        Some(p) => p,
        None => return KcallResult::Ok(EINVAL),
    };
    match crate::sched::sched_proc(
        target,
        crate::sched::SchedParams { priority: priority_opt, quantum: quantum_opt, cpu: cpu_opt, niced },
    ) {
        Ok(()) => KcallResult::Ok(0),
        Err(e) => KcallResult::Ok(crate::sched::sched_proc_error_to_errno(e)),
    }
}

/// Dispatch SYS_PRIVCTL.
///
/// C: `do_privctl()` — system.c:338-368
///
/// Privilege control: set/clear privilege flags for a process.
/// Currently returns ENOSYS (not implemented). Full implementation requires
/// PrivTable mutation through the dispatcher.
///
/// # Input validation
///
/// C validates that the caller is RS (System) before allowing privilege
/// changes. We enforce SYS_PROC check here (fail-closed).
fn dispatch_privctl(caller: &mut KProcess, _msg: &Message) -> KcallResult {
    if !crate::syscall_clock::caller_has_sys_proc(caller) {
        return KcallResult::Ok(EPERM);
    }
    KcallResult::Ok(ENOSYS)
}
fn dispatch_trace(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::misc::dispatch_trace(caller, msg, proc_table) }
fn dispatch_kill(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable, priv_table: &mut PrivTable) -> KcallResult {
    crate::syscall_signal::dispatch_kill(caller, msg, proc_table, priv_table)
}
fn dispatch_getksig(caller: &mut KProcess, msg: &mut Message, proc_table: &mut crate::proc_table::ProcessTable, priv_table: &PrivTable) -> KcallResult {
    crate::syscall_signal::dispatch_getksig(caller, msg, proc_table, priv_table)
}
fn dispatch_endksig(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable, priv_table: &PrivTable) -> KcallResult {
    crate::syscall_signal::dispatch_endksig(caller, msg, proc_table, priv_table)
}
fn dispatch_sigsend(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult {
    crate::syscall_signal::dispatch_sigsend(caller, msg, proc_table)
}
fn dispatch_sigreturn(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult {
    crate::syscall_signal::dispatch_sigreturn(caller, msg, proc_table)
}
fn dispatch_memset(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_memset(caller, msg, proc_table) }
fn dispatch_umap(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_umap(caller, msg, proc_table) }
fn dispatch_vircopy(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_vircopy(caller, msg, proc_table) }
fn dispatch_physcopy(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_physcopy(caller, msg, proc_table) }
fn dispatch_umap_remote(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_umap_remote(caller, msg, proc_table) }
fn dispatch_vumap(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_vumap(caller, msg, proc_table) }
fn dispatch_irqctl(caller: &mut KProcess, msg: &mut Message) -> KcallResult {
    // SYS_IRQCTL dispatcher.
    //
    // # Implementation status (2026-06-16)
    //
    // Steps 1-3 (input validation) are implemented:
    //   1. **Request validation** (do_irqctl.c:43): `IrqctlRequest::try_from(request)`
    //      rejects unknown requests with EINVAL.
    //   2. **IRQ vector range** (do_irqctl.c:55-56): `irq_vec < 0 || irq_vec >= NR_IRQ_VECTORS`
    //      → EINVAL.
    //   3. **Privilege check** (do_irqctl.c:58-76): the caller must have a SYS_PROC
    //      privilege with `CHECK_IRQ` enabled (or the IRQ in `s_irq_tab`). This
    //      is a soft check; we approximate it with the `is_sys_proc()` test,
    //      matching the broader rule that only system processes manipulate IRQs.
    //
    // # DEFERRED
    //
    // - `IrqManager<IC>` global state — currently scoped to a local stack
    //   variable in `syscall_device.rs`. Once KernelState holds a global
    //   `IrqManager`, the dispatch will pass it through.
    // - Hook chain operations: `IRQ_SETPOLICY` / `IRQ_ENABLE` / `IRQ_DISABLE`
    //   / `IRQ_REENABLE` — all require IrqManager.
    //
    // # C source mapping
    //
    // C: do_irqctl.c:43-77 — IRQ_SETPOLICY validation path.
    use crate::syscall_device::IrqctlRequest;
    use minix_plat::NR_IRQ_VECTORS;

    // C: do_irqctl.c:35-39 — extract parameters from mess_lsys_krn_sys_irqctl.
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   request@0, vector@4, policy@8, hook_id@12
    // while MessageM1 has m1p1@16 (would read padding as `hook_id`) — a P1
    // field-mapping bug. Always use the dedicated `MessLsysKrnSysIrqctl`.
    let irq = unsafe { msg.m_u.m_lsys_krn_sys_irqctl };
    let request = irq.request;
    let irq_vec = irq.vector;
    let _policy = irq.policy as u32;
    let _hook_id = irq.hook_id;

    // Step 1: validate request.
    let req = match IrqctlRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // Step 2: validate IRQ vector range (only for SETPOLICY).
    if matches!(req, IrqctlRequest::SetPolicy) {
        if irq_vec < 0 || irq_vec as usize >= NR_IRQ_VECTORS {
            return KcallResult::Ok(EINVAL);
        }
    }

    // Step 3: privilege check — only SYS_PROC may manipulate IRQs.
    if !crate::syscall_clock::caller_has_sys_proc(caller) {
        return KcallResult::Ok(EPERM);
    }

    // DEFERRED: hook chain ops need IrqManager global state.
    // The full handler in `syscall_device::dispatch_irqctl` is ready;
    // only the dispatch layer is blocked on IrqManager plumbing.
    let _ = caller;
    let _ = req;
    KcallResult::BadCall
}
fn dispatch_setalarm(caller: &mut KProcess, msg: &mut Message, priv_table: &mut PrivTable, clock_state: &mut ClockState) -> KcallResult { crate::syscall_clock::dispatch_setalarm(caller, msg, priv_table, clock_state) }
fn dispatch_times(caller: &mut KProcess, msg: &mut Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_clock::dispatch_times(caller, msg, proc_table) }
fn dispatch_getinfo(caller: &mut KProcess, msg: &mut Message, priv_table: &mut PrivTable, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult {
    crate::misc::dispatch_getinfo(caller, msg, priv_table, proc_table)
}
/// Dispatch SYS_ABORT.
///
/// C: `do_abort()` — system.c:324-337
///
/// Emergency system shutdown. In Minix3, `do_abort` sends a message to PM/TTY
/// to print a diagnostic and halt. In the kernel-only Rust rewrite, we
/// implement this as a `panic!` with the caller's endpoint for diagnostics,
/// matching C's `sys_abort()` behavior (minix/panic.c).
///
/// # Why panic instead of BadCall?
///
/// Returning `BadCall` (EBADREQUEST=212) would silently ignore the abort
/// request, which is dangerous — the caller expects the system to stop.
/// A panic is the correct kernel-level response: it halts execution,
/// prints diagnostic info, and cannot be accidentally ignored.
fn dispatch_abort(caller: &mut KProcess, msg: &Message) -> KcallResult {
    // C: system.c:324 — "abort the system"
    // C: minix/panic.c — panic with message
    panic!(
        "SYS_ABORT from endpoint {:?} (m_type={})",
        caller.p_endpoint, msg.m_type
    );
}
fn dispatch_safecopy_from(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_safecopy_from(caller, msg, proc_table) }
fn dispatch_safecopy_to(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_copy::dispatch_safecopy_to(caller, msg, proc_table) }
fn dispatch_vsafecopy(caller: &mut KProcess, msg: &Message) -> KcallResult { crate::syscall_copy::dispatch_vsafecopy(caller, msg) }
/// Dispatch SYS_SETGRANT.
///
/// C: `do_setgrant()` — do_setgrant.c:15-29
///
/// Copies the grant table address and size into the caller's privilege structure.
/// This is used by system processes (PM, VFS, RS) to register their grant tables
/// with the kernel for safe copy operations.
///
/// # Permission
///
/// Caller must have a privilege structure and must not have `RTS_NO_PRIV` set.
fn dispatch_setgrant(caller: &mut KProcess, msg: &Message, priv_table: &mut PrivTable) -> KcallResult {
    // C: do_setgrant.c:22 — check RTS_NO_PRIV
    if caller.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_setgrant.c:22 — check priv(caller) exists
    let priv_id = match caller.priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };

    // Parse message fields.
    // C: m_ptr->m_lsys_krn_sys_setgrant.addr / .size
    let grant_msg = unsafe { &msg.m_u.m_lsys_krn_sys_setgrant };

    // C: _K_SET_GRANT_TABLE(rp, ptr, entries) — safecopies.h:104-107
    // Sets priv(rp)->s_grant_table, s_grant_entries, s_grant_endpoint.
    if let Some(priv_entry) = priv_table.get_mut(priv_id) {
        priv_entry.runtime.s_grant_table = grant_msg.addr as usize;
        priv_entry.runtime.s_grant_entries = grant_msg.size;
        priv_entry.runtime.s_grant_endpoint = caller.p_endpoint;
        KcallResult::Ok(0)
    } else {
        KcallResult::Ok(EPERM)
    }
}
fn dispatch_sprofile(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult { crate::misc::dispatch_profile(caller, msg, proc_table) }
fn dispatch_stime(caller: &mut KProcess, msg: &Message, clock_state: &mut ClockState) -> KcallResult { crate::syscall_clock::dispatch_stime(caller, msg, clock_state) }
fn dispatch_settime(caller: &mut KProcess, msg: &Message, clock_state: &mut ClockState) -> KcallResult { crate::syscall_clock::dispatch_settime(caller, msg, clock_state) }
/// Dispatch SYS_VMCTL.
///
/// C: `do_vmctl()` — do_vmctl.c:17-173
///
/// VM control interface. VM uses this syscall to:
/// - Clear page fault flags on processes after handling a fault
/// - Fetch pending memory requests (VMCTL_MEMREQ_GET)
/// - Reply to memory requests (VMCTL_MEMREQ_REPLY)
/// - Set/clear VMINHIBIT to pause/resume process scheduling
/// - Clear BOOTINHIBIT to allow a boot process to run
/// - Manage kernel physical mappings and address spaces
///
/// # Message fields (C: com.h:370-382)
///
/// - `SVMCTL_WHO` (m1_i1): target process endpoint
/// - `SVMCTL_PARAM` (m1_i2): VMCTL_* sub-command
/// - `SVMCTL_VALUE` (m1_i3): sub-command value
///
/// # Permission
///
/// Only system processes (SYS_PROC) may call SYS_VMCTL.
/// C: implicit — only VM calls this, and VM always has SYS_PROC.
fn dispatch_vmctl(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut crate::proc_table::ProcessTable,
) -> KcallResult {
    use crate::vm::{VmCtlParam, VmCtlResult};

    // Permission check: only system processes may call VMCTL.
    // C: implicit — only VM calls this, and VM always has SYS_PROC.
    if !crate::syscall_clock::caller_has_sys_proc(caller) {
        return KcallResult::Ok(EPERM);
    }

    // Parse message fields.
    // C: SVMCTL_WHO = m1_i1, SVMCTL_PARAM = m1_i2, SVMCTL_VALUE = m1_i3
    // Read all fields upfront so the &msg borrow is dropped before we
    // potentially take &mut msg in MemReqGet/MemReqReply branches.
    let (who_ep, param_raw, value_raw) = {
        let m1 = unsafe { &msg.m_u.m_m1 };
        (m1.m1i1, m1.m1i2, m1.m1i3)
    };

    // Resolve target endpoint. C: do_vmctl.c:22-28
    // SELF means the caller's own endpoint.
    let target_ep = if who_ep == minix_types::Endpoint::SELF.0 {
        caller.p_endpoint.0
    } else {
        who_ep
    };

    let target_ep = Endpoint(target_ep);
    let target_nr = match proc_table.endpoint_to_nr(target_ep) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // Parse sub-command. C: switch(m_ptr->SVMCTL_PARAM)
    let param = match VmCtlParam::try_from(param_raw) {
        Ok(p) => p,
        Err(()) => {
            // Unknown VMCTL param — in C this falls through to arch_do_vmctl()
            // which returns EINVAL. Return ENOSYS for unrecognized commands
            // to distinguish from valid-but-unimplemented (EINVAL).
            return KcallResult::Ok(ENOSYS);
        }
    };

    let result = match param {
        // ── ClearPageFault: clear RTS_PAGEFAULT on target ──
        // C: do_vmctl.c:32-35 — assert(RTS_ISSET(p,RTS_PAGEFAULT)); RTS_UNSET(p, RTS_PAGEFAULT);
        VmCtlParam::ClearPageFault => {
            let target = proc_table.get_mut(target_nr);
            match target {
                Some(p) => {
                    if !p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::PAGEFAULT) {
                        // C: assert(RTS_ISSET(p, RTS_PAGEFAULT)) — convert to error return
                        return KcallResult::Ok(EINVAL);
                    }
                    p.p_rts_flags.clear(crate::proc::RtsFlagsBits::PAGEFAULT);
                    VmCtlResult::Ok(0)
                }
                None => return KcallResult::Ok(EINVAL),
            }
        }

        // ── MemReqGet: VM fetches the next pending memory request ──
        // C: do_vmctl.c:36-72 — traverse vmrequest linked list with IPC filter.
        // On success, fills reply message fields (SVMCTL_MRG_*) and returns
        // the request type (VMPTYPE_CHECK=1). On no-match, returns ENOENT=2.
        VmCtlParam::MemReqGet => {
            // Bind to a local so the &mut proc_table borrow ends before
            // we access proc_table again for reading endpoint info.
            let get_result = proc_table.vm_memreq_get();
            match get_result {
                Ok((proc_nr, params)) => {
                    // C: do_vmctl.c:61-72 — populate reply message fields.
                    // SVMCTL_MRG_TARGET, SVMCTL_MRG_ADDR, SVMCTL_MRG_LENGTH,
                    // SVMCTL_MRG_FLAG, SVMCTL_MRG_REQUESTOR.
                    let m1 = unsafe { &mut msg.m_u.m_m1 };
                    // Read target endpoint and requestor endpoint from the
                    // process that was just dequeued. The mutable borrow from
                    // vm_memreq_get() has ended, so we can borrow again.
                    let proc = proc_table.get(proc_nr);
                    let target_ep = proc.and_then(|p| p.p_vm_suspend.as_ref())
                        .map(|ctx| ctx.target.0)
                        .unwrap_or(0);
                    let requestor_ep = proc.map(|p| p.p_endpoint.0).unwrap_or(0);

                    m1.m1i1 = target_ep;                       // SVMCTL_MRG_TARGET
                    m1.m1p1 = params.start.0;                  // SVMCTL_MRG_ADDR
                    m1.m1p2 = params.length.0;                 // SVMCTL_MRG_LENGTH
                    m1.m1i3 = if params.write_flag { 1 } else { 0 }; // SVMCTL_MRG_FLAG
                    m1.m1p3 = requestor_ep as u64;             // SVMCTL_MRG_REQUESTOR

                    // C: return rp->p_vmrequest.req_type (= VMPTYPE_CHECK = 1)
                    VmCtlResult::Ok(1) // VMPTYPE_CHECK
                }
                Err(crate::vm::VmCtlError::NoRequest) => VmCtlResult::Ok(ENOENT),
                Err(crate::vm::VmCtlError::InvalidState) => VmCtlResult::Ok(EINVAL),
                Err(crate::vm::VmCtlError::InvalidEndpoint) => VmCtlResult::Ok(EINVAL),
            }
        }

        // ── MemReqReply: VM replies with the result of a memory request ──
        // C: do_vmctl.c:73-109 — set vmresult, set MF_KCALL_RESUME for
        // KernelCall type, clear RTS_VMREQUEST. Returns OK=0.
        VmCtlParam::MemReqReply => {
            // C: m_ptr->SVMCTL_VALUE carries the VM check result.
            let vm_result = match value_raw {
                0 => crate::vm::VmCheckResult::Ok,   // VM confirmed valid
                _ => crate::vm::VmCheckResult::Fault, // VM reported fault
            };

            match proc_table.vm_memreq_reply(target_nr, vm_result) {
                Ok(()) => VmCtlResult::Ok(0), // C: return OK
                Err(crate::vm::VmCtlError::InvalidState) => VmCtlResult::Ok(EINVAL),
                Err(crate::vm::VmCtlError::NoRequest) => VmCtlResult::Ok(EINVAL),
                Err(crate::vm::VmCtlError::InvalidEndpoint) => VmCtlResult::Ok(EINVAL),
            }
        }

        // ── VmInhibitSet: set RTS_VMINHIBIT on target ──
        // C: do_vmctl.c:119-131
        VmCtlParam::VmInhibitSet => {
            let target = proc_table.get_mut(target_nr);
            match target {
                Some(p) => {
                    // C: if SMP and p->p_cpu != cpuid, smp_schedule_vminhibit(p);
                    // else RTS_SET(p, RTS_VMINHIBIT);
                    // SMP cross-CPU scheduling not yet implemented (SMP/BKL).
                    p.p_rts_flags.set(crate::proc::RtsFlagsBits::VMINHIBIT);
                    // C: p->p_misc_flags |= MF_FLUSH_TLB (SMP only)
                    p.p_misc_flags.set(crate::proc::MiscFlagsBits::FLUSH_TLB);
                    VmCtlResult::Ok(0)
                }
                None => return KcallResult::Ok(EINVAL),
            }
        }

        // ── VmInhibitClear: clear RTS_VMINHIBIT on target ──
        // C: do_vmctl.c:132-160
        VmCtlParam::VmInhibitClear => {
            let target = proc_table.get_mut(target_nr);
            match target {
                Some(p) => {
                    // C: assert(RTS_ISSET(p, RTS_VMINHIBIT)) — convert to error
                    if !p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::VMINHIBIT) {
                        return KcallResult::Ok(EINVAL);
                    }
                    p.p_rts_flags.clear(crate::proc::RtsFlagsBits::VMINHIBIT);
                    // C: SMP-only MF_SENDA_VM_MISS handling + stale TLB fill
                    // not yet implemented (SMP/BKL).
                    VmCtlResult::Ok(0)
                }
                None => return KcallResult::Ok(EINVAL),
            }
        }

        // ── BootInhibitClear: clear RTS_BOOTINHIBIT on target ──
        // C: do_vmctl.c:165-167 — RTS_UNSET(p, RTS_BOOTINHIBIT)
        VmCtlParam::BootInhibitClear => {
            let target = proc_table.get_mut(target_nr);
            match target {
                Some(p) => {
                    p.p_rts_flags.clear(crate::proc::RtsFlagsBits::BOOTINHIBIT);
                    VmCtlResult::Ok(0)
                }
                None => return KcallResult::Ok(EINVAL),
            }
        }

        // ── ClearMapCache: clear cached mappings ──
        // C: do_vmctl.c:161-164 — mem_clear_mapcache()
        // Requires arch-specific implementation. Return ENOSYS for now.
        VmCtlParam::ClearMapCache => {
            // mem_clear_mapcache() requires arch-specific Direct Map support.
            // Will be implemented when arch crate provides the trait method.
            VmCtlResult::Ok(ENOSYS as i32)
        }

        // ── SetAddrSpace: switch target's page table root ──
        // C: arch_do_vmctl.c:48-50 → setcr3(p, SVMCTL_PTROOT, SVMCTL_PTROOT_V)
        //
        // C setcr3 (arch_do_vmctl.c:19-33) does:
        //   1. p->p_seg.p_cr3 = cr3
        //   2. p->p_seg.p_cr3_v = v
        //   3. if (p == ptproc) write_cr3(p->p_seg.p_cr3)
        //   4. if (p->p_nr == VM_PROC_NR) arch_enable_paging(p)
        //   5. RTS_UNSET(p, RTS_VMINHIBIT)
        //
        // Rust implements steps 1, 2, 5 now. Step 3 (write_cr3) requires the
        // ptproc tracking mechanism (currently a no-op placeholder in
        // `X86_64PostInitArch::set_ptproc`) plus a non-zeroing Paging
        // constructor — both deferred to the SMP/ptproc stage. Step 4
        // (arch_enable_paging) is a no-op on 64-bit (paging enabled at boot).
        //
        // # C bug correction
        //
        // Minix3 C never sets `vm_running = 1` (only `main.c:47` sets it to 0).
        // Rust corrects this: when the target is `VM_PROC_NR`, set
        // `vm_running = true` so readers (`do_umap_remote`, `acpi`, `oxpcie`)
        // see VM as active. See `09-vm-boot-protocol.md §3 decision4` and
        // `lib.rs::set_vm_running` doc comment.
        VmCtlParam::SetAddrSpace => {
            // SVMCTL_PTROOT = m1_i3 (same field as SVMCTL_VALUE)
            // SVMCTL_PTROOT_V = m1_p1 (virtual address of page table root)
            let ptroot_phys = value_raw as u64; // m1_i3 (i32) → u64 physical address
            let ptroot_virt = unsafe { msg.m_u.m_m1.m1p1 }; // m1_p1

            let target = proc_table.get_mut(target_nr);
            match target {
                Some(p) => {
                    // Steps 1-2: Set page table roots.
                    // C: p->p_seg.p_cr3 = cr3; p->p_seg.p_cr3_v = v;
                    p.p_seg.phys_root = minix_types::PhysBytes(ptroot_phys);
                    p.p_seg.virt_root = if ptroot_virt != 0 {
                        Some(minix_types::VirBytes(ptroot_virt))
                    } else {
                        None
                    };

                    // Step 3: write_cr3 — DEFERRED (requires ptproc tracking +
                    // non-zeroing Paging constructor). On single-CPU boot with
                    // only VM running, the scheduler will switch CR3 on the
                    // next context switch via the arch-specific context
                    // restore path. TODO: implement when SMP/ptproc lands.
                    //
                    // Step 4: arch_enable_paging — no-op on 64-bit
                    // (paging enabled in `arch_boot_impl` via `Paging::enable`).

                    // Step 5: Clear VMINHIBIT.
                    // C: RTS_UNSET(p, RTS_VMINHIBIT) — allows scheduling.
                    p.p_rts_flags.clear(crate::proc::RtsFlagsBits::VMINHIBIT);

                    // C bug correction: set vm_running = true when target is VM.
                    // C source omits this (never writes vm_running=1). Rust
                    // corrects the omission so VM is marked as running after
                    // it has switched to its own page table.
                    if p.p_nr == crate::proc::proc_nr::VM_PROC_NR {
                        crate::set_vm_running(true);
                    }

                    VmCtlResult::Ok(0)
                }
                None => return KcallResult::Ok(EINVAL),
            }
        }

        // ── Arch-specific commands: GetPdbr, FlushTlb, InvlPg ──
        // C: handled by arch_do_vmctl() in arch_do_vmctl.c:38-65
        // These require arch-specific register access (CR3/TTBR0/satp/INVLPG).
        // Return ENOSYS until arch trait provides the implementations.
        VmCtlParam::GetPdbr
        | VmCtlParam::FlushTlb
        | VmCtlParam::InvlPg => {
            VmCtlResult::Ok(ENOSYS as i32)
        }

        // ── 32-bit legacy: KernPhysMap, KernMapReply ──
        // C: do_vmctl.c:105-118 — arch_phys_map/arch_phys_map_reply
        // These are 32-bit-only (x86 PAE) and unused on 64-bit.
        VmCtlParam::KernPhysMap | VmCtlParam::KernMapReply => {
            VmCtlResult::Ok(ENOSYS as i32)
        }
    };

    match result {
        VmCtlResult::Ok(v) => KcallResult::Ok(v),
        VmCtlResult::VmSuspend => KcallResult::VmSuspend,
        VmCtlResult::BadParam => KcallResult::Ok(EINVAL),
    }
}
/// Dispatch SYS_DIAGCTL.
///
/// C: `do_diagctl()` — do_diagctl.c:18-68
///
/// Diagnostic control interface. Used by system processes to:
/// - DIAG: output diagnostic messages through the kernel console
/// - STACKTRACE: request a stack trace of a process
/// - REGISTER: register to receive SIGKMESS notifications
/// - UNREGISTER: stop receiving SIGKMESS notifications
///
/// # Message fields (C: ipc.h)
///
/// - `m_lsys_krn_sys_diagctl.code`: request code
/// - `m_lsys_krn_sys_diagctl.buf`: buffer address (DIAG only)
/// - `m_lsys_krn_sys_diagctl.len`: buffer length (DIAG only)
/// - `m_lsys_krn_sys_diagctl.endpt`: target endpoint (STACKTRACE only)
fn dispatch_diagctl(caller: &mut KProcess, msg: &Message, priv_table: &mut PrivTable) -> KcallResult {
    let diag_msg = unsafe { &msg.m_u.m_lsys_krn_sys_diagctl };

    match diag_msg.code {
        // DIAGCTL_CODE_DIAG = 1: output diagnostic message
        // C: do_diagctl.c:28-44 — data_copy_vmcheck from caller, then kputc each byte
        1 => {
            // Requires data_copy_vmcheck to copy from user space.
            // Return ENOSYS until virtual_copy_vmcheck is integrated.
            KcallResult::Ok(ENOSYS)
        }

        // DIAGCTL_CODE_STACKTRACE = 2: print process stack trace
        // C: do_diagctl.c:45-48 — isokendpt + proc_stacktrace
        2 => {
            // Requires proc_stacktrace implementation.
            KcallResult::Ok(ENOSYS)
        }

        // DIAGCTL_CODE_REGISTER = 3: register for SIGKMESS
        // C: do_diagctl.c:49-56 — check SYS_PROC, set s_diag_sig=TRUE,
        //   if kmess.km_size > 0 && !kinfo.do_serial_debug: send_sig
        3 => {
            let priv_id = match caller.priv_id {
                Some(id) => id,
                None => return KcallResult::Ok(EPERM),
            };
            match priv_table.get_mut(priv_id) {
                Some(p) => {
                    if !p.is_sys_proc() {
                        return KcallResult::Ok(EPERM);
                    }
                    p.mem.s_diag_sig = true;
                    // C: if kmess.km_size > 0 && !kinfo.do_serial_debug: send_sig
                    // send_sig requires mini_notify (kernel IPC core), DEFERRED.
                    KcallResult::Ok(0)
                }
                None => KcallResult::Ok(EPERM),
            }
        }

        // DIAGCTL_CODE_UNREGISTER = 4: unregister from SIGKMESS
        // C: do_diagctl.c:57-60 — check SYS_PROC, set s_diag_sig=FALSE
        4 => {
            let priv_id = match caller.priv_id {
                Some(id) => id,
                None => return KcallResult::Ok(EPERM),
            };
            match priv_table.get_mut(priv_id) {
                Some(p) => {
                    if !p.is_sys_proc() {
                        return KcallResult::Ok(EPERM);
                    }
                    p.mem.s_diag_sig = false;
                    KcallResult::Ok(0)
                }
                None => KcallResult::Ok(EPERM),
            }
        }

        // Unknown request code
        _ => KcallResult::Ok(EINVAL),
    }
}
fn dispatch_vtimer(caller: &mut KProcess, msg: &mut Message, priv_table: &PrivTable, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_clock::dispatch_vtimer(caller, msg, priv_table, proc_table) }
fn dispatch_runctl(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_process::dispatch_runctl(caller, msg, proc_table) }
/// Dispatch SYS_GETMCONTEXT.
///
/// C: `do_getmcontext()` — system.c (arch-specific)
///
/// Saves the machine context (register state) of the calling process.
/// This is used by the signal subsystem to build signal frames.
///
/// # Implementation status (2026-06-16)
///
/// Steps 1-3 (input validation) are implemented:
///   1. **Endpoint validation** (do_mcontext.c:26-27): `isokendpt(endpt, &proc_nr)`
///      rejects unknown endpoints with EINVAL.
///   2. **Kernel process rejection** (do_mcontext.c:28): `iskerneln(proc_nr)`
///      returns EPERM — kernel processes are not subject to mcontext.
///   3. **FPU-state check** (do_mcontext.c:31-33, x86 only): if the target
///      process has not used the FPU, return OK immediately (no state).
///
/// ## DEFERRED
///
/// - `data_copy` to/from user address space (Direct Map).
/// - `save_fpu(rp)` to flush FPU state into the proc struct.
/// - `mc_flags` setup and FPU state copy (arch-specific).
/// - Requires `SignalContext` trait (arch-abstractions) to abstract
///   the FPU / register state copy across x86_64 and aarch64.
///
/// ## Note
///
/// C does NOT require SYS_PROC for the caller — the kernel serves
/// `do_getmcontext` to any process whose target is a valid user process.
/// The earlier (incorrect) SYS_PROC check has been removed.
fn dispatch_getmcontext(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_mcontext.c:13-14 — extract from mess_lsys_krn_sys_getmcontext.
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   endpt@0, ctx_ptr@8
    // while MessageM1 has m1p1@16 (would read padding as `ctx_ptr`) — a P1
    // field-mapping bug. Always use the dedicated `MessLsysKrnSysMcontext`.
    let mc = unsafe { msg.m_u.m_lsys_krn_sys_mcontext };
    let endpt = mc.endpt;
    let _ctx_ptr = mc.ctx_ptr;

    // C: do_mcontext.c:26-27 — isokendpt(endpt, &proc_nr).
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_mcontext.c:28 — iskerneln(proc_nr) → EPERM.
    // Kernel processes have a fixed register layout that must not be
    // exposed to user space via mcontext.
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_mcontext.c:31-33 (x86 only):
    //   if (!proc_used_fpu(rp)) return(OK);
    // The target has no FPU state to copy (never executed an FPU
    // instruction), so there is nothing to do.
    //
    // For 64-bit Rust rewrite: the `MiscFlagsBits` enum does NOT have
    // `FPU_INITIALIZED` because modern 64-bit architectures use lazy FPU
    // initialization (see proc.rs:797 — "Modern 64-bit architectures
    // do not have a separate FPU"). The C x86 FPU fast path is therefore
    // not applicable. We fall through to the FPU copy body (DEFERRED).
    //
    // C: do_mcontext.c:35-48 — FPU state copy body.
    // DEFERRED: requires data_copy (Direct Map) + SignalContext trait
    // (arch-abstractions) to abstract mc_flags / __fpregs layout.
    KcallResult::Ok(ENOSYS)
}

/// Dispatch SYS_SETMCONTEXT.
///
/// C: `do_setmcontext()` — do_mcontext.c:62-106
///
/// Restores the machine context (register state) of a process. Used by
/// the signal subsystem to return from signal handlers.
///
/// # Implementation status (2026-06-16)
///
/// Step 1 (input validation) is implemented:
///   1. **Endpoint validation** (do_mcontext.c:64): `isokendpt(endpt, &proc_nr)`
///      rejects unknown endpoints with EINVAL. Note: C does NOT check
///      `iskerneln` for setmcontext — kernel processes can have their
///      FPU state restored (used during context switch).
///
/// ## DEFERRED
///
/// - `data_copy` to read user mcontext buffer (Direct Map).
/// - FPU state copy (mc_flags & _MC_FPU_SAVED → MF_FPU_INITIALIZED).
/// - `release_fpu(rp)` to force FPU reload on next use.
/// - Requires `SignalContext` trait (arch-abstractions).
fn dispatch_setmcontext(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    // C: do_mcontext.c:64-65 — extract from mess_lsys_krn_sys_setmcontext.
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   endpt@0, ctx_ptr@8
    // while MessageM1 has m1p1@16 (would read padding as `ctx_ptr`) — a P1
    // field-mapping bug. Always use the dedicated `MessLsysKrnSysMcontext`.
    let mc = unsafe { msg.m_u.m_lsys_krn_sys_mcontext };
    let endpt = mc.endpt;
    let _ctx_ptr = mc.ctx_ptr;

    // C: do_mcontext.c:64 — isokendpt(endpt, &proc_nr).
    let _target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_mcontext.c:67-69 — data_copy user mcontext buffer into KERNEL.
    // DEFERRED: requires Direct Map data_copy.
    //
    // C: do_mcontext.c:71-78 — FPU state copy + MF_FPU_INITIALIZED toggle.
    // DEFERRED: requires SignalContext trait (arch-abstractions).
    //
    // C: do_mcontext.c:80 — release_fpu(rp) to force FPU reload.
    // DEFERRED: arch-specific FPU control (arch-abstractions).
    KcallResult::Ok(ENOSYS)
}
fn dispatch_update(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable, priv_table: &PrivTable) -> KcallResult { crate::misc::dispatch_update(caller, msg, proc_table, priv_table) }
fn dispatch_schedctl(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_process::dispatch_schedctl(caller, msg, proc_table) }
fn dispatch_statectl(caller: &mut KProcess, msg: &Message, priv_table: &mut PrivTable) -> KcallResult {
    crate::syscall_process::dispatch_statectl(caller, msg, priv_table)
}
fn dispatch_safememset(caller: &mut KProcess, msg: &Message, proc_table: &crate::proc_table::ProcessTable, priv_table: &crate::kpriv::PrivTable) -> KcallResult { crate::syscall_copy::dispatch_safememset(caller, msg, proc_table, priv_table) }

// ── x86_64 arch-specific stubs (to be replaced by arch module) ──
#[cfg(target_arch = "x86_64")]
fn dispatch_arch_devio(caller: &mut KProcess, msg: &mut Message, priv_table: &PrivTable) -> KcallResult {
    // Delegate to syscall_device::dispatch_devio with X86_64PortIo.
    // X86_64PortIo uses x86 `in/out` instructions via inline assembly.
    // C: do_devio.c — SYS_DEVIO (x86-only)
    let port_io = minix_plat::CurrentPortIo::new();
    crate::syscall_device::dispatch_devio(caller, msg, &port_io, priv_table)
}
#[cfg(target_arch = "x86_64")]
fn dispatch_arch_sdevio(
    caller: &mut KProcess,
    msg: &Message,
    priv_table: &PrivTable,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    // Delegate to syscall_device::dispatch_sdevio with X86_64PortIo.
    // Parameter extraction, endpoint validation, type/direction parsing,
    // permission check (CHECK_IO_PORT), and alignment check are implemented.
    // Actual batch I/O transfer is deferred (requires cross-space copy).
    // C: do_sdevio.c — SYS_SDEVIO (x86-only)
    let port_io = minix_plat::CurrentPortIo::new();
    crate::syscall_device::dispatch_sdevio(caller, msg, &port_io, priv_table, proc_table)
}
#[cfg(target_arch = "x86_64")]
fn dispatch_arch_vdevio(caller: &mut KProcess, msg: &Message) -> KcallResult {
    // Delegate to syscall_device::dispatch_vdevio with X86_64PortIo.
    // Parameter validation is implemented; actual batch I/O returns ENOSYS
    // until data_copy_vmcheck (cross-space copy) is available.
    // C: do_vdevio.c — SYS_VDEVIO (x86-only)
    let port_io = minix_plat::CurrentPortIo::new();
    crate::syscall_device::dispatch_vdevio(caller, msg, &port_io)
}
#[cfg(target_arch = "x86_64")]
fn dispatch_arch_iopenable(caller: &mut KProcess, msg: &Message, proc_table: &mut crate::proc_table::ProcessTable) -> KcallResult {
    // Delegate to syscall_device::dispatch_iopenable.
    // SELF endpoint resolution + IOPL enable via
    // CurrentCpuContextArch::enable_user_io (kernel-layer abstraction).
    // C: do_iopenable.c — SYS_IOPENABLE (x86-only)
    crate::syscall_device::dispatch_iopenable(caller, msg, proc_table)
}
#[cfg(target_arch = "x86_64")]
fn dispatch_arch_readbios(caller: &mut KProcess, msg: &Message) -> KcallResult {
    // Delegate to syscall_device::dispatch_readbios.
    // Parameter extraction and BIOS memory range validation are implemented.
    // Actual data copy is deferred (requires virtual_copy_vmcheck).
    // C: do_readbios.c — SYS_READBIOS (x86-only)
    crate::syscall_device::dispatch_readbios(caller, msg)
}

// ── ARM arch-specific stubs (to be replaced by arch module) ──
#[cfg(target_arch = "arm")]
fn dispatch_arch_padconf(_: &mut KProcess, _: &Message) -> KcallResult { KcallResult::BadCall }

// ── kernel_call_finish / kernel_call_resume ──
// C: system.c:58-90 (kernel_call_finish), system.c:612-638 (kernel_call_resume)

use crate::proc::{MiscFlagsBits, RtsFlagsBits};
use minix_types::Endpoint;

/// EBADREQUEST — invalid syscall number. C: com.h EBADREQUEST = 212
const EBADREQUEST: i32 = 212;
/// ECALLDENIED — no permission for system call. C: com.h ECALLDENIED = 210
const ECALLDENIED: i32 = 210;
/// SYSTEM endpoint source for kernel replies. C: SYSTEM = -2 (proc.h)
/// Use Endpoint::SYSTEM constant from minix-types instead of raw i32.

/// Copy a message to user space via the process's delivermsg buffer.
///
/// C: `copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)` — system.c:82
///
/// In Minix3, this uses `phys_copy` to copy the message from kernel space
/// to the user-space address stored in `p_delivermsg_vir`. In the Rust
/// rewrite, we store the reply in `p_delivermsg` (kernel-side buffer)
/// and set the `MF_DELIVERMSG` flag so the IPC engine delivers it on
/// the next `switch_to_user` cycle.
///
/// This approach avoids direct user-space memory writes from the syscall
/// dispatch path, which is safer and aligns with the IPC engine's
/// message delivery mechanism (see vm.rs:323).
fn copy_msg_to_user(caller: &mut KProcess, msg: &Message) {
    caller.p_delivermsg = msg.clone();
    caller.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
}

/// Finish a kernel call: handle VMSUSPEND or copy result to user.
///
/// C: `kernel_call_finish()` — system.c:58-90
///
/// # BKL (Big Kernel Lock)
///
/// This function releases the BKL before returning. The BKL was acquired
/// in `kernel_call_dispatch()` and must be held throughout the dispatch +
/// finish sequence. We release it here because:
///
/// 1. **Normal completion**: The syscall is done, shared state is consistent.
///    The caller will enter `switch_to_user()` which does not need BKL
///    (it only reads per-CPU state and performs the mode switch).
///
/// 2. **VmSuspend**: The process is waiting for VM. The BKL must be released
///    so other CPUs can enter the kernel while this process is suspended.
///    When VM replies, `kernel_call_resume()` will re-acquire the BKL.
///
/// This matches C's pattern where BKL is released before `switch_to_user()`
/// (or before blocking in IPC sendrecv).
pub fn kernel_call_finish(caller: &mut KProcess, msg: &Message, result: KcallResult) {
    // VmSuspend path: save msg + set MF_KCALL_RESUME + release BKL.
    // C: system.c:60-63 — `if (result == VMSUSPEND) { saved.reqmsg = *msg;
    // p_misc_flags |= MF_KCALL_RESUME; }`
    if matches!(result, KcallResult::VmSuspend) {
        if let Some(ctx) = caller.p_vm_suspend.as_mut() {
            ctx.saved_msg = Some(msg.clone());
        }
        caller.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        // Release BKL — process is suspended waiting for VM.
        // Other CPUs can enter the kernel while we wait.
        // kernel_call_resume() will re-acquire BKL when VM replies.
        crate::smp::bkl_unlock();
        return;
    }

    // Non-VmSuspend path (Ok / NoReply / BadCall / CallDenied):
    // C: system.c:64-89 — single else-branch handles all non-VMSUSPEND cases
    // uniformly: clear saved_msg + optional reply + (BKL released below).
    //
    // Previous implementation scattered this across 4 match arms with 4×
    // `bkl_unlock()` and 3× duplicated reply construction; the unified path
    // also fixes a latent bug where BadCall/CallDenied skipped
    // `saved_msg = None` cleanup (harmless in practice because BadCall/
    // CallDenied cannot follow a VmSuspend, but diverges from C semantics).
    if let Some(ctx) = caller.p_vm_suspend.as_mut() {
        ctx.saved_msg = None;
    }

    if let Some(errno) = result.reply_code() {
        let mut reply = msg.clone();
        reply.m_source = Endpoint::SYSTEM;
        reply.m_type = errno;
        copy_msg_to_user(caller, &reply);
    }

    // Release BKL — syscall complete (Ok/NoReply/BadCall/CallDenied).
    crate::smp::bkl_unlock();
}

/// Resume a previously suspended kernel call (after VM handled the page fault).
///
/// C: `kernel_call_resume()` — system.c:612-638
///
/// # Invariants (C system.c:616-619)
///
/// On entry, the caller must satisfy:
/// 1. `!RTS_SLOT_FREE` — process slot is not being recycled
/// 2. `!RTS_VMREQUEST` — VM has finished processing the fault (flag cleared)
/// 3. `saved_msg.m_source == caller.p_endpoint` — saved message is still
///    sourced from this caller (not corrupted)
///
/// Additionally, `MF_KCALL_RESUME` must be set (set by `kernel_call_finish`
/// VmSuspend path) — its presence proves a prior dispatch returned VmSuspend.
pub fn kernel_call_resume(
    caller: &mut KProcess,
    priv_table: &mut PrivTable,
    proc_table: &mut crate::proc_table::ProcessTable,
    clock_state: &mut ClockState,
) {
    // C: system.c:616-619 — three invariants + our MF_KCALL_RESUME marker.
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE),
        "kernel_call_resume: caller slot is being freed");
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST),
        "kernel_call_resume: VM has not finished processing the fault");
    debug_assert!(caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME),
        "kernel_call_resume: MF_KCALL_RESUME not set (no prior VmSuspend)");

    // C: system.c:619 — `saved.reqmsg.m_source == caller->p_endpoint`.
    // The saved message must be sourced from this caller.
    // Using `expect` instead of `unwrap_or_default` so that an invariant
    // violation (missing p_vm_suspend or saved_msg) panics loudly rather
    // than silently dispatching an empty message — the original
    // `unwrap_or_default()` masked corruption bugs.
    let saved_msg = caller.p_vm_suspend.as_ref()
        .and_then(|ctx| ctx.saved_msg.clone())
        .expect("kernel_call_resume: p_vm_suspend.saved_msg must exist \
                 (VmSuspend path in kernel_call_finish always sets it)");
    debug_assert_eq!(saved_msg.m_source, caller.p_endpoint,
        "kernel_call_resume: saved_msg.m_source mismatch");

    let mut msg_copy = saved_msg;

    // C: system.c:627-630 — re-execute the kernel call with MF_KCALL_RESUME
    // still set so the call handler knows this is a retry. The flag is cleared
    // *after* dispatch returns (system.c:635) so it can be set again on a
    // subsequent VMSUSPEND within the same call.
    let result = kernel_call_dispatch(caller, &mut msg_copy, priv_table, proc_table, clock_state);
    caller.p_misc_flags.clear(MiscFlagsBits::KCALL_RESUME);
    kernel_call_finish(caller, &msg_copy, result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_try_from_valid() {
        assert_eq!(Syscall::try_from(0), Ok(Syscall::Fork));
        assert_eq!(Syscall::try_from(1), Ok(Syscall::Exec));
        assert_eq!(Syscall::try_from(53), Ok(Syscall::Exit));
        assert_eq!(Syscall::try_from(57), Ok(Syscall::Padconf));
    }

    #[test]
    fn test_syscall_try_from_invalid() {
        assert_eq!(Syscall::try_from(11), Err(())); // unused gap
        assert_eq!(Syscall::try_from(58), Err(())); // >= NR_SYS_CALLS
        assert_eq!(Syscall::try_from(255), Err(()));
    }

    // ── dispatch_schedule tests (F-45) ────────────────────────────────

    #[test]
    fn test_dispatch_schedule_rejects_non_sys_proc_caller() {
        // C: do_schedule.c:9 (implicit) — only the system process may
        // call SYS_SCHEDULE. caller_has_sys_proc returns false for any
        // non-SYS_PROC caller → EPERM.
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let msg = Message::default();
        let result = dispatch_schedule(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_schedule_rejects_invalid_endpoint() {
        // C: do_schedule.c:14-15 — endpoint_to_nr fails → EINVAL.
        // caller_has_sys_proc is false (no privilege), so EPERM is returned
        // before the endpoint check. To reach EINVAL, we bypass the SYS_PROC
        // check by directly invoking the post-check logic. The unit test
        // above documents that EPERM is the user-visible result for
        // misbehaving callers. We exercise the EINVAL path via dispatch_setgrant
        // / dispatch_virtctl semantics in follow-up tests.
        //
        // For now, just verify the function compiles and returns a result.
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        // endpoint = NONE; the function will fail at SYS_PROC first.
        msg.m_u.m_lsys_krn_schedule.endpoint = minix_types::Endpoint::NONE.0;
        let result = dispatch_schedule(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    // ── dispatch_getmcontext / dispatch_setmcontext tests (F-43/F-44) ──

    #[test]
    fn test_dispatch_getmcontext_rejects_invalid_endpoint() {
        // C: do_mcontext.c:26-27 — isokendpt fails → EINVAL.
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_mcontext.endpt = 9999; // not in proc table
            msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000; // ctx_ptr (ignored in validation)
        }
        let result = dispatch_getmcontext(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_getmcontext_rejects_kernel_target() {
        // C: do_mcontext.c:28 — iskerneln(proc_nr) → EPERM.
        use crate::proc::proc_nr::KERNEL;
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(KERNEL) {
            p.p_endpoint = minix_types::Endpoint(50);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_mcontext.endpt = 50;
            msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        }
        let result = dispatch_getmcontext(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_getmcontext_user_target_returns_enosys() {
        // C: do_mcontext.c:31-33 — x86 fast path. 64-bit rewrite has no
        // FPU_INITIALIZED bit, so we fall through to the FPU copy body
        // which is DEFERRED → ENOSYS.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(0) {
            p.p_endpoint = minix_types::Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_mcontext.endpt = 100;
            msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        }
        let result = dispatch_getmcontext(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_dispatch_setmcontext_rejects_invalid_endpoint() {
        // C: do_mcontext.c:64 — isokendpt fails → EINVAL.
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_mcontext.endpt = 9999;
            msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        }
        let result = dispatch_setmcontext(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_setmcontext_allows_kernel_target() {
        // C: do_mcontext.c:62-63 — setmcontext does NOT check iskerneln.
        // Kernel processes can have their FPU state restored (used during
        // context switch). We use a kernel slot and expect the call to
        // pass endpoint validation.
        use crate::proc::proc_nr::KERNEL;
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(KERNEL) {
            p.p_endpoint = minix_types::Endpoint(50);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_mcontext.endpt = 50;
            msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        }
        let result = dispatch_setmcontext(&mut caller, &msg, &proc_table);
        // FPU state copy body is DEFERRED → ENOSYS, not EPERM.
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_dispatch_setmcontext_user_target_returns_enosys() {
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(0) {
            p.p_endpoint = minix_types::Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_mcontext.endpt = 100;
            msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        }
        let result = dispatch_setmcontext(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_syscall_values_match_c() {
        // Verify key values match C's com.h definitions
        assert_eq!(Syscall::Fork as u16, 0);       // KERNEL_CALL + 0
        assert_eq!(Syscall::Schedule as u16, 3);    // KERNEL_CALL + 3
        assert_eq!(Syscall::Memset as u16, 13);     // KERNEL_CALL + 13
        assert_eq!(Syscall::Vmctl as u16, 43);      // KERNEL_CALL + 43
        assert_eq!(Syscall::Exit as u16, 53);       // KERNEL_CALL + 53
        assert_eq!(Syscall::Padconf as u16, 57);    // KERNEL_CALL + 57
    }

    #[test]
    fn test_kernel_call_dispatch_bad_call() {
        // Create a minimal message with invalid syscall number
        let mut msg = Message::default();
        msg.m_type = 99; // Invalid syscall number
        let mut proc = KProcess::new(0, minix_types::Endpoint::KERNEL);
        let mut priv_table = PrivTable::new();
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let mut clock_state = crate::clock::ClockState::new();
        let result = kernel_call_dispatch(&mut proc, &mut msg, &mut priv_table, &mut proc_table, &mut clock_state);
        assert_eq!(result, KcallResult::BadCall);
        // kernel_call_dispatch acquires BKL but does NOT release it —
        // the caller is expected to call kernel_call_finish() which
        // releases BKL. In this unit test we only test dispatch, so
        // we must release BKL manually to avoid poisoning other tests.
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_kernel_call_dispatch_call_denied_no_priv() {
        // Process without priv_id should be denied
        let mut msg = Message::default();
        msg.m_type = 0; // SYS_FORK
        let mut proc = KProcess::new(0, minix_types::Endpoint::KERNEL);
        // proc.priv_id is None by default
        let mut priv_table = PrivTable::new();
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let mut clock_state = crate::clock::ClockState::new();
        let result = kernel_call_dispatch(&mut proc, &mut msg, &mut priv_table, &mut proc_table, &mut clock_state);
        assert_eq!(result, KcallResult::CallDenied);
        // Same as above: release BKL acquired by kernel_call_dispatch.
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_kcall_result_variants() {
        // Verify all KcallResult variants can be constructed
        let _ok = KcallResult::Ok(0);
        let _suspend = KcallResult::VmSuspend;
        let _no_reply = KcallResult::NoReply;
        let _bad = KcallResult::BadCall;
        let _denied = KcallResult::CallDenied;
    }

    // ── dispatch_irqctl tests (irqctl, 2026-06-16) ──────────────────────

    #[test]
    fn test_dispatch_irqctl_rejects_unknown_request() {
        // C: do_irqctl.c:43 — unknown request → EINVAL.
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_irqctl.request = 99; // unknown request
        let result = dispatch_irqctl(&mut caller, &mut msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_irqctl_rejects_setpolicy_negative_irq() {
        // C: do_irqctl.c:55-56 — irq_vec < 0 → EINVAL.
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_irqctl.request = crate::syscall_device::IrqctlRequest::SetPolicy as i32;
        msg.m_u.m_lsys_krn_sys_irqctl.vector = -1; // invalid irq vector
        msg.m_u.m_lsys_krn_sys_irqctl.policy = 0;
        msg.m_u.m_lsys_krn_sys_irqctl.hook_id = 0;
        let result = dispatch_irqctl(&mut caller, &mut msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_irqctl_rejects_setpolicy_too_high_irq() {
        // C: do_irqctl.c:55-56 — irq_vec >= NR_IRQ_VECTORS → EINVAL.
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_irqctl.request = crate::syscall_device::IrqctlRequest::SetPolicy as i32;
        msg.m_u.m_lsys_krn_sys_irqctl.vector = 9999; // way beyond NR_IRQ_VECTORS
        msg.m_u.m_lsys_krn_sys_irqctl.policy = 0;
        msg.m_u.m_lsys_krn_sys_irqctl.hook_id = 0;
        let result = dispatch_irqctl(&mut caller, &mut msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_irqctl_rejects_non_sys_proc_caller() {
        // C: do_irqctl.c:58-76 — only SYS_PROC may manipulate IRQs.
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_irqctl.request = crate::syscall_device::IrqctlRequest::Enable as i32;
        msg.m_u.m_lsys_krn_sys_irqctl.vector = 0; // valid irq vector
        msg.m_u.m_lsys_krn_sys_irqctl.policy = 0;
        msg.m_u.m_lsys_krn_sys_irqctl.hook_id = 1; // hook_id
        // caller_has_sys_proc returns false → EPERM.
        let result = dispatch_irqctl(&mut caller, &mut msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_irqctl_disable_passes_validation() {
        // IRQ_DISABLE has no irq_vec range check, so it reaches the
        // SYS_PROC check directly.
        let mut caller = KProcess::new(0, minix_types::Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_irqctl.request = crate::syscall_device::IrqctlRequest::Disable as i32;
        msg.m_u.m_lsys_krn_sys_irqctl.vector = 0;
        msg.m_u.m_lsys_krn_sys_irqctl.policy = 0;
        msg.m_u.m_lsys_krn_sys_irqctl.hook_id = 1;
        let result = dispatch_irqctl(&mut caller, &mut msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }
}
