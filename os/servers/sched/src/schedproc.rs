//! SCHED process record: who, whose, whether, how high, how long, where.
//!
//! Mirrors `struct schedproc` (`minix3/minix/servers/sched/schedproc.h:23-39`).
//! 03-schedproc-struct.md.
//!
//! The record owns the role split and nothing else: identity, occupancy,
//! caps, and placement. Table management (04), the model behind the
//! numbers (05), and handler writes (06~08) stay out.

use minix_types::Endpoint;

/// `NR_SCHED_QUEUES` (`minix3/minix/include/minix/config.h:66`): sixteen
/// queues, so priorities run `0..16`.
///
/// The full constant table lands in 05 (plan §5.2); only the bound travels
/// here, because `Priority::new` needs it and nothing else does.
pub const NR_SCHED_QUEUES: u8 = 16;

/// `IN_USE` (`schedproc.h:39`): the historical bit value.
///
/// Kept as a note for C readers; the body is [`SlotState`]. The table is
/// the only flag in the whole struct — one flag, one meaning.
pub const IN_USE: u32 = 0x00001;

/// Slot occupancy (`flags` + `IN_USE`, `schedproc.h:26,39`).
///
/// C keeps an `unsigned flags` for a single bit; the type closes the
/// question the spare bits ask ("what else could set them?"): nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// Slot free. C: `!(flags & IN_USE)`.
    Free,
    /// Slot in use. C: `flags & IN_USE`.
    InUse,
}

/// Scheduling priority (`max_priority`/`priority`, `schedproc.h:23-24`).
///
/// A `u8` newtype, mirroring the kernel's `Priority` in shape but not in
/// name (`os/kernel/src/proc.rs`: each crate owns its verdict). `u8` says
/// non-negative; construction checks the bound, so an out-of-range
/// priority cannot be built — only refused.
///
/// [ARCH S-2] (plan.md): bare `unsigned` → ranged newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority(u8);

impl Priority {
    /// Build a priority; `None` past the last queue (`>= 16` is `EINVAL`
    /// at the takeover door, 06).
    pub const fn new(value: u8) -> Option<Self> {
        if value < NR_SCHED_QUEUES {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Read the raw queue number back.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// One scheduling record (`struct schedproc`, `schedproc.h:23-36`).
///
/// Seven fields ride; the eighth (`cpu_mask`) does not: it is never
/// written nor read anywhere in Minix3 (`schedule.c:185` holds only a
/// `FIXME`), and no message carries affinity (`ipc.h:1428-1433`).
///
/// [ARCH S-3] (plan.md §7.3): structural elimination, not omission.
/// If affinity ever grows a message surface, it arrives as a new field
/// with a new contract — not as a resurrection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedProc {
    /// Whose record. C: `endpoint_t endpoint` — schedproc.h:24.
    pub endpoint: Endpoint,
    /// Who bore it. C: `endpoint_t parent` — schedproc.h:25.
    pub parent: Endpoint,
    /// Whether the slot lives. C: `flags` + `IN_USE` — schedproc.h:26,39.
    pub state: SlotState,
    /// Highest allowed priority. C: `unsigned max_priority` — schedproc.h:29.
    pub max_priority: Priority,
    /// Current priority. C: `unsigned priority` — schedproc.h:30.
    pub priority: Priority,
    /// Time slice in ms. C: `unsigned time_slice` — schedproc.h:31.
    ///
    /// The name carries the unit (draft once miscalled it ticks, R-4):
    /// the value runs straight into the kernel's `p_quantum_size_ms`.
    /// [ARCH S-6] (plan.md).
    pub time_slice_ms: u32,
    /// Which CPU it runs on. C: `unsigned cpu` — schedproc.h:32.
    pub cpu: u32,
}

impl SchedProc {
    /// Whether the slot lives.
    pub fn is_used(self) -> bool {
        self.state == SlotState::InUse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SchedProc {
        SchedProc {
            endpoint: Endpoint(5),
            parent: Endpoint(0),
            state: SlotState::InUse,
            max_priority: Priority::new(10).expect("10 < 16"),
            priority: Priority::new(12).expect("12 < 16"),
            time_slice_ms: 200,
            cpu: 0,
        }
    }

    #[test]
    fn test_names_and_flag() {
        // Identity walks through (`schedproc.h:24-25`); one flag, one
        // meaning (`schedproc.h:26,39`).
        let proc_ = sample();
        assert_eq!(proc_.endpoint, Endpoint(5));
        assert_eq!(proc_.parent, Endpoint(0));
        assert!(proc_.is_used());
        assert!(
            !SchedProc {
                state: SlotState::Free,
                ..proc_
            }
            .is_used()
        );
        assert_eq!(IN_USE, 0x00001);
    }

    #[test]
    fn test_priority_bound() {
        // Priorities run 0..16 (`config.h:66`); 16 refuses (`06` EINVAL).
        assert_eq!(Priority::new(0).map(|p| p.get()), Some(0));
        assert_eq!(Priority::new(15).map(|p| p.get()), Some(15));
        assert_eq!(Priority::new(16), None);
        assert_eq!(Priority::new(255), None);
        assert_eq!(NR_SCHED_QUEUES, 16);
    }

    #[test]
    fn test_slice_and_cpu() {
        // Slice is milliseconds (S-6); cpu is the current home.
        let proc_ = sample();
        assert_eq!(proc_.time_slice_ms, 200);
        assert_eq!(proc_.cpu, 0);
        // The eighth C field (cpu_mask) has no Rust shape: S-3
        // elimination is structural, verified by reading the struct
        // above — absence leaves nothing to assert at runtime.
    }
}
