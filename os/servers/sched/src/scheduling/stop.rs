//! The release arm: one letter, two verdicts, two releases.
//!
//! Mirrors `do_stop_scheduling()`
//! (`minix3/minix/servers/sched/schedule.c:112-137`).
//! 07-stop-scheduling.md.
//!
//! The arm owns the role split and nothing else: which letter may release
//! a slot, and what the caller must undo. Table reads arrive as verdicts
//! from the caller, who holds the table (04); clearing the slot and
//! debiting the CPU stay caller-side. The START arm (06) is the mirror:
//! birth plans forward, release plans backward, through the same door
//! order.
//!
//! Single-threaded event loop: pure functions, no shared state.

use crate::table::SlotVerdict;
use minix_types::{EPERM, Endpoint};

/// A release request: the message body (`ipc.h:1440-1444`).
///
/// One field. The STOP letter carries only *who* (`endpoint`); there is
/// no ceiling to check, no share to state, no parent to consult — the
/// dead need no numbers. The shape says so by having nowhere to put them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// The process to stop scheduling (`endpoint`, `ipc.h:1441`).
    pub child: Endpoint,
}

/// What the caller must undo (`schedule.c:128-132`).
///
/// Two releases: the load debit (`cpu_proc[cpu]--`, `130`, SMP builds)
/// and the flag clear (`flags = 0`, `132`). Both stay caller-side — the
/// caller holds the load table (10) and the slot (04); the arm only
/// names the turn, exactly as the START arm names births without
/// performing the takeover (06 D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release {
    /// Whose slot empties. C: `rmp` at schedule.c:128.
    pub endpoint: Endpoint,
    /// Which CPU loses one load unit. C: `rmp->cpu` at schedule.c:130.
    pub cpu: u32,
}

/// Check the sender, then the slot (`118-125`).
///
/// The door order is diagnosis, shared with the START arm (06 D2): a
/// stranger's letter earns `EPERM` even naming a live slot (`118-119`),
/// and a dead slot earns its verdict even from PM (`121-125`). The
/// mirror runs one step shorter — STOP verifies *occupancy* (`isokendpt`),
/// where START verifies *vacancy* (`isemtyendpt`, `154`): release checks
/// presence, birth checks absence.
pub fn admit(sender_ok: bool, slot: SlotVerdict) -> Result<(), i32> {
    if !sender_ok {
        return Err(EPERM);
    }
    if !slot.is_ok() {
        return Err(slot.errno());
    }
    Ok(())
}

/// Plan a release (`112-134`).
///
/// Past the door there is nothing to decide: no ceiling, no branch, no
/// retry ring — the slot empties and one CPU sheds a unit. `cpu` arrives
/// from the caller, who read it off the slot after the verdict passed
/// (table reads stay caller-side, 04 D2). The SMP gate stays caller-side
/// too: C debits only under `CONFIG_SMP` (`129-131`), so on a single CPU
/// the caller skips the debit — the `Release` still names the CPU, so the
/// rule has one home and the gate one reader (10 consumes this).
pub fn plan_stop(
    sender_ok: bool,
    slot: SlotVerdict,
    req: &Request,
    cpu: u32,
) -> Result<Release, i32> {
    admit(sender_ok, slot)?;
    Ok(Release {
        endpoint: req.child,
        cpu,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{check_occupied, check_vacant};
    use minix_types::{EBADEPT, EDEADEPT, EINVAL};

    #[test]
    fn test_doors_in_order() {
        let req = Request { child: Endpoint(20) };
        let live = SlotVerdict::Occupied;
        // Strangers refuse first, even naming a live slot (`118-119`).
        assert_eq!(plan_stop(false, live, &req, 0), Err(EPERM));
        // Dead slots refuse next, even from PM (`121-125`).
        assert_eq!(
            plan_stop(true, SlotVerdict::Dead, &req, 0),
            Err(EDEADEPT)
        );
        assert_eq!(
            plan_stop(true, SlotVerdict::Task, &req, 0),
            Err(EBADEPT)
        );
        assert_eq!(
            plan_stop(true, SlotVerdict::OutOfRange, &req, 0),
            Err(EINVAL)
        );
        // A live slot passes (`126` falls through to release).
        assert!(plan_stop(true, live, &req, 0).is_ok());
    }

    #[test]
    fn test_release_shape() {
        // Two releases ride one answer: who empties, which CPU sheds.
        let req = Request { child: Endpoint(20) };
        let release = plan_stop(true, SlotVerdict::Occupied, &req, 3)
            .expect("valid STOP");
        assert_eq!(release.endpoint, Endpoint(20));
        assert_eq!(release.cpu, 3);
    }

    #[test]
    fn test_no_special_slots() {
        // Release has no init branch: INIT, system children, and user
        // processes all release alike — unlike births (06 §1.3), death
        // asks nothing about parentage.
        for child in [Endpoint::INIT, Endpoint::RS, Endpoint(20), Endpoint(100)] {
            let req = Request { child };
            let release = plan_stop(true, SlotVerdict::Occupied, &req, 0)
                .expect("every slot releases");
            assert_eq!(release.endpoint, child);
        }
    }

    #[test]
    fn test_start_stop_roundtrip() {
        // A full life through both arms with real doors: START admits a
        // vacant slot (06), STOP admits the now-occupied one, and the
        // vacant door agrees afterwards that the slot is free again.
        let slot = 20;
        let len = 256;
        // Birth: vacant claim passes (`utility.c:46-56` mirror logic).
        assert!(check_vacant(slot, len, false).is_ok());
        // Life: occupied claim passes (`utility.c:29-41`).
        let live = check_occupied(slot, len, true, true);
        let req = Request {
            child: Endpoint(slot),
        };
        let release = plan_stop(true, live, &req, 1).expect("live releases");
        assert_eq!(release.cpu, 1);
        // Death: the slot reads free again — the caller's clear (`132`)
        // restores exactly what the vacant door checks.
        assert!(check_vacant(slot, len, false).is_ok());
        assert!(!check_occupied(slot, len, true, false).is_ok());
    }
}
