//! Miscellaneous and unported system calls: getinfo, trace, update, profile, unused.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_getinfo.c` — SYS_GETINFO
//! - `do_trace.c` — SYS_TRACE
//! - `do_update.c` — SYS_UPDATE
//! - `do_sprofile.c` — SYS_SPROF
//! - `do_unused.c` — unimplemented system calls
//!
//! # Design Decisions (25-misc-unported.md §3)
//!
//! - **D1**: `GetInfoRequest` enum for GETINFO sub-requests
//! - **D2**: Return ENOSYS for unimplemented calls (matches C)
//! - **D5**: TRACE deferred — debugging feature, not core

use minix_types::{
    Message, MessageM4, MessKrnLsysSysGetwhoami, MessLsysKrnSysGetinfo,
    MessLsysKrnSysTrace, Endpoint, VirBytes,
};
use core::sync::atomic::{AtomicBool, Ordering};
use minix_plat::NR_IRQ_VECTORS;

use crate::proc::{KProcess, MiscFlagsBits, RtsFlagsBits, PROC_NAME_LEN};
#[cfg(test)]
use crate::proc::ProcNr;
use crate::kpriv::PrivTable;
use crate::proc_table::{NR_PROCS, NR_TASKS, ProcessTable};
use crate::syscall::{KcallResult, Syscall};
use crate::cross_space::data_copy_vmcheck;
use crate::vm::{AddressRef, CrossSpaceResult};
use crate::clock::ClockState;
use minix_arch::{CurrentDirectMap, DirectMapArch};

// ── Minix3 error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
// Previously EBUSY=27 (should be 16) and ENOSYS=38 (should be 78) here.
use crate::errno::*;

// ── GETINFO request types ──

/// GETINFO sub-request types.
///
/// C: `GET_*` macros — com.h:316-339.
///
/// Only includes the 19 sub-requests that C `do_getinfo` actually handles
/// (do_getinfo.c:60-207). Macros defined but NOT handled by C —
/// `GET_KADDRESSES=9`, `GET_SCHEDINFO=10`, `GET_LOCKTIMING=13`,
/// `GET_BIOSBUFFER=14`, and undefined value `7` — fall through to C's
/// `default: return EINVAL`. Rust replicates this: values not in the enum
/// yield `TryFrom::Err`, and `dispatch_getinfo` returns `EINVAL`.
///
/// **IPC protocol constraint**: the `#[repr(i32)]` values MUST match the
/// C macro values exactly. User-space libraries (libsys) pass these as
/// raw integers in `m_lsis_krn_sys_getinfo.request`. A mismatch would
/// cause the wrong data to be returned or EINVAL for valid requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GetInfoRequest {
    /// Kernel information structure. C: `GET_KINFO = 0` (com.h:316)
    KInfo = 0,
    /// System boot image table. C: `GET_IMAGE = 1` (com.h:317)
    Image = 1,
    /// Entire kernel process table. C: `GET_PROCTAB = 2` (com.h:318)
    ProcTab = 2,
    /// Randomness buffer (all bins). C: `GET_RANDOMNESS = 3` (com.h:319)
    Randomness = 3,
    /// Boot monitor parameters. C: `GET_MONPARAMS = 4` (com.h:320)
    MonParams = 4,
    /// IRQ hook table. C: `GET_IRQHOOKS = 6` (com.h:322)
    IrqHooks = 6,
    /// Kernel privilege table. C: `GET_PRIVTAB = 8` (com.h:323)
    PrivTab = 8,
    /// Single process slot. C: `GET_PROC = 11` (com.h:326)
    Proc = 11,
    /// Machine information. C: `GET_MACHINE = 12` (com.h:327)
    Machine = 12,
    /// Load average information. C: `GET_LOADINFO = 15` (com.h:330)
    LoadInfo = 15,
    /// IRQ active masks. C: `GET_IRQACTIDS = 16` (com.h:331)
    IrqActids = 16,
    /// Single privilege structure. C: `GET_PRIV = 17` (com.h:332)
    Priv = 17,
    /// System HZ value. C: `GET_HZ = 18` (com.h:333)
    Hz = 18,
    /// Own name, endpoint, and privileges. C: `GET_WHOAMI = 19` (com.h:334)
    WhoAmI = 19,
    /// One randomness bin (by index). C: `GET_RANDOMNESS_BIN = 20` (com.h:335)
    RandomnessBin = 20,
    /// Cumulative idle TSC. C: `GET_IDLETSC = 21` (com.h:336)
    IdleTsc = 21,
    /// Per-CPU information. C: `GET_CPUINFO = 23` (com.h:337)
    CpuInfo = 23,
    /// General process registers. C: `GET_REGS = 24` (com.h:338)
    Regs = 24,
    /// Per-state CPU ticks. C: `GET_CPUTICKS = 25` (com.h:339)
    CpuTicks = 25,
}

impl TryFrom<i32> for GetInfoRequest {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::KInfo),
            1 => Ok(Self::Image),
            2 => Ok(Self::ProcTab),
            3 => Ok(Self::Randomness),
            4 => Ok(Self::MonParams),
            6 => Ok(Self::IrqHooks),
            8 => Ok(Self::PrivTab),
            11 => Ok(Self::Proc),
            12 => Ok(Self::Machine),
            15 => Ok(Self::LoadInfo),
            16 => Ok(Self::IrqActids),
            17 => Ok(Self::Priv),
            18 => Ok(Self::Hz),
            19 => Ok(Self::WhoAmI),
            20 => Ok(Self::RandomnessBin),
            21 => Ok(Self::IdleTsc),
            23 => Ok(Self::CpuInfo),
            24 => Ok(Self::Regs),
            25 => Ok(Self::CpuTicks),
            _ => Err(()),
        }
    }
}

// ── TRACE request types ──

/// TRACE sub-request types.
///
/// C: `T_*` macros — ptrace.h:225-250.
///
/// Only includes the 13 sub-requests that C `do_trace` actually handles
/// (do_trace.c:88-204). Requests handled by the Process Manager (PM)
/// instead of the kernel — `T_OK=0`, `T_EXIT=8`, `T_ATTACH=9`,
/// `T_SETOPT=105`, `T_GETRANGE=106`, `T_SETRANGE=107` — are NOT in this
/// enum. If they reach the kernel syscall path, they fall through to
/// `TryFrom::Err → EINVAL`, matching C's `default: return(EINVAL)`.
///
/// **IPC protocol constraint**: the `#[repr(i32)]` values MUST match the
/// C `T_*` macro values exactly. User-space `ptrace()` passes these as
/// raw integers in `m_lsis_krn_sys_trace.request`. A mismatch would
/// cause the wrong trace operation to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TraceRequest {
    /// Stop the process. C: `T_STOP = -1` (ptrace.h:238)
    Stop = -1,
    /// Return value from instruction space. C: `T_GETINS = PT_READ_I = 1` (ptrace.h:227)
    GetIns = 1,
    /// Return value from data space. C: `T_GETDATA = PT_READ_D = 2` (ptrace.h:228)
    GetData = 2,
    /// Set value in instruction space. C: `T_SETINS = PT_WRITE_I = 4` (ptrace.h:229)
    SetIns = 4,
    /// Set value in data space. C: `T_SETDATA = PT_WRITE_D = 5` (ptrace.h:230)
    SetData = 5,
    /// Resume execution. C: `T_RESUME = PT_CONTINUE = 7` (ptrace.h:231)
    Resume = 7,
    /// Detach tracer. C: `T_DETACH = PT_DETACH = 10` (ptrace.h:235)
    Detach = 10,
    /// Trace system call. C: `T_SYSCALL = PT_SYSCALL = 14` (ptrace.h:233)
    Syscall = 14,
    /// Read a byte from instruction space (untraced, root-only).
    /// C: `T_READB_INS = 100` (ptrace.h:239)
    ReadBIns = 100,
    /// Write a byte in instruction space (untraced, root-only).
    /// C: `T_WRITEB_INS = 101` (ptrace.h:242)
    WriteBIns = 101,
    /// Return value from user process table. C: `T_GETUSER = 102` (ptrace.h:245)
    GetUser = 102,
    /// Set value in user process table. C: `T_SETUSER = 103` (ptrace.h:246)
    SetUser = 103,
    /// Set trace bit (single-step). C: `T_STEP = 104` (ptrace.h:247)
    Step = 104,
}

impl TryFrom<i32> for TraceRequest {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            -1 => Ok(Self::Stop),
            1 => Ok(Self::GetIns),
            2 => Ok(Self::GetData),
            4 => Ok(Self::SetIns),
            5 => Ok(Self::SetData),
            7 => Ok(Self::Resume),
            10 => Ok(Self::Detach),
            14 => Ok(Self::Syscall),
            100 => Ok(Self::ReadBIns),
            101 => Ok(Self::WriteBIns),
            102 => Ok(Self::GetUser),
            103 => Ok(Self::SetUser),
            104 => Ok(Self::Step),
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
    msg.debug_check_m_type_any(&[Syscall::Trace as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_krn_sys_trace }
}

/// Write the reply `data` field into the trace message.
///
/// C: `m_ptr->m_krn_lsis_sys_trace.data = <value>` — do_trace.c uses the
/// `m_krn_lsis_sys_trace` union member for the reply. In the Rust message
/// union, `m_lsys_krn_sys_trace` and `m_krn_lsis_sys_trace` occupy the same
/// memory (C union semantics). The `data` field is at offset 16 in both
/// layouts, so writing to `m_lsys_krn_sys_trace.data` is equivalent to
/// writing to C's `m_krn_lsis_sys_trace.data`.
///
/// # Safety
///
/// `m_type == SYS_TRACE` guarantees the `m_lsys_krn_sys_trace` variant is
/// active. `#[repr(C)]` union write is sound.
#[inline]
fn write_trace_reply_data(msg: &mut Message, value: i64) {
    // SAFETY: `m_type == SYS_TRACE` guarantees the `m_lsys_krn_sys_trace`
    // variant is active. `#[repr(C)]` union write is sound.
    msg.m_u.m_lsys_krn_sys_trace.data = value;
}

/// Read a `u64` (C `long`) from a `#[repr(C)]` struct at a byte offset.
///
/// Used by `T_GETUSER` to read a word from a `ProcInfoStruct` snapshot at
/// the byte offset specified by `tr_addr`. The caller MUST validate that
/// `offset` is aligned to `size_of::<u64>()` and that
/// `offset + size_of::<u64>() <= size_of::<T>()`.
///
/// # Safety
///
/// The caller guarantees bounds and alignment. The struct is `#[repr(C)]`
/// so the layout is deterministic. `core::ptr::read` (not `read_unaligned`)
/// is sound because the caller checks `tr_addr & (sizeof(long)-1) == 0`
/// before invoking this helper (mirrors C's alignment check at
/// do_trace.c:106).
fn read_word_at_offset<T>(val: &T, offset: usize) -> u64 {
    let base = val as *const T as *const u8;
    // SAFETY: caller validates `offset` is aligned to `size_of::<u64>()`
    // and within bounds of the struct.
    unsafe {
        let ptr = base.add(offset) as *const u64;
        core::ptr::read(ptr)
    }
}

/// Read getinfo fields from a message.
/// C: `m_ptr->m_lsys_krn_sys_getinfo.*` — uses mess_lsys_krn_sys_getinfo union member.
///
/// **IMPORTANT**: Do NOT use `msg_m1` for SYS_GETINFO. The `mess_lsys_krn_sys_getinfo`
/// layout (request@0, endpt@4, val_ptr@8, val_len@16) differs from `MessageM1`
/// (m1i1@0, m1i2@4, m1i3@8, m1p1@16). Using `m1.m1p1` for `val_ptr` reads
/// `val_len` instead — a P1 field-mapping bug. Always use `msg_getinfo`.
fn msg_getinfo(msg: &Message) -> MessLsysKrnSysGetinfo {
    msg.debug_check_m_type_any(&[Syscall::Getinfo as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_lsys_krn_sys_getinfo }
}

// ── GETINFO C-compatible structs ──

/// C: `struct loadinfo` — type.h:98-102
/// `_LOAD_HISTORY` = 180 (15 minutes / 5 seconds per slot)
#[repr(C)]
#[derive(Clone, Copy)]
struct LoadInfoStruct {
    /// C: `proc_load_history[_LOAD_HISTORY]` — u16[180]
    proc_load_history: [u16; 180],
    /// C: `proc_last_slot`
    proc_last_slot: u16,
    /// C: `last_clock` — clock_t (u64 on 64-bit)
    last_clock: u64,
}

/// C: `struct machine` — type.h:122-131
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MachineStruct {
    /// C: `processors_count`
    processors_count: u32,
    /// C: `bsp_id`
    bsp_id: u32,
    /// C: `padding`
    padding: i32,
    /// C: `apic_enabled`
    apic_enabled: i32,
    /// C: `acpi_rsdp` — phys_bytes (u64)
    acpi_rsdp: u64,
    /// C: `board_id`
    board_id: u32,
}

/// C: `struct cpuinfo` — type.h:146-159
/// Per-CPU info entry.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CpuInfoEntry {
    /// C: `cpu_id` — CPU identifier
    cpu_id: u32,
    /// C: `cpu_cycles` — 64-bit cycle counter
    cpu_cycles: u64,
    /// C: `cpu_load` — load percentage
    cpu_load: u32,
}

/// C-compatible process info structure exposed by GET_PROC / GET_PROCTAB.
///
/// # Design
///
/// minix-rs is a complete Rust rewrite — this struct does NOT mirror C's
/// `struct proc` field-by-field (which contains pointers, intrusive linked
/// list nodes, and arch-private register state). Instead, it exposes the
/// user-visible fields that IS/MIB/VM/PM actually consume:
/// - `p_endpoint`, `p_nr`, `p_name` — identification
/// - `p_rts_flags`, `p_misc_flags` — state
/// - `p_priority`, `p_cpu_time_left`, `p_quantum_size_ms` — scheduling
/// - `p_cpu` — CPU affinity
/// - `p_user_time`, `p_sys_time`, `p_cycles` — accounting
/// - `p_pending` — signals
/// - `p_getfrom_e`, `p_sendto_e` — IPC state
///
/// C: `struct proc` — kernel/proc.h:22-137
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcInfoStruct {
    /// C: `p_nr` — process number (slot index).
    pub p_nr: i32,
    /// C: `p_endpoint` — endpoint identifier.
    pub p_endpoint: i32,
    /// C: `p_rts_flags` — runtime status flags.
    pub p_rts_flags: u32,
    /// C: `p_misc_flags` — miscellaneous flags.
    pub p_misc_flags: u32,
    /// C: `p_priority` — current scheduling priority.
    pub p_priority: i8,
    /// C: `p_cpu` — CPU the process is running on.
    pub p_cpu: u32,
    /// C: `p_quantum_size_ms` — time quantum in milliseconds.
    pub p_quantum_size_ms: u32,
    /// C: `p_cpu_time_left` — CPU time remaining (ticks).
    pub p_cpu_time_left: u64,
    /// C: `p_user_time` — user time in ticks.
    pub p_user_time: u64,
    /// C: `p_sys_time` — system time in ticks.
    pub p_sys_time: u64,
    /// C: `p_cycles` — cycles consumed.
    pub p_cycles: u64,
    /// C: `p_pending` — pending signal bitmap.
    pub p_pending: u64,
    /// C: `p_getfrom_e` — endpoint to receive from.
    pub p_getfrom_e: i32,
    /// C: `p_sendto_e` — endpoint to send to.
    pub p_sendto_e: i32,
    /// C: `p_name` — process name (16 bytes including NUL).
    pub p_name: [u8; 16],
    /// C: `p_priv` (index) — privilege table index (replaces pointer).
    /// -1 = no privilege assigned.
    pub p_priv_id: i32,
    /// Padding to align the struct to 8 bytes.
    pub _padding: [u8; 4],
}

impl ProcInfoStruct {
    /// Build a ProcInfoStruct from a KProcess.
    /// C: `proc_addr(nr)` → struct proc (kernel builds the struct in-place;
    /// Rust builds a snapshot because KProcess layout differs from C).
    fn from_kprocess(p: &crate::proc::KProcess) -> Self {
        use core::sync::atomic::Ordering;
        Self {
            p_nr: p.p_nr.0,
            p_endpoint: p.p_endpoint.0,
            p_rts_flags: p.p_rts_flags.load(),
            p_misc_flags: p.p_misc_flags.load(),
            p_priority: p.p_sched.priority.load(Ordering::Acquire) as i8,
            p_cpu: p.p_sched.cpu.load(Ordering::Acquire),
            p_quantum_size_ms: p.p_sched.quantum.size_ms.load(Ordering::Acquire),
            p_cpu_time_left: p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire),
            p_user_time: p.p_time.user_time.load(Ordering::Acquire),
            p_sys_time: p.p_time.sys_time.load(Ordering::Acquire),
            p_cycles: p.p_cycles.total.load(Ordering::Acquire),
            p_pending: p.p_pending.get(),
            p_getfrom_e: p.p_getfrom_e.0,
            p_sendto_e: p.p_sendto_e.0,
            p_name: *p.p_name.as_bytes(),
            p_priv_id: p.priv_id.map(|id| id as i32).unwrap_or(-1),
            _padding: [0; 4],
        }
    }
}

impl Default for ProcInfoStruct {
    fn default() -> Self {
        Self {
            p_nr: 0,
            p_endpoint: 0,
            p_rts_flags: 0,
            p_misc_flags: 0,
            p_priority: 0,
            p_cpu: 0,
            p_quantum_size_ms: 0,
            p_cpu_time_left: 0,
            p_user_time: 0,
            p_sys_time: 0,
            p_cycles: 0,
            p_pending: 0,
            p_getfrom_e: 0,
            p_sendto_e: 0,
            p_name: [0; 16],
            p_priv_id: -1,
            _padding: [0; 4],
        }
    }
}

/// C-compatible privilege info structure exposed by GET_PRIV / GET_PRIVTAB.
///
/// # Design
///
/// Like `ProcInfoStruct`, this struct does NOT mirror C's `struct priv`
/// field-by-field. It exposes the user-visible fields that IS/MIB/PM
/// consume:
/// - `s_proc_nr`, `s_id`, `s_flags` — identity/capability
/// - `s_trap_mask`, `s_ipc_to`, `s_k_call_mask` — IPC allowlists
/// - `s_sig_mgr`, `s_notify_pending`, `s_sig_pending` — signals
/// - `s_grant_entries` — grant table
///
/// C: `struct priv` — include/minix/priv.h:76-110
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PrivInfoStruct {
    /// C: `s_proc_nr` — process number this privilege belongs to.
    pub s_proc_nr: i32,
    /// C: `s_id` — privilege ID.
    pub s_id: i32,
    /// C: `s_flags` — privilege flags (s_flags).
    pub s_flags: u32,
    /// C: `s_trap_mask` — allowed traps.
    pub s_trap_mask: u32,
    /// C: `s_grant_entries` — number of grant entries.
    pub s_grant_entries: i32,
    /// C: `s_sig_mgr` — signal manager endpoint.
    pub s_sig_mgr: i32,
    /// C: `s_notify_pending` — pending notifications.
    pub s_notify_pending: u64,
    /// C: `s_sig_pending` — pending signals.
    pub s_sig_pending: u64,
    /// C: `s_ipc_to` — bitmap of endpoints allowed to send to.
    pub s_ipc_to: u64,
    /// C: `s_k_call_mask` — allowed kernel calls (SYS_CALL_MASK_SIZE=2).
    pub s_k_call_mask: [u32; crate::kpriv::SYS_CALL_MASK_SIZE],
}

impl PrivInfoStruct {
    /// Build a PrivInfoStruct from a KPriv.
    fn from_kpriv(p: &crate::kpriv::KPriv) -> Self {
        Self {
            s_proc_nr: p.capability.s_proc_nr.map(|nr| nr.0).unwrap_or(-1),
            s_id: p.capability.s_id as i32,
            s_flags: p.capability.s_flags.bits() as u32,
            s_trap_mask: p.ipc.s_trap_mask as u32,
            s_grant_entries: p.runtime.s_grant_entries,
            s_sig_mgr: p.signals.s_sig_mgr.0,
            s_notify_pending: p.signals.s_notify_pending,
            s_sig_pending: p.signals.s_sig_pending.get(),
            s_ipc_to: p.ipc.s_ipc_to,
            s_k_call_mask: p.ipc.s_k_call_mask,
        }
    }
}

impl Default for PrivInfoStruct {
    fn default() -> Self {
        Self {
            s_proc_nr: -1,
            s_id: 0,
            s_flags: 0,
            s_trap_mask: 0,
            s_grant_entries: 0,
            s_sig_mgr: 0,
            s_notify_pending: 0,
            s_sig_pending: 0,
            s_ipc_to: 0,
            s_k_call_mask: [0; crate::kpriv::SYS_CALL_MASK_SIZE],
        }
    }
}

/// Number of per-state CPU tick counters.
/// C: `#define MINIX_CPUSTATES 5` — const.h:176
const MINIX_CPUSTATES: usize = 5;

// ── C-compatible IRQ hook struct (GET_IRQHOOKS) ──

/// C-compatible IRQ hook structure exposed by GET_IRQHOOKS.
///
/// C: `struct irq_hook` — kernel/type.h:18-26
///
/// # Layout (64-bit)
///
/// ```text
/// offset  field       type           size
/// 0       next        *mut           8    (pointer: next hook in chain)
/// 8       handler     *mut           8    (function pointer: handler)
/// 16      irq         i32            4    (IRQ vector number)
/// 20      id          i32            4    (id of this hook)
/// 24      proc_nr_e   i32            4    (endpoint; NONE if not in use)
/// 28      padding     [u8;4]         4    (align notify_id to 8)
/// 32      notify_id   u64            8    (irq_id_t = unsigned long)
/// 40      policy      u64            8    (irq_policy_t = unsigned long)
/// total                                48 bytes
/// ```
///
/// # Design
///
/// Unlike `ProcInfoStruct` (which is a semantic snapshot), this struct
/// mirrors the C `struct irq_hook` field-by-field because user-space tools
/// (IS server's `dmp_kernel.c`) interpret the raw bytes via the C layout.
/// The `next` and `handler` pointer fields are exported as raw addresses
/// (0 for NULL); user-space tools only read the non-pointer fields.
#[repr(C)]
#[derive(Clone, Copy)]
struct IrqHookStruct {
    /// C: `next` — pointer to next hook in chain. Exported as 0 (NULL)
    /// because Rust uses index-based linked lists, not pointers. User-space
    /// tools do not dereference this field.
    next: u64,
    /// C: `handler` — function pointer to interrupt handler. Exported as 0
    /// because Rust uses `fn` pointers stored in the IrqManager, not in the
    /// exported struct. User-space tools do not call this field.
    handler: u64,
    /// C: `irq` — IRQ vector number.
    irq: i32,
    /// C: `id` — id of this hook (bit position in irq_actids).
    id: i32,
    /// C: `proc_nr_e` — owning process endpoint (NONE if not in use).
    proc_nr_e: i32,
    /// Padding to align `notify_id` to 8 bytes (matches C ABI on 64-bit).
    _pad0: [u8; 4],
    /// C: `notify_id` — id to return on interrupt (irq_id_t = unsigned long).
    notify_id: u64,
    /// C: `policy` — bit mask for policy (irq_policy_t = unsigned long).
    policy: u64,
}

impl Default for IrqHookStruct {
    fn default() -> Self {
        Self {
            next: 0,
            handler: 0,
            irq: -1,
            id: 0,
            proc_nr_e: Endpoint::NONE.0,
            _pad0: [0; 4],
            notify_id: 0,
            policy: 0,
        }
    }
}

// ── C-compatible boot image struct (GET_IMAGE) ──

/// C-compatible boot image structure exposed by GET_IMAGE.
///
/// C: `struct boot_image` — include/minix/type.h:148-154
///
/// # Layout (64-bit)
///
/// ```text
/// offset  field       type           size
/// 0       proc_nr     i32            4
/// 4       proc_name   [u8;16]        16
/// 20      endpoint    i32            4
/// 24      start_addr  u64            8    (phys_bytes, 8-byte aligned)
/// 32      len         u64            8    (phys_bytes)
/// total                                40 bytes
/// ```
///
/// # Design
///
/// In C, `image[]` is a static table initialized in `table.c` describing
/// the boot-time processes (CLOCK, IDLE, KERNEL, PM, VM, etc.). In Rust,
/// this information is built from the process table snapshot + boot modules
/// at syscall time. The `start_addr` and `len` fields come from
/// `KernelInfo.boot_modules`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BootImageStruct {
    /// C: `proc_nr` — process number to use.
    proc_nr: i32,
    /// C: `proc_name[PROC_NAME_LEN]` — name in process table (16 bytes).
    proc_name: [u8; 16],
    /// C: `endpoint` — endpoint number when started.
    endpoint: i32,
    /// C: `start_addr` — physical address where the process image is in memory.
    /// 8-byte aligned (phys_bytes = u64 on 64-bit).
    start_addr: u64,
    /// C: `len` — length of the process image in bytes.
    len: u64,
}

/// Copy a kernel-stack struct to the caller's `val_ptr` via `data_copy_vmcheck`.
///
/// Implements the C `do_getinfo` common tail (do_getinfo.c:209-217): the
/// `val_len` E2BIG check followed by the kernel→user `data_copy_vmcheck`. The
/// source is a kernel direct-mapped local; the destination is the caller's
/// user-space `val_ptr`.
///
/// Returns `OK` on success, `EFAULT` on address error, `VmSuspend` if the
/// caller's destination page is not yet faulted in.
fn copy_struct_to_caller<T>(
    caller: &mut KProcess,
    data: &T,
    val_ptr: u64,
    val_len: i32,
) -> KcallResult {
    // C: do_getinfo.c:210-212 — if val_len > 0 and length > val_len, return E2BIG
    let length = core::mem::size_of::<T>();
    if val_len > 0 && (length as i64) > (val_len as i64) {
        return KcallResult::Ok(E2BIG);
    }
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let src_phys = CurrentDirectMap::virt_to_phys(VirBytes(data as *const T as u64));
    let proc_cr3 = |endpt: Endpoint| {
        if endpt == caller_endpt { Some(caller_cr3) } else { None }
    };
    let src = AddressRef::Physical(src_phys);
    let dst = AddressRef::Process {
        endpoint: caller_endpt,
        offset: VirBytes(val_ptr),
    };
    match data_copy_vmcheck(caller, src, dst, length, proc_cr3) {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

// ── Dispatch functions ──

/// Dispatch SYS_GETINFO.
///
/// C: `do_getinfo()` — do_getinfo.c
///
/// Query kernel information. Various sub-requests return different data.
pub fn dispatch_getinfo(caller: &mut KProcess, msg: &mut Message, priv_table: &PrivTable, proc_table: &ProcessTable, clock_state: &ClockState) -> KcallResult {
    let gi = msg_getinfo(msg);
    // C: do_getinfo.c:22-24 — extract parameters
    let request = gi.request;       // m_lsys_krn_sys_getinfo.request
    let val_ptr = gi.val_ptr;       // m_lsys_krn_sys_getinfo.val_ptr
    let val_len = gi.val_len;       // m_lsys_krn_sys_getinfo.val_len
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
            msg.m_u.m_krn_lsys_sys_getwhoami = MessKrnLsysSysGetwhoami {
                endpt: caller.p_endpoint.get(),
                privflags,
                initflags,
                name: name_buf,
            };
            KcallResult::Ok(OK)
        }
        GetInfoRequest::KInfo => {
            // C: do_getinfo.c:41-70 — build kinfo structure
            // C copies the entire `struct kinfo` via data_copy_vmcheck.
            // Rust: Return key fields in the reply message (m_m4 format).
            // Full data_copy_vmcheck path: `data_copy_vmcheck` is available,
            // but `struct kinfo` C-compatible layout + conversion is not yet
            // designed. Key fields are returned in the reply message instead.
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
            msg.m_u.m_m4 = MessageM4 {
                m4l1: NR_PROCS as i64,
                m4l2: NR_TASKS as i64,
                m4l3: user_sp,
                m4l4: freepde_start,
                m4l5: vir_kern_start,
                _padding: [0u8; 16],
            };
            KcallResult::Ok(OK)
        }
        GetInfoRequest::Proc => {
            // C: do_getinfo.c:107-114 — copy single process table entry.
            // C: nr_e = (val_len2_e == SELF) ? caller->p_endpoint : val_len2_e
            // C: if(!isokendpt(nr_e, &nr)) return EINVAL
            let target_ep = if val_len2_e == minix_types::Endpoint::SELF.0 {
                caller.p_endpoint.0
            } else {
                val_len2_e
            };
            let target_nr = match proc_table.endpoint_to_nr(Endpoint(target_ep)) {
                Some(nr) => nr,
                None => return KcallResult::Ok(EINVAL),
            };
            // Build a snapshot of the KProcess's user-visible fields, then
            // release the proc_table borrow before data_copy_vmcheck borrows
            // caller (caller and proc_table are separate parameters, but
            // building the owned snapshot first keeps the borrow flow clean).
            let info = proc_table.get(target_nr)
                .map(ProcInfoStruct::from_kprocess)
                .unwrap_or_default();
            copy_struct_to_caller(caller, &info, val_ptr, val_len)
        }
        GetInfoRequest::ProcTab => {
            // C: do_getinfo.c:102-130 — copy entire process table.
            // NR_PROCS + NR_TASKS entries; each copied separately to avoid
            // a large stack buffer (261 * sizeof(ProcInfoStruct) ≈ 27KB,
            // which would overflow the typical 8-16KB kernel stack).
            let total = NR_PROCS + NR_TASKS;
            let elem_size = core::mem::size_of::<ProcInfoStruct>();
            let length = total * elem_size;
            if val_len > 0 && (length as i64) > (val_len as i64) {
                return KcallResult::Ok(E2BIG);
            }
            let caller_endpt = caller.p_endpoint;
            let caller_cr3 = caller.p_seg.phys_root;
            let proc_cr3 = |endpt: Endpoint| {
                if endpt == caller_endpt { Some(caller_cr3) } else { None }
            };
            for i in 0..total {
                // Build the snapshot; the proc_table borrow ends here.
                let info = proc_table.get_by_index(i)
                    .map(ProcInfoStruct::from_kprocess)
                    .unwrap_or_default();
                let src_phys = CurrentDirectMap::virt_to_phys(
                    VirBytes(&info as *const ProcInfoStruct as u64),
                );
                let src = AddressRef::Physical(src_phys);
                let dst = AddressRef::Process {
                    endpoint: caller_endpt,
                    offset: VirBytes(val_ptr + (i * elem_size) as u64),
                };
                match data_copy_vmcheck(caller, src, dst, elem_size, proc_cr3) {
                    CrossSpaceResult::Completed(Ok(())) => continue,
                    CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                    CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                }
            }
            KcallResult::Ok(OK)
        }
        GetInfoRequest::PrivTab => {
            // C: do_getinfo.c:132-150 — copy privilege table.
            // NR_SYS_PROCS entries; each copied separately (same chunked
            // approach as GET_PROCTAB to keep stack usage bounded).
            let total = crate::kpriv::NR_SYS_PROCS;
            let elem_size = core::mem::size_of::<PrivInfoStruct>();
            let length = total * elem_size;
            if val_len > 0 && (length as i64) > (val_len as i64) {
                return KcallResult::Ok(E2BIG);
            }
            let caller_endpt = caller.p_endpoint;
            let caller_cr3 = caller.p_seg.phys_root;
            let proc_cr3 = |endpt: Endpoint| {
                if endpt == caller_endpt { Some(caller_cr3) } else { None }
            };
            for i in 0..total {
                let info = priv_table.get(i as crate::kpriv::PrivId)
                    .map(PrivInfoStruct::from_kpriv)
                    .unwrap_or_default();
                let src_phys = CurrentDirectMap::virt_to_phys(
                    VirBytes(&info as *const PrivInfoStruct as u64),
                );
                let src = AddressRef::Physical(src_phys);
                let dst = AddressRef::Process {
                    endpoint: caller_endpt,
                    offset: VirBytes(val_ptr + (i * elem_size) as u64),
                };
                match data_copy_vmcheck(caller, src, dst, elem_size, proc_cr3) {
                    CrossSpaceResult::Completed(Ok(())) => continue,
                    CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                    CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                }
            }
            KcallResult::Ok(OK)
        }
        GetInfoRequest::LoadInfo => {
            // C: do_getinfo.c:71-75 — copy load info
            let history = clock_state.load_history();
            let mut loadinfo = LoadInfoStruct {
                proc_load_history: [0u16; 180],
                proc_last_slot: 0,
                last_clock: clock_state.uptime(),
            };
            // Copy available history entries (Rust keeps 12; C ABI expects 180).
            let n = history.len().min(180);
            for (i, hist_val) in history.iter().take(n).enumerate() {
                loadinfo.proc_load_history[i] = *hist_val as u16;
            }
            copy_struct_to_caller(caller, &loadinfo, val_ptr, val_len)
        }
        GetInfoRequest::Priv => {
            // C: do_getinfo.c:115-122 — copy single privilege structure.
            // Same endpoint validation as GET_PROC.
            let target_ep = if val_len2_e == minix_types::Endpoint::SELF.0 {
                caller.p_endpoint.0
            } else {
                val_len2_e
            };
            let target_nr = match proc_table.endpoint_to_nr(Endpoint(target_ep)) {
                Some(nr) => nr,
                None => return KcallResult::Ok(EINVAL),
            };
            // Resolve process → priv_id → KPriv, then build a snapshot.
            // The chain releases each borrow before the next (priv_id is
            // Copy, so the proc_table borrow ends before priv_table is touched).
            let info = proc_table.get(target_nr)
                .and_then(|p| p.priv_id)
                .and_then(|pid| priv_table.get(pid))
                .map(PrivInfoStruct::from_kpriv)
                .unwrap_or_default();
            copy_struct_to_caller(caller, &info, val_ptr, val_len)
        }
        GetInfoRequest::Regs => {
            // C: do_getinfo.c:123-131 — copy general process registers (p_reg).
            // The register state is arch-private (CurrentCpuContext). Expose
            // it as raw bytes, matching C's `sizeof(p->p_reg)` behavior.
            let target_ep = if val_len2_e == minix_types::Endpoint::SELF.0 {
                caller.p_endpoint.0
            } else {
                val_len2_e
            };
            let target_nr = match proc_table.endpoint_to_nr(Endpoint(target_ep)) {
                Some(nr) => nr,
                None => return KcallResult::Ok(EINVAL),
            };
            // Extract the physical address and size of cpu_context before
            // calling data_copy_vmcheck. The cpu_context lives in the
            // process table (a stable, direct-mapped allocation), so the
            // physical address remains valid across a VmSuspend retry —
            // unlike a stack-local source, this is safe to resume from.
            let (reg_phys, reg_size) = match proc_table.get(target_nr) {
                Some(p) => {
                    let size = core::mem::size_of_val(&p.cpu_context);
                    let phys = CurrentDirectMap::virt_to_phys(
                        VirBytes(&p.cpu_context as *const _ as u64),
                    );
                    (phys, size)
                }
                None => return KcallResult::Ok(EINVAL),
            };
            if val_len > 0 && (reg_size as i64) > (val_len as i64) {
                return KcallResult::Ok(E2BIG);
            }
            let caller_endpt = caller.p_endpoint;
            let caller_cr3 = caller.p_seg.phys_root;
            let proc_cr3 = |endpt: Endpoint| {
                if endpt == caller_endpt { Some(caller_cr3) } else { None }
            };
            let src = AddressRef::Physical(reg_phys);
            let dst = AddressRef::Process {
                endpoint: caller_endpt,
                offset: VirBytes(val_ptr),
            };
            match data_copy_vmcheck(caller, src, dst, reg_size, proc_cr3) {
                CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
                CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
                CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
            }
        }
        GetInfoRequest::CpuTicks => {
            // C: do_getinfo.c:192-202 — per-state CPU ticks.
            // val_len2_e is the CPU index, not an endpoint.
            let cpu = val_len2_e as u32;
            // C: if (cpu >= CONFIG_MAX_CPUS) return EINVAL
            // CONFIG_MAX_CPUS is typically 1 or small; use a conservative bound.
            if cpu >= 256 {
                return KcallResult::Ok(EINVAL);
            }
            // C: get_cpu_ticks(cpu, ticks) — per-state tick counter.
            // Rust does not yet expose per-CPU tick accounting; return zeros
            // (the array is zero-initialized) until get_cpu_ticks is wired.
            let ticks: [u64; MINIX_CPUSTATES] = [0; MINIX_CPUSTATES];
            copy_struct_to_caller(caller, &ticks, val_ptr, val_len)
        }
        GetInfoRequest::Hz => {
            // C: do_getinfo.c:81-84 — copy system_hz
            let hz: i32 = clock_state.system_hz();
            copy_struct_to_caller(caller, &hz, val_ptr, val_len)
        }
        GetInfoRequest::Machine => {
            // C: do_getinfo.c:61-65 — copy machine info
            // SAFETY: BKL is held by kernel_call_dispatch (syscall.rs:245).
            // try_smp_state() returns None before boot init (e.g. in unit
            // tests); fall back to single-CPU defaults in that case.
            let smp = unsafe { crate::try_smp_state() };
            let machine = MachineStruct {
                processors_count: smp.as_ref().map(|s| s.ncpus()).unwrap_or(1),
                bsp_id: smp.as_ref().map(|s| s.bsp_cpu_id().raw()).unwrap_or(0),
                ..Default::default()
            };
            copy_struct_to_caller(caller, &machine, val_ptr, val_len)
        }
        GetInfoRequest::CpuInfo => {
            // C: do_getinfo.c:76-80 — copy per-CPU info array
            // Build the full CONFIG_MAX_CPUS array (C copies sizeof(cpu_info)).
            let mut cpuinfo: [CpuInfoEntry; crate::smp::MAX_CPUS] =
                [CpuInfoEntry::default(); crate::smp::MAX_CPUS];
            // SAFETY: BKL held; see GET_MACHINE above.
            let ncpus = unsafe { crate::try_smp_state() }
                .as_ref()
                .map(|s| s.ncpus())
                .unwrap_or(1);
            for (i, entry) in cpuinfo.iter_mut().take(ncpus as usize).enumerate() {
                entry.cpu_id = i as u32;
            }
            copy_struct_to_caller(caller, &cpuinfo, val_ptr, val_len)
        }
        GetInfoRequest::IrqActids => {
            // C: do_getinfo.c:179-183 — copy irq_actids[] array.
            //
            // C: `sys_datacopy_check(caller_ptr, val_ptr, KERNEL, (vir_bytes)
            //     irq_actids, sizeof(irq_actids))` where `irq_actids` is a
            //     `u32_t[NR_IRQ_VECTORS]` global (glo.h:49).
            //
            // We snapshot the global `IRQ_MANAGER.actids` under BKL and copy
            // it to the caller. If `IRQ_MANAGER` is not yet initialized
            // (e.g. in unit tests that skip boot), return EINVAL — matching
            // C's behavior of not having the data available.
            //
            // SAFETY: BKL is held by kernel_call_dispatch (syscall.rs:245).
            // try_irq_manager() returns None before boot init (e.g. in unit
            // tests); we return EINVAL in that case.
            let actids_snapshot: [u32; NR_IRQ_VECTORS] = match unsafe { crate::try_irq_manager() } {
                Some(mgr) => {
                    let src = mgr.irq_actids();
                    // src.len() is always NR_IRQ_VECTORS by construction;
                    // the assert catches a future size mismatch if the
                    // IrqManager layout ever diverges from this constant.
                    debug_assert_eq!(src.len(), NR_IRQ_VECTORS);
                    let mut arr = [0u32; NR_IRQ_VECTORS];
                    arr.copy_from_slice(src);
                    arr
                }
                None => return KcallResult::Ok(EINVAL),
            };
            copy_struct_to_caller(caller, &actids_snapshot, val_ptr, val_len)
        }
        GetInfoRequest::IdleTsc => {
            // C: do_getinfo.c:184-191 — copy IDLE process's p_cycles.
            //
            // C: `update_idle_time()` resets `idl->p_cycles = 0` then sums
            // all CPUs' `idle_proc.p_cycles`. In single-CPU Rust, we read
            // the IDLE process's `p_cycles.total` directly.
            //
            // For SMP, each CPU's idle_proc is a separate KProcess slot;
            // summing would require iterating CpuLocal. Since the current
            // build is single-CPU, we read the single IDLE slot.
            let idle_cycles: u64 = proc_table
                .get(crate::proc::proc_nr::IDLE)
                .map(|p| p.p_cycles.total.load(core::sync::atomic::Ordering::Acquire))
                .unwrap_or(0);
            copy_struct_to_caller(caller, &idle_cycles, val_ptr, val_len)
        }
        GetInfoRequest::Randomness => {
            // C: do_getinfo.c:148-160 — copy entire krandom struct, then
            // wipe all bins.
            //
            // C uses a static `copy` variable to preserve counters while
            // wiping the original. We snapshot under BKL, then wipe.
            //
            // SAFETY: BKL is held by kernel_call_dispatch (syscall.rs:245).
            let krandom_snapshot = match unsafe { crate::krandom::try_krandom() } {
                Some(kr) => {
                    let snapshot = *kr;
                    kr.wipe_all();
                    snapshot
                }
                None => return KcallResult::Ok(EINVAL),
            };
            copy_struct_to_caller(caller, &krandom_snapshot, val_ptr, val_len)
        }
        GetInfoRequest::RandomnessBin => {
            // C: do_getinfo.c:161-178 — copy one randomness bin by index,
            // then wipe that bin after successful copy.
            //
            // val_len2_e is the bin index (not an endpoint).
            let bin = val_len2_e;
            // C: if(bin < 0 || bin >= RANDOM_SOURCES) return EINVAL
            // RANDOM_SOURCES = 16 (include/minix/type.h:182).
            if bin < 0 || bin >= crate::krandom::RANDOM_SOURCES as i32 {
                return KcallResult::Ok(EINVAL);
            }
            // SAFETY: BKL is held by kernel_call_dispatch (syscall.rs:245).
            let bin_snapshot = match unsafe { crate::krandom::try_krandom() } {
                Some(kr) => {
                    let bin_idx = bin as usize;
                    // C: if(krandom.bin[bin].r_size < RANDOM_ELEMENTS)
                    //         return ENOENT
                    if kr.bin[bin_idx].r_size < crate::krandom::RANDOM_ELEMENTS as i32 {
                        return KcallResult::Ok(ENOENT);
                    }
                    let snapshot = kr.bin[bin_idx];
                    // C: wipe_rnd_bin = bin (wiped after successful copy)
                    kr.wipe_bin(bin_idx);
                    snapshot
                }
                None => return KcallResult::Ok(EINVAL),
            };
            copy_struct_to_caller(caller, &bin_snapshot, val_ptr, val_len)
        }
        GetInfoRequest::IrqHooks => {
            // C: do_getinfo.c:91-95 — copy irq_hooks[] array.
            //
            // C: `length = sizeof(struct irq_hook) * NR_IRQ_HOOKS`
            //     `src_vir = (vir_bytes) irq_hooks`
            //
            // Build a C-compatible snapshot of the IRQ hook table from
            // the global IrqManager. Rust uses index-based linked lists,
            // so `next` and `handler` are exported as 0 (user-space tools
            // only read the non-pointer fields).
            let mut hooks: [IrqHookStruct; crate::syscall_device::NR_IRQ_HOOKS] =
                [IrqHookStruct::default(); crate::syscall_device::NR_IRQ_HOOKS];
            // SAFETY: BKL is held by kernel_call_dispatch (syscall.rs:245).
            if let Some(mgr) = unsafe { crate::try_irq_manager() } {
                for (slot, hook) in hooks.iter_mut().enumerate() {
                    if let (Some(irq), Some(id), Some(ep), Some(notify_id), Some(policy)) = (
                        mgr.hook_irq(slot),
                        mgr.hook_irq_id(slot),
                        mgr.hook_owner(slot),
                        mgr.hook_notify_id(slot),
                        mgr.hook_policy(slot),
                    ) {
                        hook.irq = irq.get() as i32;
                        hook.id = id.0 as i32;
                        hook.proc_nr_e = ep.0;
                        hook.notify_id = notify_id.get() as u64;
                        hook.policy = policy.bits() as u64;
                    }
                }
            }
            copy_struct_to_caller(caller, &hooks, val_ptr, val_len)
        }
        GetInfoRequest::Image => {
            // C: do_getinfo.c:86-90 — copy boot image table.
            //
            // C: `length = sizeof(struct boot_image) * NR_BOOT_PROCS`
            //     `src_vir = (vir_bytes) image`
            //
            // In C, `image[]` is a static table initialized in `table.c`.
            // In Rust, we build it from the process table (proc_nr, name,
            // endpoint) + boot modules (start_addr, len).
            //
            // NR_BOOT_PROCS is the number of boot-time processes. We use
            // NR_PROCS + NR_TASKS as the upper bound (the C table has
            // entries for all boot processes, some may be empty).
            let mut image: [BootImageStruct; NR_PROCS + NR_TASKS] =
                [BootImageStruct::default(); NR_PROCS + NR_TASKS];
            // Populate from process table.
            for (i, proc) in proc_table.iter().enumerate() {
                if i >= image.len() {
                    break;
                }
                image[i].proc_nr = proc.p_nr.0;
                image[i].endpoint = proc.p_endpoint.0;
                let name_bytes = proc.p_name.as_bytes();
                let copy_len = name_bytes.len().min(15);
                image[i].proc_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
            }
            // Populate start_addr + len from boot modules.
            if let Some(ki) = crate::kernel_info() {
                for (i, module) in ki.boot_modules.iter().enumerate() {
                    if i >= image.len() {
                        break;
                    }
                    image[i].start_addr = module.start.0;
                    image[i].len = module.len as u64;
                }
            }
            copy_struct_to_caller(caller, &image, val_ptr, val_len)
        }
        GetInfoRequest::MonParams => {
            // C: do_getinfo.c:143-146 — copy boot monitor parameter buffer.
            //
            // C: `src_vir = (vir_bytes) kinfo.param_buf`
            //     `length = sizeof(kinfo.param_buf)`
            //
            // P9-1 (2026-08-13): `KernelInfo.param_buf` field now exists
            // (minix-boot/src/kernel_info.rs:129, `&'static [u8]`).
            // However, the boot-shim currently populates it with an empty
            // slice (`&[]`) because UEFI load options are not yet wired
            // to fill it. When the boot-shim forwards UEFI load options
            // into `param_buf`, this branch should copy the bytes to the
            // caller's buffer via `data_copy_vmcheck`.
            //
            // Until then, return EINVAL to indicate the data is not
            // available (buffer is empty). This matches C's behavior when
            // the multiboot parameter buffer is empty.
            KcallResult::Ok(EINVAL)
        }
    }
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
/// ## Implemented (pure flag/RTS operations + reply data writeback)
///
/// - `T_STOP`     → `RTS_SET(rp, RTS_P_STOP)` + clear `MF_SC_TRACE|MF_STEP` (C: do_trace.c:89-93)
/// - `T_RESUME`   → `RTS_UNSET(rp, RTS_P_STOP)` + write `data=0` (C: do_trace.c:174-177)
/// - `T_STEP`     → set `MF_STEP` + `RTS_UNSET(rp, RTS_P_STOP)` + write `data=0` (C: do_trace.c:179-183)
/// - `T_SYSCALL`  → set `MF_SC_TRACE` + `RTS_UNSET(rp, RTS_P_STOP)` + write `data=0` (C: do_trace.c:185-189)
/// - `T_DETACH`   → clear `MF_SC_ACTIVE` + fall through to `T_RESUME` behavior (C: do_trace.c:170-177)
///
/// ## Cross-address-space copy (wired to `data_copy_vmcheck`)
///
/// - `T_GETINS` / `T_GETDATA` → copy `sizeof(long)` bytes from target to kernel,
///   write result to reply. C: COPYFROMPROC (do_trace.c:95-103).
/// - `T_SETINS` / `T_SETDATA` → copy `sizeof(long)` bytes from kernel to target,
///   write `data=0` to reply. C: COPYTOPROC (do_trace.c:126-134).
/// - `T_READB_INS` → copy 1 byte from target to kernel, write result to reply.
///   C: do_trace.c:191-194.
/// - `T_WRITEB_INS` → copy 1 byte from kernel to target, write `data=0` to reply.
///   C: do_trace.c:196-200.
///
/// VMSUSPEND: if the target's page is not yet faulted in, `data_copy_vmcheck`
/// suspends the caller (debugger) and the syscall is retried after VM resolves
/// the fault. C's `virtual_copy` fails with EFAULT in this case; the Rust
/// version is more robust.
///
/// ## T_GETUSER (fully implemented)
///
/// - Reads a `u64` (C `long`) from the process struct or priv struct at byte
///   offset `tr_addr`.
/// - Proc-struct branch: builds a `ProcInfoStruct` snapshot from `KProcess`,
///   then reads at offset (C: do_trace.c:108-111).
/// - Priv-struct branch: aligns `sizeof(ProcInfoStruct)` up to `sizeof(long)`,
///   subtracts to get priv offset, builds `PrivInfoStruct` from `KPriv` (via
///   `priv_table`), then reads at offset (C: do_trace.c:117-123).
/// - C: do_trace.c:105-124.
///
/// ## T_SETUSER (fully implemented)
///
/// - Writes to the register area (`cpu_context`) via arch-specific
///   `CpuContextArch::write_user_register`.
/// - `dispatch_trace` takes `&mut ProcessTable` for `proc_table.get_mut()`.
/// - Arch layer handles segment register protection (x86: forbid
///   cs/ds/es/gs/fs/ss; PSW masked with user-bit mask) and offset mapping
///   (arm64/riscv64: psr/pc/sp/a0 + gp_regs[0..30]).
/// - C: do_trace.c:136-168.
///
/// All 13 TRACE requests are fully implemented.
pub fn dispatch_trace(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
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

    // Target's page-table root for cross-space copies (T_GETINS etc.).
    // PhysBytes is Copy, so this extracts the value without holding the borrow.
    let target_cr3 = target.p_seg.phys_root;

    // Word size used by C `long` in the struct proc / struct priv layout.
    // C: `sizeof(long)` — 8 on 64-bit, 4 on 32-bit. We pin it as 8 (the
    // Rust rewrite is 64-bit only; the doc explicitly targets 64-bit).
    const WORD_SIZE: u64 = 8;
    const WORD_MASK: u64 = WORD_SIZE - 1;

    match req {
        // ── Implemented: pure flag/RTS operations ───────────────────

        // C: do_trace.c:89-93 — T_STOP: set RTS_P_STOP, clear trace flags.
        // C returns OK directly (no data writeback for T_STOP).
        TraceRequest::Stop => {
            target.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
            target.p_misc_flags.clear(MiscFlagsBits::SC_TRACE | MiscFlagsBits::STEP);
            KcallResult::Ok(0)
        }

        // C: do_trace.c:170-177 — T_DETACH: clear MF_SC_ACTIVE, fall through
        // to T_RESUME (clear RTS_P_STOP), write data=0.
        TraceRequest::Detach => {
            target.p_misc_flags.clear(MiscFlagsBits::SC_ACTIVE);
            target.p_rts_flags.clear(RtsFlagsBits::PROC_STOP);
            write_trace_reply_data(msg, 0);
            KcallResult::Ok(0)
        }

        // C: do_trace.c:174-177 — T_RESUME: clear RTS_P_STOP, write data=0.
        TraceRequest::Resume => {
            target.p_rts_flags.clear(RtsFlagsBits::PROC_STOP);
            write_trace_reply_data(msg, 0);
            KcallResult::Ok(0)
        }

        // C: do_trace.c:179-183 — T_STEP: set MF_STEP + clear RTS_P_STOP,
        // write data=0.
        TraceRequest::Step => {
            target.p_misc_flags.set(MiscFlagsBits::STEP);
            target.p_rts_flags.clear(RtsFlagsBits::PROC_STOP);
            write_trace_reply_data(msg, 0);
            KcallResult::Ok(0)
        }

        // C: do_trace.c:185-189 — T_SYSCALL: set MF_SC_TRACE + clear
        // RTS_P_STOP, write data=0.
        TraceRequest::Syscall => {
            target.p_misc_flags.set(MiscFlagsBits::SC_TRACE);
            target.p_rts_flags.clear(RtsFlagsBits::PROC_STOP);
            write_trace_reply_data(msg, 0);
            KcallResult::Ok(0)
        }

        // ── Cross-address-space copy (wired to data_copy_vmcheck) ──
        //
        // C's COPYFROMPROC/COPYTOPROC macros call virtual_copy, which is a
        // byte-level copy (like memcpy) — NO alignment requirement. Rust
        // uses `data_copy_vmcheck` which performs the same byte-level copy
        // via Direct Map + PTE walk, and additionally handles page faults
        // via VMSUSPEND (C's virtual_copy fails with EFAULT on unmapped
        // pages; data_copy_vmcheck asks VM to fault them in).

        // C: do_trace.c:95-103 — T_GETINS / T_GETDATA:
        // COPYFROMPROC(tr_addr, &tr_data, sizeof(long)).
        TraceRequest::GetIns | TraceRequest::GetData => {
            let mut buf: u64 = 0;
            let buf_phys = CurrentDirectMap::virt_to_phys(VirBytes(
                &mut buf as *mut u64 as u64,
            ));
            let proc_cr3 = |ep: Endpoint| {
                if ep == target_endpoint { Some(target_cr3) } else { None }
            };
            let src = AddressRef::Process {
                endpoint: target_endpoint,
                offset: VirBytes(tr_addr),
            };
            let dst = AddressRef::Physical(buf_phys);
            match data_copy_vmcheck(caller, src, dst, WORD_SIZE as usize, proc_cr3) {
                CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                CrossSpaceResult::Completed(Ok(())) => {
                    write_trace_reply_data(msg, buf as i64);
                }
            }
            KcallResult::Ok(0)
        }

        // C: do_trace.c:126-134 — T_SETINS / T_SETDATA:
        // COPYTOPROC(tr_addr, &tr_data, sizeof(long)).
        TraceRequest::SetIns | TraceRequest::SetData => {
            let buf: u64 = tr_data as u64;
            let buf_phys = CurrentDirectMap::virt_to_phys(VirBytes(
                &buf as *const u64 as u64,
            ));
            let proc_cr3 = |ep: Endpoint| {
                if ep == target_endpoint { Some(target_cr3) } else { None }
            };
            let src = AddressRef::Physical(buf_phys);
            let dst = AddressRef::Process {
                endpoint: target_endpoint,
                offset: VirBytes(tr_addr),
            };
            match data_copy_vmcheck(caller, src, dst, WORD_SIZE as usize, proc_cr3) {
                CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                CrossSpaceResult::Completed(Ok(())) => {
                    write_trace_reply_data(msg, 0);
                }
            }
            KcallResult::Ok(0)
        }

        // C: do_trace.c:191-194 — T_READB_INS:
        // COPYFROMPROC(tr_addr, &ub, 1) — byte-level copy.
        TraceRequest::ReadBIns => {
            let mut buf: u8 = 0;
            let buf_phys = CurrentDirectMap::virt_to_phys(VirBytes(
                // R-16 (2026-08-12): SAFETY: this is a 64-bit kernel, so
                // `*mut u8` is 64-bit and `as u64` cannot truncate.
                &mut buf as *mut u8 as u64,
            ));
            let proc_cr3 = |ep: Endpoint| {
                if ep == target_endpoint { Some(target_cr3) } else { None }
            };
            let src = AddressRef::Process {
                endpoint: target_endpoint,
                offset: VirBytes(tr_addr),
            };
            let dst = AddressRef::Physical(buf_phys);
            match data_copy_vmcheck(caller, src, dst, 1, proc_cr3) {
                CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                CrossSpaceResult::Completed(Ok(())) => {
                    write_trace_reply_data(msg, buf as i64);
                }
            }
            KcallResult::Ok(0)
        }

        // C: do_trace.c:196-200 — T_WRITEB_INS:
        // COPYTOPROC(tr_addr, &ub, 1) — byte-level copy.
        TraceRequest::WriteBIns => {
            let buf: u8 = tr_data as u8;
            let buf_phys = CurrentDirectMap::virt_to_phys(VirBytes(
                &buf as *const u8 as u64,
            ));
            let proc_cr3 = |ep: Endpoint| {
                if ep == target_endpoint { Some(target_cr3) } else { None }
            };
            let src = AddressRef::Physical(buf_phys);
            let dst = AddressRef::Process {
                endpoint: target_endpoint,
                offset: VirBytes(tr_addr),
            };
            match data_copy_vmcheck(caller, src, dst, 1, proc_cr3) {
                CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                CrossSpaceResult::Completed(Ok(())) => {
                    write_trace_reply_data(msg, 0);
                }
            }
            KcallResult::Ok(0)
        }

        // C: do_trace.c:105-124 — T_GETUSER: read from proc/priv struct.
        // C explicitly checks alignment: `if ((tr_addr & (sizeof(long)-1)) != 0) return(EFAULT)`.
        // Step 2: if offset <= sizeof(proc) - WORD_SIZE → read from proc.
        //         else → offset -= round_up(sizeof(proc)); if within priv → read priv.
        // Step 3: writeback tr_data into m_krn_lsys_sys_trace.data.
        TraceRequest::GetUser => {
            // C: do_trace.c:106 — alignment check
            if tr_addr & WORD_MASK != 0 {
                return KcallResult::Ok(EFAULT);
            }
            // C: do_trace.c:108-111 — read long from proc struct at offset.
            // Build a ProcInfoStruct snapshot, then read a u64 at the offset.
            let proc_info = proc_table.get(target_nr)
                .map(ProcInfoStruct::from_kprocess)
                .unwrap_or_default();
            let proc_size = core::mem::size_of::<ProcInfoStruct>() as u64;
            let word: u64 = if tr_addr + WORD_SIZE <= proc_size {
                read_word_at_offset(&proc_info, tr_addr as usize)
            } else {
                // C: do_trace.c:117-123 — read from priv struct.
                // Align proc_size up to sizeof(long) boundary, then
                // subtract to get the priv-struct offset.
                let priv_offset = tr_addr - ((proc_size + WORD_MASK) & !WORD_MASK);
                let priv_info = proc_table.get(target_nr)
                    .and_then(|rp| rp.priv_id)
                    .and_then(|pid| priv_table.get(pid))
                    .map(PrivInfoStruct::from_kpriv)
                    .unwrap_or_default();
                let priv_size = core::mem::size_of::<PrivInfoStruct>() as u64;
                if priv_offset + WORD_SIZE > priv_size {
                    return KcallResult::Ok(EFAULT);
                }
                read_word_at_offset(&priv_info, priv_offset as usize)
            };
            write_trace_reply_data(msg, word as i64);
            KcallResult::Ok(0)
        }

        // C: do_trace.c:136-168 — T_SETUSER: write to p_reg (register area).
        // C checks alignment + bounds: `tr_addr & (sizeof(reg_t)-1) != 0 ||
        // tr_addr > sizeof(stackframe_s) - sizeof(reg_t)` → EFAULT.
        //
        // Implementation: alignment check matches C. The actual register
        // write uses `CpuContextArch::write_user_register`, which
        // provides arch-specific segment register protection (x86:
        // forbid cs/ds/es/gs/fs/ss) and PSW bit masking (SETPSW).
        //
        // C bug note: on x86_64, the C source has no write path
        // (only `#if defined(__i386__)` is compiled). Rust implements
        // the correct behavior for all architectures.
        TraceRequest::SetUser => {
            // C: do_trace.c:136-138 — alignment + bounds check.
            if tr_addr & WORD_MASK != 0 {
                return KcallResult::Ok(EFAULT);
            }
            // Delegate to arch layer for the actual write.
            // `write_user_register` returns Err(()) for protected
            // registers (segment selectors on x86) and out-of-bounds
            // offsets.
            use minix_arch::{CpuContextArch, CurrentCpuContextArch};
            let write_result = proc_table.get_mut(target_nr).map(|rp| {
                <CurrentCpuContextArch as CpuContextArch>::write_user_register(
                    &mut rp.cpu_context,
                    tr_addr as usize,
                    tr_data as u64,
                )
            });
            match write_result {
                Some(Ok(())) => {
                    write_trace_reply_data(msg, 0);
                    KcallResult::Ok(0)
                }
                Some(Err(())) => {
                    // Protected register or out-of-bounds offset.
                    KcallResult::Ok(EFAULT)
                }
                None => KcallResult::Ok(EINVAL),
            }
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
/// # Implementation status (2026-08-01)
///
/// All 12 steps implemented:
/// 1-7: validation (endpoint, SYS_PROC, updatable checks)
/// 8: inherit_priv_irq/io/mem (KPriv::add_irq/add_io/add_mem)
/// 9: copy s_ipc_to target mask
/// 10: abort_proc_ipc_send (SYS_UPD_ROLLBACK — clears RTS_SENDING +
///     removes src from target's caller_q via SenderQueue::remove_by_nr)
/// 11: slot swap (ProcessTable::swap_slots + PrivTable::swap_slots via
///     core::mem::swap + split_at_mut)
/// 12: adjust_proc_slot/adjust_priv_slot (restore identity fields:
///     endpoint, nr, priv_id, caller_q, scheduler, cpu, cpu_mask on proc;
///     s_id, s_proc_nr, pending bits, alarm, diag_sig on priv)
///
/// `swap_proc_slot_pointer` (ptproc) and `swap_memreq` (vmrequest chain)
/// are no-ops: both processes are non-runnable (proc_is_updatable check),
/// so neither ptproc nor vmrequest chain can reference them.
/// `adjust_asyn_table` (async message table copy via data_copy) is skipped:
/// it is non-fatal in C (warning on failure), requires data_copy between
/// process address spaces, and is only triggered when both src and dst
/// have non-zero asynsize + matching asynendpoint (live update scenario).
pub fn dispatch_update(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
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
        .is_some_and(|kp| kp.is_sys_proc());
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
        .is_some_and(|kp| kp.is_sys_proc());
    if !dst_is_sys {
        return KcallResult::Ok(EPERM);
    }

    // Rust-specific check: reject src == dst explicitly → EINVAL.
    // C does NOT check this — C's do_update.c:73 has a different assert:
    //   `assert(!proc_is_runnable(src) && !proc_is_runnable(dst))`
    // which checks non-runnability (debug-only). Rust adds the self-swap
    // guard because a swapped-with-itself operation is nonsensical and
    // would corrupt state silently. C's runnability assert is omitted
    // because proc_is_updatable (checked next) is a stricter condition.
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

    // C: do_update.c:93-105 — inherit_priv_irq/io/mem.
    // Copy IRQ hooks, I/O ranges, and memory ranges from src's priv to
    // dst's priv. dst inherits src's hardware access permissions.
    let src_priv_id = proc_table.get(src_nr).and_then(|p| p.priv_id);
    let dst_priv_id = proc_table.get(dst_nr).and_then(|p| p.priv_id);
    if let (Some(src_pid), Some(dst_pid)) = (src_priv_id, dst_priv_id) {
        // Copy arrays out of src_priv (immutable borrow ends after scope)
        let (irqs, irq_count, io_tab, io_count, mem_tab, mem_count, ipc_to) = {
            let src_priv = match priv_table.get(src_pid) {
                Some(p) => p,
                None => return KcallResult::Ok(EINVAL),
            };
            (
                src_priv.io.s_irq_tab,
                src_priv.io.s_nr_irq as usize,
                src_priv.io.s_io_tab,
                src_priv.io.s_nr_io_range as usize,
                src_priv.mem.s_mem_tab,
                src_priv.mem.s_nr_mem_range as usize,
                src_priv.ipc.s_ipc_to,
            )
        };
        // Now add each to dst's priv (mutable borrow is safe — src borrow ended)
        if let Some(dst_priv) = priv_table.get_mut(dst_pid) {
            for irq in irqs.iter().take(irq_count) {
                let _ = dst_priv.add_irq(*irq);
            }
            for ior in io_tab.iter().take(io_count) {
                let _ = dst_priv.add_io(ior);
            }
            for memr in mem_tab.iter().take(mem_count) {
                let _ = dst_priv.add_mem(memr);
            }
            // C: do_update.c:107-112 — copy s_ipc_to target mask from src to dst.
            dst_priv.ipc.s_ipc_to |= ipc_to;
        }
    }

    // C: do_update.c:117 — `if (flags & SYS_UPD_ROLLBACK) abort_proc_ipc_send(src)`.
    // Abort any pending send() on src: clear RTS_SENDING, remove src
    // from its send target's caller_q.
    if (flags & SYS_UPD_ROLLBACK) != 0 {
        use crate::proc::RtsFlagsBits;
        // Read src's send target and SENDING flag before mutation.
        let (sendto_e, is_sending) = proc_table.get(src_nr)
            .map(|p| (p.p_sendto_e, p.p_rts_flags.is_set(RtsFlagsBits::SENDING)))
            .unwrap_or((Endpoint::NONE, false));
        if is_sending {
            // Clear RTS_SENDING on src
            if let Some(src) = proc_table.get_mut(src_nr) {
                src.p_rts_flags.clear(RtsFlagsBits::SENDING);
                src.p_misc_flags.clear(crate::proc::MiscFlagsBits::SENDING_FROM_KERNEL);
            }
            // Remove src from target's caller_q
            if let Some(target_nr) = proc_table.endpoint_to_nr(sendto_e)
                && let Some(target) = proc_table.get_mut(target_nr) {
                    target.caller_q.remove_by_nr(src_nr);
                }
        }
    }

    // C: do_update.c:114-118 — Save identity fields before swap.
    //
    // We save the "identity" fields that adjust_proc_slot/adjust_priv_slot
    // will restore after the swap. For Copy types, we just copy the values.
    // For caller_q (non-Copy), we extract it via mem::replace (replaced
    // with empty queue, restored after swap).
    let src_endpoint = proc_table.get(src_nr).map(|p| p.p_endpoint).unwrap_or(Endpoint::NONE);
    let dst_endpoint = proc_table.get(dst_nr).map(|p| p.p_endpoint).unwrap_or(Endpoint::NONE);
    let src_nr_val = src_nr;
    let dst_nr_val = dst_nr;
    let src_priv_id_val = src_priv_id;
    let dst_priv_id_val = dst_priv_id;
    let src_scheduler = proc_table.get(src_nr).and_then(|p| p.p_sched.scheduler);
    let dst_scheduler = proc_table.get(dst_nr).and_then(|p| p.p_sched.scheduler);
    let src_cpu = proc_table.get(src_nr).map(|p| p.p_sched.cpu.load(Ordering::Relaxed)).unwrap_or(0);
    let dst_cpu = proc_table.get(dst_nr).map(|p| p.p_sched.cpu.load(Ordering::Relaxed)).unwrap_or(0);
    let src_cpu_mask = proc_table.get(src_nr).map(|p| p.p_sched.cpu_mask).unwrap_or_default();
    let dst_cpu_mask = proc_table.get(dst_nr).map(|p| p.p_sched.cpu_mask).unwrap_or_default();

    // Extract caller_q (non-Copy) from both slots
    let src_caller_q = proc_table.get_mut(src_nr)
        .map(|p| core::mem::replace(&mut p.caller_q, crate::ipc::SenderQueue::new()))
        .unwrap_or_default();
    let dst_caller_q = proc_table.get_mut(dst_nr)
        .map(|p| core::mem::replace(&mut p.caller_q, crate::ipc::SenderQueue::new()))
        .unwrap_or_default();

    // Save priv identity fields
    let src_s_id = src_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.capability.s_id));
    let dst_s_id = dst_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.capability.s_id));
    let src_asyn_pending = src_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_asyn_pending));
    let dst_asyn_pending = dst_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_asyn_pending));
    let src_notify_pending = src_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_notify_pending));
    let dst_notify_pending = dst_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_notify_pending));
    let src_int_pending = src_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_int_pending));
    let dst_int_pending = dst_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_int_pending));
    let src_sig_pending = src_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_sig_pending));
    let dst_sig_pending = dst_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.signals.s_sig_pending));
    let src_diag_sig = src_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.mem.s_diag_sig));
    let dst_diag_sig = dst_priv_id.and_then(|pid| priv_table.get(pid).map(|p| p.mem.s_diag_sig));

    // C: do_update.c:129-133 — Swap slots.
    proc_table.swap_slots(src_nr, dst_nr);
    if let (Some(src_pid), Some(dst_pid)) = (src_priv_id, dst_priv_id) {
        priv_table.swap_slots(src_pid, dst_pid);
    }

    // C: do_update.c:135-137 — adjust_proc_slot.
    // Restore identity fields: endpoint, nr, priv_id, caller_q, scheduler,
    // cpu, cpu_mask stay with their original slot.
    if let Some(p) = proc_table.get_mut(src_nr) {
        p.p_endpoint = src_endpoint;
        p.p_nr = src_nr_val;
        p.priv_id = src_priv_id_val;
        p.p_sched.scheduler = src_scheduler;
        p.p_sched.cpu.store(src_cpu, Ordering::Relaxed);
        p.p_sched.cpu_mask = src_cpu_mask;
        p.caller_q = src_caller_q;
    }
    if let Some(p) = proc_table.get_mut(dst_nr) {
        p.p_endpoint = dst_endpoint;
        p.p_nr = dst_nr_val;
        p.priv_id = dst_priv_id_val;
        p.p_sched.scheduler = dst_scheduler;
        p.p_sched.cpu.store(dst_cpu, Ordering::Relaxed);
        p.p_sched.cpu_mask = dst_cpu_mask;
        p.caller_q = dst_caller_q;
    }

    // C: do_update.c:139-141 — adjust_priv_slot.
    // Restore identity fields on priv slots.
    if let (Some(src_pid), Some(dst_pid)) = (src_priv_id, dst_priv_id) {
        if let Some(p) = priv_table.get_mut(src_pid) {
            if let Some(v) = src_s_id { p.capability.s_id = v; }
            if let Some(v) = src_asyn_pending { p.signals.s_asyn_pending = v; }
            if let Some(v) = src_notify_pending { p.signals.s_notify_pending = v; }
            if let Some(v) = src_int_pending { p.signals.s_int_pending = v; }
            if let Some(v) = src_sig_pending { p.signals.s_sig_pending = v; }
            if let Some(v) = src_diag_sig { p.mem.s_diag_sig = v; }
            p.capability.s_proc_nr = Some(src_nr_val);
        }
        if let Some(p) = priv_table.get_mut(dst_pid) {
            if let Some(v) = dst_s_id { p.capability.s_id = v; }
            if let Some(v) = dst_asyn_pending { p.signals.s_asyn_pending = v; }
            if let Some(v) = dst_notify_pending { p.signals.s_notify_pending = v; }
            if let Some(v) = dst_int_pending { p.signals.s_int_pending = v; }
            if let Some(v) = dst_sig_pending { p.signals.s_sig_pending = v; }
            if let Some(v) = dst_diag_sig { p.mem.s_diag_sig = v; }
            p.capability.s_proc_nr = Some(dst_nr_val);
        }
    }

    // C: do_update.c:144 — swap_proc_slot_pointer(get_cpulocal_var_ptr(ptproc), src_rp, dst_rp)
    // No-op: both src and dst are non-runnable (checked by proc_is_updatable),
    // so the per-CPU "currently running process" pointer (ptproc) never
    // points to either. The swap would be a no-op in C as well.

    // C: do_update.c:147 — swap_memreq(src_rp, dst_rp)
    // No-op: the global vmrequest chain is not yet implemented in Rust.
    // When it is, this should swap src/dst in the chain if exactly one
    // has RTS_VMREQUEST set. Both processes are non-runnable (checked
    // by proc_is_updatable), so VMREQUEST is typically not set.

    let _ = caller;
    KcallResult::Ok(0)
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

impl TryFrom<i32> for ProfAction {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
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

impl TryFrom<i32> for ProfIntrType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Rtc),
            1 => Ok(Self::Nmi),
            _ => Err(()),
        }
    }
}

/// Profiling info struct copied to user space on PROF_STOP.
///
/// C: `struct sprof_info_s` — minix/profile.h:18-24.
///
/// Five `int` fields = 20 bytes on all architectures.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SprofInfo {
    /// Number of bytes written to the sample buffer. C: `mem_used`
    pub mem_used: i32,
    /// Total samples collected. C: `total_samples`
    pub total_samples: i32,
    /// Samples taken while CPU was idle. C: `idle_samples`
    pub idle_samples: i32,
    /// Samples taken in kernel mode. C: `system_samples`
    pub system_samples: i32,
    /// Samples taken in user mode. C: `user_samples`
    pub user_samples: i32,
}

/// Sample buffer size for statistical profiling.
///
/// C: `SAMPLE_BUFFER_SIZE = (64 << 20)` = 64 MB — profile.h:10.
///
/// Rust uses a smaller buffer (256 KB) because:
/// 1. 64 MB of BSS in a `no_std` kernel is excessive for typical
///    profiling sessions (256 KB holds ~10K samples + proc records).
/// 2. The buffer size can be increased if longer profiling sessions
///    are needed.
pub const SAMPLE_BUFFER_SIZE: usize = 256 * 1024;

/// Global profiling state (set during PROF_START, read during PROF_STOP).
///
/// C: `sprof_ep`, `sprof_info_addr_vir`, `sprof_data_addr_vir`,
/// `sprof_mem_size`, `sprof_info` — profile.h:15-18, do_sprofile.c:23.
///
/// Scalar fields use atomics (Rust 2024 `static_mut_refs` compliance, see
/// P1-5). `SPROF_INFO` is a struct and uses `addr_of_mut!` for field access
/// to avoid creating references. All access is serialized by the BKL.
static SPROF_EP: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static SPROF_INFO_ADDR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static SPROF_DATA_ADDR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static SPROF_MEM_SIZE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static mut SPROF_INFO: SprofInfo = SprofInfo {
    mem_used: 0,
    total_samples: 0,
    idle_samples: 0,
    system_samples: 0,
    user_samples: 0,
};

/// Static sample buffer (BSS-allocated, zero-initialized).
///
/// C: `char sprof_sample_buffer[SAMPLE_BUFFER_SIZE]` — profile.c:16.
/// Rust uses a smaller buffer; see `SAMPLE_BUFFER_SIZE` comment.
///
/// Written to by `sprof_save_sample` / `sprof_save_proc` during profiling,
/// read by `dispatch_profile` (PROF_STOP) to copy data to user space.
static mut SPROF_SAMPLE_BUFFER: [u8; SAMPLE_BUFFER_SIZE] = [0; SAMPLE_BUFFER_SIZE];

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
pub fn dispatch_profile(caller: &mut KProcess, msg: &Message, proc_table: &ProcessTable) -> KcallResult {
    // C: do_sprofile.c — read from mess_lsys_krn_sys_sprof
    msg.debug_check_m_type_any(&[Syscall::Sprof as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
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

            // C: do_sprofile.c:59-72 — store profiling parameters.
            // Atomics: BKL held, but atomics document the cross-CPU visibility.
            SPROF_EP.store(sprof.endpt, Ordering::Relaxed);
            SPROF_INFO_ADDR.store(sprof.ctl_ptr, Ordering::Relaxed);
            SPROF_DATA_ADDR.store(sprof.mem_ptr, Ordering::Relaxed);
            SPROF_MEM_SIZE.store(
                (sprof.mem_size as usize).min(SAMPLE_BUFFER_SIZE),
                Ordering::Relaxed,
            );
            // C: do_sprofile.c:64-68 — reset counters.
            // SAFETY: BKL is held; SPROFILING was just set to true (exclusive access).
            let info = core::ptr::addr_of_mut!(SPROF_INFO);
            unsafe { *info = SprofInfo::default(); }

            // C: do_sprofile.c:75-82 — intr-specific initialization.
            //   PROF_RTC → init_profile_clock(freq)
            //   PROF_NMI → nmi_watchdog_start_profiling(freq)
            match intr_type {
                ProfIntrType::Rtc => {
                    // C: init_profile_clock(freq) — arch-specific timer setup.
                    match crate::clock::init_profile_clock(sprof.freq as u32) {
                        Ok(()) => {
                            // Timer started; sprofiling stays true until PROF_STOP.
                            KcallResult::Ok(0)
                        }
                        Err(()) => {
                            SPROFILING.store(false, Ordering::Release);
                            KcallResult::Ok(EINVAL)
                        }
                    }
                }
                ProfIntrType::Nmi => {
                    // NMI watchdog profiling is WONTFIX — requires a complete NMI
                    // subsystem (NMI vector setup in IDT/VBAR/stvec, separate NMI
                    // stack, NMI-enabled IPI mechanism). The current IRQ framework
                    // (`IrqManager`) handles only maskable interrupts; NMI requires
                    // arch-specific entry points that bypass the standard IRQ path.
                    //
                    // Minix3 itself wraps the entire `profile.c` in `#if SPROFILE`
                    // and the NMI handler `nmi_sprofile_handler` (watchdog.c:165-198)
                    // is a separate code path from `profile_clock_handler`.
                    //
                    // The non-NMI profile path (`ProfIntrType::Clock`) is fully
                    // implemented — see `profile_sample` + `profile_clock_handler`
                    // + `clock::ack_profile_clock`. NMI is only needed for profiling
                    // through interrupt-disabled kernel critical sections.
                    //
                    // Return ENOSYS (function not implemented) to signal that the
                    // NMI sub-mechanism is not available; the user-space profiler
                    // should fall back to clock-based profiling.
                    SPROFILING.store(false, Ordering::Release);
                    KcallResult::Ok(ENOSYS)
                }
            }
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

            // C: do_sprofile.c:82 — stop_profile_clock()
            // Only PROF_RTC was supported in START (PROF_NMI returns ENOSYS),
            // so unconditionally call stop_profile_clock() here.
            crate::clock::stop_profile_clock();

            // C: do_sprofile.c:117-120 — copy sprof_info + sample buffer to user.
            //
            // data_copy(KERNEL, &sprof_info, sprof_ep, sprof_info_addr_vir, sizeof(sprof_info));
            // data_copy(KERNEL, sprof_sample_buffer, sprof_ep, sprof_data_addr_vir, sprof_info.mem_used);
            //
            // Atomics: BKL held, but atomics document cross-CPU visibility.
            // SPROF_INFO is read via addr_of! to avoid static_mut_refs (P1-5).
            let sprof_ep = Endpoint(SPROF_EP.load(Ordering::Relaxed));
            let info_addr = SPROF_INFO_ADDR.load(Ordering::Relaxed);
            let data_addr = SPROF_DATA_ADDR.load(Ordering::Relaxed);
            // SAFETY: BKL is held; SPROF_INFO accessed only under BKL.
            let mem_used = unsafe {
                let info = core::ptr::addr_of!(SPROF_INFO);
                (*info).mem_used as usize
            };

            // Capture caller's CR3 for the proc_cr3 closure.
            let caller_cr3 = caller.p_seg.phys_root;

            // Copy 1: sprof_info struct → user space.
            //
            // SAFETY: BKL is held; SPROF_INFO is a static that was reset during
            // PROF_START and only written by the profiling ISR (not yet implemented).
            let info_src_phys = {
                use minix_arch::{CurrentDirectMap, DirectMapArch};
                use minix_types::VirBytes;
                use core::ptr::addr_of;
                let ptr = addr_of!(SPROF_INFO) as u64;
                CurrentDirectMap::virt_to_phys(VirBytes(ptr))
            };
            let info_proc_cr3 = |ep: Endpoint| {
                if ep == sprof_ep { Some(caller_cr3) } else { None }
            };
            let info_src = crate::vm::AddressRef::Physical(info_src_phys);
            let info_dst = crate::vm::AddressRef::Process {
                endpoint: sprof_ep,
                offset: minix_types::VirBytes(info_addr),
            };
            match data_copy_vmcheck(
                caller,
                info_src,
                info_dst,
                core::mem::size_of::<SprofInfo>(),
                info_proc_cr3,
            ) {
                crate::vm::CrossSpaceResult::Completed(Ok(())) => {}
                crate::vm::CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                crate::vm::CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            }

            // Copy 2: sample buffer → user space (mem_used bytes).
            //
            // If no samples were collected (mem_used == 0), this is a no-op.
            if mem_used > 0 {
                let buf_src_phys = {
                    use minix_arch::{CurrentDirectMap, DirectMapArch};
                    use minix_types::VirBytes;
                    use core::ptr::addr_of;
                    let ptr = addr_of!(SPROF_SAMPLE_BUFFER) as u64;
                    CurrentDirectMap::virt_to_phys(VirBytes(ptr))
                };
                let buf_proc_cr3 = |ep: Endpoint| {
                    if ep == sprof_ep { Some(caller_cr3) } else { None }
                };
                let buf_src = crate::vm::AddressRef::Physical(buf_src_phys);
                let buf_dst = crate::vm::AddressRef::Process {
                    endpoint: sprof_ep,
                    offset: minix_types::VirBytes(data_addr),
                };
                match data_copy_vmcheck(
                    caller,
                    buf_src,
                    buf_dst,
                    mem_used,
                    buf_proc_cr3,
                ) {
                    crate::vm::CrossSpaceResult::Completed(Ok(())) => {}
                    crate::vm::CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                    crate::vm::CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
                }
            }

            KcallResult::Ok(OK)
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

// ── Statistical profiling sample collection ──────────────────────────────
//
// C: profile.c:51-126 — sprof_save_sample, sprof_save_proc, profile_sample,
// profile_clock_handler.
//
// These functions run in interrupt context (the profile clock ISR) and
// write sample data into SPROF_SAMPLE_BUFFER. The buffer is later copied
// to user space by dispatch_profile (PROF_STOP).

/// A single profiling sample: process endpoint + program counter.
///
/// C: `struct sprof_sample { endpoint_t proc; void *pc; }` — profile.h:27-30.
///
/// `#[repr(C)]` ensures the layout matches the C struct so the user-space
/// profiling tool can decode the buffer. On 64-bit, `proc` (4 bytes) is
/// followed by 4 bytes of alignment padding, then `pc` (8 bytes), total
/// 16 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SprofSample {
    /// Endpoint of the sampled process. C: `endpoint_t proc`
    pub proc: i32,
    /// Program counter at sample time. C: `void *pc`
    pub pc: u64,
}

/// A process record: endpoint + name (saved once per process).
///
/// C: `struct sprof_proc { endpoint_t proc; char name[PROC_NAME_LEN]; }`
/// — profile.h:32-35.
///
/// Written to the sample buffer the first time a system process is
/// sampled (gated by `MF_SPROF_SEEN`). The user-space profiling tool
/// uses these records to map endpoints to human-readable names.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SprofProc {
    /// Endpoint of the process. C: `endpoint_t proc`
    pub proc: i32,
    /// Process name (NUL-terminated). C: `char name[PROC_NAME_LEN]`
    pub name: [u8; PROC_NAME_LEN],
}

/// Save a profiling sample (endpoint + PC) to the sample buffer.
///
/// C: `sprof_save_sample()` — profile.c:51-61.
///
/// Writes a `SprofSample` record at the current `mem_used` offset in
/// `SPROF_SAMPLE_BUFFER`, then advances `mem_used` by `sizeof(SprofSample)`.
///
/// # Safety
///
/// Caller must hold the BKL (interrupt context under BKL). `SPROF_INFO`
/// and `SPROF_SAMPLE_BUFFER` are `static mut` accessed only under BKL via
/// `addr_of_mut!` (Rust 2024 `static_mut_refs` compliance, P1-5).
unsafe fn sprof_save_sample(endpoint: i32, pc: u64) { unsafe {
    // SAFETY: BKL held; access via addr_of_mut! avoids static_mut_refs.
    let info = core::ptr::addr_of_mut!(SPROF_INFO);
    let offset = (*info).mem_used as usize;
    if offset + core::mem::size_of::<SprofSample>() > SAMPLE_BUFFER_SIZE {
        // Buffer overflow — should have been caught by the space check in
        // profile_sample. Mark as full to prevent further writes.
        (*info).mem_used = -1;
        return;
    }
    let sample = SprofSample { proc: endpoint, pc };
    let base = core::ptr::addr_of_mut!(SPROF_SAMPLE_BUFFER) as *mut u8;
    let dst = base.add(offset) as *mut SprofSample;
    core::ptr::write_unaligned(dst, sample);
    (*info).mem_used += core::mem::size_of::<SprofSample>() as i32;
}}

/// Save a process record (endpoint + name) to the sample buffer.
///
/// C: `sprof_save_proc()` — profile.c:63-73.
///
/// Writes a `SprofProc` record at the current `mem_used` offset, then
/// advances `mem_used` by `sizeof(SprofProc)`. Called once per process
/// (gated by `MF_SPROF_SEEN`).
///
/// # Safety
///
/// Caller must hold the BKL. `SPROF_INFO` and `SPROF_SAMPLE_BUFFER` are
/// `static mut` accessed only under BKL via `addr_of_mut!` (P1-5).
unsafe fn sprof_save_proc(proc: &KProcess) { unsafe {
    // SAFETY: BKL held; access via addr_of_mut! avoids static_mut_refs.
    let info = core::ptr::addr_of_mut!(SPROF_INFO);
    let offset = (*info).mem_used as usize;
    if offset + core::mem::size_of::<SprofProc>() > SAMPLE_BUFFER_SIZE {
        (*info).mem_used = -1;
        return;
    }
    let record = SprofProc {
        proc: proc.p_endpoint.get(),
        name: *proc.p_name.as_bytes(),
    };
    let base = core::ptr::addr_of_mut!(SPROF_SAMPLE_BUFFER) as *mut u8;
    let dst = base.add(offset) as *mut SprofProc;
    core::ptr::write_unaligned(dst, record);
    (*info).mem_used += core::mem::size_of::<SprofProc>() as i32;
}}

/// Collect a profiling sample for the current process.
///
/// C: `profile_sample()` — profile.c:75-110.
///
/// Called on every tick of the profiling clock. Classifies the sample
/// as idle / system / user and writes it to the sample buffer (system
/// samples only). Updates the counters in `SPROF_INFO`.
///
/// # Arguments
///
/// * `proc` — the currently running process (C: `get_cpulocal_var(proc_ptr)`)
/// * `pc` — the saved program counter from the trap frame (C: `p->p_reg.pc`)
/// * `priv_table` — privilege table for `SYS_PROC` check (C: `priv(p)`)
///
/// # Safety
///
/// Caller must hold the BKL. `SPROF_INFO` accessed via `addr_of_mut!` (P1-5).
pub unsafe fn profile_sample(proc: &KProcess, pc: u64, priv_table: &PrivTable) { unsafe {
    // SAFETY: BKL held; access via addr_of_mut! avoids static_mut_refs.
    let info = core::ptr::addr_of_mut!(SPROF_INFO);

    // C: profile.c:80-81 — are we profiling, and not full?
    if !SPROFILING.load(Ordering::Acquire) || (*info).mem_used == -1 {
        return;
    }

    // C: profile.c:84-89 — check if enough memory available before writing.
    //
    // C code checks: mem_used + sizeof(sprof_info) + 2*sizeof(sprof_sample)
    //                + 2*sizeof(sprof_sample) > sprof_mem_size
    //
    // NOTE: C has a typo — the second `2*sizeof(struct sprof_sample)` should
    // be `2*sizeof(struct sprof_proc)`. We replicate C's exact check for
    // semantic alignment, even though it's slightly less conservative than
    // intended (sizeof(sprof_sample) < sizeof(sprof_proc) on all platforms).
    let space_needed = (*info).mem_used as usize
        + core::mem::size_of::<SprofInfo>()
        + 2 * core::mem::size_of::<SprofSample>()
        + 2 * core::mem::size_of::<SprofSample>();
    let mem_size = SPROF_MEM_SIZE.load(Ordering::Relaxed);
    if space_needed > mem_size {
        (*info).mem_used = -1;
        return;
    }

    // C: profile.c:92-107 — classify the sample.
    let endpoint = proc.p_endpoint.get();

    if endpoint == Endpoint::IDLE.get() {
        // C: profile.c:93 — idle sample.
        (*info).idle_samples += 1;
    } else if endpoint == Endpoint::KERNEL.get()
        || (is_sys_proc_runnable(proc, priv_table))
    {
        // C: profile.c:94-103 — runnable system process.
        //
        // Save proc record if this is the first time we see it
        // (gated by MF_SPROF_SEEN, matching C's p_misc_flags check).
        if !proc.p_misc_flags.is_set(MiscFlagsBits::SPROF_SEEN) {
            proc.p_misc_flags.set(MiscFlagsBits::SPROF_SEEN);
            sprof_save_proc(proc);
        }
        sprof_save_sample(endpoint, pc);
        (*info).system_samples += 1;
    } else {
        // C: profile.c:106 — user process.
        (*info).user_samples += 1;
    }

    // C: profile.c:109 — total samples (always, unless early return).
    (*info).total_samples += 1;
}}

/// Check if a process is a runnable system process.
///
/// C: `priv(p)->s_flags & SYS_PROC && proc_is_runnable(p)` — profile.c:95.
///
/// Extracted as a helper for clarity. Returns `true` if the process has
/// `SYS_PROC` privilege AND is currently runnable.
fn is_sys_proc_runnable(proc: &KProcess, priv_table: &PrivTable) -> bool {
    let priv_id = match proc.priv_id {
        Some(id) => id,
        None => return false,
    };
    let priv_ = match priv_table.get(priv_id) {
        Some(p) => p,
        None => return false,
    };
    priv_.is_sys_proc() && proc.is_runnable()
}

/// Profile clock interrupt handler.
///
/// C: `profile_clock_handler()` — profile.c:115-126.
///
/// Called by the trap entry path on each profile clock tick. Collects
/// a sample for the currently running process, then acknowledges the
/// interrupt.
///
/// # Arguments
///
/// * `proc` — the currently running process (C: `get_cpulocal_var(proc_ptr)`)
/// * `pc` — the saved program counter from the trap frame (C: `p->p_reg.pc`)
/// * `priv_table` — privilege table for `SYS_PROC` check
///
/// # Safety
///
/// Caller must hold the BKL. Delegates to `profile_sample` which accesses
/// `static mut` profiling state.
///
/// # Trap entry integration
///
/// The arch-specific trap entry path should call this function when it
/// detects the profile clock IRQ (IRQ8 on x86-64 RTC). The generic
/// `dispatch_hardware_irq` does not pass the saved PC, so the trap entry
/// must call this handler directly with the PC extracted from the trap
/// frame before falling through to the generic IRQ dispatch.
pub unsafe fn profile_clock_handler(
    proc: &KProcess,
    pc: u64,
    priv_table: &PrivTable,
) { unsafe {
    profile_sample(proc, pc, priv_table);
    crate::clock::ack_profile_clock();
}}

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
        // C: com.h:316-339 — GET_* macro values
        assert_eq!(GetInfoRequest::try_from(0), Ok(GetInfoRequest::KInfo));
        assert_eq!(GetInfoRequest::try_from(11), Ok(GetInfoRequest::Proc));
        assert_eq!(GetInfoRequest::try_from(15), Ok(GetInfoRequest::LoadInfo));
        assert_eq!(GetInfoRequest::try_from(19), Ok(GetInfoRequest::WhoAmI));
        assert_eq!(GetInfoRequest::try_from(25), Ok(GetInfoRequest::CpuTicks));
        // Values not handled by C do_getinfo → Err (→ EINVAL in dispatch)
        assert_eq!(GetInfoRequest::try_from(7), Err(()));   // undefined
        assert_eq!(GetInfoRequest::try_from(9), Err(()));   // GET_KADDRESSES (not handled)
        assert_eq!(GetInfoRequest::try_from(10), Err(()));  // GET_SCHEDINFO (not handled)
        assert_eq!(GetInfoRequest::try_from(99), Err(()));
    }

    #[test]
    fn test_trace_request_try_from() {
        // C: ptrace.h:225-250 — T_* macro values
        assert_eq!(TraceRequest::try_from(-1), Ok(TraceRequest::Stop));
        assert_eq!(TraceRequest::try_from(1), Ok(TraceRequest::GetIns));   // PT_READ_I
        assert_eq!(TraceRequest::try_from(7), Ok(TraceRequest::Resume));   // PT_CONTINUE
        assert_eq!(TraceRequest::try_from(104), Ok(TraceRequest::Step));
        assert_eq!(TraceRequest::try_from(14), Ok(TraceRequest::Syscall)); // PT_SYSCALL
        // PM-handled requests → Err (→ EINVAL in kernel dispatch)
        assert_eq!(TraceRequest::try_from(0), Err(()));   // T_OK (PT_TRACE_ME) — PM handles
        assert_eq!(TraceRequest::try_from(8), Err(()));   // T_EXIT (PT_KILL) — PM handles
        assert_eq!(TraceRequest::try_from(99), Err(()));
    }

    /// Helper: create a proc_table with one user process activated at `nr`.
    fn make_proc_table_with_user(nr: i32, endpoint_val: i32) -> ProcessTable {
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        if let Some(p) = proc_table.get_mut(ProcNr(nr)) {
            p.p_endpoint = Endpoint(endpoint_val);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        proc_table
    }

    #[test]
    fn test_dispatch_trace_step_clears_proc_stop_and_sets_step_flag() {
        let mut proc_table = make_proc_table_with_user(0, 100);
        // Pre-set PROC_STOP on target.
        proc_table.get_mut(ProcNr(0)).unwrap().p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100; // target endpoint
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Step as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0));
        // Verify side effects.
        let target = proc_table.get(ProcNr(0)).unwrap();
        assert!(target.p_misc_flags.is_set(MiscFlagsBits::STEP));
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_trace_resume_clears_proc_stop() {
        // C: do_trace.c:174-177 — T_RESUME: clear RTS_P_STOP, write data=0.
        let mut proc_table = make_proc_table_with_user(0, 100);
        proc_table.get_mut(ProcNr(0)).unwrap().p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Resume as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0));
        let target = proc_table.get(ProcNr(0)).unwrap();
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_trace_stop_sets_proc_stop_and_clears_trace_flags() {
        // C: do_trace.c:89-93 — T_STOP: set RTS_P_STOP + clear MF_SC_TRACE|MF_STEP.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Stop as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0));
        let target = proc_table.get(ProcNr(0)).unwrap();
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
        assert!(!target.p_misc_flags.is_set(MiscFlagsBits::SC_TRACE));
        assert!(!target.p_misc_flags.is_set(MiscFlagsBits::STEP));
    }

    #[test]
    fn test_dispatch_trace_detach_clears_sc_active_and_proc_stop() {
        // C: do_trace.c:170-177 — T_DETACH: clear MF_SC_ACTIVE + RTS_P_STOP.
        let mut proc_table = make_proc_table_with_user(0, 100);
        proc_table.get_mut(ProcNr(0)).unwrap().p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc_table.get_mut(ProcNr(0)).unwrap().p_misc_flags.set(MiscFlagsBits::SC_ACTIVE);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Detach as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0));
        let target = proc_table.get(ProcNr(0)).unwrap();
        assert!(!target.p_misc_flags.is_set(MiscFlagsBits::SC_ACTIVE));
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_trace_syscall_sets_sc_trace_and_clears_proc_stop() {
        // C: do_trace.c:185-189 — T_SYSCALL: set MF_SC_TRACE + clear RTS_P_STOP.
        let mut proc_table = make_proc_table_with_user(0, 100);
        proc_table.get_mut(ProcNr(0)).unwrap().p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Syscall as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0));
        let target = proc_table.get(ProcNr(0)).unwrap();
        assert!(target.p_misc_flags.is_set(MiscFlagsBits::SC_TRACE));
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_dispatch_trace_exit_returns_einval() {
        // C: do_trace.c:202-204 — T_EXIT (PT_KILL=8) is NOT in the kernel's
        // switch; it falls to `default: return EINVAL`. T_EXIT is handled
        // by the Process Manager (PM) via SIGKILL, not the kernel.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = 8; // T_EXIT = PT_KILL = 8
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_trace_getins_wired_to_data_copy_vmcheck() {
        // C: do_trace.c:95-103 — T_GETINS: COPYFROMPROC(tr_addr, &tr_data, sizeof(long)).
        // Wired to `data_copy_vmcheck` (was DEFERRED → ENOSYS). In mock mode
        // the PTE walk always misses (MockPteWalk::walk returns None), so the
        // copy suspends with a Src page fault → KcallResult::VmSuspend, and
        // RTS_VMREQUEST is set on the caller (mirrors C's vm_suspend() path).
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetIns as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    // ── TRACE memory copy tests ──────────────────────────────────
    //
    // C's COPYFROMPROC/COPYTOPROC use virtual_copy (byte-level copy, no
    // alignment requirement). Rust delegates to `data_copy_vmcheck`, which
    // performs the same byte-level copy via Direct Map + PTE walk. In mock
    // mode the walk always misses, so every copy suspends (VmSuspend) —
    // verifying the copy path is actually invoked rather than short-circuited
    // by an alignment check. Only T_GETUSER/T_SETUSER have C-mandated
    // alignment checks (tested separately below).

    #[test]
    fn test_dispatch_trace_getins_unaligned_no_alignment_check() {
        // C: do_trace.c:96 — COPYFROMPROC is byte-level (no alignment check).
        // Unaligned address must NOT return EFAULT. Wired path suspends
        // (Src page fault) rather than rejecting the address — proving the
        // byte-level copy semantics match C.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetIns as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0x1001; // unaligned (1 byte off 8-byte boundary)
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_trace_getdata_aligned_wired_to_data_copy_vmcheck() {
        // C: do_trace.c:100-103 — T_GETDATA: COPYFROMPROC with aligned addr.
        // Wired to `data_copy_vmcheck` (was DEFERRED → ENOSYS). Mock PTE walk
        // misses → VmSuspend + RTS_VMREQUEST on caller.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetData as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0x1000; // aligned
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_trace_setins_unaligned_no_alignment_check() {
        // C: do_trace.c:127 — COPYTOPROC is byte-level (no alignment check).
        // Unaligned address must NOT return EFAULT. Wired path suspends
        // (Dst page fault) rather than rejecting the address.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetIns as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0x1003; // unaligned
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_trace_setdata_aligned_wired_to_data_copy_vmcheck() {
        // C: do_trace.c:131-134 — T_SETDATA: COPYTOPROC with aligned addr.
        // Wired to `data_copy_vmcheck` (was DEFERRED → ENOSYS). Mock PTE walk
        // misses → VmSuspend + RTS_VMREQUEST on caller.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetData as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0x1000; // aligned
        msg.m_u.m_lsys_krn_sys_trace.data = 0xDEAD_BEEF; // data to write
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_trace_getuser_rejects_unaligned_address() {
        // C: do_trace.c:106 — `if ((tr_addr & (sizeof(long) - 1)) != 0) return(EFAULT)`.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0x4; // unaligned
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_setuser_rejects_unaligned_address() {
        // C: do_trace.c:137 — alignment check before offset range check.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0x2; // unaligned
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_setuser_writes_rip() {
        // T_SETUSER writing to rip (offset 56 on x86_64) succeeds.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 56; // rip offset
        msg.m_u.m_lsys_krn_sys_trace.data = 0xdead_beef;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0), "writing rip should succeed");
    }

    #[test]
    fn test_dispatch_trace_setuser_rejects_segment_register() {
        // T_SETUSER writing to cs (offset 8 on x86_64) is protected → EFAULT.
        // C: do_trace.c:145-151 — segment registers (cs/ds/es/fs/gs/ss) are
        // protected because writing them could crash the kernel at context
        // switch.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 8; // cs offset (protected)
        msg.m_u.m_lsys_krn_sys_trace.data = 0x10;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_trace_setuser_psw_succeeds() {
        // T_SETUSER writing to psw (offset 0) succeeds — PSW is writable
        // with user-bit masking (C: SETPSW). The arch layer applies
        // PSW_USER_MASK internally.
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::SetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = 0; // psw offset
        msg.m_u.m_lsys_krn_sys_trace.data = 0x0200; // IF (bit 9)
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(0), "writing psw should succeed");
    }

    #[test]
    fn test_dispatch_trace_rejects_invalid_request() {
        let mut proc_table = make_proc_table_with_user(0, 100);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = 99; // out of range
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_trace_rejects_invalid_target_endpoint() {
        let mut proc_table = ProcessTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 9999; // not in proc table
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Step as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
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
        let mut caller = KProcess::new(ProcNr(0),Endpoint(200));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 50;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::Step as i32;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &PrivTable::new());
        // ProcessTable::is_kernel(KERNEL=-1) returns true → EPERM.
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_trace_getuser_priv_struct() {
        // C: do_trace.c:117-123 — T_GETUSER reading from priv struct.
        // When tr_addr exceeds sizeof(ProcInfoStruct) (aligned up), the
        // read should fall through to the priv struct snapshot.
        let (mut proc_table, priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(300));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        let proc_size = core::mem::size_of::<ProcInfoStruct>() as u64;
        let priv_offset = 0u64; // s_proc_nr is first field of PrivInfoStruct
        let tr_addr = ((proc_size + 7) & !7) + priv_offset;
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = tr_addr as u64;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(0), "priv-struct read should succeed");
        // The first field of PrivInfoStruct is s_proc_nr (i32). For slot 0
        // with assign_static(0), s_proc_nr should be 0.
        msg.debug_check_m_type_any(&[Syscall::Trace as i32]);
        // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
        let reply = unsafe { msg.m_u.m_lsys_krn_sys_trace.data };
        assert_eq!(reply & 0xFFFF_FFFF, 0, "s_proc_nr should be 0 for slot 0");
    }

    #[test]
    fn test_dispatch_trace_getuser_priv_struct_out_of_range() {
        // C: do_trace.c:120 — priv-struct offset beyond sizeof(struct priv)
        // returns EFAULT.
        let (mut proc_table, priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(300));
        let mut msg = Message::default();
        msg.m_type = Syscall::Trace as i32;
        let proc_size = core::mem::size_of::<ProcInfoStruct>() as u64;
        let priv_size = core::mem::size_of::<PrivInfoStruct>() as u64;
        let tr_addr = ((proc_size + 7) & !7) + priv_size; // one past end
        msg.m_u.m_lsys_krn_sys_trace.endpt = 100;
        msg.m_u.m_lsys_krn_sys_trace.request = TraceRequest::GetUser as i32;
        msg.m_u.m_lsys_krn_sys_trace.address = tr_addr;
        let result = dispatch_trace(&mut caller, &mut msg, &mut proc_table, &priv_table);
        assert_eq!(result, KcallResult::Ok(EFAULT));
    }

    #[test]
    fn test_dispatch_unused() {
        // ENOSYS = 78 (errno.h:137). Previously asserted as 38 (ENOTSOCK's
        // value) due to a duplicated errno constant — fixed in FIX-01.
        assert!(matches!(dispatch_unused(), KcallResult::Ok(ENOSYS)));
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
            if let Some(p) = proc_table.get_mut(ProcNr(slot_i32)) {
                p.p_endpoint = Endpoint(ep);
                p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE); // mark slot occupied
                p.p_rts_flags.set(RtsFlagsBits::NO_PRIV); // not in kernel
                p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
                p.p_rts_flags.clear(RtsFlagsBits::SENDING);
                p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
                let pid = priv_table.assign_static(ProcNr(slot_i32)).expect("static priv slot");
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
        let (mut proc_table, mut priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = minix_types::Endpoint::NONE.0; // src = NONE
        msg.m_u.m_m1.m1i2 = 200;
        msg.m_u.m_m1.m1i3 = 0;
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_update_rejects_self_swap() {
        // src == dst is nonsensical and would corrupt state. Rust rejects
        // it explicitly with EINVAL. C's assert would have crashed.
        let (mut proc_table, mut priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = 100; // src = 100
        msg.m_u.m_m1.m1i2 = 100; // dst = 100 (same)
        msg.m_u.m_m1.m1i3 = 0;
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_update_rejects_busy_process() {
        // C: do_update.c:77-79 — proc_is_updatable returns false
        // when the process has NO_PRIV unset AND no SIG_PENDING AND
        // not in (RECEIVING && !SENDING) state. Make src be in
        // "kernel mode" (NO_PRIV unset), no sig pending, no receiving.
        let (mut proc_table, mut priv_table) = make_two_sys_procs(0, 100, 1, 200);
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // in kernel mode
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        }
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = 100;
        msg.m_u.m_m1.m1i2 = 200;
        msg.m_u.m_m1.m1i3 = 0;
        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(EBUSY));
    }

    #[test]
    fn test_dispatch_update_quiescent_swaps_slots() {
        // Both processes are quiescent — proc_is_updatable is true for both.
        // The swap body is now implemented: swaps runtime state while
        // preserving identity fields (endpoint, nr, priv_id, caller_q, etc.).
        let (mut proc_table, mut priv_table) = make_two_sys_procs(0, 100, 1, 200);
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = 100;
        msg.m_u.m_m1.m1i2 = 200;
        msg.m_u.m_m1.m1i3 = 0;
        // Verify pre-swap endpoints
        assert_eq!(proc_table.get(ProcNr(0)).unwrap().p_endpoint, Endpoint(100));
        assert_eq!(proc_table.get(ProcNr(1)).unwrap().p_endpoint, Endpoint(200));

        let result = dispatch_update(&mut caller, &msg, &mut proc_table, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(0), "swap should succeed");

        // Post-swap: identity fields preserved (endpoint, nr stay in original slot)
        assert_eq!(proc_table.get(ProcNr(0)).unwrap().p_endpoint, Endpoint(100));
        assert_eq!(proc_table.get(ProcNr(0)).unwrap().p_nr, ProcNr(0));
        assert_eq!(proc_table.get(ProcNr(1)).unwrap().p_endpoint, Endpoint(200));
        assert_eq!(proc_table.get(ProcNr(1)).unwrap().p_nr, ProcNr(1));
    }

    #[test]
    fn test_proc_is_updatable_user_mode_returns_true() {
        // C: do_update.c:18 — `RTS_NO_PRIV` set means user-mode (no
        // kernel privilege). proc_is_updatable allows user-mode processes.
        let mut proc_table = ProcessTable::new();
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.set(RtsFlagsBits::NO_PRIV); // user-mode (no priv)
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        }
        let p = proc_table.get(ProcNr(0)).unwrap();
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
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // in kernel
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            p.p_rts_flags.clear(RtsFlagsBits::SENDING);
        }
        let p = proc_table.get(ProcNr(0)).unwrap();
        assert!(proc_is_updatable(p));
    }

    #[test]
    fn test_proc_is_updatable_kernel_blocked_returns_false() {
        // In kernel mode, no sig pending, no RECEIVING/SENDING.
        // proc_is_updatable returns false: the process is actively
        // executing kernel code and cannot be swapped safely.
        let mut proc_table = ProcessTable::new();
        use crate::proc::RtsFlagsBits;
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // in kernel
            p.p_rts_flags.clear(RtsFlagsBits::SIG_PENDING);
            p.p_rts_flags.clear(RtsFlagsBits::RECEIVING);
            p.p_rts_flags.clear(RtsFlagsBits::SENDING);
        }
        let p = proc_table.get(ProcNr(0)).unwrap();
        assert!(!proc_is_updatable(p));
    }

    #[test]
    fn test_getinfo_proc_self_replaces_with_caller_endpoint() {
        use crate::proc_table::ProcessTable;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = ProcessTable::new();
        // Activate slot 0 so endpoint_to_nr succeeds for caller's endpoint
        let caller_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(ProcNr(0)) {
            target.p_endpoint = caller_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),caller_ep);
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        // Set request = GET_PROC (11), val_len2_e = SELF (-1)
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc as i32;
        msg.m_u.m_m1.m1p3 = Endpoint::SELF.0 as u64; // val_len2_e = SELF
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        // Endpoint is valid (SELF replaced with caller's). data_copy_vmcheck
        // is now wired (was DEFERRED → ENOSYS). In mock mode the PTE walk
        // always misses, so the copy suspends → VmSuspend + RTS_VMREQUEST.
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_getinfo_proc_returns_proc_info() {
        // GET_PROC with a valid endpoint: data_copy_vmcheck is wired, so the
        // result is VmSuspend (mock PTE walk misses) rather than ENOSYS.
        use crate::proc_table::ProcessTable;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(ProcNr(0)) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        // Set request = GET_PROC (11), val_len2_e = valid endpoint
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc as i32;
        msg.m_u.m_m1.m1p3 = target_ep.0 as u64; // val_len2_e = valid endpoint
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_getinfo_proc_invalid_endpoint_returns_einval() {
        // C: do_getinfo.c:107-114 — GET_PROC with invalid endpoint → EINVAL.
        use crate::proc_table::ProcessTable;

        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Proc as i32;
        msg.m_u.m_m1.m1p3 = 9999u64; // val_len2_e = invalid
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_getinfo_proctab_wired_to_data_copy_vmcheck() {
        // GET_PROCTAB: chunked copy is wired. Mock PTE walk misses on the
        // very first element → VmSuspend + RTS_VMREQUEST.
        use crate::proc::RtsFlagsBits;
        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        msg.m_u.m_m1.m1i1 = GetInfoRequest::ProcTab as i32;
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_getinfo_priv_wired_to_data_copy_vmcheck() {
        // GET_PRIV with a valid endpoint + assigned privilege: the priv
        // snapshot is built and data_copy_vmcheck suspends in mock mode.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        let mut priv_table = PrivTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        let pid = priv_table.assign_static(ProcNr(0)).expect("static priv slot");
        if let Some(target) = proc_table.get_mut(ProcNr(0)) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            target.priv_id = Some(pid);
        }
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Priv as i32;
        msg.m_u.m_m1.m1p3 = target_ep.0 as u64;
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_getinfo_privtab_wired_to_data_copy_vmcheck() {
        // GET_PRIVTAB: chunked copy is wired. First element suspends.
        use crate::proc::RtsFlagsBits;
        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        msg.m_u.m_m1.m1i1 = GetInfoRequest::PrivTab as i32;
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_getinfo_regs_wired_to_data_copy_vmcheck() {
        // GET_REGS with a valid endpoint: cpu_context raw-byte copy is wired.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(ProcNr(0)) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Regs as i32;
        msg.m_u.m_m1.m1p3 = target_ep.0 as u64;
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::VmSuspend);
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
    }

    #[test]
    fn test_dispatch_getinfo_regs_invalid_endpoint_returns_einval() {
        // C: do_getinfo.c:123-131 — GET_REGS with invalid endpoint → EINVAL.
        use crate::proc_table::ProcessTable;

        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Regs as i32;
        msg.m_u.m_m1.m1p3 = 9999u64; // val_len2_e = invalid
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_getinfo_priv_rejects_invalid_endpoint() {
        // C: do_getinfo.c:115-122 — GET_PRIV: same endpoint validation as GET_PROC.
        use crate::proc_table::ProcessTable;

        let proc_table = ProcessTable::new();
        let priv_table = PrivTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Getinfo as i32;
        // Set request = GET_PRIV (17), val_len2_e = invalid endpoint
        msg.m_u.m_m1.m1i1 = GetInfoRequest::Priv as i32;
        msg.m_u.m_m1.m1p3 = 9999u64; // val_len2_e = invalid
        let result = dispatch_getinfo(&mut caller, &mut msg, &priv_table, &proc_table, &crate::clock::ClockState::new());
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
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
        msg.m_u.m_lsys_krn_sys_sprof.action = 99; // unknown action
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_start_rejects_invalid_endpoint() {
        let _lock = sprof_test_setup();
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
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
        if let Some(target) = proc_table.get_mut(ProcNr(0)) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = target_ep.0;
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = 99; // unknown intr_type
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_start_valid_returns_ok() {
        let _lock = sprof_test_setup();
        let mut proc_table = ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 0);
        if let Some(target) = proc_table.get_mut(ProcNr(0)) {
            target.p_endpoint = target_ep;
            target.p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = target_ep.0;
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = ProfIntrType::Rtc as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        // Validation passes, init_profile_clock wired → OK (0)
        assert_eq!(result, KcallResult::Ok(0));
        // SPROFILING must remain true after successful START
        assert!(SPROFILING.load(Ordering::Acquire));
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
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Stop as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EBUSY));
        sprof_test_teardown();
    }

    #[test]
    fn test_sprof_stop_after_start_returns_ok_or_vmsuspend() {
        // PROF_STOP after PROF_START now reaches the data_copy path.
        // In mock mode, the page table walk returns None (page fault),
        // causing data_copy_vmcheck to return VmSuspend.
        // On real hardware, it would return OK after copying sprof_info.
        let _lock = sprof_test_setup();
        // Simulate PROF_START having succeeded: set SPROFILING + state.
        SPROFILING.store(true, Ordering::Release);
        // Set up SPROF state (normally done by PROF_START).
        SPROF_EP.store(100, Ordering::Relaxed);
        SPROF_INFO_ADDR.store(0x1000, Ordering::Relaxed);
        SPROF_DATA_ADDR.store(0x2000, Ordering::Relaxed);
        // SAFETY: BKL is held (simulated by test lock); SPROF_INFO accessed via addr_of_mut!.
        let info = core::ptr::addr_of_mut!(SPROF_INFO);
        unsafe { *info = SprofInfo::default(); }
        let proc_table = ProcessTable::new();
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Stop as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        // In mock mode: VmSuspend (page table walk fails).
        // On real hardware: Ok(OK) (data_copy succeeds).
        // We accept either outcome; the key assertion is that SPROFILING
        // is cleared (STOP was reached and processed).
        assert!(
            result == KcallResult::VmSuspend || result == KcallResult::Ok(OK),
            "expected VmSuspend or Ok(OK), got {:?}",
            result
        );
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
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
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
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
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
        let mut caller = KProcess::new(ProcNr(0),Endpoint(100));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sprof as i32;
        msg.m_u.m_lsys_krn_sys_sprof.action = ProfAction::Start as i32;
        msg.m_u.m_lsys_krn_sys_sprof.endpt = 9999; // invalid
        msg.m_u.m_lsys_krn_sys_sprof.intr_type = ProfIntrType::Rtc as i32;
        let result = dispatch_profile(&mut caller, &msg, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
        // Verify rollback.
        assert!(!SPROFILING.load(Ordering::Acquire));
        sprof_test_teardown();
    }

    // ── profile_sample tests (P8-10) ──

    /// Helper: set up profiling state for sample collection tests.
    ///
    /// Sets SPROFILING=true, resets SPROF_INFO to default, and sets
    /// SPROF_MEM_SIZE to a reasonable value. Returns a guard that
    /// tears down on drop.
    fn profile_sample_setup(mem_size: usize) -> impl Drop {
        let lock = sprof_test_setup();
        SPROFILING.store(true, Ordering::Release);
        SPROF_MEM_SIZE.store(mem_size, Ordering::Relaxed);
        // SAFETY: BKL is held (simulated by test lock); SPROF_INFO accessed via addr_of_mut!.
        let info = core::ptr::addr_of_mut!(SPROF_INFO);
        unsafe { *info = SprofInfo::default(); }
        // Return a guard that calls sprof_test_teardown on drop.
        struct ProfileSampleGuard;
        impl Drop for ProfileSampleGuard {
            fn drop(&mut self) {
                SPROF_MEM_SIZE.store(0, Ordering::Relaxed);
                // SAFETY: test lock is still held (sprof_test_setup guard).
                let info = core::ptr::addr_of_mut!(SPROF_INFO);
                unsafe { *info = SprofInfo::default(); }
                SPROFILING.store(false, Ordering::Release);
                sprof_test_teardown();
            }
        }
        let _ = lock;
        ProfileSampleGuard
    }

    #[test]
    fn test_profile_sample_noop_when_not_profiling() {
        // C: profile.c:80 — `if (!sprofiling) return`.
        let _lock = sprof_test_setup();
        // SPROFILING is false (not set).
        let priv_table = PrivTable::new();
        let proc = KProcess::new(ProcNr(0), Endpoint(100));
        // SAFETY: BKL is held (test lock); SPROF_INFO is in default state.
        unsafe {
            profile_sample(&proc, 0xDEAD, &priv_table);
        }
        // No counters should have changed.
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.total_samples, 0);
        assert_eq!(info.mem_used, 0);
        sprof_test_teardown();
    }

    #[test]
    fn test_profile_sample_noop_when_buffer_full() {
        // C: profile.c:81 — `if (sprof_info.mem_used == -1) return`.
        let _guard = profile_sample_setup(SAMPLE_BUFFER_SIZE);
        // SAFETY: BKL is held; SPROF_INFO accessed via addr_of_mut!.
        let info = core::ptr::addr_of_mut!(SPROF_INFO);
        unsafe { (*info).mem_used = -1; }
        let priv_table = PrivTable::new();
        let proc = KProcess::new(ProcNr(0), Endpoint(100));
        // SAFETY: BKL is held.
        unsafe {
            profile_sample(&proc, 0xDEAD, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.total_samples, 0);
        assert_eq!(info.mem_used, -1);
    }

    #[test]
    fn test_profile_sample_idle_increments_idle_samples() {
        // C: profile.c:93 — `if (p->p_endpoint == IDLE) idle_samples++`.
        let _guard = profile_sample_setup(SAMPLE_BUFFER_SIZE);
        let priv_table = PrivTable::new();
        let proc = KProcess::new(ProcNr(0), Endpoint::IDLE);
        // SAFETY: BKL is held; SPROFILING is true.
        unsafe {
            profile_sample(&proc, 0x1000, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.idle_samples, 1);
        assert_eq!(info.system_samples, 0);
        assert_eq!(info.user_samples, 0);
        assert_eq!(info.total_samples, 1);
        // No buffer writes for idle samples.
        assert_eq!(info.mem_used, 0);
    }

    #[test]
    fn test_profile_sample_kernel_endpoint_saves_system_sample() {
        // C: profile.c:94 — `if (p->p_endpoint == KERNEL)`.
        let _guard = profile_sample_setup(SAMPLE_BUFFER_SIZE);
        let priv_table = PrivTable::new();
        let proc = KProcess::new(ProcNr(0), Endpoint::KERNEL);
        // SAFETY: BKL is held; SPROFILING is true.
        unsafe {
            profile_sample(&proc, 0xBADC0DE, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.system_samples, 1);
        assert_eq!(info.idle_samples, 0);
        assert_eq!(info.user_samples, 0);
        assert_eq!(info.total_samples, 1);
        // Should have written a SprofProc record + a SprofSample record.
        let expected = core::mem::size_of::<SprofProc>() + core::mem::size_of::<SprofSample>();
        assert_eq!(info.mem_used as usize, expected);
    }

    #[test]
    fn test_profile_sample_user_process_increments_user_samples() {
        // C: profile.c:106 — user process (not SYS_PROC or not runnable).
        let _guard = profile_sample_setup(SAMPLE_BUFFER_SIZE);
        let priv_table = PrivTable::new();
        // A user process with no priv_id → is_sys_proc_runnable returns false.
        let proc = KProcess::new(ProcNr(0), Endpoint(100));
        // SAFETY: BKL is held; SPROFILING is true.
        unsafe {
            profile_sample(&proc, 0x2000, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.user_samples, 1);
        assert_eq!(info.system_samples, 0);
        assert_eq!(info.idle_samples, 0);
        assert_eq!(info.total_samples, 1);
        // No buffer writes for user samples.
        assert_eq!(info.mem_used, 0);
    }

    #[test]
    fn test_profile_sample_runnable_sys_proc_saves_sample_and_proc() {
        // C: profile.c:95 — `priv(p)->s_flags & SYS_PROC && proc_is_runnable(p)`.
        let _guard = profile_sample_setup(SAMPLE_BUFFER_SIZE);
        let mut priv_table = PrivTable::new();
        // Set up privilege slot 0 with SYS_PROC flag.
        if let Some(p) = priv_table.get_mut(0u16) {
            p.capability.s_flags |= crate::kpriv::PrivFlagsBits::SYS_PROC;
        }
        let mut proc = KProcess::new(ProcNr(0), Endpoint(50));
        proc.priv_id = Some(0u16);
        // Make the process runnable (clear SLOT_FREE).
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // SAFETY: BKL is held; SPROFILING is true.
        unsafe {
            profile_sample(&proc, 0x3000, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.system_samples, 1);
        assert_eq!(info.total_samples, 1);
        // Should have written SprofProc + SprofSample.
        let expected = core::mem::size_of::<SprofProc>() + core::mem::size_of::<SprofSample>();
        assert_eq!(info.mem_used as usize, expected);
        // MF_SPROF_SEEN should be set.
        assert!(proc.p_misc_flags.is_set(MiscFlagsBits::SPROF_SEEN));
    }

    #[test]
    fn test_profile_sample_second_sample_does_not_resave_proc() {
        // C: profile.c:97-100 — MF_SPROF_SEEN gates proc record saving.
        // On the second sample for the same process, only SprofSample is written
        // (not SprofProc again).
        let _guard = profile_sample_setup(SAMPLE_BUFFER_SIZE);
        let mut priv_table = PrivTable::new();
        if let Some(p) = priv_table.get_mut(0u16) {
            p.capability.s_flags |= crate::kpriv::PrivFlagsBits::SYS_PROC;
        }
        let mut proc = KProcess::new(ProcNr(0), Endpoint(50));
        proc.priv_id = Some(0u16);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // SAFETY: BKL is held; SPROFILING is true.
        unsafe {
            profile_sample(&proc, 0x3000, &priv_table);
            profile_sample(&proc, 0x3004, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        assert_eq!(info.system_samples, 2);
        assert_eq!(info.total_samples, 2);
        // 1 SprofProc + 2 SprofSample.
        let expected = core::mem::size_of::<SprofProc>()
            + 2 * core::mem::size_of::<SprofSample>();
        assert_eq!(info.mem_used as usize, expected);
    }

    #[test]
    fn test_profile_sample_buffer_full_marks_mem_used_minus1() {
        // C: profile.c:84-89 — space check fails → mem_used = -1.
        // Set SPROF_MEM_SIZE to a very small value so the space check fails.
        let _guard = profile_sample_setup(10);
        let priv_table = PrivTable::new();
        let proc = KProcess::new(ProcNr(0), Endpoint::KERNEL);
        // SAFETY: BKL is held; SPROFILING is true.
        unsafe {
            profile_sample(&proc, 0xDEAD, &priv_table);
        }
        // SAFETY: BKL is held.
        let info = unsafe { SPROF_INFO };
        // mem_used should be -1 (buffer full marker).
        assert_eq!(info.mem_used, -1);
        // No sample should have been counted.
        assert_eq!(info.total_samples, 0);
    }
}
