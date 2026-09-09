//! The demotion arm: the kernel speaks, one queue yields.
//!
//! Mirrors `do_noquantum()`
//! (`minix3/minix/servers/sched/schedule.c:87-109`).
//! 08-noquantum-nice.md.
//!
//! The arm owns the role split and nothing else: which spent slot may be
//! demoted, and by how much. The sender is *not* checked here — there is
//! no `accept_message` in this handler (`92` reads `m_source` straight):
//! the seal was verified one level up, by the main loop's
//! `IPC_FLG_MSG_FROM_KERNEL` gate (`main.c:68-84`, 02 owns it). Trust
//! asymmetry is the whole point of this arm, and the type says so by
//! taking no sender argument at all.
//!
//! Single-threaded event loop: pure functions, no shared state.

use crate::priority::MIN_USER_Q;
use crate::schedproc::Priority;
use crate::table::{OccupiedSlot, SlotVerdict};
use minix_types::Endpoint;

/// A spent-quantum report: who ran dry (`m_source`, `schedule.c:92`).
///
/// One field. The kernel names the spender by source, not by message
/// body — there is no `NO_QUANTUM` body to parse, only a mourner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// The process whose quantum expired. C: `m_ptr->m_source`.
    pub source: Endpoint,
}

/// Check the slot (`92-96`).
///
/// No sender verdict arrives — the arm never learned the sender's name,
/// so it cannot judge it. The caller proves the kernel's seal *before*
/// calling (dispatch, 02); here only the slot is judged, and a dead one
/// earns its verdict. Forgery defence lives at the loop's gate, never
/// here. On passage the probed slot rides back — the demotion needs the
/// row's current queue, the caller needs the row's index to write.
pub fn admit(slot: &Result<OccupiedSlot, SlotVerdict>) -> Result<&OccupiedSlot, i32> {
    slot.as_ref().map_err(|v| v.errno())
}

/// Demote one queue (`99-101`).
///
/// `priority += 1` unless already at the floor (`MIN_USER_Q`): the spent
/// sink one step, the bottomed stay. The floor cap makes construction
/// total — every input lands in `0..=15` — so `None` is defensive only
/// (it fires solely if the constants above ever drift apart); today
/// every priority demotes to `Some`.
pub fn demote(current: Priority) -> Option<Priority> {
    let queue = current.get().saturating_add(1).min(MIN_USER_Q);
    Priority::new(queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::CpuId;
    use crate::schedproc::{SchedProc, SlotState};
    use minix_types::{EBADEPT, EDEADEPT, EINVAL};

    fn live() -> Result<OccupiedSlot, SlotVerdict> {
        Ok(OccupiedSlot {
            index: 20,
            row: SchedProc {
                endpoint: Endpoint(20),
                parent: Endpoint(0),
                state: SlotState::InUse,
                max_priority: Priority::new(5).expect("5 < 16"),
                priority: Priority::new(5).expect("5 < 16"),
                time_slice_ms: 100,
                cpu: CpuId(0),
            },
        })
    }

    #[test]
    fn test_door_without_sender() {
        // The arm takes no sender: dead slots refuse, live pass (`92-96`).
        // Forgery defence lives one level up (main.c:68-84), not here —
        // the signature itself is the trust model.
        assert!(admit(&live()).is_ok());
        assert_eq!(admit(&Err(SlotVerdict::Dead)), Err(EDEADEPT));
        assert_eq!(admit(&Err(SlotVerdict::Task)), Err(EBADEPT));
        assert_eq!(admit(&Err(SlotVerdict::OutOfRange)), Err(EINVAL));
    }

    #[test]
    fn test_demote_one_step() {
        // One step down the queues (`99-101`); the floor holds.
        let step = |q: u8| Priority::new(q).and_then(demote).map(|p| p.get());
        assert_eq!(step(0), Some(1));
        assert_eq!(step(7), Some(8));
        assert_eq!(step(14), Some(MIN_USER_Q));
        assert_eq!(step(MIN_USER_Q), Some(MIN_USER_Q));
        assert_eq!(MIN_USER_Q, 15);
    }
}
