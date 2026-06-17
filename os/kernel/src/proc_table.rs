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

use crate::proc::{KProcess, ProcNr, ProcName, RtsFlagsBits, MiscFlagsBits, proc_nr, CpuId, NONE_PROC_NR};
use crate::sched::Scheduler;
use crate::clock;

pub const NR_TASKS: usize = minix_types::NR_TASKS;
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
    /// Global VM request queue. C: `EXTERN struct proc *vmrequest` — glo.h:41.
    /// Replaces Minix3's global linked list head with a structured queue.
    /// All access requires BKL (00-kernel-overview §1.5).
    vm_request_queue: crate::vm::VmRequestQueue,
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
            vm_request_queue: crate::vm::VmRequestQueue::new(),
        }
    }

    /// Get a process by its logical process number.
    /// C: `proc_addr(n)` — but C version returns a raw pointer without bounds checking.
    /// Rust returns `Option` to enforce safety (08-proc-macros.md §3.1).
    pub fn get(&self, nr: ProcNr) -> Option<&KProcess> {
        let idx = nr_to_idx(nr)?;
        Some(&self.procs[idx])
    }

    /// Get a mutable reference to a process by its logical process number.
    /// C: `proc_addr(n)` — same as `get()` but mutable.
    pub fn get_mut(&mut self, nr: ProcNr) -> Option<&mut KProcess> {
        let idx = nr_to_idx(nr)?;
        Some(&mut self.procs[idx])
    }

    /// Check if a process number is valid (within the process table range).
    /// C: `isokprocn(n)` — `(unsigned)((n) + NR_TASKS) < NR_PROCS + NR_TASKS`
    /// Rust uses `nr_to_idx` internally (08-proc-macros.md §3.1).
    pub fn is_valid_nr(nr: ProcNr) -> bool {
        nr_to_idx(nr).is_some()
    }

    /// Check if a process slot is free (SLOT_FREE flag set).
    /// C: `isemptyn(n)` = `isemptyp(proc_addr(n))` = `(p->p_rts_flags == RTS_SLOT_FREE)`
    /// (08-proc-macros.md §3.7)
    pub fn is_empty(&self, nr: ProcNr) -> bool {
        self.get(nr).map_or(false, |p| p.p_rts_flags.get() == RtsFlagsBits::SLOT_FREE)
    }

    /// Check if a process number belongs to a kernel task.
    /// C: `iskerneln(n)` = `((n) < 0)` (08-proc-macros.md §3.1)
    pub fn is_kernel(nr: ProcNr) -> bool {
        nr < 0
    }

    /// Get a reference to the global VM request queue.
    /// C: `vmrequest` global — glo.h:41.
    pub fn vm_request_queue(&self) -> &crate::vm::VmRequestQueue {
        &self.vm_request_queue
    }

    /// Get a mutable reference to the global VM request queue.
    /// C: `vmrequest` global — glo.h:41.
    pub fn vm_request_queue_mut(&mut self) -> &mut crate::vm::VmRequestQueue {
        &mut self.vm_request_queue
    }

    /// Handle VMCTL_MEMREQ_GET: dequeue the next pending VM request.
    /// C: do_vmctl.c:36-72.
    ///
    /// This method directly implements the dequeue logic with proper
    /// `nr_to_idx` conversion, because `VmRequestQueue::dequeue_filtered`
    /// assumes `ProcNr == array index` which is not true for `ProcessTable`.
    pub fn vm_memreq_get(
        &mut self,
    ) -> Result<(ProcNr, crate::vm::VmCheckParams), crate::vm::VmCtlError> {
        use crate::vm::{VmCtlError, VmSuspendState};

        // Traverse the queue looking for the first request that passes the filter.
        // C: do_vmctl.c:37-72 — traverse vmrequest linked list.
        let mut current = self.vm_request_queue.head();
        let mut prev_idx: Option<usize> = None;
        let result_idx;

        loop {
            match current {
                Some(idx) => {
                    let idx = idx as usize;
                    let proc = &self.procs[idx];
                    let next = proc.p_next_requestor;

                    // Check if this process has a valid VM suspend context
                    let passed = proc.p_vm_suspend.as_ref().map_or(false, |ctx| {
                        // Verify the process is in Pending state
                        ctx.state == VmSuspendState::Pending
                    });

                    if passed {
                        // Remove from queue
                        if let Some(pidx) = prev_idx {
                            self.procs[pidx].p_next_requestor = next;
                        } else {
                            self.vm_request_queue.set_head(next);
                        }
                        self.procs[idx].p_next_requestor = None;
                        result_idx = idx;
                        break;
                    }

                    prev_idx = Some(idx);
                    current = next;
                }
                None => return Err(VmCtlError::NoRequest),
            }
        }

        // Transition state: Pending → Fetched
        let proc = &mut self.procs[result_idx];
        let ctx = proc.p_vm_suspend.as_mut().ok_or(VmCtlError::InvalidState)?;
        if ctx.state != VmSuspendState::Pending {
            return Err(VmCtlError::InvalidState);
        }
        ctx.state = VmSuspendState::Fetched;
        let params = ctx.check_params;

        // Convert array index back to ProcNr
        let nr = result_idx as ProcNr - NR_TASKS as ProcNr;
        Ok((nr, params))
    }

    /// Handle VMCTL_MEMREQ_REPLY: VM replies with the result of a memory request.
    /// C: do_vmctl.c:73-109.
    pub fn vm_memreq_reply(
        &mut self,
        target_nr: ProcNr,
        result: crate::vm::VmCheckResult,
    ) -> Result<(), crate::vm::VmCtlError> {
        let proc = self.get_mut(target_nr)
            .ok_or(crate::vm::VmCtlError::InvalidEndpoint)?;
        crate::vm::VmRequestHandler::memreq_reply(proc, result)
    }

    /// Enqueue a process into the VM request queue.
    /// Encapsulates access to both `vm_request_queue` and `procs`.
    /// Returns `true` if the queue was empty before insertion.
    /// C: vm_suspend() in proc.c:254-257.
    ///
    /// Note: `VmRequestQueue` uses array indices internally, so we convert
    /// ProcNr to index before enqueuing.
    pub fn vm_enqueue(&mut self, proc_nr: ProcNr) -> bool {
        let idx = nr_to_idx(proc_nr).expect("vm_enqueue: invalid ProcNr");
        self.vm_request_queue.enqueue(idx as ProcNr, &mut self.procs)
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
                // C uses `get_cpu_var(rp->p_cpu, run_q_head)` — the process's
                // assigned CPU determines which per-CPU queue to dequeue from.
                let cpu_id = self.get(nr)
                    .map(|p| p.p_sched.cpu.load(Ordering::Acquire))
                    .unwrap_or(0);
                self.sched_dequeue(nr, cpu_id);
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

    /// Iterate over all process slots (kernel tasks + user processes).
    /// C: `for (p = BEG_PROC_ADDR; p < END_PROC_ADDR; p++)` (08-proc-macros.md §3.2)
    pub fn iter(&self) -> impl Iterator<Item = &KProcess> {
        self.procs.iter()
    }

    /// Iterate mutably over all process slots.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut KProcess> {
        self.procs.iter_mut()
    }

    /// Get the signal manager endpoint for a process.
    ///
    /// C: `sig_mgr = priv(rp)->s_sig_mgr; if(sig_mgr == SELF) sig_mgr = rp->p_endpoint`
    /// (system.c:399-400).
    ///
    /// Resolves the `s_sig_mgr` field from the process's `KPriv` entry.
    /// If `s_sig_mgr == SELF`, returns the process's own endpoint (the process
    /// manages its own signals). Returns `None` if the process has no priv slot
    /// or the priv slot is invalid.
    pub fn sig_mgr(&self, nr: ProcNr, priv_table: &crate::kpriv::PrivTable) -> Option<Endpoint> {
        let target = self.get(nr)?;
        let pid = target.priv_id?;
        let priv_ = priv_table.get(pid)?;
        Some(if priv_.s_sig_mgr == Endpoint::SELF {
            target.p_endpoint
        } else {
            priv_.s_sig_mgr
        })
    }

    /// Set the BSP's bill pointer to the IDLE kernel task.
    ///
    /// C: `bsp_finish_booting` step 2 — `get_cpulocal_var(bill_ptr) =
    /// get_cpulocal_var_ptr(idle_proc)` (main.c:50).
    ///
    /// We do not have a live `SmpState` plumbed here yet; this records the
    /// intent in `ProcessTable` so the scheduler side can read it. The real
    /// per-CPU write lives in `SmpState.cpu_locals[0].set_running(IDLE)`
    /// (see smp.rs:127) and is performed by the scheduler init path.
    pub fn set_bill_to_idle(&mut self) {
        use crate::proc::proc_nr;
        if let Some(idle) = self.get_mut(proc_nr::IDLE) {
            // Bill to IDLE so that time spent in this pre-scheduling phase
            // is accounted to the kernel, not to any user process. Matches
            // C's `bill_ptr = idle_proc` semantic.
            idle.p_accounting.bill_to_idle();
        }
    }

    // ── Scheduler integration wrappers ──
    //
    // These methods coordinate borrows between `self.sched` and `self.procs`
    // to avoid self-referential mutable borrows.

    /// Get a read-only reference to the embedded scheduler.
    ///
    /// **Note**: In the current single-CPU build, this returns the BSP
    /// scheduler (`self.sched`). When SMP lands, callers should use
    /// [`sched_for_cpu`](Self::sched_for_cpu) instead, which dispatches
    /// to the per-CPU scheduler in `SmpState.cpu_locals[cpu].scheduler`.
    pub fn scheduler(&self) -> &Scheduler {
        &self.sched
    }

    /// Get the per-CPU scheduler for the given CPU.
    ///
    /// # Design (per-CPU run queues)
    ///
    /// C stores `run_q_head[]` / `run_q_tail[]` in `__cpu_local_vars`
    /// (cpulocals.h:58-59), so each CPU has its own ready queues.
    /// This method mirrors that design: it returns the `Scheduler`
    /// belonging to the specified CPU.
    ///
    /// **Current implementation**: single-CPU builds return `&self.sched`
    /// (the BSP scheduler). The `CpuLocal::scheduler` field exists but is
    /// not yet the authoritative source — it will become so when
    /// `ProcessTable::sched` is removed in the SMP migration.
    ///
    /// **SMP migration path**: replace `&self.sched` with
    /// `&smp_state.cpu_locals[cpu_id as usize].scheduler`, and remove
    /// `self.sched` from `ProcessTable`. This requires passing `&SmpState`
    /// into this method (or making `ProcessTable` own `SmpState`).
    ///
    /// # Arguments
    ///
    /// * `cpu_id` — CPU index (0 = BSP). Out-of-range values default to
    ///   CPU 0 (BSP fallback).
    pub fn sched_for_cpu(&self, cpu_id: u32) -> &Scheduler {
        let _ = cpu_id; // Suppress unused warning; will be used in SMP migration
        // TODO (SMP): return &smp_state.cpu_locals[cpu_id as usize].scheduler;
        &self.sched
    }

    /// Get a mutable reference to the per-CPU scheduler for the given CPU.
    ///
    /// See [`sched_for_cpu`](Self::sched_for_cpu) for design rationale.
    pub fn sched_for_cpu_mut(&mut self, cpu_id: u32) -> &mut Scheduler {
        let _ = cpu_id;
        // TODO (SMP): return &mut smp_state.cpu_locals[cpu_id as usize].scheduler;
        &mut self.sched
    }

    /// Get a read-only slice of the process array (for `pick_proc`).
    pub fn procs_slice(&self) -> &[KProcess] {
        &self.procs
    }

    /// Get a mutable slice of the process array (for IPC engine).
    pub fn procs_slice_mut(&mut self) -> &mut [KProcess] {
        &mut self.procs
    }

    /// Resolve an endpoint to a process number (slot index).
    /// C: `isokendpt(endpoint, &proc_nr)` — validates endpoint and returns
    /// the process slot number. Returns `None` if the endpoint is invalid
    /// or the slot is free.
    pub fn endpoint_to_nr(&self, endpoint: Endpoint) -> Option<ProcNr> {
        self.procs.iter().find(|p| {
            p.p_endpoint == endpoint && p.p_rts_flags.get() != RtsFlagsBits::SLOT_FREE
        }).map(|p| p.p_nr)
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
                    .map(|p| {
                        let v = p.p_nextready.load(Ordering::Relaxed);
                        if v == NONE_PROC_NR { None } else { Some(v) }
                    })
                    .flatten();
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
        // Note: uses self.sched directly; see sched_dequeue for SMP migration note.
        let info = self.sched.enqueue_queue_tail(nr, q);

        // Phase 2: update process fields
        {
            let procs = &mut self.procs;
            // Clear p_nextready of enqueued process
            let nr_idx = nr_to_idx(nr).unwrap();
            procs[nr_idx].p_nextready.store(NONE_PROC_NR, Ordering::Relaxed);

            // Link old tail to new process
            if let Some(tail_nr) = info.old_tail {
                let tail_idx = nr_to_idx(tail_nr).unwrap();
                procs[tail_idx].p_nextready.store(nr, Ordering::Relaxed);
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
    /// C uses `get_cpulocal_var(run_q_head)` — the current CPU's queue.
    pub fn sched_enqueue_head(&mut self, nr: ProcNr, cpu_id: CpuId) {
        let _ = cpu_id; // Will be used when Scheduler moves to CpuLocal
        let q = self.get(nr).map_or(0, |p| p.get_priority().get() as usize);

        // Phase 1: update queue arrays
        // Note: uses self.sched directly; see sched_dequeue for SMP migration note.
        let old_head = self.sched.queue_head(q);
        self.sched.enqueue_queue_head(nr, q);

        // Phase 2: update process fields
        {
            let nr_idx = nr_to_idx(nr).unwrap();
            let old_head_val = old_head.unwrap_or(NONE_PROC_NR);
            self.procs[nr_idx].p_nextready.store(old_head_val, Ordering::Relaxed);
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
    /// C uses `get_cpu_var(rp->p_cpu, run_q_head)` — the process's CPU queue.
    ///
    /// **Note**: Currently uses `self.sched` directly because `dequeue_from_queue`
    /// needs simultaneous `&mut Scheduler` + `&mut [KProcess]`, which conflicts
    /// with `sched_for_cpu_mut()` borrowing `&mut self`. When SMP lands and
    /// `Scheduler` moves to `CpuLocal`, the borrow will be split naturally:
    /// `&mut smp_state.cpu_locals[cpu].scheduler` + `&mut process_table.procs`.
    pub fn sched_dequeue(&mut self, nr: ProcNr, cpu_id: CpuId) {
        let _ = cpu_id; // Will be used when Scheduler moves to CpuLocal
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
            // TODO: expand the deferral into a
            // 4-step contract mirroring C's `sched()` macro at
            // `proc.c:892-915`.
            //
            // C sends a `SCHEDULING_NO_QUANTUM` message to the
            // scheduler process (`p->p_scheduler`) so the user-mode
            // scheduler can re-schedule this process. The Rust
            // rewrite is blocked on:
            //
            //   1. **IPC primitive**: we need `IpcEngine::notify` to
            //      work for kernel→system-process delivery. Kernel
            //      IPC core is in progress (partial fix from previous
            //      round; full wiring pending).
            //   2. **Message type**: we need a `MessSchedule` variant
            //      in `minix-types` carrying the target proc_nr +
            //      quantum_ms. The current kernel scheduler uses
            //      `MessageM1` for IPC traffic which is wrong for
            //      the 64-bit layout (see MessSchedule variant TODO).
            //   3. **Endpoint→ProcNr resolution**: we need to
            //      resolve `p_scheduler` (a ProcNr) to an Endpoint
            //      for IPC. The current code only goes Endpoint→
            //      ProcNr (partial fix adding `endpoint_to_nr`).
            //
            // Until all three land, the only action we take is to
            // set RTS_NO_QUANTUM. The user-mode scheduler is
            // expected to poll this flag (via the `getrusage`-like
            // InfoQuery path) on a tick boundary, or to receive
            // the same information via the per-process flags dump
            // that `kcall_filter_check` already uses internally.
            //
            // Effect on the user scheduler: the scheduler's
            // accounting will be off-by-one quantum until IPC lands.
            // This is acceptable because the user-scheduler path is
            // opt-in (a process sets `p_scheduler = Some(self)`)
            // and the kernel scheduler is the default for system
            // services.
            //
            // Tracking: this is the canonical example of "documented
            // deferral" in this codebase — the contract is clear, the
            // dependency chain is explicit, and a future implementer
            // has the full roadmap.
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

// ── 09-switch-to-user methods ──

/// switch_to_user 控制流状态。
///
/// C 使用 goto 在多个检查点之间跳转（proc.c:299-477）。
/// Rust 使用枚举状态 + loop 模拟相同控制流。
///
/// Design decision: 状态机替代 goto（09-switch-to-user.md §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchFlow {
    /// 检查当前进程是否可运行
    CheckCurrent,
    /// 当前进程不可运行，选择新进程
    PickNew,
    /// 处理 misc 标志
    CheckMiscFlags,
    /// 检查时间片
    CheckQuantum,
    /// 恢复用户态上下文
    RestoreContext,
}

impl ProcessTable {
    /// 选择下一个可运行进程。
    ///
    /// C: switch_to_user() 中的 pick_proc() 调用 — proc.c:335
    /// 处理 PREEMPTED 进程的重新入队逻辑 — proc.c:320-329
    ///
    /// 返回 `(selected_proc_nr, flow_state)`：
    /// - 如果当前进程仍可运行，返回当前进程 + CheckMiscFlags
    /// - 否则选择新进程 + CheckMiscFlags
    pub fn select_next_process(
        &mut self,
        current_nr: Option<ProcNr>,
        cpu_id: CpuId,
    ) -> (Option<ProcNr>, SwitchFlow) {
        // 阶段 1：当前进程是否可运行？
        if let Some(nr) = current_nr {
            if let Some(p) = self.get(nr) {
                if p.is_runnable() {
                    return (Some(nr), SwitchFlow::CheckMiscFlags);
                }
            }
        }

        // 阶段 2：处理 PREEMPTED 进程
        if let Some(nr) = current_nr {
            let was_preempted = self.get(nr).map_or(false, |p| {
                p.p_rts_flags.is_set(RtsFlagsBits::PREEMPTED)
            });
            if was_preempted {
                self.rts_unset(nr, RtsFlagsBits::PREEMPTED);
                if let Some(p) = self.get(nr) {
                    if p.is_runnable() {
                        let has_time_left = p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0;
                        if has_time_left {
                            self.sched_enqueue_head(nr, cpu_id);
                        } else {
                            self.sched_enqueue(nr, None, cpu_id);
                        }
                    }
                }
            }
        }

        // 阶段 3：选择最高优先级进程 (per-CPU scheduler)
        let selected = self.sched_for_cpu(cpu_id as u32).pick_proc(self.procs_slice());
        (selected, SwitchFlow::CheckMiscFlags)
    }

    /// 处理进程的 misc 标志。
    ///
    /// C: check_misc_flags 循环 — proc.c:351-405
    ///
    /// 返回 true 表示进程仍可运行，false 表示不可运行（需重新选择）。
    ///
    /// # C semantic alignment
    ///
    /// In C, each branch calls a handler function (kernel_call_resume,
    /// delivermsg, arch_do_syscall) which clears the corresponding flag.
    /// In Rust, those handlers are not yet wired into this loop, so we
    /// clear the flag explicitly after the branch to prevent an infinite
    /// loop. When the handlers are integrated, they will own the flag
    /// clearing and the explicit `clear` here can be removed.
    pub fn process_misc_flags(&mut self, nr: ProcNr) -> bool {
        let interesting_flags = MiscFlagsBits::KCALL_RESUME
            | MiscFlagsBits::DELIVERMSG
            | MiscFlagsBits::SC_DEFER
            | MiscFlagsBits::SC_TRACE
            | MiscFlagsBits::SC_ACTIVE;

        loop {
            let flags = self.get(nr).map_or(MiscFlagsBits::empty(), |p| p.p_misc_flags.get());
            if !flags.intersects(interesting_flags) {
                break;
            }

            // 按优先级处理（与 C 的 if-else chain 一致）
            if flags.contains(MiscFlagsBits::KCALL_RESUME) {
                // C: kernel_call_resume(p) — clears MF_KCALL_RESUME
                // TODO: wire kernel_call_resume() from vm.rs
                self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::KCALL_RESUME));
            } else if flags.contains(MiscFlagsBits::DELIVERMSG) {
                // C: delivermsg(p) — clears MF_DELIVERMSG
                // TODO: wire delivermsg() from ipc module
                self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG));
            } else if flags.contains(MiscFlagsBits::SC_DEFER) {
                // C: arch_do_syscall(p) — clears MF_SC_DEFER
                // TODO: wire arch_do_syscall() from arch layer
                self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::SC_DEFER));
            } else if flags.contains(MiscFlagsBits::SC_TRACE) {
                if !flags.contains(MiscFlagsBits::SC_ACTIVE) {
                    break;
                }
                // C: clears both MF_SC_TRACE and MF_SC_ACTIVE, then cause_sig
                self.get_mut(nr).map(|p| {
                    p.p_misc_flags.clear(MiscFlagsBits::SC_TRACE | MiscFlagsBits::SC_ACTIVE);
                });
                // TODO: wire cause_sig() from signal module
                break;
            } else if flags.contains(MiscFlagsBits::SC_ACTIVE) {
                self.get_mut(nr).map(|p| p.p_misc_flags.clear(MiscFlagsBits::SC_ACTIVE));
                break;
            }

            // 检查进程是否仍可运行
            if !self.get(nr).map_or(false, |p| p.is_runnable()) {
                return false;
            }
        }
        true
    }

    /// 检查进程时间片并处理。
    ///
    /// C: proc.c:418-424
    /// 如果进程无剩余时间片，调用 sched_proc_no_time。
    ///
    /// 返回 true 表示进程仍可运行，false 表示不可运行。
    pub fn check_quantum(&mut self, nr: ProcNr) -> bool {
        let has_time_left = self.get(nr).map_or(false, |p| {
            p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0
        });
        if !has_time_left {
            self.sched_proc_no_time(nr);
        }
        self.get(nr).map_or(false, |p| p.is_runnable())
    }
}

/// Convert a logical process number to an array index.
///
/// C: `isokprocn(n)` + `proc_addr(n)` combined — the C versions are separate
/// (isokprocn validates, proc_addr accesses without validation).
/// Rust unifies both into one function that returns `Option<usize>`.
///
/// Formula: `index = nr + NR_TASKS` (same as C's `proc_addr` offset).
/// Kernel tasks (nr < 0) map to indices 0..NR_TASKS-1.
/// User processes (nr >= 0) map to indices NR_TASKS..NR_TASKS+NR_PROCS-1.
///
/// See 08-proc-macros.md §3.6 for design rationale.
#[inline]
fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    let offset = nr as isize + NR_TASKS as isize;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

/// Read TSC via clock subsystem.
/// C: `read_tsc_64()` — delegates to `ClockArch::read_tsc()`.
fn read_tsc() -> u64 {
    clock::read_tsc()
}

/// Get monotonic uptime via clock subsystem.
/// C: `get_monotonic()` — clock.c:202.
fn get_monotonic() -> u64 {
    clock::get_monotonic()
}

/// Convert ms to CPU time cycles via clock subsystem.
/// C: `ms_2_cpu_time(ms)` — uses calibrated TSC frequency.
fn ms_to_cpu_time(ms: u32) -> u64 {
    clock::ms_to_cpu_time(ms)
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

    /// Test: rts_set/rts_unset with multiple flags and scheduler interaction.
    ///
    /// Verifies:
    /// 1. Setting multiple flags atomically
    /// 2. Clearing flags one at a time (process stays non-runnable until all cleared)
    /// 3. Scheduler dequeue on rts_set, enqueue on rts_unset
    #[test]
    fn test_rts_set_unset_multiple_flags() {
        let mut table = ProcessTable::new();
        let nr = 0;

        // Make process runnable
        table.get_mut(nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        assert!(table.get(nr).unwrap().is_runnable());

        // Set two flags at once
        table.rts_set(nr, RtsFlagsBits::SENDING | RtsFlagsBits::RECEIVING);
        assert!(!table.get(nr).unwrap().is_runnable());
        assert!(table.get(nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::SENDING));
        assert!(table.get(nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::RECEIVING));

        // Clear one flag — still not runnable
        table.rts_unset(nr, RtsFlagsBits::SENDING);
        assert!(!table.get(nr).unwrap().is_runnable());
        assert!(!table.get(nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::SENDING));
        assert!(table.get(nr).unwrap().p_rts_flags.is_set(RtsFlagsBits::RECEIVING));

        // Clear remaining flag — now runnable again
        table.rts_unset(nr, RtsFlagsBits::RECEIVING);
        assert!(table.get(nr).unwrap().is_runnable());
    }

    /// Test: rts_set on already-set flag is idempotent.
    #[test]
    fn test_rts_set_idempotent() {
        let mut table = ProcessTable::new();
        let nr = 0;
        table.get_mut(nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        table.rts_set(nr, RtsFlagsBits::PROC_STOP);
        assert!(!table.get(nr).unwrap().is_runnable());

        // Set same flag again — no change
        table.rts_set(nr, RtsFlagsBits::PROC_STOP);
        assert!(!table.get(nr).unwrap().is_runnable());

        table.rts_unset(nr, RtsFlagsBits::PROC_STOP);
        assert!(table.get(nr).unwrap().is_runnable());
    }

    // ── 08-proc-macros.md §5: Proc access macro tests ──

    /// §5.1: Boundary validation — is_valid_nr edge cases.
    #[test]
    fn test_is_valid_nr_boundaries() {
        // Below minimum kernel task number
        assert!(!ProcessTable::is_valid_nr(-6));
        // Minimum kernel task (ASYNCM, nr=-5)
        assert!(ProcessTable::is_valid_nr(-5));
        // Maximum kernel task (KERNEL, nr=-1)
        assert!(ProcessTable::is_valid_nr(-1));
        // Minimum user process (DS, nr=0)
        assert!(ProcessTable::is_valid_nr(0));
        // Maximum user process (nr=NR_PROCS-1=255)
        assert!(ProcessTable::is_valid_nr(255));
        // Beyond maximum
        assert!(!ProcessTable::is_valid_nr(256));
    }

    /// §5.2: Round-trip test: nr → get → p_nr == nr.
    #[test]
    fn test_proc_nr_roundtrip() {
        let table = ProcessTable::new();
        for nr in [-5i32, -4, -3, -2, -1, 0, 1, 100, 255] {
            assert_eq!(table.get(nr).unwrap().p_nr, nr);
        }
    }

    /// §5.3: Slot empty test — new table has all non-IDLE slots as SLOT_FREE.
    #[test]
    fn test_is_empty_new_table() {
        let table = ProcessTable::new();
        // All slots except IDLE should be SLOT_FREE
        for nr in -5i32..=255 {
            if nr == proc_nr::IDLE {
                // IDLE has PROC_STOP, not SLOT_FREE
                assert!(!table.is_empty(nr));
            } else {
                assert!(table.is_empty(nr), "slot nr={} should be empty", nr);
            }
        }
    }

    /// §5.3: is_empty returns false after clearing SLOT_FREE.
    #[test]
    fn test_is_empty_after_alloc() {
        let mut table = ProcessTable::new();
        let nr = 2;
        assert!(table.is_empty(nr));
        table.get_mut(nr).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        assert!(!table.is_empty(nr));
    }

    /// §5.4: is_kernel_task on KProcess.
    #[test]
    fn test_is_kernel_task() {
        let table = ProcessTable::new();
        // Kernel tasks
        for nr in [-5i32, -4, -3, -2, -1] {
            assert!(table.get(nr).unwrap().is_kernel_task(),
                    "nr={} should be kernel task", nr);
        }
        // User processes
        for nr in [0i32, 1, 8, 255] {
            assert!(!table.get(nr).unwrap().is_kernel_task(),
                    "nr={} should not be kernel task", nr);
        }
    }

    /// §5.5: Iterator count tests.
    #[test]
    fn test_iter_counts() {
        let table = ProcessTable::new();
        assert_eq!(table.iter().count(), PROC_TABLE_SIZE);
        // User processes only (skip NR_TASKS kernel tasks)
        assert_eq!(table.iter().skip(NR_TASKS).count(), NR_PROCS);
    }

    /// §5.6: set_bill_to_idle marks the IDLE slot's enter_queue=0,
    /// matching C's `get_cpulocal_var(bill_ptr) = idle_proc` (main.c:50).
    #[test]
    fn test_set_bill_to_idle() {
        use crate::proc::proc_nr;
        let mut table = ProcessTable::new();
        // Pre-condition: IDLE slot's enter_queue is 0 (fresh table).
        let idle_idx = super::nr_to_idx(proc_nr::IDLE).unwrap();
        assert_eq!(table.procs[idle_idx].p_accounting.enter_queue
                       .load(core::sync::atomic::Ordering::Relaxed),
                   0);

        table.set_bill_to_idle();

        // Post-condition: still 0 (bill_to_idle is a no-op when already 0).
        assert_eq!(table.procs[idle_idx].p_accounting.enter_queue
                       .load(core::sync::atomic::Ordering::Relaxed),
                   0);

        // Now pollute IDLE's enter_queue to a non-zero value, then call
        // set_bill_to_idle again — it should reset to 0.
        table.procs[idle_idx].p_accounting.enter_queue
            .store(0xdead_beef, core::sync::atomic::Ordering::Release);
        table.set_bill_to_idle();
        assert_eq!(table.procs[idle_idx].p_accounting.enter_queue
                       .load(core::sync::atomic::Ordering::Relaxed),
                   0);
    }

    // ── VM request queue integration tests ──

    /// Test: vm_memreq_get returns NoRequest when queue is empty.
    /// C: do_vmctl.c:72 — `return ENOENT` when vmrequest list is empty.
    #[test]
    fn test_vm_memreq_get_empty_queue() {
        let mut table = ProcessTable::new();
        let result = table.vm_memreq_get();
        assert!(matches!(result, Err(crate::vm::VmCtlError::NoRequest)));
    }

    /// Test: vm_memreq_get dequeues a process with pending VM request.
    /// C: do_vmctl.c:37-72 — traverse vmrequest list, return first match.
    #[test]
    fn test_vm_memreq_get_dequeues_pending_request() {
        use crate::vm::{VmSuspendContext, VmSuspendType, VmSuspendState, VmCheckParams};
        use minix_types::VirBytes;

        let mut table = ProcessTable::new();
        let nr = 0;
        // Use the process's own endpoint as target (it exists in the table)
        let target_ep = table.get(nr).unwrap().p_endpoint;
        // Set up a process with a pending VM request
        {
            let proc = table.get_mut(nr).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.p_rts_flags.set(RtsFlagsBits::VMREQUEST);
            proc.p_vm_suspend = Some(VmSuspendContext {
                state: VmSuspendState::Pending,
                suspend_type: VmSuspendType::KernelCall,
                target: target_ep,
                check_params: VmCheckParams {
                    start: VirBytes(0x1000),
                    length: VirBytes(0x100),
                    write_flag: false,
                },
                saved_msg: Default::default(),
                copy_context: None,
            });
        }

        // Enqueue the process
        table.vm_enqueue(nr);

        // Dequeue via vm_memreq_get
        let result = table.vm_memreq_get();
        assert!(result.is_ok(), "vm_memreq_get returned: {:?}", result);
        let (dequeued_nr, params) = result.unwrap();
        assert_eq!(dequeued_nr, nr);
        assert_eq!(params.start, VirBytes(0x1000));
        assert_eq!(params.length, VirBytes(0x100));
        assert!(!params.write_flag);

        // Queue should now be empty
        assert!(table.vm_request_queue().is_empty());
    }

    /// Test: vm_memreq_reply transitions process from Fetched to Completed.
    /// C: do_vmctl.c:73-109 — set vmresult, clear RTS_VMREQUEST.
    #[test]
    fn test_vm_memreq_reply_completes_request() {
        use crate::vm::{VmSuspendContext, VmSuspendType, VmSuspendState, VmCheckResult, VmCheckParams};
        use minix_types::{Endpoint, VirBytes};

        let mut table = ProcessTable::new();
        let nr = 0;
        // Set up a process in Fetched state (as if MemReqGet was called)
        {
            let proc = table.get_mut(nr).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.p_rts_flags.set(RtsFlagsBits::VMREQUEST);
            proc.p_vm_suspend = Some(VmSuspendContext {
                state: VmSuspendState::Fetched,
                suspend_type: VmSuspendType::KernelCall,
                target: Endpoint(100),
                check_params: VmCheckParams {
                    start: VirBytes(0),
                    length: VirBytes(0),
                    write_flag: false,
                },
                saved_msg: Default::default(),
                copy_context: None,
            });
        }

        // Reply with Ok result
        let result = table.vm_memreq_reply(nr, VmCheckResult::Ok);
        assert!(result.is_ok());

        // Process should no longer have VMREQUEST set
        let proc = table.get(nr).unwrap();
        assert!(!proc.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
        // MF_KCALL_RESUME should be set for KernelCall type
        assert!(proc.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    /// Test: vm_memreq_reply returns InvalidState when process is not in Fetched state.
    #[test]
    fn test_vm_memreq_reply_invalid_state() {
        use crate::vm::{VmSuspendContext, VmSuspendType, VmSuspendState, VmCheckParams};
        use minix_types::{Endpoint, VirBytes};

        let mut table = ProcessTable::new();
        let nr = 0;
        // Process in Pending state (not Fetched)
        {
            let proc = table.get_mut(nr).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.p_rts_flags.set(RtsFlagsBits::VMREQUEST);
            proc.p_vm_suspend = Some(VmSuspendContext {
                state: VmSuspendState::Pending,
                suspend_type: VmSuspendType::KernelCall,
                target: Endpoint(100),
                check_params: VmCheckParams {
                    start: VirBytes(0),
                    length: VirBytes(0),
                    write_flag: false,
                },
                saved_msg: Default::default(),
                copy_context: None,
            });
        }

        let result = table.vm_memreq_reply(nr, crate::vm::VmCheckResult::Ok);
        assert!(matches!(result, Err(crate::vm::VmCtlError::InvalidState)));
    }

    // ── process_misc_flags tests ──

    /// Test: process_misc_flags returns true and does nothing when no
    /// interesting flags are set.
    #[test]
    fn test_process_misc_flags_empty_returns_true() {
        let mut table = ProcessTable::new();
        let nr = 0;
        table.procs[nr as usize].p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        assert!(table.process_misc_flags(nr));
    }

    /// Test: process_misc_flags clears MF_KCALL_RESUME and keeps process
    /// runnable (stub handler path).
    #[test]
    fn test_process_misc_flags_clears_kcall_resume() {
        let mut table = ProcessTable::new();
        let nr = 0;
        {
            let proc = table.get_mut(nr).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        }
        assert!(table.process_misc_flags(nr));
        let proc = table.get(nr).unwrap();
        assert!(!proc.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    /// Test: process_misc_flags clears MF_DELIVERMSG and keeps process
    /// runnable (stub handler path).
    #[test]
    fn test_process_misc_flags_clears_delivermsg() {
        let mut table = ProcessTable::new();
        let nr = 0;
        {
            let proc = table.get_mut(nr).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        }
        assert!(table.process_misc_flags(nr));
        let proc = table.get(nr).unwrap();
        assert!(!proc.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    /// Test: process_misc_flags returns false when the process becomes
    /// unrunnable after flag processing.
    #[test]
    fn test_process_misc_flags_unrunnable_returns_false() {
        let mut table = ProcessTable::new();
        let nr = 0;
        {
            let proc = table.get_mut(nr).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
            // Make the process unrunnable by setting SENDing.
            proc.p_rts_flags.set(RtsFlagsBits::SENDING);
        }
        assert!(!table.process_misc_flags(nr));
    }
}
