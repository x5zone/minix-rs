//! PID 生成器实现
//!
//! 这是 Minix3 `get_free_pid` 函数的 Rust 重写版本。
//!
//! # 设计方案
//!
//! 采用**方案五：单调递增 + 局部验证**，这是结合 Minix 原版精神和 Rust 现代语法的最优方案。
//!
//! # 核心思想
//!
//! ```text
//! next_pid += 1
//! 只在冲突时 scan mproc
//! ```
//!
//! # 复杂度分析
//!
//! - **期望复杂度**: O(1)
//!   - 冲突概率 = NR_PROCS / NR_PIDS ≈ 256 / 30000 ≈ 0.8%
//!   - 99.2% 的情况下，第一个候选 PID 就没有冲突
//! - **最坏复杂度**: O(N)（极罕见）
//!
//! # Minix3 源码映射
//!
//! ```c
//! // minix3/minix/servers/pm/utility.c (第 32-52 行)
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
//! # 单一真理来源原则
//!
//! 此实现遵循**单一真理来源（Single Source of Truth）**原则：
//! - 没有位图，所有状态都在 `mproc` 表中
//! - 避免了状态同步的复杂性
//! - 永远不会出现"位图说空闲但进程表说占用"的不一致问题

use core::cell::Cell;
use minix_types::Pid;
use crate::mproc::{ProcTable, NR_PIDS, INIT_PID};

/// PID 生成器
///
/// 采用单调递增 + 冲突检测的策略。
///
/// # 设计哲学
///
/// 利用 `NR_PIDS >> NR_PROCS` 的特性，保证期望复杂度 O(1)。
/// 无需位图，避免了状态同步的复杂性（Single Source of Truth）。
///
/// # 线程安全
///
/// 使用 `Cell<Pid>` 实现内部可变性。在 PM 单线程环境中是安全的。
/// 如果未来 PM 变成多线程，需要改用 `AtomicI32`。
#[derive(Debug)]
pub struct PidGenerator {
    /// 下一个候选 PID
    ///
    /// 初始值为 `INIT_PID + 1 = 2`
    next_pid: Cell<Pid>,
}

impl PidGenerator {
    /// 创建新的 PID 生成器
    ///
    /// 初始 `next_pid` 为 `INIT_PID + 1 = 2`
    pub const fn new() -> Self {
        Self {
            next_pid: Cell::new(INIT_PID + 1),
        }
    }

    /// 获取一个空闲的 PID
    ///
    /// # 算法逻辑
    ///
    /// 1. 候选 PID = next_pid++ （单调递增，循环复用）
    /// 2. 检查候选 PID 是否与任何进程的 PID 或进程组 ID 冲突
    /// 3. 无冲突则返回；有冲突则回到第 1 步
    ///
    /// # 冲突检测规则
    ///
    /// Minix3 规则：PID 不能与任何进程的 `mp_pid` 或 `mp_procgrp` 相同。
    ///
    /// 为什么需要检查 `mp_procgrp`？
    /// - `mp_procgrp` 是进程组 ID，通常等于进程组组长的 PID
    /// - 如果一个进程是进程组组长，它的 `mp_procgrp == mp_pid`
    /// - 如果一个进程加入了某个进程组，它的 `mp_procgrp` 等于组长的 PID
    /// - 因此，PID 不能与任何进程的 `mp_pid` 或 `mp_procgrp` 冲突
    ///
    /// # 复杂度
    ///
    /// - 期望: O(1) (因为冲突概率极低，约 0.8%)
    /// - 最坏: O(N) (极罕见)
    ///
    /// # Minix3 映射
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

    /// 检查候选 PID 是否与现有进程冲突
    ///
    /// Minix3 规则：PID 不能与任何进程的 `mp_pid` 或 `mp_procgrp` 相同。
    ///
    /// # 实现细节
    ///
    /// 使用 Rust 迭代器的 `any` 方法，具有以下优势：
    /// - **惰性求值**：在 99.2% 的情况下，循环根本不会执行
    /// - **短路求值**：一旦发现冲突，立刻返回
    /// - **语义清晰**：代码直接表达了"检查是否有冲突"的意图
    fn any_conflict(&self, candidate: Pid, table: &ProcTable) -> bool {
        table.iter_active().any(|proc| {
            proc.pid() == candidate || proc.procgrp() == candidate
        })
    }

    /// 重置 PID 生成器（仅用于测试）
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
}
