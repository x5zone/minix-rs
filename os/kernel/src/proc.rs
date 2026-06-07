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

use minix_types::{Endpoint, Message, VirBytes};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicI8, Ordering};

use crate::vm::{VmSuspendContext, VmSuspendType, VmCheckParams, VmSuspendState, VmCopyContext};

const EXT_REG_STATE_SIZE: usize = 576;

#[repr(align(64))]
#[derive(Debug, Clone)]
pub struct ExtRegState {
    data: [u8; EXT_REG_STATE_SIZE],
    valid: bool,
}

impl ExtRegState {
    pub fn new() -> Self {
        Self { data: [0; EXT_REG_STATE_SIZE], valid: false }
    }

    pub fn is_valid(&self) -> bool { self.valid }

    pub fn mark_valid(&mut self) { self.valid = true; }

    pub fn invalidate(&mut self) { self.valid = false; }
}

impl Default for ExtRegState {
    fn default() -> Self { Self::new() }
}

/// Process number type (corresponds to C's `proc_nr_t`).
pub type ProcNr = i32;

/// Clock ticks type.
pub type ClockTicks = u64;

/// CPU cycles type.
pub type CpuCycles = u64;

/// Number of kernel tasks.
/// TODO: Move to a shared constant (minix-types or kernel config).
const NR_TASKS: usize = 5;

/// Process number constants.
/// Note: In Minix3, p_nr values are slot indices. Kernel tasks have negative
/// p_nr (e.g., CLOCK=-3, SYSTEM=-2, KERNEL=-1), user processes have p_nr >= 0.
/// These are dynamic values assigned at slot allocation, not special constants.
/// The special "NONE" value belongs to Endpoint, not ProcNr.
pub mod proc_nr {
    use super::ProcNr;
    pub const MIN_TASK_NR: ProcNr = -(super::NR_TASKS as ProcNr);
    pub const IDLE: ProcNr = -4;
    pub const CLOCK: ProcNr = -3;
    pub const SYSTEM: ProcNr = -2;
    pub const KERNEL: ProcNr = -1;
}

/// Runtime status flags.
pub mod rts {
    pub const SLOT_FREE: u32 = 0x01;
    pub const PROC_STOP: u32 = 0x02;
    pub const SENDING: u32 = 0x04;
    pub const RECEIVING: u32 = 0x08;
    pub const SIGNALED: u32 = 0x10;
    pub const SIG_PENDING: u32 = 0x20;
    pub const P_STOP: u32 = 0x40;
    pub const NO_PRIV: u32 = 0x80;
    pub const NO_ENDPOINT: u32 = 0x100;
    pub const VMINHIBIT: u32 = 0x200;
    pub const PAGEFAULT: u32 = 0x400;
    pub const VMREQUEST: u32 = 0x800;
    pub const VMREQTARGET: u32 = 0x1000;
    pub const PREEMPTED: u32 = 0x4000;
    pub const NO_QUANTUM: u32 = 0x8000;
    pub const BOOTINHIBIT: u32 = 0x10000;
}

/// Miscellaneous flags.
pub mod mf {
    pub const REPLY_PEND: u32 = 0x001;
    pub const VIRT_TIMER: u32 = 0x002;
    pub const PROF_TIMER: u32 = 0x004;
    pub const KCALL_RESUME: u32 = 0x008;
    pub const DELIVERMSG: u32 = 0x040;
    pub const SIG_DELAY: u32 = 0x080;
    pub const SC_ACTIVE: u32 = 0x100;
    pub const SC_DEFER: u32 = 0x200;
    pub const SC_TRACE: u32 = 0x400;
    pub const EXT_REG_INITIALIZED: u32 = 0x1000;
    pub const SENDING_FROM_KERNEL: u32 = 0x2000;
    pub const CONTEXT_SET: u32 = 0x4000;
    pub const SPROF_SEEN: u32 = 0x8000;
    pub const FLUSH_TLB: u32 = 0x10000;
    pub const SENDA_VM_MISS: u32 = 0x20000;
    pub const STEP: u32 = 0x40000;
    pub const MSGFAILED: u32 = 0x80000;
    pub const NICED: u32 = 0x100000;
}

/// Priority range constants.
pub mod priority {
    pub const TASK_Q: i8 = 0;
    pub const MAX_USER_Q: i8 = 0;
    pub const USER_Q: i8 = 7;
    pub const MIN_USER_Q: i8 = 15;
    pub const NR_SCHED_QUEUES: usize = 16;
}

/// Runtime status flags (wraps atomic operations).
#[derive(Debug)]
pub struct RtsFlags(AtomicU32);

/// Miscellaneous flags (wraps atomic operations).
#[derive(Debug)]
pub struct MiscFlags(AtomicU32);

/// Priority newtype (wraps validity check).
///
/// Valid range: `TASK_Q(0)` to `MIN_USER_Q(15)`.
/// The special value `-1` in Minix3's `sched_proc()` means "keep current priority"
/// and is NOT a valid `Priority` — it is a parameter sentinel, not a priority value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority(i8);

impl Priority {
    pub fn new(value: i8) -> Option<Self> {
        if value >= priority::TASK_Q && value <= priority::MIN_USER_Q {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn get(&self) -> i8 {
        self.0
    }

    pub fn is_kernel(&self) -> bool {
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
    pub fn new(size_ms: u32) -> Self {
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

/// CPU ID.
pub type CpuId = u32;

/// Scheduling fields extension.
#[derive(Debug)]
pub struct SchedFields {
    pub priority: AtomicI8,
    pub quantum: Quantum,
    pub cpu: AtomicU32,
    /// User-space scheduler process number.
    /// `None` means kernel default scheduling (C: `p_scheduler == NULL || p_scheduler == self`).
    /// `Some(nr)` means the process at slot `nr` is the user-space scheduler.
    /// Corresponds to C's `struct proc *p_scheduler`.
    pub scheduler: Option<ProcNr>,
}

impl SchedFields {
    pub fn new() -> Self {
        Self {
            priority: AtomicI8::new(priority::USER_Q),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
            scheduler: None,
        }
    }

    pub fn with_priority(priority: i8) -> Self {
        Self {
            priority: AtomicI8::new(priority),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
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
    pub fn new() -> Self {
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
    pub fn new() -> Self {
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
    pub fn new() -> Self {
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
    pub fn new() -> Self {
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
#[derive(Debug)]
pub struct KProcess {
    /// Process number (slot index).
    pub p_nr: ProcNr,
    /// Endpoint identifier.
    pub p_endpoint: Endpoint,
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

    pub p_defer: DeferArgs,

    // IPC queue pointers
    /// Next process pointer in ready queue.
    /// Used by scheduler to manage ready process list at same priority.
    pub p_nextready: Option<ProcNr>,

    /// Sender queue head pointer.
    /// Points to head of process queue waiting to send message to this process.
    pub p_caller_q: Option<ProcNr>,

    /// Sender queue link pointer.
    /// Links to next process in same sender queue.
    pub p_q_link: Option<ProcNr>,

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

    /// Extended register state (XSAVE area on x86-64, VFP/NEON on ARM64, F/D on RISC-V).
    ///
    /// Modern 64-bit architectures do not have a separate FPU. Instead, floating-point
    /// and SIMD operations use extended registers that are part of the general context.
    /// This field stores the architecture-specific extended register save area.
    ///
    /// Only valid when `MF_EXT_REG_INITIALIZED` flag is set.
    pub p_ext_reg_state: ExtRegState,

    // VM request fields (03-vm-request.md §3.4, §3.5)
    /// Next process in vmrestart chain.
    /// C: `p_vmrequest.nextrestart` (struct proc *)
    pub p_next_restart: Option<ProcNr>,

    /// Next process in vmrequest queue.
    /// C: `p_vmrequest.nextrequestor` (struct proc *)
    /// Design decision: §3.4 (ProcNr index replaces *proc pointer, moved from
    /// p_vmrequest to KProcess top level alongside p_nextready/p_caller_q).
    pub p_next_requestor: Option<ProcNr>,

    /// VM suspend context.
    /// `Some` when the process has a pending VM memory request.
    /// `None` when no request is pending.
    /// C: `p_vmrequest` anonymous struct (always embedded, validity controlled by RTS_VMREQUEST).
    /// Design decision: §3.5 (Option replaces always-embedded struct + RTS_VMREQUEST flag).
    ///
    /// Invariant: `p_rts_flags.is_set(rts::VMREQUEST) <==> p_vm_suspend.is_some()`
    pub p_vm_suspend: Option<VmSuspendContext>,
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
#[derive(Debug, Clone, Copy, Default)]
pub struct SigSet(u64);

impl SigSet {
    /// Creates empty signal set.
    pub const fn empty() -> Self {
        Self(0)
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
}

impl RtsFlags {
    pub fn new(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }

    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn store(&self, value: u32) {
        self.0.store(value, Ordering::Release);
    }

    pub fn is_runnable(&self) -> bool {
        self.load() == 0
    }

    pub fn is_set(&self, flags: u32) -> bool {
        (self.load() & flags) == flags
    }

    // TODO: RTS_SET/RTS_UNSET in Minix3 also call dequeue/enqueue when
    // the process transitions between runnable/non-runnable. Once the
    // scheduler is implemented, these methods need scheduling integration.
    pub fn set(&self, flags: u32) {
        self.0.fetch_or(flags, Ordering::AcqRel);
    }

    pub fn clear(&self, flags: u32) {
        self.0.fetch_and(!flags, Ordering::AcqRel);
    }
}

impl MiscFlags {
    pub fn new(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }

    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn is_set(&self, flags: u32) -> bool {
        (self.load() & flags) == flags
    }

    pub fn set(&self, flags: u32) {
        self.0.fetch_or(flags, Ordering::AcqRel);
    }

    pub fn clear(&self, flags: u32) {
        self.0.fetch_and(!flags, Ordering::AcqRel);
    }
}

impl KProcess {
    pub fn new(nr: ProcNr, endpoint: Endpoint) -> Self {
        Self {
            p_nr: nr,
            p_endpoint: endpoint,
            p_rts_flags: RtsFlags::new(rts::SLOT_FREE),
            p_misc_flags: MiscFlags::new(0),
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_cpuavg: CpuAvg::new(),
            p_dequeued: AtomicU64::new(0),
            p_defer: DeferArgs::default(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            p_pending: SigSet::empty(),
            p_name: ProcName::new(),
            p_sendmsg: Message::default(),
            p_delivermsg: Message::default(),
            p_delivermsg_vir: VirBytes::new(0),
            p_ext_reg_state: ExtRegState::new(),
            p_next_restart: None,
            p_next_requestor: None,
            p_vm_suspend: None,
        }
    }

    pub fn is_runnable(&self) -> bool {
        self.p_rts_flags.is_runnable()
    }

    pub fn get_priority(&self) -> Priority {
        Priority::new(self.p_sched.priority.load(Ordering::Acquire))
            .unwrap_or(Priority::default())
    }

    /// Sets priority with validation. Returns false if value is out of range.
    pub fn set_priority(&self, prio: i8) -> bool {
        if let Some(p) = Priority::new(prio) {
            self.p_sched.priority.store(p.get(), Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Sets priority without validation. For internal use by sched_proc
    /// where the caller has already validated the range.
    pub fn set_priority_unchecked(&self, prio: i8) {
        self.p_sched.priority.store(prio, Ordering::Release);
    }

    pub fn reset_accounting(&self) {
        self.p_accounting.reset();
    }

    pub fn blocked_on(&self) -> Option<Endpoint> {
        if self.p_rts_flags.is_set(rts::SENDING) {
            Some(self.p_sendto_e)
        } else if self.p_rts_flags.is_set(rts::RECEIVING) {
            Some(self.p_getfrom_e)
        } else {
            None
        }
    }

    // ── VM suspend/resume methods (03-vm-request.md §3.5, §3.9, §3.10) ──

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
        debug_assert!(!self.p_rts_flags.is_set(rts::VMREQUEST));
        debug_assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
            copy_context: None,
        });

        self.p_rts_flags.set(rts::VMREQUEST);
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
        debug_assert!(!self.p_rts_flags.is_set(rts::VMREQUEST));
        debug_assert!(self.p_vm_suspend.is_none());

        self.p_vm_suspend = Some(VmSuspendContext {
            suspend_type,
            target,
            check_params: params,
            state: VmSuspendState::Pending,
            saved_msg,
            copy_context: Some(copy_ctx),
        });

        self.p_rts_flags.set(rts::VMREQUEST);
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
        self.p_rts_flags.clear(rts::VMREQUEST);
    }

    /// Check if this process is suspended waiting for a VM memory request.
    ///
    /// C: `RTS_ISSET(p, RTS_VMREQUEST)`
    pub fn is_vm_suspended(&self) -> bool {
        self.p_rts_flags.is_set(rts::VMREQUEST)
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
    /// | `p_nextready/p_caller_q/p_q_link` | Pointer copied but child gets own queues | `None` (child not queued yet) |
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
            let flags = parent.p_rts_flags.load();
            let flags = flags | rts::NO_QUANTUM;
            let flags = flags & !(rts::SIGNALED | rts::SIG_PENDING | rts::P_STOP | rts::VMREQUEST);
            RtsFlags::new(flags)
        };

        // Copy p_misc_flags then clear timer/trace flags:
        // rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_SPROF_SEEN | MF_STEP)
        let child_mf = {
            let flags = parent.p_misc_flags.load();
            let flags = flags
                & !(mf::VIRT_TIMER | mf::PROF_TIMER | mf::SC_TRACE | mf::SPROF_SEEN | mf::STEP);
            MiscFlags::new(flags)
        };

        let mut child = Self {
            p_nr: child_nr,
            p_endpoint: child_endpoint,
            p_rts_flags: child_rts,
            p_misc_flags: child_mf,
            p_sched: SchedFields {
                priority: AtomicI8::new(parent.p_sched.priority.load(Ordering::Acquire)),
                quantum: Quantum::new(parent.p_sched.quantum.size_ms.load(Ordering::Acquire)),
                cpu: AtomicU32::new(parent.p_sched.cpu.load(Ordering::Acquire)),
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
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: parent.p_getfrom_e,
            p_sendto_e: parent.p_sendto_e,
            // p_pending cleared: corresponds to sigemptyset(&rpc->p_pending)
            p_pending: SigSet::empty(),
            p_name: parent.p_name,
            p_sendmsg: parent.p_sendmsg.clone(),
            p_delivermsg: parent.p_delivermsg.clone(),
            p_delivermsg_vir: parent.p_delivermsg_vir,
            p_ext_reg_state: ExtRegState::new(),
            p_next_restart: None,
            // VM request fields: child has no pending VM request
            // C: fork copies p_vmrequest but child never has RTS_VMREQUEST set
            p_next_requestor: None,
            p_vm_suspend: None,
        };

        // Copy extended register state if parent has initialized it
        // Corresponds to Minix3's FPU copy on i386:
        //   if(proc_used_fpu(rpp))
        //       memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
        if parent.p_misc_flags.is_set(mf::EXT_REG_INITIALIZED) {
            child.p_ext_reg_state = parent.p_ext_reg_state.clone();
            child.p_misc_flags.set(mf::EXT_REG_INITIALIZED);
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
        child.p_rts_flags.set(rts::NO_PRIV);
    }

    // Set VMINHIBIT if requested
    // Corresponds to Minix3's:
    //   if(m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT) {
    //       RTS_SET(rpc, RTS_VMINHIBIT);
    //   }
    if flags & fork_flags::VMINHIBIT != 0 {
        child.p_rts_flags.set(rts::VMINHIBIT);
    }

    // Add "*F" suffix to process name
    // Corresponds to Minix3's:
    //   namelen = strlen(rpc->p_name);
    //   if(namelen+strlen(FORKSTR) < sizeof(rpc->p_name))
    //       strcat(rpc->p_name, "*F");
    child.p_name.push_suffix("*F");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rts_flags_runnable() {
        let flags = RtsFlags::new(0);
        assert!(flags.is_runnable());

        flags.set(rts::PROC_STOP);
        assert!(!flags.is_runnable());

        flags.clear(rts::PROC_STOP);
        assert!(flags.is_runnable());
    }

    #[test]
    fn test_rts_flags_multiple() {
        let flags = RtsFlags::new(0);
        flags.set(rts::SENDING | rts::RECEIVING);

        assert!(flags.is_set(rts::SENDING));
        assert!(flags.is_set(rts::RECEIVING));
        assert!(!flags.is_runnable());

        flags.clear(rts::SENDING);
        assert!(!flags.is_set(rts::SENDING));
        assert!(flags.is_set(rts::RECEIVING));
    }

    #[test]
    fn test_kprocess_new() {
        let proc = KProcess::new(1, Endpoint(1));

        assert_eq!(proc.p_nr, 1);
        assert!(!proc.is_runnable());
    }

    #[test]
    fn test_kprocess_runnable() {
        let proc = KProcess::new(1, Endpoint(1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

        assert!(proc.is_runnable());
    }

    #[test]
    fn test_priority_valid() {
        assert!(Priority::new(0).is_some());
        assert!(Priority::new(7).is_some());
        assert!(Priority::new(15).is_some());
        assert!(Priority::new(-1).is_none());
        assert!(Priority::new(16).is_none());
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
        let proc = KProcess::new(1, Endpoint(1));
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
        let proc = KProcess::new(1, Endpoint(1));
        proc.p_accounting.record_ipc_sync();
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 1);

        proc.reset_accounting();
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_basic() {
        let parent = KProcess::new(5, Endpoint::from_generation_slot(3, 5));
        parent.p_rts_flags.clear(rts::SLOT_FREE);
        parent.set_priority(priority::USER_Q);

        let child_endpoint = Endpoint::fork_new_endpoint(
            Endpoint::from_generation_slot(0, 10), 10
        );
        let child = KProcess::fork_from(&parent, 10, child_endpoint);

        assert_eq!(child.p_nr, 10);
        assert_eq!(child.p_endpoint, child_endpoint);
        assert_eq!(child.get_priority(), parent.get_priority());
        assert_eq!(child.p_time.user_time.load(Ordering::Relaxed), 0);
        assert_eq!(child.p_time.sys_time.load(Ordering::Relaxed), 0);
        assert!(child.p_pending.is_empty());
    }

    #[test]
    fn test_fork_from_accounting_reset() {
        let parent = KProcess::new(5, Endpoint(5));
        parent.p_accounting.record_ipc_sync();
        parent.p_accounting.record_ipc_sync();

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_independent_queues() {
        let mut parent = KProcess::new(5, Endpoint(5));
        parent.p_nextready = Some(3);
        parent.p_caller_q = Some(7);

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_nextready, None);
        assert_eq!(child.p_caller_q, None);
        assert_eq!(child.p_q_link, None);
    }

    #[test]
    fn test_fork_from_inherits_ipc_endpoints() {
        let mut parent = KProcess::new(5, Endpoint(5));
        parent.p_getfrom_e = Endpoint::PM;
        parent.p_sendto_e = Endpoint::VFS;

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_getfrom_e, Endpoint::PM);
        assert_eq!(child.p_sendto_e, Endpoint::VFS);
    }

    #[test]
    fn test_fork_from_cycles_reset() {
        let parent = KProcess::new(5, Endpoint(5));
        parent.p_cycles.add_cycles(1000);
        parent.p_cycles.add_kcall_cycles(200);

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        assert_eq!(child.p_cycles.total.load(Ordering::Relaxed), 0);
        assert_eq!(child.p_cycles.kcall.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_rts_flags_corrections() {
        // Parent has SIGNALED and SIG_PENDING set — child must NOT inherit these
        let parent = KProcess::new(5, Endpoint::from_generation_slot(1, 5));
        parent.p_rts_flags.clear(rts::SLOT_FREE);
        parent.p_rts_flags.set(rts::SIGNALED | rts::SIG_PENDING | rts::P_STOP);

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        // Child must have NO_QUANTUM set (C: RTS_SET(rpc, RTS_NO_QUANTUM))
        assert!(child.p_rts_flags.is_set(rts::NO_QUANTUM));
        // Child must NOT have SIGNALED, SIG_PENDING, P_STOP (C: RTS_UNSET)
        assert!(!child.p_rts_flags.is_set(rts::SIGNALED));
        assert!(!child.p_rts_flags.is_set(rts::SIG_PENDING));
        assert!(!child.p_rts_flags.is_set(rts::P_STOP));
    }

    #[test]
    fn test_fork_from_misc_flags_corrections() {
        // Parent has VIRT_TIMER, PROF_TIMER, STEP set — child must NOT inherit
        let parent = KProcess::new(5, Endpoint::from_generation_slot(1, 5));
        parent.p_misc_flags.set(mf::VIRT_TIMER | mf::PROF_TIMER | mf::STEP | mf::SC_TRACE | mf::SPROF_SEEN);
        // Also set a flag that SHOULD be inherited
        parent.p_misc_flags.set(mf::REPLY_PEND);

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        // Cleared flags
        assert!(!child.p_misc_flags.is_set(mf::VIRT_TIMER));
        assert!(!child.p_misc_flags.is_set(mf::PROF_TIMER));
        assert!(!child.p_misc_flags.is_set(mf::STEP));
        assert!(!child.p_misc_flags.is_set(mf::SC_TRACE));
        assert!(!child.p_misc_flags.is_set(mf::SPROF_SEEN));
        // Inherited flag
        assert!(child.p_misc_flags.is_set(mf::REPLY_PEND));
    }

    // ── §5.1: KProcess VM suspend/resume tests ──

    #[test]
    fn test_suspend_for_vm_sets_rts_and_context() {
        let mut proc = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

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

        assert!(proc.p_rts_flags.is_set(rts::VMREQUEST));
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
        let mut proc = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

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
        let mut proc = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

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
        assert!(!proc.p_rts_flags.is_set(rts::VMREQUEST));
        assert!(proc.p_vm_suspend.is_none());
        assert!(proc.vm_suspend_context().is_none());
    }

    #[test]
    fn test_clear_vm_suspend_does_not_clear_kcall_resume() {
        let mut proc = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

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
        proc.p_misc_flags.set(mf::KCALL_RESUME);
        proc.p_rts_flags.clear(rts::VMREQUEST);

        proc.clear_vm_suspend();

        // MF_KCALL_RESUME should NOT be cleared by clear_vm_suspend
        // (C's clear_memreq only clears RTS_VMREQUEST, not MF_KCALL_RESUME)
        assert!(proc.p_misc_flags.is_set(mf::KCALL_RESUME));
    }

    #[test]
    fn test_vm_suspend_context_mut() {
        let mut proc = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

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
        let mut parent = KProcess::new(5, Endpoint::from_generation_slot(1, 5));
        parent.p_rts_flags.clear(rts::SLOT_FREE);

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

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        // Child should NOT inherit VM suspend state
        assert!(!child.is_vm_suspended());
        assert!(child.p_vm_suspend.is_none());
        assert_eq!(child.p_next_requestor, None);
    }

    #[test]
    fn test_is_vm_suspended_reflects_rts_flag() {
        let proc = KProcess::new(1, Endpoint::from_generation_slot(1, 1));
        // New process has SLOT_FREE, not VMREQUEST
        assert!(!proc.is_vm_suspended());
    }
}
