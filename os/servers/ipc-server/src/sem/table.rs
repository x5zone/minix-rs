//! Semaphore-set table: ten slots, one life cycle.
//!
//! C: `sem_list` / `sem_list_nr` / `sem_find_key` / `sem_find_id` /
//! `do_semget` / `remove_set` / `is_sem_nil`
//! (sem.c:39-46/:53-92/:93-156/:251-281/:854-858).
//! Document `05-ipc-sem-table.md` §3 (decisions D1-D4).
//!
//! The table never touches effects: event-subscription changes come back
//! as [`TableEffect`], wake-ups come back as [`Wakeup`] lists. The service
//! layer executes them (documents 09/06).

use minix_types::{
    ACCESSPERMS, EACCES, IPC_CREAT, IPC_EXCL, IPC_PRIVATE, SEM_ALLOC, SEM_SEQ_MASK, SEMMNI, SEMMSL,
};

use super::{NO_REPLY, SemError};
use crate::perms::{Identity, IpcPerm, check_perm};

// ============================================================================
// Data
// ============================================================================

/// One semaphore: value plus the two waiter counters plus the last actor.
///
/// C: `struct semaphore` — sem.c:16-21. The counters are maintained by the
/// operation module (`op.rs`); the table only stores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Semaphore {
    /// Current value. C: `semval`.
    pub value: u16,
    /// Processes waiting for zero. C: `semzcnt`.
    pub zero_waiters: u16,
    /// Processes waiting for increase. C: `semncnt`.
    pub raise_waiters: u16,
    /// Process that did the last operation. C: `sempid`.
    pub last_pid: i32,
}

impl Semaphore {
    /// Fresh semaphore: zero value, nobody waiting, no actor yet.
    pub const fn new() -> Self {
        Self {
            value: 0,
            zero_waiters: 0,
            raise_waiters: 0,
            last_pid: 0,
        }
    }
}

impl Default for Semaphore {
    fn default() -> Self {
        Self::new()
    }
}

/// One live set: identity plus values plus times.
///
/// C: `struct sem_struct` — sem.c:39-43 (descriptor `semid_ds` plus the
/// `sems` array; the waiter queue head lives with the operation module).
/// Permission fields reuse the shared [`IpcPerm`] (document 04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemSet {
    /// Permission record (key, owners, mode with `SEM_ALLOC`, sequence).
    pub perm: IpcPerm,
    /// Number of semaphores. C: `sem_nsems`.
    pub count: usize,
    /// Values (only the first `count` entries are live). C: `sems`.
    pub sems: [Semaphore; SEMMSL],
    /// Last operation time. C: `sem_otime`.
    pub op_time: u64,
    /// Last change time. C: `sem_ctime`.
    pub change_time: u64,
}

/// One table slot: either free or holding a set.
///
/// C: freeness hides in `mode & SEM_ALLOC`, tested at every site
/// (sem.c:58/:82/:120/:273). The enum makes occupancy a type
/// (document 05 §3 D1): a free slot cannot be mistaken for a set.
/// A free slot retains its last sequence number — C reads `seq` *before*
/// zeroing the slot on reuse (sem.c:127-128), so identifiers keep aging
/// across free/reuse cycles. Dropping it would reissue identifiers.
/// The `Used` variant dominates the enum size, but slots live in a fixed
/// array indexed in place — sets are never moved by value on any path —
/// so boxing the payload would only add allocation churn.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemSlot {
    /// Free slot, carrying the last sequence number.
    Free {
        /// Last sequence number (0 on a never-used slot).
        seq: u16,
    },
    /// Live set.
    Used(SemSet),
}

/// The set table: ten slots plus the high-water mark.
///
/// C: `sem_list[SEMMNI]` + `sem_list_nr` — sem.c:45-46. The mark always
/// equals the highest used slot plus one: lookups scan only up to it,
/// removal pulls it back over trailing free slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemaphoreTable {
    slots: [SemSlot; SEMMNI],
    high_water: usize,
}

/// Side effects of table mutations, returned — never performed here.
///
/// C calls `update_sem_sub` directly (sem.c:148/:280). Returning the
/// effect keeps this module free of cross-module calls (document 05 §3 D3);
/// the service layer forwards them to document 09.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableEffect {
    /// Nothing to do.
    None,
    /// First set was born: subscribe to process events.
    SubscribeEvents,
    /// Last set is gone: unsubscribe from process events.
    UnsubscribeEvents,
}

/// One wake-up owed to a waiter: destination plus reply code.
///
/// C: `complete_semop` sends from inside (sem.c:242-243). As a value, the
/// same shape serves both the remove path here and the completion path in
/// `waiter.rs` (document 05 §3 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wakeup {
    /// Waiting process endpoint.
    pub endpoint: minix_types::Endpoint,
    /// Reply code (`OK`, `EIDRM`, …; never sent when it equals NO_REPLY —
    /// the sender checks).
    pub code: i32,
}

impl Default for SemaphoreTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SemaphoreTable {
    /// Empty table: ten free slots, water at zero.
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| SemSlot::Free { seq: 0 }),
            high_water: 0,
        }
    }

    /// Number of live sets (== high-water mark after trailing shrink).
    pub fn live_count(&self) -> usize {
        self.high_water
    }

    /// Find a set by key. The key must not be private (the caller splits
    /// private keys into the create path first).
    ///
    /// C: `sem_find_key` — sem.c:53-65: scan up to the mark, skip free
    /// slots, match the key.
    pub fn find_key(&self, key: i32) -> Option<usize> {
        for (i, slot) in self.slots.iter().enumerate().take(self.high_water) {
            if let SemSlot::Used(set) = slot
                && set.perm.key == key
            {
                return Some(i);
            }
        }
        None
    }

    /// Find a set by identifier: index from the low sixteen bits, then
    /// occupancy, then sequence match.
    ///
    /// C: `sem_find_id` — sem.c:72-87.
    pub fn find_id(&self, id: i32) -> Option<usize> {
        let index = (id & 0xffff) as usize;
        if index >= self.high_water {
            return None;
        }
        if let SemSlot::Used(set) = &self.slots[index]
            && set.perm.seq as i32 == ((id >> 16) & 0xffff)
        {
            return Some(index);
        }
        None
    }

    /// Borrow a live set by slot index.
    pub fn get(&self, index: usize) -> Option<&SemSet> {
        match self.slots.get(index) {
            Some(SemSlot::Used(set)) => Some(set),
            _ => None,
        }
    }

    /// Mutably borrow a live set by slot index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut SemSet> {
        match self.slots.get_mut(index) {
            Some(SemSlot::Used(set)) => Some(set),
            _ => None,
        }
    }

    /// Create-or-open a set (`semget`).
    ///
    /// C: `do_semget` — sem.c:93-156. Returns the identifier plus the
    /// subscription effect. `now` is the creation/change timestamp
    /// (`clock_time`, injected for testability).
    pub fn create(
        &mut self,
        key: i32,
        count: i32,
        flag: i32,
        caller: Identity,
        now: u64,
    ) -> Result<(i32, TableEffect), SemError> {
        // Existing-set branch (sem.c:104-111).
        if key != IPC_PRIVATE {
            if let Some(index) = self.find_key(key) {
                if flag & IPC_CREAT != 0 && flag & IPC_EXCL != 0 {
                    return Err(SemError::Exists);
                }
                let set = self.get(index).expect("find_key returned live slot");
                if !check_perm(&set.perm, caller, flag as u32) {
                    return Err(SemError::Access);
                }
                if count > set.count as i32 {
                    return Err(SemError::Invalid);
                }
                return Ok((encode_id(index, set.perm.seq), TableEffect::None));
            }
            if flag & IPC_CREAT == 0 {
                return Err(SemError::Missing);
            }
        }
        // Fresh-set branch (sem.c:115-152).
        if count <= 0 || count as usize > SEMMSL {
            return Err(SemError::Invalid);
        }
        let index = self
            .slots
            .iter()
            .position(|s| matches!(s, SemSlot::Free { .. }))
            .ok_or(SemError::NoSpace)?;
        // C reads the stale sequence before zeroing (sem.c:127-128).
        let old_seq = match &self.slots[index] {
            SemSlot::Used(set) => set.perm.seq,
            SemSlot::Free { seq } => *seq,
        };
        let set = SemSet {
            perm: IpcPerm {
                key,
                uid: caller.uid,
                gid: caller.gid,
                creator_uid: caller.uid,
                creator_gid: caller.gid,
                mode: SEM_ALLOC | (flag as u32 & ACCESSPERMS),
                seq: next_seq(old_seq),
            },
            count: count as usize,
            sems: [Semaphore::new(); SEMMSL],
            op_time: 0,
            change_time: now,
        };
        let id = encode_id(index, set.perm.seq);
        self.slots[index] = SemSlot::Used(set);
        let mut effect = TableEffect::None;
        if index == self.high_water {
            if self.high_water == 0 {
                // First set born: subscribe (sem.c:147-148).
                effect = TableEffect::SubscribeEvents;
            }
            self.high_water += 1;
        }
        Ok((id, effect))
    }

    /// Remove a set: clear occupancy, pull back the mark, report the effect.
    ///
    /// C: `remove_set` — sem.c:251-281 minus the waiter loop. Queued
    /// waiters are woken by the waiter table (`waiter.rs::drain_set`,
    /// one `EIDRM` wake-up each, sem.c:259-263) *before* this runs; the
    /// set data dies here.
    pub fn remove(&mut self, index: usize) -> TableEffect {
        let seq = match &self.slots[index] {
            SemSlot::Used(set) => set.perm.seq,
            // Programming error (C calls remove_set only with a live set):
            // panics like the C asserts elsewhere, never a wire error.
            SemSlot::Free { .. } => panic!("remove of a free slot"),
        };
        self.slots[index] = SemSlot::Free { seq };
        while self.high_water > 0 && matches!(self.slots[self.high_water - 1], SemSlot::Free { .. })
        {
            self.high_water -= 1;
        }
        if self.high_water == 0 {
            // Last set gone: unsubscribe (sem.c:279-280).
            TableEffect::UnsubscribeEvents
        } else {
            TableEffect::None
        }
    }

    /// True when no set is allocated. C: `is_sem_nil` — sem.c:854-858.
    pub fn is_empty(&self) -> bool {
        self.high_water == 0
    }
}

/// Encode slot index plus sequence as an identifier.
///
/// C: `IXSEQ_TO_IPCID(ix, perm)` — sys/ipc.h:110: low sixteen bits index,
/// high sixteen bits sequence.
pub const fn encode_id(index: usize, seq: u16) -> i32 {
    ((seq as i32) << 16) | ((index as i32) & 0xffff)
}

/// Advance the sequence number, keeping fifteen bits.
///
/// C: `(seq + 1) & 0x7fff` — sem.c:135 (document 05 §3 D2).
pub const fn next_seq(seq: u16) -> u16 {
    ((seq as u32 + 1) & SEM_SEQ_MASK) as u16
}

/// Access-denied error code (re-export for mask-table readers).
pub const DENIED: i32 = EACCES;

/// Suppression marker passthrough (exit path produces no message).
pub const SUPPRESSED: i32 = NO_REPLY;

#[cfg(test)]
mod tests {
    use super::*;

    fn caller() -> Identity {
        Identity { uid: 100, gid: 200 }
    }

    #[test]
    fn create_new_assigns_id() {
        // C: sem.c:112-155 — fresh branch fills identity, mode, sequence.
        let mut table = SemaphoreTable::new();
        let (id, effect) = table.create(0x1234, 3, 0o1000, caller(), 999).unwrap();
        assert_eq!(effect, TableEffect::SubscribeEvents);
        assert_eq!(id & 0xffff, 0, "first slot is index zero");
        let set = table.get(0).unwrap();
        assert_eq!((set.count, set.change_time), (3, 999));
        assert_eq!((set.perm.uid, set.perm.key), (100, 0x1234));
        // Identifier round-trips through find_id.
        assert_eq!(table.find_id(id), Some(0));
        assert_eq!(table.find_key(0x1234), Some(0));
    }

    #[test]
    fn create_existing_checks() {
        // C: sem.c:104-111 — exclusive collision, permission, count cap.
        // Note the C quirk: a zero flag asks for zero bits, which the
        // final check rejects (utility.c:31) — reopening needs a read bit.
        let mut table = SemaphoreTable::new();
        let (id, _) = table.create(7, 2, 0o1000 | 0o600, caller(), 0).unwrap();
        // Exclusive re-create collides.
        assert_eq!(
            table.create(7, 2, 0o1000 | 0o2000, caller(), 0),
            Err(SemError::Exists)
        );
        // Same key with a read bit re-opens the same set.
        let (id2, effect) = table.create(7, 1, 0o400, caller(), 0).unwrap();
        assert_eq!((id2, effect), (id, TableEffect::None));
        // Asking for more than the set holds fails.
        assert_eq!(
            table.create(7, 3, 0o400, caller(), 0),
            Err(SemError::Invalid)
        );
        // A stranger with no bits gets access-denied.
        let stranger = Identity { uid: 999, gid: 999 };
        assert_eq!(
            table.create(7, 1, 0o600, stranger, 0),
            Err(SemError::Access)
        );
        // Non-private key without create flag misses.
        assert_eq!(
            table.create(0xBEEF, 1, 0o400, caller(), 0),
            Err(SemError::Missing)
        );
    }

    #[test]
    fn create_full_returns_nospc() {
        // C: sem.c:119-123 — ten slots, the eleventh fails.
        let mut table = SemaphoreTable::new();
        for k in 1..=SEMMNI as i32 {
            table.create(k, 1, 0o1000, caller(), 0).unwrap();
        }
        assert_eq!(
            table.create(999, 1, 0o1000, caller(), 0),
            Err(SemError::NoSpace)
        );
        // Bad counts fail before the full check matters.
        assert_eq!(
            table.create(IPC_PRIVATE, 0, 0o1000, caller(), 0),
            Err(SemError::Invalid)
        );
        assert_eq!(
            table.create(IPC_PRIVATE, SEMMSL as i32 + 1, 0o1000, caller(), 0),
            Err(SemError::Invalid)
        );
    }

    #[test]
    fn find_id_rejects_stale_seq() {
        // C: sem.c:84-85 — reused slot with a new sequence rejects old ids.
        let mut table = SemaphoreTable::new();
        let (id, _) = table.create(1, 1, 0o1000, caller(), 0).unwrap();
        let effect = table.remove(0);
        assert_eq!(effect, TableEffect::UnsubscribeEvents);
        assert_eq!(table.find_id(id), None, "old id must not match");
        // A fresh set in the same slot gets a new sequence.
        let (id2, _) = table.create(2, 1, 0o1000, caller(), 0).unwrap();
        assert_ne!(id2, id);
        assert_eq!(table.find_id(id2), Some(0));
    }

    #[test]
    fn remove_clears_and_shrinks() {
        // C: sem.c:266-280 — occupancy cleared, mark pulled back over
        // trailing free slots, unsubscribe when the table empties.
        // (Waiter wake-ups live in waiter.rs::drain_set, tested there.)
        let mut table = SemaphoreTable::new();
        table.create(1, 1, 0o1000, caller(), 0).unwrap();
        table.create(2, 1, 0o1000, caller(), 0).unwrap();
        assert_eq!(table.remove(0), TableEffect::None, "one set remains");
        // Slot 0 is free but the mark stays (slot 1 still live).
        assert_eq!(table.live_count(), 2);
        assert_eq!(table.get(0), None);
        // Removing the tail pulls the mark back over both free slots.
        assert_eq!(table.remove(1), TableEffect::UnsubscribeEvents);
        assert_eq!(table.live_count(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn subscribe_effect_edges() {
        // C: sem.c:147-148/:279-280 — subscribe on first birth,
        // unsubscribe on last death, silence in between.
        let mut table = SemaphoreTable::new();
        let (_, e1) = table.create(1, 1, 0o1000, caller(), 0).unwrap();
        let (_, e2) = table.create(2, 1, 0o1000, caller(), 0).unwrap();
        assert_eq!((e1, e2), (TableEffect::SubscribeEvents, TableEffect::None));
        assert_eq!(table.remove(0), TableEffect::None);
        assert_eq!(table.remove(1), TableEffect::UnsubscribeEvents);
    }
}
