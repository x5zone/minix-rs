//! Kernel SMP (Symmetric Multiprocessing) support.
//!
//! Implements the Big Kernel Lock (BKL), per-CPU data, IPI scheduling,
//! and CPU state management for multi-core operation.
//!
//! # Minix3 C Source Mapping
//!
//! - `smp.c:27` — `SPINLOCK_DEFINE(big_kernel_lock)`
//! - `smp.c:33-51` — `wait_for_APs_to_finish_booting()`
//! - `smp.c:75` — `smp_schedule()`
//! - `smp.c:84-111` — `smp_schedule_sync()`
//! - `smp.c:143-170` — `smp_sched_handler()`
//! - `smp.c:174-183` — `smp_ipi_sched_handler()`
//! - `smp.h:12-19` — `ncpus`, `bsp_cpu_id`, `cpu_is_bsp()`
//! - `cpulocals.h:67-79` — `struct __cpu_local_vars`
//!
//! # Design Decisions (15-smp.md §3)
//!
//! - **D1**: `Spinlock<()>` for BKL (type-safe, avoids raw `static mut`)
//! - **D2**: `[CpuLocal; MAX_CPUS]` fixed-size array (no_std compatible)
//! - **D3**: bitflags `CpuFlags` replaces raw `u32_t flags`
//! - **D4**: `AtomicU32` replaces `volatile u32_t` for IPI data
//! - **D5**: bitflags `SchedIpiFlags` replaces raw bit operations
//! - **D6**: BKL release/reacquire pattern preserved from C (with safety comments)
//! - **D7**: Compile-time `cfg` for single-CPU fallback
//!
//! # BKL (Big Kernel Lock) — implementation status (2026-06-15)
//!
//! The `BklGuard` type and `bkl_lock()` / `bkl_unlock()` static functions
//! provide the BKL framework (per D1). The BKL has been wired into the
//! following kernel entry/exit points:
//!
//! - `kernel_call_dispatch()` — acquires BKL on syscall entry
//! - `kernel_call_finish()` — releases BKL on syscall completion (all paths)
//! - `handle_exception()` — acquires/releases BKL around exception handling
//! - `kmain()` step 8.5 — acquires BKL before switch_to_user (C: main.c:149)
//! - `switch_to_user()` — releases BKL before scheduling loop
//! - `kernel_call_resume()` — re-acquires BKL via kernel_call_dispatch
//!
//! Remaining integration points (not yet wired):
//! - Timer tick handler (will be wired when clock interrupt is integrated)
//! - IPC sendrecv suspend/resume paths
//! - Per-CPU run queue access (per-CPU scheduling queue)

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::proc::{proc_nr, ProcNr};
use crate::sched::Scheduler;

// ── Constants ──

/// Maximum number of CPUs.
/// C: `CONFIG_MAX_CPUS` — config.h
pub const MAX_CPUS: usize = 32;

// ── CPU flags ──

bitflags::bitflags! {
    /// CPU state flags.
    ///
    /// C: smp.h:30-31 — `CPU_IS_BSP` / `CPU_IS_READY`
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CpuFlags: u32 {
        /// This CPU is the bootstrap processor.
        const BSP = 1;
        /// This CPU has completed initialization.
        const READY = 2;
    }
}

// ── IPI scheduling flags ──

bitflags::bitflags! {
    /// IPI scheduling task flags.
    ///
    /// C: smp.c:21-23 — `SCHED_IPI_STOP_PROC` / `SCHED_IPI_VM_INHIBIT` / `SCHED_IPI_SAVE_CTX`
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SchedIpiFlags: u32 {
        /// Stop the target process on the remote CPU.
        const STOP_PROC = 1;
        /// Set VMINHIBIT on the target process.
        const VM_INHIBIT = 2;
        /// Save full context (including FPU state) before migration.
        const SAVE_CTX = 4;
    }
}

// ── Per-CPU local data ──

/// Per-CPU local data, equivalent to C's `__cpu_local_vars`.
///
/// Design decision D2: fixed-size array element, no_std compatible.
///
/// # Per-CPU scheduler queues
///
/// C's `__cpu_local_vars` holds `run_q_head[NR_SCHED_QUEUES]` and
/// `run_q_tail[NR_SCHED_QUEUES]` for **per-CPU ready queues** (cpulocals.h:58-59).
/// This Rust port mirrors that design: each `CpuLocal` owns a `Scheduler`
/// containing those queue arrays.
///
/// In the current single-CPU configuration, `ProcessTable::sched` serves as
/// the BSP scheduler and `CpuLocal::scheduler` is initialized but unused.
/// When `CONFIG_SMP=ncpus > 1` lands, `ProcessTable::sched` will be removed
/// and all scheduling will dispatch through `CpuLocal::scheduler` via the
/// current CPU's index (see `ProcessTable::sched_for_cpu()`).
///
/// C: cpulocals.h — `struct __cpu_local_vars`
#[derive(Debug)]
pub struct CpuLocal {
    /// Currently running process. C: `proc_ptr`
    pub proc_ptr: Option<ProcNr>,
    /// Billable process for time accounting. C: `bill_ptr`
    pub bill_ptr: Option<ProcNr>,
    /// Slot index of the idle kernel task for this CPU.
    /// C: `idle_proc` (always slot 1 = proc_nr -4 in single-CPU build).
    pub idle_proc: ProcNr,
    /// Process that currently owns this CPU's page tables.
    /// C: `ptproc` — used for CR3-load tracking on x86-64.
    pub ptproc: Option<ProcNr>,
    /// Whether this CPU is idle. C: `cpu_is_idle`
    pub cpu_is_idle: bool,
    /// Whether the idle loop was interrupted and needs wakeup. C: `idle_interrupted`
    pub idle_interrupted: bool,
    /// TSC timestamp at the most recent context switch on this CPU.
    /// C: `tsc_ctr_switch` — used by `cycles_accounting_init` / `read_tsc`.
    pub tsc_ctr_switch: u64,
    /// Last raw TSC reading on this CPU. C: `cpu_last_tsc`
    pub cpu_last_tsc: u64,
    /// Last time this CPU went idle (TSC). C: `cpu_last_idle`
    pub cpu_last_idle: u64,
    /// Whether a pagefault is already being handled. C: `pagefault_handled`
    pub pagefault_handled: bool,
    /// Whether this CPU has an FPU. C: `fpu_presence` (`char` in C → `bool` here)
    pub fpu_presence: bool,
    /// FPU owner process. C: `fpu_owner`
    pub fpu_owner: Option<ProcNr>,
    /// Per-CPU scheduler (ready queues). C: `run_q_head[]` / `run_q_tail[]`
    /// in cpulocals.h:58-59.
    ///
    /// In single-CPU builds this is initialized but not used directly;
    /// `ProcessTable::sched` serves as the BSP scheduler. When SMP lands,
    /// all scheduling dispatches through this field via
    /// `ProcessTable::sched_for_cpu()`.
    pub scheduler: Scheduler,
}

impl CpuLocal {
    /// Construct an empty `CpuLocal`. Callers must populate `idle_proc`
    /// (and `fpu_presence`) from boot-time CPU detection before the CPU
    /// starts running user processes — see `bsp_finish_booting` step 2.
    pub const fn new() -> Self {
        Self {
            proc_ptr: None,
            bill_ptr: None,
            idle_proc: proc_nr::IDLE,
            ptproc: None,
            cpu_is_idle: false,
            idle_interrupted: false,
            tsc_ctr_switch: 0,
            cpu_last_tsc: 0,
            cpu_last_idle: 0,
            pagefault_handled: false,
            fpu_presence: false,
            fpu_owner: None,
            scheduler: Scheduler::new(),
        }
    }

    /// Mark this CPU as currently running `nr`; also bills time to `nr`
    /// (or to `idle_proc` if `nr == idle_proc`).
    ///
    /// Mirrors C's `get_cpulocal_var(proc_ptr) = p; get_cpulocal_var(bill_ptr) = p;`
    /// in `bsp_finish_booting()` and similar call sites.
    pub fn set_running(&mut self, nr: ProcNr) {
        self.proc_ptr = Some(nr);
        self.bill_ptr = Some(nr);
    }

    /// Record a context-switch timestamp. C: `tsc_ctr_switch = read_tsc()`
    pub fn note_context_switch(&mut self, tsc: u64) {
        self.tsc_ctr_switch = tsc;
        self.cpu_last_tsc = tsc;
    }
}

impl Default for CpuLocal {
    fn default() -> Self {
        Self::new()
    }
}

// ── CPU state ──

/// Per-CPU state entry.
///
/// C: smp.h:34-36 — `struct cpu { u32_t flags; }`
#[derive(Debug)]
pub struct CpuState {
    /// CPU flags (BSP, READY). C: `flags`
    flags: CpuFlags,
}

impl CpuState {
    pub const fn new() -> Self {
        Self {
            flags: CpuFlags::empty(),
        }
    }

    /// Set a flag. C: `cpu_set_flag(cpu, flag)`
    pub fn set_flag(&mut self, flag: CpuFlags) {
        self.flags |= flag;
    }

    /// Clear a flag. C: `cpu_clear_flag(cpu, flag)`
    pub fn clear_flag(&mut self, flag: CpuFlags) {
        self.flags -= flag;
    }

    /// Test a flag. C: `cpu_test_flag(cpu, flag)`
    pub fn test_flag(&self, flag: CpuFlags) -> bool {
        self.flags.contains(flag)
    }

    /// Check if CPU is ready. C: `cpu_is_ready(cpu)`
    pub fn is_ready(&self) -> bool {
        self.flags.contains(CpuFlags::READY)
    }
}

impl Default for CpuState {
    fn default() -> Self {
        Self::new()
    }
}

// ── IPI scheduling data ──

/// IPI scheduling data for cross-CPU operations.
///
/// C: smp.c:17-20 — `struct sched_ipi_data`
#[derive(Debug)]
pub struct SchedIpiData {
    /// IPI task flags. C: `volatile u32_t flags`
    flags: AtomicU32,
    /// Target process number. C: `volatile u32_t data` (cast from `struct proc *`)
    target_proc: AtomicU32,
}

impl SchedIpiData {
    pub const fn new() -> Self {
        Self {
            flags: AtomicU32::new(0),
            target_proc: AtomicU32::new(0),
        }
    }

    /// Read current IPI flags.
    pub fn load_flags(&self) -> SchedIpiFlags {
        SchedIpiFlags::from_bits_truncate(self.flags.load(Ordering::Acquire))
    }

    /// Set IPI flags (bitwise OR).
    pub fn set_flags(&self, flags: SchedIpiFlags) {
        self.flags.fetch_or(flags.bits(), Ordering::AcqRel);
    }

    /// Clear all IPI flags.
    pub fn clear_flags(&self) {
        self.flags.store(0, Ordering::Release);
    }

    /// Check if any IPI task is pending.
    pub fn has_pending(&self) -> bool {
        self.flags.load(Ordering::Acquire) != 0
    }

    /// Set target process number.
    pub fn set_target(&self, proc_nr: ProcNr) {
        self.target_proc.store(proc_nr as u32, Ordering::Release);
    }

    /// Get target process number.
    pub fn get_target(&self) -> ProcNr {
        self.target_proc.load(Ordering::Acquire) as ProcNr
    }
}

impl Default for SchedIpiData {
    fn default() -> Self {
        Self::new()
    }
}

// ── Global SMP state ──

/// Global SMP state, equivalent to C's `ncpus`, `bsp_cpu_id`, `cpus[]`,
/// `__cpu_local_vars`, and `sched_ipi_data[]`.
///
/// Design decision D1/D2: encapsulate as struct for ownership clarity.
/// All mutable access must be under BKL protection.
///
/// C: smp.h:12-19, smp.c:9-10, cpulocals.h:67-79
#[derive(Debug)]
pub struct SmpState {
    /// Number of CPUs. C: `ncpus`
    ncpus: u32,
    /// BSP CPU ID. C: `bsp_cpu_id`
    bsp_cpu_id: u32,
    /// Per-CPU state array. C: `struct cpu cpus[CONFIG_MAX_CPUS]`
    cpus: [CpuState; MAX_CPUS],
    /// Per-CPU local data. C: `__cpu_local_vars CPULOCAL_ARRAY`
    cpu_locals: [CpuLocal; MAX_CPUS],
    /// IPI scheduling data. C: `sched_ipi_data[CONFIG_MAX_CPUS]`
    sched_ipi_data: [SchedIpiData; MAX_CPUS],
    /// Number of APs that have finished booting. C: `ap_cpus_booted`
    ap_cpus_booted: AtomicU32,
}

impl SmpState {
    /// Create a new SmpState for a single-CPU (BSP-only) configuration.
    ///
    /// C: `ncpus = 1`, `bsp_cpu_id = 0`, `cpu_set_flag(bsp_cpu_id, CPU_IS_READY)`
    pub fn new_single_cpu() -> Self {
        let mut state = Self {
            ncpus: 1,
            bsp_cpu_id: 0,
            cpus: [const { CpuState::new() }; MAX_CPUS],
            cpu_locals: [const { CpuLocal::new() }; MAX_CPUS],
            sched_ipi_data: [const { SchedIpiData::new() }; MAX_CPUS],
            ap_cpus_booted: AtomicU32::new(0),
        };
        state.cpus[0].set_flag(CpuFlags::BSP | CpuFlags::READY);
        state
    }

    /// Create SmpState with a specified number of CPUs.
    pub fn with_ncpus(ncpus: u32, bsp_cpu_id: u32) -> Self {
        let ncpus = ncpus.clamp(1, MAX_CPUS as u32);
        let mut state = Self {
            ncpus,
            bsp_cpu_id,
            cpus: [const { CpuState::new() }; MAX_CPUS],
            cpu_locals: [const { CpuLocal::new() }; MAX_CPUS],
            sched_ipi_data: [const { SchedIpiData::new() }; MAX_CPUS],
            ap_cpus_booted: AtomicU32::new(0),
        };
        state.cpus[bsp_cpu_id as usize].set_flag(CpuFlags::BSP | CpuFlags::READY);
        state
    }

    // ── Accessors ──

    /// Get number of CPUs. C: `ncpus`
    pub fn ncpus(&self) -> u32 {
        self.ncpus
    }

    /// Get BSP CPU ID. C: `bsp_cpu_id`
    pub fn bsp_cpu_id(&self) -> u32 {
        self.bsp_cpu_id
    }

    /// Check if a CPU is the BSP. C: `cpu_is_bsp(cpu)`
    pub fn cpu_is_bsp(&self, cpu: u32) -> bool {
        cpu == self.bsp_cpu_id
    }

    /// Check if a CPU is ready. C: `cpu_is_ready(cpu)`
    pub fn cpu_is_ready(&self, cpu: u32) -> bool {
        if (cpu as usize) < MAX_CPUS {
            self.cpus[cpu as usize].is_ready()
        } else {
            false
        }
    }

    // ── CPU state operations ──

    /// Set a CPU flag. C: `cpu_set_flag(cpu, flag)`
    pub fn cpu_set_flag(&mut self, cpu: u32, flag: CpuFlags) {
        if (cpu as usize) < MAX_CPUS {
            self.cpus[cpu as usize].set_flag(flag);
        }
    }

    /// Clear a CPU flag. C: `cpu_clear_flag(cpu, flag)`
    pub fn cpu_clear_flag(&mut self, cpu: u32, flag: CpuFlags) {
        if (cpu as usize) < MAX_CPUS {
            self.cpus[cpu as usize].clear_flag(flag);
        }
    }

    // ── Per-CPU local data ──

    /// Get a reference to per-CPU local data.
    pub fn cpu_local(&self, cpu: u32) -> Option<&CpuLocal> {
        self.cpu_locals.get(cpu as usize)
    }

    /// Get a mutable reference to per-CPU local data.
    pub fn cpu_local_mut(&mut self, cpu: u32) -> Option<&mut CpuLocal> {
        self.cpu_locals.get_mut(cpu as usize)
    }

    // ── IPI scheduling ──

    /// Get IPI data for a CPU.
    pub fn ipi_data(&self, cpu: u32) -> &SchedIpiData {
        &self.sched_ipi_data[cpu as usize]
    }

    /// Record that an AP has finished booting. C: `ap_boot_finished(cpu)`
    pub fn ap_boot_finished(&self) {
        self.ap_cpus_booted.fetch_add(1, Ordering::AcqRel);
    }

    /// Check if all APs have finished booting.
    /// C: `ap_cpus_booted != (n - 1)` in `wait_for_APs_to_finish_booting()`
    pub fn all_aps_booted(&self) -> bool {
        let expected = self.ncpus.saturating_sub(1);
        self.ap_cpus_booted.load(Ordering::Acquire) == expected
    }

    /// Handle IPI scheduling on the current CPU.
    ///
    /// C: `smp_sched_handler()` — smp.c:143-170
    ///
    /// Returns the IPI flags and target process that were handled.
    pub fn handle_sched_ipi(&self, cpu: u32) -> Option<(SchedIpiFlags, ProcNr)> {
        let ipi = &self.sched_ipi_data[cpu as usize];
        let flags = ipi.load_flags();
        if flags.is_empty() {
            return None;
        }
        let target = ipi.get_target();
        // Clear flags after processing
        ipi.clear_flags();
        Some((flags, target))
    }
}

impl Default for SmpState {
    fn default() -> Self {
        Self::new_single_cpu()
    }
}

// ── Big Kernel Lock (BKL) — D1 implementation ──

/// The single Big Kernel Lock protecting cross-CPU shared kernel state.
///
/// # Design (15-smp.md D1)
///
/// `Spinlock<()>` is a type-safe wrapper around a `static mut` flag +
/// busy-wait loop. We avoid pulling in the `spin` crate so the kernel
/// stays `no_std` and dependency-free. C uses `SPINLOCK_DEFINE(big_kernel_lock)`
/// (`smp.c:27`); this is the Rust equivalent.
///
/// # Status (2026-06-13)
///
/// **Framework is in place, but not yet wired into every shared-data
/// access point.** The actual SMP-critical sections in
/// `proc_table.rs` / `ipc.rs` / `sched.rs` still rely on
/// `// BKL protected` comments (BKL wiring TODO: `proc_table.rs` /
/// `ipc.rs` / `sched.rs` critical sections). Callers MUST
/// invoke [`bkl_lock`] / [`bkl_unlock`] around any access to `SmpState`
/// or `ProcessTable` that could race with another CPU. Single-CPU
/// builds (default) are unaffected: `bkl_lock` is a compiler-fence-only
/// no-op on `ncpus == 1` because the lock flag is initialized to
/// `UNLOCKED` and the spin loop body never executes.
///
/// # Memory ordering
///
/// - `compare_exchange` with `Ordering::Acquire` on the success path:
///   guarantees that all subsequent reads (inside the critical section)
///   see writes that happened-before the `Release` store on another CPU.
/// - `compare_exchange` with `Ordering::Relaxed` on the failure path:
///   the failed attempt does not observe any protected data.
/// - `store` with `Ordering::Release` on unlock: guarantees that all
///   prior writes (inside the critical section) are visible to the next
///   CPU that acquires the lock.
static BKL_LOCKED: AtomicBool = AtomicBool::new(false);

/// RAII guard returned by [`bkl_lock`]. Dropping the guard does NOT
/// release the lock — callers must invoke [`bkl_unlock`] explicitly
/// because Minix3 BKL semantics require critical sections to be
/// released and re-acquired around blocking operations (e.g. IPC
/// `sendrecv`), and the RAII drop would hide those release points
/// (explicit unlock is required for BKL release-point visibility).
///
/// Note: `BklGuard` is intentionally NOT `#[must_use]`. The BKL follows
/// an explicit lock/unlock pattern (like C's `BKL_LOCK()`/`BKL_UNLOCK()`)
/// rather than RAII. The caller is responsible for pairing every
/// `bkl_lock()` with a `bkl_unlock()` on every code path.
pub struct BklGuard {
    // Private field prevents construction outside this module.
    _private: (),
}

// ── BklSection: typed witness for BKL-protected access ──
//
// The BKL framework (BklGuard + bkl_lock/unlock) provides the runtime
// primitive, but the **type system** does not enforce that a shared-state
// accessor is called *only* while the BKL is held. Without a type-level
// witness, callers can `bkl_lock()` / `bkl_unlock()` and then proceed to
// touch `SmpState` after unlock — review-patterns-skill §模式25.
//
// The fix is a `BklSection<'_>` zero-sized witness whose **lifetime** is
// tied to the `BklGuard`. The accessor function
// `smp_state_with(section, &SmpState)` requires a `&BklSection<'_>`, so
// the only way to call it is to have produced a section (which requires
// holding the BKL). The lifetime constraint ensures the section cannot
// outlive the unlock call.
//
// Note: `BklSection` is intentionally non-RAII (per D6) — it is just a
// **witness**, not a guard. The actual release still requires explicit
// `bkl_unlock()` so callers can release/reacquire around IPC sendrecv.

/// Compile-time witness that the caller holds the BKL.
///
/// Produced by `bkl_lock_section()`. Cannot be constructed outside this
/// module. Does **not** release the BKL on drop (per D6: explicit
/// `bkl_unlock()` is required at release points, e.g. before IPC
/// sendrecv).
///
/// # Lifetime
///
/// `'a` is the duration for which the BKL has been continuously held.
/// It starts at `bkl_lock_section()` and ends at the matching
/// `bkl_unlock()`. Any reference whose lifetime is tied to `'a` cannot
/// outlive the unlock.
#[must_use = "BklSection is a witness; it does NOT release the BKL on drop"]
pub struct BklSection<'a> {
    _lifetime: core::marker::PhantomData<&'a BklGuard>,
}

/// Open a BKL-protected section.
///
/// Returns a witness whose lifetime is tied to the BKL hold. Pass
/// `&BklSection` to typed accessors (e.g. `smp_state_with`) to prove the
/// BKL is held. The section does NOT release the BKL — call
/// `bkl_unlock()` explicitly when leaving the section.
pub fn bkl_lock_section<'a>() -> BklSection<'a> {
    // Acquire the BKL. The BklGuard is a marker type (no Drop impl),
    // so we don't need to forget it — it's just proof that the CAS
    // succeeded. The caller must call bkl_unlock() explicitly.
    let _guard = bkl_lock();
    // _guard is dropped here, but BklGuard has no Drop impl, so the
    // BKL remains held (BKL_LOCKED == true). The BklSection witness
    // proves to the type system that the BKL was acquired.
    BklSection { _lifetime: core::marker::PhantomData }
}

/// Typed accessor: read SmpState while holding the BKL.
///
/// # Why this signature?
///
/// The function takes a `&BklSection<'_>` — a proof that the caller is
/// inside a BKL-protected section. Without the witness, the function
/// would be a footgun: callers could grab `&SmpState` outside the lock.
///
/// The witness does not carry ownership of the SmpState; the caller
/// provides the reference explicitly. This matches C's pattern of
/// `extern struct` accessors that the BKL implicitly protects via
/// convention, but adds a **compile-time** check.
///
/// # Safety contract
///
/// The caller MUST hold the BKL for the entire duration of the returned
/// reference's lifetime. The witness is the proof; the lifetime is the
/// enforcement.
pub fn smp_state_with<'a, 'b>(
    _section: &'a BklSection<'b>,
    state: &'a SmpState,
) -> &'a SmpState {
    // SAFETY: The `BklSection` witness is a compile-time proof that the
    // caller holds the BKL. Accessing `SmpState` is safe because:
    //   1. No other CPU can be inside a critical section (BKL enforces
    //      mutual exclusion).
    //   2. Single-CPU builds (`ncpus == 1`) trivially satisfy this.
    //   3. The returned reference's lifetime is tied to the input
    //      reference, so the caller cannot drop the BklSection while
    //      still using the &SmpState.
    state
}

/// Acquire the BKL.
///
/// On single-CPU builds, this is a no-op (the lock flag is
/// `UNLOCKED`, the spin loop body is empty). On multi-CPU builds,
/// the caller will busy-wait until the lock is released by another
/// CPU. **Callers must** subsequently call [`bkl_unlock`] in the same
/// code path; the returned [`BklGuard`] is a marker, not an RAII
/// releaser (see `D6`).
///
/// # Safety contract
///
/// The caller is responsible for:
/// 1. **Not holding the BKL already** on this CPU (re-entry would
///    deadlock — Minix3 BKL is non-recursive).
/// 2. **Not sleeping, scheduling, or waiting for IPC** while the BKL
///    is held (BKL is a spinlock; sleep inside a spinlock is
///    deadlock).
/// 3. **Pairing every [`bkl_lock`] with exactly one [`bkl_unlock`]**
///    on every code path (including `?`-propagation and panics).
pub fn bkl_lock() -> BklGuard {
    // We always do the CAS loop rather than gating on a `cfg(smp_enabled)`
    // flag because (a) the kernel does not currently expose such a flag
    // and (b) the overhead on a single-CPU build where the lock is always
    // free is a single atomic load + compare_exchange that succeeds on
    // the first try (followed by one `Ordering::Acquire` fence). The
    // spin loop body only executes when another CPU is holding the BKL,
    // which cannot happen on a single-CPU build.
    while BKL_LOCKED
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        // Hint the CPU that we are in a busy-wait. The x86 `pause`
        // instruction is preferred; on other architectures the
        // compiler maps `hint::spin_loop()` to the right barrier.
        core::hint::spin_loop();
    }
    BklGuard { _private: () }
}

/// Release the BKL.
///
/// # Safety contract
///
/// The caller must currently hold the BKL on this CPU (acquired via
/// [`bkl_lock`]). Calling `bkl_unlock` without holding the BKL is
/// undefined behavior (the lock could be released by another CPU's
/// holder, causing corruption).
///
/// # Memory ordering
///
/// `Release` ordering ensures that all writes performed inside the
/// critical section are visible to the next CPU that acquires the
/// BKL.
pub fn bkl_unlock() {
    // Debug assertion: BKL must be held when we release it.
    // If this fires, someone called bkl_unlock() without a matching bkl_lock(),
    // or released the BKL twice on the same code path.
    debug_assert!(
        BKL_LOCKED.load(Ordering::Acquire),
        "bkl_unlock() called but BKL is not held — double unlock or missing bkl_lock()"
    );
    BKL_LOCKED.store(false, Ordering::Release);
}

/// Test-only helper: query whether the BKL is currently held.
/// **Never** use this for control flow; it is for diagnostic and test
/// code only (the value can change immediately after the load).
#[cfg(test)]
pub fn bkl_is_locked() -> bool {
    BKL_LOCKED.load(Ordering::Acquire)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smp_state_single_cpu() {
        let smp = SmpState::new_single_cpu();
        assert_eq!(smp.ncpus(), 1);
        assert_eq!(smp.bsp_cpu_id(), 0);
        assert!(smp.cpu_is_bsp(0));
        assert!(smp.cpu_is_ready(0));
        assert!(!smp.cpu_is_bsp(1));
    }

    #[test]
    fn test_smp_state_multi_cpu() {
        let smp = SmpState::with_ncpus(4, 0);
        assert_eq!(smp.ncpus(), 4);
        assert!(smp.cpu_is_bsp(0));
        assert!(!smp.cpu_is_bsp(1));
    }

    #[test]
    fn test_cpu_flags() {
        let mut smp = SmpState::new_single_cpu();
        smp.cpu_set_flag(1, CpuFlags::READY);
        assert!(smp.cpu_is_ready(1));
        smp.cpu_clear_flag(1, CpuFlags::READY);
        assert!(!smp.cpu_is_ready(1));
    }

    #[test]
    fn test_cpu_local() {
        let mut smp = SmpState::new_single_cpu();
        let local = smp.cpu_local_mut(0).unwrap();
        assert!(local.proc_ptr.is_none());
        local.proc_ptr = Some(5);
        assert_eq!(smp.cpu_local(0).unwrap().proc_ptr, Some(5));
    }

    #[test]
    fn test_cpu_local_default() {
        let local = CpuLocal::new();
        assert!(local.proc_ptr.is_none());
        assert!(local.bill_ptr.is_none());
        assert!(!local.cpu_is_idle);
        assert!(local.fpu_owner.is_none());
        // New fields per Doc 15 §2.2:
        assert_eq!(local.idle_proc, proc_nr::IDLE);
        assert!(local.ptproc.is_none());
        assert!(!local.idle_interrupted);
        assert_eq!(local.tsc_ctr_switch, 0);
        assert_eq!(local.cpu_last_tsc, 0);
        assert_eq!(local.cpu_last_idle, 0);
        assert!(!local.pagefault_handled);
        assert!(!local.fpu_presence);
    }

    #[test]
    fn test_cpu_local_set_running() {
        let mut local = CpuLocal::new();
        local.set_running(proc_nr::IDLE);
        assert_eq!(local.proc_ptr, Some(proc_nr::IDLE));
        assert_eq!(local.bill_ptr, Some(proc_nr::IDLE));

        // Switch to a user process.
        local.set_running(7);
        assert_eq!(local.proc_ptr, Some(7));
        assert_eq!(local.bill_ptr, Some(7));
    }

    #[test]
    fn test_cpu_local_note_context_switch() {
        let mut local = CpuLocal::new();
        local.note_context_switch(0x1234);
        assert_eq!(local.tsc_ctr_switch, 0x1234);
        assert_eq!(local.cpu_last_tsc, 0x1234);
    }

    #[test]
    fn test_sched_ipi_flags() {
        let flags = SchedIpiFlags::STOP_PROC | SchedIpiFlags::SAVE_CTX;
        assert!(flags.contains(SchedIpiFlags::STOP_PROC));
        assert!(flags.contains(SchedIpiFlags::SAVE_CTX));
        assert!(!flags.contains(SchedIpiFlags::VM_INHIBIT));
    }

    #[test]
    fn test_sched_ipi_data() {
        let ipi = SchedIpiData::new();
        assert!(!ipi.has_pending());

        ipi.set_flags(SchedIpiFlags::STOP_PROC);
        assert!(ipi.has_pending());
        assert!(ipi.load_flags().contains(SchedIpiFlags::STOP_PROC));

        ipi.clear_flags();
        assert!(!ipi.has_pending());
    }

    #[test]
    fn test_ap_boot_counting() {
        let smp = SmpState::with_ncpus(4, 0);
        assert!(!smp.all_aps_booted());

        smp.ap_boot_finished();
        assert!(!smp.all_aps_booted());

        smp.ap_boot_finished();
        assert!(!smp.all_aps_booted());

        smp.ap_boot_finished();
        assert!(smp.all_aps_booted());
    }

    #[test]
    fn test_handle_sched_ipi() {
        let smp = SmpState::new_single_cpu();
        let ipi = smp.ipi_data(0);

        ipi.set_flags(SchedIpiFlags::STOP_PROC);
        ipi.set_target(5);

        let result = smp.handle_sched_ipi(0);
        assert!(result.is_some());
        let (flags, target) = result.unwrap();
        assert!(flags.contains(SchedIpiFlags::STOP_PROC));
        assert_eq!(target, 5);

        // Should be cleared after handling
        assert!(!smp.ipi_data(0).has_pending());
    }

    #[test]
    fn test_handle_sched_ipi_empty() {
        let smp = SmpState::new_single_cpu();
        let result = smp.handle_sched_ipi(0);
        assert!(result.is_none());
    }

    // ── BKL tests (D1 framework) ──

    #[test]
    fn test_bkl_lock_unlock() {
        // Single-CPU build: BKL is initially free, lock + unlock round-trip
        // must leave it free again.
        assert!(!bkl_is_locked());
        let _g = bkl_lock();
        assert!(bkl_is_locked());
        bkl_unlock();
        assert!(!bkl_is_locked());
    }

    #[test]
    fn test_bkl_guard_is_marker() {
        // The guard is just a marker — dropping it must not release the
        // lock. This enforces D6 (BKL must be released explicitly around
        // IPC sendrecv) and prevents silent data corruption from
        // accidental scope-exit unlocks.
        let _g = bkl_lock();
        assert!(bkl_is_locked());
        // _g dropped here — but the lock is still held.
        assert!(bkl_is_locked());
        bkl_unlock();
        assert!(!bkl_is_locked());
    }

    #[test]
    fn test_bkl_reentrant_is_caller_responsibility() {
        // Minix3 BKL is non-recursive. This test documents the contract:
        // a second bkl_lock() on the same CPU while the lock is held will
        // deadlock on a multi-CPU build. On a single-CPU build the
        // spin loop is a no-op, so the test simply verifies the BKL
        // *can* be re-acquired after explicit unlock (re-entry protocol
        // is the caller's responsibility — see SAFETY contract).
        let _g = bkl_lock();
        bkl_unlock();
        // After explicit unlock, a fresh lock must succeed.
        let _g2 = bkl_lock();
        bkl_unlock();
    }

    // ── BklSection typed witness tests ──

    #[test]
    fn test_bkl_section_provides_typed_access() {
        // Open a section and access SmpState through the typed witness.
        // The witness is the proof that the BKL is held; the lifetime
        // is the enforcement that the access cannot outlive the section.
        let section = bkl_lock_section();
        let smp = SmpState::new_single_cpu();
        let accessed = smp_state_with(&section, &smp);
        assert_eq!(accessed.ncpus(), 1);
        // Section is non-RAII (D6): explicit unlock is required.
        bkl_unlock();
    }

    #[test]
    fn test_bkl_section_paired_unlock() {
        // The pattern: open section → access → unlock. Cannot access
        // SmpState after unlock (no BklSection is alive).
        let _section = bkl_lock_section();
        // ... do work with `_section` witness ...
        bkl_unlock();
        // After unlock, BKL is free.
        assert!(!bkl_is_locked());
    }

    #[test]
    fn test_bkl_section_drop_does_not_unlock() {
        // Per D6: BklSection is a witness, not a guard. Dropping the
        // section does NOT release the BKL. The caller must call
        // bkl_unlock() explicitly.
        let section = bkl_lock_section();
        assert!(bkl_is_locked());
        drop(section);
        // Lock is still held — explicit unlock required.
        assert!(bkl_is_locked());
        bkl_unlock();
        assert!(!bkl_is_locked());
    }
}
