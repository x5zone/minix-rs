//! DS notification sweep: tell subscribers what changed.
//!
//! Mirrors `update_subscribers` plus the `DSF_INITIAL` scan inside
//! `do_subscribe` (`minix3/minix/servers/ds/store.c:198-224, 511-528`).
//! 10-ds-subscribe-check.md (notify half).
//!
//! The module owns the sweep and nothing else: walk the subscription
//! table, keep the matching seats, flip their told-bits, and report
//! whom to wake. Sending the wake-up (`ipc_notify`, 02) stays out —
//! this is a pure function returning endpoints, testable without IPC.
//!
//! One deliberate deviation: C resolves every subscriber name through
//! `ds_getprocep`, which *panics* when the name has no label seat
//! (:213). A dead subscriber would therefore kill the store. Rust
//! skips stale names, leaves their bits untouched, and reports the
//! count — availability over crash, documented here and in the design
//! doc. Everything else follows C order exactly.
//!
//! Single-threaded event loop: pure functions over caller-held tables,
//! no shared state.

use minix_types::Endpoint;

use crate::identity::resolve_endpoint;
use crate::slots::EntrySlot;
use crate::store::DsStore;
use crate::subscribe::{PatternMatcher, entry_matches};
use crate::subscription::{DsSubs, NR_DS_SUBS, Subscription};

/// What one sweep did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepStats {
    /// Endpoints to wake (`ipc_notify` targets, in table order).
    pub notified: usize,
    /// Subscriber seats skipped: owner name resolves to no endpoint.
    /// C panics here (`ds_getprocep`, store.c:213/137); Rust skips.
    pub skipped_stale: usize,
}

/// Sweep after a publish or delete (`update_subscribers`, :198-224).
///
/// For the entry at `entry`: every live subscription whose type arm
/// intersects the entry's and whose match verdict passes gets its bit
/// set (`publish`, `set = true`) or cleared (`delete`, `set = false`),
/// and its endpoint appended to `out`. Returns how many seats were
/// visited with what outcome.
///
/// `out` is caller scratch with one lane per subscription seat; only
/// the first `stats.notified` lanes are written.
pub fn apply_update<M: PatternMatcher>(
    store: &DsStore,
    subs: &mut DsSubs,
    entry: EntrySlot,
    set: bool,
    engine: &M,
    out: &mut [Endpoint; NR_DS_SUBS],
) -> SweepStats {
    let mut stats = SweepStats::default();
    let target = match entry.get(store) {
        Some(e) => e,
        None => return stats,
    };
    // Copy the lanes C's loop reads repeatedly: the entry may be
    // cleared by the caller right after, but the sweep sees C order.
    let target_copy = *target;
    for seat in subs.iter_mut() {
        let Some(sub) = seat else { continue };
        if sub.is_vacant() {
            continue;
        }
        // Resolve first, as C does (:213 comes before the match at
        // :214): an unresolvable owner never reaches the match.
        let endpoint = match resolve_endpoint(store, &sub.owner) {
            Some(ep) => ep,
            None => {
                stats.skipped_stale += 1;
                continue;
            }
        };
        if !entry_matches(
            &target_copy,
            Some(&sub.owner),
            engine,
            sub,
            store,
            entry.index(),
        ) {
            continue;
        }
        if let Some(live) = seat {
            live.old_subs.set(entry.index(), set);
        }
        if stats.notified < out.len() {
            out[stats.notified] = endpoint;
        }
        stats.notified += 1;
    }
    stats
}

/// Immediate scan after subscribing (`DSF_INITIAL`, :511-528).
///
/// Walks the whole entry table for live entries that already match the
/// new subscription; each hit sets the subscriber's bit. Returns
/// whether the source should be woken once (`match_found` at :526):
/// `Some(source)` appends nothing to `out` — the caller wakes the
/// source directly.
pub fn initial_scan<M: PatternMatcher>(
    store: &DsStore,
    subs: &mut DsSubs,
    sub: crate::slots::SubSlot,
    source: Endpoint,
    subscriber_name: &[u8],
    engine: &M,
) -> Option<Endpoint> {
    let snapshot: Subscription = match subs[sub.index()] {
        Some(seat) if !seat.is_vacant() => seat,
        _ => return None,
    };
    let mut found = false;
    for (index, seat) in store.iter().enumerate() {
        let Some(entry) = seat else { continue };
        if entry.is_vacant() {
            continue;
        }
        if !entry_matches(entry, Some(subscriber_name), engine, &snapshot, store, index) {
            continue;
        }
        if let Some(live) = subs[sub.index()].as_mut() {
            live.old_subs.set(index, true);
        }
        found = true;
    }
    if found {
        Some(source)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DataBody, DataEntry, NR_DS_KEYS};
    use crate::subscribe::LiteralMatcher;
    use minix_types::{DS_MAX_KEYLEN, DsFlags};

    fn label_entry(key: &[u8], ep: u32) -> DataEntry {
        let mut entry = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_LABEL,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: ep },
        };
        entry.key[..key.len()].copy_from_slice(key);
        entry
    }

    fn u32_entry(key: &[u8], owner: &[u8], value: u32) -> DataEntry {
        let mut entry = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_U32,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: value },
        };
        entry.key[..key.len()].copy_from_slice(key);
        entry.owner[..owner.len()].copy_from_slice(owner);
        entry
    }

    /// Store with labels for rs/vfs/pm plus one watched entry.
    fn wired_store() -> DsStore {
        let mut store: DsStore = [None; NR_DS_KEYS];
        store[0] = Some(label_entry(b"rs", 2));
        store[1] = Some(label_entry(b"vfs", 9));
        store[2] = Some(label_entry(b"pm", 4));
        store[3] = Some(u32_entry(b"clk", b"rs", 1));
        store
    }

    fn sub_for(owner: &[u8], pattern: &[u8]) -> Subscription {
        let mut sub = Subscription::vacant();
        sub.flags = DsFlags::IN_USE | DsFlags::TYPE_U32;
        sub.owner[..owner.len()].copy_from_slice(owner);
        sub.pattern[..pattern.len()].copy_from_slice(pattern);
        sub
    }

    #[test]
    fn test_publish_wakes_matching_subscriber() {
        let store = wired_store();
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        subs[0] = Some(sub_for(b"vfs", b"clk"));
        subs[1] = Some(sub_for(b"pm", b"rst"));
        let engine = LiteralMatcher;
        let mut out = [Endpoint(0); NR_DS_SUBS];
        let slot = EntrySlot::from_index(3).unwrap();
        let stats = apply_update(&store, &mut subs, slot, true, &engine, &mut out);
        assert_eq!(stats.notified, 1);
        assert_eq!(out[0], Endpoint(9));
        assert!(subs[0].as_ref().unwrap().old_subs.get(3));
        assert!(!subs[1].as_ref().unwrap().old_subs.get(3));
    }

    #[test]
    fn test_delete_clears_bit_and_still_wakes() {
        // `set = false` clears the bit but still notifies (:217-222).
        let store = wired_store();
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        let mut sub = sub_for(b"vfs", b"clk");
        sub.old_subs.set(3, true);
        subs[0] = Some(sub);
        let engine = LiteralMatcher;
        let mut out = [Endpoint(0); NR_DS_SUBS];
        let slot = EntrySlot::from_index(3).unwrap();
        let stats = apply_update(&store, &mut subs, slot, false, &engine, &mut out);
        assert_eq!(stats.notified, 1);
        assert!(!subs[0].as_ref().unwrap().old_subs.get(3));
    }

    #[test]
    fn test_stale_owner_skips_without_panic() {
        // Subscriber "ghost" has no label seat: C would panic in
        // ds_getprocep; Rust skips and counts (documented deviation).
        let store = wired_store();
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        subs[0] = Some(sub_for(b"ghost", b"clk"));
        let engine = LiteralMatcher;
        let mut out = [Endpoint(0); NR_DS_SUBS];
        let slot = EntrySlot::from_index(3).unwrap();
        let stats = apply_update(&store, &mut subs, slot, true, &engine, &mut out);
        assert_eq!(stats.notified, 0);
        assert_eq!(stats.skipped_stale, 1);
    }

    #[test]
    fn test_initial_scan_sets_bits_and_wakes_source() {
        let store = wired_store();
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        subs[5] = Some(sub_for(b"pm", b"clk"));
        let engine = LiteralMatcher;
        let slot = crate::slots::SubSlot::from_index(5).unwrap();
        let wake = initial_scan(&store, &mut subs, slot, Endpoint(4), b"pm", &engine);
        assert_eq!(wake, Some(Endpoint(4)));
        assert!(subs[5].as_ref().unwrap().old_subs.get(3));
    }

    #[test]
    fn test_initial_scan_quiet_without_match() {
        let store = wired_store();
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        subs[5] = Some(sub_for(b"pm", b"zzz"));
        let engine = LiteralMatcher;
        let slot = crate::slots::SubSlot::from_index(5).unwrap();
        let wake = initial_scan(&store, &mut subs, slot, Endpoint(4), b"pm", &engine);
        assert_eq!(wake, None);
    }
}
