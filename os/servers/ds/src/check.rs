//! DS check: fetch one pending update for a subscriber.
//!
//! Mirrors `do_check` (`minix3/minix/servers/ds/store.c:536-578`).
//! 10-ds-subscribe-check.md (check half).
//!
//! The module owns the verdict and the bit clear, nothing else: owner
//! resolution is tendered, the subscription lookup, the first-set-bit
//! scan, and the mark-consumed step. Key transport (`sys_safecopyto`,
//! 02/12) stays out, as does the reply write-back — the caller moves
//! the bytes and fills `flags`/`owner` from [`CheckHit`].
//!
//! Single-threaded event loop: verdicts are pure; the apply step takes
//! `&mut` tables from the caller, no shared state.

use minix_types::{ENOENT, ESRCH, DsFlags};

use crate::identity::resolve_endpoint;
use crate::slots::{EntrySlot, SubSlot, lookup_sub};
use crate::store::{DsStore, NR_DS_KEYS};
use crate::subscription::DsSubs;
use minix_types::Endpoint;

/// Why a check is refused (`do_check` error paths, store.c:544-558).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckReject {
    /// Nameless source. C: `ESRCH` — store.c:544-546.
    UnknownSource,
    /// The caller holds no subscription. C: `ESRCH` — store.c:549-550.
    ///
    /// Same code as `UnknownSource`, different reason: the first says
    /// "we don't know who you are", this one "we know you, but you
    /// never subscribed". One code, two names — diagnosable, honest.
    NoSubscription,
    /// Subscribed, but no update is pending. C: `ENOENT` — store.c:557.
    NoUpdate,
}

impl CheckReject {
    /// The Minix3 errno each refusal carries (no invented codes).
    pub const fn errno(self) -> i32 {
        match self {
            Self::UnknownSource | Self::NoSubscription => ESRCH,
            Self::NoUpdate => ENOENT,
        }
    }
}

/// One pending update: the subscriber's seat and the entry it points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckHit {
    /// The subscriber seat (for the mark-consumed step).
    pub sub: SubSlot,
    /// The updated entry (first set bit, lowest index first).
    pub entry: EntrySlot,
}

impl CheckHit {
    /// The reply type mask (`m_ds_req.flags = entry.flags & MASK`,
    /// store.c:571): which arm the subscriber should read back.
    pub fn reply_type(&self, store: &DsStore) -> DsFlags {
        self.entry
            .get(store)
            .map(|e| {
                e.flags
                    .intersection(DsFlags::from_bits_truncate(minix_types::DSF_MASK_TYPE))
            })
            .unwrap_or_else(DsFlags::empty)
    }

    /// The reply owner (`m_ds_req.owner = ds_getprocep(entry.owner)`,
    /// store.c:570): who published the update.
    ///
    /// C panics when the publisher's name has no label seat (:570 via
    /// :137). Rust returns `None` and lets the caller decide — the
    /// same deferred-verdict rule as `resolve_endpoint` (05 D2).
    pub fn reply_owner(&self, store: &DsStore) -> Option<Endpoint> {
        self.entry
            .get(store)
            .and_then(|e| resolve_endpoint(store, &e.owner))
    }
}

/// Decide a check (`do_check` verdict, store.c:544-558).
///
/// Three steps, in C order: nameless source refuses; missing
/// subscription refuses; no set bit refuses. The scan takes the
/// *lowest* set bit — updates are consumed oldest-first, and two
/// subscribers never share a map, so one consumer's read never
/// starves another's.
pub fn plan_check(
    subs: &DsSubs,
    owner: Option<&[u8]>,
) -> Result<CheckHit, CheckReject> {
    let owner = owner.ok_or(CheckReject::UnknownSource)?;
    let sub = lookup_sub(subs, owner).ok_or(CheckReject::NoSubscription)?;
    let seat = sub.get(subs).expect("lookup hit always seats a sub");
    let index = (0..NR_DS_KEYS).find(|&i| seat.old_subs.get(i));
    match index {
        Some(i) => Ok(CheckHit {
            sub,
            entry: EntrySlot::from_index(i).expect("told-map is entry-sized"),
        }),
        None => Err(CheckReject::NoUpdate),
    }
}

/// Mark the update consumed (`UNSET_BIT`, store.c:575).
///
/// Runs after the caller moved the key bytes: a failed copy must not
/// consume the update (C clears only on the success path too).
pub fn apply_check(subs: &mut DsSubs, hit: CheckHit) {
    if let Some(sub) = subs[hit.sub.index()].as_mut()
        && !sub.is_vacant()
    {
        sub.old_subs.set(hit.entry.index(), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::{NR_DS_SUBS, Subscription};

    fn wired_subs() -> DsSubs {
        let mut subs: DsSubs = [None; NR_DS_SUBS];
        let mut sub = Subscription::vacant();
        sub.flags = DsFlags::IN_USE | DsFlags::TYPE_U32;
        sub.owner[..3].copy_from_slice(b"vfs");
        sub.old_subs.set(3, true);
        sub.old_subs.set(9, true);
        subs[0] = Some(sub);
        subs
    }

    #[test]
    fn test_takes_lowest_set_bit_first() {
        // Two pending updates: the lowest index comes first (:553-558).
        let subs = wired_subs();
        let hit = plan_check(&subs, Some(b"vfs")).expect("update pending");
        assert_eq!(hit.entry.index(), 3);
    }

    #[test]
    fn test_consume_advances_to_next() {
        let mut subs = wired_subs();
        let hit = plan_check(&subs, Some(b"vfs")).unwrap();
        apply_check(&mut subs, hit);
        let hit = plan_check(&subs, Some(b"vfs")).expect("second update pending");
        assert_eq!(hit.entry.index(), 9);
        apply_check(&mut subs, hit);
        assert_eq!(
            plan_check(&subs, Some(b"vfs")),
            Err(CheckReject::NoUpdate)
        );
    }

    #[test]
    fn test_stranger_without_subscription_is_esrch() {
        let subs = wired_subs();
        assert_eq!(
            plan_check(&subs, Some(b"pm")),
            Err(CheckReject::NoSubscription)
        );
        assert_eq!(plan_check(&subs, None), Err(CheckReject::UnknownSource));
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(CheckReject::UnknownSource.errno(), ESRCH);
        assert_eq!(CheckReject::NoSubscription.errno(), ESRCH);
        assert_eq!(CheckReject::NoUpdate.errno(), ENOENT);
    }
}
