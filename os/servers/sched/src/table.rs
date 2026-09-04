//! SCHED slot doors: occupied or vacant, never both, never neither.
//!
//! Mirrors `sched_isokendpt()` + `sched_isemtyendpt()`
//! (`minix3/minix/servers/sched/utility.c:29-56`).
//! 04-schedproc-table.md.
//!
//! The doors own the role split and nothing else: which verdict a lookup
//! earns. Table reads (name match, occupancy) arrive as booleans from
//! the caller, who holds the table; slot arithmetic reuses
//! `Endpoint::slot` (02 already depends on it for dispatch).

use minix_types::{EBADEPT, EDEADEPT, EINVAL};

/// What a slot lookup earns (`sched_isokendpt`, `utility.c:29-41`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotVerdict {
    /// The slot lives and matches: proceed.
    Occupied,
    /// Negative slot: a kernel task, never scheduled (`32-33`).
    Task,
    /// Past the table end (`34-35`).
    OutOfRange,
    /// Name mismatch or empty slot (`36-39`, one code, two causes).
    Dead,
}

impl SlotVerdict {
    /// Whether the lookup passed.
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Occupied)
    }

    /// The C errno for this verdict, read in one place.
    ///
    /// Callers (06~08) turn door refusals into handler answers through
    /// here, so the mapping never scatters: `EBADEPT` (216), `EINVAL`
    /// (22), `EDEADEPT` (215).
    pub fn errno(self) -> i32 {
        match self {
            Self::Occupied => 0,
            Self::Task => EBADEPT,
            Self::OutOfRange => EINVAL,
            Self::Dead => EDEADEPT,
        }
    }
}

/// Judge an occupied-slot claim (`sched_isokendpt`, `utility.c:29-41`).
///
/// Order is priority: tasks first, then range, then name, then occupancy
/// — the C sequence `31-40` does not commute, and neither does this.
/// `Occupied` earns `0` (C returns `OK`, which carries no news either).
pub fn check_occupied(slot: i32, table_len: usize, name_match: bool, in_use: bool) -> SlotVerdict {
    if slot < 0 {
        return SlotVerdict::Task;
    }
    if slot as usize >= table_len {
        return SlotVerdict::OutOfRange;
    }
    if !name_match || !in_use {
        return SlotVerdict::Dead;
    }
    SlotVerdict::Occupied
}

/// Judge a vacant-slot claim (`sched_isemtyendpt`, `utility.c:46-56`).
///
/// The mirror door: same arithmetic, same first two refusals, the last
/// judgment flipped — occupancy refuses. No name check: an empty slot
/// has no name to match (checking one would presume somebody).
pub fn check_vacant(slot: i32, table_len: usize, in_use: bool) -> SlotVerdict {
    if slot < 0 {
        return SlotVerdict::Task;
    }
    if slot as usize >= table_len {
        return SlotVerdict::OutOfRange;
    }
    if in_use {
        return SlotVerdict::Dead;
    }
    SlotVerdict::Occupied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_occupied_door() {
        // Four judgments in C order (`utility.c:31-40`).
        assert_eq!(check_occupied(3, 256, true, true), SlotVerdict::Occupied);
        // Tasks first: negative slots never schedule (`32-33`).
        assert_eq!(check_occupied(-1, 256, true, true), SlotVerdict::Task);
        assert_eq!(check_occupied(-1024, 256, false, false), SlotVerdict::Task);
        // Then range (`34-35`).
        assert_eq!(
            check_occupied(256, 256, true, true),
            SlotVerdict::OutOfRange
        );
        assert_eq!(
            check_occupied(5000, 256, true, true),
            SlotVerdict::OutOfRange
        );
        // Then name (`36-37`), then occupancy (`38-39`) — one code, two causes.
        assert_eq!(check_occupied(3, 256, false, true), SlotVerdict::Dead);
        assert_eq!(check_occupied(3, 256, true, false), SlotVerdict::Dead);
        assert_eq!(check_occupied(3, 256, false, false), SlotVerdict::Dead);
        // Order is priority: task beats range beats name.
        assert_eq!(check_occupied(-1, 0, false, false), SlotVerdict::Task);
        // Verdicts and errnos.
        assert!(SlotVerdict::Occupied.is_ok());
        assert!(!SlotVerdict::Dead.is_ok());
        assert_eq!(SlotVerdict::Occupied.errno(), 0);
        assert_eq!(SlotVerdict::Task.errno(), EBADEPT);
        assert_eq!(SlotVerdict::OutOfRange.errno(), EINVAL);
        assert_eq!(SlotVerdict::Dead.errno(), EDEADEPT);
        assert_eq!((EBADEPT, EINVAL, EDEADEPT), (216, 22, 215));
    }

    #[test]
    fn test_vacant_mirror() {
        // Same arithmetic, same first refusals, last judgment flipped.
        assert_eq!(check_vacant(3, 256, false), SlotVerdict::Occupied);
        assert_eq!(check_vacant(-1, 256, false), SlotVerdict::Task);
        assert_eq!(check_vacant(256, 256, false), SlotVerdict::OutOfRange);
        assert_eq!(check_vacant(3, 256, true), SlotVerdict::Dead);
        // Occupied and vacant never agree on one slot.
        assert_ne!(
            check_occupied(3, 256, true, true),
            check_vacant(3, 256, true)
        );
    }
}
