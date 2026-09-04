//! SCHED queue balance: demote fast, restore slowly.
//!
//! Mirrors the balance arm (`minix3/minix/servers/sched/
//! schedule.c:16-18,334-369`): the five-second timer (`init_scheduling`)
//! and the one-level-per-round restore (`balance_queues`).
//! 11-balance-queues.md.
//!
//! The module owns the value and the verdict, nothing else: how long to
//! wait (`Balancer`) and whether a priority deserves one step back up
//! (`rebalance_one`). The table walk, the reissue (`PRIO|QUANTUM`, 09),
//! and the re-arming bell stay caller-side — the caller holds the table
//! (04 D2's precedent: doors judge, holders act), this module only names
//! the wait and the verdict.
//!
//! Single-threaded event loop: pure functions, no shared state.

use crate::schedproc::Priority;

/// How often to balance queues, in seconds (`schedule.c:18`).
///
/// The human unit; ticks are the clock's. The wait itself
/// ([`Balancer::timeout_ticks`]) is that count times the clock rate.
pub const BALANCE_TIMEOUT_SECS: u32 = 5;

/// The balance wait (`schedule.c:16,338`).
///
/// C keeps `static unsigned balance_timeout`: seconds times `sys_hz()`,
/// armed once at birth (`init_scheduling`) and re-armed after every
/// round (`balance_queues`). The Rust form holds the computed wait as a
/// value — arming the bell (`sys_setalarm`, 02's loop) and its fatal
/// policy (C `panic`s, `340-341,367-368`) stay caller-side.
///
/// [ARCH S-9] (plan.md): this struct *is* the default policy C's comment
/// (`348-352`, "will soon be changed") promises to replace. The second
/// policy arrives as a trait then — not one day sooner (pattern 25: one
/// behaviour needs no polymorphism).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balancer {
    /// Ticks to wait between rounds. C: `balance_timeout` — schedule.c:16.
    timeout_ticks: u32,
}

impl Balancer {
    /// Compute the wait from the clock rate (`schedule.c:338`).
    ///
    /// `hz` is C's `sys_hz()` (`sysutil.h:60`, ticks per second).
    /// Saturating: on legal rates this equals C's `5 * hz` exactly; on
    /// a wild rate it clamps instead of wrapping the wait to zero —
    /// a zero wait would ring the bell without pause (10 D3's precedent:
    /// the wrap is C's accident, not its contract).
    pub fn init(hz: u32) -> Self {
        Self {
            timeout_ticks: BALANCE_TIMEOUT_SECS.saturating_mul(hz),
        }
    }

    /// Read the wait back, in ticks.
    pub const fn timeout_ticks(&self) -> u32 {
        self.timeout_ticks
    }
}

/// One round's verdict for one priority (`schedule.c:359-361`).
///
/// Returns the step back up when the current queue sits below its ceiling
/// (`current > max`, i.e. demoted and not yet restored); `None` means
/// "at the ceiling, nothing to do" — C's empty `else` branch, spoken as
/// a value. The caller applies the step and reissues `PRIO|QUANTUM`
/// (the local fan-out, 09); the verdict never moves placement (D3).
///
/// Both rungs are [`Priority`], so the step can never pass the ceiling:
/// the `>= 16` door (06) already refused at construction.
pub fn rebalance_one(max: Priority, current: Priority) -> Option<Priority> {
    if current.get() > max.get() {
        // Below the ceiling means at rung 1 or deeper (`max >= 0`), so
        // the step up stays in range and the constructor cannot refuse.
        Priority::new(current.get() - 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prio(value: u8) -> Priority {
        Priority::new(value).expect("test rung in range")
    }

    #[test]
    fn test_init_timeout() {
        // Five seconds times the clock rate (`338`): 100 Hz waits 500.
        assert_eq!(Balancer::init(100).timeout_ticks(), 500);
        // A silent clock waits nothing (documented: callers always pass
        // the real `sys_hz`, unreachable in practice).
        assert_eq!(Balancer::init(0).timeout_ticks(), 0);
    }

    #[test]
    fn test_init_saturates() {
        // A wild rate clamps instead of wrapping the wait to zero —
        // zero would ring the bell without pause.
        assert_eq!(Balancer::init(u32::MAX).timeout_ticks(), u32::MAX);
    }

    #[test]
    fn test_restore_one_level() {
        // Below the ceiling steps one rung up (`359-360`).
        assert_eq!(rebalance_one(prio(8), prio(10)), Some(prio(9)));
        // The floor's neighbour steps onto the floor, not past it.
        assert_eq!(rebalance_one(prio(0), prio(1)), Some(prio(0)));
    }

    #[test]
    fn test_at_ceiling_rests() {
        // At the ceiling there is nothing to do (`359` false branch).
        assert_eq!(rebalance_one(prio(8), prio(8)), None);
        // Above the ceiling (never demoted) also rests: balance restores,
        // never promotes.
        assert_eq!(rebalance_one(prio(8), prio(5)), None);
    }

    #[test]
    fn test_gradual_return() {
        // Hysteresis in shape: repeated rounds converge on the ceiling,
        // one rung at a time, then rest.
        let max = prio(8);
        let mut current = prio(11);
        let mut steps = 0;
        while let Some(next) = rebalance_one(max, current) {
            current = next;
            steps += 1;
        }
        assert_eq!(current, max);
        assert_eq!(steps, 3);
    }
}
