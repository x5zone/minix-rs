//! Kernel SMP (Symmetric Multiprocessing) support.
//!
//! Implements the Big Kernel Lock (BKL), per-CPU data, IPI scheduling,
//! and CPU state management for multi-core operation.
//!
//! # Minix3 C Source Mapping
//!
//! - `smp.c:27` — `SPINLOCK_DEFINE(big_kernel_lock)`
//! - `smp.c:30-49` — `wait_for_APs_to_finish_booting()`
//! - `smp.c:56-61` — `smp_ipi_halt_handler()`
//! - `smp.c:63-66` — `smp_schedule()`
//! - `smp.c:75-112` — `smp_schedule_sync()`
//! - `smp.c:114-154` — `smp_schedule_stop_proc/vminhibit/stop_proc_save_ctx/migrate_proc`
//! - `smp.c:156-187` — `smp_sched_handler()`
//! - `smp.c:194-204` — `smp_ipi_sched_handler()`
//! - `smp.h:12-19` — `ncpus`, `bsp_cpu_id`, `cpu_is_bsp()`
//! - `cpulocals.h:37-75` — `struct __cpu_local_vars`
//!
//! # Design Decisions (16-smp.md §3)
//!
//! - **D1**: `AtomicBool` + CAS for BKL (type-safe, avoids raw `static mut`)
//! - **D2**: `[CpuLocal; MAX_CPUS]` fixed-size array (no_std compatible)
//! - **D3**: bitflags `CpuFlags` replaces raw `u32_t flags`
//! - **D4**: `AtomicU32` replaces `volatile u32_t` for IPI data
//! - **D5**: bitflags `SchedIpiFlags` replaces raw bit operations
//! - **D6**: BKL release/reacquire pattern preserved from C. R-05 (2026-08-12)
//!   unified `BklGuard` and `BklGuardRaii` into a single RAII type — `Drop`
//!   releases the BKL. Cross-function BKL transfer uses `mem::forget(guard)`;
//!   explicit early release uses `BklGuard::release()`.
//! - **D7**: `SmpArch` trait for hardware abstraction (no `#[cfg(target_arch)]`)
//! - **D8**: `ProcNr` index replaces raw `struct proc *` pointers
//! - **D9**: Runtime CAS for single-CPU fallback (no cfg gate, zero overhead)
//!
//! # BKL (Big Kernel Lock) — wiring status
//!
//! BKL is acquired/released at kernel entry/exit points:
//!
//! - `kernel_call_dispatch()` — acquires BKL on syscall entry
//! - `kernel_call_finish()` — releases BKL on syscall completion (all paths)
//! - `ExceptionDispatcher::handle()` — acquires/releases BKL around exception
//!   handling (BKL acquisition happens in the assembly trap entry for
//!   user-mode exceptions; see `os/arch/src/arch/exception_dispatcher.rs`)
//! - `kmain()` step 8.5 — acquires BKL before switch_to_user (C: main.c:149)
//! - `switch_to_user()` — releases BKL before scheduling loop
//! - `kernel_call_resume()` — re-acquires BKL via kernel_call_dispatch
//!
//! Pending integration points (require arch layer or SMP testing):
//! - Timer tick handler
//! - IPC sendrecv suspend/resume paths
//! - Per-CPU run queue cross-CPU access

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::proc::{proc_nr, CpuId, MiscFlagsBits, ProcNr, RtsFlagsBits};
use crate::sched::Scheduler;

// ── Constants ──

/// Maximum number of CPUs.
/// C: `CONFIG_MAX_CPUS` — config.h
pub const MAX_CPUS: usize = 32;

// ── CPU flags ──

bitflags::bitflags! {
    /// CPU state flags.
    ///
    /// C: smp.h:32-33 — `CPU_IS_BSP` / `CPU_IS_READY`
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

// ── Architecture abstraction (D7) ──
//
// The `SmpArch` trait is defined in `os/arch/src/arch/smp.rs` and re-exported
// via `minix_arch::SmpArch`. Each architecture provides a zero-sized
// implementor:
//   - x86_64:    `minix_arch::x86_64::smp::X86_64SmpArch` (LAPIC ICR + EOI)
//   - aarch64:   `minix_arch::arm64::smp::AArch64SmpArch`  (GIC SGIR + EOIR)
//   - riscv64:   `minix_arch::riscv64::smp::Riscv64SmpArch` (SBI ecall)
//   - mock/test: `minix_arch::arch::smp::MockSmpArch`       (no-op)
//
// The compile-time alias `minix_arch::CurrentSmpArch` selects the right
// backend, so kernel code never uses `#[cfg(target_arch)]` for behavior
// selection (D7 in 16-smp.md §3).
//
// C: arch-specific functions called from smp.c:
// - `arch_send_smp_schedule_ipi(cpu)` — smp.c:65
// - `arch_smp_halt_cpu()` — smp.c:60
// - `ipi_ack()` — smp.c:58,198
// - AP boot protocol — arch/i386/smp.c
pub use minix_arch::SmpArch;

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
        self.target_proc.store(proc_nr.0 as u32, Ordering::Release);
    }

    /// Get target process number.
    pub fn get_target(&self) -> ProcNr {
        ProcNr(self.target_proc.load(Ordering::Acquire) as i32)
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
    bsp_cpu_id: CpuId,
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
            bsp_cpu_id: CpuId::BSP,
            cpus: [const { CpuState::new() }; MAX_CPUS],
            cpu_locals: [const { CpuLocal::new() }; MAX_CPUS],
            sched_ipi_data: [const { SchedIpiData::new() }; MAX_CPUS],
            ap_cpus_booted: AtomicU32::new(0),
        };
        state.cpus[0].set_flag(CpuFlags::BSP | CpuFlags::READY);
        state
    }

    /// Create SmpState with a specified number of CPUs.
    pub fn with_ncpus(ncpus: u32, bsp_cpu_id: CpuId) -> Self {
        let ncpus = ncpus.clamp(1, MAX_CPUS as u32);
        let mut state = Self {
            ncpus,
            bsp_cpu_id,
            cpus: [const { CpuState::new() }; MAX_CPUS],
            cpu_locals: [const { CpuLocal::new() }; MAX_CPUS],
            sched_ipi_data: [const { SchedIpiData::new() }; MAX_CPUS],
            ap_cpus_booted: AtomicU32::new(0),
        };
        state.cpus[bsp_cpu_id.index()].set_flag(CpuFlags::BSP | CpuFlags::READY);
        state
    }

    // ── Accessors ──

    /// Get number of CPUs. C: `ncpus`
    pub fn ncpus(&self) -> u32 {
        self.ncpus
    }

    /// Get BSP CPU ID. C: `bsp_cpu_id`
    pub fn bsp_cpu_id(&self) -> CpuId {
        self.bsp_cpu_id
    }

    /// Check if a CPU is the BSP. C: `cpu_is_bsp(cpu)`
    pub fn cpu_is_bsp(&self, cpu: CpuId) -> bool {
        cpu == self.bsp_cpu_id
    }

    /// Check if a CPU is ready. C: `cpu_is_ready(cpu)`
    pub fn cpu_is_ready(&self, cpu: CpuId) -> bool {
        if cpu.index() < MAX_CPUS {
            self.cpus[cpu.index()].is_ready()
        } else {
            false
        }
    }

    // ── CPU state operations ──

    /// Set a CPU flag. C: `cpu_set_flag(cpu, flag)`
    pub fn cpu_set_flag(&mut self, cpu: CpuId, flag: CpuFlags) {
        if cpu.index() < MAX_CPUS {
            self.cpus[cpu.index()].set_flag(flag);
        }
    }

    /// Clear a CPU flag. C: `cpu_clear_flag(cpu, flag)`
    pub fn cpu_clear_flag(&mut self, cpu: CpuId, flag: CpuFlags) {
        if cpu.index() < MAX_CPUS {
            self.cpus[cpu.index()].clear_flag(flag);
        }
    }

    // ── Per-CPU local data ──

    /// Get a reference to per-CPU local data.
    pub fn cpu_local(&self, cpu: CpuId) -> Option<&CpuLocal> {
        self.cpu_locals.get(cpu.index())
    }

    /// Get a mutable reference to per-CPU local data.
    pub fn cpu_local_mut(&mut self, cpu: CpuId) -> Option<&mut CpuLocal> {
        self.cpu_locals.get_mut(cpu.index())
    }

    // ── IPI scheduling ──

    /// Get IPI data for a CPU.
    pub fn ipi_data(&self, cpu: CpuId) -> &SchedIpiData {
        &self.sched_ipi_data[cpu.index()]
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
    /// C: `smp_sched_handler()` — smp.c:156-187
    ///
    /// Returns the IPI flags and target process that were handled.
    /// Note: This only reads and clears flags. For full semantics
    /// (RTS_SET + FPU save), use `sched_handler_full`.
    pub fn handle_sched_ipi(&self, cpu: CpuId) -> Option<(SchedIpiFlags, ProcNr)> {
        let ipi = &self.sched_ipi_data[cpu.index()];
        let flags = ipi.load_flags();
        if flags.is_empty() {
            return None;
        }
        let target = ipi.get_target();
        // Clear flags after processing
        ipi.clear_flags();
        Some((flags, target))
    }

    /// Full IPI scheduling handler with RTS_SET and FPU save.
    ///
    /// C: smp.c:156-187 — `smp_sched_handler()`
    ///
    /// Reads flags → STOP_PROC sets RTS_PROC_STOP →
    /// SAVE_CTX saves FPU → VM_INHIBIT sets RTS_VMINHIBIT →
    /// clears flags.
    ///
    /// Requires `&mut ProcessTable` for RTS_SET operations.
    pub fn sched_handler_full(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        cpu: CpuId,
    ) {
        let ipi = &self.sched_ipi_data[cpu.index()];
        let flags = ipi.load_flags();
        if flags.is_empty() {
            return;
        }
        let target = ipi.get_target();

        // C: smp.c:167-169 — STOP_PROC
        if flags.contains(SchedIpiFlags::STOP_PROC) {
            proc_table.rts_set(target, RtsFlagsBits::PROC_STOP);
        }

        // C: smp.c:170-179 — SAVE_CTX (FPU save)
        if flags.contains(SchedIpiFlags::SAVE_CTX) {
            let used_fpu = proc_table
                .get(target)
                .map(|p| p.p_misc_flags.get().contains(MiscFlagsBits::EXT_REG_INITIALIZED))
                .unwrap_or(false);
            if used_fpu && self.cpu_locals[cpu.index()].fpu_owner == Some(target) {
                // C: disable_fpu_exception(); save_local_fpu(p, FALSE); release_fpu(p);
                //
                // Save the process's FPU state to its per-process `fpu_state`
                // buffer so it can be restored when the process resumes on
                // the target CPU after migration. Then release FPU ownership.
                //
                // `CurrentFpuArch` is a stateless ZST (selected at compile time
                // via `#[cfg(target_arch)]` in the arch crate), so constructing
                // a default instance is zero-cost. The `FpuArch::save` method
                // issues the architecture-specific save instruction (fxsave on
                // x86-64, stp q0..q31 on aarch64, fsd f0..f31 on riscv64).
                //
                // C: smp.c:173-176 — disable_fpu_exception(); save_local_fpu(p, FALSE);
                //                    release_fpu(p);
                use minix_arch::{CurrentFpuArch, FpuArch};
                let fpu_arch = CurrentFpuArch::default();
                fpu_arch.disable_exception();
                if let Some(p) = proc_table.get_mut(target) {
                    fpu_arch.save(&mut p.fpu_state);
                }
                // release_fpu(p): clear per-CPU fpu_owner so the next FPU
                // access traps (lazy restore on target CPU).
                self.cpu_locals[cpu.index()].fpu_owner = None;
            }
        }

        // C: smp.c:180-182 — VM_INHIBIT
        if flags.contains(SchedIpiFlags::VM_INHIBIT) {
            proc_table.rts_set(target, RtsFlagsBits::VMINHIBIT);
        }

        // C: __insn_barrier() + clear flags — smp.c:185-186
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        ipi.clear_flags();
    }

    /// Synchronous cross-CPU scheduling operation.
    ///
    /// C: smp.c:75-112 — `smp_schedule_sync(p, task)`
    ///
    /// Sets IPI data → sends IPI → releases BKL → waits for completion →
    /// reacquires BKL. Handles reentrant IPI while waiting.
    ///
    /// # Safety contract
    /// - Caller must hold BKL on entry
    /// - `target_cpu` must differ from `current_cpu`
    /// - BKL is released during wait; caller must not hold any other lock
    pub fn schedule_sync<A: SmpArch>(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        target_cpu: CpuId,
        current_cpu: CpuId,
        target_proc: ProcNr,
        task: SchedIpiFlags,
    ) {
        debug_assert!(target_cpu != current_cpu, "schedule_sync: target == current CPU");
        debug_assert!(target_cpu.raw() < self.ncpus, "schedule_sync: target_cpu out of range");

        // Wait if another CPU has a pending request to the same target.
        // C: smp.c:85-95
        if self.sched_ipi_data[target_cpu.index()].has_pending() {
            bkl_unlock();
            while self.sched_ipi_data[target_cpu.index()].has_pending() {
                // Reentrant: handle our own IPI if pending
                if self.sched_ipi_data[current_cpu.index()].has_pending() {
                    // R-05: forget guard — explicit bkl_unlock() below
                    core::mem::forget(bkl_lock());
                    self.sched_handler_full(proc_table, current_cpu);
                    bkl_unlock();
                }
                A::pause();
            }
            // R-05: forget guard — BKL stays held, released later by caller
            core::mem::forget(bkl_lock());
        }

        // Set IPI data and flags
        self.sched_ipi_data[target_cpu.index()].set_target(target_proc);
        self.sched_ipi_data[target_cpu.index()].set_flags(task);
        // C: __insn_barrier() — smp.c:99
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        A::send_sched_ipi(target_cpu.raw());

        // Wait until target CPU finishes
        // C: smp.c:103-111
        bkl_unlock();
        while self.sched_ipi_data[target_cpu.index()].has_pending() {
            if self.sched_ipi_data[current_cpu.index()].has_pending() {
                // R-05: forget guard — explicit bkl_unlock() below
                core::mem::forget(bkl_lock());
                self.sched_handler_full(proc_table, current_cpu);
                bkl_unlock();
            }
            A::pause();
        }
        // R-05: forget guard — BKL stays held, released later by caller
        core::mem::forget(bkl_lock());
    }

    /// Stop a process on a remote CPU.
    /// C: smp.c:114-121 — `smp_schedule_stop_proc(p)`
    pub fn schedule_stop_proc<A: SmpArch>(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        proc_nr: ProcNr,
        current_cpu: CpuId,
    ) {
        let (is_runnable, target_cpu) = proc_table
            .get(proc_nr)
            .map(|p| {
                let cpu = CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire));
                (p.is_runnable(), cpu)
            })
            .unwrap_or((false, CpuId::BSP));

        if is_runnable {
            self.schedule_sync::<A>(proc_table, target_cpu, current_cpu, proc_nr, SchedIpiFlags::STOP_PROC);
        } else {
            proc_table.rts_set(proc_nr, RtsFlagsBits::PROC_STOP);
        }
    }

    /// Set VMINHIBIT on a process on a remote CPU.
    /// C: smp.c:123-130 — `smp_schedule_vminhibit(p)`
    pub fn schedule_vminhibit<A: SmpArch>(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        proc_nr: ProcNr,
        current_cpu: CpuId,
    ) {
        let (is_runnable, target_cpu) = proc_table
            .get(proc_nr)
            .map(|p| {
                let cpu = CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire));
                (p.is_runnable(), cpu)
            })
            .unwrap_or((false, CpuId::BSP));

        if is_runnable {
            self.schedule_sync::<A>(proc_table, target_cpu, current_cpu, proc_nr, SchedIpiFlags::VM_INHIBIT);
        } else {
            proc_table.rts_set(proc_nr, RtsFlagsBits::VMINHIBIT);
        }
    }

    /// Stop a process and save its full context (for migration).
    /// C: smp.c:132-140 — `smp_schedule_stop_proc_save_ctx(p)`
    pub fn schedule_stop_proc_save_ctx<A: SmpArch>(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        proc_nr: ProcNr,
        current_cpu: CpuId,
    ) {
        let target_cpu = proc_table
            .get(proc_nr)
            .map(|p| CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire)))
            .unwrap_or(CpuId::BSP);

        self.schedule_sync::<A>(
            proc_table, target_cpu, current_cpu, proc_nr,
            SchedIpiFlags::STOP_PROC | SchedIpiFlags::SAVE_CTX,
        );
    }

    /// Migrate a process to a different CPU.
    /// C: smp.c:142-154 — `smp_schedule_migrate_proc(p, dest_cpu)`
    pub fn schedule_migrate_proc<A: SmpArch>(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        proc_nr: ProcNr,
        current_cpu: CpuId,
        dest_cpu: CpuId,
    ) {
        self.schedule_stop_proc_save_ctx::<A>(proc_table, proc_nr, current_cpu);
        // C: p->p_cpu = dest_cpu; RTS_UNSET(p, RTS_PROC_STOP);
        if let Some(p) = proc_table.get_mut(proc_nr) {
            p.p_sched.cpu.store(dest_cpu.raw(), Ordering::Release);
        }
        proc_table.rts_unset(proc_nr, RtsFlagsBits::PROC_STOP);
    }

    /// IPI schedule handler: ack + preempt current process.
    /// C: smp.c:194-204 — `smp_ipi_sched_handler()`
    pub fn ipi_sched_handler<A: SmpArch>(
        &mut self,
        proc_table: &mut crate::proc_table::ProcessTable,
        current_cpu: CpuId,
    ) {
        A::ack_ipi();
        let curr = self.cpu_locals[current_cpu.index()].proc_ptr;
        if let Some(curr_nr) = curr
            && curr_nr != proc_nr::IDLE {
                proc_table.rts_set(curr_nr, RtsFlagsBits::PREEMPTED);
            }
    }

    /// IPI halt handler: ack + stop local timer + halt CPU.
    /// C: smp.c:56-61 — `smp_ipi_halt_handler()`
    ///
    /// Stops the per-CPU local timer before halting to prevent timer
    /// interrupts during the halt. Delegates to `clock::stop_local_timer()`,
    /// which constructs a transient `CurrentClockArch` instance (same pattern
    /// as `clock::read_tsc()`).
    pub fn ipi_halt_handler<A: SmpArch>(&self) {
        A::ack_ipi();
        crate::clock::stop_local_timer();
        A::halt_cpu();
    }

    /// BSP waits for all APs to finish booting.
    /// C: smp.c:30-49 — `wait_for_APs_to_finish_booting()`
    ///
    /// Releases BKL → waits for `ap_cpus_booted == ncpus - 1` →
    /// reacquires BKL. Tolerates partial AP boot failure.
    pub fn wait_for_aps<A: SmpArch>(&self) {
        // Count ready CPUs (tolerate partial failure)
        // C: smp.c:36-41
        let n = self.cpus.iter().filter(|c| c.is_ready()).count() as u32;
        if n != self.ncpus {
            // C: printf("WARNING: only %d out of %d cpus booted\n", n, ncpus)
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
            Console::write_str("WARNING: not all CPUs booted\n");
        }

        // Release BKL so APs can enter kernel
        // C: smp.c:44
        bkl_unlock();

        // Wait for APs
        // C: smp.c:45-46
        let expected = n.saturating_sub(1);
        while self.ap_cpus_booted.load(Ordering::Acquire) != expected {
            A::pause();
        }

        // Reacquire BKL
        // C: smp.c:48
        // R-05: forget guard — BKL stays held, released later by caller
        core::mem::forget(bkl_lock());
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
/// # Design (16-smp.md D1)
///
/// `AtomicBool` + CAS spinlock. C uses `SPINLOCK_DEFINE(big_kernel_lock)`
/// (`smp.c:27`); this is the Rust equivalent. Avoids the `spin` crate
/// to keep the kernel `no_std` and dependency-free.
///
/// # Wiring status
///
/// BKL is wired into kernel entry/exit points (syscall dispatch, exception
/// handler, kmain, switch_to_user). Pending integration: timer tick
/// handler, IPC sendrecv paths, per-CPU run queue cross-CPU access.
/// Single-CPU builds are unaffected: `bkl_lock` CAS succeeds on first
/// try (no contention), spin loop body never executes.
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

/// Guard returned by [`bkl_lock`]. RAII: dropping releases the BKL.
///
/// R-05 (2026-08-12): `BklGuard` is now RAII by default — `Drop` calls
/// `bkl_unlock()`. This eliminates the "forgot to call `bkl_unlock()`"
/// class of deadlocks. For code paths that need to keep the BKL held
/// after the guard's scope (e.g. `kernel_call_dispatch` →
/// `kernel_call_finish`), use `core::mem::forget(guard)`. For explicit
/// early release, use [`BklGuard::release`].
///
/// D6 update: The old design had two types — `BklGuard` (non-RAII) and
/// `BklGuardRaii` (RAII). R-05 unifies them into a single RAII type.
/// Cross-function BKL transfer uses `mem::forget`; blocking-IPC release/
/// reacquire uses `release()`/`bkl_lock()`.
///
/// C: `BKL_LOCK()` / `BKL_UNLOCK()` — smp.c:27, spinlock.h
pub struct BklGuard {
    /// If `true`, `Drop` will call `bkl_unlock()`. Set to `false` by
    /// [`release`](Self::release) to prevent double-unlock.
    active: bool,
}

impl BklGuard {
    /// Obtain a BKL section witness that borrows this guard.
    ///
    /// The witness can be passed to `proc_table_with()` etc. to prove
    /// BKL ownership at compile time (R-03 capability token pattern).
    /// The witness borrows this guard, so it cannot outlive the guard.
    pub fn section(&self) -> BklSection<'_> {
        BklSection { _lifetime: core::marker::PhantomData }
    }

    /// Explicitly release the BKL early, consuming the guard.
    ///
    /// Equivalent to `drop(guard)` but more explicit at call sites where
    /// the release point is significant (e.g. before blocking IPC).
    /// After `release()`, `Drop` will NOT call `bkl_unlock()` again.
    pub fn release(mut self) {
        self.active = false;
        bkl_unlock();
    }
}

impl Drop for BklGuard {
    fn drop(&mut self) {
        if self.active {
            bkl_unlock();
        }
    }
}

// ── BklSection: typed witness for BKL-protected access ──
//
// D4: Capability pattern — type-level proof of BKL ownership.
// The BKL framework (BklGuard + bkl_lock/unlock) provides the runtime
// primitive, but the type system does not enforce that a shared-state
// accessor is called *only* while the BKL is held. Without a type-level
// witness, callers can `bkl_lock()` / `bkl_unlock()` and then proceed to
// touch `SmpState` after unlock.
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
    // Acquire the BKL. R-05: BklGuard is now RAII (Drop releases BKL),
    // so we must forget the guard to keep the BKL held. The caller must
    // call bkl_unlock() explicitly. The BklSection witness proves to the
    // type system that the BKL was acquired.
    let guard = bkl_lock();
    core::mem::forget(guard);
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
/// CPU.
///
/// R-05 (2026-08-12): The returned [`BklGuard`] is RAII — `Drop` calls
/// [`bkl_unlock`]. For most critical sections, simply let the guard go
/// out of scope. For cross-function BKL transfer (e.g. `kernel_call_dispatch`
/// → `kernel_call_finish`), use `core::mem::forget(guard)` to keep the BKL
/// held and call `bkl_unlock()` explicitly at the release point. For explicit
/// early release (e.g. before blocking IPC), use [`BklGuard::release`].
///
/// # Safety contract
///
/// The caller is responsible for:
/// 1. **Not holding the BKL already** on this CPU (re-entry would
///    deadlock — Minix3 BKL is non-recursive).
/// 2. **Not sleeping, scheduling, or waiting for IPC** while the BKL
///    is held (BKL is a spinlock; sleep inside a spinlock is
///    deadlock).
/// 3. **Ensuring exactly one release per `bkl_lock()`** — either via
///    `Drop` (RAII), `mem::forget` + explicit `bkl_unlock()`, or
///    `BklGuard::release()`. Do NOT mix explicit `bkl_unlock()` with
///    RAII drop on the same guard (double unlock).
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
    BklGuard { active: true }
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

// R-05: bkl_lock_raii() and BklGuardRaii have been removed.
// BklGuard is now RAII (Drop releases BKL). Use bkl_lock() for all
// critical sections. For cross-function BKL transfer, use mem::forget(guard).
// For explicit early release, use guard.release().

/// Diagnostic helper: query whether the BKL is currently held.
/// **Never** use this for control flow; it is for diagnostic, debug
/// assertion, and test code only (the value can change immediately
/// after the load).
///
/// Available in `test` and `debug_assertions` builds. In release builds
/// without `debug_assertions`, this function is not available — the BKL
/// witness (`BklSection`) should be used for compile-time proof instead.
#[cfg(any(test, debug_assertions))]
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
        assert_eq!(smp.bsp_cpu_id(), CpuId::BSP);
        assert!(smp.cpu_is_bsp(CpuId::BSP));
        assert!(smp.cpu_is_ready(CpuId::BSP));
        assert!(!smp.cpu_is_bsp(CpuId::new_unchecked(1)));
    }

    #[test]
    fn test_smp_state_multi_cpu() {
        let smp = SmpState::with_ncpus(4, CpuId::BSP);
        assert_eq!(smp.ncpus(), 4);
        assert!(smp.cpu_is_bsp(CpuId::BSP));
        assert!(!smp.cpu_is_bsp(CpuId::new_unchecked(1)));
    }

    #[test]
    fn test_cpu_flags() {
        let mut smp = SmpState::new_single_cpu();
        smp.cpu_set_flag(CpuId::new_unchecked(1), CpuFlags::READY);
        assert!(smp.cpu_is_ready(CpuId::new_unchecked(1)));
        smp.cpu_clear_flag(CpuId::new_unchecked(1), CpuFlags::READY);
        assert!(!smp.cpu_is_ready(CpuId::new_unchecked(1)));
    }

    #[test]
    fn test_cpu_local() {
        let mut smp = SmpState::new_single_cpu();
        let local = smp.cpu_local_mut(CpuId::BSP).unwrap();
        assert!(local.proc_ptr.is_none());
        local.proc_ptr = Some(ProcNr(5));
        assert_eq!(smp.cpu_local(CpuId::BSP).unwrap().proc_ptr, Some(ProcNr(5)));
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
        local.set_running(ProcNr(7));
        assert_eq!(local.proc_ptr, Some(ProcNr(7)));
        assert_eq!(local.bill_ptr, Some(ProcNr(7)));
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
        let smp = SmpState::with_ncpus(4, CpuId::BSP);
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
        let ipi = smp.ipi_data(CpuId::BSP);

        ipi.set_flags(SchedIpiFlags::STOP_PROC);
        ipi.set_target(ProcNr(5));

        let result = smp.handle_sched_ipi(CpuId::BSP);
        assert!(result.is_some());
        let (flags, target) = result.unwrap();
        assert!(flags.contains(SchedIpiFlags::STOP_PROC));
        assert_eq!(target, ProcNr(5));

        // Should be cleared after handling
        assert!(!smp.ipi_data(CpuId::BSP).has_pending());
    }

    #[test]
    fn test_handle_sched_ipi_empty() {
        let smp = SmpState::new_single_cpu();
        let result = smp.handle_sched_ipi(CpuId::BSP);
        assert!(result.is_none());
    }

    // ── BKL tests (D1 framework) ──
    //
    // BKL tests share the global `BKL_LOCKED` AtomicBool and must be
    // serialized to prevent parallel `cargo test` races. We use a
    // simple spinlock (`BKL_TEST_LOCK`) acquired in setup and released
    // in teardown — same pattern as `SPROF_TEST_LOCK` in misc.rs.

    static BKL_TEST_LOCK: AtomicBool = AtomicBool::new(false);

    fn bkl_test_setup() -> bool {
        while BKL_TEST_LOCK
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            core::hint::spin_loop();
        }
        // Ensure BKL starts unlocked (clean up after a panicked test).
        BKL_LOCKED.store(false, Ordering::Release);
        true
    }

    fn bkl_test_teardown() {
        BKL_LOCKED.store(false, Ordering::Release);
        BKL_TEST_LOCK.store(false, Ordering::Release);
    }

    #[test]
    fn test_bkl_lock_unlock() {
        let _lock = bkl_test_setup();
        // Single-CPU build: BKL is initially free, lock + unlock round-trip
        // must leave it free again.
        // R-05: forget guard since we unlock explicitly.
        assert!(!bkl_is_locked());
        core::mem::forget(bkl_lock());
        assert!(bkl_is_locked());
        bkl_unlock();
        assert!(!bkl_is_locked());
        bkl_test_teardown();
    }

    // R-05: test_bkl_guard_raii_releases_on_drop was removed — it duplicated
    // test_bkl_guard_releases_on_drop (both test RAII Drop releases BKL).
    // The explicit `drop(_g)` variant added no coverage over scope-based drop.

    #[test]
    fn test_bkl_reentrant_is_caller_responsibility() {
        let _lock = bkl_test_setup();
        // Minix3 BKL is non-recursive. This test documents the contract:
        // a second bkl_lock() on the same CPU while the lock is held will
        // deadlock on a multi-CPU build. On a single-CPU build the
        // spin loop is a no-op, so the test simply verifies the BKL
        // *can* be re-acquired after explicit unlock (re-entry protocol
        // is the caller's responsibility — see SAFETY contract).
        // R-05: forget guards since we unlock explicitly.
        core::mem::forget(bkl_lock());
        bkl_unlock();
        // After explicit unlock, a fresh lock must succeed.
        core::mem::forget(bkl_lock());
        bkl_unlock();
        bkl_test_teardown();
    }

    // ── BklSection typed witness tests ──

    #[test]
    fn test_bkl_section_provides_typed_access() {
        let _lock = bkl_test_setup();
        // Open a section and access SmpState through the typed witness.
        // The witness is the proof that the BKL is held; the lifetime
        // is the enforcement that the access cannot outlive the section.
        let section = bkl_lock_section();
        let smp = SmpState::new_single_cpu();
        let accessed = smp_state_with(&section, &smp);
        assert_eq!(accessed.ncpus(), 1);
        // Section is non-RAII (D6): explicit unlock is required.
        bkl_unlock();
        bkl_test_teardown();
    }

    #[test]
    fn test_bkl_section_paired_unlock() {
        let _lock = bkl_test_setup();
        // The pattern: open section → access → unlock. Cannot access
        // SmpState after unlock (no BklSection is alive).
        let _section = bkl_lock_section();
        // ... do work with `_section` witness ...
        bkl_unlock();
        // After unlock, BKL is free.
        assert!(!bkl_is_locked());
        bkl_test_teardown();
    }

    #[test]
    fn test_bkl_section_drop_does_not_unlock() {
        let _lock = bkl_test_setup();
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
        bkl_test_teardown();
    }

    // ── BklGuard::section() + *_with() accessor tests (R-03) ──

    #[test]
    fn test_bkl_guard_section_enables_with_accessors() {
        let _lock = bkl_test_setup();
        // R-03: BklGuard::section() produces a BklSection witness that
        // can be passed to proc_table_with() etc. for compile-time BKL proof.
        let guard = bkl_lock();
        assert!(bkl_is_locked());
        {
            let section = guard.section();
            // The section borrows the guard; both are alive.
            // proc_table_with(&section) would prove BKL ownership at compile time.
            // (Cannot call proc_table_with here because PROC_TABLE may not be
            // initialized in this test, but the type system proof is the point.)
            let _ = &section; // use the section
        } // section dropped here, guard no longer borrowed
        // R-05: BklGuard is RAII — use release() for explicit early release
        // (equivalent to drop(guard) but more explicit at the release point).
        guard.release();
        assert!(!bkl_is_locked());
        bkl_test_teardown();
    }

    #[test]
    fn test_bkl_guard_section_lifetime_tied_to_guard() {
        let _lock = bkl_test_setup();
        // The BklSection borrows the BklGuard, so it cannot outlive it.
        // This test verifies the section is usable while the guard is alive.
        let guard = bkl_lock();
        {
            let _section = guard.section();
            // section is alive here, guard is alive here
            assert!(bkl_is_locked());
            // section dropped here, but guard is still alive
        }
        // guard is still alive, BKL still held
        assert!(bkl_is_locked());
        // R-05: release() consumes the guard and releases the BKL.
        guard.release();
        bkl_test_teardown();
    }

    // ── BklGuard RAII tests (R-05: BklGuard is now RAII) ──

    #[test]
    fn test_bkl_guard_releases_on_drop() {
        let _lock = bkl_test_setup();
        // R-05: BklGuard is now RAII — Drop releases BKL automatically.
        assert!(!bkl_is_locked());
        {
            let _guard = bkl_lock();
            assert!(bkl_is_locked());
        } // _guard dropped here — BKL released.
        assert!(!bkl_is_locked());
        bkl_test_teardown();
    }

    #[test]
    fn test_bkl_guard_nested_release() {
        let _lock = bkl_test_setup();
        // Verify RAII + explicit unlock can coexist:
        // explicit unlock → RAII reacquire → RAII release.
        // R-05: forget the first guard since we unlock explicitly.
        core::mem::forget(bkl_lock());
        bkl_unlock();
        assert!(!bkl_is_locked());
        {
            let _raii = bkl_lock();
            assert!(bkl_is_locked());
        }
        assert!(!bkl_is_locked());
        bkl_test_teardown();
    }

    #[test]
    fn test_bkl_guard_release_method() {
        let _lock = bkl_test_setup();
        // R-05: guard.release() explicitly releases BKL and consumes the guard.
        let guard = bkl_lock();
        assert!(bkl_is_locked());
        guard.release();
        assert!(!bkl_is_locked());
        bkl_test_teardown();
    }

    // ── SmpArch trait + MockSmpArch tests ──
    //
    // Use the arch crate's MockSmpArch (available via `feature = "mock"`
    // in dev-dependencies) instead of a local mock — single source of truth.

    use minix_arch::arch::smp::MockSmpArch;

    #[test]
    fn test_smp_arch_trait_mock() {
        // Verify SmpArch trait can be implemented and called.
        MockSmpArch::send_sched_ipi(1);
        MockSmpArch::ack_ipi();
        MockSmpArch::halt_cpu();
        MockSmpArch::boot_ap(1, 0x1000);
        // Default pause() method should work.
        MockSmpArch::pause();
    }

    #[test]
    fn test_sched_handler_full_empty() {
        // sched_handler_full with no pending IPI should be a no-op.
        let mut smp = SmpState::new_single_cpu();
        let mut pt = crate::proc_table::ProcessTable::new();
        // No IPI flags set — should return without modifying anything.
        smp.sched_handler_full(&mut pt, CpuId::BSP);
        assert!(!smp.ipi_data(CpuId::BSP).has_pending());
    }

    #[test]
    fn test_sched_handler_full_stop_proc() {
        // sched_handler_full with STOP_PROC should set RTS_PROC_STOP.
        let mut smp = SmpState::new_single_cpu();
        let mut pt = crate::proc_table::ProcessTable::new();

        // Set IPI flags for STOP_PROC targeting process slot 0.
        smp.ipi_data(CpuId::BSP).set_flags(SchedIpiFlags::STOP_PROC);
        smp.ipi_data(CpuId::BSP).set_target(ProcNr(0));

        smp.sched_handler_full(&mut pt, CpuId::BSP);

        // Flags should be cleared after handling.
        assert!(!smp.ipi_data(CpuId::BSP).has_pending());
        // Process 0 should have RTS_PROC_STOP set.
        let proc = pt.get(ProcNr(0)).expect("process slot 0 must exist");
        assert!(proc.p_rts_flags.get().contains(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_sched_handler_full_vminhibit() {
        // sched_handler_full with VM_INHIBIT should set RTS_VMINHIBIT.
        let mut smp = SmpState::new_single_cpu();
        let mut pt = crate::proc_table::ProcessTable::new();

        smp.ipi_data(CpuId::BSP).set_flags(SchedIpiFlags::VM_INHIBIT);
        smp.ipi_data(CpuId::BSP).set_target(ProcNr(0));

        smp.sched_handler_full(&mut pt, CpuId::BSP);

        assert!(!smp.ipi_data(CpuId::BSP).has_pending());
        let proc = pt.get(ProcNr(0)).expect("process slot 0 must exist");
        assert!(proc.p_rts_flags.get().contains(RtsFlagsBits::VMINHIBIT));
    }

    #[test]
    fn test_ipi_sched_handler_idle_no_preempt() {
        // ipi_sched_handler with IDLE as current process should NOT
        // set RTS_PREEMPTED.
        let mut smp = SmpState::new_single_cpu();
        let mut pt = crate::proc_table::ProcessTable::new();

        // Set current CPU's proc_ptr to IDLE.
        smp.cpu_local_mut(CpuId::BSP).unwrap().proc_ptr = Some(proc_nr::IDLE);

        smp.ipi_sched_handler::<MockSmpArch>(&mut pt, CpuId::BSP);

        // IDLE process should NOT have RTS_PREEMPTED set.
        if let Some(idle_proc) = pt.get(proc_nr::IDLE) {
            assert!(!idle_proc.p_rts_flags.get().contains(RtsFlagsBits::PREEMPTED));
        }
    }

    #[test]
    fn test_wait_for_aps_single_cpu() {
        // Single-CPU config: wait_for_APs should return immediately
        // (expected = 0, ap_cpus_booted = 0).
        let smp = SmpState::new_single_cpu();
        // Acquire BKL first (wait_for_APs releases and reacquires).
        // R-05: BklGuard is RAII — guard.release() at end releases BKL.
        let guard = bkl_lock();
        smp.wait_for_aps::<MockSmpArch>();
        // BKL should be reacquired after wait.
        assert!(bkl_is_locked());
        guard.release();
    }
}
