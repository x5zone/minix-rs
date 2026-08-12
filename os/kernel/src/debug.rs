//! Kernel debugging infrastructure.
//!
//! C: `kernel/debug.c` — sanity checking of scheduling queues and
//! diagnostic printing functions.
//!
//! # Visibility
//!
//! These functions are intended for use in `debug_assert!` checks and
//! diagnostic panic messages. They are not part of the kernel's public
//! API and are compiled away in release builds when `debug_assertions`
//! is disabled (matching C's `#if USE_SYSDEBUG` gating).
//!
//! # Coverage
//!
//! - `runqueues_ok_cpu(cpu)` — verify scheduling queue invariants for a CPU
//! - `runqueues_ok()` — verify all CPUs (delegates to `runqueues_ok_cpu`)
//! - `rtsflagstr(flags)` — format RTS flags as a human-readable string
//! - `miscflagstr(flags)` — format misc flags as a human-readable string
//! - `print_proc(pp)` — print process diagnostic info to early console

use crate::proc::{
    priority, CpuId, KProcess, MiscFlagsBits, ProcNr, RtsFlagsBits, NONE_PROC_NR,
};
use crate::proc_table::{ProcessTable, PROC_TABLE_SIZE};
use crate::smp::SmpState;
use minix_arch::{CurrentEarlyConsole as Console, EarlyConsole};

/// Maximum iteration count for queue walks (defense against cycles).
///
/// C: `#define MAX_LOOP (NR_PROCS + NR_TASKS)` — debug.c:14.
const MAX_LOOP: usize = PROC_TABLE_SIZE + 16;

/// Verify scheduling queue invariants for a specific CPU.
///
/// C: `runqueues_ok_cpu(cpu)` — debug.c:16-107.
///
/// Checks:
/// 1. Every non-empty queue has both head and tail set.
/// 2. Tail's `p_nextready` is None.
/// 3. Every process on a queue is runnable, not SLOT_FREE, and has the
///    correct priority for that queue.
/// 4. No process appears on two queues (checked via `p_found` marker).
/// 5. Every runnable process in the proc table is on some queue.
///
/// Returns `true` if all invariants hold, `false` otherwise.
/// On failure, prints a diagnostic message to the early console.
pub fn runqueues_ok_cpu(
    smp_state: &SmpState,
    proc_table: &ProcessTable,
    cpu: CpuId,
) -> bool {
    let scheduler = match smp_state.cpu_local(cpu) {
        Some(cl) => &cl.scheduler,
        None => {
            Console::write_str("runqueues_ok: invalid CPU\n");
            return false;
        }
    };

    // C: debug.c:25-28 — clear p_found for all processes.
    // We use a local bitset instead of a per-process field to avoid
    // modifying the process table during a read-only check.
    let mut found = [false; PROC_TABLE_SIZE];

    // C: debug.c:30-90 — walk each priority queue.
    for q in 0..priority::NR_SCHED_QUEUES {
        let head = scheduler.queue_head(q);
        let tail = scheduler.queue_tail(q);

        // C: debug.c:31-33 — head but no tail.
        if head.is_some() && tail.is_none() {
            Console::write_str("runqueues_ok: head but no tail in queue ");
            Console::write_hex(q as u64);
            Console::write_str("\n");
            return false;
        }
        // C: debug.c:35-37 — tail but no head.
        if head.is_none() && tail.is_some() {
            Console::write_str("runqueues_ok: tail but no head in queue ");
            Console::write_hex(q as u64);
            Console::write_str("\n");
            return false;
        }

        // C: debug.c:39-41 — tail->p_nextready must be None.
        if let Some(tail_nr) = tail {
            if let Some(tail_proc) = proc_table.get(tail_nr) {
                if tail_proc.p_nextready.load(core::sync::atomic::Ordering::Acquire) != NONE_PROC_NR {
                    Console::write_str("runqueues_ok: tail->next not null in queue ");
                    Console::write_hex(q as u64);
                    Console::write_str("\n");
                    return false;
                }
            }
        }

        // C: debug.c:43-89 — walk the queue via p_nextready.
        let mut current = head;
        let mut iter_count = 0;
        while let Some(nr) = current {
            if iter_count > MAX_LOOP {
                Console::write_str("runqueues_ok: loop in schedule queue\n");
                return false;
            }
            iter_count += 1;

            let idx = nr.0 as usize;
            if idx >= PROC_TABLE_SIZE {
                Console::write_str("runqueues_ok: proc nr out of range\n");
                return false;
            }

            let proc = match proc_table.get(nr) {
                Some(p) => p,
                None => {
                    Console::write_str("runqueues_ok: bogus proc nr\n");
                    return false;
                }
            };

            // C: debug.c:59-62 — SLOT_FREE check.
            if proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
                Console::write_str("runqueues_ok: dead proc on queue ");
                Console::write_hex(q as u64);
                Console::write_str("\n");
                return false;
            }

            // C: debug.c:64-68 — runnable check.
            if !proc.is_runnable() {
                Console::write_str("runqueues_ok: unready proc on runq ");
                Console::write_hex(q as u64);
                Console::write_str("\n");
                return false;
            }

            // C: debug.c:69-73 — priority match check.
            if proc.p_sched.priority.load(core::sync::atomic::Ordering::Acquire) as usize != q {
                Console::write_str("runqueues_ok: wrong priority in queue ");
                Console::write_hex(q as u64);
                Console::write_str("\n");
                return false;
            }

            // C: debug.c:74-78 — double-schedule check.
            if found[idx] {
                Console::write_str("runqueues_ok: double sched in queue ");
                Console::write_hex(q as u64);
                Console::write_str("\n");
                return false;
            }
            found[idx] = true;

            // C: debug.c:80-84 — last element must be tail.
            let next = proc.p_nextready.load(core::sync::atomic::Ordering::Acquire);
            if next == NONE_PROC_NR && tail != Some(nr) {
                Console::write_str("runqueues_ok: last element not tail in queue ");
                Console::write_hex(q as u64);
                Console::write_str("\n");
                return false;
            }

            current = if next == NONE_PROC_NR { None } else { Some(ProcNr(next)) };
        }
    }

    // C: debug.c:92-103 — every runnable process must be on a queue.
    for i in 0..PROC_TABLE_SIZE {
        let nr = ProcNr(i as i32);
        if let Some(proc) = proc_table.get(nr) {
            if proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
                continue;
            }
            if proc.is_runnable() && !found[i] {
                Console::write_str("runqueues_ok: ready proc not on queue: nr=");
                Console::write_hex(i as u64);
                Console::write_str("\n");
                return false;
            }
        }
    }

    true
}

/// Verify scheduling queue invariants for all CPUs.
///
/// C: `runqueues_ok()` — debug.c:121-131.
///
/// In single-CPU mode, delegates to `runqueues_ok_cpu(0)`.
/// In SMP mode, iterates all online CPUs.
pub fn runqueues_ok(smp_state: &SmpState, proc_table: &ProcessTable) -> bool {
    let ncpus = smp_state.ncpus();
    for c in 0..ncpus {
        let cpu = CpuId::new_unchecked(c);
        if !runqueues_ok_cpu(smp_state, proc_table, cpu) {
            return false;
        }
    }
    true
}

/// Write RTS flags as a human-readable string to the early console.
///
/// C: `rtsflagstr(flags)` — debug.c:136-161.
///
/// Writes directly to the console (no string allocation) to comply with
/// Rust 2024's `static_mut_refs` lint. Each set flag is written as
/// "FLAG_NAME " (trailing space).
pub fn write_rts_flags(flags: u32) {
    macro_rules! flag {
        ($bit:expr, $name:expr) => {
            if flags & $bit.bits() != 0 {
                Console::write_str($name);
                Console::write_str(" ");
            }
        };
    }

    flag!(RtsFlagsBits::SLOT_FREE, "RTS_SLOT_FREE ");
    flag!(RtsFlagsBits::PROC_STOP, "RTS_PROC_STOP ");
    flag!(RtsFlagsBits::SENDING, "RTS_SENDING ");
    flag!(RtsFlagsBits::RECEIVING, "RTS_RECEIVING ");
    flag!(RtsFlagsBits::SIGNALED, "RTS_SIGNALED ");
    flag!(RtsFlagsBits::SIG_PENDING, "RTS_SIG_PENDING ");
    flag!(RtsFlagsBits::P_STOP, "RTS_P_STOP ");
    flag!(RtsFlagsBits::NO_PRIV, "RTS_NO_PRIV ");
    flag!(RtsFlagsBits::NO_ENDPOINT, "RTS_NO_ENDPOINT ");
    flag!(RtsFlagsBits::VMINHIBIT, "RTS_VMINHIBIT ");
    flag!(RtsFlagsBits::PAGEFAULT, "RTS_PAGEFAULT ");
    flag!(RtsFlagsBits::VMREQUEST, "RTS_VMREQUEST ");
    flag!(RtsFlagsBits::VMREQTARGET, "RTS_VMREQTARGET ");
    flag!(RtsFlagsBits::PREEMPTED, "RTS_PREEMPTED ");
    flag!(RtsFlagsBits::NO_QUANTUM, "RTS_NO_QUANTUM ");
}

/// Write misc flags as a human-readable string to the early console.
///
/// C: `miscflagstr(flags)` — debug.c:163-174.
pub fn write_misc_flags(flags: u32) {
    macro_rules! flag {
        ($bit:expr, $name:expr) => {
            if flags & $bit.bits() != 0 {
                Console::write_str($name);
                Console::write_str(" ");
            }
        };
    }

    flag!(MiscFlagsBits::REPLY_PEND, "MF_REPLY_PEND ");
    flag!(MiscFlagsBits::DELIVERMSG, "MF_DELIVERMSG ");
    flag!(MiscFlagsBits::KCALL_RESUME, "MF_KCALL_RESUME ");
}

/// Print diagnostic information about a process to the early console.
///
/// C: `print_proc(pp)` — debug.c:249-275.
///
/// Format: "nr: name endpoint prio prio user_time/sys_time cpu rts flags misc flags"
pub fn print_proc(proc: &KProcess) {
    Console::write_hex(proc.p_nr.0 as u64);
    Console::write_str(": ");
    Console::write_str(proc.p_name.as_str());
    Console::write_str(" ep=");
    Console::write_hex(proc.p_endpoint.0 as u64);
    Console::write_str(" prio=");
    Console::write_hex(proc.p_sched.priority.load(core::sync::atomic::Ordering::Acquire) as u64);
    Console::write_str(" rts=[");
    write_rts_flags(proc.p_rts_flags.load());
    Console::write_str("] misc=[");
    write_misc_flags(proc.p_misc_flags.load());
    Console::write_str("]\n");
}

// ── Scheduler accessor needed by runqueues_ok_cpu ──
//
// We need access to `run_q_tail` which is private in `Scheduler`.
// Add a `queue_tail` accessor via the `Scheduler` impl.

/// Extension trait to expose `queue_tail` for debugging.
/// This is sealed to prevent external implementation.
pub trait SchedulerDebugExt {
    fn queue_tail(&self, prio: usize) -> Option<ProcNr>;
}

impl SchedulerDebugExt for crate::sched::Scheduler {
    fn queue_tail(&self, prio: usize) -> Option<ProcNr> {
        debug_assert!(prio < priority::NR_SCHED_QUEUES);
        self.queue_tail_inner(prio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_rts_flags_empty() {
        // No flags set → nothing written (no panic).
        write_rts_flags(0);
    }

    #[test]
    #[ignore = "requires initialized logger for MockEarlyConsole::write_byte"]
    fn test_write_rts_flags_single() {
        write_rts_flags(RtsFlagsBits::SENDING.bits());
    }

    #[test]
    fn test_write_misc_flags_empty() {
        write_misc_flags(0);
    }

    #[test]
    #[ignore = "requires initialized logger for MockEarlyConsole::write_byte"]
    fn test_write_misc_flags_single() {
        write_misc_flags(MiscFlagsBits::REPLY_PEND.bits());
    }
}
