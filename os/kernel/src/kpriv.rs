use minix_types::Endpoint;

use crate::errno::{ENOSPC, EINVAL, EBUSY};
use crate::proc::{ProcNr, SigSet};
use crate::proc::NR_BOOT_PROCS;
use crate::proc_table::NR_TASKS;

pub type PrivId = u16;
pub type SysId = u16;

// C: minix/include/minix/config.h:52,55,58
pub const NR_IO_RANGE: usize = 64;
pub const NR_MEM_RANGE: usize = 20;
pub const NR_IRQ: usize = 16;

// C: minix/include/minix/com.h:272 — SYS_CALL_MASK_SIZE = BITMAP_CHUNKS(NR_SYS_CALLS) = 2
pub const SYS_CALL_MASK_SIZE: usize = 2;

// C: minix/include/minix/priv.h:28-29 — kernel call mask constants.
/// No kernel calls allowed (`NO_C`). Used for kernel tasks and user processes.
pub const K_CALL_MASK_NONE: [u32; SYS_CALL_MASK_SIZE] = [0; SYS_CALL_MASK_SIZE];
/// All kernel calls allowed (`ALL_C`). Used for system services (VM, RS, etc.).
pub const K_CALL_MASK_ALL: [u32; SYS_CALL_MASK_SIZE] = [0xFFFF_FFFF; SYS_CALL_MASK_SIZE];

// C: minix/include/minix/priv.h:24-25 — IPC target mask constants.
/// No IPC targets allowed (`NO_M`). Used for kernel tasks.
pub const IPC_TO_NONE: u64 = 0;
/// All IPC targets allowed (`ALL_M`). Used for system services (VM, RS, etc.).
pub const IPC_TO_ALL: u64 = !0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoRange {
    pub base: u32,
    pub limit: u32,
}

impl Default for IoRange {
    fn default() -> Self {
        Self::new()
    }
}

impl IoRange {
    pub const fn new() -> Self {
        Self { base: 0, limit: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemRange {
    pub base: u64,
    pub limit: u64,
}

impl Default for MemRange {
    fn default() -> Self {
        Self::new()
    }
}

impl MemRange {
    pub const fn new() -> Self {
        Self { base: 0, limit: 0 }
    }
}

bitflags::bitflags! {
    /// Privilege flags for `KPriv::s_flags`.
    ///
    /// C: minix/include/minix/const.h:143-154
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PrivFlagsBits: u16 {
        const PREEMPTIBLE     = 0x002;  // const.h:143
        const BILLABLE        = 0x004;  // const.h:144
        const DYN_PRIV_ID     = 0x008;  // const.h:145
        const SYS_PROC        = 0x010;  // const.h:147
        const CHECK_IO_PORT   = 0x020;  // const.h:148
        const CHECK_IRQ       = 0x040;  // const.h:149
        const CHECK_MEM       = 0x080;  // const.h:150
        const ROOT_SYS_PROC   = 0x100;  // const.h:151
        const VM_SYS_PROC     = 0x200;  // const.h:152
        const LU_SYS_PROC     = 0x400;  // const.h:153
        const RST_SYS_PROC    = 0x800;  // const.h:154
    }
}

/// Predefined privilege flag combinations for process types.
///
/// C: minix/include/minix/priv.h:36-49 — `IDL_F`, `TSK_F`, `SRV_F`, etc.
pub mod priv_flag_set {
    use super::PrivFlagsBits as F;
    /// C: priv.h:36 — IDL_F = SYS_PROC | BILLABLE (idle is not preemptible)
    pub const IDL_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::BILLABLE.bits());
    /// C: priv.h:44 — TSK_F = SYS_PROC (other kernel tasks)
    pub const TSK_F: F = F::from_bits_truncate(F::SYS_PROC.bits());
    /// C: priv.h:45 — SRV_F = SYS_PROC | PREEMPTIBLE (system services)
    pub const SRV_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::PREEMPTIBLE.bits());
    /// C: priv.h:46 — DSRV_F = SRV_F | DYN_PRIV_ID (dynamic system services)
    pub const DSRV_F: F = F::from_bits_truncate(SRV_F.bits() | F::DYN_PRIV_ID.bits());
    /// C: priv.h:47 — RSYS_F = SRV_F | ROOT_SYS_PROC (root system proc)
    pub const RSYS_F: F = F::from_bits_truncate(SRV_F.bits() | F::ROOT_SYS_PROC.bits());
    /// C: priv.h:48 — VM_F = SYS_PROC | VM_SYS_PROC (vm)
    pub const VM_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::VM_SYS_PROC.bits());
    /// C: priv.h:49 — USR_F = BILLABLE | PREEMPTIBLE (user processes)
    pub const USR_F: F = F::from_bits_truncate(F::BILLABLE.bits() | F::PREEMPTIBLE.bits());
}

/// C: minix/include/minix/priv.h:18 — USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR)
/// ROOT_USR_PROC_NR = INIT_PROC_NR = 11, so USER_PRIV_ID = NR_TASKS + 11 = 16
pub const INIT_PROC_NR: PrivId = 11;
pub const USER_PRIV_ID: PrivId = NR_TASKS as PrivId + INIT_PROC_NR;

/// C: minix/include/minix/priv.h:12 — static_priv_id(n) = NR_TASKS + n
#[inline]
pub fn static_priv_id(proc_nr: ProcNr) -> PrivId {
    (NR_TASKS as i32 + proc_nr.0) as PrivId
}

/// C: minix/include/minix/priv.h:11 — is_static_priv_id(id)
#[inline]
pub fn is_static_priv_id(id: PrivId) -> bool {
    let nr_static = NR_BOOT_PROCS as PrivId;
    id < nr_static
}

/// C: minix/include/minix/priv.h:14 — NULL_PRIV_ID = -1
pub const NULL_PRIV_ID: PrivId = u16::MAX;

// ── KPriv 6 substructures (06-design-final.md §12.9) ─────────────────────
//
// KPriv is split into 6 substructures by responsibility. Each is its own
// `Default`/`const fn new()`-constructible type so the kernel layer can
// pre-build `PrivTable` via `[KPriv; NR_SYS_PROCS]` instead of heap-allocating.
//
// Field name C-prefix `s_` is preserved for traceability to Minix3's `struct priv`.

/// Identity / capability metadata (was `s_proc_nr`, `s_id`, `s_flags`,
/// `s_init_flags`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivCapability {
    pub(crate) s_proc_nr: Option<ProcNr>,
    pub(crate) s_id: SysId,
    pub(crate) s_flags: PrivFlagsBits,
    pub(crate) s_init_flags: i32,
}

impl Default for PrivCapability {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivCapability {
    /// Const-constructible zeroed capability (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_proc_nr: None,
            s_id: 0,
            s_flags: PrivFlagsBits::empty(),
            s_init_flags: 0,
        }
    }
}

/// Signal bookkeeping (asynchronous table, manager, pending signals).
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivSignals {
    pub(crate) s_asyntab: u64,
    pub(crate) s_asynsize: usize,
    pub(crate) s_asynendpoint: Endpoint,
    pub(crate) s_sig_mgr: Endpoint,
    pub(crate) s_bak_sig_mgr: Endpoint,
    pub(crate) s_notify_pending: u64,
    pub(crate) s_asyn_pending: u64,
    pub(crate) s_int_pending: u32,
    pub(crate) s_sig_pending: SigSet,
}

impl Default for PrivSignals {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivSignals {
    /// Const-constructible zeroed signals (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_asyntab: 0,
            s_asynsize: 0,
            s_asynendpoint: Endpoint::NONE,
            s_sig_mgr: Endpoint::NONE,
            s_bak_sig_mgr: Endpoint::NONE,
            s_notify_pending: 0,
            s_asyn_pending: 0,
            s_int_pending: 0,
            s_sig_pending: SigSet::empty(),
        }
    }
}

/// IPC allowlists (trap, ipc-to, kernel-call masks).
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivIpc {
    pub(crate) s_trap_mask: u16,
    pub(crate) s_ipc_to: u64,
    pub(crate) s_k_call_mask: [u32; SYS_CALL_MASK_SIZE],
}

impl PrivIpc {
    /// Const-constructible zeroed IPC masks (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_trap_mask: 0,
            s_ipc_to: 0,
            s_k_call_mask: [0; SYS_CALL_MASK_SIZE],
        }
    }
}

impl Default for PrivIpc {
    fn default() -> Self {
        Self::new()
    }
}

/// I/O port + IRQ allowlists.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivIo {
    pub(crate) s_nr_io_range: i32,
    pub(crate) s_io_tab: [IoRange; NR_IO_RANGE],
    pub(crate) s_nr_irq: i32,
    pub(crate) s_irq_tab: [i32; NR_IRQ],
}

impl Default for PrivIo {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivIo {
    /// Const-constructible zeroed I/O + IRQ allowlists (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_nr_io_range: 0,
            s_io_tab: [IoRange::new(); NR_IO_RANGE],
            s_nr_irq: 0,
            s_irq_tab: [0; NR_IRQ],
        }
    }
}

/// Memory-range allowlists + cross-space IPC + stack guard + diag signal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivMem {
    pub(crate) s_nr_mem_range: i32,
    pub(crate) s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub(crate) s_ipcf: Option<usize>,
    /// C: `s_stack_guard` — priv.h. Stack guard page address. Currently
    /// stored but not read — used by future stack overflow detection.
    #[allow(dead_code)]
    pub(crate) s_stack_guard: Option<usize>,
    pub(crate) s_diag_sig: bool,
}

impl Default for PrivMem {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivMem {
    /// Const-constructible zeroed memory allowlists (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_nr_mem_range: 0,
            s_mem_tab: [MemRange::new(); NR_MEM_RANGE],
            s_ipcf: None,
            s_stack_guard: None,
            s_diag_sig: false,
        }
    }
}

/// Init flags + alarm timer + grant table + state table (volatile state).
#[derive(Debug, Clone)]
pub(crate) struct PrivRuntime {
    /// Synchronous alarm timer + its `TimerId` for queue management.
    ///
    /// `None` when no alarm is set. When `Some`, holds `(entry, id)` where
    /// `id` is the `TimerId` returned by `ClockState::set_timer()`, used to
    /// call `ClockState::reset_timer(id)` when the alarm is cancelled or
    /// replaced (15-design.md §4.4).
    pub(crate) s_alarm_timer: Option<(crate::clock::TimerEntry, crate::clock::TimerId)>,
    pub(crate) s_grant_table: usize,
    pub(crate) s_grant_entries: i32,
    pub(crate) s_grant_endpoint: Endpoint,
    pub(crate) s_state_table: usize,
    pub(crate) s_state_entries: i32,
}

impl PrivRuntime {
    /// Const-constructible zeroed runtime (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_alarm_timer: None,
            s_grant_table: 0,
            s_grant_entries: 0,
            s_grant_endpoint: Endpoint::NONE,
            s_state_table: 0,
            s_state_entries: 0,
        }
    }
}

impl Default for PrivRuntime {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct KPriv {
    pub(crate) capability: PrivCapability,
    pub(crate) signals: PrivSignals,
    pub(crate) ipc: PrivIpc,
    pub(crate) io: PrivIo,
    pub(crate) mem: PrivMem,
    pub(crate) runtime: PrivRuntime,
}

impl KPriv {
    pub fn new(id: SysId) -> Self {
        Self {
            capability: PrivCapability {
                s_proc_nr: None,
                s_id: id,
                s_flags: PrivFlagsBits::empty(),
                s_init_flags: 0,
            },
            signals: PrivSignals::default(),
            ipc: PrivIpc::default(),
            io: PrivIo::default(),
            mem: PrivMem::default(),
            runtime: PrivRuntime::default(),
        }
    }

    /// Const-constructible zeroed KPriv with `s_id = id` (for `const fn`
    /// `PrivTable::new()` / `static mut` init).
    ///
    /// See `06-design-final.md` §4.1 / §17.1.
    pub const fn new_zeroed(id: SysId) -> Self {
        Self {
            capability: PrivCapability {
                s_proc_nr: None,
                s_id: id,
                s_flags: PrivFlagsBits::empty(),
                s_init_flags: 0,
            },
            signals: PrivSignals::new(),
            ipc: PrivIpc::new(),
            io: PrivIo::new(),
            mem: PrivMem::new(),
            runtime: PrivRuntime::new(),
        }
    }

    pub fn is_sys_proc(&self) -> bool {
        self.capability.s_flags.contains(PrivFlagsBits::SYS_PROC)
    }

    /// C: `priv(rp)->s_flags & PREEMPTIBLE` — scheduler preemption check.
    /// Currently unused — scheduler not yet wired to check this flag.
    #[allow(dead_code)]
    pub fn is_preemptible(&self) -> bool {
        self.capability.s_flags.contains(PrivFlagsBits::PREEMPTIBLE)
    }

    /// C: `priv(rp)->s_flags & BILLABLE` — CPU time accounting check.
    /// Currently unused — CPU accounting not yet wired.
    #[allow(dead_code)]
    pub fn is_billable(&self) -> bool {
        self.capability.s_flags.contains(PrivFlagsBits::BILLABLE)
    }

    pub fn may_send_to(&self, target_id: SysId) -> bool {
        if target_id as usize >= 64 {
            return false;
        }
        (self.ipc.s_ipc_to & (1u64 << target_id)) != 0
    }

    /// Add an IRQ to this priv's allowlist.
    /// C: `priv_add_irq()` — system.c:918-942.
    ///
    /// Sets `CHECK_IRQ` flag, deduplicates, and appends to `s_irq_tab`.
    /// Returns `Err(())` if the table is full (`NR_IRQ` exceeded).
    pub fn add_irq(&mut self, irq: i32) -> Result<(), ()> {
        self.capability.s_flags |= PrivFlagsBits::CHECK_IRQ;
        // Dedup check
        for i in 0..self.io.s_nr_irq as usize {
            if self.io.s_irq_tab[i] == irq {
                return Ok(());
            }
        }
        let i = self.io.s_nr_irq as usize;
        if i >= NR_IRQ {
            return Err(());
        }
        self.io.s_irq_tab[i] = irq;
        self.io.s_nr_irq += 1;
        Ok(())
    }

    /// Add an I/O port range to this priv's allowlist.
    /// C: `priv_add_io()` — system.c:945-969.
    ///
    /// Sets `CHECK_IO_PORT` flag, deduplicates, and appends to `s_io_tab`.
    /// Returns `Err(())` if the table is full (`NR_IO_RANGE` exceeded).
    pub fn add_io(&mut self, ior: &IoRange) -> Result<(), ()> {
        self.capability.s_flags |= PrivFlagsBits::CHECK_IO_PORT;
        // Dedup check
        for i in 0..self.io.s_nr_io_range as usize {
            if self.io.s_io_tab[i].base == ior.base
                && self.io.s_io_tab[i].limit == ior.limit
            {
                return Ok(());
            }
        }
        let i = self.io.s_nr_io_range as usize;
        if i >= NR_IO_RANGE {
            return Err(());
        }
        self.io.s_io_tab[i] = *ior;
        self.io.s_nr_io_range += 1;
        Ok(())
    }

    /// Add a memory range to this priv's allowlist.
    /// C: `priv_add_mem()` — system.c:973-997.
    ///
    /// Sets `CHECK_MEM` flag, deduplicates, and appends to `s_mem_tab`.
    /// Returns `Err(())` if the table is full (`NR_MEM_RANGE` exceeded).
    pub fn add_mem(&mut self, memr: &MemRange) -> Result<(), ()> {
        self.capability.s_flags |= PrivFlagsBits::CHECK_MEM;
        // Dedup check
        for i in 0..self.mem.s_nr_mem_range as usize {
            if self.mem.s_mem_tab[i].base == memr.base
                && self.mem.s_mem_tab[i].limit == memr.limit
            {
                return Ok(());
            }
        }
        let i = self.mem.s_nr_mem_range as usize;
        if i >= NR_MEM_RANGE {
            return Err(());
        }
        self.mem.s_mem_tab[i] = *memr;
        self.mem.s_nr_mem_range += 1;
        Ok(())
    }

    /// Update privilege fields from a user-supplied request struct.
    /// C: `update_priv()` — do_privctl.c:280-368.
    ///
    /// Copies flags, signal managers, IRQ/I-O/memory tables (gated by
    /// CHECK_IRQ / CHECK_IO_PORT / CHECK_MEM), trap mask, IPC target
    /// mask, and kernel-call mask from `req` into `self`.
    ///
    /// Returns `Err(())` (→ EINVAL in C) if any count is out of range.
    pub fn update_from_request(&mut self, req: &PrivUpdateRequest) -> Result<(), ()> {
        // C: do_privctl.c:287-290 — copy flags + signal managers.
        self.capability.s_flags = req.s_flags;
        self.capability.s_init_flags = req.s_init_flags;
        self.signals.s_sig_mgr = req.s_sig_mgr;
        self.signals.s_bak_sig_mgr = req.s_bak_sig_mgr;

        // C: do_privctl.c:293-305 — copy IRQs (gated by CHECK_IRQ).
        if req.s_flags.contains(PrivFlagsBits::CHECK_IRQ) {
            if req.s_nr_irq < 0 || req.s_nr_irq as usize > NR_IRQ {
                return Err(());
            }
            self.io.s_nr_irq = req.s_nr_irq;
            for i in 0..req.s_nr_irq as usize {
                self.io.s_irq_tab[i] = req.s_irq_tab[i];
            }
        }

        // C: do_privctl.c:308-322 — copy I/O ranges (gated by CHECK_IO_PORT).
        if req.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
            if req.s_nr_io_range < 0 || req.s_nr_io_range as usize > NR_IO_RANGE {
                return Err(());
            }
            self.io.s_nr_io_range = req.s_nr_io_range;
            for i in 0..req.s_nr_io_range as usize {
                self.io.s_io_tab[i] = req.s_io_tab[i];
            }
        }

        // C: do_privctl.c:325-339 — copy memory ranges (gated by CHECK_MEM).
        if req.s_flags.contains(PrivFlagsBits::CHECK_MEM) {
            if req.s_nr_mem_range < 0 || req.s_nr_mem_range as usize > NR_MEM_RANGE {
                return Err(());
            }
            self.mem.s_nr_mem_range = req.s_nr_mem_range;
            for i in 0..req.s_nr_mem_range as usize {
                self.mem.s_mem_tab[i] = req.s_mem_tab[i];
            }
        }

        // C: do_privctl.c:342 — copy trap mask.
        self.ipc.s_trap_mask = req.s_trap_mask;

        // C: do_privctl.c:353 — fill_sendto_mask(rp, &priv->s_ipc_to).
        // In Rust, s_ipc_to is a direct u64 bitmap (no separate fill step).
        self.ipc.s_ipc_to = req.s_ipc_to;

        // C: do_privctl.c:364-365 — copy kernel call mask.
        self.ipc.s_k_call_mask = req.s_k_call_mask;

        Ok(())
    }

    /// Reset all pending IPC state (used by SET_SYS after copying caller's priv).
    /// C: do_privctl.c:121-131 — clear s_asyn_pending, s_notify_pending,
    /// s_int_pending, s_sig_pending, s_alarm_timer, s_asyntab, s_asynsize,
    /// s_asynendpoint, s_diag_sig.
    pub fn reset_pending_ipc(&mut self) {
        self.signals.s_asyn_pending = 0;
        self.signals.s_notify_pending = 0;
        self.signals.s_int_pending = 0;
        self.signals.s_sig_pending = SigSet::empty();
        self.runtime.s_alarm_timer = None;
        self.signals.s_asyntab = 0;
        self.signals.s_asynsize = 0;
        self.signals.s_asynendpoint = Endpoint::NONE;
        self.mem.s_diag_sig = false;
    }

    /// Reset all resource counters to zero (used by SET_SYS defaults).
    /// C: do_privctl.c:156-164 — s_nr_io_range=0, s_nr_mem_range=0,
    /// s_nr_irq=0, s_grant_table=0, s_grant_entries=0, s_grant_endpoint,
    /// s_state_table=0, s_state_entries=0, s_ipcf=0.
    pub fn reset_resources(&mut self, endpoint: Endpoint) {
        self.io.s_nr_io_range = 0;
        self.mem.s_nr_mem_range = 0;
        self.io.s_nr_irq = 0;
        self.runtime.s_grant_table = 0;
        self.runtime.s_grant_entries = 0;
        self.runtime.s_grant_endpoint = endpoint;
        self.runtime.s_state_table = 0;
        self.runtime.s_state_entries = 0;
        self.mem.s_ipcf = None;
    }
}

/// User-space-visible privilege update request.
///
/// This is the Rust equivalent of C's `struct priv` as passed to
/// `SYS_PRIV_SET_SYS` / `SYS_PRIV_UPDATE_SYS` via `arg_ptr`. Unlike C's
/// `struct priv` (which includes kernel-internal timer/pointer fields),
/// this struct contains ONLY the fields that `update_priv` actually reads.
///
/// # Layout
///
/// `#[repr(C)]` ensures deterministic layout for cross-space `data_copy`.
/// The user-space RS (Reincarnation Server) fills this struct and passes
/// its address via `m_lsys_krn_sys_privctl.arg_ptr`.
///
/// C: `struct priv` — kernel/priv.h:21-66; `update_priv` — do_privctl.c:280-368.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PrivUpdateRequest {
    /// C: `s_flags` — privilege flags (PREEMPTIBLE, BILLABLE, SYS_PROC, etc.)
    pub s_flags: PrivFlagsBits,
    /// C: `s_init_flags` — initialization flags.
    pub s_init_flags: i32,
    /// C: `s_sig_mgr` — signal manager endpoint.
    pub s_sig_mgr: Endpoint,
    /// C: `s_bak_sig_mgr` — backup signal manager endpoint.
    pub s_bak_sig_mgr: Endpoint,
    /// C: `s_trap_mask` — allowed system call traps.
    pub s_trap_mask: u16,
    /// C: `s_ipc_to` — allowed IPC destination bitmap.
    pub s_ipc_to: u64,
    /// C: `s_k_call_mask` — allowed kernel calls bitmap.
    pub s_k_call_mask: [u32; SYS_CALL_MASK_SIZE],
    /// C: `s_nr_irq` — number of valid IRQ entries (gated by CHECK_IRQ).
    pub s_nr_irq: i32,
    /// C: `s_irq_tab` — IRQ allowlist table.
    pub s_irq_tab: [i32; NR_IRQ],
    /// C: `s_nr_io_range` — number of valid I/O range entries (gated by CHECK_IO_PORT).
    pub s_nr_io_range: i32,
    /// C: `s_io_tab` — I/O port range allowlist table.
    pub s_io_tab: [IoRange; NR_IO_RANGE],
    /// C: `s_nr_mem_range` — number of valid memory range entries (gated by CHECK_MEM).
    pub s_nr_mem_range: i32,
    /// C: `s_mem_tab` — memory range allowlist table.
    pub s_mem_tab: [MemRange; NR_MEM_RANGE],
    /// C: `s_id` — privilege ID (for SET_SYS static allocation; ignored if DYN_PRIV_ID set).
    pub s_id: SysId,
}

impl PrivUpdateRequest {
    /// Create a zeroed request (all fields default/empty).
    pub const fn new() -> Self {
        Self {
            s_flags: PrivFlagsBits::empty(),
            s_init_flags: 0,
            s_sig_mgr: Endpoint::NONE,
            s_bak_sig_mgr: Endpoint::NONE,
            s_trap_mask: 0,
            s_ipc_to: 0,
            s_k_call_mask: [0; SYS_CALL_MASK_SIZE],
            s_nr_irq: 0,
            s_irq_tab: [0; NR_IRQ],
            s_nr_io_range: 0,
            s_io_tab: [IoRange::new(); NR_IO_RANGE],
            s_nr_mem_range: 0,
            s_mem_tab: [MemRange::new(); NR_MEM_RANGE],
            s_id: 0,
        }
    }
}

impl Default for PrivUpdateRequest {
    fn default() -> Self {
        Self::new()
    }
}

pub const NR_SYS_PROCS: usize = 64;

/// Kernel privilege table.
///
/// # Storage (06-design-final.md §4.1)
///
/// `privs` is a fixed-size array `[KPriv; NR_SYS_PROCS]`, NOT a
/// `Box<[KPriv]>`. This eliminates heap allocation in the boot phase
/// (`#![no_std]` + no allocator yet) and gives a compile-time-fixed address
/// (matching C's `EXTERN struct priv priv[NR_SYS_PROCS]` in BSS).
///
/// The global instance lives in `static mut PRIV_TABLE` (see `lib.rs`).
pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}

impl PrivTable {
    /// Const-constructible privilege table (for `static PRIV_TABLE` init).
    ///
    /// Each slot starts with `s_id = i`, `s_proc_nr = None`.
    ///
    /// See `06-design-final.md` §4.1.
    pub const fn new() -> Self {
        let mut privs = [const { KPriv::new_zeroed(0) }; NR_SYS_PROCS];
        let mut i = 0;
        while i < NR_SYS_PROCS {
            privs[i].capability.s_id = i as SysId;
            i += 1;
        }
        Self { privs }
    }

    pub(crate) fn get(&self, id: PrivId) -> Option<&KPriv> {
        let idx = id as usize;
        if idx < NR_SYS_PROCS {
            Some(&self.privs[idx])
        } else {
            None
        }
    }

    pub(crate) fn get_mut(&mut self, id: PrivId) -> Option<&mut KPriv> {
        let idx = id as usize;
        if idx < NR_SYS_PROCS {
            Some(&mut self.privs[idx])
        } else {
            None
        }
    }

    /// Swap two privilege slots.
    ///
    /// Used by `do_update` (SYS_UPDATE) to swap src and dst priv slots.
    /// C: `*src_privp = orig_dst_priv; *dst_privp = orig_src_priv;`
    /// — do_update.c:131-133.
    ///
    /// Uses `split_at_mut` to obtain two simultaneous `&mut` references
    /// from the same array (the borrow checker cannot prove non-aliasing
    /// otherwise).
    pub(crate) fn swap_slots(&mut self, a: PrivId, b: PrivId) {
        let ia = a as usize;
        let ib = b as usize;
        debug_assert!(ia < NR_SYS_PROCS && ib < NR_SYS_PROCS);
        if ia == ib {
            return;
        }
        if ia < ib {
            let (left, right) = self.privs.split_at_mut(ib);
            core::mem::swap(&mut left[ia], &mut right[0]);
        } else {
            let (left, right) = self.privs.split_at_mut(ia);
            core::mem::swap(&mut left[ib], &mut right[0]);
        }
    }

    pub fn init(&mut self) {
        for (i, priv_) in self.privs.iter_mut().enumerate() {
            *priv_ = KPriv::new(i as SysId);
        }
    }

    /// Assign a static privilege to a boot process.
    ///
    /// Corresponds to Minix3's `get_priv(rp, static_priv_id(proc_nr))`.
    /// static_priv_id maps: `priv_id = NR_TASKS + proc_nr` (for proc_nr >= 0).
    /// Kernel tasks (proc_nr < 0) use the same formula since their priv_id
    /// is determined by the static slot layout: IDLE=0, CLOCK=1, SYSTEM=2, KERNEL=3.
    ///
    /// C: `get_priv()` — system.c:272-311, `static_priv_id()` — priv.h:12
    ///
    /// # Returns
    /// `Some(priv_id)` on success, `None` if priv_id is out of range or
    /// the slot is already occupied by another process.
    pub fn assign_static(&mut self, proc_nr: ProcNr) -> Option<PrivId> {
        // C: priv_id = static_priv_id(proc_nr) = NR_TASKS + proc_nr
        // For kernel tasks (proc_nr < 0), proc_nr maps to priv_id directly:
        //   IDLE=-4 → priv_id=0, CLOCK=-3 → priv_id=1, etc.
        // For user processes (proc_nr >= 0): priv_id = NR_TASKS + proc_nr
        let priv_id = if proc_nr.0 < 0 {
            (NR_TASKS as i32 + proc_nr.0) as PrivId
        } else {
            (NR_TASKS as PrivId + proc_nr.0 as PrivId) as PrivId
        };

        let priv_ = self.get_mut(priv_id)?;

        // C: if(priv[priv_id].s_proc_nr != NONE) return EBUSY
        if priv_.capability.s_proc_nr.is_some() {
            return None;
        }

        // C: rc->p_priv = sp; sp->s_proc_nr = proc_nr(rc)
        priv_.capability.s_proc_nr = Some(proc_nr);

        Some(priv_id)
    }

    /// Allocate a privilege slot for a process (static or dynamic).
    ///
    /// C: `get_priv()` — system.c:274-302.
    ///
    /// If `priv_id == NULL_PRIV_ID` (u16::MAX), scan the dynamic slot range
    /// `[NR_BOOT_PROCS .. NR_SYS_PROCS)` for a free slot (s_proc_nr == None).
    /// Returns `Err(ENOSPC)` if all dynamic slots are occupied.
    ///
    /// Otherwise, allocate the specified static slot (must be `< NR_BOOT_PROCS`
    /// and unoccupied). Returns `Err(EINVAL)` for invalid static IDs and
    /// `Err(EBUSY)` if the slot is already in use.
    ///
    /// On success, links the process to the slot: sets `s_proc_nr = proc_nr`
    /// and returns `Ok(priv_id)`.
    pub fn get_priv(&mut self, proc_nr: ProcNr, priv_id: PrivId) -> Result<PrivId, i32> {
        if priv_id == NULL_PRIV_ID {
            // C: system.c:285-287 — scan dynamic slots for a free one.
            for i in (NR_BOOT_PROCS as PrivId)..(NR_SYS_PROCS as PrivId) {
                if let Some(slot) = self.get(i)
                    && slot.capability.s_proc_nr.is_none() {
                        // Found a free dynamic slot.
                        if let Some(slot) = self.get_mut(i) {
                            slot.capability.s_proc_nr = Some(proc_nr);
                        }
                        return Ok(i);
                    }
            }
            Err(ENOSPC)
        } else {
            // C: system.c:289-297 — allocate static slot by id.
            if !is_static_priv_id(priv_id) {
                return Err(EINVAL);
            }
            if let Some(slot) = self.get(priv_id) {
                if slot.capability.s_proc_nr.is_some() {
                    return Err(EBUSY);
                }
            } else {
                return Err(EINVAL);
            }
            if let Some(slot) = self.get_mut(priv_id) {
                slot.capability.s_proc_nr = Some(proc_nr);
            }
            Ok(priv_id)
        }
    }

    /// Set privilege flags, trap mask, IPC mask, kernel call mask, and
    /// scheduling parameters for a boot process. Corresponds to the per-type
    /// privilege setup in main.c:178-248.
    ///
    /// C: main.c:178-248 (sets s_flags, s_trap_mask, s_ipc_to, s_k_call_mask, priority, quantum)
    // R-18 (2026-08-13): Legacy 2-step API (assign_static + configure_boot_priv),
    // replaced by grant_capability() for new callers. 8 params match C field set.
    // Kept for internal use (grant_capability delegates to it). Allowed per clippy.
    #[allow(clippy::too_many_arguments)]
    pub fn configure_boot_priv(
        &mut self,
        priv_id: PrivId,
        flags: PrivFlagsBits,
        init_flags: i32,
        trap_mask: u16,
        ipc_to: u64,
        k_call_mask: [u32; 2],
        sig_mgr: Endpoint,
    ) {
        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.capability.s_flags = flags;
            priv_.capability.s_init_flags = init_flags;
            priv_.ipc.s_trap_mask = trap_mask;
            priv_.ipc.s_ipc_to = ipc_to;
            priv_.ipc.s_k_call_mask = k_call_mask;
            priv_.signals.s_sig_mgr = sig_mgr;
        }
    }

    /// Grant a capability template to a process (06-design-final.md §3.6).
    ///
    /// Replaces the previous two-step `assign_static` +
    /// `configure_boot_priv` pattern with a single call. Picking one
    /// of the 5 stock templates fills in the right flags + IPC masks
    /// + kernel-call mask by construction; the caller cannot forget
    ///   to set the IPC mask.
    ///
    /// # Returns
    /// `Ok(priv_id)` on success, `Err(CapabilityError)` on failure.
    pub fn grant_capability(
        &mut self,
        proc_nr: ProcNr,
        template: crate::capability::CapabilityTemplate,
    ) -> Result<PrivId, crate::capability::CapabilityError> {
        use crate::capability::CapabilityError;

        let priv_id = self.assign_static(proc_nr)
            .ok_or(CapabilityError::SlotOccupied)?;
        let template_caps = template.capabilities();

        // Translate ProcessCapability (kernel-layer abstraction) to
        // PrivFlagsBits (Minix3 bitflags). The two are different
        // encoding spaces; this mapping is the single source of truth.
        let mut flags = PrivFlagsBits::empty();
        if template_caps.contains(crate::capability::ProcessCapability::SYS_PROC) {
            flags |= PrivFlagsBits::SYS_PROC;
        }
        if template_caps.contains(crate::capability::ProcessCapability::BILLABLE) {
            flags |= PrivFlagsBits::BILLABLE;
        }
        if template_caps.contains(crate::capability::ProcessCapability::VM_F) {
            flags |= PrivFlagsBits::SYS_PROC | PrivFlagsBits::VM_SYS_PROC;
        }
        if template_caps.contains(crate::capability::ProcessCapability::RSYS_F) {
            flags |= PrivFlagsBits::SYS_PROC | PrivFlagsBits::PREEMPTIBLE | PrivFlagsBits::ROOT_SYS_PROC;
        }
        if template_caps.contains(crate::capability::ProcessCapability::IDL_F) {
            flags |= PrivFlagsBits::SYS_PROC;
        }
        if template_caps.contains(crate::capability::ProcessCapability::TSK_F) {
            flags |= PrivFlagsBits::SYS_PROC;
        }

        let trap_mask_bits = template.trap_mask().bits();
        let ipc_to_bits = template.ipc_mask().bits();
        let kcall_mask_bits = template.kcall_mask().bits();

        // sig_mgr defaults to endpoint-of-self (matches C init).
        let sig_mgr = Endpoint::from_generation_slot(0, proc_nr.0);

        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.capability.s_flags = flags;
            priv_.capability.s_init_flags = 0;
            priv_.ipc.s_trap_mask = trap_mask_bits as u16;
            priv_.ipc.s_ipc_to = ipc_to_bits;
            // KCallMask.bits() is u64; pack into [u32; 2] (low word first).
            priv_.ipc.s_k_call_mask = [
                // R-16 (2026-08-12): SAFETY: `kcall_mask_bits` is u64; masking
                // with 0xFFFF_FFFF yields the low 32 bits, which fit in u32.
                (kcall_mask_bits & 0xFFFF_FFFF) as u32,
                // R-16 (2026-08-12): SAFETY: the `>> 32` shifts the high 32
                // bits into the low position, so the result fits in u32.
                (kcall_mask_bits >> 32) as u32,
            ];
            priv_.signals.s_sig_mgr = sig_mgr;
        }
        Ok(priv_id)
    }
}

impl Default for PrivTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kpriv_new() {
        let priv_ = KPriv::new(5);
        assert_eq!(priv_.capability.s_id, 5);
        assert_eq!(priv_.capability.s_proc_nr, None);
        assert_eq!(priv_.capability.s_flags, PrivFlagsBits::empty());
        assert_eq!(priv_.ipc.s_k_call_mask, [0u32; SYS_CALL_MASK_SIZE]);
    }

    #[test]
    fn test_kpriv_is_sys_proc() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_sys_proc());
        priv_.capability.s_flags = PrivFlagsBits::SYS_PROC;
        assert!(priv_.is_sys_proc());
    }

    #[test]
    fn test_kpriv_flag_predicates() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_preemptible());
        assert!(!priv_.is_billable());

        priv_.capability.s_flags = priv_flag_set::USR_F;
        assert!(priv_.is_preemptible());
        assert!(priv_.is_billable());
        assert!(!priv_.is_sys_proc()); // USR_F has no SYS_PROC
    }

    #[test]
    fn test_priv_flag_set_idl() {
        // IDL_F = SYS_PROC | BILLABLE, no PREEMPTIBLE
        let flags = priv_flag_set::IDL_F;
        assert!(flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(flags.contains(PrivFlagsBits::BILLABLE));
        assert!(!flags.contains(PrivFlagsBits::PREEMPTIBLE));
    }

    #[test]
    fn test_priv_flag_set_usr_no_sys_proc() {
        // USR_F = BILLABLE | PREEMPTIBLE — user processes are NOT system processes
        let flags = priv_flag_set::USR_F;
        assert!(!flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(flags.contains(PrivFlagsBits::BILLABLE));
        assert!(flags.contains(PrivFlagsBits::PREEMPTIBLE));
    }

    #[test]
    fn test_priv_flag_set_vm() {
        let flags = priv_flag_set::VM_F;
        assert!(flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(flags.contains(PrivFlagsBits::VM_SYS_PROC));
    }

    #[test]
    fn test_priv_table_new() {
        let table = PrivTable::new();
        assert!(table.get(0).is_some());
        assert!(table.get(63).is_some());
        assert!(table.get(64).is_none());
    }

    #[test]
    fn test_priv_table_const_init_sets_per_slot_s_id() {
        // 06-design-final.md §4.1: PrivTable is `const fn`-initialized with
        // each slot's `s_id = i` and `s_proc_nr = None`.
        let table = PrivTable::new();
        for i in 0..NR_SYS_PROCS {
            let p = table.get(i as PrivId).expect("slot must exist");
            assert_eq!(p.capability.s_id, i as SysId,
                "slot {} s_id mismatch", i);
            assert!(p.capability.s_proc_nr.is_none(),
                "slot {} must start unassigned", i);
        }
    }

    #[test]
    fn test_priv_table_assign_static() {
        let mut table = PrivTable::new();

        // Assign kernel task: IDLE = -4 → priv_id = NR_TASKS + (-4) = 1
        let id = table.assign_static(ProcNr(-4));
        assert!(id.is_some());
        let id = id.unwrap();
        // NR_TASKS=5, proc_nr=-4 → priv_id = 5 + (-4) = 1
        assert_eq!(id, 1);
        assert_eq!(table.get(id).unwrap().capability.s_proc_nr, Some(ProcNr(-4)));

        // Duplicate assignment fails
        let id2 = table.assign_static(ProcNr(-4));
        assert!(id2.is_none());
    }

    #[test]
    fn test_priv_table_assign_static_user_proc() {
        let mut table = PrivTable::new();

        // Assign user process: RS_PROC_NR = 1 → priv_id = NR_TASKS + 1 = 6
        let id = table.assign_static(ProcNr(1));
        assert!(id.is_some());
        let id = id.unwrap();
        assert_eq!(id, NR_TASKS as PrivId + 1);
    }

    #[test]
    fn test_priv_table_configure_boot_priv() {
        let mut table = PrivTable::new();
        let priv_id = table.assign_static(ProcNr(-4)).unwrap();

        table.configure_boot_priv(
            priv_id,
            priv_flag_set::IDL_F,
            0,
            0,
            0,
            [0; 2],
            Endpoint::NONE,
        );

        let priv_ = table.get(priv_id).unwrap();
        assert!(priv_.capability.s_flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(priv_.capability.s_flags.contains(PrivFlagsBits::BILLABLE));
    }

    #[test]
    fn test_k_call_mask_constants() {
        // C: NO_C → all-zero mask; ALL_C → all-one mask.
        assert_eq!(K_CALL_MASK_NONE, [0; SYS_CALL_MASK_SIZE]);
        assert_eq!(K_CALL_MASK_ALL, [0xFFFF_FFFF; SYS_CALL_MASK_SIZE]);
    }

    #[test]
    fn test_ipc_to_constants() {
        // C: NO_M → 0; ALL_M → all bits set.
        assert_eq!(IPC_TO_NONE, 0);
        assert_eq!(IPC_TO_ALL, !0);
    }

    #[test]
    fn test_configure_boot_priv_sets_masks() {
        let mut table = PrivTable::new();
        let priv_id = table.assign_static(ProcNr(0)).unwrap();

        // System services (VM/RS) get ALL_M + ALL_C.
        table.configure_boot_priv(
            priv_id,
            priv_flag_set::RSYS_F,
            0,
            0,
            IPC_TO_ALL,
            K_CALL_MASK_ALL,
            Endpoint::NONE,
        );

        let priv_ = table.get(priv_id).unwrap();
        assert_eq!(priv_.ipc.s_ipc_to, IPC_TO_ALL);
        assert_eq!(priv_.ipc.s_k_call_mask, K_CALL_MASK_ALL);
    }

    #[test]
    fn test_may_send_to() {
        let mut priv_ = KPriv::new(0);
        priv_.ipc.s_ipc_to = 1 << 5;
        assert!(priv_.may_send_to(5));
        assert!(!priv_.may_send_to(3));
        assert!(!priv_.may_send_to(64)); // out of range
    }

    #[test]
    fn test_static_priv_id() {
        // C: static_priv_id(n) = NR_TASKS + n
        assert_eq!(static_priv_id(ProcNr(-4)), 1); // IDLE: 5 + (-4) = 1
        assert_eq!(static_priv_id(ProcNr(0)), NR_TASKS as PrivId); // DS: 5 + 0 = 5
        assert_eq!(static_priv_id(ProcNr(1)), NR_TASKS as PrivId + 1); // RS: 5 + 1 = 6
    }

    #[test]
    fn test_is_static_priv_id() {
        assert!(is_static_priv_id(0));
        assert!(is_static_priv_id(NR_BOOT_PROCS as PrivId - 1));
        assert!(!is_static_priv_id(NR_BOOT_PROCS as PrivId));
    }

    #[test]
    fn test_user_priv_id() {
        // C: USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR) = NR_TASKS + 11 = 16
        assert_eq!(USER_PRIV_ID, NR_TASKS as PrivId + 11);
    }

    #[test]
    fn test_io_range_new() {
        let range = IoRange::new();
        assert_eq!(range.base, 0);
        assert_eq!(range.limit, 0);
    }

    #[test]
    fn test_mem_range_new() {
        let range = MemRange::new();
        assert_eq!(range.base, 0);
        assert_eq!(range.limit, 0);
    }

    #[test]
    fn test_kpriv_alarm_timer_default_none() {
        // Doc 21 §4.2 / Ch3 D7 promises `s_alarm_timer: Option<(TimerEntry, TimerId)>`.
        // Default state must be `None` (no alarm pending), matching C's
        // `tmr_inittimer(&sp->s_alarm_timer)` at system.c:180.
        let p = KPriv::new(0);
        assert!(p.runtime.s_alarm_timer.is_none());
    }

    #[test]
    fn test_kpriv_alarm_timer_some_carries_action() {
        // Unlike C's bare `u64` (which lost TimerAction), the tuple
        // `(TimerEntry, TimerId)` preserves exp_time, action, and the stable
        // TimerId needed for reset_timer(id). This enforces "非法状态不可表达".
        use crate::clock::{TimerAction, TimerEntry, TimerId};
        use minix_types::Endpoint;
        let mut p = KPriv::new(0);
        p.runtime.s_alarm_timer = Some((
            TimerEntry {
                exp_time: 1000,
                action: TimerAction::NotifyAlarm {
                    endpoint: Endpoint::NONE,
                },
            },
            TimerId::new(42),
        ));
        let (entry, id) = p.runtime.s_alarm_timer.as_ref().unwrap();
        assert_eq!(entry.exp_time, 1000);
        assert_eq!(id.raw(), 42);
        assert!(matches!(entry.action, TimerAction::NotifyAlarm { .. }));
    }

    // ── grant_capability tests (06-design-final.md §3.6) ──────────────

    use crate::capability::CapabilityTemplate;

    #[test]
    fn test_grant_capability_idle() {
        let mut table = PrivTable::new();
        let id = table.grant_capability(ProcNr(-4), CapabilityTemplate::Idle).unwrap();
        let p = table.get(id).unwrap();
        assert!(p.capability.s_flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(p.capability.s_flags.contains(PrivFlagsBits::BILLABLE));
        assert_eq!(p.ipc.s_ipc_to, 0);
        assert_eq!(p.ipc.s_k_call_mask, [0u32; SYS_CALL_MASK_SIZE]);
    }

    #[test]
    fn test_grant_capability_vm() {
        let mut table = PrivTable::new();
        let id = table.grant_capability(ProcNr(8), CapabilityTemplate::Vm).unwrap();
        let p = table.get(id).unwrap();
        assert!(p.capability.s_flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(p.capability.s_flags.contains(PrivFlagsBits::VM_SYS_PROC));
        // VM is a system service: ALL IPC + ALL kcalls
        assert_eq!(p.ipc.s_ipc_to, !0u64);
        assert_eq!(p.ipc.s_k_call_mask, [0xFFFF_FFFF; SYS_CALL_MASK_SIZE]);
    }

    #[test]
    fn test_grant_capability_root_service() {
        let mut table = PrivTable::new();
        let id = table.grant_capability(ProcNr(1), CapabilityTemplate::RootService).unwrap();
        let p = table.get(id).unwrap();
        assert!(p.capability.s_flags.contains(PrivFlagsBits::ROOT_SYS_PROC));
        assert!(p.capability.s_flags.contains(PrivFlagsBits::PREEMPTIBLE));
        assert_eq!(p.ipc.s_ipc_to, !0u64);
    }

    #[test]
    fn test_grant_capability_deferred_no_flags() {
        let mut table = PrivTable::new();
        let id = table.grant_capability(ProcNr(0), CapabilityTemplate::Deferred).unwrap();
        let p = table.get(id).unwrap();
        assert_eq!(p.capability.s_flags, PrivFlagsBits::empty());
        assert_eq!(p.ipc.s_ipc_to, 0);
        assert_eq!(p.ipc.s_k_call_mask, [0u32; SYS_CALL_MASK_SIZE]);
    }

    #[test]
    fn test_grant_capability_duplicate_fails() {
        use crate::capability::CapabilityError;
        let mut table = PrivTable::new();
        assert!(table.grant_capability(ProcNr(-4), CapabilityTemplate::Idle).is_ok());
        // Same proc_nr again should fail (slot occupied)
        assert_eq!(
            table.grant_capability(ProcNr(-4), CapabilityTemplate::Idle),
            Err(CapabilityError::SlotOccupied)
        );
    }
}
