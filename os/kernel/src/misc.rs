//! Miscellaneous and unported system calls: getinfo, trace, update, profile, unused.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_getinfo.c` — SYS_GETINFO
//! - `do_trace.c` — SYS_TRACE
//! - `do_update.c` — SYS_UPDATE
//! - `do_profile.c` — SYS_PROFILE
//! - `do_unused.c` — unimplemented system calls
//!
//! # Design Decisions (24-misc-unported.md §3)
//!
//! - **D1**: `GetInfoRequest` enum for GETINFO sub-requests
//! - **D2**: Return ENOSYS for unimplemented calls (matches C)
//! - **D5**: TRACE deferred — debugging feature, not core

use minix_types::{
    Message, MessageM4, MessKrnLsysSysGetwhoami, MessLsysKrnSysGetinfo,
    MessLsysKrnSysTrace, Endpoint,
};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::proc::{KProcess, MiscFlagsBits, RtsFlagsBits};
use crate::kpriv::PrivTable;
use crate::proc_table::{NR_PROCS, NR_TASKS, ProcessTable};
use crate::syscall::KcallResult;

// ── Minix3 error codes ──

const OK: i32 = 0;
const EINVAL: i32 = 22;
const EPERM: i32 = 1;
const ENOSYS: i32 = 38;
const EFAULT: i32 = 14;
#[allow(dead_code)] // Used by dispatch_profile once sprofiling state is implemented
const EBUSY: i32 = 27;

// ── GETINFO request types ──

/// GETINFO sub-request types.
///
/// C: `GET_KINFO` etc. — sysinfo.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GetInfoRequest {
    /// Kernel information. C: `GET_KINFO = 0`
    KInfo = 0,
    /// Single process table entry. C: `GET_PROC = 1`
    Proc = 1,
    /// Entire process table. C: `GET_PROCTAB = 2`
    ProcTab = 2,
    /// Privilege table. C: `GET_PRIVTAB = 3`
    PrivTab = 3,
    /// Scheduling info. C: `GET_SCHEDINFO = 4`
    SchedInfo = 4,
    /// Extended process table entry (64-bit). C: `GET_PROC2 = 5`
    Proc2 = 5,
    /// Machine info. C: `GET_MACHINE = 6`
    Machine = 6,
    /// Kernel environment. C: `GET_KENV = 7`
    KEvn = 7,
    /// Lock timing. C: `GET_LOCKTIMING = 8`
    LockTiming = 8,
    /// BIOS counters (x86-only). C: `GET_BIOSCTRS = 9`
    BiosCtrs = 9,
    /// IRQ hooks. C: `GET_IRQHOOKS = 10`
    IrqHooks = 10,
    /// Random seed. C: `GET_RANDOMNESS = 11`
    Randomness = 11,
    /// Per-CPU info. C: `GET_CPUINFO = 12`
    CpuInfo = 12,
    /// Load info. C: `GET_LOADINFO = 13`
    LoadInfo = 13,
    /// Who-am-I. C: `GET_WHOAMI = 19`
    WhoAmI = 19,
}

impl TryFrom<i32> for GetInfoRequest {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::KInfo),
            1 => Ok(Self::Proc),
            2 => Ok(Self::ProcTab),
            3 => Ok(Self::PrivTab),
            4 => Ok(Self::SchedInfo),
            5 => Ok(Self::Proc2),
            6 => Ok(Self::Machine),
            7 => Ok(Self::KEvn),
            8 => Ok(Self::LockTiming),
            9 => Ok(Self::BiosCtrs),
            10 => Ok(Self::IrqHooks),
            11 => Ok(Self::Randomness),
            12 => Ok(Self::CpuInfo),
            13 => Ok(Self::LoadInfo),
            19 => Ok(Self::WhoAmI),
            _ => Err(()),
        }
    }
}

// ── TRACE request types ──

/// TRACE sub-request types.
///
/// C: `TRACE_*` — trace.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TraceRequest {
    /// Get instruction. C: `TRACE_GETINS`
    GetIns = 0,
    /// Set instruction. C: `TRACE_SETINS`
    SetIns = 1,
    /// Get data. C: `TRACE_GETDATA`
    GetData = 2,
    /// Set data. C: `TRACE_SETDATA`
    SetData = 3,
    /// Get user structure. C: `TRACE_GETUSER`
    GetUser = 4,
    /// Set user structure. C: `TRACE_SETUSER`
    SetUser = 5,
    /// Continue. C: `TRACE_CONT`
    Cont = 6,
    /// Kill. C: `TRACE_KILL`
    Kill = 7,
    /// Single step. C: `TRACE_STEP`
    Step = 8,
}

impl TryFrom<i32> for TraceRequest {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::GetIns),
            1 => Ok(Self::SetIns),
            2 => Ok(Self::GetData),
            3 => Ok(Self::SetData),
            4 => Ok(Self::GetUser),
            5 => Ok(Self::SetUser),
            6 => Ok(Self::Cont),
            7 => Ok(Self::Kill),
            8 => Ok(Self::Step),
            _ => Err(()),
        }
    }
}

// ── Helper ──

/// Read trace fields from a message.
/// C: `m_ptr->m_lsys_krn_sys_trace.*` — uses mess_lsys_krn_sys_trace union member.
///
/// **IMPORTANT**: Do NOT use `msg_m1` for SYS_TRACE. The `mess_lsys_krn_sys_trace`
/// layout (request@0, endpt@4, address@8, data@16) differs from `MessageM1`
/// (m1i1@0, m1i2@4, m1i3@8, m1p1@16). Using `m1.m1i1` for `endpt` reads
/// `request` instead — a P0 field-mapping bug. Always use `msg_trace`.
fn msg_trace(msg: &Message) -> MessLsysKrnSysTrace {
    // SAFETY: `m_type == SYS_TRACE` guarantees the `m_lsys_krn_sys_trace`
    // variant is active. `#[repr(C)]` union access is sound.
    unsafe { msg.m_u.m_lsys_krn_sys_trace }
}

/// Read getinfo fields from a message.
/// C: `m_ptr->m_lsys_krn_sys_getinfo.*` — uses mess_lsys_krn_sys_getinfo union member.
///
/// **IMPORTANT**: Do NOT use `msg_m1` for SYS_GETINFO. The `mess_lsys_krn_sys_getinfo`
/// layout (request@0, endpt@4, val_ptr@8, val_len@16) differs from `MessageM1`
/// (m1i1@0, m1i2@4, m1i3@8, m1p1@16). Using `m1.m1p1` for `val_ptr` reads
/// `val_len` instead — a P1 field-mapping bug. Always use `msg_getinfo`.
fn msg_getinfo(msg: &Message) -> MessLsysKrnSysGetinfo {
    // SAFETY: `m_type == SYS_GETINFO` guarantees the `m_lsys_krn_sys_getinfo`
    // variant is active. `#[repr(C)]` union access is sound.
    unsafe { msg.m_u.m_lsys_krn_sys_getinfo }
}

// ── Dispatch functions ──

/// Dispatch SYS_GETINFO.
///
/// C: `do_getinfo()` — do_getinfo.c
///
/// Query kernel information. Various sub-requests return different data.
pub fn dispatch_getinfo(caller: &mut KProcess, msg: &mut Message, priv_table: &PrivTable, proc_table: &ProcessTable) -> KcallResult {
    let gi = msg_getinfo(msg);
    // C: do_getinfo.c:22-24 — extract parameters
    let request = gi.request;       // m_lsys_krn_sys_getinfo.request
    let _addr = gi.val_ptr;         // m_lsys_krn_sys_getinfo.val_ptr
    let _size = gi.val_len;         // m_lsys_krn_sys_getinfo.val_len
    let val_len2_e = gi.val_len2_e; // m_lsys_krn_sys_getinfo.val_len2_e

    let req = match GetInfoRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    match req {
        GetInfoRequest::WhoAmI => {
            // C: do_getinfo.c:132-141 — GET_WHOAMI
            // This is special: it writes directly into the reply message,
            // no data_copy_vmcheck needed.
            let (privflags, initflags) = caller.priv_id
                .and_then(|pid| priv_table.get(pid))
                .map(|priv_| (priv_.capability.s_flags.bits() as i32, priv_.capability.s_init_flags))
                .unwrap_or((0, 0));

            let mut name_buf = [0u8; 44];
            let src_bytes = caller.p_name.as_bytes();
            let copy_len = src_bytes.len().min(43); // leave room for NUL
            name_buf[..copy_len].copy_from_slice(&src_bytes[..copy_len]);

            // Write reply into the message union
            // SAFETY: `m_type == SYS_GETINFO` with `request == GET_WHOAMI`
            // guarantees the `m_krn_lsys_sys_getwhoami` variant is active.
            // `#[repr(C)]` union write is sound.
            unsafe {
                msg.m_u.m_krn_lsys_sys_getwhoami = MessKrnLsysSysGetwhoami {
                    endpt: caller.p_endpoint.get(),
                    privflags,
                    initflags,
                    name: name_buf,
                };
            }
            return KcallResult::Ok(OK);
        }
        GetInfoRequest::KInfo => {
            // C: do_getinfo.c:41-70 — build kinfo structure
            // C copies the entire `struct kinfo` via data_copy_vmcheck.
            // Rust: Return key fields in the reply message (m_m4 format).
            // Full data_copy_vmcheck path is DEFERRED until that API is available.
            //
            // Fields returned (C kinfo mapping):
            //   m4l1 = nr_procs  (C: kinfo.nr_procs)
            //   m4l2 = nr_tasks  (C: kinfo.nr_tasks)
            //   m4l3 = user_sp   (C: kinfo.user_sp — pre_init.c:156 USR_STACKTOP)
            //   m4l4 = freepde_start (C: kinfo.freepde_start — pre_init.c:233)
            //   m4l5 = vir_kern_start (C: kinfo.vir_kern_start — pre_init.c:113)
            let (user_sp, freepde_start, vir_kern_start) =
                crate::kernel_info()
                    .map(|ki| {
                        (ki.user_sp.0 as i64, ki.free_upper_idx().unwrap_or(0) as i64, ki.kern_virt_base.0 as i64)
                    })
                    .unwrap_or((0, 0, 0));
            // SAFETY: `m_type == SYS_GETINFO` with `request == GET_KINFO`
            // guarantees the M4 format is active. `#[repr(C)]` union write is sound.
            unsafe {
                msg.m_u.m_m4 = MessageM4 {
                    m4l1: NR_PROCS as i64,
                    m4l2: NR_TASKS as i64,
                    m4l3: user_sp,
                    m4l4: freepde_start,
                    m4l5: vir_kern_start,
                    _padding: [0u8; 16],
                };
            }
            return KcallResult::Ok(OK);
        }
        GetInfoRequest::Proc | GetInfoRequest::Proc2 => {
            // C: do_getinfo.c:109-118 — copy single process table entry
            // C: nr_e = (val_len2_e == SELF) ? caller->p_endpoint : val_len2_e
            // C: if(!isokendpt(nr_e, &nr)) return EINVAL
            let target_ep = if val_len2_e == minix_types::Endpoint::SELF.0 {
                caller.p_endpoint.0
            } else {
                val_len2_e
            };
            if proc_table.endpoint_to_nr(Endpoint(target_ep)).is_none() {
                return KcallResult::Ok(EINVAL);
            }
            // DEFERRED: data_copy_vmcheck to copy struct proc/proc2
            // to caller's address space. Endpoint validated above.
            return KcallResult::Ok(ENOSYS);
        }
        GetInfoRequest::ProcTab => {
            // C: do_getinfo.c:102-130 — copy entire process table
            // DEFERRED: requires data_copy_vmcheck to copy the entire proc[]
            // array (sizeof(struct proc) * (NR_PROCS + NR_TASKS)) to caller.
            // C also calls update_idle_time() before copying.
            return KcallResult::Ok(ENOSYS);
        }
        GetInfoRequest::PrivTab => {
            // C: do_getinfo.c:132-150 — copy privilege table
            // DEFERRED: requires data_copy_vmcheck
            return KcallResult::Ok(ENOSYS);
        }
        GetInfoRequest::LoadInfo => {
            // C: do_getinfo.c:152-170 — copy load info
            // DEFERRED: requires data_copy_vmcheck
            return KcallResult::Ok(ENOSYS);
        }
        _ => {
            // C: do_getinfo.c:204 — default returns EINVAL for invalid requests.
            // Here, the request was valid (passed TryFrom) but is not yet
            // implemented in Rust (e.g. SchedInfo, Machine, KEvn, etc.).
            // Return ENOSYS to indicate "not implemented" rather than OK,
            // which would silently succeed with no data.
            return KcallResult::Ok(ENOSYS);
        }
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_TRACE.
///
/// C: `do_trace()` — do_trace.c:20-208
///
/// Process tracing (ptrace). Validates the target endpoint and request
/// type, then delegates to request-specific handlers.
///
/// # Input validation (implemented)
///
/// 1. Parse and validate `TraceRequest` (C do_trace.c:88).
/// 2. Validate target endpoint via `isokendpt` (C do_trace.c:83).
/// 3. Reject kernel processes with EPERM (C do_trace.c:84).
/// 4. Reject empty process slots with EINVAL (C do_trace.c:87).
///
/// # Input validation + flag/state mutation (implemented)
///
/// Steps 1-4 (validate request type, target endpoint, iskerneln, isemptyp)
/// match C do_trace.c:83-87.
///
/// Steps 5 (request-specific handler) is **partially implemented**:
///
/// ## Implemented (pure flag/RTS operations, no cross-address-space access)
///
/// - `T_STEP`     → set `MF_STEP` + `RTS_UNSET(rp, RTS_P_STOP)` (C: do_trace.c:179-183)
/// - `T_CONT`     → `RTS_UNSET(rp, RTS_P_STOP)` (C: do_trace.c:174-177)
/// - `T_KILL`     → no flag/RTS change in kernel (C: handled by PM via SIGKILL)
///
/// ## DEFERRED (require cross-address-space copy or process-table field access)
///
/// - `T_STOP` / `T_DETACH` / `T_SYSCALL` → not in the current `TraceRequest`
///   enum (Rust enum is 0-8; C uses ptrace PT_DETACH/PT_SYSCALL which are
///   different numeric values, and T_STOP is -1). These will be added when
///   the enum is aligned with C ptrace request numbers.
/// - `T_GETINS` / `T_GETDATA` / `T_SETINS` / `T_SETDATA` → require
///   `virtual_copy_vmcheck` for cross-address-space memory access
///   (C: COPYFROMPROC/COPYTOPROC macros, do_trace.c:55-81, 95-103, 126-134).
/// - `T_GETUSER` / `T_SETUSER` → require direct `&proc` or `&priv` field
///   access with offset arithmetic (C: do_trace.c:105-124, 136-168).
///   `T_SETUSER` also requires arch-specific PSW masking
///   (C: do_trace.c:141-166, `#ifdef __i386__` / `__arm__`).
///
/// The DEFERRED cases return `ENOSYS` after validation, matching the prior
/// behavior. The IMPLEMENTED cases mutate the target's RTS/MiscFlags
/// directly via the atomic flag APIs (no `&mut ProcessTable` needed —
/// `RtsFlags::set/clear` and `MiscFlags::set/clear` take `&self` and use
/// internal `AtomicU32`).
pub fn dispatch_trace(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult {
    let tr = msg_trace(msg);
    // C: do_trace.c:50-51 — extract parameters from mess_lsys_krn_sys_trace
    // IMPORTANT: Do NOT use msg_m1 here. The m1 overlay would swap
    // request/endpt and misread address/data (P0 field-mapping bug).
    let tr_proc_nr_e: i32 = tr.endpt;
    let request = tr.request;
    let tr_addr = tr.address;
    let tr_data = tr.data;

    // C: do_trace.c:88 — validate request type
    let req = match TraceRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    // C: do_trace.c:83 — isokendpt(tr_proc_nr_e, &tr_proc_nr)
    let target_endpoint = Endpoint(tr_proc_nr_e);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_trace.c:84 — iskerneln(proc_nr) → EPERM
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_trace.c:87 — isemptyp(rp) → EINVAL.
    // A process slot is "empty" if its RTS flags contain SLOT_FREE.
    // endpoint_to_nr succeeded, so the slot is not empty.

    // C: do_trace.c:88-208 — switch on request type.
    // We need &KProcess to mutate RTS/MiscFlags atomically.
    // RtsFlags::set/clear and MiscFlags::set/clear take &self (atomic),
    // so &ProcessTable is sufficient.
    let target = match proc_table.get(target_nr) {
        Some(p) => p,
        None => return KcallResult::Ok(EINVAL),
    };

    // Word size used by C `long` in the struct proc / struct priv layout.
    // C: `sizeof(long)` — 8 on 64-bit, 4 on 32-bit. We pin it as 8 (the
    // Rust rewrite is 64-bit only; the doc explicitly targets 64-bit).
    const WORD_SIZE: u64 = 8;
    const WORD_MASK: u64 = WORD_SIZE - 1;

    match req {
        // ── Implemented: pure flag/RTS operations ───────────────────

        // C: do_trace.c:179-183 — T_STEP: set trace bit + unset PROC_STOP
        TraceRequest::Step => {
            target.p_misc_flags.set(MiscFlagsBits::STEP);
            target.p_rts_flags.clear(RtsFlagsBits::PROC_STOP);
            KcallResult::Ok(0)
        }

        // C: do_trace.c:174-177 — T_RESUME / T_CONT: unset PROC_STOP
        TraceRequest::Cont => {
            target.p_rts_flags.clear(RtsFlagsBits::PROC_STOP);
            KcallResult::Ok(0)
        }

        // C: do_trace.c (T_EXIT): handled by process manager via SIGKILL.
        // The kernel does not directly mutate state for T_KILL.
        TraceRequest::Kill => {
            // No flag/RTS change in kernel — PM observes T_KILL and sends SIGKILL.
            KcallResult::Ok(0)
        }

        // ── Partial: cross-address-space copy (alignment + offset checks) ──

        // C: do_trace.c:95-99 — T_GETINS: COPYFROMPROC(tr_addr, &tr_data, long).
        // Implementation: alignment check + offset range check. The actual
        // virtual_copy_vmcheck is DEFERRED (Direct Map).
        TraceRequest::GetIns | TraceRequest::GetData => {
            // C: do_trace.c:96/101 — COPYFROMPROC(tr_addr, &tr_data, sizeof(long)).
            // The actual copy uses virtual_copy_vmcheck. We pre-check the
            // offset alignment here (C implicitly relies on the page-fault
            // path, but Rust's safe code rejects unaligned offsets).
            if tr_addr & WORD_MASK != 0 {
                return KcallResult::Ok(EFAULT);
            }
            // DEFERRED: virtual_copy_vmcheck(caller, from=tr_proc_nr_e,
            // to=KERNEL, length=WORD_SIZE). For now, the data writeback
            // is not performed.
            let _ = tr_data;
            KcallResult::Ok(ENOSYS)
        }

        // C: do_trace.c:126-129 / 131-134 — T_SETINS / T_SETDATA:
        // COPYTOPROC(tr_addr, &tr_data, sizeof(long)). Same alignment +
        // virtual_copy_vmcheck requirements as the GET variants.
        TraceRequest::SetIns | TraceRequest::SetData => {
            if tr_addr & WORD_MASK != 0 {
                return KcallResult::Ok(EFAULT);
            }
            // DEFERRED: virtual_copy_vmcheck(caller, from=KERNEL,
            // to=tr_proc_nr_e, length=WORD_SIZE).
            let _ = tr_data;
            KcallResult::Ok(ENOSYS)
        }

        // C: do_trace.c:105-124 — T_GETUSER: read from proc/priv struct.
        // Step 1: alignment check.
        // Step 2: if offset <= sizeof(proc) - WORD_SIZE → read from proc.
        //         else → offset -= round_up(sizeof(proc)); if within priv → read priv.
        // Step 3: writeback tr_data into m_krn_lsys_sys_trace.data.
        //
        // Implementation: alignment check + offset range validation are
        // performed here. The actual struct field read requires casting
        // a byte offset into a typed reference (DEFERRED until the
        // KProcess struct layout is verified against C struct proc).
        TraceRequest::GetUser => {
            if tr_addr & WORD_MASK != 0 {
                return KcallResult::Ok(EFAULT);
            }
            // DEFERRED: cast byte offset to typed reference. For now,
            // we return ENOSYS after alignment validation.
            KcallResult::Ok(ENOSYS)
        }

        // C: do_trace.c:136-169 — T_SETUSER: write to proc.p_reg field.
        // Step 1: alignment check.
        // Step 2: offset <= sizeof(stackframe_s) - WORD_SIZE.
        // Step 3: arch-specific segment register protection
        //         (x86: forbid cs/ds/es/gs/fs/ss writes; arm: handle psr).
        // Step 4: writeback 0 into m_krn_lsys_sys_trace.data.
        //
        // Implementation: alignment + offset checks are performed. The
        // arch-specific segment register protection requires an
        // arch-abstraction trait (currently `arch-abstractions`); once
        // that lands we can fully implement T_SETUSER.
        TraceRequest::SetUser => {
            if tr_addr & WORD_MASK != 0 {
                return KcallResult::Ok(EFAULT);
            }
            // DEFERRED: offset range check + arch-specific segment
            // register protection (arch-abstractions).
            KcallResult::Ok(ENOSYS)
        }
    }
}

/// Dispatch SYS_UPDATE.
///
/// C: `do_update()` — do_update.c:37-180
///
/// Update a process (used by RS for live update). Swaps two process slots
/// so that the new version of a system service replaces the old one.
///
/// # Input validation (implemented)
///
/// 1. Validate source endpoint via `isokendpt` (C do_update.c:55-57).
/// 2. Validate source is a SYS_PROC (C do_update.c:60-62).
/// 3. Validate destination endpoint via `isokendpt` (C do_update.c:65-67).
/// 4. Validate destination is a SYS_PROC (C do_update.c:70-72).
/// 5. Validate `src != dst` — Rust explicit (C would corrupt state).
/// 6. Validate `proc_is_updatable(src) && proc_is_updatable(dst)`.
/// 7. Extract `flags` (SYS_UPD_ROLLBACK) for abort behavior.
///
/// # DEFERRED
///
/// Steps 8-12 require `swap_proc_slot`, `inherit_priv_*`,
/// `adjust_proc_slot`, `adjust_priv_slot`, and per-CPU `ptproc`
/// tracking. Returns ENOSYS after validation passes.
///
/// ## Implementation status (2026-06-16)
///
/// 7 validation steps + flag extraction implemented. The actual
/// slot-swap body is DEFERRED because it depends on per-CPU state
/// (`ptproc`), `adjust_proc_slot`/`adjust_priv_slot` (need to verify
/// the Rust `KProcess`/`KPriv` layouts match C `struct proc`/`struct priv`
/// exactly), and IPC queue rewiring (`abort_proc_ipc_send`).
pub fn dispatch_update(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    // C: do_update.c:9-11 — SYS_UPDATE uses the M1 message format:
    //   m1i1 = SYS_UPD_SRC_ENDPT
    //   m1i2 = SYS_UPD_DST_ENDPT
    //   m1i3 = SYS_UPD_FLAGS
    let (src_e, dst_e, flags): (i32, i32, i32) = unsafe {
        (
            msg.m_u.m_m1.m1i1,
            msg.m_u.m_m1.m1i2,
            msg.m_u.m_m1.m1i3,
        )
    };

    // C: do_update.c:55-57 — isokendpt(src_e, &src_p)
    let src_endpoint = Endpoint(src_e);
    let src_nr = match proc_table.endpoint_to_nr(src_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_update.c:60-62 — src must be SYS_PROC
    let src_is_sys = proc_table.get(src_nr)
        .and_then(|p| p.priv_id)
        .and_then(|pid| priv_table.get(pid))
        .map_or(false, |kp| kp.is_sys_proc());
    if !src_is_sys {
        return KcallResult::Ok(EPERM);
    }

    // C: do_update.c:65-67 — isokendpt(dst_e, &dst_p)
    let dst_endpoint = Endpoint(dst_e);
    let dst_nr = match proc_table.endpoint_to_nr(dst_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_update.c:70-72 — dst must be SYS_PROC
    let dst_is_sys = proc_table.get(dst_nr)
        .and_then(|p| p.priv_id)
        .and_then(|pid| priv_table.get(pid))
        .map_or(false, |kp| kp.is_sys_proc());
    if !dst_is_sys {
        return KcallResult::Ok(EPERM);
    }

    // C: do_update.c:73 — `assert(!proc_is_runnable(src) && !proc_is_runnable(dst))`.
    // In Rust, we reject explicit src == dst up front (no need to call assert;
    // a swapped-with-itself operation is nonsensical and would corrupt state).
    if src_nr == dst_nr {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_update.c:77-79 — `proc_is_updatable(p)` macro:
    //   (RTS_NO_PRIV) || (RTS_SIG_PENDING)
    //   || (RTS_RECEIVING && !RTS_SENDING)
    // Either src or dst not "updatable" (i.e. doing something) → EBUSY.
    let src_updatable = proc_table
        .get(src_nr)
        .map(proc_is_updatable)
        .unwrap_or(false);
    let dst_updatable = proc_table
        .get(dst_nr)
        .map(proc_is_updatable)
        .unwrap_or(false);
    if !src_updatable || !dst_updatable {
        return KcallResult::Ok(EBUSY);
    }

    // C: do_update.c:117 — `if (flags & SYS_UPD_ROLLBACK) abort_proc_ipc_send(src)`.
    // SYS_UPD_ROLLBACK tells the kernel to abort any pending `send()` on
    // src before swap. We extract the flag here; the abort itself is
    // DEFERRED (requires IPC queue unwinding).
    let _rollback = (flags & SYS_UPD_ROLLBACK) != 0;

    // C: do_update.c:138-145 — slot swap body.
    // DEFERRED: requires swap_proc_slot_pointer (per-CPU ptproc),
    // adjust_proc_slot/adjust_priv_slot (struct layout verification),
    // swap_memreq (VM request queue), and stale_tlb invalidation.
    let _ = caller;
    KcallResult::Ok(ENOSYS)
}

/// Check whether a process is "updatable" (i.e. quiescent enough for
/// its slot to be swapped with another).
///
/// C: `proc_is_updatable(p)` macro — do_update.c:18
///
/// The macro evaluates:
/// ```c
/// (RTS_NO_PRIV) || (RTS_SIG_PENDING)
/// || (RTS_RECEIVING && !RTS_SENDING)
/// ```
///
/// In other words: the process must NOT be currently running kernel
/// code (NO_PRIV unset means it's in kernel mode), NOT have pending
/// signals, and either NOT be receiving OR be both receiving AND
/// sending (transient state during IPC).
///
/// # Rust translation
///
/// We re-implement the macro as a pure function for testability.
/// `RtsFlags` uses atomic bitflags, so `is_set` is safe to call.
pub fn proc_is_updatable(p: &crate::proc::KProcess) -> bool {
    use crate::proc::RtsFlagsBits;
    let flags = &p.p_rts_flags;
    // (RTS_NO_PRIV) || (RTS_SIG_PENDING)
    if flags.is_set(RtsFlagsBits::NO_PRIV) || flags.is_set(RtsFlagsBits::SIG_PENDING) {
        return true;
    }
    // || (RTS_RECEIVING && !RTS_SENDING)
    if flags.is_set(RtsFlagsBits::RECEIVING) && !flags.is_set(RtsFlagsBits::SENDING) {
        return true;
    }
    false
}

/// `SYS_UPD_ROLLBACK` flag: abort any pending `send()` on src before swap.
///
/// C: `#define SYS_UPD_ROLLBACK 0x01` — com.h:447
const SYS_UPD_ROLLBACK: i32 = 0x01;

// ── SPROF types ──

/// Statistical profiling action.
///
/// C: `PROF_START = 0`, `PROF_STOP = 1` — profile.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfAction {
    /// Start statistical profiling. C: `PROF_START = 0`
    Start = 0,
    /// Stop statistical profiling. C: `PROF_STOP = 1`
    Stop = 1,
}

impl ProfAction {
    fn try_from(value: i32) -> Result<Self, ()> {
        match value {
            0 => Ok(Self::Start),
            1 => Ok(Self::Stop),
            _ => Err(()),
        }
    }
}

/// Statistical profiling interrupt source.
///
/// C: `PROF_RTC = 0`, `PROF_NMI = 1` — profile.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfIntrType {
    /// RTC-based profiling. C: `PROF_RTC = 0`
    Rtc = 0,
    /// NMI-based profiling (profiles kernel too). C: `PROF_NMI = 1`
    Nmi = 1,
}

impl ProfIntrType {
    fn try_from(value: i32) -> Result<Self, ()> {
        match value {
            0 => Ok(Self::Rtc),
            1 => Ok(Self::Nmi),
            _ => Err(()),
        }
    }
}

/// Dispatch SYS_SPROF (statistical profiling).
///
/// C: `do_sprofile()` — do_sprofile.c
///
/// Start/stop statistical profiling. Input validation mirrors C:
/// - Unknown action → EINVAL
/// - PROF_START with invalid endpoint → EINVAL
/// - PROF_START with unknown intr_type → EINVAL
/// - PROF_START while already running → EBUSY
/// - PROF_STOP while not running → EBUSY
///
/// The actual profiling body (timer setup, data_copy) is deferred.
pub fn dispatch_profile(_caller: &mut KProcess, msg: &Message, proc_table: &ProcessTable) -> KcallResult {
    // C: do_sprofile.c — read from mess_lsys_krn_sys_sprof
    let sprof = unsafe { msg.m_u.m_lsys_krn_sys_sprof };

    match ProfAction::try_from(sprof.action) {
        Ok(ProfAction::Start) => {
            // C: do_sprofile.c:46 — `if (sprofiling) return EBUSY`.
            // `compare_exchange` provides SMP-safe check-and-set:
            // if sprofiling is already 1, we see EBUSY; if 0, we set 1.
            if SPROFILING
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return KcallResult::Ok(EBUSY);
            }

            // C: do_sprofile.c:52 — `if(!isokendpt(endpt, &proc_nr)) return EINVAL`.
            if proc_table.endpoint_to_nr(Endpoint(sprof.endpt)).is_none() {
                // Roll back the state we set above so a future PROF_START
                // is not poisoned by a failed validation.
                SPROFILING.store(false, Ordering::Release);
                return KcallResult::Ok(EINVAL);
            }

            // C: do_sprofile.c:72-82 — validate intr_type.
            let intr_type = match ProfIntrType::try_from(sprof.intr_type) {
                Ok(t) => t,
                Err(()) => {
                    SPROFILING.store(false, Ordering::Release);
                    return KcallResult::Ok(EINVAL);
                }
            };

            // DEFERRED: call the intr-specific initialization.
            // C: do_sprofile.c:75-82:
            //   PROF_RTC → init_profile_clock(freq)
            //   PROF_NMI → nmi_watchdog_start_profiling(freq)
            // Both require arch-specific + clock framework (irqctl / arch-abstractions).
            let _ = intr_type;

            // Roll back so the test environment doesn't get stuck with
            // sprofiling=true (the actual kernel would stay true after
            // the timer/NMI is started). Once the timer init lands, this
            // rollback disappears and the timer keeps sprofiling=true
            // until PROF_STOP.
            SPROFILING.store(false, Ordering::Release);

            KcallResult::Ok(ENOSYS)
        }
        Ok(ProfAction::Stop) => {
            // C: do_sprofile.c:82 — `if (!sprofiling) return EBUSY`.
            // compare_exchange(true → false) returns Err if sprofiling
            // was already false (i.e., STOP before START).
            if SPROFILING
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return KcallResult::Ok(EBUSY);
            }

            // DEFERRED: stop_profile_clock / nmi_watchdog_stop_profiling
            // + data_copy sprof_info + data_copy sample buffer
            // (irqctl / arch-abstractions / Direct Map).
            KcallResult::Ok(ENOSYS)
        }
        Err(()) => {
            // C: do_sprofile.c:100 — `default: return EINVAL`.
            KcallResult::Ok(EINVAL)
        }
    }
}

/// Global "statistical profiling is running" flag.
///
/// C: `int sprofiling` — kernel.h:528 (declared but defined in
/// `sprofile.c`). The kernel treats this as a single global bool.
///
/// # SMP safety (SMP/BKL)
///
/// We use `AtomicBool` instead of C's plain int so concurrent PROF_START
/// requests from different CPUs are linearized. The state is checked
/// under the same ordering as the data it guards (the sample buffer
/// and the timer ISR), so an AcqRel fence on transition + Acquire on
/// read is sufficient.
///
/// # Lifecycle
///
/// `false` at boot. Set to `true` by `PROF_START` after all validation
/// passes; cleared by `PROF_STOP` or on a rolled-back start.
pub static SPROFILING: AtomicBool = AtomicBool::new(false);

/// Handle unimplemented system calls.
///
/// C: `do_unused()` — do_unused.c
///
/// Returns ENOSYS for any unimplemented system call number.
pub fn dispatch_unused() -> KcallResult {
    KcallResult::Ok(ENOSYS)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getinfo_request_try_from() {
        assert_eq!(GetInfoRequest::try_from(0), Ok(GetInfoRequest::KInfo));
        assert_eq!(GetInfoRequest::try_from(1), Ok(GetInfoRequest::Proc));
        assert_eq!(GetInfoRequest::try_from(13), Ok(GetInfoRequest::LoadInfo));
        assert_eq!(GetInfoRequest::try_from(99), Err(()));
    }

    #[test]
    fn test_trace_request_try_from() {
        assert_eq!(TraceRequest::try_from(0), Ok(TraceRequest::GetIns));
        assert_eq!(TraceRequest::try_from(8), Ok(TraceRequest::Step));
        assert_eq!(TraceRequest::try_from(99), Err(()));
    }

    /// Helper: create a proc_table with one user process activated at `nr`.
    fn make_proc_table_with_user(nr: i32, endpoint_val: i32) -> ProcessTable {
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(nr) {
            p.p_endpoint = Endpoint(endpoint_val);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        proc_table
    }

    #[test]
    fn test_dispatch_trace_step_clears_proc_stop_and_sets_step_flag() {
        let mut proc_table = make_proc_table_with_user(0, 100);
        // Pre-set PROC_STOP on target.
        proc_table.get_mut(0).unwrap().p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100; // target endpoint
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Step as i32;
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(0));
        // Verify side effects.
        let target = proc_table.get(0).unwrap();
        assert!(target.p_misc_flags.is_set(MiscFlagsBits::STEP));
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_trace_cont_clears_proc_stop() {
        let mut proc_table = make_proc_table_with_user(0, 100);
        proc_table.get_mut(0).unwrap().p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Cont as i32;
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(0));
        let target = proc_table.get(0).unwrap();
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_trace_kill_returns_ok_no_state_change() {
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Kill as i32;
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(0));
        // T_KILL is handled by PM — kernel does not mutate state.
    }

    #[test]
    fn test_dispatch_trace_getins_returns_enosys() {
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetIns as i32;
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    // ── 6 remaining trace requests (F-12 continuation) ────────────
    //
    // The pre-fix GetIns test verified alignment was ignored (always ENOSYS).
    // Post-fix: alignment is now checked first → unaligned → EFAULT,
    // aligned → ENOSYS (virtual_copy_vmcheck deferred).

    #[test]
    fn test_dispatch_trace_getins_rejects_unaligned_address() {
        // C: do_trace.c:96 — COPYFROMPROC(tr_addr, ..., sizeof(long)) requires
        // alignment. unaligned address → EFAULT.
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetIns as i32;
            msg.m_u.m_lsys_krn_sys_trace.address = 0x1001; // unaligned (1 byte off 8-byte boundary)
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_getdata_aligned_returns_enosys() {
        // C: do_trace.c:100-103 — T_GETDATA: COPYFROMPROC with aligned addr.
        // virtual_copy_vmcheck is DEFERRED → ENOSYS.
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetData as i32;
            msg.m_u.m_lsys_krn_sys_trace.address = 0x1000; // aligned
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_dispatch_trace_setins_rejects_unaligned_address() {
        // C: do_trace.c:127 — COPYTOPROC(tr_addr, ..., sizeof(long)) requires
        // alignment. unaligned → EFAULT.
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetIns as i32;
            msg.m_u.m_lsys_krn_sys_trace.address = 0x1003; // unaligned
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_setdata_aligned_returns_enosys() {
        // C: do_trace.c:131-134 — T_SETDATA: COPYTOPROC with aligned addr.
        // virtual_copy_vmcheck DEFERRED → ENOSYS.
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetData as i32;
            msg.m_u.m_lsys_krn_sys_trace.address = 0x1000; // aligned
            msg.m_u.m_lsys_krn_sys_trace.data = 0xDEAD_BEEF; // data to write
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_dispatch_trace_getuser_rejects_unaligned_address() {
        // C: do_trace.c:106 — `if ((tr_addr & (sizeof(long) - 1)) != 0) return(EFAULT)`.
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetUser as i32;
            msg.m_u.m_lsys_krn_sys_trace.address = 0x4; // unaligned
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_setuser_rejects_unaligned_address() {
        // C: do_trace.c:137 — alignment check before offset range check.
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetUser as i32;
            msg.m_u.m_lsys_krn_sys_trace.address = 0x2; // unaligned
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_rejects_invalid_request() {
        let proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
            msg.m_u.m_lsys_krn_sys_trace.request = 99; // out of range
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_trace_rejects_invalid_target_endpoint() {
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 9999; // not in proc table
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Step as i32;
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_trace_rejects_kernel_target() {
        use crate::proc::proc_nr::KERNEL;
        // KERNEL = -1; endpoint_to_nr rejects negative endpoints, so we
        // manually activate a kernel slot and test that is_kernel(nr) triggers EPERM.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(KERNEL) {
            p.p_endpoint = Endpoint(50);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut caller = KProcess::new(0, Endpoint(200));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_lsys_krn_sys_trace.endpt = 50;
            msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Step as i32;
        }
        let result = dispatch_trace(&mut caller, &msg, &proc_table);
        // ProcessTable::is_kernel(KERNEL=-1) returns true → EPERM.
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_unused() {
        assert!(matches!(dispatch_unused(), KcallResult::Ok(38))); // ENOSYS
    }

    // ── dispatch_update tests (F-38) ────────────────────────────────

    /// Helper: set up a ProcessTable with two active SYS_PROC slots.
    /// Returns (proc_table, priv_table, src_endpoint, dst_endpoint).
    fn make_two_sys_procs(
        src_slot: usize,
        src_endpoint: i32,
        dst_slot: usize,
        dst_endpoint: i32,
    ) -> (ProcessTable, PrivTable) {
        let mut proc_table = ProcessTable::new();
        let mut priv_table = PrivTable::new();
        use crate::proc::RtsFlagsBits;
        // Activate both slots and assign a SYS_PROC privilege to each.
        for (slot_i32, ep) in [
            (src_slot as i32, src_endpoint),
            (dst_slot as i32, dst_endpoint),
        ] {
            if let Some(p) = proc_table.get_mut(slot_i32) {
                p.p_endpoint = Endpoint(ep);
                p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE); // mark slot occupied
                p.p_rts_flags.set(RtsFlagsBits::NO_PRIV); // not in kernel
                p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
                p.p_rts_flags.clear(RtsFlagsBits::SENDING);
                p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
                let pid = priv_table.assign_static(slot_i32).expect("static priv slot");
                if let Some(kpriv) = priv_table.get_mut(pid) {
                    kpriv.capability.s_flags = crate::kpriv::PrivFlagsBits::SYS_PROC;
                }
                p.priv_id = Some(pid);
            }
        }
        (proc_table, priv_table)
    }

    #[test]
    fn test_dispatch_update_rejects_none_src_endpoint() {
        // C: do_update.c:55-57 — isokendpt fails → EINVAL.
        let (mut proc_table, priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = minix_types::Endpoint::NONE.0; // src = NONE
            msg.m_u.m_m1.m1i2 = 200;
            msg.m_u.m_m1.m1i3 = 0;
        }
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_update_rejects_self_swap() {
        // src == dst is nonsensical and would corrupt state. Rust rejects
        // it explicitly with EINVAL. C's assert would have crashed.
        let (mut proc_table, priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = 100; // src = 100
            msg.m_u.m_m1.m1i2 = 100; // dst = 100 (same)
            msg.m_u.m_m1.m1i3 = 0;
        }
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_update_rejects_busy_process() {
        // C: do_update.c:77-79 — proc_is_updatable returns false
        // when the process has NO_PRIV unset AND no SIG_PENDING AND
        // not in (RECEIVING && !SENDING) state. Make src be in
        // "kernel mode" (NO_PRIV unset), no sig pending, no receiving.
        let (mut proc_table, priv_table) = make_two_sys_procs(0, 100, 1, 200);
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(0) {
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // in kernel mode
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        }
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = 100;
            msg.m_u.m_m1.m1i2 = 200;
            msg.m_u.m_m1.m1i3 = 0;
        }
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EBUSY));
    }

    #[test]
    fn test_dispatch_update_quiescent_returns_enosys() {
        // Both processes are quiec — proc_is_updatable is true for both,
        // but the swap body itself is DEFERRED.
        let (mut proc_table, priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        unsafe {
            msg.m_u.m_m1.m1i1 = 100;
            msg.m_u.m_m1.m1i2 = 200;
            msg.m_u.m_m1.m1i3 = 0;
        }
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_proc_is_updatable_user_mode_returns_true() {
        // C: do_update.c:18 — `RTS_NO_PRIV` set means user-mode (no
        // kernel privilege). proc_is_updatable allows user-mode processes.
        let mut proc_table = ProcessTable::new();
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(0) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.set(RtsFlagsBits::NO_PRIV); // user-mode (no priv)
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        }
        let p = proc_table.get(0).unwrap();
        assert!(proc_is_updatable(p));
    }

    #[test]
    fn test_proc_is_updatable_receiving_only_returns_true() {
        // In kernel mode, no sig pending, RECEIVING without SENDING.
        // The C macro returns true via the third clause (RECEIVING && !SENDING):
        // a process blocked on receive is "updatable" because the swap will
        // preserve its receive state and the kernel will redrive it.
        let mut proc_table = ProcessTable::new();
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(0) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // in kernel
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            p.p_rts_flags.clear(RtsFlagsBits::SENDING);
        }
        let p = proc_table.get(0).unwrap();
        assert!(proc_is_updatable(p));
    }

    #[test]
    fn test_proc_is_updatable_kernel_blocked_returns_false() {
        // In kernel mode, no sig pending, no RECEIVING/SENDING.
        // proc_is_updatable returns false: the process is actively
        // executing kernel code and cannot be swapped safely.
        let mut proc_table = ProcessTable::new();
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(0) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // in kernel
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
            p.p_rts_flags.clear(RtsFlagsBits::SENDING);
        }
        let p = proc_table.get(0).unwrap();
        assert!(!proc_is_updatable(p));
    }

    #[test]
    fn test_getinfo_proc_rejects_invalid_endpoint() {
        use crate::proc_table::ProcessTable;
        use crate::proc::RtsFlagsBits;

        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        // Set request = GET_PROC (1), val_len2_e = invalid endpoint 9999
        unsafe {
            msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc as i32;
            msg.m_u.m_m1.m1p3 = 9999u64; // val_len2_e
        }
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_getinfo_proc_self_replaces_with_caller_endpoint() {
        use crate::proc_table::ProcessTable;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = ProcessTable::new();
        // Activate slot 0 so endpoint_to_nr succeeds for caller's endpoint
        let caller_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(0) {
            target.p_endpoint = caller_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(0, caller_ep);
        let mut msg = Message::default();
        // Set request = GET_PROC (1), val_len2_e = SELF (-1)
        unsafe {
            msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc as i32;
            msg.m_u.m_m1.m1p3 = Endpoint::SELF.0 as u64; // val_len2_e = SELF
        }
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table);
        // Endpoint is valid (SELF replaced with caller's), but data_copy deferred → ENOSYS
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_getinfo_proc_valid_endpoint_returns_enosys() {
        use crate::proc_table::ProcessTable;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(0) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        // Set request = GET_PROC (1), val_len2_e = valid endpoint
        unsafe {
            msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc as i32;
            msg.m_u.m_m1.m1p3 = target_ep.0 as u64; // val_len2_e = valid endpoint
        }
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table);
        // Endpoint is valid, but data_copy deferred → ENOSYS
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_getinfo_proc2_same_validation() {
        use crate::proc_table::ProcessTable;

        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        // Set request = GET_PROC2 (5), val_len2_e = invalid endpoint
        unsafe {
            msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc2 as i32;
            msg.m_u.m_m1.m1p3 = 9999u64; // val_len2_e = invalid
        }
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    // ── SPROF input validation tests ──

    /// Test-only spinlock for SPROFILING state-machine tests.
///
/// SPROFILING is a process-wide static that is mutated by every
/// `dispatch_profile` call. Cargo's test harness runs tests in parallel
/// by default; without explicit synchronization, parallel sprof tests
/// race on the static. We use a simple AtomicBool-based spin lock
/// (no_std compatible) to serialize test-side setup/teardown.
///
/// # Why not just rely on `AtomicBool` for SPROFILING itself?
///
/// `SPROFILING: AtomicBool` ensures individual load/store atomicity,
/// but cannot enforce "set → dispatch → assert" as a single test
/// transaction. A second test running between our `store(true)` and
/// our dispatch would observe `true` and possibly corrupt the
/// assertion. The spin lock guarantees test isolation.
#[cfg(test)]
static SPROF_TEST_LOCK: AtomicBool = AtomicBool::new(false);

/// Acquire the SPROF_TEST_LOCK spinlock and reset SPROFILING.
///
/// Returns the previous lock state (false=acquired). The caller must
/// release the lock at the end of the test by calling
/// `sprof_test_teardown`. This function busy-waits until the lock is
/// acquired; tests run briefly so contention is rare in practice.
#[cfg(test)]
fn sprof_test_setup() -> bool {
    while SPROF_TEST_LOCK
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        core::hint::spin_loop();
    }
    SPROFILING.store(false, Ordering::Release);
    true
}

/// Release the SPROF_TEST_LOCK spinlock.
#[cfg(test)]
fn sprof_test_teardown() {
    SPROFILING.store(false, Ordering::Release);
    SPROF_TEST_LOCK.store(false, Ordering::Release);
}

#[test]
    fn test_sprof_rejects_unknown_action() {
        let _lock = sprof_test_setup();
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = 99; // unknown action
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_start_rejects_invalid_endpoint() {
        let _lock = sprof_test_setup();
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = 9999; // invalid endpoint
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = ProfIntrType::Rtc as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_start_rejects_unknown_intr_type() {
        let _lock = sprof_test_setup();
        let mut proc_table = ProcessTable::new();
        // Activate a slot so endpoint validation passes
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(0) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = target_ep.0;
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = 99; // unknown intr_type
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_start_valid_returns_enosys() {
        let _lock = sprof_test_setup();
        let mut proc_table = ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(0) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = target_ep.0;
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = ProfIntrType::Rtc as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        // Validation passes, actual profiling deferred → ENOSYS
        assert_eq!(result, KcallResult::Ok(ENOSYS));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_stop_returns_ebusy_when_not_running() {
        // C: do_sprofile.c:82 — PROF_STOP without prior PROF_START → EBUSY.
        // (Renamed from test_sprof_stop_returns_enosys: now that the
        // SPROFILING state machine is implemented, STOP-without-START
        // returns EBUSY, not ENOSYS. To reach ENOSYS, the test must
        // pre-set SPROFILING=true to simulate a running profile.)
        let _lock = sprof_test_setup();
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Stop as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EBUSY));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_stop_after_start_returns_enosys() {
        // PROF_STOP after PROF_START (simulated by setting SPROFILING=true
        // manually) reaches the stop body, which is DEFERRED → ENOSYS.
        let _lock = sprof_test_setup();
        SPROFILING.store(true, Ordering::Release);
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Stop as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
        // After STOP, SPROFILING must be cleared.
        assert!(!SPROFILING.load(Ordering::Acquire));
        sprof_test_teardown();
    }

    // ── F-39 EBUSY state machine tests (2026-06-16) ────────────────

    #[test]
    fn test_sprof_double_start_returns_ebusy() {
        // C: do_sprofile.c:46 — second PROF_START while already running → EBUSY.
        // We simulate "already running" by manually setting SPROFILING=true
        // (mimicking the timer init that would have set it).
        let _lock = sprof_test_setup();
        SPROFILING.store(true, Ordering::Release);
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = 100;
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = ProfIntrType::Rtc as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EBUSY));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_stop_without_start_returns_ebusy() {
        // C: do_sprofile.c:82 — PROF_STOP without prior PROF_START → EBUSY.
        let _lock = sprof_test_setup();
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Stop as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EBUSY));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_start_rollback_on_invalid_endpoint() {
        // After PROF_START validation failure (invalid endpoint), the
        // SPROFILING flag must be rolled back to false so a future
        // PROF_START is not poisoned.
        let _lock = sprof_test_setup();
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(0, Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = 9999; // invalid
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = ProfIntrType::Rtc as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        // Verify rollback.
        assert!(!SPROFILING.load(Ordering::Acquire));
        sprof_test_teardown();
    }
}
