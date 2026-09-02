//! PID generator implementation.
//!
//! This is the Rust rewrite of Minix3's `get_free_pid` function.
//!
//! # Design Approach
//!
//! Uses **Approach 5: Monotonic increment + local validation**, combining Minix's original spirit with modern Rust syntax.
//!
//! # Core Idea
//!
//! ```text
//! next_pid += 1
//! Only scan mproc on conflict
//! ```
//!
//! # Complexity Analysis
//!
//! - **Expected complexity**: O(1)
//!   - Conflict probability = NR_PROCS / NR_PIDS ≈ 256 / 30000 ≈ 0.8%
//!   - In 99.2% of cases, the first candidate PID has no conflict
//! - **Worst-case complexity**: O(N) (extremely rare)
//!
//! # Minix3 Source Mapping
//!
//! ```c
//! // minix3/minix/servers/pm/utility.c:34-52
//! pid_t get_free_pid()
//! {
//!   static pid_t next_pid = INIT_PID + 1;
//!   register struct mproc *rmp;
//!   int t;
//!
//!   do {
//!     t = 0;
//!     next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
//!     for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
//!       if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
//!         t = 1;
//!         break;
//!       }
//!   } while (t);
//!
//!   return(next_pid);
//! }
//! ```
//!
//! # ARCH 行为差异（03-mproc-table.md §3.4）
//!
//! C 扫描**全部**槽位（`for rmp = &mproc[0]; rmp < &mproc[NR_PROCS]`），
//! 释放槽的陈旧 `mp_procgrp` 会造成额外跳过；Rust 只扫活进程
//! （[`ProcTable::iter_active`]）——对活进程的 PID 唯一性契约两边一致，
//! 环绕边界上"具体选中哪个 PID"可能有差异，不构成外部语义变化。
//!
//! # Single Source of Truth Principle
//!
//! This implementation follows the **Single Source of Truth** principle:
//! - No bitmap, all state is in the `mproc` table
//! - Avoids complexity of state synchronization
//! - Never has inconsistency like "bitmap says free but process table says occupied"

use core::cell::Cell;
use minix_types::Pid;
use crate::mproc::{ProcTable, NR_PIDS, INIT_PID};

/// PID generator.
///
/// Uses monotonic increment + conflict detection strategy.
///
/// # Design Philosophy
///
/// Leverages `NR_PIDS >> NR_PROCS` characteristic to guarantee expected complexity O(1).
/// No bitmap needed, avoids state synchronization complexity (Single Source of Truth).
///
/// # Thread Safety
///
/// Uses `Cell<Pid>` for interior mutability. Safe in PM's single-threaded environment.
/// If PM becomes multi-threaded in the future, need to switch to `AtomicI32`.
#[derive(Debug)]
pub struct PidGenerator {
    /// Next candidate PID.
    ///
    /// Initial value is `INIT_PID + 1 = 2`
    next_pid: Cell<Pid>,
}

impl PidGenerator {
    /// Creates a new PID generator.
    ///
    /// Initial `next_pid` is `INIT_PID + 1 = 2`
    pub const fn new() -> Self {
        Self {
            next_pid: Cell::new(INIT_PID + 1),
        }
    }

    /// Gets a free PID.
    ///
    /// # Algorithm
    ///
    /// 1. Candidate PID = next_pid++ (monotonic increment, wrap around)
    /// 2. Check if candidate PID conflicts with any process's PID or process group ID
    /// 3. If no conflict, return; if conflict, go back to step 1
    ///
    /// # Conflict Detection Rules
    ///
    /// Minix3 rule: PID cannot be the same as any process's `mp_pid` or `mp_procgrp`.
    ///
    /// Why check `mp_procgrp`?
    /// - `mp_procgrp` is the process group ID, usually equals the process group leader's PID
    /// - If a process is a process group leader, its `mp_procgrp == mp_pid`
    /// - If a process joins a process group, its `mp_procgrp` equals the leader's PID
    /// - Therefore, PID cannot conflict with any process's `mp_pid` or `mp_procgrp`
    ///
    /// # Complexity
    ///
    /// - Expected: O(1) (because conflict probability is extremely low, ~0.8%)
    /// - Worst-case: O(N) (extremely rare)
    ///
    /// # Minix3 Mapping
    ///
    /// ```c
    /// do {
    ///     t = 0;
    ///     next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
    ///     for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
    ///         if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
    ///             t = 1;
    ///             break;
    ///         }
    /// } while (t);
    /// ```
    pub fn get_free_pid(&self, table: &ProcTable) -> Pid {
        loop {
            let candidate = self.next_pid.get();

            let next = if candidate < NR_PIDS {
                candidate + 1
            } else {
                INIT_PID + 1
            };
            self.next_pid.set(next);

            if !self.any_conflict(candidate, table) {
                return candidate;
            }
        }
    }

    /// Checks if candidate PID conflicts with existing processes.
    ///
    /// Minix3 rule: PID cannot be the same as any process's `mp_pid` or `mp_procgrp`.
    ///
    /// # Implementation Details
    ///
    /// Uses Rust iterator's `any` method with these advantages:
    /// - **Lazy evaluation**: In 99.2% of cases, the loop won't execute at all
    /// - **Short-circuit evaluation**: Returns immediately upon finding a conflict
    /// - **Clear semantics**: Code directly expresses "check for conflicts" intent
    fn any_conflict(&self, candidate: Pid, table: &ProcTable) -> bool {
        table.iter_active().any(|proc| {
            proc.pid() == candidate || proc.procgrp() == candidate
        })
    }

    /// Resets PID generator (test only).
    #[cfg(test)]
    pub fn reset(&self) {
        self.next_pid.set(INIT_PID + 1);
    }
}

impl Default for PidGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::Lifecycle;
    extern crate std;
    use std::collections::HashSet;

    fn create_test_table() -> ProcTable {
        ProcTable::new()
    }

    #[test]
    fn test_pid_generator_new() {
        let generator = PidGenerator::new();
        assert_eq!(generator.next_pid.get(), INIT_PID + 1);
    }

    #[test]
    fn test_pid_first_allocation() {
        let generator = PidGenerator::new();
        let table = create_test_table();

        let pid = generator.get_free_pid(&table);
        assert_eq!(pid, INIT_PID + 1);
    }

    #[test]
    fn test_pid_uniqueness() {
        let generator = PidGenerator::new();
        let table = create_test_table();

        let mut pids = HashSet::new();
        for _ in 0..100 {
            let pid = generator.get_free_pid(&table);
            assert!(pids.insert(pid), "PID {} already allocated", pid);
        }
    }

    #[test]
    fn test_pid_wrap_around() {
        let generator = PidGenerator::new();
        generator.next_pid.set(NR_PIDS - 1);

        let table = create_test_table();

        let pid1 = generator.get_free_pid(&table);
        assert_eq!(pid1, NR_PIDS - 1);

        let pid2 = generator.get_free_pid(&table);
        assert_eq!(pid2, NR_PIDS);

        let pid3 = generator.get_free_pid(&table);
        assert_eq!(pid3, INIT_PID + 1);
    }

    #[test]
    fn test_pid_conflict_detection() {
        let generator = PidGenerator::new();
        let mut table = create_test_table();

        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.id.pid = 2;
        table.procs[0].identity.procgrp = 2;

        let pid = generator.get_free_pid(&table);
        assert_ne!(pid, 2);
        assert!(pid >= INIT_PID + 1 && pid <= NR_PIDS);
    }

    #[test]
    fn test_pid_procgrp_conflict() {
        let generator = PidGenerator::new();
        let mut table = create_test_table();

        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.id.pid = 100;
        table.procs[0].identity.procgrp = 3;

        generator.next_pid.set(3);
        let pid = generator.get_free_pid(&table);
        assert_ne!(pid, 3);
    }

    #[test]
    fn test_pid_range() {
        let generator = PidGenerator::new();
        let table = create_test_table();

        for _ in 0..1000 {
            let pid = generator.get_free_pid(&table);
            assert!(pid > INIT_PID && pid <= NR_PIDS,
                "PID {} out of range [{}, {}]", pid, INIT_PID + 1, NR_PIDS);
        }
    }

    #[test]
    fn test_released_slot_stale_procgrp_not_conflict() {
        // ARCH 差异（03-mproc-table.md §3.4）：C 扫描全部槽位，释放槽的
        // 陈旧 mp_procgrp 会造成额外跳过；Rust 只扫活进程（iter_active）——
        // 对活进程的唯一性契约不变。
        let generator = PidGenerator::new();
        let mut table = create_test_table();
        generator.next_pid.set(7);

        // 未使用槽位持有陈旧 procgrp=7：不参与冲突检测。
        table.procs[0].identity.procgrp = 7;
        let pid = generator.get_free_pid(&table);
        assert_eq!(pid, 7);
    }
}
