//! Kernel process management module.
//!
//! This is the Rust implementation of Minix3's `proc` structure with basic and scheduling fields.
//!
//! # Minix3 Multi-Process Table Architecture
//!
//! Minix3 uses a distributed process table design with 4 copies:
//! - **Kernel/proc**: Scheduling, IPC, register saving (this module)
//! - **PM/mproc**: Process management, signals, permissions (in `minix-pm` crate)
//! - **VM/vmproc**: Virtual memory, page tables (in `minix-vm` crate)
//! - **VFS/fproc**: File descriptors, directories (in `minix-vfs` crate)
//!
//! Each process table is linked via `endpoint`.

use minix_types::{Endpoint, Message, VirBytes, PhysBytes};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicI32, AtomicU8, Ordering};

use minix_arch::{CurrentCpuContext, CurrentCpuContextArch, CpuContextArch, CurrentFpuState};

use crate::vm::{VmSuspendContext, VmSuspendType, VmCheckParams, VmSuspendState, VmCopyContext};
use crate::ipc::SenderQueue;

/// Process number type (corresponds to C's `proc_nr_t`).
///
/// Newtype wrapper providing type safety — prevents accidental mixing of
/// process numbers with raw `i32` values. The inner `i32` is accessible via
/// `.0` for `AtomicI32` interop (`p_nextready`) and array indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ProcNr(pub i32);

impl ProcNr {
    /// Create a process number from a raw `i32`.
    pub const fn new(val: i32) -> Self { ProcNr(val) }
}

impl From<i32> for ProcNr {
    fn from(val: i32) -> Self { ProcNr(val) }
}
impl From<ProcNr> for i32 {
    fn from(nr: ProcNr) -> Self { nr.0 }
}

impl core::ops::Neg for ProcNr {
    type Output = ProcNr;
    fn neg(self) -> ProcNr { ProcNr(-self.0) }
}
impl core::ops::Add for ProcNr {
    type Output = ProcNr;
    fn add(self, rhs: ProcNr) -> ProcNr { ProcNr(self.0 + rhs.0) }
}
impl core::ops::Sub for ProcNr {
    type Output = ProcNr;
    fn sub(self, rhs: ProcNr) -> ProcNr { ProcNr(self.0 - rhs.0) }
}

impl core::fmt::Display for ProcNr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Sentinel value for "no process" in atomic queue pointers.
/// Used by `p_nextready` (AtomicI32). `caller_q` uses `SenderQueue` (VecDeque).
pub const NONE_PROC_NR: i32 = -1;

/// Clock ticks type.
pub type ClockTicks = u64;

/// CPU cycles type.
pub type CpuCycles = u64;

/// Number of kernel tasks.
/// Unified constant from minix_types::NR_TASKS (C: const.h:25).
const NR_TASKS: usize = minix_types::NR_TASKS;

/// Process number constants.
/// Note: In Minix3, p_nr values are slot indices. Kernel tasks have negative
/// p_nr (e.g., CLOCK=-3, SYSTEM=-2, KERNEL=-1), user processes have p_nr >= 0.
/// These are dynamic values assigned at slot allocation, not special constants.
/// The special "NONE" value belongs to Endpoint, not ProcNr.
pub mod proc_nr {
    use super::ProcNr;
    pub const MIN_TASK_NR: ProcNr = ProcNr(-(super::NR_TASKS as i32));
    pub const IDLE: ProcNr = ProcNr(-4);
    pub const CLOCK: ProcNr = ProcNr(-3);
    pub const SYSTEM: ProcNr = ProcNr(-2);
    pub const KERNEL: ProcNr = ProcNr(-1);

    // Boot image user process numbers.
    // C: minix/com.h: PM_PROC_NR=0, RS_PROC_NR=1, VM_PROC_NR=8
    pub const RS_PROC_NR: ProcNr = ProcNr(1);
    pub const VM_PROC_NR: ProcNr = ProcNr(8);
}

/// Boot image dimensions.
/// C: minix/param.h: NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1
///    minix/com.h:  NR_BOOT_MODULES = INIT_PROC_NR + 1 (user-space modules only)
///
/// NR_BOOT_MODULES counts only user-space boot modules (DS, RS, PM, ..., INIT).
/// Kernel tasks (ASYNCM, IDLE, CLOCK, SYSTEM, KERNEL) are hardcoded, not loaded
/// from the multiboot module list.
/// C: image[] in table.c has NR_BOOT_PROCS entries (kernel tasks + user modules).
/// C: kinfo.module_list[] has only user-space modules (NR_BOOT_MODULES entries).
pub const NR_BOOT_MODULES: usize = 12;
pub const NR_BOOT_PROCS: usize = crate::proc_table::NR_TASKS + NR_BOOT_MODULES;

/// Kernel task definitions (hardcoded, matching C's image[] in table.c).
/// C: table.c — struct boot_image image[NR_BOOT_PROCS] = { ... }
/// Kernel tasks are not loaded from multiboot modules — they are compiled
/// into the kernel binary.
pub const KERNEL_TASKS: &[(&str, ProcNr); NR_TASKS] = &[
    ("asyncm", ProcNr(-5)),  // ASYNCM — async message completion notifications
    ("idle",   ProcNr(-4)),  // IDLE — runs when no other process can
    ("clock",  ProcNr(-3)),  // CLOCK — alarms and clock functions
    ("system", ProcNr(-2)),  // SYSTEM — system functionality requests
    ("kernel", ProcNr(-1)),  // KERNEL/HARDWARE — pseudo-process for IPC/scheduling
];

/// User-space boot module process numbers (matching C's image[] in table.c).
/// C: table.c — entries after kernel tasks, in boot image order.
/// C: kinfo.module_list[i] corresponds to image[NR_TASKS + i].
/// The proc_nr values are contiguous: DS=0, RS=1, PM=2, ..., INIT=11.
/// C: minix/com.h — DS_PROC_NR=0, RS_PROC_NR=1, PM_PROC_NR=2, etc.
pub const BOOT_MODULE_PROC_NRS: &[ProcNr; NR_BOOT_MODULES] = &[
    ProcNr(0),   // DS_PROC_NR
    ProcNr(1),   // RS_PROC_NR
    ProcNr(2),   // PM_PROC_NR
    ProcNr(3),   // SCHED_PROC_NR
    ProcNr(4),   // VFS_PROC_NR
    ProcNr(5),   // MEM_PROC_NR
    ProcNr(6),   // TTY_PROC_NR
    ProcNr(7),   // MIB_PROC_NR
    ProcNr(8),   // VM_PROC_NR
    ProcNr(9),   // PFS_PROC_NR
    ProcNr(10),  // MFS_PROC_NR
    ProcNr(11),  // INIT_PROC_NR
];

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RtsFlagsBits: u32 {
        const SLOT_FREE = 0x01;
        const PROC_STOP = 0x02;
        const SENDING = 0x04;
        const RECEIVING = 0x08;
        const SIGNALED = 0x10;
        const SIG_PENDING = 0x20;
        const P_STOP = 0x40;
        const NO_PRIV = 0x80;
        const NO_ENDPOINT = 0x100;
        const VMINHIBIT = 0x200;
        const PAGEFAULT = 0x400;
        const VMREQUEST = 0x800;
        const VMREQTARGET = 0x1000;
        const PREEMPTED = 0x4000;
        const NO_QUANTUM = 0x8000;
        const BOOTINHIBIT = 0x10000;
    }
}

/// Legacy rts constants for backward compatibility during migration.
/// Prefer using `RtsFlagsBits::SLOT_FREE` etc. directly.
pub mod rts {
    pub use super::RtsFlagsBits;
    pub const SLOT_FREE: u32 = super::RtsFlagsBits::SLOT_FREE.bits();
    pub const PROC_STOP: u32 = super::RtsFlagsBits::PROC_STOP.bits();
    pub const SENDING: u32 = super::RtsFlagsBits::SENDING.bits();
    pub const RECEIVING: u32 = super::RtsFlagsBits::RECEIVING.bits();
    pub const SIGNALED: u32 = super::RtsFlagsBits::SIGNALED.bits();
    pub const SIG_PENDING: u32 = super::RtsFlagsBits::SIG_PENDING.bits();
    pub const P_STOP: u32 = super::RtsFlagsBits::P_STOP.bits();
    pub const NO_PRIV: u32 = super::RtsFlagsBits::NO_PRIV.bits();
    pub const NO_ENDPOINT: u32 = super::RtsFlagsBits::NO_ENDPOINT.bits();
    pub const VMINHIBIT: u32 = super::RtsFlagsBits::VMINHIBIT.bits();
    pub const PAGEFAULT: u32 = super::RtsFlagsBits::PAGEFAULT.bits();
    pub const VMREQUEST: u32 = super::RtsFlagsBits::VMREQUEST.bits();
    pub const VMREQTARGET: u32 = super::RtsFlagsBits::VMREQTARGET.bits();
    pub const PREEMPTED: u32 = super::RtsFlagsBits::PREEMPTED.bits();
    pub const NO_QUANTUM: u32 = super::RtsFlagsBits::NO_QUANTUM.bits();
    pub const BOOTINHIBIT: u32 = super::RtsFlagsBits::BOOTINHIBIT.bits();
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MiscFlagsBits: u32 {
        const REPLY_PEND = 0x001;
        const VIRT_TIMER = 0x002;
        const PROF_TIMER = 0x004;
        const KCALL_RESUME = 0x008;
        const DELIVERMSG = 0x040;
        const SIG_DELAY = 0x080;
        const SC_ACTIVE = 0x100;
        const SC_DEFER = 0x200;
        const SC_TRACE = 0x400;
        const EXT_REG_INITIALIZED = 0x1000;
        const SENDING_FROM_KERNEL = 0x2000;
        const CONTEXT_SET = 0x4000;
        const SPROF_SEEN = 0x8000;
        const FLUSH_TLB = 0x10000;
        const SENDA_VM_MISS = 0x20000;
        const STEP = 0x40000;
        const MSGFAILED = 0x80000;
        const NICED = 0x100000;
    }
}

/// Legacy mf constants for backward compatibility during migration.
/// Prefer using `MiscFlagsBits::REPLY_PEND` etc. directly.
pub mod mf {
    pub use super::MiscFlagsBits;
    pub const REPLY_PEND: u32 = super::MiscFlagsBits::REPLY_PEND.bits();
    pub const VIRT_TIMER: u32 = super::MiscFlagsBits::VIRT_TIMER.bits();
    pub const PROF_TIMER: u32 = super::MiscFlagsBits::PROF_TIMER.bits();
    pub const KCALL_RESUME: u32 = super::MiscFlagsBits::KCALL_RESUME.bits();
    pub const DELIVERMSG: u32 = super::MiscFlagsBits::DELIVERMSG.bits();
    pub const SIG_DELAY: u32 = super::MiscFlagsBits::SIG_DELAY.bits();
    pub const SC_ACTIVE: u32 = super::MiscFlagsBits::SC_ACTIVE.bits();
    pub const SC_DEFER: u32 = super::MiscFlagsBits::SC_DEFER.bits();
    pub const SC_TRACE: u32 = super::MiscFlagsBits::SC_TRACE.bits();
    pub const EXT_REG_INITIALIZED: u32 = super::MiscFlagsBits::EXT_REG_INITIALIZED.bits();
    pub const SENDING_FROM_KERNEL: u32 = super::MiscFlagsBits::SENDING_FROM_KERNEL.bits();
    pub const CONTEXT_SET: u32 = super::MiscFlagsBits::CONTEXT_SET.bits();
    pub const SPROF_SEEN: u32 = super::MiscFlagsBits::SPROF_SEEN.bits();
    pub const FLUSH_TLB: u32 = super::MiscFlagsBits::FLUSH_TLB.bits();
    pub const SENDA_VM_MISS: u32 = super::MiscFlagsBits::SENDA_VM_MISS.bits();
    pub const STEP: u32 = super::MiscFlagsBits::STEP.bits();
    pub const MSGFAILED: u32 = super::MiscFlagsBits::MSGFAILED.bits();
    pub const NICED: u32 = super::MiscFlagsBits::NICED.bits();
}

/// Priority range constants. C: minix/kernel/proc.h:135-141.
///
/// Design decision §3.3 (11-design.v1.md): type is `u8` (not `i8`) because
/// the valid range 0..=15 fits in `u8` and `u8` cannot be negative, which
/// matches the semantic that priority is never negative. The C `sched_proc`
/// sentinel `-1` ("keep current") is NOT a priority value — it is expressed
/// via `Option<u8>` in `sched_proc` (§3.8).
pub mod priority {
    pub const TASK_Q: u8 = 0;
    pub const MAX_USER_Q: u8 = 0;
    pub const USER_Q: u8 = 7;
    pub const MIN_USER_Q: u8 = 15;
    pub const NR_SCHED_QUEUES: usize = 16;
}

/// Runtime status flags (wraps atomic operations).
///
/// Uses `RtsFlagsBits` bitflags internally for type-safe flag values.
/// The atomic wrapper allows lock-free reads from interrupt context
/// (BKL may not be held during timer interrupts).
#[derive(Debug)]
pub struct RtsFlags(AtomicU32);

impl RtsFlags {
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    pub fn with(flags: RtsFlagsBits) -> Self {
        Self(AtomicU32::new(flags.bits()))
    }

    /// Const-constructible from raw bit value (for `const fn` table init
    /// where `RtsFlagsBits::bits()` is not `const`).
    pub const fn with_raw_bits(bits: u32) -> Self {
        Self(AtomicU32::new(bits))
    }

    /// Check if a specific flag is set.
    pub fn is_set(&self, flag: RtsFlagsBits) -> bool {
        (self.0.load(Ordering::Acquire) & flag.bits()) != 0
    }

    /// Set specific flags.
    pub fn set(&self, flags: RtsFlagsBits) {
        self.0.fetch_or(flags.bits(), Ordering::AcqRel);
    }

    /// Clear specific flags.
    pub fn unset(&self, flags: RtsFlagsBits) {
        self.0.fetch_and(!flags.bits(), Ordering::AcqRel);
    }

    /// Get all flags as RtsFlagsBits.
    pub fn get(&self) -> RtsFlagsBits {
        RtsFlagsBits::from_bits_truncate(self.0.load(Ordering::Acquire))
    }

    /// Set raw value (for initialization only).
    pub fn set_raw(&self, value: u32) {
        self.0.store(value, Ordering::Release);
    }
}

impl Default for RtsFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Miscellaneous flags (wraps atomic operations).
///
/// Uses `MiscFlagsBits` bitflags internally for type-safe flag values.
#[derive(Debug)]
pub struct MiscFlags(AtomicU32);

impl MiscFlags {
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    /// Check if a specific flag is set.
    pub fn is_set(&self, flag: MiscFlagsBits) -> bool {
        (self.0.load(Ordering::Acquire) & flag.bits()) != 0
    }

    /// Set specific flags.
    pub fn set(&self, flags: MiscFlagsBits) {
        self.0.fetch_or(flags.bits(), Ordering::AcqRel);
    }

    /// Clear specific flags.
    pub fn unset(&self, flags: MiscFlagsBits) {
        self.0.fetch_and(!flags.bits(), Ordering::AcqRel);
    }

    /// Alias for `unset()`. Clears specific flags.
    pub fn clear(&self, flags: MiscFlagsBits) {
        self.unset(flags);
    }

    /// Get all flags as MiscFlagsBits.
    pub fn get(&self) -> MiscFlagsBits {
        MiscFlagsBits::from_bits_truncate(self.0.load(Ordering::Acquire))
    }

    /// Create from raw flags.
    pub fn with(flags: MiscFlagsBits) -> Self {
        Self(AtomicU32::new(flags.bits()))
    }

    /// Load raw value.
    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }
}

impl Default for MiscFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Priority newtype (wraps validity check).
///
/// Valid range: `TASK_Q(0)` to `MIN_USER_Q(15)`.
/// Design decision §3.3 (11-design.v1.md): internal type is `u8`.
/// The special value `-1` in Minix3's `sched_proc()` means "keep current priority"
/// and is NOT a valid `Priority` — it is a parameter sentinel expressed via
/// `Option<u8>` / `Option<Priority>` in `sched_proc` (§3.8), not encoded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u8);

impl Priority {
    /// Construct with validation. Returns `None` if `value > MIN_USER_Q`.
    /// Note: `u8` is always `>= 0`, so no lower bound check is needed.
    pub const fn new(value: u8) -> Option<Self> {
        if value <= priority::MIN_USER_Q {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Get the raw `u8` value.
    pub const fn get(&self) -> u8 {
        self.0
    }

    /// Whether this is a kernel-task priority (`TASK_Q == 0`).
    pub const fn is_kernel(&self) -> bool {
        self.0 == priority::TASK_Q
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self(priority::USER_Q)
    }
}

/// Time slice management.
#[derive(Debug)]
pub struct Quantum {
    pub cpu_time_left: AtomicU64,
    pub size_ms: AtomicU32,
}

impl Quantum {
    pub const fn new(size_ms: u32) -> Self {
        Self {
            cpu_time_left: AtomicU64::new(0),
            size_ms: AtomicU32::new(size_ms),
        }
    }

    pub fn allocate(&self, cycles_per_ms: u64) {
        let total = self.size_ms.load(Ordering::Relaxed) as u64 * cycles_per_ms;
        self.cpu_time_left.store(total, Ordering::Release);
    }

    pub fn consume(&self, cycles: u64) -> bool {
        let mut current = self.cpu_time_left.load(Ordering::Acquire);
        loop {
            if current <= cycles {
                // Quantum exhausted — set to 0 and return true
                match self.cpu_time_left.compare_exchange_weak(
                    current, 0, Ordering::AcqRel, Ordering::Acquire,
                ) {
                    Ok(_) => return true,
                    Err(actual) => current = actual,
                }
            } else {
                // Still has time left
                match self.cpu_time_left.compare_exchange_weak(
                    current, current - cycles, Ordering::AcqRel, Ordering::Acquire,
                ) {
                    Ok(_) => return false,
                    Err(actual) => current = actual,
                }
            }
        }
    }
}

/// Maximum number of CPUs (must match `smp::MAX_CPUS`).
///
/// Kept here to avoid a circular `proc` → `smp` → `proc` import when
/// `CpuMask` needs the array size.
///
/// Defined BEFORE `CpuId` because `CpuId::new()` references it in a
/// `const fn` (R-10).
pub const MAX_CPUS: usize = 32;

/// CPU identifier (R-10, 2026-08-12: newtype for type safety).
///
/// Wraps a `u32` CPU id. Use `CpuId::new(n)` to validate at construction
/// (rejects `n >= MAX_CPUS`), or `CpuId::new_unchecked(n)` when the caller
/// guarantees validity.
///
/// Sentinel: `CpuId::NONE` (`u32::MAX`) represents "no CPU" — used in
/// scheduling queues and IPC routing when no CPU is assigned.
///
/// C: `bsp_cpu_id` / `cpu` — `unsigned int` in Minix3. The Rust newtype
/// adds type safety: prevents accidental mixing with raw `u32` arithmetic
/// results, and centralizes the `MAX_CPUS` bound check.
///
/// redox 对照: redox `LogicalCpuId(u32)` newtype — same pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CpuId(u32);

impl CpuId {
    /// Sentinel value representing "no CPU".
    pub const NONE: CpuId = CpuId(u32::MAX);

    /// BSP CPU id (always 0 in Minix3).
    pub const BSP: CpuId = CpuId(0);

    /// Create a validated `CpuId`. Returns `None` if `id >= MAX_CPUS`.
    pub const fn new(id: u32) -> Option<Self> {
        if (id as usize) < MAX_CPUS { Some(CpuId(id)) } else { None }
    }

    /// Create a `CpuId` without validation. Caller must ensure `id < MAX_CPUS`
    /// or use `NONE` sentinel intentionally.
    pub const fn new_unchecked(id: u32) -> Self { CpuId(id) }

    /// Get the raw `u32` value.
    pub const fn raw(self) -> u32 { self.0 }

    /// Get the CPU id as a `usize` for array indexing.
    pub const fn index(self) -> usize { self.0 as usize }

    /// Is this the BSP?
    pub const fn is_bsp(self) -> bool { self.0 == 0 }

    /// Is this the "no CPU" sentinel?
    pub const fn is_none(self) -> bool { self.0 == u32::MAX }
}

/// CPU affinity bitmap (06-design.v1.md §4.2).
///
/// C: `p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]` — proc.h:37.
///
/// Bit `i` set ⇔ process may run on CPU `i`. Default = all bits set
/// (any CPU). The scheduler must check `allows(cpu)` before enqueuing
/// into a per-CPU ready queue.
///
/// Stored as `[u64; 1]` (32 bits used, 64-bit chunk for alignment).
/// `MAX_CPUS = 32` fits in a single `u64`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuMask {
    bits: u64,
}

impl CpuMask {
    /// Const-constructible all-ones mask (any CPU allowed).
    pub const fn all() -> Self {
        // Lower MAX_CPUS bits set.
        Self { bits: (1u64 << MAX_CPUS) - 1 }
    }

    /// Const-constructible empty mask (no CPU allowed — sentinel).
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Check if CPU `cpu` is allowed.
    pub fn allows(&self, cpu: CpuId) -> bool {
        cpu.index() < MAX_CPUS && (self.bits & (1u64 << cpu.raw())) != 0
    }

    /// Allow CPU `cpu`.
    pub fn set(&mut self, cpu: CpuId) {
        if cpu.index() < MAX_CPUS {
            self.bits |= 1u64 << cpu.raw();
        }
    }

    /// Disallow CPU `cpu`.
    pub fn clear(&mut self, cpu: CpuId) {
        if cpu.index() < MAX_CPUS {
            self.bits &= !(1u64 << cpu.raw());
        }
    }
}

/// Scheduling fields extension.
#[derive(Debug)]
pub struct SchedFields {
    pub priority: AtomicU8,
    pub quantum: Quantum,
    pub cpu: AtomicU32,
    /// CPU affinity bitmap (06-design.v1.md §4.2).
    /// Default = all CPUs allowed. Scheduler checks `allows(cpu)` before
    /// enqueuing into a per-CPU ready queue.
    pub cpu_mask: CpuMask,
    /// User-space scheduler process number.
    /// `None` means kernel default scheduling (C: `p_scheduler == NULL || p_scheduler == self`).
    /// `Some(nr)` means the process at slot `nr` is the user-space scheduler.
    /// Corresponds to C's `struct proc *p_scheduler`.
    pub scheduler: Option<ProcNr>,
}

impl SchedFields {
    pub const fn new() -> Self {
        Self {
            priority: AtomicU8::new(priority::USER_Q),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
            cpu_mask: CpuMask::all(),
            scheduler: None,
        }
    }

    pub fn with_priority(priority: u8) -> Self {
        Self {
            priority: AtomicU8::new(priority),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
            cpu_mask: CpuMask::all(),
            scheduler: None,
        }
    }
}

impl Default for SchedFields {
    fn default() -> Self {
        Self::new()
    }
}

/// Scheduling statistics structure.
#[derive(Debug)]
pub struct Accounting {
    pub enter_queue: AtomicU64,
    pub time_in_queue: AtomicU64,
    pub dequeues: AtomicU32,
    pub ipc_sync: AtomicU32,
    pub ipc_async: AtomicU32,
    pub preempted: AtomicU32,
}

impl Accounting {
    pub const fn new() -> Self {
        Self {
            enter_queue: AtomicU64::new(0),
            time_in_queue: AtomicU64::new(0),
            dequeues: AtomicU32::new(0),
            ipc_sync: AtomicU32::new(0),
            ipc_async: AtomicU32::new(0),
            preempted: AtomicU32::new(0),
        }
    }

    pub fn reset(&self) {
        self.enter_queue.store(0, Ordering::Release);
        self.time_in_queue.store(0, Ordering::Release);
        self.dequeues.store(0, Ordering::Release);
        self.ipc_sync.store(0, Ordering::Release);
        self.ipc_async.store(0, Ordering::Release);
        self.preempted.store(0, Ordering::Release);
    }

    /// Mark this process as the current billable target (IDLE-side).
    ///
    /// Called from `bsp_finish_booting` step 2 (Doc 08 §3) to indicate
    /// that until a real user process runs, accumulated time should be
    /// billed to the IDLE kernel task.
    ///
    /// C: `get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc)` —
    /// main.c:50.
    ///
    /// `record_enqueue(0)` here means "this is the initial enqueue state"
    /// so the next `record_dequeue(now)` will credit 0 ticks to user time
    /// (correct, since we are billing to the kernel). Real per-CPU wiring
    /// lands with SMP/BKL; this stub is enough to make the type system happy.
    pub fn bill_to_idle(&self) {
        self.enter_queue.store(0, Ordering::Release);
    }

    pub fn record_enqueue(&self, tsc: CpuCycles) {
        self.enter_queue.store(tsc, Ordering::Release);
    }

    pub fn record_dequeue(&self, tsc: CpuCycles) {
        let enter = self.enter_queue.load(Ordering::Acquire);
        if enter > 0 && tsc > enter {
            let delta = tsc - enter;
            self.time_in_queue.fetch_add(delta, Ordering::AcqRel);
        }
        self.enter_queue.store(0, Ordering::Release);
        self.dequeues.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_ipc_sync(&self) {
        self.ipc_sync.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_ipc_async(&self) {
        self.ipc_async.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_preempt(&self) {
        self.preempted.fetch_add(1, Ordering::AcqRel);
    }
}

impl Default for Accounting {
    fn default() -> Self {
        Self::new()
    }
}

/// Time statistics structure.
#[derive(Debug)]
pub struct TimeStats {
    pub user_time: AtomicU64,
    pub sys_time: AtomicU64,
    pub virt_left: AtomicU64,
    pub prof_left: AtomicU64,
}

impl TimeStats {
    pub const fn new() -> Self {
        Self {
            user_time: AtomicU64::new(0),
            sys_time: AtomicU64::new(0),
            virt_left: AtomicU64::new(0),
            prof_left: AtomicU64::new(0),
        }
    }

    pub fn add_user_time(&self, ticks: ClockTicks) {
        self.user_time.fetch_add(ticks, Ordering::AcqRel);
    }

    pub fn add_sys_time(&self, ticks: ClockTicks) {
        self.sys_time.fetch_add(ticks, Ordering::AcqRel);
    }

    pub fn tick_virt_timer(&self) -> bool {
        let mut current = self.virt_left.load(Ordering::Acquire);
        loop {
            if current == 0 {
                return false;
            }
            let new_val = current - 1;
            match self.virt_left.compare_exchange_weak(
                current, new_val, Ordering::AcqRel, Ordering::Acquire,
            ) {
                Ok(_) => return new_val == 0,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn tick_prof_timer(&self) -> bool {
        let mut current = self.prof_left.load(Ordering::Acquire);
        loop {
            if current == 0 {
                return false;
            }
            let new_val = current - 1;
            match self.prof_left.compare_exchange_weak(
                current, new_val, Ordering::AcqRel, Ordering::Acquire,
            ) {
                Ok(_) => return new_val == 0,
                Err(actual) => current = actual,
            }
        }
    }
}

impl Default for TimeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU cycles statistics structure.
#[derive(Debug)]
pub struct CyclesStats {
    pub total: AtomicU64,
    pub kcall: AtomicU64,
    pub kipc: AtomicU64,
    pub tick: AtomicU64,
}

impl CyclesStats {
    pub const fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            kcall: AtomicU64::new(0),
            kipc: AtomicU64::new(0),
            tick: AtomicU64::new(0),
        }
    }

    pub fn add_cycles(&self, cycles: CpuCycles) {
        self.total.fetch_add(cycles, Ordering::AcqRel);
    }

    pub fn add_kcall_cycles(&self, cycles: CpuCycles) {
        self.kcall.fetch_add(cycles, Ordering::AcqRel);
    }

    pub fn add_kipc_cycles(&self, cycles: CpuCycles) {
        self.kipc.fetch_add(cycles, Ordering::AcqRel);
    }
}

impl Default for CyclesStats {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU average statistics structure.
/// Corresponds to C's `struct cpuavg` in type.h:80-85.
#[derive(Debug)]
pub struct CpuAvg {
    pub ca_base: AtomicU64,
    pub ca_run: AtomicU32,
    pub ca_last: AtomicU32,
    pub ca_avg: AtomicU32,
}

impl CpuAvg {
    pub const fn new() -> Self {
        Self {
            ca_base: AtomicU64::new(0),
            ca_run: AtomicU32::new(0),
            ca_last: AtomicU32::new(0),
            ca_avg: AtomicU32::new(0),
        }
    }
}

impl Default for CpuAvg {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DeferArgs {
    pub r1: usize,
    pub r2: usize,
    pub r3: usize,
}

impl DeferArgs {
    /// Const-constructible zeroed defer args (for `const fn` table init).
    pub const fn new() -> Self {
        Self { r1: 0, r2: 0, r3: 0 }
    }
}

/// Process segment descriptor.
///
/// C: `struct segframe` — archtypes.h (x86: p_cr3/p_cr3_v, ARM: p_ttbr/p_ttbr_v)
///
/// In C, this is architecture-specific:
/// - x86: `reg_t p_cr3` (page table root physical), `u32_t *p_cr3_v` (virtual)
/// - ARM: `reg_t p_ttbr` (page table root physical), `u32_t *p_ttbr_v` (virtual)
///
/// In Rust, we unify both into a single struct with generic names.
/// `phys_root` corresponds to p_cr3/p_ttbr, `virt_root` corresponds to p_cr3_v/p_ttbr_v.
#[derive(Debug, Clone, Copy)]
pub struct ProcessSegments {
    /// Physical address of the page table root.
    /// C: `p_seg.p_cr3` (x86) / `p_seg.p_ttbr` (ARM)
    pub phys_root: PhysBytes,

    /// Virtual address of the page table root (kernel-mapped).
    /// C: `p_seg.p_cr3_v` (x86) / `p_seg.p_ttbr_v` (ARM)
    /// `None` when the process has no private page table (kernel tasks).
    pub virt_root: Option<VirBytes>,
}

impl ProcessSegments {
    /// Const-constructible zeroed segments (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            phys_root: PhysBytes(0),
            virt_root: None,
        }
    }
}

impl Default for ProcessSegments {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct KProcess {
    /// Process number (slot index).
    pub p_nr: ProcNr,
    /// Endpoint identifier.
    pub p_endpoint: Endpoint,
    /// Process segment descriptor (page table root addresses).
    /// C: `struct segframe p_seg` — proc.h:24
    pub p_seg: ProcessSegments,

    /// Privilege structure index.
    /// C: `struct priv *p_priv` — proc.h:25
    /// In C, this is a pointer to the process's privilege structure.
    /// In Rust, we store the index into PrivTable and look up on demand.
    /// `None` means the process has no assigned privilege (should not happen
    /// for running processes; assigned during boot via `get_priv()`).
    pub priv_id: Option<crate::kpriv::PrivId>,

    /// Magic number for process pointer validation.
    /// C: `int p_magic` — proc.h:127, `#define PMAGIC 0xC0FFEE1` — const.h:164
    /// C: `proc_ptr_ok(p)` checks `(p)->p_magic == PMAGIC` — proc.h:174
    /// In Rust, this is only present in debug builds for sanity checking.
    #[cfg(debug_assertions)]
    pub p_magic: u32,

    /// Runtime status flags.
    pub p_rts_flags: RtsFlags,
    /// Miscellaneous flags.
    pub p_misc_flags: MiscFlags,
    /// Scheduling fields.
    pub p_sched: SchedFields,
    /// Scheduling statistics.
    pub p_accounting: Accounting,
    /// Time statistics.
    pub p_time: TimeStats,
    /// Cycles statistics.
    pub p_cycles: CyclesStats,

    /// CPU average statistics.
    /// Corresponds to C's `struct cpuavg p_cpuavg`.
    pub p_cpuavg: CpuAvg,

    pub p_dequeued: AtomicU64,

    /// Last page-fault address (`CR2` on x86, `FAR` on ARM, `stval` on RISC-V).
    /// C: implicit — read from CR2 in `pagefault()` (exception.c:59).
    /// `None` means no pending page fault. Set by `page_fault::set_pagefault_pending`,
    /// cleared by `page_fault::clear_pagefault_pending`.
    pub p_fault_addr: Option<u64>,

    pub p_defer: DeferArgs,

    // IPC queue pointers
    // SMP: p_nextready is accessed by the scheduler across CPUs.
    // Using AtomicI32 with Ordering::Relaxed under BKL protection.
    // -1 (NONE_PROC_NR) represents None, valid ProcNr values are >= 0.

    /// Next process pointer in ready queue.
    /// Used by scheduler to manage ready process list at same priority.
    /// C: `struct proc *p_nextready` — proc.h
    pub p_nextready: AtomicI32,

    /// Sender wait queue for IPC.
    /// Blocked senders waiting to deliver a message to this process.
    /// C: `struct proc *p_caller_q` + `p_q_link` intrusive linked list — proc.h.
    /// Rust: `SenderQueue` (VecDeque<ProcNr>) — design §2.5 / ARCH-2.
    /// Owned by the process; no `p_q_link` field needed (queue storage is
    /// internal to SenderQueue).
    pub caller_q: SenderQueue,

    // IPC endpoint fields
    /// Source endpoint for receiving message.
    /// The source from which process expects to receive when calling receive().
    pub p_getfrom_e: Endpoint,

    /// Target endpoint for sending message.
    /// The target endpoint when process calls send() or sendrec().
    pub p_sendto_e: Endpoint,

    // Signal fields
    /// Pending kernel signal bitmap.
    /// Records which signals are pending for this process.
    pub p_pending: SigSet,

    // Process name fields
    /// Process name, for debugging and logging.
    /// Maximum length PROC_NAME_LEN (16 bytes including trailing \0).
    pub p_name: ProcName,

    // Message fields
    /// Send message buffer.
    /// Stores message content to send when process is blocked on send().
    pub p_sendmsg: Message,

    /// Message delivery buffer.
    /// Stores message content ready to deliver to this process.
    pub p_delivermsg: Message,

    /// Message delivery virtual address.
    /// Virtual address of user-space message buffer.
    pub p_delivermsg_vir: VirBytes,

    /// Architecture-private CPU context (PSW/PSR/sstatus, segment
    /// selectors, FPU policy, ELR/sepc, SP, etc.).
    ///
    /// Arch-internal: the kernel layer never inspects this value's
    /// fields. It is built by `CurrentCpuContextArch::build_cpu_context`
    /// at boot and applied to the trap frame by
    /// `CurrentCpuContextArch::apply_to_trap_frame` at first dispatch.
    ///
    /// Replaces the previous `initial_pc` / `initial_sp` /
    /// `initial_ps_strings_reg` / `initial_status` quadruple plus
    /// `p_ext_reg_state: ExtRegState`. See `06-design.v1.md` §3.2
    /// for the rationale.
    ///
    /// The fixed-size `[u64; ...]` / `bool` fields are arch-private;
    /// the trait bound `Copy + Default` lets `ProcessTable::new()`
    /// pre-fill all slots via `const fn`.
    pub cpu_context: CurrentCpuContext,

    /// FPU / extended-register state buffer.
    ///
    /// C: `p_seg.fpu_state[FPU_XFP_SIZE]` — kernel/proc.h.
    ///
    /// Stores the per-process FPU state for save/restore during context
    /// switches (SMP migration SAVE_CTX) and signal handling (sigreturn).
    /// Zero-initialized at process creation; saved by `FpuArch::save` when
    /// the process releases FPU ownership; restored by `FpuArch::restore`
    /// when the process reacquires FPU ownership or returns from a signal.
    ///
    /// # Sizes
    ///
    /// - mock (test):     0 bytes (ZST — no actual state saved)
    /// - x86-64 (FXSAVE): 512 bytes
    /// - aarch64 (FPSIMD): 528 bytes
    /// - riscv64 (F/D):   264 bytes
    pub fpu_state: CurrentFpuState,

    // VM request fields (24-cross-space-runtime.md §2.7)
    /// Next process in vmrestart chain.
    /// C: `p_vmrequest.nextrestart` (struct proc *)
    pub p_next_restart: Option<ProcNr>,

    /// Next process in vmrequest queue.
    /// C: `p_vmrequest.nextrequestor` (struct proc *)
    /// Design decision: §3.4 (ProcNr index replaces *proc pointer, moved from
    /// p_vmrequest to KProcess top level alongside p_nextready/caller_q).
    pub p_next_requestor: Option<ProcNr>,

    /// VM suspend context.
    /// `Some` when the process has a pending VM memory request.
    /// `None` when no request is pending.
    /// C: `p_vmrequest` anonymous struct (always embedded, validity controlled by RTS_VMREQUEST).
    /// Design decision: §3.5 (Option replaces always-embedded struct + RTS_VMREQUEST flag).
    ///
    /// Invariant: `p_rts_flags.is_set(RtsFlagsBits::VMREQUEST) <==> p_vm_suspend.is_some()`
    pub p_vm_suspend: Option<VmSuspendContext>,

    // ── Boot-time initial register state (06-proc-init-boot-proc.md §3.2, §3.3) ──
    //
    // ── Boot-time initial register state has been replaced by
    //    `cpu_context: CurrentCpuContext` (06-design.v1.md §3.2).
    //    The four old fields (`initial_pc`, `initial_sp`,
    //    `initial_ps_strings_reg`, `initial_status`) are gone; their
    //    semantics live inside the arch-private `CpuContext` value.
}

/// Maximum process name length (including trailing \0).
pub const PROC_NAME_LEN: usize = 16;

/// Process name type.
/// Fixed-size byte array, corresponds to C's char[PROC_NAME_LEN].
#[derive(Clone, Copy)]
pub struct ProcName {
    data: [u8; PROC_NAME_LEN],
}

impl ProcName {
    /// Creates empty process name.
    pub const fn new() -> Self {
        Self { data: [0; PROC_NAME_LEN] }
    }

    /// Creates process name from string.
    /// If string exceeds max length, it will be truncated.
    // R-18 (2026-08-13): Renamed from `from_str` to avoid confusion with
    // `std::str::FromStr::from_str` (different signature — no Result return).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        let mut name = Self::new();
        let bytes = s.as_bytes();
        let len = bytes.len().min(PROC_NAME_LEN - 1);
        name.data[..len].copy_from_slice(&bytes[..len]);
        name
    }

    /// Gets name string (without trailing \0).
    pub fn as_str(&self) -> &str {
        let len = self.data.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
        core::str::from_utf8(&self.data[..len]).unwrap_or("<invalid>")
    }

    /// Adds suffix to name.
    /// If adding would exceed max length, it will be truncated.
    pub fn push_suffix(&mut self, suffix: &str) {
        let current_len = self.data.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
        let suffix_bytes = suffix.as_bytes();
        let suffix_len = suffix_bytes.len();
        
        let available = PROC_NAME_LEN.saturating_sub(current_len + 1);
        let copy_len = suffix_len.min(available);
        
        if copy_len > 0 {
            self.data[current_len..current_len + copy_len].copy_from_slice(&suffix_bytes[..copy_len]);
        }
    }

    /// Gets raw byte array.
    pub fn as_bytes(&self) -> &[u8; PROC_NAME_LEN] {
        &self.data
    }

    /// Const-constructible name from a fixed byte array (for `const fn`
    /// table init where `from_str` is unavailable).
    pub const fn from_array(data: [u8; PROC_NAME_LEN]) -> Self {
        Self { data }
    }
}

impl Default for ProcName {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for ProcName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl core::fmt::Display for ProcName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Signal set type (bitmap).
/// Corresponds to C's sigset_t, using 64 bits for 64 signals.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SigSet(u64);

impl SigSet {
    /// Creates empty signal set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Construct from a raw u64 bitmap (e.g. for IPC message fields).
    /// Inverse of `get()`.
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Checks if signal is in set.
    pub fn contains(self, sig: u8) -> bool {
        if sig == 0 || sig > 64 {
            return false;
        }
        (self.0 >> (sig - 1)) & 1 == 1
    }

    /// Adds signal to set.
    pub fn add(&mut self, sig: u8) {
        if sig == 0 || sig > 64 {
            return;
        }
        self.0 |= 1 << (sig - 1);
    }

    /// Removes signal from set.
    pub fn remove(&mut self, sig: u8) {
        if sig == 0 || sig > 64 {
            return;
        }
        self.0 &= !(1 << (sig - 1));
    }

    /// Clears signal set.
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Checks if signal set is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the raw u64 bitmap value.
    /// Used for writing signal maps into IPC messages (m_sigcalls.map).
    pub fn get(self) -> u64 {
        self.0
    }
}

impl RtsFlags {
    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn store(&self, value: u32) {
        self.0.store(value, Ordering::Release);
    }

    pub fn is_runnable(&self) -> bool {
        self.load() == 0
    }

    /// Clears specific flags.
    ///
    /// # Two-level flag API: primitive (this method) vs scheduler-aware
    ///
    /// In Minix3, `RTS_SET`/`RTS_UNSET` are macros that wrap the flag
    /// modification with `dequeue()`/`enqueue()` calls when the process
    /// transitions between runnable and non-runnable states. The Rust
    /// rewrite mirrors this at TWO levels:
    ///
    /// ## Level 1 (this method, primitive):
    /// `RtsFlags::clear` is a pure primitive — it does NOT call the
    /// scheduler. It is intended for hot paths where the caller knows
    /// the flag transition does not affect runnability (e.g. clearing
    /// `MF_REPLY_PEND` after reading a reply; clearing `MF_DELIVERMSG`
    /// in `do_exec`). For those cases, the scheduler integration would
    /// be a no-op anyway.
    ///
    /// ## Level 2 (high-level, with scheduler hook):
    /// `ProcessTable::rts_unset` (in `proc_table.rs:130-`) is the
    /// public API for "clear a flag and update the scheduler if the
    /// process became runnable". It calls this primitive internally
    /// and then calls `sched_enqueue` on the runnable transition.
    /// This mirrors C's `RTS_UNSET` macro exactly:
    ///
    /// ```c
    /// #define RTS_UNSET(rp, flags) \
    ///     do {                                \
    ///         if (is_runnable(rp)) clear_ipc_ref(rp, (flags)); \
    ///         else {  RTS_UNSET(rp, flags); enqueue(rp); }     \
    ///     } while (0)
    /// ```
    ///
    /// (proc.h:142-152, paraphrased — see proc.h for exact source.)
    ///
    /// # When to use which
    ///
    /// | Use case                                  | API                  |
    /// |-------------------------------------------|----------------------|
    /// | IPC reply already in `p_delivermsg`        | `RtsFlags::clear`    |
    /// | Misc flag (MF_*) transitions              | `RtsFlags::clear`    |
    /// | SENDING/RECEIVING transitions in IPC      | `ProcessTable::rts_*` |
    /// | PROC_STOP / SLOT_FREE / SIGNALED          | `ProcessTable::rts_*` |
    ///
    /// This split mirrors C's macro-level decomposition: a low-level
    /// flag primitive plus a higher-level wrapper that adds scheduling.
    pub fn clear(&self, flags: RtsFlagsBits) {
        self.0.fetch_and(!flags.bits(), Ordering::AcqRel);
    }
}

impl KProcess {
    pub fn new(nr: ProcNr, endpoint: Endpoint) -> Self {
        Self {
            p_nr: nr,
            p_endpoint: endpoint,
            p_seg: ProcessSegments::default(),
            priv_id: None, // Assigned later via PrivTable::assign_static()
            #[cfg(debug_assertions)]
            p_magic: 0xC0FFEE1, // C: rp->p_magic = PMAGIC — proc.c:131
            p_rts_flags: RtsFlags::with(RtsFlagsBits::SLOT_FREE),
            p_misc_flags: MiscFlags::new(),
            p_fault_addr: None, // No pending page fault at spawn.
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_cpuavg: CpuAvg::new(),
            p_dequeued: AtomicU64::new(0),
            p_defer: DeferArgs::default(),
            p_nextready: AtomicI32::new(NONE_PROC_NR),
            caller_q: SenderQueue::new(),
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: Message::default(),
            p_delivermsg: Message::default(),
            p_delivermsg_vir: VirBytes::new(0),
            p_next_restart: None,
            p_next_requestor: None,
            p_vm_suspend: None,
            cpu_context: CurrentCpuContext::default(),
            fpu_state: CurrentFpuState::default(),
        }
    }

    /// Const-constructible zeroed slot marked `SLOT_FREE` (for `const fn`
    /// `ProcessTable::new()` / `static mut` init).
    ///
    /// `p_nr` and `p_endpoint` are sentinel values; `ProcessTable::new()`
    /// overwrites them per-slot. `p_rts_flags` is set to `SLOT_FREE` (0x01)
    /// using the raw bit value because `RtsFlagsBits::SLOT_FREE.bits()` is
    /// not `const` (bitflags limitation).
    ///
    /// See `06-design.v1.md` §4.1 (zero-heap `static mut` storage).
    pub const fn new_zeroed() -> Self {
        Self {
            p_nr: ProcNr(0),
            p_endpoint: Endpoint::NONE,
            p_seg: ProcessSegments::new(),
            priv_id: None,
            #[cfg(debug_assertions)]
            p_magic: 0xC0FFEE1,
            p_rts_flags: RtsFlags(AtomicU32::new(0x01)), // SLOT_FREE raw bits
            p_misc_flags: MiscFlags::new(),
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_cpuavg: CpuAvg::new(),
            p_dequeued: AtomicU64::new(0),
            p_fault_addr: None,
            p_defer: DeferArgs::new(),
            p_nextready: AtomicI32::new(NONE_PROC_NR),
            caller_q: SenderQueue::new(),
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: Message::zeroed(),
            p_delivermsg: Message::zeroed(),
            p_delivermsg_vir: VirBytes::new(0),
            p_next_restart: None,
            p_next_requestor: None,
            p_vm_suspend: None,
            cpu_context: CurrentCpuContext::new(),
            fpu_state: CurrentFpuState::new(),
        }
    }

    /// Check if this is a kernel task (p_nr < 0).
    /// C: iskernelp(p) = ((p) < BEG_USER_ADDR) — but Rust uses p_nr field
    /// instead of address comparison (06-proc-init-boot-proc.md §3.3).
    pub fn is_kernel_task(&self) -> bool {
        self.p_nr.0 < 0
    }

    pub fn is_runnable(&self) -> bool {
        self.p_rts_flags.is_runnable()
    }

    pub fn get_priority(&self) -> Priority {
        Priority::new(self.p_sched.priority.load(Ordering::Acquire))
            .unwrap_or_default()
    }

    /// Sets priority with validation. Returns false if value is out of range.
    pub fn set_priority(&self, prio: u8) -> bool {
        if let Some(p) = Priority::new(prio) {
            self.p_sched.priority.store(p.get(), Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Sets priority without validation. For internal use by sched_proc
    /// where the caller has already validated the range.
    pub fn set_priority_unchecked(&self, prio: u8) {
        self.p_sched.priority.store(prio, Ordering::Release);
    }

    pub fn reset_accounting(&self) {
        self.p_accounting.reset();
    }

    pub fn blocked_on(&self) -> Option<Endpoint> {
        if self.p_rts_flags.is_set(RtsFlagsBits::SENDING) {
            Some(self.p_sendto_e)
        } else if self.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) {
            Some(self.p_getfrom_e)
        } else {
            None
        }
    }

    // ── Boot-time initial CPU context setter (06-design.v1.md §3.2) ──

    /// Set process name. Corresponds to C's `strlcpy(rp->p_name, name, ...)`.
    /// C: main.c:170, protect.c:441
    pub fn set_boot_name(&mut self, name: &str) {
        self.p_name = ProcName::from_str(name);
    }

    /// Set the architecture-private CPU context.
    ///
    /// The kernel layer never inspects `cpu_context`'s fields; it just
    /// stores the value and hands it to
    /// `CurrentCpuContextArch::apply_to_trap_frame` at first dispatch.
    /// Caller builds the value via
    /// `CurrentCpuContextArch::build_cpu_context(kind, proc_nr, entry)`.
    ///
    /// Replaces the previous `set_boot_initial_reg_state` +
    /// `set_boot_pc_sp` pair (06-design.v1.md §3.2).
    pub fn set_boot_cpu_context(&mut self, cpu_context: CurrentCpuContext) {
        self.cpu_context = cpu_context;
    }

    /// Enable user I/O access (x86-64: set RFLAGS.IOPL = 3).
    ///
    /// This sinks the x86-64 hardware concept (RFLAGS.IOPL) entirely
    /// into the arch layer — the kernel layer only knows about the
    /// OS concept "enable user I/O". Other architectures are no-ops.
    /// Replaces the previous `target.initial_status |= X86_64_IOPL_BITS`
    /// direct write in `syscall_device.rs` (06-design.v1.md §3.5).
    pub fn enable_user_io(&mut self) {
        <CurrentCpuContextArch as CpuContextArch>::enable_user_io(&mut self.cpu_context);
    }

    // ── VM suspend/resume methods (24-cross-space-runtime.md §2.8, §4.3) ──

    /// Suspend this process for a VM memory request.
    ///
    /// Corresponds to Minix3's `vm_suspend()` in proc.c:234-257.
    /// Sets RTS_VMREQUEST (process not schedulable) and fills VmSuspendContext.
    ///
    /// Design decision: §3.5 (Option<VmSuspendContext> replaces always-embedded struct),
    /// §3.9 (vm_suspend split: this method only sets process state, queue+notify separated).
    ///
    /// # Panics
    /// Debug asserts that RTS_VMREQUEST is not already set and p_vm_suspend is None.
    pub fn suspend_for_vm(
        &mut self,
        suspend_type: VmSuspendType,
        target: Endpoint,
        params: VmCheckParams,
        saved_msg: Option<Message>,
    ) {
        debug_assert!(!self.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
        debug_assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
            copy_context: None,
        });

        self.p_rts_flags.set(RtsFlagsBits::VMREQUEST);
    }

    /// Suspend this process for a VM memory request with cross-space copy context.
    ///
    /// Same as `suspend_for_vm` but also saves the cross-space copy context
    /// for resumption. Used by `virtual_copy_f` / `cross_space_copy` scenarios.
    ///
    /// Design decision: §3.8 (merges 02 doc VmRequest as `copy_context` field).
    pub fn suspend_for_vm_with_copy(
        &mut self,
        suspend_type: VmSuspendType,
        target: Endpoint,
        params: VmCheckParams,
        saved_msg: Option<Message>,
        copy_ctx: VmCopyContext,
    ) {
        debug_assert!(!self.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
        debug_assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
            copy_context: Some(copy_ctx),
        });

        self.p_rts_flags.set(RtsFlagsBits::VMREQUEST);
    }

    /// Clear VM suspend state for this process.
    ///
    /// Corresponds to Minix3's `clear_memreq()` in system.c:488-503.
    /// Called when a process exits and its pending request must be cleaned up.
    ///
    /// Note: Unlike the Ch4 design doc, this does NOT clear MF_KCALL_RESUME.
    /// C's `clear_memreq()` only clears RTS_VMREQUEST and removes from the
    /// vmrequest linked list — it does not touch MF_KCALL_RESUME.
    /// MF_KCALL_RESUME is cleared separately in `kernel_call_resume()`
    /// (system.c:635) after the kernel call is re-executed.
    pub fn clear_vm_suspend(&mut self) {
        self.p_vm_suspend = None;
        self.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);
    }

    /// Check if this process is suspended waiting for a VM memory request.
    ///
    /// C: `RTS_ISSET(p, RTS_VMREQUEST)`
    pub fn is_vm_suspended(&self) -> bool {
        self.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST)
    }

    /// Get a reference to the VM suspend context.
    ///
    /// Returns `None` if the process is not VM-suspended.
    /// Invariant: `is_vm_suspended() <==> vm_suspend_context().is_some()`
    pub fn vm_suspend_context(&self) -> Option<&VmSuspendContext> {
        self.p_vm_suspend.as_ref()
    }

    /// Get a mutable reference to the VM suspend context.
    ///
    /// Returns `None` if the process is not VM-suspended.
    pub fn vm_suspend_context_mut(&mut self) -> Option<&mut VmSuspendContext> {
        self.p_vm_suspend.as_mut()
    }

    /// Creates child process from parent (fork).
    ///
    /// Corresponds to `*rpc = *rpp` copy in Minix3's do_fork.c with subsequent field corrections.
    /// Does not copy p_nr and p_endpoint, specified by caller via parameters.
    /// Time stats, accounting info, signal set start from zero, IPC queue pointers cleared.
    ///
    /// # Fork field corrections (matching Minix3 do_fork.c)
    ///
    /// | Field | C behavior | Rust behavior |
    /// |-------|-----------|---------------|
    /// | `p_rts_flags` | Copy then `RTS_SET(NO_QUANTUM)`, `RTS_UNSET(SIGNALED\|SIG_PENDING\|P_STOP)` | Copy then apply same corrections |
    /// | `p_misc_flags` | Copy then clear `VIRT_TIMER\|PROF_TIMER\|SC_TRACE\|SPROF_SEEN\|STEP` | Copy then apply same mask |
    /// | `p_time` | `virt_left=0, prof_left=0` | All zeroed (new) |
    /// | `p_pending` | `sigemptyset()` | Empty |
    /// | `p_nextready/caller_q` | Pointer copied but child gets own queues | `None` (child not queued yet) |
    ///
    /// # Parameters
    /// - `parent`: Reference to parent process
    /// - `child_nr`: Child process number (slot number)
    /// - `child_endpoint`: Child's new endpoint
    pub fn fork_from(parent: &KProcess, child_nr: ProcNr, child_endpoint: Endpoint) -> Self {
        // Copy p_rts_flags then apply fork corrections:
        // RTS_SET(rpc, RTS_NO_QUANTUM) — child not runnable until scheduled
        // RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP) — no signal inheritance
        let child_rts = {
            let flags = parent.p_rts_flags.get();
            let flags = flags | RtsFlagsBits::NO_QUANTUM;
            let flags = flags & !(RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING | RtsFlagsBits::P_STOP | RtsFlagsBits::VMREQUEST);
            RtsFlags::with(flags)
        };

        // Copy p_misc_flags then clear timer/trace flags:
        // rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_SPROF_SEEN | MF_STEP)
        let child_mf = {
            let flags = parent.p_misc_flags.get();
            let flags = flags
                & !(MiscFlagsBits::VIRT_TIMER | MiscFlagsBits::PROF_TIMER | MiscFlagsBits::SC_TRACE | MiscFlagsBits::SPROF_SEEN | MiscFlagsBits::STEP);
            MiscFlags::with(flags)
        };

        let mut child = Self {
            p_nr: child_nr,
            p_endpoint: child_endpoint,
            p_seg: ProcessSegments::default(), // C: rpc->p_seg.p_cr3=0, p_cr3_v=NULL
            priv_id: None, // Will be assigned USER_PRIV_ID for child (C: rpc->p_priv = priv_addr(USER_PRIV_ID))
            #[cfg(debug_assertions)]
            p_magic: 0xC0FFEE1, // C: rp->p_magic = PMAGIC — proc.c:131
            p_rts_flags: child_rts,
            // Child starts with no pending page fault (C: no fork-time copy).
            p_fault_addr: None,
            p_misc_flags: child_mf,
            p_sched: SchedFields {
                priority: AtomicU8::new(parent.p_sched.priority.load(Ordering::Acquire)),
                quantum: Quantum::new(parent.p_sched.quantum.size_ms.load(Ordering::Acquire)),
                cpu: AtomicU32::new(parent.p_sched.cpu.load(Ordering::Acquire)),
                // Child inherits parent's CPU affinity (C: p_cpu_mask memcpy).
                cpu_mask: parent.p_sched.cpu_mask,
                scheduler: None,
            },
            p_accounting: Accounting::new(),
            // p_time zeroed: corresponds to rpc->p_user_time=0, p_sys_time=0, p_virt_left=0, p_prof_left=0
            p_time: TimeStats::new(),
            // p_cycles zeroed: corresponds to rpc->p_cycles=0, p_kcall_cycles=0, p_kipc_cycles=0
            p_cycles: CyclesStats::new(),
            // p_cpuavg zeroed: child starts with fresh CPU average
            p_cpuavg: CpuAvg::new(),
            p_dequeued: AtomicU64::new(0),
            p_defer: DeferArgs::default(),
            // IPC queue pointers: child is not queued, no callers, no links
            p_nextready: AtomicI32::new(NONE_PROC_NR),
            caller_q: SenderQueue::new(),
            p_getfrom_e: parent.p_getfrom_e,
            p_sendto_e: parent.p_sendto_e,
            // p_pending cleared: corresponds to sigemptyset(&rpc->p_pending)
            p_pending: SigSet::empty(),
            p_name: parent.p_name,
            p_sendmsg: parent.p_sendmsg,
            p_delivermsg: parent.p_delivermsg,
            p_delivermsg_vir: parent.p_delivermsg_vir,
            p_next_restart: None,
            // VM request fields: child has no pending VM request
            // C: fork copies p_vmrequest but child never has RTS_VMREQUEST set
            p_next_requestor: None,
            p_vm_suspend: None,
            cpu_context: CurrentCpuContext::default(),
            fpu_state: CurrentFpuState::default(),
        };

        // Inherit extended-register / FPU state from parent if parent
        // has touched the FPU. Each arch overrides `inherit_fpu_state`:
        //   x86-64    → copies `fpu_policy` (LazyUserInit / KernelTask)
        //   aarch64   → copies `fpu_enable_el0` (CPACR_EL1.FPEN policy)
        //   riscv64   → copies `sstatus` (preserves FS field)
        // The kernel layer is unaware of which field is copied — it
        // only observes `EXT_REG_INITIALIZED` propagation.
        //
        // C: do_fork.c memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state,
        //                     FPU_XFP_SIZE) under proc_used_fpu(rpp)
        //
        // See 06-design.v1.md §D7.
        if parent.p_misc_flags.is_set(MiscFlagsBits::EXT_REG_INITIALIZED) {
            <CurrentCpuContextArch as CpuContextArch>::inherit_fpu_state(
                &mut child.cpu_context,
                &parent.cpu_context,
            );
            child.p_misc_flags.set(MiscFlagsBits::EXT_REG_INITIALIZED);
        }

        child
    }
}

/// Fork flags (corresponds to Minix3's PFF_* flags).
pub mod fork_flags {
    /// VM inhibit flag - child starts with VM inhibit set.
    pub const VMINHIBIT: u32 = 0x01;
}

/// Completes fork setup with privilege handling and flags.
///
/// Corresponds to Minix3's do_fork.c privilege handling:
/// - System processes get downgraded to user privilege
/// - VMINHIBIT flag is set if requested
/// - Process name gets "*F" suffix
///
/// # Parameters
/// - `child`: Mutable reference to child process
/// - `parent_is_sys_proc`: Whether parent is a system process
/// - `flags`: Fork flags (PFF_*)
pub fn complete_fork_setup(child: &mut KProcess, parent_is_sys_proc: bool, flags: u32) {
    // If parent is a system process, child gets user privilege
    // Corresponds to Minix3's:
    //   if (priv(rpp)->s_flags & SYS_PROC) {
    //       rpc->p_priv = priv_addr(USER_PRIV_ID);
    //       rpc->p_rts_flags |= RTS_NO_PRIV;
    //   }
    if parent_is_sys_proc {
        child.p_rts_flags.set(RtsFlagsBits::NO_PRIV);
    }

    // Set VMINHIBIT if requested
    // Corresponds to Minix3's:
    //   if(m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT) {
    //       RTS_SET(rpc, RTS_VMINHIBIT);
    //   }
    if flags & fork_flags::VMINHIBIT != 0 {
        child.p_rts_flags.set(RtsFlagsBits::VMINHIBIT);
    }

    // Add "*F" suffix to process name
    // Corresponds to Minix3's:
    //   namelen = strlen(rpc->p_name);
    //   if(namelen+strlen(FORKSTR) < sizeof(rpc->p_name))
    //       strcat(rpc->p_name, "*F");
    child.p_name.push_suffix("*F");
}

// ── IPC status register helpers ──
//
// C: `IPC_STATUS_ADD_CALL` / `IPC_STATUS_ADD_FLAGS` macros — ipc.h:40-48.
// These OR-merge status metadata into the receiver's IPC status register
// when a message is delivered. User-space libraries read this register to
// determine the delivery type (SEND, NOTIFY, SENDA, etc.) and whether the
// message originated from the kernel.

/// Add a call type to the process's IPC status register.
///
/// C: `IPC_STATUS_ADD_CALL(p, call)` — ipc.h:45-46
///
/// Encodes `call` into the low 6 bits of the IPC status register and
/// OR-merges it. Skipped when `MF_REPLY_PEND` is set (SENDREC's receive
/// phase must not overwrite the status set by the send phase).
///
/// # Arguments
///
/// * `proc` — the **receiver** process (the one getting the message delivered)
/// * `call` — the IPC primitive that delivered the message
pub fn ipc_status_add_call(proc: &mut KProcess, call: crate::ipc::IpcCall) {
    // C: IPC_STATUS_ADD(p, m) — skip if MF_REPLY_PEND is set.
    if !proc.p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
        let value = crate::ipc::ipc_status_call_to(call);
        CurrentCpuContextArch::or_ipc_status_reg(&mut proc.cpu_context, value);
    }
}

/// Add flags to the process's IPC status register.
///
/// C: `IPC_STATUS_ADD_FLAGS(p, flags)` — ipc.h:47-48
///
/// Encodes `flags` into bits 16+ of the IPC status register and OR-merges
/// them. Skipped when `MF_REPLY_PEND` is set (same rationale as
/// `ipc_status_add_call`).
///
/// # Arguments
///
/// * `proc` — the **receiver** process
/// * `flags` — bitwise-OR of `IPC_FLG_*` constants
pub fn ipc_status_add_flags(proc: &mut KProcess, flags: u32) {
    if !proc.p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
        let value = crate::ipc::ipc_status_flags(flags);
        CurrentCpuContextArch::or_ipc_status_reg(&mut proc.cpu_context, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_mask_default_all_allows_any_cpu() {
        let mask = CpuMask::all();
        assert!(mask.allows(CpuId::BSP));
        assert!(mask.allows(CpuId::new_unchecked(1)));
        assert!(mask.allows(CpuId::new_unchecked((MAX_CPUS as u32) - 1)));
        // Out-of-range CPU is never allowed.
        assert!(!mask.allows(CpuId::new_unchecked(MAX_CPUS as u32)));
    }

    #[test]
    fn test_cpu_mask_clear_and_set() {
        let mut mask = CpuMask::all();
        mask.clear(CpuId::new_unchecked(2));
        assert!(!mask.allows(CpuId::new_unchecked(2)));
        assert!(mask.allows(CpuId::BSP));
        mask.set(CpuId::new_unchecked(2));
        assert!(mask.allows(CpuId::new_unchecked(2)));
    }

    #[test]
    fn test_cpu_mask_empty_allows_nothing() {
        let mask = CpuMask::empty();
        assert!(!mask.allows(CpuId::BSP));
        assert!(!mask.allows(CpuId::new_unchecked(1)));
    }

    #[test]
    fn test_sched_fields_new_has_all_cpu_mask() {
        let s = SchedFields::new();
        assert!(s.cpu_mask.allows(CpuId::BSP), "default SchedFields must allow CPU 0");
        assert_eq!(s.cpu.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_rts_flags_runnable() {
        let flags = RtsFlags::new();
        assert!(flags.is_runnable());

        flags.set(RtsFlagsBits::PROC_STOP);
        assert!(!flags.is_runnable());

        flags.clear(RtsFlagsBits::PROC_STOP);
        assert!(flags.is_runnable());
    }

    #[test]
    fn test_rts_flags_multiple() {
        let flags = RtsFlags::new();
        flags.set(RtsFlagsBits::SENDING | RtsFlagsBits::RECEIVING);

        assert!(flags.is_set(RtsFlagsBits::SENDING));
        assert!(flags.is_set(RtsFlagsBits::RECEIVING));
        assert!(!flags.is_runnable());

        flags.clear(RtsFlagsBits::SENDING);
        assert!(!flags.is_set(RtsFlagsBits::SENDING));
        assert!(flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_kprocess_new() {
        let proc = KProcess::new(ProcNr(1), Endpoint(1));

        assert_eq!(proc.p_nr, ProcNr(1));
        assert!(!proc.is_runnable());
    }

    #[test]
    fn test_kprocess_runnable() {
        let proc = KProcess::new(ProcNr(1), Endpoint(1));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        assert!(proc.is_runnable());
    }

    #[test]
    fn test_priority_valid() {
        // Design decision §3.3: Priority is u8, so negative values are
        // rejected at the type level (cannot be constructed). The C
        // `sched_proc` sentinel `-1` is expressed via Option<u8> (§3.8).
        assert!(Priority::new(0).is_some());
        assert!(Priority::new(7).is_some());
        assert!(Priority::new(15).is_some());
        assert!(Priority::new(16).is_none());
        assert!(Priority::new(255).is_none());
    }

    #[test]
    fn test_priority_default() {
        let p = Priority::default();
        assert_eq!(p.get(), priority::USER_Q);
    }

    #[test]
    fn test_priority_is_kernel() {
        let kernel = Priority::new(priority::TASK_Q).unwrap();
        assert!(kernel.is_kernel());

        let user = Priority::new(priority::USER_Q).unwrap();
        assert!(!user.is_kernel());
    }

    #[test]
    fn test_quantum_allocate() {
        let q = Quantum::new(200);
        assert_eq!(q.size_ms.load(Ordering::Relaxed), 200);

        q.allocate(1_000_000);
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 200_000_000);
    }

    #[test]
    fn test_quantum_consume() {
        let q = Quantum::new(200);
        q.allocate(1);

        assert!(!q.consume(50));
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 150);

        assert!(q.consume(200));
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_sched_fields_new() {
        let sf = SchedFields::new();
        assert_eq!(sf.priority.load(Ordering::Relaxed), priority::USER_Q);
        assert_eq!(sf.cpu.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_kprocess_priority() {
        let proc = KProcess::new(ProcNr(1), Endpoint(1));
        assert_eq!(proc.get_priority(), Priority(priority::USER_Q));

        proc.set_priority(priority::TASK_Q);
        assert_eq!(proc.get_priority(), Priority(priority::TASK_Q));
    }

    #[test]
    fn test_accounting_new() {
        let acc = Accounting::new();
        assert_eq!(acc.enter_queue.load(Ordering::Relaxed), 0);
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_reset() {
        let acc = Accounting::new();
        acc.dequeues.store(10, Ordering::Release);
        acc.ipc_sync.store(5, Ordering::Release);
        acc.reset();
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 0);
        assert_eq!(acc.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_enqueue_dequeue() {
        let acc = Accounting::new();
        acc.record_enqueue(1000);
        acc.record_dequeue(1500);

        assert_eq!(acc.time_in_queue.load(Ordering::Relaxed), 500);
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 1);
        assert_eq!(acc.enter_queue.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_ipc() {
        let acc = Accounting::new();
        acc.record_ipc_sync();
        acc.record_ipc_sync();
        acc.record_ipc_async();

        assert_eq!(acc.ipc_sync.load(Ordering::Relaxed), 2);
        assert_eq!(acc.ipc_async.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_time_stats() {
        let ts = TimeStats::new();
        ts.add_user_time(10);
        ts.add_sys_time(5);

        assert_eq!(ts.user_time.load(Ordering::Relaxed), 10);
        assert_eq!(ts.sys_time.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_virt_timer() {
        let ts = TimeStats::new();
        ts.virt_left.store(3, Ordering::Release);

        assert!(!ts.tick_virt_timer());
        assert!(!ts.tick_virt_timer());
        assert!(ts.tick_virt_timer());
        assert!(!ts.tick_virt_timer());
    }

    #[test]
    fn test_cycles_stats() {
        let cs = CyclesStats::new();
        cs.add_cycles(1000);
        cs.add_kcall_cycles(200);
        cs.add_kipc_cycles(50);

        assert_eq!(cs.total.load(Ordering::Relaxed), 1000);
        assert_eq!(cs.kcall.load(Ordering::Relaxed), 200);
        assert_eq!(cs.kipc.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn test_kprocess_accounting() {
        let proc = KProcess::new(ProcNr(1), Endpoint(1));
        proc.p_accounting.record_ipc_sync();
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 1);

        proc.reset_accounting();
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_basic() {
        let parent = KProcess::new(ProcNr(5), Endpoint::from_generation_slot(3, 5));
        parent.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        parent.set_priority(priority::USER_Q);

        let child_endpoint = Endpoint::fork_new_endpoint(
            Endpoint::from_generation_slot(0, 10), 10
        );
        let child = KProcess::fork_from(&parent, ProcNr(10), child_endpoint);

        assert_eq!(child.p_nr, ProcNr(10));
        assert_eq!(child.p_endpoint, child_endpoint);
        assert_eq!(child.get_priority(), parent.get_priority());
        assert_eq!(child.p_time.user_time.load(Ordering::Relaxed), 0);
        assert_eq!(child.p_time.sys_time.load(Ordering::Relaxed), 0);
        assert!(child.p_pending.is_empty());
    }

    #[test]
    fn test_fork_from_accounting_reset() {
        let parent = KProcess::new(ProcNr(5), Endpoint(5));
        parent.p_accounting.record_ipc_sync();
        parent.p_accounting.record_ipc_sync();

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_independent_queues() {
        let mut parent = KProcess::new(ProcNr(5), Endpoint(5));
        parent.p_nextready.store(3, Ordering::Relaxed);
        parent.caller_q.push_back(ProcNr(7));

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_nextready.load(Ordering::Relaxed), NONE_PROC_NR);
        assert!(child.caller_q.is_empty());
    }

    #[test]
    fn test_fork_from_inherits_ipc_endpoints() {
        let mut parent = KProcess::new(ProcNr(5), Endpoint(5));
        parent.p_getfrom_e = Endpoint::PM;
        parent.p_sendto_e = Endpoint::VFS;

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_getfrom_e, Endpoint::PM);
        assert_eq!(child.p_sendto_e, Endpoint::VFS);
    }

    #[test]
    fn test_fork_from_cycles_reset() {
        let parent = KProcess::new(ProcNr(5), Endpoint(5));
        parent.p_cycles.add_cycles(1000);
        parent.p_cycles.add_kcall_cycles(200);

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_cycles.total.load(Ordering::Relaxed), 0);
        assert_eq!(child.p_cycles.kcall.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_rts_flags_corrections() {
        // Parent has SIGNALED and SIG_PENDING set — child must NOT inherit these
        let parent = KProcess::new(ProcNr(5), Endpoint::from_generation_slot(1, 5));
        parent.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        parent.p_rts_flags.set(RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING | RtsFlagsBits::P_STOP);

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        // Child must have NO_QUANTUM set (C: RTS_SET(rpc, RTS_NO_QUANTUM))
        assert!(child.p_rts_flags.is_set(RtsFlagsBits::NO_QUANTUM));
        // Child must NOT have SIGNALED, SIG_PENDING, P_STOP (C: RTS_UNSET)
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING));
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::P_STOP));
    }

    #[test]
    fn test_fork_from_misc_flags_corrections() {
        // Parent has VIRT_TIMER, PROF_TIMER, STEP set — child must NOT inherit
        let parent = KProcess::new(ProcNr(5), Endpoint::from_generation_slot(1, 5));
        parent.p_misc_flags.set(MiscFlagsBits::VIRT_TIMER | MiscFlagsBits::PROF_TIMER | MiscFlagsBits::STEP | MiscFlagsBits::SC_TRACE | MiscFlagsBits::SPROF_SEEN);
        // Also set a flag that SHOULD be inherited
        parent.p_misc_flags.set(MiscFlagsBits::REPLY_PEND);

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        // Cleared flags
        assert!(!child.p_misc_flags.is_set(MiscFlagsBits::VIRT_TIMER));
        assert!(!child.p_misc_flags.is_set(MiscFlagsBits::PROF_TIMER));
        assert!(!child.p_misc_flags.is_set(MiscFlagsBits::STEP));
        assert!(!child.p_misc_flags.is_set(MiscFlagsBits::SC_TRACE));
        assert!(!child.p_misc_flags.is_set(MiscFlagsBits::SPROF_SEEN));
        // Inherited flag
        assert!(child.p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND));
    }

    // ── §5.1: KProcess VM suspend/resume tests ──

    #[test]
    fn test_suspend_for_vm_sets_rts_and_context() {
        let mut proc = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let params = crate::vm::VmCheckParams {
            start: minix_types::VirBytes::new(0x1000),
            length: minix_types::VirBytes::new(0x100),
            write_flag: true,
        };
        proc.suspend_for_vm(
            crate::vm::VmSuspendType::KernelCall,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
        );

        assert!(proc.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
        assert!(proc.p_vm_suspend.is_some());
        assert!(proc.is_vm_suspended());

        let ctx = proc.vm_suspend_context().unwrap();
        assert_eq!(ctx.suspend_type, crate::vm::VmSuspendType::KernelCall);
        assert_eq!(ctx.state, crate::vm::VmSuspendState::Pending);
        assert!(ctx.saved_msg.is_none());
        assert!(ctx.copy_context.is_none());
    }

    #[test]
    fn test_suspend_for_vm_with_copy() {
        let mut proc = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let params = crate::vm::VmCheckParams {
            start: minix_types::VirBytes::new(0x2000),
            length: minix_types::VirBytes::new(0x200),
            write_flag: false,
        };
        let copy_ctx = crate::vm::VmCopyContext::new(
            crate::vm::AddressRef::Process {
                endpoint: Endpoint::from_generation_slot(1, 2),
                offset: minix_types::VirBytes::new(0x1000),
            },
            crate::vm::AddressRef::Process {
                endpoint: Endpoint::from_generation_slot(1, 3),
                offset: minix_types::VirBytes::new(0x2000),
            },
            0x100,
            crate::vm::VmFaultType::Src,
        );
        proc.suspend_for_vm_with_copy(
            crate::vm::VmSuspendType::KernelCall,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
            copy_ctx,
        );

        let ctx = proc.vm_suspend_context().unwrap();
        assert!(ctx.copy_context.is_some());
    }

    #[test]
    fn test_clear_vm_suspend() {
        let mut proc = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let params = crate::vm::VmCheckParams {
            start: minix_types::VirBytes::new(0x1000),
            length: minix_types::VirBytes::new(0x100),
            write_flag: true,
        };
        proc.suspend_for_vm(
            crate::vm::VmSuspendType::KernelCall,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
        );

        assert!(proc.is_vm_suspended());

        proc.clear_vm_suspend();

        assert!(!proc.is_vm_suspended());
        assert!(!proc.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
        assert!(proc.p_vm_suspend.is_none());
        assert!(proc.vm_suspend_context().is_none());
    }

    #[test]
    fn test_clear_vm_suspend_does_not_clear_kcall_resume() {
        let mut proc = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let params = crate::vm::VmCheckParams {
            start: minix_types::VirBytes::new(0x1000),
            length: minix_types::VirBytes::new(0x100),
            write_flag: true,
        };
        proc.suspend_for_vm(
            crate::vm::VmSuspendType::KernelCall,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
        );
        proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);

        proc.clear_vm_suspend();

        // MF_KCALL_RESUME should NOT be cleared by clear_vm_suspend
        // (C's clear_memreq only clears RTS_VMREQUEST, not MF_KCALL_RESUME)
        assert!(proc.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    #[test]
    fn test_vm_suspend_context_mut() {
        let mut proc = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let params = crate::vm::VmCheckParams {
            start: minix_types::VirBytes::new(0x1000),
            length: minix_types::VirBytes::new(0x100),
            write_flag: true,
        };
        proc.suspend_for_vm(
            crate::vm::VmSuspendType::KernelCall,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
        );

        let ctx = proc.vm_suspend_context_mut().unwrap();
        ctx.state = crate::vm::VmSuspendState::Fetched;

        assert_eq!(proc.vm_suspend_context().unwrap().state, crate::vm::VmSuspendState::Fetched);
    }

    #[test]
    fn test_fork_child_has_no_vm_suspend() {
        let mut parent = KProcess::new(ProcNr(5), Endpoint::from_generation_slot(1, 5));
        parent.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        let params = crate::vm::VmCheckParams {
            start: minix_types::VirBytes::new(0x1000),
            length: minix_types::VirBytes::new(0x100),
            write_flag: true,
        };
        parent.suspend_for_vm(
            crate::vm::VmSuspendType::KernelCall,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
        );

        let child = KProcess::fork_from(&parent, ProcNr(10), Endpoint::from_generation_slot(1, 10));

        // Child should NOT inherit VM suspend state
        assert!(!child.is_vm_suspended());
        assert!(child.p_vm_suspend.is_none());
        assert_eq!(child.p_next_requestor, None);
    }

    #[test]
    fn test_is_vm_suspended_reflects_rts_flag() {
        let proc = KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1));
        // New process has SLOT_FREE, not VMREQUEST
        assert!(!proc.is_vm_suspended());
    }
}
