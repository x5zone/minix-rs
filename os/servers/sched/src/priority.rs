//! SCHED priority and time-slice model: how high, how long, whose child.
//!
//! Mirrors the constant tables (`minix3/minix/include/minix/config.h:66-77`),
//! the slice default and system-process test
//! (`minix3/minix/servers/sched/schedule.c:41-44`), and the nice conversion
//! (`minix3/minix/servers/pm/utility.c:91-101`).
//! 05-priority-timeslice-model.md.
//!
//! The module owns the role split and nothing else: the vocabulary every
//! handler speaks (queues, quanta, nice, system-ness) without any handler's
//! transitions. Queue arithmetic (06), quantum exhaustion (08), nice writes
//! (08), parameter fan-out (09), CPU choice (10), and queue rebalancing (11)
//! all read these names; none of them lives here.

use crate::schedproc::Priority;
use minix_types::Endpoint;

/// How many scheduling queues exist (`config.h:66`).
///
/// Sixteen queues, so priorities run `0..16`. Re-exported from
/// [`crate::schedproc`] so the table has one home: 03 lends the bound,
/// this module lays the table.
pub use crate::schedproc::NR_SCHED_QUEUES;

/// Highest queue, used for kernel tasks (`config.h:67`).
pub const TASK_Q: u8 = 0;

/// Highest queue a user process may hold (`config.h:68`).
///
/// Reads 0, the same as [`TASK_Q`]: user processes may climb all the way
/// to the top queue. The distinction is who grants it (the scheduler),
/// not the number.
pub const MAX_USER_Q: u8 = 0;

/// Lowest queue a user process may sink to (`config.h:70-71`).
pub const MIN_USER_Q: u8 = 15;

/// Default queue for a new user process (`config.h:69`).
///
/// C writes it as a macro over the other two
/// (`(MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q`); the Rust form keeps
/// that shape so the value stays derived, not pinned: `15 / 2 + 0 = 7`.
/// Integer division truncates, exactly as in C.
pub const USER_Q: u8 = (MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q;

/// Default scheduling quantum in milliseconds (`config.h:74`).
///
/// The name carries no unit in C; the unit is nailed down here by the
/// wire it feeds: [`DEFAULT_USER_TIME_SLICE`] runs straight into the
/// kernel's `p_quantum_size_ms` (`system.c:683`), so both are ms.
/// [ARCH S-6] (plan.md): the draft's "ticks" reading (R-4) is retired.
pub const USER_QUANTUM: u32 = 200;

/// Default time slice for a fresh user process, in ms
/// (`schedule.c:41`).
///
/// Numerically equal to [`USER_QUANTUM`] but semantically distinct: one
/// is the config-file default quantum, the other the code default for a
/// slot that never received a quantum. They coincide today; the two
/// names keep the two provenances from merging.
pub const DEFAULT_USER_TIME_SLICE: u32 = 200;

/// "Use the default CPU" sentinel (`config.h:77`).
///
/// C spells it `-1`. The hyphen is not a CPU; [`CpuChoice`] says so in
/// the type. This constant stays for wire readers (messages carry `i32`)
/// and for [`CpuChoice::from_raw`].
pub const USER_DEFAULT_CPU: i32 = -1;

/// Lowest nice value (`sys/resource.h:43`).
pub const PRIO_MIN: i32 = -20;

/// Highest nice value (`sys/resource.h:44`).
pub const PRIO_MAX: i32 = 20;

/// Which CPU a process should run on.
///
/// C threads a bare `int` through the calls (`-1` means "default",
/// `sched_start.c` through `schedule.c:314-316`); the hyphen then needs
/// a comment at every use. The enum moves that comment into the type:
/// [`CpuChoice::Default`] is the `-1`, [`CpuChoice::Cpu`] a real home.
/// [ARCH S-4] (plan.md): the `-1`-as-`unsigned`-load-counter sibling
/// (`cpu_proc[]`/`CPU_DEAD`) is 10's business; this enum covers the
/// selection side only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuChoice {
    /// No preference: pick the default (C: `USER_DEFAULT_CPU`, -1).
    Default,
    /// A specific CPU number.
    Cpu(u32),
}

impl CpuChoice {
    /// Read a wire value (`-1` keeps, `>= 0` pins).
    ///
    /// Mirrors the kernel's gate (`system.c:652-653`): anything below -1
    /// is neither default nor CPU, so it refuses (`None`; the caller
    /// answers `EINVAL`, 09). C's SCHED side never checks — the kernel
    /// does — so this check is documented as 09's, provided here for
    /// one home.
    pub const fn from_raw(value: i32) -> Option<Self> {
        if value == USER_DEFAULT_CPU {
            Some(Self::Default)
        } else if value >= 0 {
            // Guarded non-negative by the check above; the widening cast
            // cannot lose information.
            Some(Self::Cpu(value as u32))
        } else {
            None
        }
    }

    /// Whether the choice names a specific CPU.
    pub const fn is_pinned(self) -> bool {
        matches!(self, Self::Cpu(_))
    }
}

/// A Unix nice value (`PRIO_MIN..=PRIO_MAX`).
///
/// C passes a bare `int` and lets `nice_to_priority`
/// (`pm/utility.c:91`) refuse the out-of-range (`EINVAL`); the range
/// then lives in the caller. The newtype moves it into construction:
/// a `Nice` is always mappable, so mapping itself cannot fail. The
/// `EINVAL` answer still exists — it is just spoken at the edge
/// (`None` from [`Nice::new`], 04-stage-pm/16's surface), not mid-formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nice(i32);

impl Nice {
    /// Build a nice value; `None` outside `-20..=20` (C: `EINVAL`,
    /// `pm/utility.c:93`).
    pub const fn new(value: i32) -> Option<Self> {
        if value >= PRIO_MIN && value <= PRIO_MAX {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Read the raw nice number back.
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Convert to a scheduling queue (`pm/utility.c:95-96`).
    ///
    /// The C formula, with the out-param and the errno left behind:
    /// `MAX_USER_Q + (nice - PRIO_MIN) * SPAN_QUEUES / SPAN_NICE`, where
    /// `SPAN_QUEUES` is 16 queues and `SPAN_NICE` 41 nice levels. Forty-one
    /// levels over sixteen queues cannot land evenly — integer division
    /// truncates, so neighbors share queues (e.g. nice -20 and -19 both
    /// read queue 0). The sharing is the formula's, not a bug.
    ///
    /// C clamps the result into range afterwards (`utility.c:99-100`);
    /// the clamp is dead arithmetic — a valid nice provably lands in
    /// `0..=15` — so the Rust form skips it and says why. `None` is
    /// defensive only (it fires solely if the constants above ever drift
    /// apart); today every valid nice maps to `Some`.
    pub const fn to_priority(self) -> Option<Priority> {
        const SPAN_QUEUES: i32 = (MIN_USER_Q - MAX_USER_Q + 1) as i32;
        const SPAN_NICE: i32 = PRIO_MAX - PRIO_MIN + 1;
        let queue = MAX_USER_Q as i32 + (self.0 - PRIO_MIN) * SPAN_QUEUES / SPAN_NICE;
        if queue >= MAX_USER_Q as i32 && queue <= MIN_USER_Q as i32 {
            // Guarded 0..=15 by the check above, so the narrowing cast
            // cannot truncate; the `Option` is the `Priority` bound check.
            Priority::new(queue as u8)
        } else {
            None
        }
    }
}

/// Whether a quantum value the kernel would accept (`system.c:648-649`).
///
/// The kernel refuses `quantum < 1` (`EINVAL`); SCHED's START branch
/// stores whatever the message carries (`schedule.c:196`) and lets the
/// kernel judge at fan-out. The predicate lives here so the rule has
/// one home; the refusal itself stays at the gate (09).
pub const fn is_valid_quantum(quantum_ms: u32) -> bool {
    quantum_ms >= 1
}

/// Whether a slot counts as a system process (`schedule.c:44`).
///
/// Parentage, not privilege: a process borne of RS (`RS_PROC_NR`,
/// `com.h:61`) is a system process. The test reads the parent endpoint —
/// [`Endpoint::RS`] is 2 — so grandchild processes of RS-spawned servers
/// read non-system: system-ness is one generation deep, by construction.
/// CPU choice (10) leans on this: system processes start on the BSP.
pub fn is_system_proc(parent: Endpoint) -> bool {
    parent == Endpoint::RS
}

/// Whether a ceiling marks its bearer "niced" (`schedule.c:319`).
///
/// `niced = (max_priority > USER_Q)`: anyone whose ceiling sits below
/// the default queue (numerically above 7) carries the flag down to the
/// kernel (`MF_NICED`). The flag is advisory accounting, not a gate —
/// the gate is the ceiling itself. Computed here, consumed at fan-out
/// (09); the formula keeps one home.
pub fn is_niced(max_priority: Priority) -> bool {
    max_priority.get() > USER_Q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_table() {
        // The full queue table (`config.h:66-71`): sixteen queues, user
        // band 0..=15, default derived — not pinned — at 7.
        assert_eq!(NR_SCHED_QUEUES, 16);
        assert_eq!(TASK_Q, 0);
        assert_eq!(MAX_USER_Q, 0);
        assert_eq!(MIN_USER_Q, 15);
        assert_eq!(USER_Q, 7);
        assert_eq!(USER_Q, (MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q);
        // The user band tops out at the kernel queue: MAX_USER_Q == TASK_Q
        // is C's spelling, not a typo.
        assert_eq!(MAX_USER_Q, TASK_Q);
    }

    #[test]
    fn test_quantum_defaults() {
        // Two names, one number (`config.h:74`, `schedule.c:41`); both ms
        // (S-6: straight into `p_quantum_size_ms`).
        assert_eq!(USER_QUANTUM, 200);
        assert_eq!(DEFAULT_USER_TIME_SLICE, 200);
        // The kernel's gate (`system.c:648-649`): 0 refuses, 1 passes.
        assert!(!is_valid_quantum(0));
        assert!(is_valid_quantum(1));
        assert!(is_valid_quantum(200));
    }

    #[test]
    fn test_nice_bounds() {
        // Range edges (`sys/resource.h:43-44` + `pm/utility.c:93`).
        assert_eq!(Nice::new(PRIO_MIN).map(|n| n.get()), Some(-20));
        assert_eq!(Nice::new(PRIO_MAX).map(|n| n.get()), Some(20));
        assert_eq!(Nice::new(0).map(|n| n.get()), Some(0));
        assert_eq!(Nice::new(PRIO_MIN - 1), None);
        assert_eq!(Nice::new(PRIO_MAX + 1), None);
        assert_eq!((PRIO_MIN, PRIO_MAX), (-20, 20));
    }

    #[test]
    fn test_nice_mapping() {
        // Formula pins (`pm/utility.c:95-96`): bottom→0, middle→USER_Q,
        // top→15. Nice 0 lands on USER_Q (7): the config comment's
        // "should correspond to nice 0" holds by arithmetic
        // (20 * 16 / 41 = 7, truncated), not by fiat.
        let prio = |nice: i32| Nice::new(nice).and_then(|n| n.to_priority());
        assert_eq!(prio(-20).map(|p| p.get()), Some(0));
        assert_eq!(prio(0).map(|p| p.get()), Some(USER_Q));
        assert_eq!(prio(20).map(|p| p.get()), Some(MIN_USER_Q));
        // Neighbors share queues (41 levels over 16 queues truncate).
        assert_eq!(prio(-19).map(|p| p.get()), Some(0));
        // Out-of-range nice never reaches the formula (edge speaks None).
        assert_eq!(Nice::new(-21).and_then(|n| n.to_priority()), None);
        assert_eq!(Nice::new(21).and_then(|n| n.to_priority()), None);
    }

    #[test]
    fn test_system_proc() {
        // Parentage test (`schedule.c:44` + `com.h:61`): RS's children
        // are system processes; everyone else's are not — one generation
        // deep, grandchildren excluded by construction.
        assert!(is_system_proc(Endpoint::RS));
        assert!(is_system_proc(Endpoint(2)));
        assert!(!is_system_proc(Endpoint::PM));
        assert!(!is_system_proc(Endpoint::INIT));
        assert!(!is_system_proc(Endpoint::SCHED));
        assert!(!is_system_proc(Endpoint(100)));
    }

    #[test]
    fn test_cpu_choice() {
        // `-1` keeps, `>= 0` pins (`config.h:77`); below `-1` refuses
        // (kernel's gate, `system.c:652-653`, spoken here).
        assert_eq!(CpuChoice::from_raw(USER_DEFAULT_CPU), Some(CpuChoice::Default));
        assert_eq!(CpuChoice::from_raw(-1), Some(CpuChoice::Default));
        assert_eq!(CpuChoice::from_raw(0), Some(CpuChoice::Cpu(0)));
        assert_eq!(CpuChoice::from_raw(3), Some(CpuChoice::Cpu(3)));
        assert_eq!(CpuChoice::from_raw(-2), None);
        assert!(!CpuChoice::Default.is_pinned());
        assert!(CpuChoice::Cpu(0).is_pinned());
    }

    #[test]
    fn test_niced_flag() {
        // `max_priority > USER_Q` (`schedule.c:319`): ceilings at or above
        // the default queue read clean; anything below reads niced.
        let flag = |q: u8| Priority::new(q).map(is_niced);
        assert_eq!(flag(0), Some(false));
        assert_eq!(flag(USER_Q), Some(false));
        assert_eq!(flag(USER_Q + 1), Some(true));
        assert_eq!(flag(MIN_USER_Q), Some(true));
    }
}
