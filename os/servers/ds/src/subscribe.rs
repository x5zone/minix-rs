//! DS subscribe: register interest in future updates.
//!
//! Mirrors `do_subscribe` (`minix3/minix/servers/ds/store.c:456-534`).
//! 10-ds-subscribe-check.md (subscribe half).
//!
//! The module owns the verdict and the seat write, nothing else: owner
//! resolution is tendered, the overwrite rule, seat taking, pattern
//! storage, the type mask, and the optional immediate scan. Matching
//! itself is a trait (`PatternMatcher`): the default engine covers
//! literal patterns exactly (C anchors `^…$`, so literals are exact
//! matches); meta-characters await the full engine (A-2), which plugs
//! into the same trait without reshaping any table.
//!
//! Single-threaded event loop: verdicts are pure; the apply step takes
//! `&mut` tables from the caller, no shared state.

use minix_types::{DSF_MASK_TYPE, DS_MAX_KEYLEN, EAGAIN, EEXIST, EINVAL, ESRCH, DsFlags};

use crate::publish::check_key_len;
use crate::slots::{SubSlot, alloc_sub_slot, free_sub_slot, lookup_sub};
use crate::store::DsStore;
use crate::subscription::{DsSubs, Subscription};

/// Why a subscribe is refused (`do_subscribe` error paths).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscribeReject {
    /// Nameless source. C: `ESRCH` — store.c:466-468.
    ///
    /// Note the code: subscribe is the one path that answers ESRCH
    /// (not EPERM) for a nameless caller — a subscriber without a name
    /// cannot be told anything later, so there is nothing to register.
    UnknownSource,
    /// A subscription already exists and overwrite was not asked.
    /// C: `EEXIST` — store.c:473-474.
    Exists,
    /// No vacant subscription seat. C: `EAGAIN` — store.c:480-481.
    ///
    /// Not ENOMEM: the table is a rendezvous, not memory — "try again
    /// later" is the honest code, and C agrees.
    NoSpace,
    /// Bogus key length. C: forwarded from `get_key_name` — store.c:488.
    BadKey,
    /// The pattern does not compile. C: `EINVAL` — store.c:493-498.
    ///
    /// Raised by the engine for meta-character patterns the default
    /// matcher cannot decide. Literal patterns never refuse here.
    BadPattern,
}

impl SubscribeReject {
    /// The Minix3 errno each refusal carries (no invented codes).
    pub const fn errno(self) -> i32 {
        match self {
            Self::UnknownSource => ESRCH,
            Self::Exists => EEXIST,
            Self::NoSpace => EAGAIN,
            Self::BadKey | Self::BadPattern => EINVAL,
        }
    }
}

/// Decides whether a stored pattern matches an entry key.
///
/// C compiles `^pattern$` with `REG_EXTENDED` and runs `regexec`
/// (store.c:487-498, 190-193). The trait keeps that seam: engines are
/// interchangeable, tables never care which one judged.
///
/// Both sides are NUL-padded lanes; engines compare up to the first
/// NUL on each side (the `strcmp` stop rule, 04).
pub trait PatternMatcher {
    /// Judge `pattern` against `key`.
    fn matches(&self, pattern: &[u8; DS_MAX_KEYLEN], key: &[u8; DS_MAX_KEYLEN]) -> bool;

    /// Compile-check `pattern`: `Ok` means subscribable.
    ///
    /// The default accepts every lane (literals always compile in C
    /// too); engines with a smaller language refuse what they cannot
    /// decide, surfacing as `BadPattern`.
    fn check(&self, _pattern: &[u8; DS_MAX_KEYLEN]) -> Result<(), SubscribeReject> {
        Ok(())
    }
}

/// The literal engine: exact match, nothing more.
///
/// C anchors every pattern (`^…$`), so a pattern without
/// meta-characters matches exactly one key — byte for byte. This
/// engine decides exactly that case, with identical outcomes to C.
/// Patterns containing meta-characters are *not* decided here (see
/// [`DeferredEngine`]): guessing at regex semantics would invent
/// behavior C never stated.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiteralMatcher;

impl PatternMatcher for LiteralMatcher {
    fn matches(&self, pattern: &[u8; DS_MAX_KEYLEN], key: &[u8; DS_MAX_KEYLEN]) -> bool {
        crate::slots::key_eq(pattern, key)
    }
}

/// Marker for patterns the literal engine must not decide.
///
/// A pattern containing regex meta-characters (`. * [ ] ( ) | + ? ^ $
/// `\`) needs the full engine (A-2). This helper detects them so the
/// caller can refuse with `BadPattern` instead of mis-matching:
/// C would compile and match; we honestly cannot — yet.
pub fn needs_full_engine(pattern: &[u8; DS_MAX_KEYLEN]) -> bool {
    let len = pattern.iter().position(|&b| b == 0).unwrap_or(pattern.len());
    pattern[..len]
        .iter()
        .any(|&b| matches!(b, b'.' | b'*' | b'[' | b']' | b'(' | b')' | b'|' | b'+' | b'?' | b'^' | b'$' | b'\\'))
}

/// Where a subscribe lands (seat plus scan wish, store.c:470-511).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribePlan {
    /// The taken seat (old seat freed first when overwriting).
    pub slot: SubSlot,
    /// The type mask to store (`flags & MASK`, or full mask when the
    /// request names no type — store.c:501-503).
    pub type_mask: DsFlags,
    /// The caller asked for the immediate scan (`DSF_INITIAL`).
    pub initial_scan: bool,
}

/// Inputs to a subscribe verdict, bundled.
///
/// Eight lanes are one too many for a bare signature
/// (clippy::too_many_arguments) — and they travel together through
/// every caller, so they read better as one named bundle than as a
/// positional run.
pub struct SubscribeArgs<'a, M: PatternMatcher> {
    /// Already-resolved caller name (`None` = nameless, 05).
    pub owner: Option<&'a [u8]>,
    /// Ferried pattern text.
    pub key: &'a [u8],
    /// Granted key length (bounds-checked against 2..=80).
    pub key_len: usize,
    /// Request flags (type mask + ornaments).
    pub flags: DsFlags,
    /// `OVERWRITE` ornament: replace the existing seat.
    pub overwrite: bool,
    /// `INITIAL` ornament: scan now, notify on match.
    pub initial: bool,
    /// The match engine behind the verdict.
    pub engine: &'a M,
}

/// Decide a subscribe (`do_subscribe` verdict, store.c:466-508).
///
/// Six steps, in C order: nameless source refuses; an existing seat
/// needs overwrite (else `Exists`) and is freed first; a full house
/// refuses; the key bounds refuse; the engine compile-checks; the mask
/// and scan wish are recorded.
pub fn plan_subscribe<M: PatternMatcher>(
    subs: &DsSubs,
    args: SubscribeArgs<'_, M>,
) -> Result<(SubscribePlan, Option<SubSlot>), SubscribeReject> {
    let owner = args.owner.ok_or(SubscribeReject::UnknownSource)?;
    let freed = match lookup_sub(subs, owner) {
        Some(_) if !args.overwrite => return Err(SubscribeReject::Exists),
        Some(old) => Some(old),
        None => None,
    };
    // The seat probe runs before the key ferry in C (:480 vs :488):
    // a full house refuses even for a bogus key. Keep the order.
    let slot = alloc_sub_slot(subs).ok_or(SubscribeReject::NoSpace)?;
    if check_key_len(args.key_len).is_err() {
        return Err(SubscribeReject::BadKey);
    }
    let mut pattern = [0u8; DS_MAX_KEYLEN];
    let copy_len = args.key.len().min(DS_MAX_KEYLEN - 1);
    pattern[..copy_len].copy_from_slice(&args.key[..copy_len]);
    args.engine.check(&pattern)?;
    let mask_bits = args
        .flags
        .intersection(DsFlags::from_bits_truncate(DSF_MASK_TYPE));
    let type_mask = if mask_bits.is_empty() {
        DsFlags::from_bits_truncate(DSF_MASK_TYPE)
    } else {
        mask_bits
    };
    Ok((
        SubscribePlan {
            slot,
            type_mask,
            initial_scan: args.initial,
        },
        freed,
    ))
}

/// Carry out a planned subscribe (seat write, store.c:505-508).
///
/// Frees the overwritten seat first (when the plan carries one), then
/// writes flags, owner, pattern, and a cleared told-map. The immediate
/// scan (`INITIAL`) runs separately in `notify.rs`: it needs endpoint
/// resolution, which is transport, not furniture.
pub fn apply_subscribe(
    subs: &mut DsSubs,
    store_owner: &[u8],
    pattern: &[u8; DS_MAX_KEYLEN],
    plan: SubscribePlan,
    freed: Option<SubSlot>,
) {
    if let Some(old) = freed {
        free_sub_slot(subs, old);
    }
    let mut sub = Subscription::vacant();
    sub.flags = DsFlags::IN_USE | plan.type_mask;
    let copy_len = store_owner.len().min(DS_MAX_KEYLEN - 1);
    sub.owner[..copy_len].copy_from_slice(&store_owner[..copy_len]);
    sub.pattern = *pattern;
    subs[plan.slot.index()] = Some(sub);
    // The told-map starts clear (:507-508); matches arrive via the
    // sweep, never from the seat write. `vacant()` already clears,
    // but the loop below keeps the C order explicit.
    if let Some(seat) = subs[plan.slot.index()].as_mut() {
        seat.old_subs.clear();
    }
}

/// Match one entry against one subscriber (`check_sub_match`).
///
/// Both gates, in C order (store.c:190-193): the entry's subscribe
/// gate must admit the subscriber endpoint, and the pattern must
/// match the entry key. `subscriber` is the subscriber endpoint's
/// already-resolved name.
pub fn entry_matches<M: PatternMatcher>(
    entry: &crate::store::DataEntry,
    subscriber: Option<&[u8]>,
    engine: &M,
    sub: &Subscription,
    store: &DsStore,
    _entry_index: usize,
) -> bool {
    if !crate::auth::check_auth(entry, subscriber, DsFlags::PRIV_SUBSCRIBE) {
        return false;
    }
    // Type pre-gate, as the sweep does (:210): disjoint arms never meet.
    if !entry.flags.intersects(sub.flags) {
        return false;
    }
    let _ = store;
    engine.matches(&sub.pattern, &entry.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DataBody, DataEntry, NR_DS_KEYS};
    use crate::subscription::NR_DS_SUBS;

    fn test_subs() -> DsSubs {
        [None; NR_DS_SUBS]
    }

    fn args<'a, M: PatternMatcher>(
        owner: Option<&'a [u8]>,
        key: &'a [u8],
        key_len: usize,
        flags: DsFlags,
        overwrite: bool,
        initial: bool,
        engine: &'a M,
    ) -> SubscribeArgs<'a, M> {
        SubscribeArgs {
            owner,
            key,
            key_len,
            flags,
            overwrite,
            initial,
            engine,
        }
    }

    #[test]
    fn test_first_subscribe_takes_seat_zero() {
        let subs = test_subs();
        let engine = LiteralMatcher;
        let (plan, freed) = plan_subscribe(
            &subs,
            args(
                Some(b"vfs"),
                b"disk.*",
                7,
                DsFlags::TYPE_U32,
                false,
                false,
                &engine,
            ),
        )
        .expect("first subscribe must land");
        assert_eq!(plan.slot.index(), 0);
        assert_eq!(freed, None);
        assert!(!plan.initial_scan);
    }

    #[test]
    fn test_second_subscribe_without_overwrite_refuses() {
        let mut subs = test_subs();
        let engine = LiteralMatcher;
        let (plan, freed) = plan_subscribe(
            &subs,
            args(
                Some(b"vfs"),
                b"a",
                2,
                DsFlags::TYPE_U32,
                false,
                false,
                &engine,
            ),
        )
        .unwrap();
        let mut pattern = [0u8; DS_MAX_KEYLEN];
        pattern[0] = b'a';
        apply_subscribe(&mut subs, b"vfs", &pattern, plan, freed);
        assert_eq!(
            plan_subscribe(
                &subs,
                args(
                    Some(b"vfs"),
                    b"b",
                    2,
                    DsFlags::TYPE_U32,
                    false,
                    false,
                    &engine
                ),
            ),
            Err(SubscribeReject::Exists)
        );
    }

    #[test]
    fn test_overwrite_frees_old_seat_first() {
        let mut subs = test_subs();
        let engine = LiteralMatcher;
        let (plan, freed) = plan_subscribe(
            &subs,
            args(
                Some(b"vfs"),
                b"a",
                2,
                DsFlags::TYPE_U32,
                false,
                false,
                &engine,
            ),
        )
        .unwrap();
        let mut pattern = [0u8; DS_MAX_KEYLEN];
        pattern[0] = b'a';
        apply_subscribe(&mut subs, b"vfs", &pattern, plan, freed);
        let (plan2, freed2) = plan_subscribe(
            &subs,
            args(
                Some(b"vfs"),
                b"b",
                2,
                DsFlags::TYPE_U32,
                true,
                false,
                &engine,
            ),
        )
        .expect("overwrite must land");
        assert_eq!(freed2.map(|s| s.index()), Some(0));
        assert_eq!(plan2.slot.index(), 1);
    }

    #[test]
    fn test_empty_mask_means_all_types() {
        // No type arm named → full mask (store.c:501-503).
        let subs = test_subs();
        let engine = LiteralMatcher;
        let (plan, _) = plan_subscribe(
            &subs,
            args(
                Some(b"vfs"),
                b"ab",
                3,
                DsFlags::empty(),
                false,
                false,
                &engine,
            ),
        )
        .unwrap();
        assert_eq!(
            plan.type_mask,
            DsFlags::from_bits_truncate(DSF_MASK_TYPE)
        );
    }

    #[test]
    fn test_literal_engine_matches_exactly() {
        let engine = LiteralMatcher;
        let mut pattern = [0u8; DS_MAX_KEYLEN];
        pattern[..3].copy_from_slice(b"vfs");
        let mut key = [0u8; DS_MAX_KEYLEN];
        key[..3].copy_from_slice(b"vfs");
        assert!(engine.matches(&pattern, &key));
        key[..4].copy_from_slice(b"vfs0");
        assert!(!engine.matches(&pattern, &key));
    }

    #[test]
    fn test_meta_patterns_need_full_engine() {
        let mut pattern = [0u8; DS_MAX_KEYLEN];
        pattern[..6].copy_from_slice(b"disk.*");
        assert!(needs_full_engine(&pattern));
        pattern = [0u8; DS_MAX_KEYLEN];
        pattern[..4].copy_from_slice(b"disk");
        assert!(!needs_full_engine(&pattern));
    }

    #[test]
    fn test_nameless_source_is_esrch() {
        let subs = test_subs();
        let engine = LiteralMatcher;
        assert_eq!(
            plan_subscribe(
                &subs,
                args(None, b"ab", 3, DsFlags::TYPE_U32, false, false, &engine)
            )
            .map(|_| ()),
            Err(SubscribeReject::UnknownSource)
        );
    }

    #[test]
    fn test_entry_matches_gates() {
        // Entry/subscriber fixtures for the match verdict.
        let store: DsStore = [None; NR_DS_KEYS];
        let engine = LiteralMatcher;
        let mut entry = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_U32,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 1 },
        };
        entry.key[..3].copy_from_slice(b"clk");
        let mut sub = Subscription::vacant();
        sub.flags = DsFlags::IN_USE | DsFlags::TYPE_U32;
        sub.pattern[..3].copy_from_slice(b"clk");
        assert!(entry_matches(&entry, Some(b"vfs"), &engine, &sub, &store, 0));
        // Wrong pattern: no match.
        sub.pattern = [0u8; DS_MAX_KEYLEN];
        sub.pattern[..3].copy_from_slice(b"rst");
        assert!(!entry_matches(&entry, Some(b"vfs"), &engine, &sub, &store, 0));
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(SubscribeReject::UnknownSource.errno(), ESRCH);
        assert_eq!(SubscribeReject::Exists.errno(), EEXIST);
        assert_eq!(SubscribeReject::NoSpace.errno(), EAGAIN);
        assert_eq!(SubscribeReject::BadKey.errno(), EINVAL);
        assert_eq!(SubscribeReject::BadPattern.errno(), EINVAL);
    }
}
