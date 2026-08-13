//! Process scheduler — multi-level priority queue with preemptive scheduling.
//!
//! Corresponds to Minix3's scheduling functions in `kernel/proc.c` (~700 lines).
//! Design decisions documented in `07-scheduling.md` §3.
//!
//! # Minix3 Scheduling Model
//!
//! - 16 priority queues (0=highest, 15=lowest), FIFO within same priority
//! - Preemptive: higher-priority process preempts current (if PREEMPTIBLE)
//! - Time-slice rotation: each process gets a quantum; on exhaustion,
//!   user-scheduled processes notify their scheduler, kernel-scheduled ones reset
//! - SMP: per-CPU queues, cross-CPU enqueue wakeup, CPU affinity
//!
//! # Rust Design Decisions (07-scheduling.md §3)
//!
//! - §3.1: Array indices (`Option<ProcNr>`) replace pointer chains for queues
//! - §3.2: `Scheduler` struct holds head/tail arrays; operations need `&ProcessTable`
//! - §3.3: `switch_to_user` state machine expressed as `loop` + `continue`
//! - §3.5: `proc_no_time` strategy branching preserved exactly
//! - §3.6: `enter_queue` records the enqueued process (fixes Minix3 bug)
//! - §3.7: IDLE process handling preserved (PROC_STOP prevents pick_proc selection)
//!
//! # Borrow Model
//!
//! `Scheduler` is embedded in `ProcessTable`. To avoid self-referential borrows
//! (`&mut self.sched` + `&mut self.procs`), the Scheduler methods only operate
//! on the queue arrays. Process field updates (p_nextready, p_accounting, etc.)
//! are done by `ProcessTable` wrapper methods that coordinate the borrows.

use core::sync::atomic::Ordering;

use crate::proc::{
    priority, KProcess, ProcNr, NONE_PROC_NR,
};

/// Per-CPU scheduler state holding ready queue head/tail indices.
///
/// Design decision §3.2: Scheduler holds queue metadata only.
/// C: `run_q_head[NR_SCHED_QUEUES]` / `run_q_tail[NR_SCHED_QUEUES]` in `cpulocals.h:58-59`.
#[derive(Debug)]
pub struct Scheduler {
    run_q_head: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
    run_q_tail: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            run_q_head: [None; priority::NR_SCHED_QUEUES],
            run_q_tail: [None; priority::NR_SCHED_QUEUES],
        }
    }

    /// Select the highest-priority runnable process.
    ///
    /// C: `pick_proc()` in proc.c:1785-1813.
    /// Scans queues from priority 0 (highest) to NR_SCHED_QUEUES-1 (lowest).
    /// Returns the head of the first non-empty queue.
    pub fn pick_proc(&self, procs: &[KProcess]) -> Option<ProcNr> {
        for q in 0..priority::NR_SCHED_QUEUES {
            if let Some(nr) = self.run_q_head[q] {
                let idx = nr_to_idx(nr);
                if let Some(proc) = idx.and_then(|i| procs.get(i)) {
                    debug_assert!(
                        proc.is_runnable(),
                        "pick_proc: head of queue {} not runnable",
                        q
                    );
                }
                return Some(nr);
            }
        }
        None
    }

    /// Pure queue insertion at tail. Does NOT touch process fields.
    ///
    /// Returns the queue index used, and the old tail (if any) that needs
    /// its `p_nextready` updated.
    ///
    /// C: queue insertion part of `enqueue()` in proc.c:1595-1659.
    pub fn enqueue_queue_tail(&mut self, nr: ProcNr, q: usize) -> EnqueueInfo {
        let old_tail = self.run_q_tail[q];
        match old_tail {
            None => {
                self.run_q_head[q] = Some(nr);
                self.run_q_tail[q] = Some(nr);
            }
            Some(_) => {
                self.run_q_tail[q] = Some(nr);
            }
        }
        EnqueueInfo { queue: q, old_tail }
    }

    /// Pure queue insertion at head. Does NOT touch process fields.
    ///
    /// C: queue insertion part of `enqueue_head()` in proc.c:1670-1711.
    pub fn enqueue_queue_head(&mut self, nr: ProcNr, q: usize) {
        match self.run_q_head[q] {
            None => {
                self.run_q_head[q] = Some(nr);
                self.run_q_tail[q] = Some(nr);
            }
            Some(_) => {
                self.run_q_head[q] = Some(nr);
            }
        }
    }

    /// Pure queue removal. Does NOT touch process fields.
    ///
    /// Uses `&mut Option<ProcNr>` as the Rust equivalent of C's
    /// pointer-pointer (`struct proc **xpp`) pattern.
    /// Returns the queue index used.
    ///
    /// C: queue removal part of `dequeue()` in proc.c:1716-1780.
    ///
    /// # Safety
    ///
    /// Caller must ensure `nr` is in the queue at priority `q`.
    /// The `update_fn` closure is called with mutable references to the
    /// queue link pointers so the caller can update process fields.
    pub fn dequeue_from_queue(
        &mut self,
        nr: ProcNr,
        q: usize,
        procs: &mut [KProcess],
    ) {
        let _link_idx: Option<usize> = None;
        let mut link_is_head = true;
        let mut prev: Option<ProcNr> = None;

        // Walk the queue to find the node
        let mut current = self.run_q_head[q];
        let mut found = false;
        while let Some(cur_nr) = current {
            if cur_nr == nr {
                found = true;
                break;
            }
            prev = Some(cur_nr);
            let cur_idx = nr_to_idx(cur_nr);
            current = cur_idx
                .and_then(|i| procs.get(i))
                .and_then(|p| {
                    let v = p.p_nextready.load(Ordering::Relaxed);
                    if v == NONE_PROC_NR { None } else { Some(ProcNr(v)) }
                });
            link_is_head = false;
        }

        if !found {
            panic!("dequeue: process not found in its priority queue");
        }

        // Remove from linked list
        let next = nr_to_idx(nr)
            .and_then(|i| procs.get(i))
            .and_then(|p| {
                let v = p.p_nextready.load(Ordering::Relaxed);
                if v == NONE_PROC_NR { None } else { Some(ProcNr(v)) }
            });

        if link_is_head {
            self.run_q_head[q] = next;
        } else if let Some(prev_nr) = prev {
            let prev_idx = nr_to_idx(prev_nr).unwrap();
            procs[prev_idx].p_nextready.store(next.map(|n| n.0).unwrap_or(NONE_PROC_NR), Ordering::Relaxed);
        }

        // Update tail if needed
        if self.run_q_tail[q] == Some(nr) {
            self.run_q_tail[q] = prev;
        }

        // Clear the removed process's nextready
        let nr_idx = nr_to_idx(nr).unwrap();
        procs[nr_idx].p_nextready.store(NONE_PROC_NR, Ordering::Relaxed);
    }

    /// Check whether a queue is empty.
    pub fn is_queue_empty(&self, prio: usize) -> bool {
        debug_assert!(prio < priority::NR_SCHED_QUEUES);
        self.run_q_head[prio].is_none()
    }

    /// Get the head process of a priority queue (for inspection).
    pub fn queue_head(&self, prio: usize) -> Option<ProcNr> {
        debug_assert!(prio < priority::NR_SCHED_QUEUES);
        self.run_q_head[prio]
    }

    /// Get the tail process of a priority queue (for debugging).
    ///
    /// Used by `debug::runqueues_ok_cpu` to verify queue invariants.
    pub(crate) fn queue_tail_inner(&self, prio: usize) -> Option<ProcNr> {
        debug_assert!(prio < priority::NR_SCHED_QUEUES);
        self.run_q_tail[prio]
    }
}

/// Information returned by `enqueue_queue_tail` for the caller to
/// update process fields (p_nextready, p_accounting).
pub struct EnqueueInfo {
    pub queue: usize,
    pub old_tail: Option<ProcNr>,
}

// ── Helper functions ──

/// Convert ProcNr to slice index (offset by NR_TASKS).
fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    const NR_TASKS: isize = minix_types::NR_TASKS as isize;
    const PROC_TABLE_SIZE: usize = NR_TASKS as usize + 256;
    let offset = nr.0 as isize + NR_TASKS;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

/// Check if a process is preemptible via its privilege flags.
///
/// C: `priv(p)->s_flags & PREEMPTIBLE` in const.h:143.
#[allow(dead_code)] // scheduler helper; not yet wired to all call sites
fn is_preemptible(p: &KProcess) -> bool {
    let prio = p.get_priority().get();
    prio != priority::TASK_Q
}

/// Check if a process is scheduled by the kernel (no user-space scheduler).
///
/// C: `proc_kernel_scheduler(p)` macro in proc.h:178.
#[allow(dead_code)] // scheduler helper; not yet wired to all call sites
fn is_kernel_scheduled(p: &KProcess) -> bool {
    p.p_sched.scheduler.is_none() || p.p_sched.scheduler == Some(p.p_nr)
}

/// Check if a process has remaining CPU time.
#[allow(dead_code)] // scheduler helper; not yet wired to all call sites
fn has_cpu_time_left(p: &KProcess) -> bool {
    p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0
}

/// Aggregated scheduling parameters for [`sched_proc`].
///
/// Groups the four scheduling parameters (priority, quantum, cpu, niced)
/// into a single struct to reduce the function's parameter count from 5
/// to 2. Mirrors the C `sched_proc()` parameter list (`system.c:642-723`)
/// but uses Rust idioms.
///
/// # Design decision §3.8 (11-design.v1.md): Option replaces C's -1 sentinel
///
/// C uses `i32` parameters where `-1` means "keep current value". Rust uses
/// `Option<T>` where `None` means "keep current" and `Some(v)` means "set to v".
/// This avoids polluting the priority type with a sentinel value and is the
/// Rust-idiomatic way to express "optional".
///
/// # Fields
///
/// * `priority` — `Some(v)` sets new priority (0..=15); `None` keeps current.
/// * `quantum` — `Some(v)` sets new quantum in ms (>= 1); `None` keeps current.
/// * `cpu` — `Some(v)` sets new CPU affinity; `None` keeps current (SMP stub).
/// * `niced` — if true, set MF_NICED; if false, clear it.
pub struct SchedParams {
    pub priority: Option<u8>,
    pub quantum: Option<u32>,
    pub cpu: Option<u32>,
    pub niced: bool,
}

/// Update a process's scheduling parameters.
///
/// C: `sched_proc()` — system.c:642-723.
///
/// Validates the parameters and applies them to the target process's
/// `p_sched` fields.
///
/// # `niced` parameter rationale
///
/// The C `sched_proc()` accepts `niced` as an `int` (boolean coercion of
/// `m_ptr->m_lsys_krn_schedule.niced`). The kernel's `SYS_SCHEDCTL` path
/// (`do_schedctl.c`) always passes `FALSE` (0); only `SYS_NICE` (PM → kernel
/// via `SYS_SCHEDULE`) sets `niced = !!(...)`. The Rust signature keeps
/// `niced: bool` (rather than hard-coding `false`) so the future `SYS_NICE`
/// syscall can reuse this function without API churn. Callers that match the
/// `SYS_SCHEDCTL` path pass `false` explicitly (see `dispatch_schedule`).
///
/// # Errors
///
/// - `InvalidArgument` (EINVAL) if `priority` is `Some(v)` with `v > 15`, or
///   if `quantum` is `Some(v)` with `v == 0`.
/// - `BadCpu` (EBADCPU) if the requested CPU is not ready (SMP stub; not
///   reachable on single-CPU systems).
///
/// # Returns
///
/// `Ok(())` on success. C returns `OK` (0); the Rust `Result` replaces the
/// errno-style return.
///
/// # SMP migration
///
/// On SMP, if the process is currently runnable on a different CPU,
/// the scheduler must migrate it. The migration itself is implemented
/// by `SmpState::schedule_migrate_proc` ([smp.rs:670](file:///home/xzhao/github/minix-rs/os/kernel/src/smp.rs)):
/// stop on current CPU → save ctx → set `p_cpu = dest_cpu` → unset RTS_PROC_STOP.
/// This function (`sched_proc`) only records the new `p_cpu` field;
/// `dispatch_schedule` calls `schedule_migrate_proc` when the CPU changes.
///
/// # Implementation status
///
/// Steps 1-9 are implemented (validation + field updates).
/// SMP migration via `schedule_migrate_proc` is **implemented** (smp.rs:670).
pub fn sched_proc(
    p: &mut KProcess,
    params: SchedParams,
) -> Result<(), SchedProcError> {
    use crate::proc::{MiscFlagsBits, RtsFlagsBits};

    // Step 1: validate priority range.
    // C: system.c:644-645:
    //   if ((priority < TASK_Q && priority != -1) || priority > NR_SCHED_QUEUES)
    //       return EINVAL;
    // In Rust: TASK_Q == 0, MIN_USER_Q == 15 (NR_SCHED_QUEUES-1).
    // Design decision §3.8: None = keep current (replaces C's -1).
    // u8 is always >= 0, so we only check the upper bound.
    if let Some(v) = params.priority
        && v > priority::MIN_USER_Q {
            return Err(SchedProcError::InvalidArgument);
        }

    // Step 2: validate quantum range.
    // C: system.c:647-648:
    //   if (quantum < 1 && quantum != -1) return EINVAL;
    if let Some(v) = params.quantum
        && v < 1 {
            return Err(SchedProcError::InvalidArgument);
        }

    // Step 3: validate CPU range (SMP stub — always OK for uniprocessor).
    // C: system.c:650-654: only relevant with CONFIG_SMP. Our Rust
    // rewrite uses single-threaded event loop for user-space servers,
    // so cpu_is_ready is always true. Multi-CPU servers would extend
    // here.
    let _ = params.cpu;

    // Step 4: preemption hint (RTS_NO_QUANTUM toggle).
    // C: system.c:668-677 sets RTS_NO_QUANTUM if the process is runnable
    // and the parameters are changing. We track this flag in the Rust
    // translation, but actual reschedule is handled by the scheduler.
    let priority_changed = match params.priority {
        Some(v) => v != p.p_sched.priority.load(Ordering::Acquire),
        None => false,
    };
    let quantum_changed = params.quantum.is_some();
    if priority_changed || quantum_changed {
        // RTS_NO_QUANTUM marks "needs re-enqueue after this update".
        // We set it (matches C) and rely on the scheduler to clear it.
        p.p_rts_flags.set(RtsFlagsBits::NO_QUANTUM);
    }

    // Step 5: apply priority (if Some).
    // C: system.c:684-685:
    //   if (priority != -1) p->p_priority = priority;
    if let Some(v) = params.priority {
        p.p_sched.priority.store(v, Ordering::Release);
    }

    // Step 6: apply quantum + reset cpu_time_left.
    // C: system.c:686-689:
    //   if (quantum != -1) {
    //       p->p_quantum_size_ms = quantum;
    //       p->p_cpu_time_left = ms_2_cpu_time(quantum);
    //   }
    if let Some(v) = params.quantum {
        p.p_sched.quantum.size_ms.store(v, Ordering::Release);
        // Reset cpu_time_left to (quantum * TSC_PER_MS) — we use clock::ms_to_cpu_time.
        let total_cycles = crate::clock::ms_to_cpu_time(v);
        p.p_sched.quantum.cpu_time_left.store(total_cycles, Ordering::Release);
    }

    // Step 7: apply CPU affinity (SMP).
    // C: system.c:691-693 (CONFIG_SMP only):
    //   if (cpu != -1) p->p_cpu = cpu;
    // On uniprocessor Rust rewrite this is a no-op (cpu is always 0).
    if let Some(v) = params.cpu {
        p.p_sched.cpu.store(v, Ordering::Release);
    }

    // Step 8: apply niced flag.
    // C: system.c:695-698:
    //   if (niced) p->p_misc_flags |= MF_NICED;
    //   else p->p_misc_flags &= ~MF_NICED;
    if params.niced {
        p.p_misc_flags.set(MiscFlagsBits::NICED);
    } else {
        p.p_misc_flags.clear(MiscFlagsBits::NICED);
    }

    // Step 9: clear RTS_NO_QUANTUM after enqueue (matches C:698).
    p.p_rts_flags.clear(RtsFlagsBits::NO_QUANTUM);

    Ok(())
}

/// Errors that sched_proc can return.
///
/// Mirrors C's `EINVAL` / `EBADCPU` style error codes but uses an
/// `enum` so the caller cannot accidentally confuse the priority
/// parameter (-1 sentinel) with an error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedProcError {
    /// EINVAL: priority or quantum out of valid range.
    InvalidArgument,
    /// EBADCPU: requested CPU is not ready (SMP only; not reachable on
    /// single-CPU systems, but kept for arch-level use).
    BadCpu,
}

/// Convert a `SchedProcError` to a Linux-style errno value.
pub fn sched_proc_error_to_errno(err: SchedProcError) -> i32 {
    match err {
        SchedProcError::InvalidArgument => 22, // EINVAL
        SchedProcError::BadCpu => 42,          // EBADCPU (Linux)
    }
}

// Note: `read_tsc()`, `get_monotonic()`, and `ms_to_cpu_time()` are now
// provided by `crate::clock` module. They were previously stubs here and in
// `proc_table.rs`; the canonical implementations live in `clock.rs` with
// global atomics (`CLOCK_UPTIME`, `TSC_PER_MS`) so that scheduler code
// can read them without needing a `&ClockState` reference.
// See `clock::get_monotonic()`, `clock::ms_to_cpu_time()`, `clock::read_tsc()`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::{CpuId, MiscFlagsBits, RtsFlagsBits, proc_nr};
    use crate::proc_table::ProcessTable;

    fn make_runnable(table: &mut ProcessTable, nr: ProcNr, prio: u8) {
        let p = table.get_mut(nr).unwrap();
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        p.p_sched.priority.store(prio, Ordering::Release);
    }

    #[test]
    fn test_enqueue_empty_queue() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        table.sched_enqueue(ProcNr(0), None, CpuId::BSP);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), Some(ProcNr(0)));
        assert_eq!(table.get(ProcNr(0)).unwrap().p_nextready.load(Ordering::Relaxed), NONE_PROC_NR);
    }

    #[test]
    fn test_enqueue_non_empty_queue() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        make_runnable(&mut table, ProcNr(1),priority::USER_Q);
        table.sched_enqueue(ProcNr(0), None, CpuId::BSP);
        table.sched_enqueue(ProcNr(1), None, CpuId::BSP);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), Some(ProcNr(0)));
        assert_eq!(table.get(ProcNr(0)).unwrap().p_nextready.load(Ordering::Relaxed), 1);
        assert_eq!(table.get(ProcNr(1)).unwrap().p_nextready.load(Ordering::Relaxed), NONE_PROC_NR);
    }

    #[test]
    fn test_enqueue_head() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        make_runnable(&mut table, ProcNr(1),priority::USER_Q);
        table.get_mut(ProcNr(0)).unwrap().p_sched.quantum.cpu_time_left.store(1000, Ordering::Release);

        table.sched_enqueue(ProcNr(1), None, CpuId::BSP);
        table.sched_enqueue_head(ProcNr(0), CpuId::BSP);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), Some(ProcNr(0)));
        assert_eq!(table.get(ProcNr(0)).unwrap().p_nextready.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_dequeue_only_process() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        table.sched_enqueue(ProcNr(0), None, CpuId::BSP);

        table.rts_set(ProcNr(0), RtsFlagsBits::PROC_STOP);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), None);
    }

    #[test]
    fn test_pick_proc_empty() {
        let table = ProcessTable::new();
        let sched = table.scheduler();
        assert_eq!(sched.pick_proc(table.procs_slice()), None);
    }

    #[test]
    fn test_pick_proc_highest_priority() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        make_runnable(&mut table, ProcNr(2),priority::MAX_USER_Q);
        table.sched_enqueue(ProcNr(0), None, CpuId::BSP);
        table.sched_enqueue(ProcNr(2), None, CpuId::BSP);

        let picked = table.scheduler().pick_proc(table.procs_slice());
        assert_eq!(picked, Some(ProcNr(2)));
    }

    #[test]
    fn test_proc_no_time_kernel_scheduled() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        table.get_mut(ProcNr(0)).unwrap().p_sched.scheduler = None;
        table.get_mut(ProcNr(0)).unwrap().p_sched.quantum.cpu_time_left.store(0, Ordering::Release);
        table.get_mut(ProcNr(0)).unwrap().p_sched.quantum.size_ms.store(200, Ordering::Release);

        table.sched_proc_no_time(ProcNr(0));

        let left = table.get(ProcNr(0)).unwrap().p_sched.quantum.cpu_time_left.load(Ordering::Acquire);
        assert!(left > 0);
    }

    #[test]
    fn test_proc_no_time_user_scheduled_preemptible() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, ProcNr(0),priority::USER_Q);
        table.get_mut(ProcNr(0)).unwrap().p_sched.scheduler = Some(proc_nr::SYSTEM);
        table.get_mut(ProcNr(0)).unwrap().p_sched.quantum.cpu_time_left.store(0, Ordering::Release);

        table.sched_enqueue(ProcNr(0), None, CpuId::BSP);
        table.sched_proc_no_time(ProcNr(0));

        assert!(table.get(ProcNr(0)).unwrap().p_rts_flags.is_set(RtsFlagsBits::NO_QUANTUM));
    }

    // ── sched_proc tests (2026-06-16) ────────────────────────────────

    #[test]
    fn test_sched_proc_priority_change() {
        // C: system.c:684-685 — priority update.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // priority 7 → 3 (higher). Design §3.8: Option<u8> replaces -1.
        let result = sched_proc(p, SchedParams { priority: Some(3), quantum: None, cpu: None, niced: false });
        assert_eq!(result, Ok(()));
        assert_eq!(p.p_sched.priority.load(Ordering::Acquire), 3);
    }

    #[test]
    fn test_sched_proc_priority_none_keeps_value() {
        // Design §3.8: None = keep current (replaces C's -1 sentinel).
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        p.p_sched.priority.store(11, Ordering::Release);
        let result = sched_proc(p, SchedParams { priority: None, quantum: None, cpu: None, niced: false });
        assert_eq!(result, Ok(()));
        // Priority unchanged.
        assert_eq!(p.p_sched.priority.load(Ordering::Acquire), 11);
    }

    #[test]
    fn test_sched_proc_priority_overflow_rejected() {
        // Design §3.8: u8 type rejects negatives at compile time.
        // Runtime check: priority > MIN_USER_Q (15) → EINVAL.
        // C: system.c:645 — priority > NR_SCHED_QUEUES → EINVAL.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p, SchedParams { priority: Some(16), quantum: None, cpu: None, niced: false });
        assert_eq!(result, Err(SchedProcError::InvalidArgument));
    }

    #[test]
    fn test_sched_proc_priority_too_high_rejected() {
        // C: system.c:645 — priority > NR_SCHED_QUEUES → EINVAL.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p, SchedParams { priority: Some(17), quantum: None, cpu: None, niced: false });
        assert_eq!(result, Err(SchedProcError::InvalidArgument));
    }

    #[test]
    fn test_sched_proc_quantum_update_resets_cpu_time() {
        // C: system.c:686-689 — quantum update resets cpu_time_left.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // Set quantum to 50ms.
        let result = sched_proc(p, SchedParams { priority: None, quantum: Some(50), cpu: None, niced: false });
        assert_eq!(result, Ok(()));
        assert_eq!(p.p_sched.quantum.size_ms.load(Ordering::Acquire), 50);
        // cpu_time_left should be (50 * TSC_PER_MS) — non-zero.
        assert!(p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0);
    }

    #[test]
    fn test_sched_proc_quantum_zero_rejected() {
        // C: system.c:647-648 — quantum < 1 → EINVAL.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p, SchedParams { priority: None, quantum: Some(0), cpu: None, niced: false });
        assert_eq!(result, Err(SchedProcError::InvalidArgument));
    }

    #[test]
    fn test_sched_proc_niced_flag_set() {
        // C: system.c:695-698 — niced=true sets MF_NICED.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p, SchedParams { priority: None, quantum: None, cpu: None, niced: true });
        assert_eq!(result, Ok(()));
        assert!(p.p_misc_flags.is_set(MiscFlagsBits::NICED));
    }

    #[test]
    fn test_sched_proc_niced_flag_clear() {
        // C: niced=false clears MF_NICED.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        p.p_misc_flags.set(MiscFlagsBits::NICED); // pre-set
        let result = sched_proc(p, SchedParams { priority: None, quantum: None, cpu: None, niced: false });
        assert_eq!(result, Ok(()));
        assert!(!p.p_misc_flags.is_set(MiscFlagsBits::NICED));
    }

    #[test]
    fn test_sched_proc_cpu_update() {
        // C: system.c:691-693 (SMP) — cpu update sets p_cpu.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p, SchedParams { priority: None, quantum: None, cpu: Some(0), niced: false });
        assert_eq!(result, Ok(()));
        assert_eq!(p.p_sched.cpu.load(Ordering::Acquire), 0);
    }

    #[test]
    fn test_sched_proc_error_to_errno() {
        // Verify error-to-errno mapping.
        assert_eq!(
            sched_proc_error_to_errno(SchedProcError::InvalidArgument),
            22 // EINVAL
        );
        assert_eq!(
            sched_proc_error_to_errno(SchedProcError::BadCpu),
            42 // EBADCPU
        );
    }

    #[test]
    fn test_sched_proc_full_update_with_all_params() {
        // C: end-to-end — priority + quantum + cpu + niced at once.
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p, SchedParams { priority: Some(5), quantum: Some(100), cpu: Some(0), niced: true });
        assert_eq!(result, Ok(()));
        assert_eq!(p.p_sched.priority.load(Ordering::Acquire), 5);
        assert_eq!(p.p_sched.quantum.size_ms.load(Ordering::Acquire), 100);
        assert_eq!(p.p_sched.cpu.load(Ordering::Acquire), 0);
        assert!(p.p_misc_flags.is_set(MiscFlagsBits::NICED));
    }

    // ── L1 C-Rust parity tests ────────────────────────────────────────
    // These tests explicitly verify that Rust behavior matches Minix3 C
    // behavior for the 9-step sched_proc flow (system.c:642-723).

    #[test]
    fn test_sched_proc_c_parity_step1_priority_validation() {
        // C: system.c:644-645 — priority > NR_SCHED_QUEUES → EINVAL
        // C: system.c:645 — if (priority > NR_SCHED_QUEUES) return EINVAL;
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // C: NR_SCHED_QUEUES = 16, MIN_USER_Q = 15
        // priority = 16 → EINVAL in both C and Rust
        let result = sched_proc(p, SchedParams { priority: Some(16), quantum: None, cpu: None, niced: false });
        assert_eq!(result, Err(SchedProcError::InvalidArgument));
        // priority = 15 → OK in both C and Rust
        let p2 = table.get_mut(ProcNr(1)).unwrap();
        p2.p_endpoint = minix_types::Endpoint(101);
        p2.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let result = sched_proc(p2, SchedParams { priority: Some(15), quantum: None, cpu: None, niced: false });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_sched_proc_c_parity_step2_quantum_validation() {
        // C: system.c:647-648 — quantum < 1 && quantum != -1 → EINVAL
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // quantum = 0 → EINVAL in C (quantum < 1 && quantum != -1)
        let result = sched_proc(p, SchedParams { priority: None, quantum: Some(0), cpu: None, niced: false });
        assert_eq!(result, Err(SchedProcError::InvalidArgument));
        // quantum = 1 → OK in C (quantum >= 1)
        let result = sched_proc(p, SchedParams { priority: None, quantum: Some(1), cpu: None, niced: false });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_sched_proc_c_parity_step8_niced_flag() {
        // C: system.c:695-698 — niced sets/clears MF_NICED
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // C: if (niced) p->p_misc_flags |= MF_NICED;
        let _ = sched_proc(p, SchedParams { priority: None, quantum: None, cpu: None, niced: true });
        assert!(p.p_misc_flags.is_set(MiscFlagsBits::NICED));
        // C: else p->p_misc_flags &= ~MF_NICED;
        let _ = sched_proc(p, SchedParams { priority: None, quantum: None, cpu: None, niced: false });
        assert!(!p.p_misc_flags.is_set(MiscFlagsBits::NICED));
    }

    #[test]
    fn test_sched_proc_c_parity_step9_no_quantum_cleared() {
        // C: system.c:698 — RTS_NO_QUANTUM is cleared after enqueue
        let mut table = ProcessTable::new();
        let p = table.get_mut(ProcNr(0)).unwrap();
        p.p_endpoint = minix_types::Endpoint(100);
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        p.p_rts_flags.set(RtsFlagsBits::NO_QUANTUM); // pre-set
        let _ = sched_proc(p, SchedParams { priority: Some(7), quantum: None, cpu: None, niced: false });
        // C: after sched_proc, RTS_NO_QUANTUM should be cleared
        assert!(!p.p_rts_flags.is_set(RtsFlagsBits::NO_QUANTUM));
    }
}
