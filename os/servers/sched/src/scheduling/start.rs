//! The takeover arm: two letters, one door, three births, one retry ring.
//!
//! Mirrors `do_start_scheduling()`
//! (`minix3/minix/servers/sched/schedule.c:140-252`).
//! 06-start-scheduling.md.
//!
//! The arm owns the role split and nothing else: which letter may enter,
//! what a fresh slot holds, and when the fan-out must try another CPU.
//! Table reads arrive as verdicts from the caller, who holds the table
//! (04); the takeover call, the fan-out call, and the CPU choice itself
//! stay caller-side (09, 10, 12).
//!
//! Single-threaded event loop: pure functions, no shared state.

use crate::dispatch::SchedMsg;
use crate::priority::NR_SCHED_QUEUES;
use crate::schedproc::Priority;
use crate::table::SlotVerdict;
use minix_types::{EBADCPU, EINVAL, EPERM, Endpoint};

/// Which of the two takeover letters arrived (`schedule.c:146`).
///
/// C asserts the type at the door (`SCHEDULING_START || SCHEDULING_INHERIT`);
/// the assert becomes a gate: only these two convert, the rest refuse.
/// Dispatch (02) already classified the arrival; this gate keeps the arm
/// total even if a future caller forgets the dispatch contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A fresh (system) process: values arrive explicitly (`191-197`).
    Start,
    /// A forked child: values arrive by inheritance (`199-211`).
    Inherit,
}

impl Kind {
    /// Read the letter; anything but the two takeover kinds refuses
    /// (`None`; C: `assert`, `146` + `213`).
    pub const fn from_msg(msg: SchedMsg) -> Option<Self> {
        match msg {
            SchedMsg::Start => Some(Self::Start),
            SchedMsg::Inherit => Some(Self::Inherit),
            SchedMsg::Stop | SchedMsg::SetNice | SchedMsg::NoQuantum => None,
        }
    }
}

/// A takeover request: the message body after dispatch (`1437`).
///
/// `maxprio` rides the wire as `int` (`ipc.h:1432`); negativity is refused
/// at planning, not at parsing — C funnels it through the unsigned slot
/// into the same `>= 16` refusal (`164`), so the observable answer is
/// identical either way. `quantum` rides as `int` too (`ipc.h:1433`) and
/// is stored raw (see [`plan_start`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// Which letter this is.
    pub kind: Kind,
    /// The process to take over (`endpoint`, `ipc.h:1431`).
    pub child: Endpoint,
    /// Its parent (`parent`, `ipc.h:1432`).
    pub parent: Endpoint,
    /// Its ceiling (`maxprio`, `ipc.h:1433`).
    pub maxprio: i32,
    /// Its share (`quantum`, `ipc.h:1434`).
    pub quantum: i32,
}

/// A ready slot: what the table will hold (`161-163, 193-208`).
///
/// The birth values only — occupancy (`IN_USE`, `223`), the takeover
/// call (`218`), the CPU (`226`), and the reply (`246`) stay caller-side.
/// `endpoint == parent` marks the init shape (see [`plan_start`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed {
    /// Whose slot. C: `rmp->endpoint` — schedule.c:160.
    pub endpoint: Endpoint,
    /// Who bore it. C: `rmp->parent` — schedule.c:161.
    pub parent: Endpoint,
    /// The ceiling. C: `rmp->max_priority` — schedule.c:162.
    pub max_priority: Priority,
    /// Where it runs now. C: `rmp->priority`.
    pub priority: Priority,
    /// Its share in ms. C: `rmp->time_slice`.
    pub time_slice_ms: u32,
}

/// A parent's readable state, for the INHERIT branch (`207-208`).
///
/// The caller reads the parent slot after its verdict passes; only the
/// two inherited fields travel — the arm never sees the parent's table
/// row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentState {
    /// Where the parent runs now. C: `schedproc[parent_nr_n].priority`.
    pub priority: Priority,
    /// The parent's share. C: `schedproc[parent_nr_n].time_slice`.
    pub time_slice_ms: u32,
}

/// Check the sender, then the ceiling (`150-166`).
///
/// The door order is diagnosis: a stranger's letter earns `EPERM` even
/// with a perfect slot (`150-152`), and a taken slot earns its verdict
/// even with a perfect ceiling (`154-157` beats `164-166`). Returns the
/// ceiling on passage so both branches share one door.
fn admit(sender_ok: bool, child: SlotVerdict, maxprio: i32) -> Result<Priority, i32> {
    if !sender_ok {
        return Err(EPERM);
    }
    if !child.is_ok() {
        return Err(child.errno());
    }
    // C compares the unsigned slot against 16 (`164`); a negative wire
    // value wraps unsigned into the same refusal. The Rust form refuses
    // negatives directly — same observable answer, no wrap trick.
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

/// Plan a START birth (`191-197`, via the init shape `171-188`).
///
/// The init provisional values (`USER_Q` / `DEFAULT_USER_TIME_SLICE`,
/// `174-175`) are overwritten unconditionally by the START branch
/// (`195-196`), so the net birth equals a direct START for every input —
/// including init itself (PM sends `USER_Q`/`USER_QUANTUM`, the same
/// numbers by a different road). The lasting init effect is the CPU
/// (`machine.bsp_id`, `184`, SMP builds only): the caller keeps a
/// self-parented seed on the BSP (10 consumes this rule).
/// `quantum` stores raw (`196`): whatever the message carries, even
/// nonsense — the kernel judges at fan-out (09). The wrapping cast
/// mirrors C's unsigned store; legit clients assert `quantum > 0`
/// (`sched_start.c`), so the wrap is unreachable through them.
pub fn plan_start(sender_ok: bool, child: SlotVerdict, req: &Request) -> Result<Seed, i32> {
    let ceiling = admit(sender_ok, child, req.maxprio)?;
    Ok(Seed {
        endpoint: req.child,
        parent: req.parent,
        max_priority: ceiling,
        priority: ceiling,
        // Stores raw per `196`; see above for why no validation here.
        time_slice_ms: req.quantum as u32,
    })
}

/// Plan an INHERIT birth (`199-211`).
///
/// Past the shared door, the parent must itself be scheduled (`203-206`):
/// a vacant or mismatched parent earns its verdict, and the birth never
/// happens. A self-parented INHERIT needs no special case — the parent
/// slot *is* the child slot, still unflagged (`flags = IN_USE` lands at
/// `223`, after the switch), so the caller's parent verdict reads `Dead`
/// and the refusal matches C exactly. Values then copy from the parent
/// (`207-208`): the child opens where the parent runs, with the parent's
/// share — fork opens as continuation, not as novelty.
pub fn plan_inherit(
    sender_ok: bool,
    child: SlotVerdict,
    parent: SlotVerdict,
    req: &Request,
    parent_state: &ParentState,
) -> Result<Seed, i32> {
    let ceiling = admit(sender_ok, child, req.maxprio)?;
    if !parent.is_ok() {
        return Err(parent.errno());
    }
    Ok(Seed {
        endpoint: req.child,
        parent: req.parent,
        max_priority: ceiling,
        priority: parent_state.priority,
        time_slice_ms: parent_state.time_slice_ms,
    })
}

/// What one fan-out attempt earned (`227-233`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fanout {
    /// The attempt settled, well or badly: leave the ring with its code.
    Done(i32),
    /// The CPU is dead: brand it and try another (`229-230`).
    CpuDead,
}

/// Classify a fan-out answer (`227`).
///
/// Only `EBADCPU` circles back; every other code — `OK` or otherwise —
/// leaves the ring (`233-237` judge it next). The branding
/// (`cpu_proc[cpu] = CPU_DEAD`) and the re-pick stay caller-side (10
/// owns the load table); this verdict only names the turn.
pub const fn classify_fanout(rv: i32) -> Fanout {
    if rv == EBADCPU {
        Fanout::CpuDead
    } else {
        Fanout::Done(rv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::priority::{DEFAULT_USER_TIME_SLICE, MIN_USER_Q, USER_Q};
    use minix_types::{EBADEPT, EDEADEPT};

    fn start_req(child: Endpoint, parent: Endpoint, maxprio: i32, quantum: i32) -> Request {
        Request {
            kind: Kind::Start,
            child,
            parent,
            maxprio,
            quantum,
        }
    }

    fn inherit_req(child: Endpoint, parent: Endpoint, maxprio: i32) -> Request {
        Request {
            kind: Kind::Inherit,
            child,
            parent,
            maxprio,
            quantum: 0,
        }
    }

    #[test]
    fn test_kind_gate() {
        // Only the two takeover letters convert (`146`); the rest refuse.
        assert_eq!(Kind::from_msg(SchedMsg::Start), Some(Kind::Start));
        assert_eq!(Kind::from_msg(SchedMsg::Inherit), Some(Kind::Inherit));
        assert_eq!(Kind::from_msg(SchedMsg::Stop), None);
        assert_eq!(Kind::from_msg(SchedMsg::SetNice), None);
        assert_eq!(Kind::from_msg(SchedMsg::NoQuantum), None);
    }

    #[test]
    fn test_doors_in_order() {
        let req = start_req(Endpoint(20), Endpoint::PM, 7, 200);
        let vacant = SlotVerdict::Occupied;
        let taken = SlotVerdict::Dead;
        // Strangers refuse first, even with a perfect slot (`150-152`).
        assert_eq!(plan_start(false, vacant, &req), Err(EPERM));
        // Taken slots refuse next, even with a perfect ceiling (`154-157`).
        assert_eq!(plan_start(true, taken, &req), Err(taken.errno()));
        assert_eq!(plan_start(true, SlotVerdict::Task, &req), Err(EBADEPT));
        // The ceiling refuses last (`164-166`): 16, far past, negative.
        for bad in [16, 99, -1] {
            let r = start_req(Endpoint(20), Endpoint::PM, bad, 200);
            assert_eq!(plan_start(true, vacant, &r), Err(EINVAL), "max {bad}");
        }
        // The top of the band passes (`15 < 16`).
        let top = start_req(Endpoint(20), Endpoint::PM, MIN_USER_Q as i32, 200);
        assert!(plan_start(true, vacant, &top).is_ok());
    }

    #[test]
    fn test_start_birth() {
        // START states values explicitly (`195-196`): ceiling and quantum.
        let req = start_req(Endpoint(20), Endpoint::RS, 5, 100);
        let seed = plan_start(true, SlotVerdict::Occupied, &req).expect("valid START");
        assert_eq!(seed.endpoint, Endpoint(20));
        assert_eq!(seed.parent, Endpoint::RS);
        assert_eq!(seed.max_priority.get(), 5);
        assert_eq!(seed.priority.get(), 5);
        assert_eq!(seed.time_slice_ms, 100);
        // Init rides START too: provisional USER_Q/DEFAULT are overwritten
        // (`174-175` then `195-196`), so the net birth is a plain START —
        // and the self-parent shape survives for the caller's BSP rule.
        let init = start_req(Endpoint::INIT, Endpoint::INIT, 7, 200);
        let seed = plan_start(true, SlotVerdict::Occupied, &init).expect("init START");
        assert_eq!(seed.endpoint, seed.parent);
        assert_eq!(seed.priority.get(), USER_Q);
        assert_eq!(seed.time_slice_ms, DEFAULT_USER_TIME_SLICE);
    }

    #[test]
    fn test_inherit_birth() {
        let parent_state = ParentState {
            priority: Priority::new(9).expect("9 < 16"),
            time_slice_ms: 150,
        };
        let req = inherit_req(Endpoint(21), Endpoint(12), 7);
        // The child opens where the parent runs, with the parent's share;
        // the ceiling stays the message's (`162` + `207-208`).
        let seed = plan_inherit(
            true,
            SlotVerdict::Occupied,
            SlotVerdict::Occupied,
            &req,
            &parent_state,
        )
        .expect("valid INHERIT");
        assert_eq!(seed.max_priority.get(), 7);
        assert_eq!(seed.priority.get(), 9);
        assert_eq!(seed.time_slice_ms, 150);
        // A dead parent refuses; the birth never happens (`203-206`).
        assert_eq!(
            plan_inherit(true, SlotVerdict::Occupied, SlotVerdict::Dead, &req, &parent_state),
            Err(EDEADEPT)
        );
        // A self-parented INHERIT refuses the same way: the parent slot is
        // the (still unflagged) child slot, so its verdict reads Dead —
        // no special case needed (`171` + `223` ordering does it).
        let self_req = inherit_req(Endpoint(22), Endpoint(22), 7);
        assert_eq!(
            plan_inherit(
                true,
                SlotVerdict::Occupied,
                SlotVerdict::Dead,
                &self_req,
                &parent_state
            ),
            Err(EDEADEPT)
        );
    }

    #[test]
    fn test_fanout_retry() {
        // Only EBADCPU circles back (`227`); all else leaves the ring.
        assert_eq!(classify_fanout(EBADCPU), Fanout::CpuDead);
        assert_eq!(classify_fanout(0), Fanout::Done(0));
        assert_eq!(classify_fanout(EINVAL), Fanout::Done(EINVAL));
        // The wire value pins the contract (`errno.h:213`).
        assert_eq!(EBADCPU, 217);
    }
}
