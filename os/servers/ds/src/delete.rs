//! DS delete: remove what was published, and clean up after labels.
//!
//! Mirrors `do_delete` (`minix3/minix/servers/ds/store.c:583-651`).
//! 09-ds-delete.md.
//!
//! The module owns the verdict and the table edit, nothing else: key
//! bounds, lookup, the owner check, the per-type teardown, and the
//! label cascade. Heap release (A-3) and grant transport (02/12) stay
//! out: byte-range buffers are handed back to the caller for release,
//! and subscriber notification rides the sweep in `notify.rs` (10).
//!
//! Single-threaded event loop: verdicts are pure; the apply step takes
//! `&mut` tables from the caller, no shared state.

use minix_types::{DSF_MASK_TYPE, EINVAL, EPERM, ESRCH, DsFlags};

use crate::publish::check_key_len;
use crate::slots::{EntrySlot, key_eq, lookup_entry};
use crate::store::{DsStore, MemBody, NR_DS_KEYS};
use crate::subscription::DsSubs;

/// Why a delete is refused (`do_delete` error paths, store.c:593-638).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteReject {
    /// Nameless source. C: `EPERM` — store.c:593-595.
    UnknownSource,
    /// Bogus key length. C: `EINVAL` via `get_key_name` — store.c:598-599.
    BadKey,
    /// No entry under this name and type. C: `ESRCH` — store.c:602-603.
    NotFound,
    /// Caller is not the entry's owner. C: `EPERM` — store.c:606-607.
    ///
    /// Note this is a plain name comparison, not `check_auth`: delete
    /// ignores the `PRIV_*` gates entirely. An unguarded entry is still
    /// owner-only for deletion — C compares `owner` against `source`
    /// unconditionally.
    Forbidden,
    /// Type arm outside the four known arms. C: `EINVAL` — store.c:638.
    BadType,
}

impl DeleteReject {
    /// The Minix3 errno each refusal carries (no invented codes).
    pub const fn errno(self) -> i32 {
        match self {
            Self::UnknownSource | Self::Forbidden => EPERM,
            Self::BadKey | Self::BadType => EINVAL,
            Self::NotFound => ESRCH,
        }
    }
}

/// Where a delete lands (`dsp` with its teardown, store.c:602-645).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletePlan {
    /// The seat to clear.
    pub slot: EntrySlot,
    /// True when the entry is a label: deleting a label also clears
    /// every subscription and every entry owned by that label name
    /// (the cascade, store.c:613-636).
    pub cascade_label: bool,
}

/// Decide a delete (`do_delete` verdict, store.c:593-638).
///
/// Five steps, in C order: nameless source refuses; key bounds refuse;
/// name-and-type lookup misses; a non-owner refuses (plain comparison,
/// gates ignored); the type arm must be one of the four known arms.
///
/// `source` is the already-resolved caller name (`None` = an endpoint
/// with no published label, 05).
pub fn plan_delete(
    store: &DsStore,
    key: &[u8],
    key_len: usize,
    flags: DsFlags,
    source: Option<&[u8]>,
) -> Result<DeletePlan, DeleteReject> {
    let source = source.ok_or(DeleteReject::UnknownSource)?;
    if check_key_len(key_len).is_err() {
        return Err(DeleteReject::BadKey);
    }
    let ty = flags.intersection(DsFlags::from_bits_truncate(DSF_MASK_TYPE));
    let slot = lookup_entry(store, key, ty).ok_or(DeleteReject::NotFound)?;
    let entry = slot
        .get(store)
        .expect("lookup hit always seats an entry");
    if !key_eq(&entry.owner, source) {
        return Err(DeleteReject::Forbidden);
    }
    // C switches on the masked request type (:609): exactly one known
    // arm proceeds; anything else (zero, multi-bit, reserved) refuses.
    if ty == DsFlags::TYPE_U32 {
        Ok(DeletePlan {
            slot,
            cascade_label: false,
        })
    } else if ty == DsFlags::TYPE_LABEL {
        Ok(DeletePlan {
            slot,
            cascade_label: true,
        })
    } else if ty == DsFlags::TYPE_STR || ty == DsFlags::TYPE_MEM {
        Ok(DeletePlan {
            slot,
            cascade_label: false,
        })
    } else {
        Err(DeleteReject::BadType)
    }
}

/// What clearing a seat hands back for release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteEffect {
    /// Seats cleared in the entry table (1, plus cascade victims).
    pub cleared_entries: usize,
    /// Subscription seats cleared by a label cascade (0 otherwise).
    pub cleared_subs: usize,
    /// Byte-range buffers handed back for release (A-3).
    pub heap_buffers: usize,
}

/// Carry out a planned delete (`do_delete` teardown, store.c:609-648).
///
/// Clears the target seat; for labels, first clears every subscription
/// owned by the label name (:616-621) and every entry owned by it
/// (:624-631). Stale notify bits for cleared indices are wiped from all
/// subscriber maps — observationally identical to C's per-victim
/// `update_subscribers(…, 0)`, since only matching subscribers ever
/// hold the bit.
///
/// Byte-range buffers (`STR`/`MEM`) are *not* freed here: there is no
/// global allocator yet (A-3). Each buffer is copied into `heap_out`
/// (caller-provided scratch, one slot per entry seat) for the owner to
/// release; `effect.heap_buffers` counts them.
///
/// Notification (`ipc_notify`, 10) stays out: the caller runs the sweep
/// in `notify.rs` with the cleared indices.
pub fn apply_delete(
    store: &mut DsStore,
    subs: &mut DsSubs,
    plan: DeletePlan,
    heap_out: &mut [Option<MemBody>; NR_DS_KEYS],
) -> DeleteEffect {
    let mut effect = DeleteEffect {
        cleared_entries: 0,
        cleared_subs: 0,
        heap_buffers: 0,
    };
    // Read the victim's lanes before clearing: the cascade keys on them.
    let (victim_key, victim_owner, victim_is_wide) = match store[plan.slot.index()] {
        Some(entry) => {
            let wide = entry.flags.intersects(DsFlags::TYPE_STR | DsFlags::TYPE_MEM);
            (entry.key, entry.owner, wide)
        }
        None => return effect,
    };

    if plan.cascade_label {
        // Subscriptions owned by the label name go first (:616-621).
        // C calls free_sub_slot (assert + regfree + clear); the Rust
        // seat holds no engine formula yet (A-2), so clearing is total.
        for seat in subs.iter_mut() {
            if let Some(sub) = seat
                && !sub.is_vacant()
                && key_eq(&sub.owner, &victim_key)
            {
                *seat = None;
                effect.cleared_subs += 1;
            }
        }
        // Entries owned by the label name follow (:624-631).
        for (index, seat) in store.iter_mut().enumerate() {
            if let Some(entry) = seat
                && !entry.is_vacant()
                && key_eq(&entry.owner, &victim_key)
            {
                take_heap_buffer(entry, heap_out, &mut effect);
                *seat = None;
                effect.cleared_entries += 1;
                clear_notify_bit(subs, index);
            }
        }
    } else if victim_is_wide {
        // STR/MEM: hand the buffer back (C: `free(data)`, :635).
        if let Some(entry) = store[plan.slot.index()] {
            take_heap_buffer(&entry, heap_out, &mut effect);
        }
    }

    // The victim itself: notify bits wiped, seat cleared
    // (C: `update_subscribers(dsp, 0)` at :642, `flags = 0` at :645).
    clear_notify_bit(subs, plan.slot.index());
    if store[plan.slot.index()].is_some() {
        store[plan.slot.index()] = None;
        effect.cleared_entries += 1;
    }
    // Silence unused binding when neither cascade nor wide applied.
    let _ = victim_owner;
    effect
}

/// Copy a byte-range buffer into the caller's release scratch.
fn take_heap_buffer(
    entry: &crate::store::DataEntry,
    heap_out: &mut [Option<MemBody>; NR_DS_KEYS],
    effect: &mut DeleteEffect,
) {
    // SAFETY: wide-arm entries are written through the wide arm only
    // (publish path, 07); reading the buffer descriptor back is the
    // arm's documented use. Only the descriptor moves — the bytes
    // stay where the owner put them until released.
    let buffer = unsafe { entry.body.mem };
    if effect.heap_buffers < heap_out.len() {
        heap_out[effect.heap_buffers] = Some(buffer);
    }
    effect.heap_buffers += 1;
}

/// Wipe one entry index from every subscriber's told-map.
fn clear_notify_bit(subs: &mut DsSubs, index: usize) {
    for seat in subs.iter_mut() {
        if let Some(sub) = seat
            && !sub.is_vacant()
        {
            sub.old_subs.set(index, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DataBody, DataEntry};
    use crate::subscription::{NR_DS_SUBS, Subscription};
    use minix_types::{DS_MAX_KEYLEN, DsFlags};

    fn owned_entry(key: &[u8], owner: &[u8], flags: DsFlags) -> DataEntry {
        let mut entry = DataEntry {
            flags,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        };
        entry.key[..key.len()].copy_from_slice(key);
        entry.owner[..owner.len()].copy_from_slice(owner);
        entry
    }

    fn test_store() -> DsStore {
        let mut store: DsStore = [None; NR_DS_KEYS];
        store[0] = Some(owned_entry(
            b"cnt",
            b"vfs",
            DsFlags::IN_USE | DsFlags::TYPE_U32,
        ));
        store
    }

    fn empty_heap() -> [Option<MemBody>; NR_DS_KEYS] {
        [None; NR_DS_KEYS]
    }

    #[test]
    fn test_owner_deletes_plain_entry() {
        let store = test_store();
        let plan = plan_delete(&store, b"cnt", 4, DsFlags::TYPE_U32, Some(b"vfs"))
            .expect("owner must delete");
        assert!(!plan.cascade_label);
        let mut store = store;
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        let mut heap = empty_heap();
        let effect = apply_delete(&mut store, &mut subs, plan, &mut heap);
        assert_eq!(effect.cleared_entries, 1);
        assert_eq!(effect.heap_buffers, 0);
        assert!(store[0].is_none());
    }

    #[test]
    fn test_stranger_refuses_even_unguarded() {
        // Delete ignores PRIV gates: only the owner deletes, even when
        // no gate is set (store.c:606-607).
        let store = test_store();
        assert_eq!(
            plan_delete(&store, b"cnt", 4, DsFlags::TYPE_U32, Some(b"pm")),
            Err(DeleteReject::Forbidden)
        );
    }

    #[test]
    fn test_nameless_source_is_eperm() {
        let store = test_store();
        assert_eq!(
            plan_delete(&store, b"cnt", 4, DsFlags::TYPE_U32, None),
            Err(DeleteReject::UnknownSource)
        );
    }

    #[test]
    fn test_missing_entry_is_esrch() {
        let store = test_store();
        assert_eq!(
            plan_delete(&store, b"ghost", 6, DsFlags::TYPE_U32, Some(b"vfs")),
            Err(DeleteReject::NotFound)
        );
    }

    #[test]
    fn test_label_delete_cascades() {
        // Deleting a label clears subscriptions and entries owned by
        // that label name (store.c:613-631).
        let mut store: DsStore = [None; NR_DS_KEYS];
        store[0] = Some(owned_entry(
            b"svc",
            b"rs",
            DsFlags::IN_USE | DsFlags::TYPE_LABEL,
        ));
        store[1] = Some(owned_entry(
            b"cfg",
            b"svc",
            DsFlags::IN_USE | DsFlags::TYPE_U32,
        ));
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        let mut sub = Subscription::vacant();
        sub.flags = DsFlags::IN_USE | DsFlags::TYPE_U32;
        sub.owner[..3].copy_from_slice(b"svc");
        sub.old_subs.set(1, true);
        subs[0] = Some(sub);

        let plan = plan_delete(&store, b"svc", 4, DsFlags::TYPE_LABEL, Some(b"rs"))
            .expect("rs owns the label");
        assert!(plan.cascade_label);
        let mut heap = empty_heap();
        let effect = apply_delete(&mut store, &mut subs, plan, &mut heap);
        assert_eq!(effect.cleared_subs, 1);
        // Victim label + owned entry.
        assert_eq!(effect.cleared_entries, 2);
        assert!(store[0].is_none() && store[1].is_none());
        assert!(subs[0].is_none());
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(DeleteReject::UnknownSource.errno(), EPERM);
        assert_eq!(DeleteReject::Forbidden.errno(), EPERM);
        assert_eq!(DeleteReject::BadKey.errno(), EINVAL);
        assert_eq!(DeleteReject::BadType.errno(), EINVAL);
        assert_eq!(DeleteReject::NotFound.errno(), ESRCH);
    }
}
