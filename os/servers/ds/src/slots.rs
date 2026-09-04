//! DS slot dynamics: who takes a seat, who leaves, who is found.
//!
//! Mirrors the six slot primitives (`minix3/minix/servers/ds/store.c:11-107`).
//! 04-ds-slot-management.md.
//!
//! The module owns the motion and nothing else: first-fit taking, full
//! release, and triple lookup. The shapes moved on (entries in `store.rs`,
//! subscribers in `subscription.rs`, 03), naming lives ahead (identity in
//! 05), and the match engine lives ahead (regex in 10, A-2).
//!
//! Single-threaded event loop: pure functions over caller-held tables, no
//! shared state. In and out ride typed indices ([`EntrySlot`]/[`SubSlot`]),
//! never pointers: an index can be re-read, a borrow cannot cross an IPC
//! point (D1).
//!
//! [ARCH A-4]: the fixed tables stay fixed; the bound rides the type, and
//! the scan order is the image order (`do_getsysinfo` copies the table
//! as-is, 11) — so first-fit ascending is load-bearing, not incidental.

use minix_types::{DS_MAX_KEYLEN, DsFlags};

use crate::store::{DsStore, NR_DS_KEYS};
use crate::subscription::{DsSubs, NR_DS_SUBS};

/// A seat in the entry table (`ds_store[i]`, `store.c:5`).
///
/// The index is the seat: construction is gated by [`EntrySlot::from_index`],
/// so a slot always names a real seat — a seat in the wrong table is
/// inexpressible (D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntrySlot {
    index: usize,
}

impl EntrySlot {
    /// Name a seat; out-of-range indices refuse (`None`).
    pub const fn from_index(index: usize) -> Option<Self> {
        if index < NR_DS_KEYS {
            Some(Self { index })
        } else {
            None
        }
    }

    /// The seat number (table order; doubles as image order, 11).
    pub const fn index(self) -> usize {
        self.index
    }

    /// Read the seated entry; a vacant seat reads `None`.
    pub fn get<'a>(self, store: &'a DsStore) -> Option<&'a crate::store::DataEntry> {
        store.get(self.index)?.as_ref()
    }
}

/// A seat in the subscription table (`ds_subs[i]`, `store.c:6`).
///
/// Same gate as [`EntrySlot`]: a subscription slot never names an entry
/// seat, and the reverse — the two tables keep separate currencies (D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubSlot {
    index: usize,
}

impl SubSlot {
    /// Name a seat; out-of-range indices refuse (`None`).
    pub const fn from_index(index: usize) -> Option<Self> {
        if index < NR_DS_SUBS {
            Some(Self { index })
        } else {
            None
        }
    }

    /// The seat number (table order).
    pub const fn index(self) -> usize {
        self.index
    }

    /// Read the seated subscriber; a vacant seat reads `None`.
    pub fn get<'a>(self, subs: &'a DsSubs) -> Option<&'a crate::subscription::Subscription> {
        subs.get(self.index)?.as_ref()
    }
}

/// Take the first vacant entry seat (`alloc_data_slot`, `store.c:11-22`).
///
/// Ascending first-fit: the lowest vacant seat wins. A full house reads
/// `None` (C: `NULL`, `store.c:21`) — fullness is a state (later freeing
/// re-opens seats), not an error, so `Option`, not `Result` (D2/D3).
pub fn alloc_entry_slot(store: &DsStore) -> Option<EntrySlot> {
    // Position scans from seat 0 upward, mirroring `for (i = 0; ...)` in C:
    // the scan order IS the image order (11), so it must not change (D3).
    store
        .iter()
        .position(|seat| seat.is_none())
        .map(|index| EntrySlot { index })
}

/// Take the first vacant subscription seat (`alloc_sub_slot`, `store.c:27-38`).
///
/// Same motion as [`alloc_entry_slot`], over the wider table (256 seats).
/// A full house reads `None` (C: `NULL`, `store.c:37`).
pub fn alloc_sub_slot(subs: &DsSubs) -> Option<SubSlot> {
    subs
        .iter()
        .position(|seat| seat.is_none())
        .map(|index| SubSlot { index })
}

/// Release a subscription seat (`free_sub_slot`, `store.c:43-52`).
///
/// The seat must be taken: freeing a vacant seat is a caller ordering bug,
/// not a state to tolerate, so it panics — mirroring C's `assert`
/// (`store.c:46`) in the same position (D5).
///
/// Release clears the whole seat (`None`): a superset of C's `flags = 0`
/// (`store.c:51`), with no outward difference — the subscription table never
/// enters the published image (only the entry table is copied out, 11), and
/// lookups gate on occupancy first, so residue is unreadable either way.
///
/// Engine hook (A-2): when the match engine (10) lands its stored formula
/// here, drop it at this point — the same seat where C calls `regfree`
/// (`store.c:48`).
pub fn free_sub_slot(subs: &mut DsSubs, slot: SubSlot) {
    let seat = &mut subs[slot.index];
    assert!(seat.is_some(), "free_sub_slot: seat already vacant");
    *seat = None;
}

/// Compare a fixed lane against a query name (`strcmp`, `store.c:65,100`).
///
/// Both sides end at the first NUL: lane residue past the terminator (a
/// previous longer name's tail) never joins the verdict, and a query's tail
/// past its own NUL neither — exactly `strcmp`'s stop rule (D4). A side with
/// no NUL compares whole.
pub fn key_eq(lane: &[u8; DS_MAX_KEYLEN], name: &[u8]) -> bool {
    cstr_len(lane) == cstr_len(name) && lane[..cstr_len(lane)] == name[..cstr_len(name)]
}

/// Length up to (excluding) the first NUL, or the full length if none.
fn cstr_len(bytes: &[u8]) -> usize {
    bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())
}

/// Find an entry by name and type (`lookup_entry`, `store.c:57-70`).
///
/// Three gates, in C order: occupied, then type membership
/// (`flags & type`, `store.c:64` — intersection, not equality), then name
/// (`key_eq`, D4). A vacant seat never matches even if its bytes would —
/// the gate runs first (D5/D6).
///
/// Callers pass a single type bit (`DSF_TYPE_*`); membership and containment
/// agree there, while membership also stays honest for wider queries (D6).
pub fn lookup_entry(store: &DsStore, key: &[u8], ty: DsFlags) -> Option<EntrySlot> {
    store
        .iter()
        .position(|seat| match seat {
            Some(entry) => {
                !entry.is_vacant() && entry.flags.intersects(ty) && key_eq(&entry.key, key)
            }
            None => false,
        })
        .map(|index| EntrySlot { index })
}

/// Find a label entry by endpoint (`lookup_label_entry`, `store.c:75-88`).
///
/// Labels ride the number lane (03 D2): the search is by value (`u.u32 == num`,
/// `store.c:83`), gated on occupancy and the label arm (`DSF_TYPE_LABEL`,
/// `store.c:82`). A same-valued number of another type does not match.
pub fn lookup_label_entry(store: &DsStore, num: u32) -> Option<EntrySlot> {
    store
        .iter()
        .position(|seat| match seat {
            // SAFETY: `DataBody` is `#[repr(C)]` with `u32` as its narrow arm
            // (store.rs); reading the narrow arm is the union's documented use
            // (labels are written through it, `store.c:241`). No other arm is
            // read while a narrow value is live.
            Some(entry) => {
                !entry.is_vacant()
                    && entry.flags.intersects(DsFlags::TYPE_LABEL)
                    && unsafe { entry.body.u32 } == num
            }
            None => false,
        })
        .map(|index| EntrySlot { index })
}

/// Find a subscription by subscriber name (`lookup_sub`, `store.c:93-105`).
///
/// Two gates, in C order: occupied, then owner equality (`store.c:99-100`).
/// One subscriber holds at most one seat: a second subscribe with overwrite
/// intent frees this seat first (10).
pub fn lookup_sub(subs: &DsSubs, owner: &[u8]) -> Option<SubSlot> {
    subs.iter()
        .position(|seat| match seat {
            Some(sub) => !sub.is_vacant() && key_eq(&sub.owner, owner),
            None => false,
        })
        .map(|index| SubSlot { index })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DataBody;
    use crate::subscription::Subscription;
    use minix_types::DsFlags;

    /// A table with `taken` lowest seats occupied (plain U32 entries).
    fn filled_store(taken: usize) -> DsStore {
        let mut store: DsStore = [None; NR_DS_KEYS];
        for (i, seat) in store.iter_mut().enumerate().take(taken) {
            *seat = Some(crate::store::DataEntry {
                flags: DsFlags::IN_USE | DsFlags::TYPE_U32,
                key: [0u8; DS_MAX_KEYLEN],
                owner: [0u8; DS_MAX_KEYLEN],
                body: DataBody { u32: i as u32 },
            });
        }
        store
    }

    fn named_entry(key: &[u8], ty: DsFlags) -> crate::store::DataEntry {
        let mut entry = crate::store::DataEntry {
            flags: DsFlags::IN_USE | ty,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        };
        entry.key[..key.len()].copy_from_slice(key);
        entry
    }

    fn named_sub(owner: &[u8]) -> Subscription {
        let mut sub = Subscription::vacant();
        sub.flags = DsFlags::IN_USE;
        sub.owner[..owner.len()].copy_from_slice(owner);
        sub
    }

    #[test]
    fn test_alloc_first_fit() {
        // Seats fill upward: the lowest vacant seat always wins
        // (`store.c:16-19`).
        let mut store: DsStore = [None; NR_DS_KEYS];
        assert_eq!(alloc_entry_slot(&store).unwrap().index(), 0);
        store[0] = Some(named_entry(b"a", DsFlags::TYPE_U32));
        assert_eq!(alloc_entry_slot(&store).unwrap().index(), 1);
        store[1] = Some(named_entry(b"b", DsFlags::TYPE_U32));
        assert_eq!(alloc_entry_slot(&store).unwrap().index(), 2);
        // Same motion, wider table (`store.c:32-35`).
        let subs: DsSubs = [None; NR_DS_SUBS];
        assert_eq!(alloc_sub_slot(&subs).unwrap().index(), 0);
    }

    #[test]
    fn test_alloc_full_reads_none() {
        // A full house reads None (C: NULL, `store.c:21,37`) — fullness is
        // a state, not an error (D2).
        let full = filled_store(NR_DS_KEYS);
        assert_eq!(alloc_entry_slot(&full), None);
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        for seat in subs.iter_mut() {
            *seat = Some(named_sub(b"x"));
        }
        assert_eq!(alloc_sub_slot(&subs), None);
    }

    #[test]
    fn test_free_reopens_lowest() {
        // Release re-opens the seat; the next take reuses the lowest
        // (`store.c:43-52` + first-fit).
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        subs[0] = Some(named_sub(b"a"));
        subs[1] = Some(named_sub(b"b"));
        free_sub_slot(&mut subs, SubSlot::from_index(0).unwrap());
        assert!(subs[0].is_none());
        // The freed name is unfindable (vacancy gates lookup).
        assert_eq!(lookup_sub(&subs, b"a"), None);
        assert_eq!(lookup_sub(&subs, b"b").unwrap().index(), 1);
        // Next take reuses seat 0.
        assert_eq!(alloc_sub_slot(&subs).unwrap().index(), 0);
    }

    #[test]
    #[should_panic(expected = "already vacant")]
    fn test_free_vacant_panics() {
        // Freeing a vacant seat is a caller ordering bug: panic, mirroring
        // C's assert (`store.c:46`).
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        free_sub_slot(&mut subs, SubSlot::from_index(3).unwrap());
    }

    #[test]
    fn test_lookup_entry_double_gate() {
        // Name AND type must both hit (`store.c:63-65`).
        let mut store: DsStore = [None; NR_DS_KEYS];
        store[5] = Some(named_entry(b"cfg", DsFlags::TYPE_U32));
        assert_eq!(
            lookup_entry(&store, b"cfg", DsFlags::TYPE_U32)
                .unwrap()
                .index(),
            5
        );
        // Wrong name misses.
        assert_eq!(lookup_entry(&store, b"cfh", DsFlags::TYPE_U32), None);
        assert_eq!(lookup_entry(&store, b"cfgx", DsFlags::TYPE_U32), None);
        // Wrong type misses (U32 seat is not a STR seat).
        assert_eq!(lookup_entry(&store, b"cfg", DsFlags::TYPE_STR), None);
        assert_eq!(lookup_entry(&store, b"cfg", DsFlags::TYPE_LABEL), None);
    }

    #[test]
    fn test_lookup_skips_vacant_bytes() {
        // Bytes alone never match: the occupancy gate runs first. A vacant
        // seat (`None`) with would-be matching bytes is invisible.
        let store: DsStore = [None; NR_DS_KEYS];
        assert_eq!(lookup_entry(&store, b"", DsFlags::TYPE_U32), None);
        let subs: DsSubs = [None; NR_DS_SUBS];
        assert_eq!(lookup_sub(&subs, b""), None);
    }

    #[test]
    fn test_lookup_label_by_number() {
        // Labels are found by endpoint value through the number lane
        // (`store.c:80-84`); a same-valued non-label does not match.
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut label = named_entry(b"rs", DsFlags::TYPE_LABEL);
        label.body = DataBody { u32: 7 };
        store[2] = Some(label);
        let mut num = named_entry(b"n7", DsFlags::TYPE_U32);
        num.body = DataBody { u32: 7 };
        store[4] = Some(num);
        assert_eq!(lookup_label_entry(&store, 7).unwrap().index(), 2);
        assert_eq!(lookup_label_entry(&store, 8), None);
    }

    #[test]
    fn test_lookup_sub_by_owner() {
        // One seat per subscriber name (`store.c:98-101`).
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        subs[9] = Some(named_sub(b"vfs"));
        assert_eq!(lookup_sub(&subs, b"vfs").unwrap().index(), 9);
        assert_eq!(lookup_sub(&subs, b"pm"), None);
        assert_eq!(lookup_sub(&subs, b"vf"), None);
    }

    #[test]
    fn test_key_eq_nul_stop() {
        // Lane residue past NUL never joins the verdict (strcmp stop rule,
        // `store.c:65`): a recycled lane holding "cfg\\0x" still equals "cfg".
        let mut lane = [0u8; DS_MAX_KEYLEN];
        lane[..5].copy_from_slice(b"cfg\0x");
        assert!(key_eq(&lane, b"cfg"));
        assert!(key_eq(&lane, b"cfg\0ignored"));
        assert!(!key_eq(&lane, b"cfgx"));
        assert!(!key_eq(&lane, b"cf"));
        assert!(!key_eq(&lane, b""));
    }

    #[test]
    fn test_slot_currencies_separate() {
        // Entry seats and subscription seats are separate currencies:
        // out-of-range construction refuses, and cross-table use is
        // inexpressible (D1).
        assert!(EntrySlot::from_index(NR_DS_KEYS).is_none());
        assert!(SubSlot::from_index(NR_DS_SUBS).is_none());
        assert_eq!(EntrySlot::from_index(0).unwrap().index(), 0);
        assert_eq!(SubSlot::from_index(255).unwrap().index(), 255);
        // Reading through a slot reaches the seated body.
        let store = filled_store(1);
        // A zeroed lane IS the empty C string: `strcmp("", "") == 0`, so the
        // occupied zero-keyed seat matches — C-faithful (`store.c:65`).
        let slot = lookup_entry(&store, b"", DsFlags::TYPE_U32);
        assert_eq!(slot.unwrap().index(), 0);
        let mut named = filled_store(0);
        named[0] = Some(named_entry(b"k", DsFlags::TYPE_U32));
        let found = lookup_entry(&named, b"k", DsFlags::TYPE_U32).unwrap();
        assert_eq!(unsafe { found.get(&named).unwrap().body.u32 }, 0);
    }
}
