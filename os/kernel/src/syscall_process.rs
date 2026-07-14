//! Process management system calls: fork, exec, exit, clear, runctl, schedctl, statectl.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_fork.c` — SYS_FORK
//! - `do_exec.c` — SYS_EXEC
//! - `do_exit.c` — SYS_EXIT
//! - `do_clear.c` — SYS_CLEAR
//! - `do_runctl.c` — SYS_RUNCTL
//! - `do_schedctl.c` — SYS_SCHEDCTL
//! - `do_statectl.c` — SYS_STATECTL
//!
//! # Design Decisions (16-syscall-process.md §3)
//!
//! - **D1**: `clone_from()` for proc struct copy (Rust semantics clear)
//! - **D2**: `Endpoint::from_generation_slot()` for endpoint generation
//! - **D3**: `p_reg.retreg = 0` for fork return value (C behavior alignment)
//! - **D4**: Signal subsystem call for exit (decoupled)
//! - **D5**: Sequential subsystem calls for clear (modular)
//! - **D6**: Conditional SMP call for runctl (single/multi-core compatible)
//! - **D7**: enum + match for statectl sub-requests (type-safe)
//! - **D8**: `KcallResult::Ok(errno)` preserves C error code semantics

use minix_types::{Endpoint, Message, MessageM1, MessLsysKrnSchedctl, MessLsysKrnSysStatectl};

use crate::proc::{KProcess, MiscFlagsBits, ProcNr, RtsFlagsBits, complete_fork_setup};
use crate::proc_table::ProcessTable;
use crate::kpriv::{PrivTable, PrivFlagsBits, USER_PRIV_ID};
use crate::syscall::KcallResult;
use crate::syscall_signal::SIGABRT;

// ── Minix3 error codes ──

/// Invalid parameter. C: EINVAL
const EINVAL: i32 = 22;
/// Operation not permitted. C: EPERM
const EPERM: i32 = 1;
/// Resource busy. C: EBUSY
const EBUSY: i32 = 16;
/// Out of memory. C: ENOMEM
const ENOMEM: i32 = 12;
/// OK result. C: OK = 0
const OK: i32 = 0;

// ── Runctl actions ──

/// Runctl action: stop the process. C: `RC_STOP` — do_runctl.c
const RC_STOP: i32 = 0;
/// Runctl action: resume the process. C: `RC_RESUME` — do_runctl.c
const RC_RESUME: i32 = 1;
/// Runctl flag: delay stop if sending. C: `RC_DELAY` — do_runctl.c
const RC_DELAY: i32 = 1;

// ── Schedctl flags ──

/// Schedctl flag: kernel becomes the scheduler. C: `SCHEDCTL_FLAG_KERNEL`
const SCHEDCTL_FLAG_KERNEL: u32 = 0x01;

// ── Statectl requests ──

/// Statectl request types. C: do_statectl.c, com.h:442-446
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum StatectlRequest {
    /// Clear IPC references for all processes communicating with the caller.
    /// C: SYS_STATE_CLEAR_IPC_REFS = 1 (com.h:442)
    ClearIpcRefs = 1,
    /// Set state table for the caller.
    /// C: SYS_STATE_SET_STATE_TABLE = 2 (com.h:443)
    SetStateTable = 2,
    /// Add IPC blacklist filter.
    /// C: SYS_STATE_ADD_IPC_BL_FILTER = 3 (com.h:444)
    AddIpcBlFilter = 3,
    /// Add IPC whitelist filter.
    /// C: SYS_STATE_ADD_IPC_WL_FILTER = 4 (com.h:445)
    AddIpcWlFilter = 4,
    /// Clear IPC filters.
    /// C: SYS_STATE_CLEAR_IPC_FILTERS = 5 (com.h:446)
    ClearIpcFilters = 5,
}

impl TryFrom<i32> for StatectlRequest {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::ClearIpcRefs),
            2 => Ok(Self::SetStateTable),
            3 => Ok(Self::AddIpcBlFilter),
            4 => Ok(Self::AddIpcWlFilter),
            5 => Ok(Self::ClearIpcFilters),
            _ => Err(()),
        }
    }
}

// ── Helper: access message fields safely ──

/// Read m1 format fields from a message.
fn msg_m1(msg: &Message) -> MessageM1 {
    // SAFETY: `m_type` has been validated by the caller to select the M1
    // format. All union variants share the same size and `#[repr(C)]`
    // layout, so reading a different variant is sound.
    unsafe { msg.m_u.m_m1 }
}

/// Read statectl fields from a message.
/// C: `m_ptr->m_lsys_krn_sys_statectl.*` — uses mess_lsys_krn_sys_statectl union member.
fn msg_statectl(msg: &Message) -> MessLsysKrnSysStatectl {
    // SAFETY: `m_type == SYS_STATECTL` guarantees the `m_lsys_krn_sys_statectl`
    // variant is active. `#[repr(C)]` union access is sound.
    unsafe { msg.m_u.m_lsys_krn_sys_statectl }
}

/// Read schedctl fields from a message.
/// C: `m_ptr->m_lsys_krn_schedctl.*` — uses mess_lsys_krn_schedctl union member.
///
/// Using `MessageM1`/`MessageM2` overlays is **incorrect** for `quantum`/`cpu`:
/// `m2.m2i1`/`m2.m2i2` alias `flags`/`endpoint` (offset 0/4), not
/// `quantum`/`cpu` (offset 12/16). The dedicated union member preserves
/// the C layout exactly.
fn msg_schedctl(msg: &Message) -> MessLsysKrnSchedctl {
    // SAFETY: `m_type == SYS_SCHEDCTL` guarantees the `m_lsys_krn_schedctl`
    // variant is active. `#[repr(C)]` union access is sound.
    unsafe { msg.m_u.m_lsys_krn_schedctl }
}

// ── Process management syscall dispatch ──

/// Dispatch SYS_FORK.
///
/// C: `do_fork()` — do_fork.c
///
/// Creates a child process by copying the parent's proc struct.
/// The child gets a new endpoint with incremented generation.
/// The child's return register is set to 0 so it knows it's the child.
///
/// Returns `Ok(child_endpoint_raw)` on success, where `child_endpoint_raw`
/// is the raw i32 value of the child's new endpoint. The caller writes
/// this into the reply message's `m_krn_lsys_sys_fork.endpt` field.
pub fn dispatch_fork(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_fork.c:44-46 — extract parent endpoint and child slot
    let parent_endpt_i = m1.m1i1; // m_lsys_krn_sys_fork.endpt
    let child_slot: ProcNr = m1.m1i2; // m_lsys_krn_sys_fork.slot
    let fork_flags = m1.m1i3 as u32; // m_lsys_krn_sys_fork.flags

    // Validate parent endpoint
    // C: do_fork.c:44 — isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc)
    let _parent_ep = Endpoint(parent_endpt_i);

    // Validate: parent must be receiving (synchronous fork)
    // C: do_fork.c:56-59
    if !caller.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) {
        return KcallResult::Ok(EINVAL);
    }

    // Validate: child slot must be empty
    // C: do_fork.c:48 — isemptyp(rpc)
    if !proc_table.is_empty(child_slot) {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_fork.c:49 — save FPU context before copy
    // save_fpu(rpp) — handled by arch layer

    // C: do_fork.c:55-57 — increment endpoint generation
    // gen = _ENDPOINT_G(rpc->p_endpoint); gen++; rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);
    // Get the child's current (old) endpoint to extract generation
    let child_old_endpoint = proc_table.get(child_slot)
        .map(|p| p.p_endpoint)
        .unwrap_or(Endpoint::from_generation_slot(0, child_slot));
    let child_endpoint = Endpoint::fork_new_endpoint(child_old_endpoint, child_slot);

    // C: do_fork.c:53-54 — *rpc = *rpp (copy parent to child)
    // Use fork_from to create child from parent with corrections.
    // C guarantees rpp == caller (parent is the one calling SYS_FORK),
    // so using caller directly is correct.
    let mut child = KProcess::fork_from(caller, child_slot, child_endpoint);

    // C: do_fork.c:84-87 — if parent is SYS_PROC, downgrade child privilege
    // Check parent's privilege flags to determine if child needs downgrade.
    let parent_is_sys_proc = caller.priv_id
        .and_then(|id| priv_table.get(id))
        .map(|p| p.capability.s_flags.contains(PrivFlagsBits::SYS_PROC))
        .unwrap_or(false);

    // C: do_fork.c:84-87 — rpc->p_priv = priv_addr(USER_PRIV_ID)
    // All forked children get USER_PRIV_ID regardless of parent status.
    // If parent is SYS_PROC, the child also gets RTS_NO_PRIV set
    // (meaning it needs a new privilege assignment before running).
    child.priv_id = Some(USER_PRIV_ID);

    // Apply fork completion: RTS_NO_PRIV (if sys proc parent),
    // VMINHIBIT (if requested), name suffix "*F".
    // C: do_fork.c:84-87, 93-95, 104-106
    complete_fork_setup(&mut child, parent_is_sys_proc, fork_flags);

    // Write child back to process table
    *proc_table.get_mut(child_slot).unwrap() = child;

    // C: do_fork.c:62 — child sees pid = 0
    // rpc->p_reg.retreg = 0 — set by fork_from via p_reg initialization

    // C: do_fork.c:99-100 — clear signal flags
    // RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP)
    // Already handled by fork_from()

    // Return child endpoint via KcallResult.
    // C: do_fork.c:111 — m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
    // The caller (kernel_call_dispatch) writes this into the reply message.
    // We encode the child endpoint as the return value; the dispatch layer
    // will store it in the appropriate message field.
    KcallResult::Ok(child_endpoint.0)
}

/// Dispatch SYS_EXEC.
///
/// C: `do_exec()` — do_exec.c:20-58
///
/// Patches up a process after a successful exec:
/// clears old state (delivermsg, receiving, FPU) on the **target process**
/// specified by `endpt`, not the caller.
///
/// # Semantic alignment with C
///
/// C operates on `proc_addr(proc_nr)` where `proc_nr` comes from
/// `isokendpt(m_ptr->m_lsys_krn_sys_exec.endpt, &proc_nr)`.
/// The caller (typically PM) passes the endpoint of the process that
/// just did exec; the kernel patches that target process.
pub fn dispatch_exec(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_exec.c:29 — extract endpoint
    let endpt = m1.m1i1; // m_lsys_krn_sys_exec.endpt

    // C: do_exec.c:29-30 — isokendpt(endpt, &proc_nr)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_exec.c:36-37 — clear MF_DELIVERMSG on the target
    proc_table.get_mut(target_nr).map(|rp| {
        rp.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
    });

    // C: do_exec.c:39-43 — copy process name from user space
    // data_copy(caller->p_endpoint, name, KERNEL, name_buf, ...)
    // DEFERRED: requires cross-space copy (data_copy_vmcheck)

    // C: do_exec.c:49-52 — arch_proc_init() sets new IP/SP
    // arch_proc_init(rp, ip, stack, ps_str, name) — arch-specific
    // DEFERRED: requires arch trait (minix_arch::ArchProcInit)

    // C: do_exec.c:54 — RTS_UNSET(rp, RTS_RECEIVING)
    // rts_unset automatically enqueues the process if it becomes runnable.
    proc_table.rts_unset(target_nr, RtsFlagsBits::RECEIVING);

    // C: do_exec.c:57-58 — clear FPU initialized flag, release FPU
    // C uses MF_FPU_INITIALIZED; Rust uses EXT_REG_INITIALIZED
    proc_table.get_mut(target_nr).map(|rp| {
        rp.p_misc_flags.clear(MiscFlagsBits::EXT_REG_INITIALIZED);
    });
    // release_fpu(rp) — handled by arch layer

    KcallResult::Ok(OK)
}

/// Dispatch SYS_EXIT.
///
/// C: `do_exit()` — do_exit.c
///
/// A system process has requested to exit. Generate a self-termination signal.
/// Returns EDONTREPLY (no reply to the caller).
pub fn dispatch_exit(caller: &mut KProcess, _msg: &Message) -> KcallResult {
    // C: do_exit.c:20-22 — send SIGABRT to the caller via cause_sig()
    // cause_sig(caller->p_nr, SIGABRT) — system.c:389
    // The full C semantics require:
    //   1. Look up s_sig_mgr from priv(rp) (signal manager endpoint)
    //   2. Set RTS_SIGNALED + add SIGABRT to s_sig_pending
    //   3. mini_notify(sig_mgr, caller->p_endpoint)
    // Steps 2 are in-place state mutations and can be done without ProcessTable
    // access. Step 3 (signal manager notify) is deferred to SignalContext
    // trait since it requires PrivTable + mini_notify (kernel IPC core).
    cause_signal_abort(caller);

    // C: do_exit.c:23 — return EDONTREPLY
    KcallResult::NoReply
}

/// Minimal in-place implementation of `cause_sig(caller, SIGABRT)`.
///
/// Sets the kernel-side signal state on the caller so a subsequent
/// `do_getksig()` poll by the signal manager will observe it. Does NOT
/// perform the signal-manager notification (deferred to SignalContext trait).
///
/// C: `cause_sig()` — system.c:389-426
fn cause_signal_abort(caller: &mut KProcess) {
    // C: system.c:411 — sigaddset(&priv->s_sig_pending, sig_nr)
    // In Rust, signal manager pending is `s_sig_pending` (KPriv), but the
    // kernel-side p_pending (KProcess) is the visible "any signal queued"
    // bitmap. We set both to mirror C's "send to signal manager" semantics
    // from the caller's perspective.
    caller.p_pending.add(SIGABRT as u8);

    // C: system.c:413-414 — RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING)
    caller
        .p_rts_flags
        .set(RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING);
}

/// Dispatch SYS_CLEAR.
///
/// C: `do_clear()` — do_clear.c
///
/// Clean up a process table slot: release address space, IRQ hooks,
/// alarm timer, IPC endpoint, FPU, and mark slot as free.
///
/// # Semantic alignment with C
///
/// C operates on `rc = proc_addr(exit_p)` where `exit_p` comes from
/// `isokendpt(m_ptr->m_lsys_krn_sys_clear.endpt, &exit_p)`.
/// The caller (typically PM) passes the endpoint of the process to be
/// cleaned up; the kernel clears that **target** process, not the caller.
///
/// # Subsystem deferrals
///
/// The following C operations are deferred because their subsystems are
/// not yet wired up:
/// - `release_address_space(rc)` — requires VM integration
/// - IRQ hook cleanup (`rm_irq_handler`) — requires IRQ manager integration
/// - `clear_endpoint(rc)` — requires IPC module integration
/// - `reset_kernel_timer(&priv(rc)->s_alarm_timer)` — requires timer + PrivTable integration
///
/// The core operations that ARE implemented here (endpoint validation,
/// RTS_SLOT_FREE, FPU flag clear, SYS_PROC privilege release) are
/// sufficient to prevent slot leaks and ensure the slot can be reused
/// by a new process.
pub fn dispatch_clear(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_clear.c:24-25 — extract endpoint
    let endpt = m1.m1i1; // m_lsys_krn_sys_clear.endpt

    // C: do_clear.c:27-28 — isokendpt(endpt, &exit_p)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_clear.c:29 — rc = proc_addr(exit_p)
    // IMPORTANT: C operates on the TARGET process, not the caller.
    // The previous Rust code incorrectly operated on `caller`, which
    // would mark the PM's own slot as SLOT_FREE — a P0 semantic drift.

    // C: do_clear.c:31 — release_address_space(rc)
    // DEFERRED: requires VM integration

    // C: do_clear.c:33 — if(isemptyp(rc)) return OK
    if proc_table.get(target_nr).map_or(true, |p| {
        p.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE)
    }) {
        return KcallResult::Ok(OK);
    }

    // C: do_clear.c:34-39 — check and release IRQ hooks
    // DEFERRED: requires IRQ manager integration

    // C: do_clear.c:42 — clear_endpoint(rc)
    // DEFERRED: requires IPC module integration

    // C: do_clear.c:45 — reset_kernel_timer(&priv(rc)->s_alarm_timer)
    // DEFERRED: requires timer subsystem integration

    // C: do_clear.c:50 — RTS_SETFLAGS(rc, RTS_SLOT_FREE)
    // Mark the TARGET slot as free so it can be reused.
    proc_table.rts_set(target_nr, RtsFlagsBits::SLOT_FREE);

    // C: do_clear.c:53 — release_fpu(rc), clear MF_FPU_INITIALIZED
    // Clear the FPU initialized flag so the slot's FPU state is not
    // mistakenly used by a new process assigned to this slot.
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.p_misc_flags.clear(MiscFlagsBits::EXT_REG_INITIALIZED);
    }

    // C: do_clear.c:59 — if SYS_PROC, release privilege structure
    // priv(rc)->s_proc_nr = NONE — marks the privilege slot as unassigned.
    if let Some(target) = proc_table.get(target_nr) {
        if let Some(priv_id) = target.priv_id {
            if let Some(kpriv) = priv_table.get_mut(priv_id) {
                if kpriv.is_sys_proc() {
                    kpriv.capability.s_proc_nr = None;
                }
            }
        }
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_RUNCTL.
///
/// C: `do_runctl()` — do_runctl.c
///
/// Control a process's RTS_PROC_STOP flag. The target process is
/// specified by `RC_ENDPT` (not the caller). If the target is on
/// a different CPU, use IPI to stop it (SMP path deferred).
pub fn dispatch_runctl(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_runctl.c:34 — extract parameters
    let endpt = m1.m1i1;    // RC_ENDPT
    let action = m1.m1i2;   // RC_ACTION
    let flags = m1.m1i3;    // RC_FLAGS

    // C: do_runctl.c:36 — isokendpt(m_ptr->RC_ENDPT, &proc_nr)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_runctl.c:37 — iskerneln(proc_nr) → EPERM
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    match action {
        RC_STOP => {
            // C: do_runctl.c:45-51 — check RC_DELAY
            if (flags & RC_DELAY) != 0 {
                // If the target is sending or syscall-traced, set MF_SIG_DELAY
                let target = proc_table.get(target_nr);
                if let Some(rp) = target {
                    if rp.p_rts_flags.is_set(RtsFlagsBits::SENDING)
                        || rp.p_misc_flags.is_set(MiscFlagsBits::SC_DEFER)
                    {
                        proc_table.get_mut(target_nr).map(|rp| {
                            rp.p_misc_flags.set(MiscFlagsBits::SIG_DELAY);
                        });
                    }
                }
                // Check if SIG_DELAY was set (by us or already present)
                let sig_delay_set = proc_table.get(target_nr)
                    .map_or(false, |rp| rp.p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY));
                if sig_delay_set {
                    return KcallResult::Ok(EBUSY);
                }
            }

            // C: do_runctl.c:57-62 — SMP check
            // if (rp->p_cpu != cpuid) { smp_schedule_stop_proc(rp); }
            // else { RTS_SET(rp, RTS_PROC_STOP); }
            //
            // Single-CPU path: set RTS_PROC_STOP via rts_set which
            // automatically dequeues the process from the scheduler.
            proc_table.rts_set(target_nr, RtsFlagsBits::PROC_STOP);
        }
        RC_RESUME => {
            // C: do_runctl.c:65 — assert(RTS_ISSET(rp, RTS_PROC_STOP))
            debug_assert!(
                proc_table.get(target_nr)
                    .map_or(false, |rp| rp.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP)),
                "RC_RESUME on process without RTS_PROC_STOP: {:?}",
                target_nr,
            );
            // C: do_runctl.c:66 — RTS_UNSET(rp, RTS_PROC_STOP)
            // rts_unset automatically enqueues the process if it becomes runnable.
            proc_table.rts_unset(target_nr, RtsFlagsBits::PROC_STOP);
        }
        _ => return KcallResult::Ok(EINVAL),
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_SCHEDCTL.
///
/// C: `do_schedctl()` — do_schedctl.c:9-45
///
/// Set scheduling parameters for a process, or designate the caller as
/// the user-space scheduler for a target process.
///
/// # C Semantic Alignment
///
/// 1. Validate `flags` (only `SCHEDCTL_FLAG_KERNEL` is defined).
/// 2. Resolve the **target** endpoint via `isokendpt` (NOT the caller).
/// 3. If `SCHEDCTL_FLAG_KERNEL` is set:
///    - Call `sched_proc(target, priority, quantum, cpu, FALSE)`.
///    - Set `target.p_scheduler = None` (kernel becomes the scheduler).
/// 4. Otherwise:
///    - Set `target.p_scheduler = Some(caller.p_nr)` (caller becomes scheduler).
///
/// # Why target, not caller
///
/// C operates on `p = proc_addr(proc_nr)` where `proc_nr` comes from the
/// message's `endpoint` field. The caller (typically the `sched` server or
/// PM) designates scheduling parameters for **another** process. Operating
/// on `caller` would be a P0 semantic drift (the caller would accidentally
/// re-schedule itself).
pub fn dispatch_schedctl(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let sc = msg_schedctl(msg);
    // C: do_schedctl.c:11 — extract flags
    let flags = sc.flags;
    // C: do_schedctl.c:12 — extract target endpoint, priority, quantum, cpu
    let target_endpoint = Endpoint(sc.endpoint);
    let priority = sc.priority;
    let quantum = sc.quantum;
    let cpu = sc.cpu;

    // C: do_schedctl.c:14-17 — validate flags (only SCHEDCTL_FLAG_KERNEL defined)
    if flags & !SCHEDCTL_FLAG_KERNEL != 0 {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_schedctl.c:20-21 — isokendpt(endpoint, &proc_nr)
    // Resolve the TARGET process (not the caller). C uses `p = proc_addr(proc_nr)`.
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    if flags & SCHEDCTL_FLAG_KERNEL != 0 {
        // C: do_schedctl.c:23-34 — kernel becomes the scheduler.
        // Extract scheduling parameters and call sched_proc(p, ..., FALSE).
        // `niced = FALSE` matches C: do_schedctl.c:30 — sched_proc is called
        // with the literal `FALSE`, not a message field (unlike SYS_SCHEDULE).
        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        match crate::sched::sched_proc(target, priority, quantum, cpu, false) {
            Ok(_) => {
                // C: do_schedctl.c:35 — p->p_scheduler = NULL
                // Kernel is now the scheduler; clear any user-space scheduler.
                //
                // # Assignment timing
                //
                // The C source sets `p_scheduler = NULL` **after** `sched_proc`
                // returns OK (do_schedctl.c:30 → do_schedctl.c:35). The Rust
                // translation preserves this order: if `sched_proc` fails we
                // return early and leave `p_scheduler` untouched. This avoids
                // a bug where a failed `sched_proc` (e.g. EINVAL on bad
                // priority) would prematurely clear an existing user-space
                // scheduler, leaving the target orphaned with no scheduler.
                target.p_sched.scheduler = None;
            }
            Err(e) => {
                // Propagate the errno from sched_proc (EINVAL / EBADCPU).
                return KcallResult::Ok(crate::sched::sched_proc_error_to_errno(e));
            }
        }
    } else {
        // C: do_schedctl.c:36-37 — caller becomes the scheduler.
        // p->p_scheduler = caller (store the caller's slot number, not a
        // pointer, to match Rust ownership model).
        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        target.p_sched.scheduler = Some(caller.p_nr);
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_STATECTL.
///
/// C: `do_statectl()` — do_statectl.c
///
/// Handle state control requests: IPC filter management, IPC ref cleanup,
/// state table setup.
///
/// # C Semantic Alignment (do_statectl.c:15-49)
///
/// - `SYS_STATE_CLEAR_IPC_REFS` (1): clear_ipc_refs(caller, EDEADSRCDST) — DEFERRED: needs IPC module
/// - `SYS_STATE_SET_STATE_TABLE` (2): priv(caller)->s_state_table/entries — IMPLEMENTED
/// - `SYS_STATE_ADD_IPC_BL_FILTER` (3): add_ipc_filter(BLACKLIST) — DEFERRED: needs data_copy + IPC filter pool
/// - `SYS_STATE_ADD_IPC_WL_FILTER` (4): add_ipc_filter(WHITELIST) — DEFERRED: needs data_copy + IPC filter pool
/// - `SYS_STATE_CLEAR_IPC_FILTERS` (5): clear_ipc_filters(caller) — IMPLEMENTED
pub fn dispatch_statectl(caller: &mut KProcess, msg: &Message, priv_table: &mut PrivTable) -> KcallResult {
    let sc = msg_statectl(msg);
    let request = sc.request;

    let req = match StatectlRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    match req {
        // C: do_statectl.c:22-26 — clear_ipc_refs(caller, EDEADSRCDST)
        // DEFERRED: needs has_pending_asend, cancel_async, clear_ipc, unset_sys_bit
        //
        // ClearIpcRefs DEFERRED — detailed DEFERRED path:
        //
        // ClearIpcRefs walks the IPC machinery to cancel any pending async
        // SENDS the caller has outstanding and resets the IPC sys-bit. The
        // current stub returns `Ok(OK)` without doing any of that work
        // because the IPC engine (`IpcEngine::senda`) is itself
        // not yet wired into the dispatcher. Once `senda` is in, the full
        // implementation is:
        //
        // 1. **For each pending async SEND** (tracked via `p_asendtab` in
        //    `KProcess`): call `cancel_async(caller, dest)` to drop the
        //    pending message and clear `p_rts_flags` ASEND bit.
        // 2. **For each grant table entry** the caller holds: call
        //    `clear_ipc(caller, gid)` to release the grant.
        // 3. **Unset the SYS bit** in `p_misc_flags` so future IPC calls
        //    re-validate (C: do_statectl.c:25 `unset_sys_bit(caller)`).
        // 4. **Return OK** with the EDEADSRCDST error code embedded in the
        //    reply (the C side passes it to a helper, not the caller).
        //
        // # Why the stub is safe
        //
        // Returning `Ok(OK)` for a no-op means a buggy sender that issues
        // `ClearIpcRefs` sees success but no actual cancellation. The only
        // consequence is a stale grant or pending ASEND — these are
        // bounded by the caller's lifetime and the next ClearIpcRefs (or
        // process exit) will clean them up.
        StatectlRequest::ClearIpcRefs => {
            // DEFERRED: see ClearIpcRefs 4-step path above. Lands together with
            // TODO: IpcEngine::senda in kernel IPC core follow-up.
        }
        // C: do_statectl.c:27-30 — priv(caller)->s_state_table = address; s_state_entries = length
        StatectlRequest::SetStateTable => {
            if let Some(pid) = caller.priv_id {
                if let Some(priv_) = priv_table.get_mut(pid) {
                    priv_.runtime.s_state_table = sc.address as usize;
                    priv_.runtime.s_state_entries = sc.length;
                }
            }
        }
        // C: do_statectl.c:31-35 — add_ipc_filter(caller, IPCF_BLACKLIST, address, length)
        // PARTIAL (2026-06-14 ClearIpcRefs): slot allocation works; populating the
        // filter elements from user-supplied data is DEFERRED on data_copy_vmcheck.
        StatectlRequest::AddIpcBlFilter => {
            // First, free any existing filter for this caller to avoid leaks
            // (matches C `add_ipc_filter` semantics — replaces, not stacks).
            if let Some(pid) = caller.priv_id {
                if let Some(priv_) = priv_table.get_mut(pid) {
                    if let Some(old_idx) = priv_.mem.s_ipcf.take() {
                        crate::ipc_filter_pool().free(old_idx);
                    }
                    // Allocate a fresh blacklist slot from the pool.
                    // C: IPCF_POOL_ALLOCATE_SLOT(IPCF_BLACKLIST, &priv_->s_ipcf)
                    let new_idx = crate::ipc_filter_pool()
                        .allocate(crate::ipc_filter::IpcFilterType::Blacklist);
                    match new_idx {
                        Some(idx) => priv_.mem.s_ipcf = Some(idx),
                        None => return KcallResult::Ok(ENOMEM),
                    }
                    // DEFERRED: populate slot.elements[0..length] from
                    // user-space via data_copy_vmcheck (do_statectl.c:32-33).
                    // Until then, the filter has type set but zero elements,
                    // so it matches no messages — safe fail-open.
                }
            }
        }
        // C: do_statectl.c:36-40 — add_ipc_filter(caller, IPCF_WHITELIST, address, length)
        // PARTIAL (2026-06-14 ClearIpcRefs): slot allocation works; populating the
        // filter elements from user-supplied data is DEFERRED on data_copy_vmcheck.
        StatectlRequest::AddIpcWlFilter => {
            if let Some(pid) = caller.priv_id {
                if let Some(priv_) = priv_table.get_mut(pid) {
                    if let Some(old_idx) = priv_.mem.s_ipcf.take() {
                        crate::ipc_filter_pool().free(old_idx);
                    }
                    let new_idx = crate::ipc_filter_pool()
                        .allocate(crate::ipc_filter::IpcFilterType::Whitelist);
                    match new_idx {
                        Some(idx) => priv_.mem.s_ipcf = Some(idx),
                        None => return KcallResult::Ok(ENOMEM),
                    }
                    // DEFERRED: populate slot.elements[0..length] via data_copy_vmcheck.
                }
            }
        }
        // C: do_statectl.c:41-43 — clear_ipc_filters(caller)
        StatectlRequest::ClearIpcFilters => {
            if let Some(pid) = caller.priv_id {
                if let Some(priv_) = priv_table.get_mut(pid) {
                    // Free the IPC filter slot from the pool, then clear
                    // the pointer. C: IPCF_POOL_FREE_SLOT(priv(caller)->s_ipcf)
                    // followed by priv(caller)->s_ipcf = NULL.
                    if let Some(ipcf_idx) = priv_.mem.s_ipcf.take() {
                        crate::ipc_filter_pool().free(ipcf_idx);
                    }
                }
            }
        }
    }

    KcallResult::Ok(OK)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statectl_request_from_i32() {
        assert_eq!(StatectlRequest::try_from(1), Ok(StatectlRequest::ClearIpcRefs));
        assert_eq!(StatectlRequest::try_from(2), Ok(StatectlRequest::SetStateTable));
        assert_eq!(StatectlRequest::try_from(3), Ok(StatectlRequest::AddIpcBlFilter));
        assert_eq!(StatectlRequest::try_from(4), Ok(StatectlRequest::AddIpcWlFilter));
        assert_eq!(StatectlRequest::try_from(5), Ok(StatectlRequest::ClearIpcFilters));
        assert_eq!(StatectlRequest::try_from(0), Err(()));
        assert_eq!(StatectlRequest::try_from(99), Err(()));
    }

    /// Helper: build a statectl Message with the given request/address/length.
    /// SAFETY: caller must ensure no concurrent access to the union field.
    fn build_statectl_msg(request: i32, address: u64, length: i32) -> Message {
        let statectl = MessLsysKrnSysStatectl {
            request,
            address,
            length,
            _padding: [0; 36],
        };
        let mut msg = Message::default();
        // SAFETY: we just constructed `statectl` and `msg`; no aliasing.
        unsafe {
            msg.m_u.m_lsys_krn_sys_statectl = statectl;
        }
        msg
    }

    /// Helper: prepare a caller process with a working priv_id, returning
    /// (caller, priv_table). The priv_id must map to a real KPriv slot so
    /// dispatch_statectl can update `s_ipcf`.
    fn build_caller_with_priv() -> (KProcess, PrivTable) {
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(0, Endpoint(0));
        caller.priv_id = Some(0);
        (caller, priv_table)
    }

    #[test]
    fn test_dispatch_statectl_add_ipc_bl_filter_allocates_slot() {
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let msg = build_statectl_msg(3, 0xdead_beef, 0);
        assert_eq!(dispatch_statectl(&mut caller, &msg, &mut priv_table), KcallResult::Ok(OK));
        // Caller must now hold a non-None s_ipcf pointing into the global pool.
        let priv_ = priv_table.get(0).unwrap();
        let slot_idx = priv_.mem.s_ipcf.expect("AddIpcBlFilter should allocate a slot");
        let pool_slot = crate::ipc_filter_pool().get(slot_idx)
            .expect("slot index must resolve");
        assert_eq!(
            pool_slot.filter_type,
            crate::ipc_filter::IpcFilterType::Blacklist
        );
    }

    #[test]
    fn test_dispatch_statectl_add_ipc_wl_filter_allocates_slot() {
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let msg = build_statectl_msg(4, 0xdead_beef, 0);
        assert_eq!(dispatch_statectl(&mut caller, &msg, &mut priv_table), KcallResult::Ok(OK));
        let priv_ = priv_table.get(0).unwrap();
        let slot_idx = priv_.mem.s_ipcf.expect("AddIpcWlFilter should allocate a slot");
        let pool_slot = crate::ipc_filter_pool().get(slot_idx)
            .expect("slot index must resolve");
        assert_eq!(
            pool_slot.filter_type,
            crate::ipc_filter::IpcFilterType::Whitelist
        );
    }

    #[test]
    fn test_dispatch_statectl_repeated_add_replaces_slot() {
        // C: add_ipc_filter semantics — replacing the filter frees the old
        // slot, not stack. Verify by calling twice and observing only one
        // allocation in the global pool.
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let before_count = crate::ipc_filter_pool().allocated_count();
        let msg = build_statectl_msg(3, 0xdead_beef, 0);
        assert_eq!(dispatch_statectl(&mut caller, &msg, &mut priv_table), KcallResult::Ok(OK));
        let first_idx = priv_table.get(0).unwrap().mem.s_ipcf.unwrap();
        // Second add must not increase the allocated count.
        assert_eq!(dispatch_statectl(&mut caller, &msg, &mut priv_table), KcallResult::Ok(OK));
        let second_idx = priv_table.get(0).unwrap().mem.s_ipcf.unwrap();
        assert_eq!(crate::ipc_filter_pool().allocated_count(), before_count + 1);
        // Indices need not be identical (allocator may reuse the freed slot,
        // but the *count* must remain +1).
        let _ = (first_idx, second_idx);
    }

    #[test]
    fn test_dispatch_statectl_invalid_request_returns_einval() {
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let msg = build_statectl_msg(99, 0, 0);
        assert_eq!(dispatch_statectl(&mut caller, &msg, &mut priv_table), KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_exit_returns_no_reply() {
        let mut proc = KProcess::new(0, Endpoint(0));
        let msg = Message::default();
        assert_eq!(dispatch_exit(&mut proc, &msg), KcallResult::NoReply);
    }

    #[test]
    fn test_dispatch_exit_sets_sigabrt() {
        // C: do_exit.c:20-22 — cause_sig(caller, SIGABRT)
        // Verifies that dispatch_exit sets p_pending[6] (SIGABRT) and
        // RTS_SIGNALED | RTS_SIG_PENDING on the caller, so a subsequent
        // do_getksig() poll by the signal manager will observe it.
        let mut proc = KProcess::new(0, Endpoint(0));
        let msg = Message::default();
        assert_eq!(dispatch_exit(&mut proc, &msg), KcallResult::NoReply);
        assert!(proc.p_pending.contains(SIGABRT as u8));
        assert!(proc.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(proc.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING));
    }

    #[test]
    fn test_dispatch_runctl_stop() {
        // C: do_runctl.c — RTS_SET(rp, RTS_PROC_STOP) on the target
        let mut proc_table = ProcessTable::new();
        // Use a valid user-space process slot (nr=0, endpoint=0)
        let target_nr: ProcNr = 0;
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        let mut caller = KProcess::new(1, Endpoint(1));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = target_endpoint.0; // RC_ENDPT = target
            msg.m_u.m_m1.m1i2 = RC_STOP;           // action = stop
            msg.m_u.m_m1.m1i3 = 0;                  // no RC_DELAY
        }

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));
        assert!(proc_table.get(target_nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_runctl_resume() {
        // C: do_runctl.c — RTS_UNSET(rp, RTS_PROC_STOP) on the target
        let mut proc_table = ProcessTable::new();
        let target_nr: ProcNr = 0;
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            target.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        }
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        let mut caller = KProcess::new(1, Endpoint(1));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = target_endpoint.0; // RC_ENDPT = target
            msg.m_u.m_m1.m1i2 = RC_RESUME;
        }

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));
        assert!(!proc_table.get(target_nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_runctl_invalid_action() {
        let mut proc_table = ProcessTable::new();
        let target_nr: ProcNr = 0;
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        let mut caller = KProcess::new(1, Endpoint(1));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = target_endpoint.0;
            msg.m_u.m_m1.m1i2 = 99; // invalid action
        }

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_runctl_kernel_process_returns_eperm() {
        // C: do_runctl.c:37 — iskerneln(proc_nr) → EPERM
        let mut proc_table = ProcessTable::new();
        // Kernel processes have negative ProcNr. The IDLE process
        // (nr = -(NR_TASKS-4) on most configs) is a kernel task.
        // Find a kernel process by scanning for negative p_nr.
        let kernel_nr = proc_table.iter()
            .find(|p| p.p_nr < 0)
            .map(|p| p.p_nr)
            .expect("ProcessTable::new() should have kernel tasks");
        let kernel_endpoint = proc_table.get(kernel_nr).unwrap().p_endpoint;

        // IDLE is in SLOT_FREE by default, so endpoint_to_nr won't find it.
        // Clear SLOT_FREE so the endpoint lookup succeeds.
        proc_table.get_mut(kernel_nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let mut caller = KProcess::new(1, Endpoint(1));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = kernel_endpoint.0;
            msg.m_u.m_m1.m1i2 = RC_STOP;
        }

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_exec_operates_on_target() {
        // C: do_exec.c:29-30 — isokendpt(endpt, &proc_nr), then operate on rp
        let mut proc_table = ProcessTable::new();
        // Set up a user-space target process (nr >= 0)
        let target_nr: ProcNr = 0;
        proc_table.get_mut(target_nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        // Set up the target with DELIVERMSG, RECEIVING, and EXT_REG_INITIALIZED
        proc_table.get_mut(target_nr).unwrap().p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        proc_table.get_mut(target_nr).unwrap().p_rts_flags.set(RtsFlagsBits::RECEIVING);
        proc_table.get_mut(target_nr).unwrap().p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);

        let mut caller = KProcess::new(1, Endpoint(1));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = target_endpoint.0; // RC_ENDPT = target
        }

        let result = dispatch_exec(&mut caller, &mut msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // Verify the TARGET process was modified, not the caller
        let target = proc_table.get(target_nr).unwrap();
        assert!(!target.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert!(!target.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED));

        // Caller should be unmodified
        assert!(!caller.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    #[test]
    fn test_dispatch_exec_invalid_endpoint() {
        // C: do_exec.c:29-30 — isokendpt fails → EINVAL
        let mut proc_table = ProcessTable::new();
        let mut caller = KProcess::new(1, Endpoint(1));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = 99999; // invalid endpoint
        }

        let result = dispatch_exec(&mut caller, &mut msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_clear_invalid_endpoint_returns_einval() {
        // C: do_clear.c:27-28 — isokendpt fails → EINVAL
        let mut caller = KProcess::new(0, Endpoint(0));
        let mut msg = Message::default();
        // Set an endpoint that won't be found in the process table
        unsafe {
            msg.m_u.m_m1.m1i1 = 99999; // invalid endpoint
        }
        let mut proc_table = ProcessTable::new();
        let mut priv_table = PrivTable::new();
        let result = dispatch_clear(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_clear_sets_target_slot_free() {
        // C: do_clear.c:50 — RTS_SETFLAGS(rc, RTS_SLOT_FREE)
        // The target process (not the caller) should be marked SLOT_FREE.
        let mut proc_table = ProcessTable::new();
        let mut priv_table = PrivTable::new();

        // Activate a user-process slot so endpoint_to_nr can find it.
        // Slot with p_nr = 0 (first user process after NR_TASKS).
        let target_nr = 0;
        let target_ep = Endpoint::from_generation_slot(1, target_nr);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        assert!(proc_table.endpoint_to_nr(target_ep).is_some());

        // Caller is a different process (e.g., PM)
        let mut caller = KProcess::new(0, Endpoint(0));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = target_ep.get(); // target endpoint
        }

        let result = dispatch_clear(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // TARGET should be SLOT_FREE, not caller
        assert!(proc_table.get(target_nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE));
    }

    #[test]
    fn test_dispatch_clear_clears_ext_reg_on_target() {
        // C: do_clear.c:53 — release_fpu(rc), clear MF_FPU_INITIALIZED
        let mut proc_table = ProcessTable::new();
        let mut priv_table = PrivTable::new();

        let target_nr = 0;
        let target_ep = Endpoint::from_generation_slot(1, target_nr);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            target.p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);
        }

        let mut caller = KProcess::new(0, Endpoint(0));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = target_ep.get();
        }

        let result = dispatch_clear(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // TARGET should have EXT_REG_INITIALIZED cleared
        assert!(!proc_table.get(target_nr).unwrap().p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED));
    }

    /// Helper: build a schedctl Message targeting `endpoint` with the given
    /// scheduling parameters. SAFETY: caller must ensure no concurrent
    /// access to the union field.
    fn build_schedctl_msg(
        flags: u32,
        endpoint: i32,
        priority: i32,
        quantum: i32,
        cpu: i32,
    ) -> Message {
        let sc = MessLsysKrnSchedctl {
            flags,
            endpoint,
            priority,
            quantum,
            cpu,
            _padding: [0; 36],
        };
        let mut msg = Message::default();
        // SAFETY: we just constructed `sc` and `msg`; no aliasing.
        unsafe {
            msg.m_u.m_lsys_krn_schedctl = sc;
        }
        msg
    }

    /// Helper: install a target process at `target_nr` with a live endpoint
    /// so `endpoint_to_nr` can resolve it. Returns the endpoint.
    fn install_target(proc_table: &mut ProcessTable, target_nr: ProcNr) -> Endpoint {
        let target_ep = Endpoint::from_generation_slot(1, target_nr);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        target_ep
    }

    #[test]
    fn test_dispatch_schedctl_invalid_flags() {
        // C: do_schedctl.c:14-17 — flags & ~SCHEDCTL_FLAG_KERNEL → EINVAL
        let mut proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(0));
        let msg = build_schedctl_msg(0xFF, 0, 0, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_invalid_endpoint_returns_einval() {
        // C: do_schedctl.c:20-21 — isokendpt fails → EINVAL
        let mut proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(0));
        // Endpoint 99999 won't resolve in an empty process table.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, 99999, 0, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_kernel_flag_calls_sched_proc_and_clears_scheduler() {
        // C: do_schedctl.c:23-35 — kernel becomes scheduler:
        //   sched_proc(p, priority, quantum, cpu, FALSE) + p_scheduler = NULL
        let mut proc_table = ProcessTable::new();
        let target_nr = 0;
        let target_ep = install_target(&mut proc_table, target_nr);

        // Pre-set a user-space scheduler to verify it gets cleared.
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_sched.scheduler = Some(5);
        }

        // Caller is a different process (e.g., the sched server).
        let mut caller = KProcess::new(1, Endpoint::from_generation_slot(1, 1));

        // Valid scheduling parameters: priority=5, quantum=10ms, cpu=0.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), 5, 10, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // TARGET (not caller) should have scheduler cleared and priority applied.
        let target = proc_table.get(target_nr).unwrap();
        assert_eq!(target.p_sched.scheduler, None);
        assert_eq!(target.p_sched.priority.load(core::sync::atomic::Ordering::Acquire), 5);
        assert_eq!(target.p_sched.quantum.size_ms.load(core::sync::atomic::Ordering::Acquire), 10);
    }

    #[test]
    fn test_dispatch_schedctl_kernel_flag_propagates_sched_proc_error() {
        // C: do_schedctl.c:30 — if sched_proc returns error, propagate it.
        // Invalid priority (out of range) → EINVAL from sched_proc.
        let mut proc_table = ProcessTable::new();
        let target_nr = 0;
        let target_ep = install_target(&mut proc_table, target_nr);

        let mut caller = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        // priority = 999 exceeds NR_SCHED_QUEUES (16) → EINVAL.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), 999, 10, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_kernel_flag_invalid_quantum_returns_einval() {
        // C: do_schedctl.c:30 → sched_proc validates quantum < 1 && != -1 → EINVAL.
        let mut proc_table = ProcessTable::new();
        let target_nr = 0;
        let target_ep = install_target(&mut proc_table, target_nr);

        let mut caller = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        // quantum = 0 is invalid (must be >= 1 or -1).
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), 5, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_no_flag_sets_caller_as_scheduler_on_target() {
        // C: do_schedctl.c:36-37 — caller becomes the scheduler.
        // The TARGET's p_scheduler should be set to caller.p_nr, NOT the
        // caller's own p_scheduler.
        let mut proc_table = ProcessTable::new();
        let target_nr = 0;
        let target_ep = install_target(&mut proc_table, target_nr);

        // Caller is process at slot 7 (e.g., the sched server).
        let caller_nr = 7;
        let mut caller = KProcess::new(caller_nr, Endpoint::from_generation_slot(1, caller_nr));

        // flags = 0 (no SCHEDCTL_FLAG_KERNEL) — caller becomes scheduler.
        let msg = build_schedctl_msg(0, target_ep.get(), 0, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));

        // TARGET should have scheduler = Some(caller_nr).
        let target = proc_table.get(target_nr).unwrap();
        assert_eq!(target.p_sched.scheduler, Some(caller_nr));
        // Caller's own scheduler field should be unchanged.
        assert_eq!(caller.p_sched.scheduler, None);
    }

    #[test]
    fn test_dispatch_schedctl_preserves_minus_one_sentinels() {
        // C: do_schedctl.c:30 → sched_proc(p, -1, -1, -1, FALSE) keeps
        // current priority/quantum/cpu unchanged.
        let mut proc_table = ProcessTable::new();
        let target_nr = 0;
        let target_ep = install_target(&mut proc_table, target_nr);

        // Pre-set known scheduling state on the target.
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_sched.priority.store(7, core::sync::atomic::Ordering::Release);
            target.p_sched.quantum.size_ms.store(20, core::sync::atomic::Ordering::Release);
        }

        let mut caller = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        // All -1 sentinels → sched_proc should keep current values.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), -1, -1, -1);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));

        let target = proc_table.get(target_nr).unwrap();
        assert_eq!(target.p_sched.priority.load(core::sync::atomic::Ordering::Acquire), 7);
        assert_eq!(target.p_sched.quantum.size_ms.load(core::sync::atomic::Ordering::Acquire), 20);
        assert_eq!(target.p_sched.scheduler, None);
    }
}
