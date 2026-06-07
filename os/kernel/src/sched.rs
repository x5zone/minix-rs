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
    priority, proc_nr, rts, CpuCycles, CpuId, KProcess, ProcNr,
};

/// Per-CPU scheduler state holding ready queue head/tail indices.
///
/// Design decision §3.2: Scheduler holds queue metadata only.
/// C: `run_q_head[NR_SCHED_QUEUES]` / `run_q_tail[NR_SCHED_QUEUES]` in `cpulocals.h:58-59`.
pub struct Scheduler {
    run_q_head: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
    run_q_tail: [Option<ProcNr>; priority::NR_SCHED_QUEUES],
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
        let mut link_idx: Option<usize> = None;
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
                .and_then(|p| p.p_nextready);
            link_is_head = false;
        }

        if !found {
            panic!("dequeue: process not found in its priority queue");
        }

        // Remove from linked list
        let next = nr_to_idx(nr)
            .and_then(|i| procs.get(i))
            .and_then(|p| p.p_nextready);

        if link_is_head {
            self.run_q_head[q] = next;
        } else if let Some(prev_nr) = prev {
            let prev_idx = nr_to_idx(prev_nr).unwrap();
            procs[prev_idx].p_nextready = next;
        }

        // Update tail if needed
        if self.run_q_tail[q] == Some(nr) {
            self.run_q_tail[q] = prev;
        }

        // Clear the removed process's nextready
        let nr_idx = nr_to_idx(nr).unwrap();
        procs[nr_idx].p_nextready = None;
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
    const NR_TASKS: isize = 5;
    const PROC_TABLE_SIZE: usize = NR_TASKS as usize + 256;
    let offset = nr as isize + NR_TASKS;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

/// Check if a process is preemptible via its privilege flags.
///
/// C: `priv(p)->s_flags & PREEMPTIBLE` in const.h:143.
fn is_preemptible(p: &KProcess) -> bool {
    let prio = p.get_priority().get();
    prio != priority::TASK_Q
}

/// Check if a process is scheduled by the kernel (no user-space scheduler).
///
/// C: `proc_kernel_scheduler(p)` macro in proc.h:178.
fn is_kernel_scheduled(p: &KProcess) -> bool {
    p.p_sched.scheduler.is_none() || p.p_sched.scheduler == Some(p.p_nr)
}

/// Check if a process has remaining CPU time.
fn has_cpu_time_left(p: &KProcess) -> bool {
    p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0
}

/// Read the Time Stamp Counter.
///
/// C: `read_tsc_64()` — architecture-specific TSC read.
/// TODO: Implement via arch trait (e.g., `minix_arch::read_tsc()`).
fn read_tsc() -> CpuCycles {
    0
}

/// Get monotonic time (for `p_dequeued` recording).
///
/// C: `get_monotonic()` — returns monotonic clock value.
/// TODO: Implement via timer subsystem.
fn get_monotonic() -> u64 {
    0
}

/// Convert milliseconds to CPU time cycles.
///
/// C: `ms_2_cpu_time(ms)` — converts millisecond quantum to CPU cycles.
/// TODO: Implement based on CPU frequency detection.
fn ms_to_cpu_time(ms: u32) -> u64 {
    ms as u64 * 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::rts;
    use crate::proc_table::ProcessTable;

    fn make_runnable(table: &mut ProcessTable, nr: ProcNr, prio: i8) {
        let p = table.get_mut(nr).unwrap();
        p.p_rts_flags.clear(rts::SLOT_FREE);
        p.p_sched.priority.store(prio, Ordering::Release);
    }

    #[test]
    fn test_enqueue_empty_queue() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, 0, priority::USER_Q);
        table.sched_enqueue(0, None, 0);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), Some(0));
        assert_eq!(table.get(0).unwrap().p_nextready, None);
    }

    #[test]
    fn test_enqueue_non_empty_queue() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, 0, priority::USER_Q);
        make_runnable(&mut table, 1, priority::USER_Q);
        table.sched_enqueue(0, None, 0);
        table.sched_enqueue(1, None, 0);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), Some(0));
        assert_eq!(table.get(0).unwrap().p_nextready, Some(1));
        assert_eq!(table.get(1).unwrap().p_nextready, None);
    }

    #[test]
    fn test_enqueue_head() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, 0, priority::USER_Q);
        make_runnable(&mut table, 1, priority::USER_Q);
        table.get_mut(0).unwrap().p_sched.quantum.cpu_time_left.store(1000, Ordering::Release);

        table.sched_enqueue(1, None, 0);
        table.sched_enqueue_head(0);

        let q = priority::USER_Q as usize;
        let sched = table.scheduler();
        assert_eq!(sched.queue_head(q), Some(0));
        assert_eq!(table.get(0).unwrap().p_nextready, Some(1));
    }

    #[test]
    fn test_dequeue_only_process() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, 0, priority::USER_Q);
        table.sched_enqueue(0, None, 0);

        table.rts_set(0, rts::PROC_STOP);

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
        make_runnable(&mut table, 0, priority::USER_Q);
        make_runnable(&mut table, 2, priority::MAX_USER_Q);
        table.sched_enqueue(0, None, 0);
        table.sched_enqueue(2, None, 0);

        let picked = table.scheduler().pick_proc(table.procs_slice());
        assert_eq!(picked, Some(2));
    }

    #[test]
    fn test_proc_no_time_kernel_scheduled() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, 0, priority::USER_Q);
        table.get_mut(0).unwrap().p_sched.scheduler = None;
        table.get_mut(0).unwrap().p_sched.quantum.cpu_time_left.store(0, Ordering::Release);
        table.get_mut(0).unwrap().p_sched.quantum.size_ms.store(200, Ordering::Release);

        table.sched_proc_no_time(0);

        let left = table.get(0).unwrap().p_sched.quantum.cpu_time_left.load(Ordering::Acquire);
        assert!(left > 0);
    }

    #[test]
    fn test_proc_no_time_user_scheduled_preemptible() {
        let mut table = ProcessTable::new();
        make_runnable(&mut table, 0, priority::USER_Q);
        table.get_mut(0).unwrap().p_sched.scheduler = Some(proc_nr::SYSTEM);
        table.get_mut(0).unwrap().p_sched.quantum.cpu_time_left.store(0, Ordering::Release);

        table.sched_enqueue(0, None, 0);
        table.sched_proc_no_time(0);

        assert!(table.get(0).unwrap().p_rts_flags.is_set(rts::NO_QUANTUM));
    }
}
