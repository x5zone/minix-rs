//! DS subscriptions: who is told, about what, what was already told.
//!
//! Mirrors `struct subscription` + the subscription table
//! (`minix3/minix/servers/ds/store.h:13,31-36`). 03-ds-data-structures.md.
//!
//! The module owns the shape and nothing else: the subscriber record, the
//! table, and the vacancy rule. The match engine (regex storage and exec,
//! 10) and slot dynamics (alloc/lookup, 04) stay out.
//!
//! Single-threaded event loop: pure functions, no shared state.

use minix_types::{Bitmap, DS_MAX_KEYLEN, DsFlags};

use crate::store::NR_DS_KEYS;

/// Subscriptions in the table (`store.h:13`: `4 * NR_SYS_PROCS` of 64).
pub const NR_DS_SUBS: usize = 256;

/// One subscriber (`struct subscription`, `store.h:31-36`).
///
/// Four lanes: flags, master name (`owner`), the match formula (`regex`,
/// body deferred to the engine in 10 — A-2), and the already-told map
/// (`old_subs`). The formula's absence is honest, not an omission: no
/// engine exists yet, and a placeholder bound would invent a contract C
/// never stated (D7).
#[derive(Debug, Clone, Copy)]
pub struct Subscription {
    /// Occupancy. C: `int flags` — store.h:32.
    pub flags: DsFlags,
    /// Subscriber name. C: `char owner[80]` — store.h:33.
    pub owner: [u8; DS_MAX_KEYLEN],
    /// Source pattern text, NUL-padded lane (without the `^…$` anchors
    /// C adds at subscribe time, `store.c:487-493`).
    ///
    /// C stores the *compiled* formula (`regex_t`); Rust stores the
    /// source text and compiles per engine behind [`crate::subscribe`]'s
    /// `PatternMatcher` trait. A literal pattern behaves exactly like
    /// C's anchored match; meta-characters need a full engine (A-2),
    /// which reads this same lane — no shape change on arrival.
    pub pattern: [u8; DS_MAX_KEYLEN],
    /// Already-told map, one bit per entry. C: `old_subs` — store.h:35.
    pub old_subs: Bitmap,
}

impl Subscription {
    /// Vacancy rule (same shape as entries: `!(flags & DSF_IN_USE)`).
    pub const fn is_vacant(&self) -> bool {
        !self.flags.contains(DsFlags::IN_USE)
    }

    /// A fresh subscriber: unflagged, unnamed, unmatched, nothing told.
    pub const fn vacant() -> Self {
        Self {
            flags: DsFlags::empty(),
            owner: [0u8; DS_MAX_KEYLEN],
            pattern: [0u8; DS_MAX_KEYLEN],
            old_subs: Bitmap::new(NR_DS_KEYS),
        }
    }
}

/// The subscription table (`ds_subs[NR_DS_SUBS]`, A-4).
///
/// Fixed at 256, like the entry table: the bound rides the type.
/// `None` is vacancy — the table never holds a half-subscription.
pub type DsSubs = [Option<Subscription>; NR_DS_SUBS];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_table_capacity() {
        // 256 fixed seats (A-4); fresh seats are all vacant.
        let table: DsSubs = [None; NR_DS_SUBS];
        assert_eq!(table.len(), NR_DS_SUBS);
        assert!(table.iter().all(|slot| slot.is_none()));
        assert!(Subscription::vacant().is_vacant());
    }

    #[test]
    fn test_old_bitmap() {
        // 128 told-bits ride the shared bitmap (A-5): set, read, clear.
        let mut sub = Subscription::vacant();
        assert_eq!(sub.old_subs.size(), NR_DS_KEYS);
        assert!(!sub.old_subs.get(7));
        sub.old_subs.set(7, true);
        assert!(sub.old_subs.get(7));
        sub.old_subs.set(7, false);
        assert!(!sub.old_subs.get(7));
    }
}
