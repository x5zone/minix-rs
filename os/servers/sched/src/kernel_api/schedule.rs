//! Parameter fan-out: aggregate a slot, pack the message, mind the gates.
//!
//! Mirrors `schedule_process()` (`minix3/minix/servers/sched/
//! schedule.c:297-332`) and documents its two readers: `sys_schedule()`
//! (`minix3/minix/lib/libsys/sys_schedule.c`) and `do_schedule()` +
//! `sched_proc()` (`minix3/minix/kernel/system/do_schedule.c`,
//! `minix3/minix/kernel/system.c:642-699`).
//! 09-schedule-process.md.
//!
//! The module owns the role split and nothing else: which fields a call
//! carries and which it keeps. Picking the CPU stays caller-side (10
//! runs `pick_cpu` before every call, `schedule.c:302`); the takeover
//! call, the retry ring, and the local/migrate shorthands' *uses* stay
//! with their arms (06~08). What travels is `Fanout`; what judges it is
//! the kernel.

use crate::priority::is_niced;
use bitflags::bitflags;
use minix_types::Endpoint;

bitflags! {
    /// Which slot fields a fan-out carries (`schedule.c:22-30`).
    ///
    /// C spells the set as an `unsigned` of three bits; the flags type
    /// keeps the bits and names the frequent unions, so call sites read
    /// `LOCAL` instead of recombining `PRIO | QUANTUM` by hand (and
    /// possibly dropping one). `ALL` is births (06), `LOCAL` the
    /// in-place arms (08), `MIGRATE` the CPU moves, `empty()` the
    /// keep-everything call (which the code never sends, but the shape
    /// allows naming).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChangeMask: u8 {
        /// Carry the priority (`SCHEDULE_CHANGE_PRIO`, `0x1`).
        const PRIO = 0x1;
        /// Carry the time slice (`SCHEDULE_CHANGE_QUANTUM`, `0x2`).
        const QUANTUM = 0x2;
        /// Carry the CPU (`SCHEDULE_CHANGE_CPU`, `0x4`).
        const CPU = 0x4;
        /// Carry everything (`SCHEDULE_CHANGE_ALL`, 06 births).
        const ALL = Self::PRIO.bits() | Self::QUANTUM.bits() | Self::CPU.bits();
        /// Carry numbers, not placement (`schedule_process_local`,
        /// `32-33`: the 08 arms' shape).
        const LOCAL = Self::PRIO.bits() | Self::QUANTUM.bits();
        /// Carry placement only (`schedule_process_migrate`, `34-35`).
        const MIGRATE = Self::CPU.bits();
    }
}

/// "Keep the kernel's value" (`schedule.c:307,312,317` + `system.c:680-688`).
///
/// Both sides agree on `-1`: SCHED sends it for every unflagged field,
/// the kernel skips every `-1` field. One constant, two readers — the
/// value lives here so the agreement has one home.
pub const KEEP: i32 = -1;

/// A slot's readable numbers, for aggregation (`schedule.c:297-301`).
///
/// The caller assembles these from its table row (and from 10's CPU
/// choice) before each call. All five travel in; the mask decides which
/// travel on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotValues {
    /// Whose parameters. C: `rmp->endpoint` — schedule.c:321.
    pub endpoint: Endpoint,
    /// Where it runs now. C: `rmp->priority` — schedule.c:305.
    pub priority: crate::schedproc::Priority,
    /// Its share in ms. C: `rmp->time_slice` — schedule.c:310.
    pub time_slice_ms: u32,
    /// Its CPU. C: `rmp->cpu` — schedule.c:315.
    pub cpu: u32,
    /// Its ceiling (for the niced note). C: `rmp->max_priority` — 319.
    pub max_priority: crate::schedproc::Priority,
}

/// What one fan-out carries (`schedule.c:304-319`).
///
/// Field-for-field the kernel's `SchedParams` (`os/kernel/src/
/// sched.rs:272-277`: endpoint aside, `priority: Option<u8>`,
/// `quantum: Option<u32>`, `cpu: Option<u32>`, `niced: bool`) — two
/// crates, one shape on the wire (same shape, different dress, 03 D3's
/// precedent).
/// `None` means keep: the packers below render it as [`KEEP`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fanout {
    /// Whose parameters. C: `rmp->endpoint` — schedule.c:321.
    pub endpoint: Endpoint,
    /// The queue, if carried. C: `new_prio` — schedule.c:304-307.
    pub priority: Option<u8>,
    /// The share in ms, if carried. C: `new_quantum` — schedule.c:309-312.
    pub quantum_ms: Option<u32>,
    /// The CPU, if carried. C: `new_cpu` — schedule.c:314-317.
    pub cpu: Option<u32>,
    /// Whether the ceiling sits below the default queue. C: `niced` —
    /// schedule.c:319. Always carried, never kept: the note rides every
    /// call, even the keep-everything one.
    pub niced: bool,
}

/// Aggregate a slot under a mask (`schedule.c:302-319`).
///
/// Flagged fields copy from the slot; unflagged read `None` (keep).
/// `niced` derives from the ceiling through 05's predicate
/// (`is_niced`: `max > USER_Q`) — the formula keeps one home, this call
/// site only uses it. The CPU choice itself is the caller's (10 ran
/// `pick_cpu` first, `302`); aggregation only decides whether it rides.
pub fn aggregate(mask: ChangeMask, slot: &SlotValues) -> Fanout {
    Fanout {
        endpoint: slot.endpoint,
        priority: mask
            .contains(ChangeMask::PRIO)
            .then_some(slot.priority.get()),
        quantum_ms: mask
            .contains(ChangeMask::QUANTUM)
            .then_some(slot.time_slice_ms),
        cpu: mask.contains(ChangeMask::CPU).then_some(slot.cpu),
        niced: is_niced(slot.max_priority),
    }
}

impl Fanout {
    /// Render the priority for the wire (`None` keeps, `KEEP`).
    pub const fn wire_priority(self) -> i32 {
        match self.priority {
            Some(queue) => queue as i32,
            None => KEEP,
        }
    }

    /// Render the share for the wire.
    ///
    /// The widening cast cannot lose information (`u32` into `i32`'s
    /// non-negative range is not guaranteed in general — but shares are
    /// millisecond counts far below `i32::MAX`; oversized values are the
    /// kernel's gate to refuse, not the packer's to clip).
    pub const fn wire_quantum_ms(self) -> i32 {
        match self.quantum_ms {
            Some(ms) => ms as i32,
            None => KEEP,
        }
    }

    /// Render the CPU for the wire.
    pub const fn wire_cpu(self) -> i32 {
        match self.cpu {
            Some(cpu) => cpu as i32,
            None => KEEP,
        }
    }

    /// Render the note for the wire (`0`/`1`; the kernel's `!!`
    /// normalizes anyway, `do_schedule.c:27`).
    pub const fn wire_niced(self) -> i32 {
        if self.niced { 1 } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::priority::USER_Q;
    use crate::schedproc::Priority;

    fn slot() -> SlotValues {
        SlotValues {
            endpoint: Endpoint(20),
            priority: Priority::new(9).expect("9 < 16"),
            time_slice_ms: 200,
            cpu: 1,
            max_priority: Priority::new(7).expect("7 < 16"),
        }
    }

    #[test]
    fn test_mask_bits() {
        // Three bits (`schedule.c:22-24`) and their three unions
        // (`26-35`): births carry all, in-place arms carry numbers,
        // moves carry placement.
        assert_eq!(ChangeMask::PRIO.bits(), 0x1);
        assert_eq!(ChangeMask::QUANTUM.bits(), 0x2);
        assert_eq!(ChangeMask::CPU.bits(), 0x4);
        assert_eq!(
            ChangeMask::ALL,
            ChangeMask::PRIO | ChangeMask::QUANTUM | ChangeMask::CPU
        );
        assert_eq!(ChangeMask::LOCAL, ChangeMask::PRIO | ChangeMask::QUANTUM);
        assert_eq!(ChangeMask::MIGRATE, ChangeMask::CPU);
        assert!(ChangeMask::empty().is_empty());
    }

    #[test]
    fn test_aggregate_all() {
        // Births carry everything (`SCHEDULE_CHANGE_ALL`, 06): every
        // field rides, and the note follows the ceiling (`319`).
        let out = aggregate(ChangeMask::ALL, &slot());
        assert_eq!(out.endpoint, Endpoint(20));
        assert_eq!(out.priority, Some(9));
        assert_eq!(out.quantum_ms, Some(200));
        assert_eq!(out.cpu, Some(1));
        // Ceiling 7 == USER_Q: not above, so not niced.
        assert!(!out.niced);
        assert_eq!(KEEP, -1);
    }

    #[test]
    fn test_aggregate_partial() {
        // In-place arms carry numbers, not placement (`LOCAL`, 08):
        // the CPU stays home (`None` keeps).
        let out = aggregate(ChangeMask::LOCAL, &slot());
        assert_eq!(out.priority, Some(9));
        assert_eq!(out.quantum_ms, Some(200));
        assert_eq!(out.cpu, None);
        // Moves carry placement only (`MIGRATE`).
        let out = aggregate(ChangeMask::MIGRATE, &slot());
        assert_eq!(out.priority, None);
        assert_eq!(out.quantum_ms, None);
        assert_eq!(out.cpu, Some(1));
        // Nothing flagged: everything keeps — a shape the code never
        // sends, but the mask allows naming.
        let out = aggregate(ChangeMask::empty(), &slot());
        assert_eq!(out.priority, None);
        assert_eq!(out.quantum_ms, None);
        assert_eq!(out.cpu, None);
        // The note still rides: it is not a masked field (`319` runs
        // outside the flag checks).
        assert!(!out.niced);
        assert_eq!(USER_Q, 7);
    }

    #[test]
    fn test_wire_keep() {
        // `None` renders as `-1` on both sides of the boundary
        // (`schedule.c:307-317` sends it, `system.c:680-688` skips it).
        let out = aggregate(ChangeMask::LOCAL, &slot());
        assert_eq!(out.wire_priority(), 9);
        assert_eq!(out.wire_quantum_ms(), 200);
        assert_eq!(out.wire_cpu(), KEEP);
        let out = aggregate(ChangeMask::empty(), &slot());
        assert_eq!(out.wire_priority(), KEEP);
        assert_eq!(out.wire_quantum_ms(), KEEP);
        assert_eq!(out.wire_cpu(), KEEP);
    }

    #[test]
    fn test_niced_derivation() {
        // The note follows the ceiling through 05's predicate
        // (`max > USER_Q`): at the default queue clean, below it niced.
        let mut s = slot();
        assert!(!aggregate(ChangeMask::ALL, &s).niced);
        s.max_priority = Priority::new(8).expect("8 < 16");
        let out = aggregate(ChangeMask::ALL, &s);
        assert!(out.niced);
        assert_eq!(out.wire_niced(), 1);
        assert_eq!(aggregate(ChangeMask::ALL, &slot()).wire_niced(), 0);
    }
}
