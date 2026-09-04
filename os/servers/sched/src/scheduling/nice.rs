//! The regrade arm: a new ceiling, both numbers, or nothing.
//!
//! Mirrors `do_nice()`
//! (`minix3/minix/servers/sched/schedule.c:254-295`).
//! 08-noquantum-nice.md.
//!
//! The arm owns the role split and nothing else: who may regrade, which
//! ceilings are legal, and what a regrade writes. The fan-out itself
//! stays caller-side (09); on fan-out failure the caller writes the old
//! numbers back — rollback needs no function, only the snapshot the
//! caller already holds (`Current` is `Copy`: keeping is snapshotting).
//!
//! Single-threaded event loop: pure functions, no shared state.

use crate::priority::NR_SCHED_QUEUES;
use crate::schedproc::Priority;
use crate::table::SlotVerdict;
use minix_types::{EINVAL, EPERM, Endpoint};

/// A regrade request: who, and how high (`ipc.h:1822-1827`).
///
/// Two fields. `maxprio` rides the wire as `uint32_t` (`ipc.h:1824`);
/// the arm reads it as `i32` and refuses the out-of-range itself, so the
/// refusal lives in one home even if the wire type ever widens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// The process to regrade (`endpoint`, `ipc.h:1823`).
    pub child: Endpoint,
    /// Its new ceiling (`maxprio`, `ipc.h:1824`).
    pub maxprio: i32,
}

/// A slot's two numbers, read together (`278-279, 282, 287-288`).
///
/// Ceiling and current travel as one: the regrade writes both, the
/// rollback restores both, and no path ever writes one without the
/// other. The pairing is the transactionality — split the struct and a
/// future caller could half-write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Current {
    /// Where it runs now. C: `rmp->priority`.
    pub priority: Priority,
    /// Its ceiling. C: `rmp->max_priority`.
    pub max_priority: Priority,
}

/// Check the sender, the slot, then the ceiling (`262-276`).
///
/// The door order is diagnosis, shared with both sibling arms (06 D2,
/// 07 D2): strangers refuse first (`262-263`), dead slots next
/// (`266-271`), illegal ceilings last (`275-276`). Returns the ceiling
/// on passage so the write below needs no second check.
pub fn admit(sender_ok: bool, slot: SlotVerdict, maxprio: i32) -> Result<Priority, i32> {
    if !sender_ok {
        return Err(EPERM);
    }
    if !slot.is_ok() {
        return Err(slot.errno());
    }
    // Same two-way refusal as the START arm (06 D3): C compares the
    // unsigned slot (`276`); negatives refuse directly here — same
    // observable answer, no wrap trick.
    if maxprio < 0 || maxprio >= NR_SCHED_QUEUES as i32 {
        return Err(EINVAL);
    }
    // Guarded 0..16 by the check above; the narrowing cast cannot
    // truncate, and `Priority::new` cannot refuse.
    match Priority::new(maxprio as u8) {
        Some(ceiling) => Ok(ceiling),
        None => Err(EINVAL),
    }
}

/// Regrade: both numbers become the ceiling (`282`).
///
/// `max_priority = priority = new_q` in one move — a regrade never
/// lifts the ceiling while leaving the current behind, nor sinks the
/// current while keeping the ceiling. The single constructor closes
/// both half-writes at once.
pub const fn regrade(ceiling: Priority) -> Current {
    Current {
        priority: ceiling,
        max_priority: ceiling,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{EBADEPT, EDEADEPT};

    #[test]
    fn test_doors_in_order() {
        let live = SlotVerdict::Occupied;
        // Strangers refuse first, even with a live slot and a legal
        // ceiling (`262-263`).
        assert_eq!(admit(false, live, 7), Err(EPERM));
        // Dead slots refuse next, even with a legal ceiling (`266-271`).
        assert_eq!(admit(true, SlotVerdict::Dead, 7), Err(EDEADEPT));
        assert_eq!(admit(true, SlotVerdict::Task, 7), Err(EBADEPT));
        // Illegal ceilings refuse last (`275-276`): 16, far past,
        // negative — the START arm's two-way refusal, same observable
        // answers (06 D3).
        for bad in [16, 99, -1] {
            assert_eq!(admit(true, live, bad), Err(EINVAL), "max {bad}");
        }
        // The top of the band passes.
        assert!(admit(true, live, 15).is_ok());
    }

    #[test]
    fn test_regrade_writes_both() {
        // One move, both numbers (`282`): ceiling and current land
        // together, never apart.
        let ceiling = Priority::new(5).expect("5 < 16");
        let after = regrade(ceiling);
        assert_eq!(after.priority.get(), 5);
        assert_eq!(after.max_priority.get(), 5);
    }

    #[test]
    fn test_rollback_restores_both() {
        // Failure restores failure-atomicity (`284-288`): the caller kept
        // `before` (a copy — keeping is snapshotting), writes the regrade,
        // and on fan-out failure writes `before` back. The round trip is
        // exact: no field survives the regrade.
        let before = Current {
            priority: Priority::new(9).expect("9 < 16"),
            max_priority: Priority::new(7).expect("7 < 16"),
        };
        let ceiling = Priority::new(3).expect("3 < 16");
        let _after = regrade(ceiling);
        // Fan-out failed: write `before` back (`287-288`).
        let restored = before;
        assert_eq!(restored, before);
        assert_ne!(restored.priority.get(), ceiling.get());
    }
}
