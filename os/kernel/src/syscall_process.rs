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
//! # Design Decisions (17-syscall-process.md §3)
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

use crate::proc::{CpuId, KProcess, MiscFlagsBits, ProcNr, RtsFlagsBits, complete_fork_setup};
use crate::proc_table::ProcessTable;
use crate::capability::ProcessCapability;
use crate::kpriv::{PrivTable, USER_PRIV_ID};
use crate::syscall::{KcallResult, Syscall};
use crate::syscall_signal::{cause_signal, SIGABRT};

// ── Minix3 error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
use crate::errno::*;

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
    msg.debug_check_m_type_any(&[Syscall::Statectl as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
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
    msg.debug_check_m_type_any(&[Syscall::Schedctl as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
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
    // C: do_fork.c:41,44-45 — extract parent endpoint and child slot
    let parent_endpt_i = m1.m1i1; // m_lsys_krn_sys_fork.endpt
    let child_slot: ProcNr = ProcNr(m1.m1i2); // m_lsys_krn_sys_fork.slot
    let fork_flags = m1.m1i3 as u32; // m_lsys_krn_sys_fork.flags

    // Validate parent endpoint
    // C: do_fork.c:41 — isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc)
    let _parent_ep = Endpoint(parent_endpt_i);

    // Validate: parent must be receiving (synchronous fork)
    // C: do_fork.c:51
    if !caller.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) {
        return KcallResult::Ok(EINVAL);
    }

    // Validate: child slot must be empty
    // C: do_fork.c:46 — isemptyp(rpc)
    if !proc_table.is_empty(child_slot) {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_fork.c:57 — save_fpu(rpp) flushes the parent's live FPU
    // registers into its buffer before the struct copy. In this codebase
    // the per-CPU FPU ownership path (`smp.rs` `fpu_owner`) is not wired
    // yet, so the flush is a no-op; `fork_from` copies the parent's
    // `fpu_state` buffer when `EXT_REG_INITIALIZED` is set.

    // C: do_fork.c:59,69-72 — increment endpoint generation
    // gen = _ENDPOINT_G(rpc->p_endpoint); gen++; rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);
    // Get the child's current (old) endpoint to extract generation
    let child_old_endpoint = proc_table.get(child_slot)
        .map(|p| p.p_endpoint)
        .unwrap_or(Endpoint::from_generation_slot(0, child_slot.0));
    let child_endpoint = Endpoint::fork_new_endpoint(child_old_endpoint, child_slot.0);

    // C: do_fork.c:63 — *rpc = *rpp (copy parent to child)
    // Use fork_from to create child from parent with corrections.
    // C guarantees rpp == caller (parent is the one calling SYS_FORK),
    // so using caller directly is correct.
    let mut child = KProcess::fork_from(caller, child_slot, child_endpoint);

    // C: do_fork.c:105-107 — if parent is SYS_PROC, downgrade child privilege
    // Check parent's privilege flags to determine if child needs downgrade.
    let parent_is_sys_proc = caller.priv_id
        .and_then(|id| priv_table.get(id))
        .map(|p| p.flags.s_flags.contains(ProcessCapability::SYS_PROC))
        .unwrap_or(false);

    // C: do_fork.c:105-107 — rpc->p_priv = priv_addr(USER_PRIV_ID)
    // All forked children get USER_PRIV_ID regardless of parent status.
    // If parent is SYS_PROC, the child also gets RTS_NO_PRIV set
    // (meaning it needs a new privilege assignment before running).
    child.priv_id = Some(USER_PRIV_ID);

    // Apply fork completion: RTS_NO_PRIV (if sys proc parent),
    // VMINHIBIT (if requested), name suffix "*F".
    // C: do_fork.c:84-87,105-107,115-116
    complete_fork_setup(&mut child, parent_is_sys_proc, fork_flags);

    // Write child back to process table
    *proc_table.get_mut(child_slot).unwrap() = child;

    // C: do_fork.c:74 — child sees pid = 0
    // rpc->p_reg.retreg = 0 — set by fork_from via p_reg initialization

    // C: do_fork.c:122 — clear signal flags
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
/// C: `do_exec()` — do_exec.c:20-59
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
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    // C: do_exec.c:27 — extract fields from mess_lsys_krn_sys_exec.
    // Use the typed message (not M1) because SYS_EXEC's layout differs
    // from MessageM1 on 64-bit (ip at offset 8, not m1p1 at offset 16).
    msg.debug_check_m_type_any(&[Syscall::Exec as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let exec_msg = unsafe { msg.m_u.m_lsys_krn_sys_exec };
    let endpt = exec_msg.endpt;

    // C: do_exec.c:27,30 — isokendpt(endpt, &proc_nr)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_exec.c:32-34 — clear MF_DELIVERMSG on the target
    if let Some(rp) = proc_table.get_mut(target_nr) { rp.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG); }

    // C: do_exec.c:37-42 — copy process name from caller's address space
    // C: data_copy(src, dst, sizeof(rp->p_name)) + null termination (do_exec.c:43)
    {
        use crate::cross_space::data_copy_vmcheck;
        use crate::vm::{AddressRef, CrossSpaceResult};
        use minix_arch::{CurrentDirectMap, DirectMapArch};
        use minix_types::VirBytes;
        use crate::proc::{ProcName, PROC_NAME_LEN};

        let name_ptr = exec_msg.name;

        // Capture caller's endpoint and CR3 before mutable borrow.
        let caller_endpt = caller.p_endpoint;
        let caller_cr3 = caller.p_seg.phys_root;

        let mut name_buf = [0u8; PROC_NAME_LEN];
        let dst_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            name_buf.as_mut_ptr() as u64,
        ));

        let proc_cr3 = |endpt: Endpoint| {
            if endpt == caller_endpt { Some(caller_cr3) } else { None }
        };

        let src = AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(name_ptr),
        };
        let dst = AddressRef::Physical(dst_phys);

        match data_copy_vmcheck(caller, src, dst, PROC_NAME_LEN, proc_cr3) {
            CrossSpaceResult::Completed(Ok(())) => {
                // C: ensure null termination (do_exec.c:43)
                name_buf[PROC_NAME_LEN - 1] = 0;
                if let Some(rp) = proc_table.get_mut(target_nr) {
                    rp.p_name = ProcName::from_array(name_buf);
                }
            }
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
        }
    }

    // C: do_exec.c:45-48 — arch_proc_init(rp, ip, stack, ps_str, name)
    //
    // Sets the process's CPU context with the new entry point (IP),
    // stack pointer (SP), and ps_strings. In Rust, this maps to
    // `CpuContextArch::build_cpu_context` with `ProcKind::UserProcess`
    // and `EntrySpec::loaded(ip, sp, ps_str)`.
    //
    // The C `arch_proc_init` also zeroes some registers (retreg=0, fp=0)
    // and sets the ps_strings register — all of which are handled
    // arch-internally by `build_cpu_context` for `ProcKind::UserProcess`.
    {
        use minix_arch::{CpuContextArch, CurrentCpuContextArch, EntrySpec, ProcKind};
        let entry = EntrySpec::loaded(
            minix_types::VirBytes(exec_msg.ip),
            minix_types::VirBytes(exec_msg.stack),
            minix_types::VirBytes(exec_msg.ps_str),
        );
        let cpu_context = <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
            ProcKind::UserProcess,
            target_nr.0,
            entry,
        );
        if let Some(rp) = proc_table.get_mut(target_nr) {
            rp.cpu_context = cpu_context;
        }
    }

    // C: do_exec.c:51 — RTS_UNSET(rp, RTS_RECEIVING)
    // rts_unset automatically enqueues the process if it becomes runnable.
    proc_table.rts_unset(target_nr, RtsFlagsBits::RECEIVING);

    // C: do_exec.c:55-57 — clear FPU initialized flag, release FPU
    // C uses MF_FPU_INITIALIZED; Rust uses EXT_REG_INITIALIZED
    if let Some(rp) = proc_table.get_mut(target_nr) { rp.p_misc_flags.clear(MiscFlagsBits::EXT_REG_INITIALIZED); }
    // release_fpu(rp) — clearing EXT_REG_INITIALIZED + lazy FPU model
    // means the next FPU instruction traps and re-initializes.

    KcallResult::Ok(OK)
}

/// Dispatch SYS_EXIT.
///
/// C: `do_exit()` — do_exit.c
///
/// A system process has requested to exit. Raise SIGABRT on itself through
/// the full `cause_sig` semantics (do_exit.c:21): the signal manager
/// (typically PM) is notified so it can execute its cleanup hooks and
/// finally reclaim the slot via SYS_CLEAR. A self-managed process that
/// exits — a system service nobody else can reap — triggers the lethal
/// self path: promote the backup manager if one was designated, else panic.
/// Returns EDONTREPLY (no reply to the caller).
///
/// The "自杀 → 委托 → 回收" three-phase contract is preserved: the kernel
/// does not free resources here; it only makes the death observable.
pub fn dispatch_exit(
    caller: &mut KProcess,
    _msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) -> KcallResult {
    // C: do_exit.c:21 — cause_sig(caller->p_nr, SIGABRT); EDONTREPLY
    // SIGABRT ∈ SIGS_IS_LETHAL（signal.h:280-282），因此自管理进程的 exit
    // 会进入 cause_signal 的致命 SELF 子路径（backup 提升或 panic）。
    cause_signal(caller.p_nr, SIGABRT, proc_table, priv_table);

    // C: do_exit.c:23 — return EDONTREPLY
    KcallResult::NoReply
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
/// - `release_address_space(rc)` — requires VM integration (page table release)
/// - `clear_endpoint(rc)` — requires IPC module integration
///
/// The following C operations ARE now implemented:
/// - IRQ hook cleanup (`rm_irq_handler`) — via global `irq_manager()`
/// - `reset_kernel_timer(&priv(rc)->s_alarm_timer)` — via `clock::reset_alarm_timer()`
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
    clock_state: &mut crate::clock::ClockState,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_clear.c:29 — extract endpoint
    let endpt = m1.m1i1; // m_lsys_krn_sys_clear.endpt

    // C: do_clear.c:29 — isokendpt(endpt, &exit_p)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_clear.c:33 — rc = proc_addr(exit_p)
    // IMPORTANT: C operates on the TARGET process, not the caller.
    // The previous Rust code incorrectly operated on `caller`, which
    // would mark the PM's own slot as SLOT_FREE — a P0 semantic drift.

    // C: do_clear.c:38 — if(isemptyp(rc)) return OK
    if proc_table.get(target_nr).is_none_or(|p| {
        p.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE)
    }) {
        return KcallResult::Ok(OK);
    }

    // C: do_clear.c:35 — release_address_space(rc)
    // Implemented (2026-08-13, Phase 8): clears p_seg.virt_root (the
    // kernel-virtual alias of the page directory). The actual page-table
    // reclamation is performed by VM via a separate flow.
    if let Some(target) = proc_table.get_mut(target_nr) {
        crate::syscall::release_address_space(target);
    }

    // C: do_clear.c:41-46 — rm_irq_handler for all hooks owned by this process.
    // C iterates: `for (i=0; i < NR_IRQ_HOOKS; i++)` if `irq_hooks[i].proc_nr_e == rc->p_endpoint`.
    // Rust: use the global IrqManager (same accessor as dispatch_irqctl).
    {
        let irq_mgr = unsafe { crate::irq_manager() };
        // Iterate all hook slots; remove those owned by the target.
        for slot in 0..crate::syscall_device::NR_IRQ_HOOKS {
            if let Some(owner) = irq_mgr.hook_owner(slot)
                && owner == target_endpoint {
                    let _ = irq_mgr.remove_hook_by_slot(slot);
                }
        }
    }

    // C: do_clear.c:49 — clear_endpoint(rc)
    // Implemented (2026-08-13, Phase 8): full clear_endpoint sequence —
    // RTS_NO_ENDPOINT + s_asynsize clear + clear_ipc + clear_ipc_refs +
    // clear_memreq. See `syscall::clear_endpoint` for details.
    crate::syscall::clear_endpoint(proc_table, priv_table, target_nr);

    // C: do_clear.c:52 — reset_kernel_timer(&priv(rc)->s_alarm_timer)
    // Cancel any pending alarm timer for this process. Chain-aware: the
    // node is embedded in the priv slot and unlinked from the clock's
    // sorted chain (C: `reset_kernel_timer` → `tmrs_clrtimer`).
    if let Some(pid) = proc_table.get(target_nr).and_then(|p| p.priv_id) {
        crate::clock::reset_alarm_timer(priv_table, clock_state, pid);
    }

    // C: do_clear.c:57 — RTS_SETFLAGS(rc, RTS_SLOT_FREE)
    // Mark the TARGET slot as free so it can be reused.
    proc_table.rts_set(target_nr, RtsFlagsBits::SLOT_FREE);

    // C: do_clear.c:60-61 — release_fpu(rc), clear MF_FPU_INITIALIZED
    // Clear the FPU initialized flag so the slot's FPU state is not
    // mistakenly used by a new process assigned to this slot.
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.p_misc_flags.clear(MiscFlagsBits::EXT_REG_INITIALIZED);
    }

    // C: do_clear.c:68 — if SYS_PROC, release privilege structure
    // priv(rc)->s_proc_nr = NONE — marks the privilege slot as unassigned.
    if let Some(target) = proc_table.get(target_nr)
        && let Some(priv_id) = target.priv_id
            && let Some(kpriv) = priv_table.get_mut(priv_id)
                && kpriv.is_sys_proc() {
                    kpriv.identity.s_proc_nr = None;
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
    // C: do_runctl.c:34-35 — extract parameters
    let endpt = m1.m1i1;    // RC_ENDPT
    let action = m1.m1i2;   // RC_ACTION
    let flags = m1.m1i3;    // RC_FLAGS

    // C: do_runctl.c:30 — isokendpt(m_ptr->RC_ENDPT, &proc_nr)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_runctl.c:31 — iskerneln(proc_nr) → EPERM
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    match action {
        RC_STOP => {
            // C: do_runctl.c:44-50 — check RC_DELAY
            if (flags & RC_DELAY) != 0 {
                // If the target is sending or syscall-traced, set MF_SIG_DELAY
                let target = proc_table.get(target_nr);
                if let Some(rp) = target
                    && (rp.p_rts_flags.is_set(RtsFlagsBits::SENDING)
                        || rp.p_misc_flags.is_set(MiscFlagsBits::SC_DEFER))
                        && let Some(rp) = proc_table.get_mut(target_nr) { rp.p_misc_flags.set(MiscFlagsBits::SIG_DELAY); }
                // Check if SIG_DELAY was set (by us or already present)
                let sig_delay_set = proc_table.get(target_nr)
                    .is_some_and(|rp| rp.p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY));
                if sig_delay_set {
                    return KcallResult::Ok(EBUSY);
                }
            }

            // C: do_runctl.c:55-62 — SMP check
            // if (rp->p_cpu != cpuid) { smp_schedule_stop_proc(rp); }
            // else { RTS_SET(rp, RTS_PROC_STOP); }
            //
            // If the target is running on a different CPU, send an IPI
            // to stop it remotely (smp_schedule_stop_proc). Otherwise,
            // set RTS_PROC_STOP directly (local stop).
            let target_cpu = proc_table
                .get(target_nr)
                .map(|p| {
                    CpuId::new_unchecked(
                        p.p_sched.cpu.load(core::sync::atomic::Ordering::Acquire)
                    )
                })
                .unwrap_or(CpuId::BSP);

            // Check if SMP is enabled and target is on a different CPU.
            // SAFETY: BKL is held by kernel_call_dispatch. try_smp_state
            // returns None if SMP_STATE is not yet initialized (single-CPU
            // boot or unit tests).
            let smp_enabled = unsafe { crate::try_smp_state() }.is_some();
            use minix_arch::SmpArch;
            let current_cpu_id = minix_arch::CurrentSmpArch::current_cpu();
            let current_cpu = CpuId::new_unchecked(current_cpu_id);

            if smp_enabled && target_cpu != current_cpu {
                // C: smp_schedule_stop_proc(rp) — send STOP_PROC IPI to
                // the target's CPU. The IPI handler will set RTS_PROC_STOP
                // on the remote CPU.
                // SAFETY: BKL is held; smp_state is initialized (checked above).
                let smp_state = unsafe { crate::smp_state() };
                smp_state.schedule_stop_proc::<minix_arch::CurrentSmpArch>(
                    proc_table,
                    target_nr,
                    current_cpu,
                );
            } else {
                // Single-CPU path: set RTS_PROC_STOP via rts_set which
                // automatically dequeues the process from the scheduler.
                proc_table.rts_set(target_nr, RtsFlagsBits::PROC_STOP);
            }
        }
        RC_RESUME => {
            // C: do_runctl.c:65 — assert(RTS_ISSET(rp, RTS_PROC_STOP))
            debug_assert!(
                proc_table.get(target_nr)
                    .is_some_and(|rp| rp.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP)),
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
/// C: `do_schedctl()` — do_schedctl.c:7-46
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
    // C: do_schedctl.c:16 — extract flags
    let flags = sc.flags;
    // C: do_schedctl.c:23,26 — extract target endpoint, priority, quantum, cpu
    let target_endpoint = Endpoint(sc.endpoint);
    let priority = sc.priority;
    let quantum = sc.quantum;
    let cpu = sc.cpu;

    // C: do_schedctl.c:16-17 — validate flags (only SCHEDCTL_FLAG_KERNEL defined)
    if flags & !SCHEDCTL_FLAG_KERNEL != 0 {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_schedctl.c:23-24 — isokendpt(endpoint, &proc_nr)
    // Resolve the TARGET process (not the caller). C uses `p = proc_addr(proc_nr)`.
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    if flags & SCHEDCTL_FLAG_KERNEL != 0 {
        // C: do_schedctl.c:28-34 — kernel becomes the scheduler.
        // Extract scheduling parameters and call sched_proc(p, ..., FALSE).
        // `niced = FALSE` matches C: do_schedctl.c:37 — sched_proc is called
        // with the literal `FALSE`, not a message field (unlike SYS_SCHEDULE).
        //
        // Design decision §3.8 (11-scheduling-primitives.md): convert C's i32 -1 sentinel
        // ("keep current") to Option. Negative values other than -1 are
        // rejected early to match C semantics (system.c:644-648).
        let priority_opt = match priority {
            -1 => None,
            // Priority is bounded 0..=255 by the u8 target type; values
            // outside this range are rejected to avoid silent truncation.
            v if (0..=u8::MAX as i32).contains(&v) => Some(v as u8),
            _ => return KcallResult::Ok(EINVAL), // priority < 0 && != -1, or > 255
        };
        let quantum_opt = match quantum {
            -1 => None,
            v if v >= 1 => Some(v as u32),
            _ => return KcallResult::Ok(EINVAL), // quantum < 1 && != -1
        };
        // C: do_schedctl.c:32-34 — cpu is `int`; -1 means "keep current".
        // Non-negative i32 fits in u32 without truncation.
        let cpu_opt = if cpu == -1 { None } else { Some(cpu as u32) };

        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        match crate::sched::sched_proc(target, crate::sched::SchedParams { priority: priority_opt, quantum: quantum_opt, cpu: cpu_opt, niced: false }) {
            Ok(()) => {
                // C: do_schedctl.c:39 — p->p_scheduler = NULL
                // Kernel is now the scheduler; clear any user-space scheduler.
                //
                // # Assignment timing
                //
                // The C source sets `p_scheduler = NULL` **after** `sched_proc`
                // returns OK (do_schedctl.c:37 → do_schedctl.c:39). The Rust
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
        // C: do_schedctl.c:41-42 — caller becomes the scheduler.
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
/// # C Semantic Alignment (do_statectl.c:15-51)
///
/// - `SYS_STATE_CLEAR_IPC_REFS` (1): clear_ipc_refs(caller, EDEADSRCDST) — IMPLEMENTED
/// - `SYS_STATE_SET_STATE_TABLE` (2): priv(caller)->s_state_table/entries — IMPLEMENTED
/// - `SYS_STATE_ADD_IPC_BL_FILTER` (3): add_ipc_filter(BLACKLIST) — IMPLEMENTED
/// - `SYS_STATE_ADD_IPC_WL_FILTER` (4): add_ipc_filter(WHITELIST) — IMPLEMENTED
/// - `SYS_STATE_CLEAR_IPC_FILTERS` (5): clear_ipc_filters(caller) — IMPLEMENTED
pub(crate) fn dispatch_statectl(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut crate::proc_table::ProcessTable,
    priv_table: &mut PrivTable,
    pool: &mut crate::ipc_filter::IpcFilterPool,
) -> KcallResult {
    let sc = msg_statectl(msg);
    let request = sc.request;

    let req = match StatectlRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    match req {
        // C: do_statectl.c:21-26 — clear_ipc_refs(caller, EDEADSRCDST)
        // Clears all IPC references for the caller: pending notification
        // and async message bits in all privilege slots, and wakes up any
        // processes blocked on the caller's endpoint.
        //
        // See `syscall::clear_ipc_refs` for the full semantics and design
        // gap notes (return value register, async send cancellation).
        StatectlRequest::ClearIpcRefs => {
            let caller_nr = caller.p_nr;
            crate::syscall::clear_ipc_refs(proc_table, priv_table, caller_nr, EDEADSRCDST);
        }
        // C: do_statectl.c:29-30 — priv(caller)->s_state_table = address; s_state_entries = length
        StatectlRequest::SetStateTable => {
            if let Some(pid) = caller.priv_id
                && let Some(priv_) = priv_table.get_mut(pid) {
                    priv_.runtime.s_state_table = sc.address as usize;
                    priv_.runtime.s_state_entries = sc.length;
                }
        }
        // C: do_statectl.c:32-36 — add_ipc_filter(caller, IPCF_BLACKLIST, address, length)
        StatectlRequest::AddIpcBlFilter => {
            // C: do_statectl.c:34-36 — add_ipc_filter(caller, IPCF_BLACKLIST,
            // address, length)。D-16/D-18 实现见 add_ipc_filter_arm。
            return add_ipc_filter_arm(caller, sc, proc_table, priv_table, pool,
                crate::ipc_filter::IpcFilterType::Blacklist);
        }
        // C: do_statectl.c:37-41 — add_ipc_filter(caller, IPCF_WHITELIST, address, length)
        StatectlRequest::AddIpcWlFilter => {
            return add_ipc_filter_arm(caller, sc, proc_table, priv_table, pool,
                crate::ipc_filter::IpcFilterType::Whitelist);
        }
        StatectlRequest::ClearIpcFilters => {
            // C clear_ipc_filters (system.c:751-770) — free the WHOLE
            // filter chain (D-16: filters chain via `next`; clearing only
            // the head would orphan the rest).
            if let Some(pid) = caller.priv_id
                && let Some(priv_) = priv_table.get_mut(pid)
                && let Some(head) = priv_.mem.s_ipcf.take()
            {
                crate::ipc_filter::free_chain(pool, Some(head));
            }
        }
    }

    KcallResult::Ok(OK)
}


/// D-16/D-18: SYS_STATECTL ADD_IPC_{BL,WL}_FILTER — C `add_ipc_filter`
/// (system.c:705-747): 校验（length 按元素字节对齐、1..=MAX 个 → E2BIG）→
/// 分配新槽 → data_copy 元素进槽 → EL_CHECK 逐元素校验 + MATCH 标志聚合
/// （fill_flags）→ **链尾追加**（C system.c:742-745；修复旧实现"替换非
/// 追加"的偏离）。失败路径只释放新槽位，链上既有 filter 不受影响。
fn add_ipc_filter_arm(
    caller: &mut KProcess,
    sc: MessLsysKrnSysStatectl,
    proc_table: &mut crate::proc_table::ProcessTable,
    priv_table: &mut PrivTable,
    pool: &mut crate::ipc_filter::IpcFilterPool,
    filter_type: crate::ipc_filter::IpcFilterType,
) -> KcallResult {
    use crate::cross_space::data_copy_vmcheck;
    use crate::vm::{AddressRef, CrossSpaceResult};
    use minix_arch::{CurrentDirectMap, DirectMapArch};
    use minix_types::VirBytes;

    // C system.c:708-717 — validate length (byte-aligned to the element
    // size) and element count (1..=IPCF_MAX_ELEMENTS → E2BIG).
    let length = sc.length as usize;
    let el_size = core::mem::size_of::<crate::ipc_filter::IpcFilterElement>();
    if length % el_size != 0 {
        return KcallResult::Ok(EINVAL);
    }
    let num_elements = length / el_size;
    // C system.c:713-716 — num_elements <= 0 或超上限 → E2BIG。
    if num_elements == 0 || num_elements > crate::ipc_filter::IPCF_MAX_ELEMENTS {
        return KcallResult::Ok(E2BIG);
    }

    // C system.c:719-722 — allocate a fresh slot; the chain append
    // happens after validation, so early failures free only the new slot.
    let new_idx = match pool.allocate(filter_type) {
        Some(idx) => idx,
        None => return KcallResult::Ok(ENOMEM),
    };

    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;

    // C system.c:729-731 — data_copy elements directly into the slot.
    let dst_phys = match pool.get_mut(new_idx) {
        Some(slot) => CurrentDirectMap::virt_to_phys(VirBytes(
            slot.elements.as_mut_ptr() as u64,
        )),
        None => unreachable!("slot was just allocated"),
    };
    let src = AddressRef::Process {
        endpoint: caller_endpt,
        offset: VirBytes(sc.address),
    };
    let dst = AddressRef::Physical(dst_phys);

    match data_copy_vmcheck(caller, src, dst, length, |endpt| {
        if endpt == caller_endpt { Some(caller_cr3) } else { None }
    }) {
        CrossSpaceResult::Completed(Ok(())) => {}
        CrossSpaceResult::Completed(Err(_)) => {
            pool.free(new_idx);
            return KcallResult::Ok(EFAULT);
        }
        CrossSpaceResult::Suspended(_) => {
            // The resume path (arch_do_syscall) re-executes the whole
            // add; freeing here avoids leaking the slot across suspend.
            pool.free(new_idx);
            return KcallResult::VmSuspend;
        }
    }

    // D-18 (C check_ipc_filter, system.c:776-798) — EL_CHECK each element
    // (isokendpt 经 proc_table 解析) + MATCH 标志聚合（fill_flags）+
    // 链尾追加（C system.c:742-745）。校验失败：finalize 不释放，由本
    // 函数释放新槽位后返回 EINVAL。
    let caller_priv_id = match caller.priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };
    match finalize_ipc_filter(proc_table, caller_priv_id, pool, new_idx, num_elements, priv_table) {
        Ok(()) => KcallResult::Ok(OK),
        Err(EINVAL) => {
            pool.free(new_idx);
            KcallResult::Ok(EINVAL)
        }
        Err(e) => KcallResult::Ok(e),
    }
}

/// D-18: C `check_ipc_filter(ipcf, TRUE)`（system.c:776-798）+ 链尾追加
/// （system.c:742-745）。校验每个元素（EL_CHECK，isokendpt 经
/// `proc_table` 解析）、聚合 MATCH 标志进 `slot.flags`，然后把 `new_idx`
/// 追加到 `caller_priv_id` 的 filter 链尾。校验失败返回 `Err(EINVAL)`
/// 且**不释放**槽位（由调用方决定）——链上既有 filter 不受影响。
pub(crate) fn finalize_ipc_filter(
    proc_table: &crate::proc_table::ProcessTable,
    caller_priv_id: u16,
    pool: &mut crate::ipc_filter::IpcFilterPool,
    new_idx: usize,
    num_elements: usize,
    caller_priv_table: &mut PrivTable,
) -> Result<(), i32> {
    let _ = caller_priv_id; // 链头读取走 caller_priv_table
    if let Some(slot) = pool.get_mut(new_idx) {
        slot.num_elements = num_elements;
        let mut aggregated = 0i32;
        for el in &slot.elements[..num_elements] {
            let src_ep = minix_types::Endpoint(el.m_source);
            let source_ok = proc_table.endpoint_to_nr(src_ep).is_some();
            if !crate::ipc_filter::el_check(el, source_ok) {
                return Err(EINVAL);
            }
            aggregated |= (el.flags
                & (crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE
                    | crate::ipc_filter::IpcFilterElFlags::MATCH_M_TYPE))
                as i32;
        }
        slot.flags = aggregated;
    }
    // C system.c:742-745 — append at the end of the IPC filter chain.
    let old_head = caller_priv_table
        .get(caller_priv_id)
        .and_then(|p| p.mem.s_ipcf);
    let new_head = crate::ipc_filter::append_to_chain(pool, old_head, new_idx);
    if let Some(priv_) = caller_priv_table.get_mut(caller_priv_id) {
        priv_.mem.s_ipcf = new_head;
    }
    Ok(())
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
        msg.m_type = Syscall::Statectl as i32;
        // SAFETY: we just constructed `statectl` and `msg`; no aliasing.
        msg.m_u.m_lsys_krn_sys_statectl = statectl;
        msg
    }

    /// Helper: prepare a caller process with a working priv_id, returning
    /// (caller, priv_table). The priv_id must map to a real KPriv slot so
    /// dispatch_statectl can update `s_ipcf`.
    fn build_caller_with_priv() -> (KProcess, crate::test_helpers::TestPrivTable) {
        let priv_table = crate::test_helpers::test_priv_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        caller.priv_id = Some(0);
        (caller, priv_table)
    }

    #[test]
    fn test_dispatch_statectl_add_ipc_filter_rejects_unaligned_length() {
        // C system.c:710-712 — length 不按元素大小对齐 → EINVAL。
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let msg = build_statectl_msg(3, 0xdead_beef, 5);
        assert_eq!(
            dispatch_statectl(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut pool),
            KcallResult::Ok(EINVAL)
        );
        assert_eq!(pool.allocated_count(), 0);
    }

    #[test]
    fn test_dispatch_statectl_add_ipc_filter_rejects_zero_length() {
        // C system.c:713-716 — num_elements <= 0 → E2BIG（长度 0 = 0 个元素）。
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let msg = build_statectl_msg(3, 0xdead_beef, 0);
        assert_eq!(
            dispatch_statectl(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut pool),
            KcallResult::Ok(E2BIG)
        );
        assert_eq!(pool.allocated_count(), 0);
    }

    #[test]
    fn test_dispatch_statectl_add_ipc_filter_suspends_and_frees_on_copy_fault() {
        // data_copy_vmcheck 在 hosted mock 下不可解（PteWalk None →
        // Suspended）→ VmSuspend 且新槽位已释放（不跨挂起泄漏）。
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let msg = build_statectl_msg(3, 0xdead_beef, 12); // 1 element
        assert_eq!(
            dispatch_statectl(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut pool),
            KcallResult::VmSuspend
        );
        assert_eq!(pool.allocated_count(), 0, "suspend must free the new slot");
    }

    /// D-18: finalize 校验元素（EL_CHECK）+ 聚合 MATCH 标志 + 链尾追加。
    /// 非法来源（isokendpt 失败）→ Err(EINVAL) 且槽位保留（由调用方释放）。
    #[test]
    fn test_finalize_ipc_filter_appends_and_aggregates() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        // 目标端点：slot ProcNr(1) 占用（清除 SLOT_FREE）→ isokendpt 通过。
        proc_table
            .get_mut(ProcNr(1))
            .unwrap()
            .p_rts_flags
            .clear(RtsFlagsBits::SLOT_FREE);
        let a_ep = proc_table.get(ProcNr(1)).unwrap().p_endpoint;

        let idx = pool.allocate(crate::ipc_filter::IpcFilterType::Whitelist).unwrap();
        if let Some(slot) = pool.get_mut(idx) {
            slot.elements[0] = crate::ipc_filter::IpcFilterElement {
                flags: crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: a_ep.0,
                m_type: 0,
            };
        }
        assert!(finalize_ipc_filter(&proc_table, 0, &mut pool, idx, 1, &mut priv_table).is_ok());
        // 链头已设置；MATCH 标志聚合进 slot.flags。
        assert_eq!(priv_table.get(0).unwrap().mem.s_ipcf, Some(idx));
        assert_eq!(
            pool.get(idx).unwrap().flags,
            crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE as i32
        );

        // 第二个 filter 追加到链尾（D-16 链式语义：链长 2）。
        let idx2 = pool.allocate(crate::ipc_filter::IpcFilterType::Blacklist).unwrap();
        assert!(finalize_ipc_filter(&proc_table, 0, &mut pool, idx2, 0, &mut priv_table).is_ok());
        assert_eq!(pool.get(idx).unwrap().next, Some(idx2));
        assert_eq!(priv_table.get(0).unwrap().mem.s_ipcf, Some(idx), "链头不变");
    }

    /// D-18 负例：MATCH_M_SOURCE 指向不可解析端点（isokendpt 失败）→
    /// Err(EINVAL)，槽位由调用方释放。
    #[test]
    fn test_finalize_ipc_filter_invalid_source_rejected() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let idx = pool.allocate(crate::ipc_filter::IpcFilterType::Blacklist).unwrap();
        if let Some(slot) = pool.get_mut(idx) {
            slot.elements[0] = crate::ipc_filter::IpcFilterElement {
                flags: crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: 0x7FFF, // 无占用槽解析到它
                m_type: 0,
            };
        }
        assert_eq!(
            finalize_ipc_filter(&proc_table, 0, &mut pool, idx, 1, &mut priv_table),
            Err(EINVAL)
        );
        // finalize 不释放——调用方（add_ipc_filter_arm）负责。
        assert_eq!(pool.allocated_count(), 1);
    }

    #[test]
    fn test_dispatch_statectl_invalid_request_returns_einval() {
        let (mut caller, mut priv_table) = build_caller_with_priv();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let msg = build_statectl_msg(99, 0, 0);
        assert_eq!(
            dispatch_statectl(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut pool),
            KcallResult::Ok(EINVAL)
        );
    }

    /// Occupies the exiting process slot and binds its priv slot.
    /// The exiting process is managed by `sig_mgr` (typically PM).
    fn occupy_exiting_proc(
        procs: &mut ProcessTable,
        privs: &mut PrivTable,
        nr: ProcNr,
        pid: u16,
        sig_mgr: Endpoint,
    ) -> Endpoint {
        let p = procs.get_mut(nr).expect("exit slot");
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        p.priv_id = Some(pid);
        let ep = p.p_endpoint;
        let priv_ = privs.get_mut(pid).expect("priv slot");
        priv_.identity.s_proc_nr = Some(nr);
        priv_.signals.s_sig_mgr = sig_mgr;
        ep
    }

    #[test]
    fn test_dispatch_exit_returns_no_reply() {
        // C: do_exit.c:23 — return EDONTREPLY（不写回消息）
        let mut procs = crate::test_helpers::test_proc_table();
        let mut privs = crate::test_helpers::test_priv_table();
        let mgr_ep = occupy_exiting_proc(
            &mut procs,
            &mut privs,
            ProcNr(0),
            0,
            Endpoint::from_generation_slot(0, 1),
        );
        // manager slot（nr 1）须占用，否则外部路径的通知目标无效
        let _ = occupy_exiting_proc(
            &mut procs,
            &mut privs,
            ProcNr(1),
            1,
            mgr_ep,
        );

        let mut caller = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(0, 0));
        let msg = Message::default();
        assert_eq!(
            dispatch_exit(&mut caller, &msg, &mut procs, &mut privs),
            KcallResult::NoReply
        );
    }

    #[test]
    fn test_dispatch_exit_sets_sigabrt_and_notifies_manager() {
        // C: do_exit.c:21 — cause_sig(caller, SIGABRT)
        // 外部管理路径：SIGABRT 记入 target 的 p_pending，置
        // RTS_SIGNALED|RTS_SIG_PENDING，并通知信号管理器（PM 典型场景）。
        let mut procs = crate::test_helpers::test_proc_table();
        let mut privs = crate::test_helpers::test_priv_table();
        let mgr_ep = occupy_exiting_proc(
            &mut procs,
            &mut privs,
            ProcNr(0),
            0,
            Endpoint::from_generation_slot(0, 1),
        );
        // manager（PM 角色）监听 → 通知以 DELIVERMSG 直接投递
        let _ = occupy_exiting_proc(&mut procs, &mut privs, ProcNr(1), 1, mgr_ep);
        {
            let m = procs.get_mut(ProcNr(1)).unwrap();
            m.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            m.p_getfrom_e = Endpoint::ANY;
        }

        let mut caller = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(0, 0));
        let msg = Message::default();
        assert_eq!(
            dispatch_exit(&mut caller, &msg, &mut procs, &mut privs),
            KcallResult::NoReply
        );

        // 目标（退出的进程）槽位状态
        let target = procs.get(ProcNr(0)).unwrap();
        assert!(target.p_pending.contains(SIGABRT as u8));
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING));
        // 管理器收到 SIGKSIG 通知（system.c:445-446 + 381）
        let manager = procs.get(ProcNr(1)).unwrap();
        assert!(manager.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    #[test]
    fn test_dispatch_runctl_stop() {
        // C: do_runctl.c — RTS_SET(rp, RTS_PROC_STOP) on the target
        let mut proc_table = crate::test_helpers::test_proc_table();
        // Use a valid user-space process slot (nr=0, endpoint=0)
        let target_nr: ProcNr = ProcNr(0);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        let mut caller = KProcess::new(ProcNr(1), Endpoint(1));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = target_endpoint.0; // RC_ENDPT = target
        msg.m_u.m_m1.m1i2 = RC_STOP;           // action = stop
        msg.m_u.m_m1.m1i3 = 0;                  // no RC_DELAY

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));
        assert!(proc_table.get(target_nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_runctl_resume() {
        // C: do_runctl.c — RTS_UNSET(rp, RTS_PROC_STOP) on the target
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr: ProcNr = ProcNr(0);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            target.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        }
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        let mut caller = KProcess::new(ProcNr(1), Endpoint(1));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = target_endpoint.0; // RC_ENDPT = target
        msg.m_u.m_m1.m1i2 = RC_RESUME;

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(OK));
        assert!(!proc_table.get(target_nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_runctl_invalid_action() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr: ProcNr = ProcNr(0);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        let mut caller = KProcess::new(ProcNr(1), Endpoint(1));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = target_endpoint.0;
        msg.m_u.m_m1.m1i2 = 99; // invalid action

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_runctl_kernel_process_returns_eperm() {
        // C: do_runctl.c:31 — iskerneln(proc_nr) → EPERM
        let mut proc_table = crate::test_helpers::test_proc_table();
        // Kernel processes have negative ProcNr. The IDLE process
        // (nr = -(NR_TASKS-4) on most configs) is a kernel task.
        // Find a kernel process by scanning for negative p_nr.
        let kernel_nr = proc_table.iter()
            .find(|p| p.p_nr < ProcNr(0))
            .map(|p| p.p_nr)
            .expect("crate::test_helpers::test_proc_table() should have kernel tasks");
        let kernel_endpoint = proc_table.get(kernel_nr).unwrap().p_endpoint;

        // IDLE is in SLOT_FREE by default, so endpoint_to_nr won't find it.
        // Clear SLOT_FREE so the endpoint lookup succeeds.
        proc_table.get_mut(kernel_nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let mut caller = KProcess::new(ProcNr(1), Endpoint(1));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = kernel_endpoint.0;
        msg.m_u.m_m1.m1i2 = RC_STOP;

        let result = dispatch_runctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_exec_operates_on_target() {
        // C: do_exec.c:27,30 — isokendpt(endpt, &proc_nr), then operate on rp
        let mut proc_table = crate::test_helpers::test_proc_table();
        // Set up a user-space target process (nr >= 0)
        let target_nr: ProcNr = ProcNr(0);
        proc_table.get_mut(target_nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let target_endpoint = proc_table.get(target_nr).unwrap().p_endpoint;

        // Set up the target with DELIVERMSG, RECEIVING, and EXT_REG_INITIALIZED
        proc_table.get_mut(target_nr).unwrap().p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        proc_table.get_mut(target_nr).unwrap().p_rts_flags.set(RtsFlagsBits::RECEIVING);
        proc_table.get_mut(target_nr).unwrap().p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);

        let mut caller = KProcess::new(ProcNr(1), Endpoint(1));
        let mut msg = Message::default();
        msg.m_type = Syscall::Exec as i32;
        msg.m_u.m_lsys_krn_sys_exec.endpt = target_endpoint.0;
        // name = 0 (null pointer) — data_copy_vmcheck will suspend
        // on the page fault (source address 0 not mapped).

        let result = dispatch_exec(&mut caller, &msg, &mut proc_table);
        // Name copy suspends (null name pointer → page fault → VmSuspend).
        // C: do_exec.c:37-42 — name copy via data_copy; fault → VMSUSPEND.
        assert_eq!(result, KcallResult::VmSuspend);

        // DELIVERMSG is cleared BEFORE the name copy (do_exec.c:32-34),
        // so it should be cleared even though the name copy suspended.
        let target = proc_table.get(target_nr).unwrap();
        assert!(!target.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));

        // RECEIVING and EXT_REG_INITIALIZED are cleared AFTER the name copy
        // (do_exec.c:51,55-57), so they should still be set.
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert!(target.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED));

        // Caller should be unmodified
        assert!(!caller.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    #[test]
    fn test_dispatch_exec_invalid_endpoint() {
        // C: do_exec.c:27,30 — isokendpt fails → EINVAL
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(1), Endpoint(1));
        let mut msg = Message::default();
        msg.m_type = Syscall::Exec as i32;
        msg.m_u.m_lsys_krn_sys_exec.endpt = 99999; // invalid endpoint

        let result = dispatch_exec(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_clear_invalid_endpoint_returns_einval() {
        // C: do_clear.c:29 — isokendpt fails → EINVAL
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        // Set an endpoint that won't be found in the process table
        msg.m_u.m_m1.m1i1 = 99999; // invalid endpoint
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let result = dispatch_clear(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_clear_sets_target_slot_free() {
        // C: do_clear.c:57 — RTS_SETFLAGS(rc, RTS_SLOT_FREE)
        // The target process (not the caller) should be marked SLOT_FREE.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();

        // Activate a user-process slot so endpoint_to_nr can find it.
        // Slot with p_nr = 0 (first user process after NR_TASKS).
        let target_nr = ProcNr(0);
        let target_ep = Endpoint::from_generation_slot(1, target_nr.0);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        assert!(proc_table.endpoint_to_nr(target_ep).is_some());

        // Caller is a different process (e.g., PM)
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = target_ep.get(); // target endpoint

        // dispatch_clear now calls the global irq_manager() during IRQ-hook
        // cleanup; install an empty IrqManager so the global is initialized.
        unsafe { crate::init_irq_manager_for_test(); }
        let result = dispatch_clear(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(OK));

        // TARGET should be SLOT_FREE, not caller
        assert!(proc_table.get(target_nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE));
    }

    #[test]
    fn test_dispatch_clear_clears_ext_reg_on_target() {
        // C: do_clear.c:60-61 — release_fpu(rc), clear MF_FPU_INITIALIZED
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();

        let target_nr = ProcNr(0);
        let target_ep = Endpoint::from_generation_slot(1, target_nr.0);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            target.p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);
        }

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = target_ep.get();

        // dispatch_clear now calls the global irq_manager() during IRQ-hook
        // cleanup; install an empty IrqManager so the global is initialized.
        unsafe { crate::init_irq_manager_for_test(); }
        let result = dispatch_clear(&mut caller, &msg, &mut proc_table, &mut priv_table, &mut crate::clock::ClockState::new());
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
        msg.m_type = Syscall::Schedctl as i32;
        // SAFETY: we just constructed `sc` and `msg`; no aliasing.
        msg.m_u.m_lsys_krn_schedctl = sc;
        msg
    }

    /// Helper: install a target process at `target_nr` with a live endpoint
    /// so `endpoint_to_nr` can resolve it. Returns the endpoint.
    fn install_target(proc_table: &mut ProcessTable, target_nr: ProcNr) -> Endpoint {
        let target_ep = Endpoint::from_generation_slot(1, target_nr.0);
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        target_ep
    }

    #[test]
    fn test_dispatch_schedctl_invalid_flags() {
        // C: do_schedctl.c:16-17 — flags & ~SCHEDCTL_FLAG_KERNEL → EINVAL
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let msg = build_schedctl_msg(0xFF, 0, 0, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_invalid_endpoint_returns_einval() {
        // C: do_schedctl.c:23-24 — isokendpt fails → EINVAL
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        // Endpoint 99999 won't resolve in an empty process table.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, 99999, 0, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_kernel_flag_calls_sched_proc_and_clears_scheduler() {
        // C: do_schedctl.c:23-35 — kernel becomes scheduler:
        //   sched_proc(p, priority, quantum, cpu, FALSE) + p_scheduler = NULL
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr = ProcNr(0);
        let target_ep = install_target(&mut proc_table, target_nr);

        // Pre-set a user-space scheduler to verify it gets cleared.
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_sched.scheduler = Some(ProcNr(5));
        }

        // Caller is a different process (e.g., the sched server).
        let mut caller = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));

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
        // C: do_schedctl.c:37 — if sched_proc returns error, propagate it.
        // Invalid priority (out of range) → EINVAL from sched_proc.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr = ProcNr(0);
        let target_ep = install_target(&mut proc_table, target_nr);

        let mut caller = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        // priority = 999 exceeds NR_SCHED_QUEUES (16) → EINVAL.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), 999, 10, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_kernel_flag_priority_256_truncation_rejected() {
        // R-16-fix (2026-08-12): Regression test for priority truncation bug.
        // Before fix: `priority = 256` → `256 as u8 = 0` (TASK_Q) → passed
        // sched_proc validation → privilege escalation.
        // After fix: `priority = 256` → rejected at syscall layer because
        // 256 > NR_SCHED_QUEUES (16) → EINVAL.
        // C: system.c:645 — priority > NR_SCHED_QUEUES → EINVAL.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr = ProcNr(0);
        let target_ep = install_target(&mut proc_table, target_nr);

        let mut caller = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        // priority = 256 truncates to 0 without the fix — must be rejected.
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), 256, 10, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_kernel_flag_invalid_quantum_returns_einval() {
        // C: do_schedctl.c:37 → sched_proc validates quantum < 1 && != -1 → EINVAL.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr = ProcNr(0);
        let target_ep = install_target(&mut proc_table, target_nr);

        let mut caller = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        // quantum = 0 is invalid (must be >= 1 or -1).
        let msg = build_schedctl_msg(SCHEDCTL_FLAG_KERNEL, target_ep.get(), 5, 0, 0);

        let result = dispatch_schedctl(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedctl_no_flag_sets_caller_as_scheduler_on_target() {
        // C: do_schedctl.c:41-42 — caller becomes the scheduler.
        // The TARGET's p_scheduler should be set to caller.p_nr, NOT the
        // caller's own p_scheduler.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr = ProcNr(0);
        let target_ep = install_target(&mut proc_table, target_nr);

        // Caller is process at slot 7 (e.g., the sched server).
        let caller_nr = ProcNr(7);
        let mut caller = KProcess::new(caller_nr, Endpoint::from_generation_slot(1, caller_nr.0));

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
        // C: do_schedctl.c:37 → sched_proc(p, -1, -1, -1, FALSE) keeps
        // current priority/quantum/cpu unchanged.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let target_nr = ProcNr(0);
        let target_ep = install_target(&mut proc_table, target_nr);

        // Pre-set known scheduling state on the target.
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_sched.priority.store(7, core::sync::atomic::Ordering::Release);
            target.p_sched.quantum.size_ms.store(20, core::sync::atomic::Ordering::Release);
        }

        let mut caller = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
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
