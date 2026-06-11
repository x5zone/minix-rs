/*! Kernel process table.

# SMP Safety (BKL Requirement)

**All access to `ProcessTable` must be performed while holding the Big Kernel Lock (BKL).**

The BKL is a spinlock that ensures at most one CPU executes kernel code at a time.
This means:
- `get()` / `get_mut()`: Safe under BKL because no other CPU can concurrently
  modify the process table.
- `rts_set()` / `rts_unset()`: Safe under BKL because the scheduler state is
  only modified by the BKL-holding CPU.
- `sched_enqueue()` / `sched_dequeue()`: Safe under BKL because the ready queues
  are only modified by the BKL-holding CPU.

**Violations**: Calling any method on `ProcessTable` without holding the BKL
is a data race and may cause UB in SMP configurations.

# Why not use `Mutex` or `RwLock`?

The kernel uses a spinlock (BKL) rather than a blocking lock because:
1. Kernel code may not sleep while holding the BKL (spinlock constraint).
2. The BKL protects the entire kernel, not just the process table.
3. Using a separate `Mutex` would add overhead without improving granularity
   since the BKL already serializes all kernel access.
*/

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use minix_types::Endpoint;

use crate::proc::{KProcess, ProcNr, ProcName, RtsFlagsBits, proc_nr, CpuId};
use crate::sched::Scheduler;

pub const NR_TASKS: usize = 5;
pub const NR_PROCS: usize = 256;
pub const NR_SYS_PROCS: usize = 64;
pub const PROC_TABLE_SIZE: usize = NR_TASKS + NR_PROCS;

/// Kernel process table.
///
/// Contains all process slots (kernel tasks + user processes) and the scheduler.
/// All methods require the caller to hold the BKL (see module-level documentation).
pub struct ProcessTable {
    procs: Box<[KProcess]>,
    sched: Scheduler,
}

impl ProcessTable {
    pub fn new() -> Self {
        let mut procs: Vec<KProcess> = (0..PROC_TABLE_SIZE)
            .map(|i| {
                let nr = (i as ProcNr) - (NR_TASKS as ProcNr);
                let endpoint = Endpoint::from_generation_slot(0, nr);
                KProcess::new(nr, endpoint)
            })
            .collect();

        let idle_idx = nr_to_idx(proc_nr::IDLE).unwrap();
        procs[idle_idx].p_endpoint = Endpoint::from_generation_slot(0, proc_nr::IDLE);
        procs[idle_idx].p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        procs[idle_idx].p_name = ProcName::from_str("IDLE");

        Self {
            procs: procs.into_boxed_slice(),
            sched: Scheduler::new(),
        }
    }

    pub fn get(&self, nr: ProcNr) -> Option<&KProcess> {
        let idx = nr_to_idx(nr)?;
        Some(&self.procs[idx])
    }

    pub fn get_mut(&mut self, nr: ProcNr) -> Option<&mut KProcess> {
        let idx = nr_to_idx(nr)?;
        Some(&mut self.procs[idx])
    }

    pub fn is_valid_nr(nr: ProcNr) -> bool {
        nr_to_idx(nr).is_some()
    }

    pub fn is_empty(&self, nr: ProcNr) -> bool {
        self.get(nr).map_or(false, |p| p.p_rts_flags.get() == RtsFlagsBits::SLOT_FREE)
    }

    pub fn is_kernel(nr: ProcNr) -> bool {
        nr < 0
    }

    /// Set RTS flags on a process. If the process transitions from runnable
    /// to non-runnable, automatically dequeues it from the scheduler.
    ///
    /// C: `RTS_SET(p, flags)` macro in proc.h — sets flags and calls
    /// `dequeue()` when the process becomes non-runnable.
    pub fn rts_set(&mut self, nr: ProcNr, flags: RtsFlagsBits) {
        let was_runnable = self.get(nr).map_or(false, |p| p.is_runnable());
        if let Some(p) = self.get_mut(nr) {
            p.p_rts_flags.set(flags);
        }
        let is_runnable = self.get(nr).map_or(false, |p| p.is_runnable());
        if was_runnable && !is_runnable {
            if self.is_in_scheduler(nr) {
                self.sched_dequeue(nr);
            }
        }
    }

    /// Clear RTS flags on a process. If the process transitions from
    /// non-runnable to runnable, automatically enqueues it in the scheduler.
    ///
    /// C: `RTS_UNSET(p, flags)` macro in proc.h — clears flags and calls
    /// `enqueue()` when the process becomes runnable.
    pub fn rts_unset(&mut self, nr: ProcNr, flags: RtsFlagsBits) {
        let was_runnable = self.get(nr).map_or(false, |p| p.is_runnable());
        if let Some(p) = self.get_mut(nr) {
            p.p_rts_flags.clear(flags);
        }
        let is_runnable = self.get(nr).map_or(false, |p| p.is_runnable());
        if !was_runnable && is_runnable {
            let cpu_id = self.get(nr).map_or(0, |p| {
                p.p_sched.cpu.load(Ordering::Acquire)
            });
            self.sched_enqueue(nr, None, cpu_id);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &KProcess> {
        self.procs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut KProcess> {
        self.procs.iter_mut()
    }

    // ── Scheduler integration wrappers ──
    //
    // These methods coordinate borrows between `self.sched` and `self.procs`
    // to avoid self-referential mutable borrows.

    /// Get a read-only reference to the embedded scheduler.
    pub fn scheduler(&self) -> &Scheduler {
        &self.sched
    }

    /// Get a read-only slice of the process array (for `pick_proc`).
    pub fn procs_slice(&self) -> &[KProcess] {
        &self.procs
    }

    /// Check if a process is currently in the scheduler's ready queues.
    ///
    /// A process is in the scheduler if it is a queue head or has a non-None
    /// `p_nextready` (meaning it's linked in a queue chain). This is a
    /// conservative check: a process that is the tail of a queue and has
    /// `p_nextready == None` will only be detected by scanning all queue heads.
    fn is_in_scheduler(&self, nr: ProcNr) -> bool {
        for q in 0..16 {
            let mut current = self.sched.queue_head(q);
            while let Some(cur_nr) = current {
                if cur_nr == nr {
                    return true;
                }
                let cur_idx = nr_to_idx(cur_nr);
                current = cur_idx
                    .and_then(|i| self.procs.get(i))
                    .and_then(|p| p.p_nextready);
            }
        }
        false
    }

    /// Enqueue a runnable process at the tail of its priority queue.
    ///
    /// C: `enqueue()` in proc.c:1595-1659.
    /// Design decision §3.6: records `enter_queue` for the enqueued process.
    pub fn sched_enqueue(&mut self, nr: ProcNr, current_nr: Option<ProcNr>, cpu_id: CpuId) {
        let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);
        debug_assert!(q < 16, "sched_enqueue: priority out of range");

        // Phase 1: update queue arrays
        let info = self.sched.enqueue_queue_tail(nr, q);

        // Phase 2: update process fields
        {
            let procs = &mut self.procs;
            // Clear p_nextready of enqueued process
            let nr_idx = nr_to_idx(nr).unwrap();
            procs[nr_idx].p_nextready = None;

            // Link old tail to new process
            if let Some(tail_nr) = info.old_tail {
                let tail_idx = nr_to_idx(tail_nr).unwrap();
                procs[tail_idx].p_nextready = Some(nr);
            }
        }

        // Phase 3: preemption check (only same CPU)
        if let Some(cur_nr) = current_nr {
            let (cur_prio, cur_cpu, cur_preemptible) = {
                let cur = self.get(cur_nr).expect("sched_enqueue: invalid current nr");
                (
                    cur.get_priority().get(),
                    cur.p_sched.cpu.load(Ordering::Acquire),
                    cur.get_priority().get() != 0,
                )
            };
            let new_prio = q as i8;
            if cur_cpu == cpu_id && cur_prio > new_prio && cur_preemptible {
                self.rts_set(cur_nr, RtsFlagsBits::PREEMPTED);
            }
        }

        // Phase 4: record enter_queue for the enqueued process
        // Design decision §3.6: fix Minix3 bug
        let tsc = read_tsc();
        self.get_mut(nr).unwrap().p_accounting.record_enqueue(tsc);
    }

    /// Enqueue a preempted process at the head of its priority queue.
    ///
    /// C: `enqueue_head()` in proc.c:1670-1711.
    pub fn sched_enqueue_head(&mut self, nr: ProcNr) {
        let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);

        // Phase 1: update queue arrays
        let old_head = self.sched.queue_head(q);
        self.sched.enqueue_queue_head(nr, q);

        // Phase 2: update process fields
        {
            let nr_idx = nr_to_idx(nr).unwrap();
            self.procs[nr_idx].p_nextready = old_head;
        }

        // Phase 3: accounting (dequeues--, preempted++)
        let tsc = read_tsc();
        let acc = &mut self.get_mut(nr).unwrap().p_accounting;
        acc.record_enqueue(tsc);
        acc.dequeues.fetch_sub(1, Ordering::AcqRel);
        acc.preempted.fetch_add(1, Ordering::AcqRel);
    }

    /// Dequeue a non-runnable process from its priority queue.
    ///
    /// C: `dequeue()` in proc.c:1716-1780.
    pub fn sched_dequeue(&mut self, nr: ProcNr) {
        let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);

        // Phase 1: remove from queue + update linked list
        self.sched.dequeue_from_queue(nr, q, &mut self.procs);

        // Phase 2: accounting
        let tsc = read_tsc();
        let acc = &mut self.get_mut(nr).unwrap().p_accounting;
        acc.record_dequeue(tsc);

        self.get_mut(nr).unwrap().p_dequeued.store(
            get_monotonic(),
            Ordering::Release,
        );
    }

    /// Handle quantum exhaustion based on scheduling policy.
    ///
    /// C: `proc_no_time()` in proc.c:1893-1910.
    pub fn sched_proc_no_time(&mut self, nr: ProcNr) {
        let (kernel_scheduled, preemptible, quantum_ms) = {
            let p = self.get(nr).expect("sched_proc_no_time: invalid proc nr");
            let ks = p.p_sched.scheduler.is_none() || p.p_sched.scheduler == Some(p.p_nr);
            let pre = p.get_priority().get() != 0;
            let qms = p.p_sched.quantum.size_ms.load(Ordering::Acquire);
            (ks, pre, qms)
        };

        if !kernel_scheduled && preemptible {
            // User-scheduled + preemptible: notify scheduler
            self.rts_set(nr, RtsFlagsBits::NO_QUANTUM);
            // TODO: send SCHEDULING_NO_QUANTUM message to scheduler process
        } else {
            // Kernel-scheduled or non-preemptible: reset quantum
            let cpu_time = ms_to_cpu_time(quantum_ms);
            self.get_mut(nr)
                .unwrap()
                .p_sched
                .quantum
                .cpu_time_left
                .store(cpu_time, Ordering::Release);
        }
    }
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    let offset = nr as isize + NR_TASKS as isize;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

fn read_tsc() -> u64 {
    0
}

fn get_monotonic() -> u64 {
    0
}

fn ms_to_cpu_time(ms: u32) -> u64 {
    ms as u64 * 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_table_new() {
        let table = ProcessTable::new();
        assert!(table.get(0).is_some());
        assert!(table.get(-1).is_some());
        assert!(table.get(255).is_some());
        assert!(table.get(256).is_none());
    }

    #[test]
    fn test_process_table_idle() {
        let table = ProcessTable::new();
        let idle = table.get(proc_nr::IDLE).unwrap();
        assert!(idle.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP));
    }

    #[test]
    fn test_is_valid_nr() {
        assert!(ProcessTable::is_valid_nr(0));
        assert!(ProcessTable::is_valid_nr(-5));
        assert!(ProcessTable::is_valid_nr(255));
        assert!(!ProcessTable::is_valid_nr(256));
    }

    #[test]
    fn test_is_kernel() {
        assert!(ProcessTable::is_kernel(-1));
        assert!(!ProcessTable::is_kernel(0));
    }

    #[test]
    fn test_rts_set_unset() {
        let mut table = ProcessTable::new();
        let nr = 1;
        table.get_mut(nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        assert!(table.get(nr).unwrap().is_runnable());

        table.rts_set(nr, RtsFlagsBits::PROC_STOP);
        assert!(!table.get(nr).unwrap().is_runnable());

        table.rts_unset(nr, RtsFlagsBits::PROC_STOP);
        assert!(table.get(nr).unwrap().is_runnable());
    }

    /// Integration test: fork flow from parent process creation through
    /// KProcess::fork_from, rts_set/rts_unset, and scheduler enqueue/dequeue.
    ///
    /// Simulates the kernel side of a fork syscall:
    /// 1. Parent is runnable
    /// 2. Child is created via fork_from
    /// 3. Child starts with NO_QUANTUM (not runnable, but slot is allocated)
    /// 4. After VM fork completes, child is made runnable via rts_unset(NO_QUANTUM)
    /// 5. Both parent and child are runnable
    #[test]
    fn test_fork_integration_flow() {
        use crate::proc::KProcess;
        use minix_types::Endpoint;

        let mut table = ProcessTable::new();

        // Step 1: Make parent (slot 0) runnable
        let parent_nr = 0;
        table.get_mut(parent_nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        assert!(table.get(parent_nr).unwrap().is_runnable());

        // Step 2: Create child via fork_from (read parent, create child separately)
        let child_nr = 1;
        let child_endpoint = Endpoint::fork_new_endpoint(
            table.get(parent_nr).unwrap().p_endpoint,
            child_nr,
        );
        let child = KProcess::fork_from(
            table.get(parent_nr).unwrap(),
            child_nr,
            child_endpoint,
        );

        // Step 3: Child starts with NO_QUANTUM (not runnable, but slot is allocated)
        assert!(child.p_rts_flags.is_set(RtsFlagsBits::NO_QUANTUM));
        assert!(!child.is_runnable());
        // Child does NOT have SLOT_FREE (slot is allocated for the child)
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE));

        // Place child into the table
        table.get_mut(child_nr).map(|slot| *slot = child);

        // Step 4: Simulate VM fork completion — make child runnable
        table.rts_unset(child_nr, RtsFlagsBits::NO_QUANTUM);
        assert!(table.get(child_nr).unwrap().is_runnable());

        // Step 5: Both should be runnable now
        assert!(table.get(parent_nr).unwrap().is_runnable());
        assert!(table.get(child_nr).unwrap().is_runnable());
    }

    /// Integration test: fork child inherits RTS flags correctly.
    ///
    /// Verifies that fork_from applies Minix3's fork corrections:
    /// - Child gets NO_QUANTUM (not yet scheduled)
    /// - Child does NOT inherit SENDING/RECEIVING
    /// - Child does NOT inherit SIGNALED/SIG_PENDING
    #[test]
    fn test_fork_child_rts_corrections() {
        use crate::proc::KProcess;
        use minix_types::Endpoint;

        let mut table = ProcessTable::new();

        // Set up parent as runnable
        let parent_nr = 0;
        {
            let parent = table.get_mut(parent_nr).unwrap();
            parent.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let child_nr = 1;
        let child_endpoint = Endpoint::fork_new_endpoint(
            table.get(parent_nr).unwrap().p_endpoint,
            child_nr,
        );
        let child = KProcess::fork_from(
            table.get(parent_nr).unwrap(),
            child_nr,
            child_endpoint,
        );

        // Child should have NO_QUANTUM (not yet scheduled)
        assert!(child.p_rts_flags.is_set(RtsFlagsBits::NO_QUANTUM));
        // Child should NOT have SENDING or RECEIVING (fork correction)
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::SENDING));
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        // Child should NOT have SIGNALED or SIG_PENDING (fork correction)
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(!child.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING));
    }
}
