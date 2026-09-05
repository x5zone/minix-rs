//! Waiter slots and completion: where suspended operations live.
//!
//! C: `iproc[NR_PROCS]` / `inc_susp_count` / `dec_susp_count` /
//! `send_reply` / `complete_semop` / `sem_process_event`
//! (sem.c:6-14/:163-200/:207-215/:223-244/:866-888).
//! Document `06-ipc-semop.md` §3 (decisions D1/D4).
//!
//! Slot occupancy is an [`Option`] (the single-suspension invariant as a
//! type — plan A-2): a free slot cannot be mistaken for a parked waiter.
//! Completion returns [`Wakeup`] values; the service layer sends them.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use minix_types::{EIDRM, EINTR, Endpoint, NR_PROCS, SEMMNI};

use super::NO_REPLY;
use super::op::SemOp;
use super::table::{SemSet, Wakeup};

// ============================================================================
// Records and table
// ============================================================================

/// One parked operation: who waits, on what, stuck where.
///
/// C: one `iproc` element (sem.c:6-14) minus the intrusive queue links
/// (the queue is a separate index deque below) and minus the endpoint-slot
/// assertion (the slot *is* the endpoint slot here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiter {
    /// Waiting process endpoint. C: `ip_endpt`.
    pub endpoint: Endpoint,
    /// Waiting process id. C: `ip_pid`.
    pub pid: i32,
    /// Operation array copy. C: `ip_sops` (malloc'ed at entry).
    pub ops: Vec<SemOp>,
    /// Index of the blocking operation. C: `ip_blkop` (as an index: array
    /// reallocations never dangle it).
    pub blocked_on: usize,
    /// Set table index waited on. C: `ip_sem` (as an index: set removal
    /// drops the whole queue, never dangles it).
    pub set_index: usize,
}

/// Waiter slots plus per-set queues.
///
/// C: `iproc[NR_PROCS]` plus one `waiters` tail queue per set
/// (sem.c:14/:42). `slots[slot]` holds the waiter for the process in
/// endpoint slot `slot` (`_ENDPOINT_P`); `queues[set]` holds the slot
/// numbers queued on set `set`, head to tail (first-in-first-out).
pub struct WaiterTable {
    slots: [Option<Waiter>; NR_PROCS],
    queues: [VecDeque<usize>; SEMMNI],
}

impl Default for WaiterTable {
    fn default() -> Self {
        Self::new()
    }
}

impl WaiterTable {
    /// Empty table: no waiter parked anywhere.
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| None),
            queues: core::array::from_fn(|_| VecDeque::new()),
        }
    }

    /// Slot number for an endpoint (C: `_ENDPOINT_P(endpt)` — sem.c:751).
    ///
    /// Decodes through [`Endpoint::slot`] (generation-aware like the C
    /// macro): two generations of the same slot share the slot, and the
    /// occupant check in [`cancel`](Self::cancel) tells them apart.
    /// Negative slots (kernel tasks) and out-of-range slots cannot park
    /// (the C `assert(slot < NR_PROCS)` becomes a `None` here — the entry
    /// path maps it to `EINVAL`).
    pub const fn slot_of(endpoint: Endpoint) -> Option<usize> {
        let slot = endpoint.slot();
        if slot >= 0 && (slot as usize) < NR_PROCS {
            Some(slot as usize)
        } else {
            None
        }
    }

    /// Park a waiter: occupy its endpoint slot, append to the set tail,
    /// bump the blocked semaphore's suspension count.
    ///
    /// C: the `SUSPEND` arm of `do_semop` (sem.c:744-769) plus
    /// `inc_susp_count` (:163-178). Panics when the slot is already
    /// occupied — the C `assert(ip->ip_sem == NULL)` (:755), kept as a
    /// loud programming error rather than a wire error.
    pub fn park(&mut self, set: &mut SemSet, set_index: usize, waiter: Waiter) {
        let slot = Self::slot_of(waiter.endpoint).expect("parked endpoint has a slot");
        assert!(
            self.slots[slot].is_none(),
            "endpoint slot already parked (single-suspension invariant)"
        );
        bump_count(set, &waiter, 1);
        self.slots[slot] = Some(waiter);
        self.queues[set_index].push_back(slot);
    }

    /// Queue length for one set.
    pub fn queue_len(&self, set_index: usize) -> usize {
        self.queues[set_index].len()
    }

    /// Slot number at one queue position (for ordered retry walks).
    pub fn queue_slot(&self, set_index: usize, position: usize) -> usize {
        self.queues[set_index][position]
    }

    /// Read-only view of a parked waiter for trial runs.
    ///
    /// Returns the operation slice, the process id, the endpoint, and the
    /// recorded blocking index. Panics on a free slot (internal caller
    /// error — queue members are always occupied).
    pub fn waiter_view(&self, slot: usize) -> (&[SemOp], i32, Endpoint, usize) {
        let waiter = self.slots[slot].as_ref().expect("queued slot occupied");
        (&waiter.ops, waiter.pid, waiter.endpoint, waiter.blocked_on)
    }

    /// Complete the waiter in one slot: dequeue, drop the suspension
    /// count, free the slot and its array copy.
    ///
    /// C: `complete_semop` minus the send (sem.c:223-244). Returns the
    /// endpoint so the caller can build the [`Wakeup`]; the reply code
    /// comes from the completion reason (see [`drain_set`], [`cancel`],
    /// and `op.rs::retry`).
    pub fn complete_slot(&mut self, slot: usize, set: &mut SemSet) -> Endpoint {
        let waiter = self.slots[slot].take().expect("queued slot occupied");
        bump_count(set, &waiter, -1);
        let queue = &mut self.queues[waiter.set_index];
        let position = queue
            .iter()
            .position(|&s| s == slot)
            .expect("parked slot is queued");
        queue.remove(position);
        waiter.endpoint
    }

    /// Move a still-blocked waiter's suspension count to a new blocking
    /// point (C: `check_set` migration — sem.c:408-420).
    pub fn migrate_block(&mut self, slot: usize, set: &mut SemSet, from: usize, to: usize) {
        let waiter = self.slots[slot].as_mut().expect("queued slot occupied");
        let mut old = waiter.clone();
        old.blocked_on = from;
        bump_count(set, &old, -1);
        waiter.blocked_on = to;
        let moved = waiter.clone();
        bump_count(set, &moved, 1);
    }

    /// Drain one set's queue: free every slot, one `EIDRM` wake-up each, in
    /// queue order.
    ///
    /// C: the waiter loop of `remove_set` (sem.c:259-263). Suspension
    /// counts die with the set — no decrements needed. Called *before*
    /// `table.remove(index)`.
    pub fn drain_set(&mut self, set_index: usize) -> Vec<Wakeup> {
        let mut wakes = Vec::new();
        while let Some(slot) = self.queues[set_index].pop_front() {
            let waiter = self.slots[slot].take().expect("queued slot occupied");
            wakes.push(Wakeup {
                endpoint: waiter.endpoint,
                code: EIDRM,
            });
        }
        wakes
    }

    /// Cancel one endpoint's wait on a process event.
    ///
    /// C: `sem_process_event` (sem.c:866-888). Not parked → nothing
    /// (`None`). Exited → free silently (`NO_REPLY`: the process is gone).
    /// Signalled → wake with `EINTR`.
    pub fn cancel(&mut self, set: &mut SemSet, endpoint: Endpoint, exited: bool) -> Option<Wakeup> {
        let slot = Self::slot_of(endpoint)?;
        self.slots[slot].as_ref()?;
        // Occupant must be the event's own endpoint (sem.c:880 asserts
        // `ip_endpt == endpt`): same assert here, a programming error if
        // it fires, never a wire error.
        assert_eq!(
            self.slots[slot].as_ref().expect("checked above").endpoint,
            endpoint,
            "slot occupant mismatch"
        );
        let endpoint = self.complete_slot(slot, set);
        if exited {
            None
        } else {
            Some(Wakeup {
                endpoint,
                code: EINTR,
            })
        }
    }

    /// Whether a slot is occupied (for invariant tests).
    #[cfg(test)]
    pub fn is_parked(&self, slot: usize) -> bool {
        self.slots[slot].is_some()
    }
}

/// Bump the blocked semaphore's suspension count up or down by one.
///
/// C: `inc_susp_count` / `dec_susp_count` (sem.c:163-200): non-zero
/// blocking operations count toward increase-waiters, zero operations
/// toward zero-waiters. Saturating in release, asserted in debug (C
/// asserts the `u16` bounds both ways).
fn bump_count(set: &mut SemSet, waiter: &Waiter, delta: i32) {
    let op = &waiter.ops[waiter.blocked_on];
    let counter = if op.op != 0 {
        &mut set.sems[op.num as usize].raise_waiters
    } else {
        &mut set.sems[op.num as usize].zero_waiters
    };
    if delta > 0 {
        debug_assert!(*counter < u16::MAX);
        *counter = counter.saturating_add(1);
    } else {
        debug_assert!(*counter > 0);
        *counter = counter.saturating_sub(1);
    }
}

/// Suppression marker passthrough (exit path produces no message).
pub const SUPPRESSED_WAKE: i32 = NO_REPLY;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::Identity;
    use crate::sem::op::SemOp;
    use crate::sem::table::SemaphoreTable;

    fn waiter_on(endpoint: i32, blocked_on: usize) -> Waiter {
        Waiter {
            endpoint: Endpoint(endpoint),
            pid: 1000 + endpoint,
            ops: alloc::vec![SemOp {
                num: 0,
                op: -1,
                flag: 0
            }],
            blocked_on,
            set_index: 0,
        }
    }

    fn live_set() -> (SemaphoreTable, usize) {
        let mut table = SemaphoreTable::new();
        table
            .create(1, 1, 0o1000 | 0o600, Identity { uid: 1, gid: 1 }, 0)
            .unwrap();
        (table, 0)
    }

    #[test]
    fn park_rejects_double() {
        // C: sem.c:755 — one suspension per process, asserted.
        let (mut table, index) = live_set();
        let mut waiters = WaiterTable::new();
        let set = table.get_mut(index).unwrap();
        waiters.park(set, index, waiter_on(10, 0));
        assert!(waiters.is_parked(10));
        assert_eq!(table.get(0).unwrap().sems[0].raise_waiters, 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let set = table.get_mut(index).unwrap();
            waiters.park(set, index, waiter_on(10, 0));
        }));
        assert!(result.is_err(), "double park must panic");
    }

    #[test]
    fn cancel_exit_suppresses() {
        // C: sem.c:887 — exited processes complete with EDONTREPLY.
        let (mut table, index) = live_set();
        let mut waiters = WaiterTable::new();
        let set = table.get_mut(index).unwrap();
        waiters.park(set, index, waiter_on(10, 0));
        let set = table.get_mut(index).unwrap();
        assert_eq!(waiters.cancel(set, Endpoint(10), true), None);
        assert!(!waiters.is_parked(10));
        assert_eq!(table.get(0).unwrap().sems[0].raise_waiters, 0);
    }

    #[test]
    fn cancel_signal_replies_intr() {
        // C: sem.c:887 — signalled processes wake with EINTR.
        let (mut table, index) = live_set();
        let mut waiters = WaiterTable::new();
        let set = table.get_mut(index).unwrap();
        waiters.park(set, index, waiter_on(10, 0));
        // A stranger's event touches nothing.
        let set = table.get_mut(index).unwrap();
        assert_eq!(waiters.cancel(set, Endpoint(11), false), None);
        assert!(waiters.is_parked(10));
        let set = table.get_mut(index).unwrap();
        assert_eq!(
            waiters.cancel(set, Endpoint(10), false),
            Some(Wakeup {
                endpoint: Endpoint(10),
                code: EINTR
            })
        );
        assert!(!waiters.is_parked(10));
    }

    #[test]
    fn generation_endpoints_share_slot() {
        // C: `_ENDPOINT_P` decodes the slot part only (sem.c:751/871) —
        // two generations of slot 10 share slot 10, and the occupant
        // check (sem.c:880) tells them apart.
        let old = Endpoint::from_generation_slot(1, 10);
        let new = Endpoint::from_generation_slot(2, 10);
        assert_eq!(
            (WaiterTable::slot_of(old), WaiterTable::slot_of(new)),
            (Some(10), Some(10))
        );
        let (mut table, index) = live_set();
        let mut waiters = WaiterTable::new();
        let set = table.get_mut(index).unwrap();
        waiters.park(
            set,
            index,
            Waiter {
                endpoint: old,
                pid: 1,
                ops: alloc::vec![SemOp {
                    num: 0,
                    op: -1,
                    flag: 0
                }],
                blocked_on: 0,
                set_index: index,
            },
        );
        // An event for the newer generation hits the same slot but a
        // different occupant: programming error, like the C assert.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let set = table.get_mut(index).unwrap();
            waiters.cancel(set, new, false);
        }));
        assert!(result.is_err(), "occupant mismatch must panic");
        // The rightful owner's signal still cancels normally.
        let set = table.get_mut(index).unwrap();
        assert_eq!(waiters.cancel(set, old, false).map(|w| w.code), Some(EINTR));
    }

    #[test]
    fn drain_set_wakes_eidrm_in_order() {
        // C: sem.c:259-263 — head-to-tail EIDRM wakes on removal.
        let (mut table, index) = live_set();
        let mut waiters = WaiterTable::new();
        for endpoint in [30, 10, 20] {
            let set = table.get_mut(index).unwrap();
            waiters.park(set, index, waiter_on(endpoint, 0));
        }
        let wakes = waiters.drain_set(index);
        assert_eq!(
            wakes,
            [
                Wakeup {
                    endpoint: Endpoint(30),
                    code: EIDRM
                },
                Wakeup {
                    endpoint: Endpoint(10),
                    code: EIDRM
                },
                Wakeup {
                    endpoint: Endpoint(20),
                    code: EIDRM
                }
            ]
        );
        assert_eq!(waiters.queue_len(index), 0);
    }
}
