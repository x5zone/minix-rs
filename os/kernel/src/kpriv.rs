use minix_types::Endpoint;

use crate::capability::{IpcMask, KCallMask, ProcessCapability, TrapMask};
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

/// C: minix/include/minix/priv.h:21 — NULL_PRIV_ID = -1
pub const NULL_PRIV_ID: PrivId = u16::MAX;

// ── KPriv 8 substructures (06-proc-init-boot-proc.md §3.10) ─────────────────────
//
// KPriv is split into 8 substructures by responsibility: PrivIdentity
// (s_proc_nr, s_id), PrivFlags (s_flags), PrivInit (s_init_flags),
// PrivSignals, PrivIpc, PrivIo, PrivMem, PrivRuntime. Each is its own
// `Default`/`const fn new()`-constructible type so the kernel layer can
// pre-build `PrivTable` via `[KPriv; NR_SYS_PROCS]` instead of heap-allocating.
//
// Field name C-prefix `s_` is preserved for traceability to Minix3's `struct priv`.

/// Process identity binding (was C's `s_proc_nr` + `s_id`).
///
/// `s_proc_nr` is `None` when the slot is unassigned (C's `NONE = -1`).
/// `s_id` is the slot index in `PrivTable` (matches C's `s_id` field at
/// `priv.h:24`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PrivIdentity {
    pub(crate) s_proc_nr: Option<ProcNr>,
    pub(crate) s_id: SysId,
}

impl PrivIdentity {
    /// Const-constructible zeroed identity (for `const fn` table init).
    ///
    /// Currently unused — `KPriv::new()` uses the struct literal initializer
    /// directly. Kept for symmetry with the other substructures (`PrivFlags`,
    /// `PrivInit`) and to allow future callers (e.g. `PrivTable::new()` could
    /// migrate to `PrivIdentity::new()` if the `s_id` per-slot init is folded
    /// into identity construction).
    #[allow(dead_code)]
    pub const fn new() -> Self {
        Self {
            s_proc_nr: None,
            s_id: 0,
        }
    }

    /// True if this slot is bound to a process (C: `priv[priv_id].s_proc_nr != NONE`).
    ///
    /// Currently unused — callers inline `.identity.s_proc_nr.is_some()`.
    /// Kept as semantic surface for the PrivIdentity abstraction; future
    /// callers (e.g. enum dispatch by slot state) can use this.
    #[allow(dead_code)]
    pub(crate) fn is_assigned(&self) -> bool {
        self.s_proc_nr.is_some()
    }
}

/// Init-stage flags (was C's `s_init_flags`).
///
/// These flags are set during boot (`init_priv`) and gradually cleared
/// as init completes (e.g., `DSRV_I` cleared after `update_from_request`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PrivInit {
    pub(crate) s_init_flags: i32,
}

impl PrivInit {
    /// Const-constructible zeroed init state.
    pub const fn new() -> Self {
        Self { s_init_flags: 0 }
    }
}

/// Capability flags (was C's `s_flags`, bit values at `const.h:143-154`).
///
/// Stored as the kernel-layer [`ProcessCapability`] newtype
/// ([`crate::capability`]); the Minix3 wire layout (`u16`) is converted at
/// the `PrivUpdateRequest` boundary via `from_wire`/`to_wire`, so callers
/// see capability semantics rather than raw wire bits.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PrivFlags {
    pub(crate) s_flags: ProcessCapability,
}

impl PrivFlags {
    /// Const-constructible empty flags.
    pub const fn new() -> Self {
        Self {
            s_flags: ProcessCapability::empty(),
        }
    }

    pub(crate) fn is_sys_proc(&self) -> bool {
        self.s_flags.contains(ProcessCapability::SYS_PROC)
    }

    /// C: `priv(rp)->s_flags & PREEMPTIBLE`.
    ///
    /// Currently unused — scheduler preemption check not yet wired.
    /// Kept as semantic surface for the PrivFlags abstraction (mirrors the
    /// prior `KPriv::is_preemptible` API); future scheduler code should call
    /// this rather than inline `.flags.s_flags.contains(PREEMPTIBLE)`.
    #[allow(dead_code)]
    pub(crate) fn is_preemptible(&self) -> bool {
        self.s_flags.contains(ProcessCapability::PREEMPTIBLE)
    }

    /// C: `priv(rp)->s_flags & BILLABLE`.
    ///
    /// Currently unused — CPU time accounting not yet wired. Same rationale
    /// as [`Self::is_preemptible`].
    #[allow(dead_code)]
    pub(crate) fn is_billable(&self) -> bool {
        self.s_flags.contains(ProcessCapability::BILLABLE)
    }

    /// Kind of system process (4 mutually-exclusive bits at
    /// `const.h:151-154`: ROOT_SYS_PROC / VM_SYS_PROC / LU_SYS_PROC /
    /// RST_SYS_PROC). Returns `None` if no kind bit is set.
    ///
    /// Currently unused — first-match wins (matches C convention of
    /// `if (s_flags & ROOT_SYS_PROC)` checks). Kept as semantic surface
    /// for the PrivFlags abstraction; future sysproc-dispatch code (e.g.
    /// endpoint validation, capability gates) should call this rather than
    /// inlining bit checks. Documented in `06-proc-init-boot-proc.md §3.10`.
    #[allow(dead_code)]
    pub(crate) fn sys_proc_kind(&self) -> Option<SysProcKind> {
        if self.s_flags.contains(ProcessCapability::ROOT_SYS_PROC) {
            Some(SysProcKind::Root)
        } else if self.s_flags.contains(ProcessCapability::VM_SYS_PROC) {
            Some(SysProcKind::Vm)
        } else if self.s_flags.contains(ProcessCapability::LU_SYS_PROC) {
            Some(SysProcKind::Lu)
        } else if self.s_flags.contains(ProcessCapability::RST_SYS_PROC) {
            Some(SysProcKind::Rst)
        } else {
            None
        }
    }
}

/// Mutually-exclusive kind of system process (`s_flags` ROOT_SYS_PROC /
/// VM_SYS_PROC / LU_SYS_PROC / RST_SYS_PROC). Replaces the 4-bit
/// convention with a type-system-enforced enum.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SysProcKind {
    Root,
    Vm,
    Lu,
    Rst,
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
///
/// Field types are the [`crate::capability`] mask newtypes (`TrapMask` /
/// `IpcMask` / `KCallMask`); the Minix3 wire widths (`u16` / `u64` /
/// `[u32; SYS_CALL_MASK_SIZE]`) are converted at the `PrivUpdateRequest`
/// boundary via each type's `from_wire`/`to_wire` codec.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrivIpc {
    pub(crate) s_trap_mask: TrapMask,
    pub(crate) s_ipc_to: IpcMask,
    pub(crate) s_k_call_mask: KCallMask,
}

impl PrivIpc {
    /// Const-constructible zeroed IPC masks (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            s_trap_mask: TrapMask::NONE,
            s_ipc_to: IpcMask::NONE,
            s_k_call_mask: KCallMask::NONE,
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
    /// Synchronous alarm timer node, embedded (C-isomorphic).
    ///
    /// C: `minix_timer_t s_alarm_timer` — kernel/priv.h:48. The node is not
    /// a separate allocation: it lives inside the privilege slot and is
    /// linked into the clock's sorted chain by `crate::clock::set_alarm_timer`
    /// / unlinked by `reset_alarm_timer` (C: `clock_timers`, clock.c:37).
    /// A node is "set" iff `action.is_some()` (C: `tmr_is_set`, timers.h:52).
    pub(crate) s_alarm_timer: crate::clock::AlarmTimerNode,
    pub(crate) s_grant_table: usize,
    pub(crate) s_grant_entries: i32,
    pub(crate) s_grant_endpoint: Endpoint,
    pub(crate) s_state_table: usize,
    pub(crate) s_state_entries: i32,
}

impl PrivRuntime {
    /// Const-constructible zeroed runtime (for `const fn` table init).
    ///
    /// `s_alarm_timer` starts as `AlarmTimerNode::new()` — C:
    /// `tmr_inittimer(&sp->s_alarm_timer)` at system.c:180
    /// (`tmr_func = NULL; tmr_next = NULL`, timers.h:64).
    pub const fn new() -> Self {
        Self {
            s_alarm_timer: crate::clock::AlarmTimerNode::new(),
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

/// KPriv (kernel-side process privilege) — 06-proc-init-boot-proc.md §3.10.
///
/// Split into 8 substructures by responsibility / lock granularity:
/// - [`PrivIdentity`]: identity binding (`s_proc_nr`, `s_id`)
/// - [`PrivFlags`]: capability bitmask (`s_flags`)
/// - [`PrivInit`]: init-stage flags (`s_init_flags`)
/// - [`PrivSignals`]: signal bookkeeping
/// - [`PrivIpc`]: IPC allowlists
/// - [`PrivIo`]: I/O port + IRQ allowlists
/// - [`PrivMem`]: memory-range allowlists + cross-space IPC
/// - [`PrivRuntime`]: volatile runtime state (alarm timer, grant/state tables)
pub(crate) struct KPriv {
    pub(crate) identity: PrivIdentity,
    pub(crate) flags: PrivFlags,
    pub(crate) init: PrivInit,
    pub(crate) signals: PrivSignals,
    pub(crate) ipc: PrivIpc,
    pub(crate) io: PrivIo,
    pub(crate) mem: PrivMem,
    pub(crate) runtime: PrivRuntime,
}

/// Slot-ownership invariant (design: `panic-in-drop.md`, §1/§2, class B).
///
/// A `KPriv` is a **privilege-table slot**, not a plain Rust value: its
/// lifetime is bound to the process it is granted to (`identity.s_proc_nr`),
/// and it carries embedded state referenced from other slots (the alarm
/// timer chain node `runtime.s_alarm_timer`, the grant table, the IPC
/// allowlist). Destroying an *assigned* slot via Rust's implicit
/// destruction semantics would silently unlink that state while the rest
/// of the kernel still refers to it — kernel invariant violation.
///
/// A slot is "assigned" iff `identity.s_proc_nr.is_some()` (C:
/// `priv[priv_id].s_proc_nr != NONE` — `priv.h`); an unassigned slot is a
/// waiting `None` binding and drops as a plain empty value. Assigning /
/// releasing a slot must go through the `PrivTable` lifecycle APIs
/// (`grant_capability`, slot clear → `s_proc_nr = None`), never through
/// Rust `Drop`.
impl Drop for KPriv {
    fn drop(&mut self) {
        if self.slot_is_occupied() {
            panic!(
                "BUG: occupied KPriv slot (s_id={}) dropped without explicit destruction; \
                 clear the slot binding first (s_proc_nr = None)",
                self.identity.s_id
            );
        }
    }
}

impl KPriv {
    /// Whether this privilege slot is bound to a process
    /// (`identity.s_proc_nr.is_some()`, i.e. C's `s_proc_nr != NONE`).
    ///
    /// Extracted from `Drop` so the decision logic is unit-testable in a
    /// `panic = "abort"` build (where `#[should_panic]` cannot catch the
    /// abort — see `panic-in-drop.md` §5).
    pub(crate) fn slot_is_occupied(&self) -> bool {
        self.identity.s_proc_nr.is_some()
    }

    pub fn new(id: SysId) -> Self {
        Self {
            identity: PrivIdentity {
                s_proc_nr: None,
                s_id: id,
            },
            flags: PrivFlags::new(),
            init: PrivInit::new(),
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
    /// See `06-proc-init-boot-proc.md` §3.2 / §3.8.
    pub const fn new_zeroed(id: SysId) -> Self {
        Self {
            identity: PrivIdentity {
                s_proc_nr: None,
                s_id: id,
            },
            flags: PrivFlags::new(),
            init: PrivInit::new(),
            signals: PrivSignals::new(),
            ipc: PrivIpc::new(),
            io: PrivIo::new(),
            mem: PrivMem::new(),
            runtime: PrivRuntime::new(),
        }
    }

    pub fn is_sys_proc(&self) -> bool {
        self.flags.is_sys_proc()
    }

    /// C: `priv(rp)->s_flags & PREEMPTIBLE` — scheduler preemption check.
    /// Currently unused — scheduler not yet wired to check this flag.
    #[allow(dead_code)]
    pub fn is_preemptible(&self) -> bool {
        self.flags.is_preemptible()
    }

    /// C: `priv(rp)->s_flags & BILLABLE` — CPU time accounting check.
    /// Currently unused — CPU accounting not yet wired.
    #[allow(dead_code)]
    pub fn is_billable(&self) -> bool {
        self.flags.is_billable()
    }

    pub fn may_send_to(&self, target_id: SysId) -> bool {
        if target_id as usize >= 64 {
            return false;
        }
        self.ipc.s_ipc_to.may_send_to(target_id as u8)
    }

    /// Add an IRQ to this priv's allowlist.
    /// C: `priv_add_irq()` — system.c:918-942.
    ///
    /// Sets `CHECK_IRQ` flag, deduplicates, and appends to `s_irq_tab`.
    /// Returns `Err(())` if the table is full (`NR_IRQ` exceeded).
    pub fn add_irq(&mut self, irq: i32) -> Result<(), ()> {
        self.flags.s_flags |= ProcessCapability::CHECK_IRQ;
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
        self.flags.s_flags |= ProcessCapability::CHECK_IO_PORT;
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
        self.flags.s_flags |= ProcessCapability::CHECK_MEM;
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
    /// This is the **wire boundary**: `req` carries the Minix3 raw widths
    /// (`s_flags: u16`, `s_trap_mask: u16`, `s_ipc_to: u64`,
    /// `s_k_call_mask: [u32; 2]`); the kernel-side newtype fields are
    /// produced here via the `from_wire` codecs. The `u16` trap mask is
    /// sign-extended exactly like C's `short` → `int` promotion at
    /// proc.c:552.
    ///
    /// Returns `Err(())` (→ EINVAL in C) if any count is out of range.
    pub fn update_from_request(&mut self, req: &PrivUpdateRequest) -> Result<(), ()> {
        // C: do_privctl.c:287-290 — copy flags + signal managers.
        let flags = ProcessCapability::from_wire(req.s_flags);
        self.flags.s_flags = flags;
        self.init.s_init_flags = req.s_init_flags;
        self.signals.s_sig_mgr = req.s_sig_mgr;
        self.signals.s_bak_sig_mgr = req.s_bak_sig_mgr;

        // C: do_privctl.c:293-305 — copy IRQs (gated by CHECK_IRQ).
        if flags.contains(ProcessCapability::CHECK_IRQ) {
            if req.s_nr_irq < 0 || req.s_nr_irq as usize > NR_IRQ {
                return Err(());
            }
            self.io.s_nr_irq = req.s_nr_irq;
            for i in 0..req.s_nr_irq as usize {
                self.io.s_irq_tab[i] = req.s_irq_tab[i];
            }
        }

        // C: do_privctl.c:308-322 — copy I/O ranges (gated by CHECK_IO_PORT).
        if flags.contains(ProcessCapability::CHECK_IO_PORT) {
            if req.s_nr_io_range < 0 || req.s_nr_io_range as usize > NR_IO_RANGE {
                return Err(());
            }
            self.io.s_nr_io_range = req.s_nr_io_range;
            for i in 0..req.s_nr_io_range as usize {
                self.io.s_io_tab[i] = req.s_io_tab[i];
            }
        }

        // C: do_privctl.c:325-339 — copy memory ranges (gated by CHECK_MEM).
        if flags.contains(ProcessCapability::CHECK_MEM) {
            if req.s_nr_mem_range < 0 || req.s_nr_mem_range as usize > NR_MEM_RANGE {
                return Err(());
            }
            self.mem.s_nr_mem_range = req.s_nr_mem_range;
            for i in 0..req.s_nr_mem_range as usize {
                self.mem.s_mem_tab[i] = req.s_mem_tab[i];
            }
        }

        // C: do_privctl.c:342 — copy trap mask (u16 wire → sign-extended
        // effective mask, matching C's short→int promotion).
        self.ipc.s_trap_mask = TrapMask::from_wire(req.s_trap_mask);

        // C: do_privctl.c:353 — fill_sendto_mask(rp, &priv->s_ipc_to).
        // In Rust, s_ipc_to is a direct u64 bitmap (no separate fill step).
        self.ipc.s_ipc_to = IpcMask::from_bits(req.s_ipc_to);

        // C: do_privctl.c:364-365 — copy kernel call mask.
        self.ipc.s_k_call_mask = KCallMask::from_wire(req.s_k_call_mask);

        Ok(())
    }

    /// Reset all pending IPC state (used by SET_SYS after copying caller's priv).
    /// C: do_privctl.c:121-131 — clear s_asyn_pending, s_notify_pending,
    /// s_int_pending, s_sig_pending, s_alarm_timer, s_asyntab, s_asynsize,
    /// s_asynendpoint, s_diag_sig.
    ///
    /// The alarm line resets the local node only (C: `tmr_inittimer`
    /// semantics, timers.h:64). The chain-aware unlink (C:
    /// `reset_kernel_timer`, do_privctl.c:127) is performed by the caller
    /// via `crate::clock::reset_alarm_timer` BEFORE this method, because
    /// the chain head lives in `ClockState`, not in `KPriv`.
    pub fn reset_pending_ipc(&mut self) {
        self.signals.s_asyn_pending = 0;
        self.signals.s_notify_pending = 0;
        self.signals.s_int_pending = 0;
        self.signals.s_sig_pending = SigSet::empty();
        self.runtime.s_alarm_timer = crate::clock::AlarmTimerNode::new();
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
/// Field order mirrors the KPriv 8-substructure sequence
/// (06-proc-init-boot-proc.md §3.10), minus the kernel-internal
/// `PrivRuntime`: identity → flags → init → signal managers → IPC masks →
/// I/O then IRQ → memory (each resource group lists its count before the
/// table, matching `PrivIo`/`PrivMem`). The RS-side fill struct `Privilege`
/// (`os/servers/rs/src/privilege.rs`) lists its fields in the same order,
/// so the two protocol ends can be compared field-by-field.
///
/// C: `struct priv` — kernel/priv.h:21-66; `update_priv` — do_privctl.c:280-368.
//
// NOTE: `s_sig_mgr` / `s_bak_sig_mgr` precede the IPC masks here, but in
//   C's `struct priv` (kernel/priv.h:40-41) they come after `s_k_call_mask`.
//   This is **not** a hard contract — Rust
//   `PrivUpdateRequest` is an INDEPENDENT struct (separate from C `struct
//   priv`); it only needs to be internally consistent with the RS server
//   (`os/servers/rs/`) payload layout and `data_copy` size.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PrivUpdateRequest {
    // ── PrivIdentity ────────────────────────────────────────────────
    /// C: `s_id` — privilege ID (for SET_SYS static allocation; ignored if DYN_PRIV_ID set).
    pub s_id: SysId,
    // ── PrivFlags ───────────────────────────────────────────────────
    /// C: `s_flags` — privilege flags (PREEMPTIBLE, BILLABLE, SYS_PROC, etc.).
    /// Raw wire width (`u16`, bit layout `const.h:143-154`); the kernel
    /// decodes it to [`crate::capability::ProcessCapability`] in
    /// [`KPriv::update_from_request`].
    pub s_flags: u16,
    // ── PrivInit ────────────────────────────────────────────────────
    /// C: `s_init_flags` — initialization flags.
    pub s_init_flags: i32,
    // ── PrivSignals (managers only; pending state is kernel-internal) ──
    /// C: `s_sig_mgr` — signal manager endpoint.
    pub s_sig_mgr: Endpoint,
    /// C: `s_bak_sig_mgr` — backup signal manager endpoint.
    pub s_bak_sig_mgr: Endpoint,
    // ── PrivIpc ─────────────────────────────────────────────────────
    /// C: `s_trap_mask` — allowed system call traps.
    pub s_trap_mask: u16,
    /// C: `s_ipc_to` — allowed IPC destination bitmap.
    pub s_ipc_to: u64,
    /// C: `s_k_call_mask` — allowed kernel calls bitmap.
    pub s_k_call_mask: [u32; SYS_CALL_MASK_SIZE],
    // ── PrivIo (I/O first, then IRQ — same order as KPriv's `PrivIo`) ──
    /// C: `s_nr_io_range` — number of valid I/O range entries (gated by CHECK_IO_PORT).
    pub s_nr_io_range: i32,
    /// C: `s_io_tab` — I/O port range allowlist table.
    pub s_io_tab: [IoRange; NR_IO_RANGE],
    /// C: `s_nr_irq` — number of valid IRQ entries (gated by CHECK_IRQ).
    pub s_nr_irq: i32,
    /// C: `s_irq_tab` — IRQ allowlist table.
    pub s_irq_tab: [i32; NR_IRQ],
    // ── PrivMem ─────────────────────────────────────────────────────
    /// C: `s_nr_mem_range` — number of valid memory range entries (gated by CHECK_MEM).
    pub s_nr_mem_range: i32,
    /// C: `s_mem_tab` — memory range allowlist table.
    pub s_mem_tab: [MemRange; NR_MEM_RANGE],
}

impl PrivUpdateRequest {
    /// Create a zeroed request (all fields default/empty).
    pub const fn new() -> Self {
        Self {
            s_id: 0,
            s_flags: 0,
            s_init_flags: 0,
            s_sig_mgr: Endpoint::NONE,
            s_bak_sig_mgr: Endpoint::NONE,
            s_trap_mask: 0,
            s_ipc_to: 0,
            s_k_call_mask: [0; SYS_CALL_MASK_SIZE],
            s_nr_io_range: 0,
            s_io_tab: [IoRange::new(); NR_IO_RANGE],
            s_nr_irq: 0,
            s_irq_tab: [0; NR_IRQ],
            s_nr_mem_range: 0,
            s_mem_tab: [MemRange::new(); NR_MEM_RANGE],
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
/// # Storage (06-proc-init-boot-proc.md §3.2)
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
    /// See `06-proc-init-boot-proc.md` §3.2.
    pub const fn new() -> Self {
        let mut privs = [const { KPriv::new_zeroed(0) }; NR_SYS_PROCS];
        let mut i = 0;
        while i < NR_SYS_PROCS {
            privs[i].identity.s_id = i as SysId;
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
        if priv_.identity.s_proc_nr.is_some() {
            return None;
        }

        // C: rc->p_priv = sp; sp->s_proc_nr = proc_nr(rc)
        priv_.identity.s_proc_nr = Some(proc_nr);

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
                    && slot.identity.s_proc_nr.is_none() {
                        // Found a free dynamic slot.
                        if let Some(slot) = self.get_mut(i) {
                            slot.identity.s_proc_nr = Some(proc_nr);
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
                if slot.identity.s_proc_nr.is_some() {
                    return Err(EBUSY);
                }
            } else {
                return Err(EINVAL);
            }
            if let Some(slot) = self.get_mut(priv_id) {
                slot.identity.s_proc_nr = Some(proc_nr);
            }
            Ok(priv_id)
        }
    }

    /// Set privilege flags, trap mask, IPC mask, kernel call mask, and
    /// scheduling parameters for a boot process. Corresponds to the per-type
    /// privilege setup in main.c:178-248.
    ///
    /// C: main.c:178-248 (sets s_flags, s_trap_mask, s_ipc_to, s_k_call_mask, priority, quantum)
    //
    // Lower-level counterpart of [`PrivTable::grant_capability`]: this 2-step
    // API (assign_static + configure_boot_priv) lets callers set fields the
    // capability templates don't expose. Parameters are the kernel-side
    // capability types ([`ProcessCapability`] / mask newtypes); the Minix3
    // wire encodings are applied only at the `PrivUpdateRequest` boundary.
    pub fn configure_boot_priv(
        &mut self,
        priv_id: PrivId,
        flags: ProcessCapability,
        init_flags: i32,
        trap_mask: TrapMask,
        ipc_to: IpcMask,
        k_call_mask: KCallMask,
        sig_mgr: Endpoint,
    ) {
        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.flags.s_flags = flags;
            priv_.init.s_init_flags = init_flags;
            priv_.ipc.s_trap_mask = trap_mask;
            priv_.ipc.s_ipc_to = ipc_to;
            priv_.ipc.s_k_call_mask = k_call_mask;
            priv_.signals.s_sig_mgr = sig_mgr;
        }
    }

    /// Grant a capability template to a process (06-proc-init-boot-proc.md §3.2).
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

        // `capabilities()` returns the exact C flag set for the template
        // (priv.h:36-49 combos); it stores directly in `KPriv` — no
        // cross-bitflags translation.
        let flags = template.capabilities();

        // Start from the role default, then apply per-process exceptions
        // (mirrors C's `main.c:218-219` ternary). The CLOCK/SYSTEM exception
        // cannot live in `CapabilityTemplate::trap_mask()` because templates
        // are role-level abstractions (proc_nr is instance-level).
        let mut trap_mask = template.trap_mask();
        if matches!(template, crate::capability::CapabilityTemplate::KernelTask) {
            // Newtype comparison (no raw-slot deconstruction): same
            // `(proc_nr == CLOCK || proc_nr == SYSTEM)` test as C main.c:218-219.
            if proc_nr == crate::proc::proc_nr::CLOCK
                || proc_nr == crate::proc::proc_nr::SYSTEM
            {
                trap_mask = TrapMask::RECEIVE;
            }
        }

        // sig_mgr defaults to endpoint-of-self (matches C init).
        let sig_mgr = Endpoint::from_generation_slot(0, proc_nr.0);

        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.flags.s_flags = flags;
            priv_.init.s_init_flags = 0;
            priv_.ipc.s_trap_mask = trap_mask;
            priv_.ipc.s_ipc_to = template.ipc_mask();
            priv_.ipc.s_k_call_mask = template.kcall_mask();
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
        assert_eq!(priv_.identity.s_id, 5);
        assert_eq!(priv_.identity.s_proc_nr, None);
        assert_eq!(priv_.flags.s_flags, ProcessCapability::empty());
        assert_eq!(priv_.ipc.s_k_call_mask, KCallMask::NONE);
    }

    #[test]
    fn test_kpriv_is_sys_proc() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_sys_proc());
        priv_.flags.s_flags = ProcessCapability::SYS_PROC;
        assert!(priv_.is_sys_proc());
    }

    #[test]
    fn test_kpriv_flag_predicates() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_preemptible());
        assert!(!priv_.is_billable());

        priv_.flags.s_flags = ProcessCapability::USR_F;
        assert!(priv_.is_preemptible());
        assert!(priv_.is_billable());
        assert!(!priv_.is_sys_proc()); // USR_F has no SYS_PROC
    }

    #[test]
    fn test_priv_flag_set_idl() {
        // C: priv.h:36 — IDL_F = SYS_PROC | BILLABLE, no PREEMPTIBLE
        let flags = ProcessCapability::IDL_F;
        assert!(flags.contains(ProcessCapability::SYS_PROC));
        assert!(flags.contains(ProcessCapability::BILLABLE));
        assert!(!flags.contains(ProcessCapability::PREEMPTIBLE));
    }

    #[test]
    fn test_priv_flag_set_usr_no_sys_proc() {
        // C: priv.h:49 — USR_F = BILLABLE | PREEMPTIBLE — user processes are NOT system processes
        let flags = ProcessCapability::USR_F;
        assert!(!flags.contains(ProcessCapability::SYS_PROC));
        assert!(flags.contains(ProcessCapability::BILLABLE));
        assert!(flags.contains(ProcessCapability::PREEMPTIBLE));
    }

    #[test]
    fn test_priv_flag_set_vm() {
        // C: priv.h:48 — VM_F = SYS_PROC | VM_SYS_PROC
        let flags = ProcessCapability::VM_F;
        assert!(flags.contains(ProcessCapability::SYS_PROC));
        assert!(flags.contains(ProcessCapability::VM_SYS_PROC));
    }

    #[test]
    fn test_priv_table_new() {
        let table = crate::test_helpers::test_priv_table();
        assert!(table.get(0).is_some());
        assert!(table.get(63).is_some());
        assert!(table.get(64).is_none());
    }

    // ── Slot-ownership invariant (panic-in-drop.md §2) ──

    /// Dropping an *unassigned* slot (`s_proc_nr = None`) is the normal
    /// lifetime end of a waiting `None` binding — it must not trip the
    /// defensive alarm.
    #[test]
    fn test_drop_unassigned_slot_is_normal() {
        let kp = KPriv::new_zeroed(3); // s_proc_nr = None
        drop(kp); // unassigned slot — no alarm
    }

    /// The Drop decision logic must agree with the process binding.
    /// Same rationale as `proc::tests::test_slot_is_occupied_agrees_with_slot_free_bit`:
    /// the occupied-path cannot be asserted under `panic = "abort"`, so we
    /// pin the decision predicate the `Drop` body is built on.
    #[test]
    fn test_slot_is_occupied_agrees_with_binding() {
        let mut kp = KPriv::new(7);
        assert!(!kp.slot_is_occupied(), "unbound slot is empty");
        kp.identity.s_proc_nr = Some(ProcNr(1));
        assert!(kp.slot_is_occupied(), "bound slot is occupied");
        kp.identity.s_proc_nr = None;
        assert!(!kp.slot_is_occupied(), "binding cleared → empty");
        drop(kp);
    }

    #[test]
    fn test_priv_table_const_init_sets_per_slot_s_id() {
        // 06-proc-init-boot-proc.md §3.2: PrivTable is `const fn`-initialized with
        // each slot's `s_id = i` and `s_proc_nr = None`.
        let table = crate::test_helpers::test_priv_table();
        for i in 0..NR_SYS_PROCS {
            let p = table.get(i as PrivId).expect("slot must exist");
            assert_eq!(p.identity.s_id, i as SysId,
                "slot {} s_id mismatch", i);
            assert!(p.identity.s_proc_nr.is_none(),
                "slot {} must start unassigned", i);
        }
    }

    #[test]
    fn test_priv_table_assign_static() {
        let mut table = crate::test_helpers::test_priv_table();

        // Assign kernel task: IDLE = -4 → priv_id = NR_TASKS + (-4) = 1
        let id = table.assign_static(ProcNr(-4));
        assert!(id.is_some());
        let id = id.unwrap();
        // NR_TASKS=5, proc_nr=-4 → priv_id = 5 + (-4) = 1
        assert_eq!(id, 1);
        assert_eq!(table.get(id).unwrap().identity.s_proc_nr, Some(ProcNr(-4)));

        // Duplicate assignment fails
        let id2 = table.assign_static(ProcNr(-4));
        assert!(id2.is_none());
    }

    #[test]
    fn test_priv_table_assign_static_user_proc() {
        let mut table = crate::test_helpers::test_priv_table();

        // Assign user process: VFS_PROC_NR = 1 → priv_id = NR_TASKS + 1 = 6
        let id = table.assign_static(ProcNr(1));
        assert!(id.is_some());
        let id = id.unwrap();
        assert_eq!(id, NR_TASKS as PrivId + 1);
    }

    #[test]
    fn test_priv_table_configure_boot_priv() {
        let mut table = crate::test_helpers::test_priv_table();
        let priv_id = table.assign_static(ProcNr(-4)).unwrap();

        table.configure_boot_priv(
            priv_id,
            ProcessCapability::IDL_F,
            0,
            TrapMask::NONE,
            IpcMask::NONE,
            KCallMask::NONE,
            Endpoint::NONE,
        );

        let priv_ = table.get(priv_id).unwrap();
        assert!(priv_.flags.s_flags.contains(ProcessCapability::SYS_PROC));
        assert!(priv_.flags.s_flags.contains(ProcessCapability::BILLABLE));
    }

    #[test]
    fn test_configure_boot_priv_sets_masks() {
        let mut table = crate::test_helpers::test_priv_table();
        let priv_id = table.assign_static(ProcNr(0)).unwrap();

        // System services (VM/RS) get ALL_M + ALL_C.
        table.configure_boot_priv(
            priv_id,
            ProcessCapability::RSYS_F,
            0,
            TrapMask::NONE,
            IpcMask::ALL,
            KCallMask::ALL,
            Endpoint::NONE,
        );

        let priv_ = table.get(priv_id).unwrap();
        assert_eq!(priv_.ipc.s_ipc_to, IpcMask::ALL);
        assert_eq!(priv_.ipc.s_k_call_mask, KCallMask::ALL);
    }

    #[test]
    fn test_may_send_to() {
        let mut priv_ = KPriv::new(0);
        priv_.ipc.s_ipc_to = IpcMask::from_bits(1 << 5);
        assert!(priv_.may_send_to(5));
        assert!(!priv_.may_send_to(3));
        assert!(!priv_.may_send_to(64)); // out of range
    }

    #[test]
    fn test_static_priv_id() {
        // C: static_priv_id(n) = NR_TASKS + n
        assert_eq!(static_priv_id(ProcNr(-4)), 1); // IDLE: 5 + (-4) = 1
        assert_eq!(static_priv_id(ProcNr(0)), NR_TASKS as PrivId); // PM: 5 + 0 = 5
        assert_eq!(static_priv_id(ProcNr(1)), NR_TASKS as PrivId + 1); // VFS: 5 + 1 = 6
        assert_eq!(static_priv_id(ProcNr(2)), NR_TASKS as PrivId + 2); // RS: 5 + 2 = 7
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
    fn test_kpriv_alarm_timer_default_unset() {
        // C: `tmr_inittimer(&sp->s_alarm_timer)` at system.c:180 — a fresh
        // slot's node has `tmr_func == NULL` (not set) and `tmr_next == NULL`.
        let p = KPriv::new(0);
        assert!(!p.runtime.s_alarm_timer.is_set());
        assert_eq!(p.runtime.s_alarm_timer.next, None);
        assert_eq!(p.runtime.s_alarm_timer.exp_time, 0);
    }

    #[test]
    fn test_kpriv_alarm_timer_node_carries_action() {
        // The node preserves exp_time + action (C: tmr_exp_time / tmr_func +
        // tmr_arg). Arming is done via `clock::set_alarm_timer`, which also
        // links the node; here we only verify the field surface.
        use crate::clock::TimerAction;
        use minix_types::Endpoint;
        let mut p = KPriv::new(0);
        p.runtime.s_alarm_timer.exp_time = 1000;
        p.runtime.s_alarm_timer.action = Some(TimerAction::NotifyAlarm {
            endpoint: Endpoint::NONE,
        });
        assert!(p.runtime.s_alarm_timer.is_set());
        assert_eq!(p.runtime.s_alarm_timer.exp_time, 1000);
        assert!(matches!(
            p.runtime.s_alarm_timer.action,
            Some(TimerAction::NotifyAlarm { .. })
        ));
    }

    // ── grant_capability tests (06-proc-init-boot-proc.md §3.2) ──────────────

    use crate::capability::CapabilityTemplate;

    #[test]
    fn test_grant_capability_idle() {
        let mut table = crate::test_helpers::test_priv_table();
        let id = table.grant_capability(ProcNr(-4), CapabilityTemplate::Idle).unwrap();
        let p = table.get(id).unwrap();
        assert!(p.flags.s_flags.contains(ProcessCapability::SYS_PROC));
        assert!(p.flags.s_flags.contains(ProcessCapability::BILLABLE));
        assert_eq!(p.ipc.s_trap_mask, TrapMask::NONE);
        assert_eq!(p.ipc.s_ipc_to, IpcMask::NONE);
        assert_eq!(p.ipc.s_k_call_mask, KCallMask::NONE);
    }

    #[test]
    fn test_grant_capability_vm() {
        let mut table = crate::test_helpers::test_priv_table();
        let id = table.grant_capability(ProcNr(8), CapabilityTemplate::Vm).unwrap();
        let p = table.get(id).unwrap();
        assert!(p.flags.s_flags.contains(ProcessCapability::SYS_PROC));
        assert!(p.flags.s_flags.contains(ProcessCapability::VM_SYS_PROC));
        // VM is a system service: SRV_T = ~0 (C: main.c:204-209), ALL IPC + ALL kcalls
        assert_eq!(p.ipc.s_trap_mask, TrapMask::ALL);
        assert_eq!(p.ipc.s_ipc_to, IpcMask::ALL);
        assert_eq!(p.ipc.s_k_call_mask, KCallMask::ALL);
    }

    #[test]
    fn test_grant_capability_root_service() {
        let mut table = crate::test_helpers::test_priv_table();
        let id = table.grant_capability(ProcNr(1), CapabilityTemplate::RootService).unwrap();
        let p = table.get(id).unwrap();
        assert!(p.flags.s_flags.contains(ProcessCapability::ROOT_SYS_PROC));
        assert!(p.flags.s_flags.contains(ProcessCapability::PREEMPTIBLE));
        // Root service: SRV_T = ~0 (C: main.c:214-217), ALL IPC + ALL kcalls
        assert_eq!(p.ipc.s_trap_mask, TrapMask::ALL);
        assert_eq!(p.ipc.s_ipc_to, IpcMask::ALL);
        assert_eq!(p.ipc.s_k_call_mask, KCallMask::ALL);
    }

    #[test]
    fn test_grant_capability_deferred_no_flags() {
        let mut table = crate::test_helpers::test_priv_table();
        let id = table.grant_capability(ProcNr(0), CapabilityTemplate::Deferred).unwrap();
        let p = table.get(id).unwrap();
        assert_eq!(p.flags.s_flags, ProcessCapability::empty());
        assert_eq!(p.ipc.s_trap_mask, TrapMask::NONE);
        assert_eq!(p.ipc.s_ipc_to, IpcMask::NONE);
        assert_eq!(p.ipc.s_k_call_mask, KCallMask::NONE);
    }

    #[test]
    fn test_grant_capability_duplicate_fails() {
        use crate::capability::CapabilityError;
        let mut table = crate::test_helpers::test_priv_table();
        assert!(table.grant_capability(ProcNr(-4), CapabilityTemplate::Idle).is_ok());
        // Same proc_nr again should fail (slot occupied)
        assert_eq!(
            table.grant_capability(ProcNr(-4), CapabilityTemplate::Idle),
            Err(CapabilityError::SlotOccupied)
        );
    }

    /// C: `priv.h:59-60` + `main.c:218-219`. Only CLOCK and SYSTEM receive
    /// `CSK_T = (1 << RECEIVE)`; all other kernel tasks (KERNEL, HARDWARE,
    /// ASYNCM) get `TSK_T = 0`. ASYNCM is `-5` (`proc.rs:118`); HARDWARE's
    /// slot is reserved for the arch layer.
    /// This is the positive half of the two-tier trap_mask design: the
    /// CSK_T exception lives in `grant_capability` (where proc_nr is known),
    /// mirroring C's init-time `(proc_nr == CLOCK || proc_nr == SYSTEM)`.
    #[test]
    fn test_grant_capability_kernel_task_clock_system_csk_t() {
        use crate::proc::proc_nr::{CLOCK, SYSTEM};
        let mut table = crate::test_helpers::test_priv_table();

        // CLOCK (proc_nr = -3): RECEIVE allowed (CSK_T = 1 << 2 = 4).
        let id = table.grant_capability(CLOCK, CapabilityTemplate::KernelTask).unwrap();
        assert_eq!(table.get(id).unwrap().ipc.s_trap_mask, TrapMask::RECEIVE,
            "CLOCK must receive CSK_T");

        // SYSTEM (proc_nr = -2): same as CLOCK.
        let id = table.grant_capability(SYSTEM, CapabilityTemplate::KernelTask).unwrap();
        assert_eq!(table.get(id).unwrap().ipc.s_trap_mask, TrapMask::RECEIVE,
            "SYSTEM must receive CSK_T");
    }

    /// KERNEL and the rest of the kernel-task family (HARDWARE, ASYNCM, …)
    /// receive `TSK_T = 0`. Verified for the documented KERNEL_TASKS slot
    /// values (`-1` = KERNEL, `-5` = ASYNCM).
    #[test]
    fn test_grant_capability_kernel_task_non_cs_gets_tsk_t() {
        use crate::proc::proc_nr::KERNEL;
        let mut table = crate::test_helpers::test_priv_table();

        // KERNEL (proc_nr = -1): TSK_T = 0.
        let id = table.grant_capability(KERNEL, CapabilityTemplate::KernelTask).unwrap();
        assert_eq!(table.get(id).unwrap().ipc.s_trap_mask, TrapMask::NONE,
            "KERNEL must receive TSK_T (0)");

        // ASYNCM (proc_nr = -5) — also a kernel task per KERNEL_TASKS table.
        let id = table.grant_capability(ProcNr(-5), CapabilityTemplate::KernelTask).unwrap();
        assert_eq!(table.get(id).unwrap().ipc.s_trap_mask, TrapMask::NONE,
            "ASYNCM must receive TSK_T (0)");
    }

    /// All kernel tasks share `ipc_to = NO_M` regardless of whether they
    /// receive CSK_T or TSK_T. The trap_mask exception does not loosen
    /// the SEND restriction. C: `priv.h:66` `TSK_M = NO_M`.
    #[test]
    fn test_grant_capability_kernel_task_ipc_to_consistent_no_send() {
        use crate::proc::proc_nr::{CLOCK, KERNEL, SYSTEM};
        let mut table = crate::test_helpers::test_priv_table();

        // CLOCK / SYSTEM / KERNEL all share ipc_to = NO_M.
        for nr in [CLOCK, SYSTEM, KERNEL] {
            let id = table.grant_capability(nr, CapabilityTemplate::KernelTask).unwrap();
            assert_eq!(table.get(id).unwrap().ipc.s_ipc_to, IpcMask::NONE,
                "{nr:?} ipc_to must be NO_M (kernel tasks cannot SEND)");
        }
    }

    /// Wire boundary of `update_from_request` (C: do_privctl.c:280-368):
    /// the request carries Minix3 raw widths; the kernel decodes via
    /// `from_wire`. Covers the C bit positions (const.h:143-154), the
    /// `short` → `int` sign extension of `s_trap_mask` (proc.c:552), and
    /// dropping of undefined wire bits.
    #[test]
    fn test_update_from_request_wire_decode() {
        let mut table = crate::test_helpers::test_priv_table();
        let id = table.assign_static(ProcNr(1)).unwrap();

        let mut req = PrivUpdateRequest::new();
        // C wire bits: SYS_PROC = 0x010, CHECK_IRQ = 0x040 (const.h:147,149).
        req.s_flags = 0x050;
        req.s_nr_irq = 1;
        req.s_irq_tab[0] = 5;
        // SRV_T = ~0 as a C short (0xFFFF) — sign-extends to the full mask.
        req.s_trap_mask = 0xFFFF;
        req.s_ipc_to = 1 << 3;
        req.s_k_call_mask = [0xFF, 0];

        {
            let p = table.get_mut(id).unwrap();
            assert!(p.update_from_request(&req).is_ok());
            assert!(p.flags.s_flags.contains(ProcessCapability::SYS_PROC));
            assert!(p.flags.s_flags.contains(ProcessCapability::CHECK_IRQ));
            assert_eq!(p.io.s_nr_irq, 1, "CHECK_IRQ gate must copy the IRQ table");
            assert_eq!(p.ipc.s_trap_mask, TrapMask::ALL,
                "0xFFFF must sign-extend to the all-ones mask (C int promotion)");
            assert!(p.ipc.s_trap_mask.contains(TrapMask::from_bits(1 << 16)),
                "SENDA (call 16) must pass after sign extension");
            assert_eq!(p.ipc.s_ipc_to, IpcMask::from_bits(1 << 3));
            assert_eq!(p.ipc.s_k_call_mask, KCallMask::from_wire([0xFF, 0]));
        }

        // Undefined wire bits (0x7000: bits 12-14, above RST_SYS_PROC=0x800)
        // are dropped by from_bits_truncate — kernel-side state stays clean
        // (C keeps them in s_flags but nothing reads them).
        let mut req2 = PrivUpdateRequest::new();
        req2.s_flags = 0x7000;
        let p = table.get_mut(id).unwrap();
        assert!(p.update_from_request(&req2).is_ok());
        assert_eq!(p.flags.s_flags, ProcessCapability::empty());
    }
}
