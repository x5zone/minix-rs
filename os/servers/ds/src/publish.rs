//! DS publish: who may plant, where it lands, overwrite or refuse.
//!
//! Mirrors the decision kernel of `do_publish` (`minix3/minix/servers/ds/
//! store.c:287-378`) plus the pure half of `get_key_name`
//! (`store.c:158-181`). 07-ds-publish.md.
//!
//! The module owns the verdict and nothing else: six refusals, two ways
//! to land. The instruments come from neighbours (seats from 04, names
//! and gates from 05); the caller tenders the already-resolved source
//! name and RS-ness, so the verdict needs no table walk of its own and
//! no IPC — pure judgement, purely testable (D5).
//!
//! The commit half (heap buffers (A-3), grant transport (02/12),
//! attribute writes, the notify ring (10)) stands beyond three
//! frontiers: this kernel decides *whether and where*, the frontiers
//! perform *how* (D1).
//!
//! Single-threaded event loop: pure functions over caller-held tables, no
//! shared state.

use minix_types::{DSF_MASK_TYPE, DsFlags, EEXIST, EINVAL, ENOMEM, EPERM, DS_MAX_KEYLEN};

use crate::auth::check_auth;
use crate::slots::{EntrySlot, alloc_entry_slot, lookup_entry, lookup_label_entry};
use crate::store::DsStore;

/// Key length bounds (`get_key_name`, `store.c:163`).
///
/// A key shorter than 2 (bare NUL) or longer than the lane (80) is
/// bogus: the former names nothing, the latter fits no lane.
pub const MIN_KEY_LEN: usize = 2;

/// Judge a key length (`get_key_name` bound half, `store.c:163`).
///
/// The pure kernel of the key ferry: lengths outside `2..=80` read
/// `Err(EINVAL)`. Transport (safecopy) and the trailing clamp
/// (`store.c:170-178`) ride the 02/12 frontier — the bound is the
/// verdict's material, the ferry is the frontier's (D4). Shared
/// instrument: 08/09 reuse this gate rather than restating the bounds.
pub const fn check_key_len(len: usize) -> Result<usize, i32> {
    if len < MIN_KEY_LEN || len > DS_MAX_KEYLEN {
        return Err(EINVAL);
    }
    Ok(len)
}

/// Why a publish is refused (`do_publish` error paths, `store.c:297-326,366`).
///
/// Seven refusals, no eighth: the C walk is exhausted by source, label
/// gate, key, seat, overwrite gate, space, and type — the total rides
/// the type, so an unhandled refusal is inexpressible (D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishReject {
    /// Nameless source. C: `EPERM` — store.c:298-299.
    UnknownSource,
    /// Non-RS publisher for a label. C: `EPERM` — store.c:302-303.
    LabelNotRs,
    /// Bogus key length. C: `EINVAL` — store.c:306-307 (via `r`).
    BadKey,
    /// Entry exists, no overwrite asked. C: `EEXIST` — store.c:325.
    Exists,
    /// No vacant seat. C: `ENOMEM` — store.c:317-318.
    NoSpace,
    /// Overwrite asked but the gate closes. C: `EPERM` — store.c:321-322.
    Forbidden,
    /// Type arm outside the four. C: `EINVAL` — store.c:366.
    BadType,
}

impl PublishReject {
    /// The Minix3 errno each refusal carries (no invented codes, D2).
    pub const fn errno(self) -> i32 {
        match self {
            Self::UnknownSource | Self::LabelNotRs | Self::Forbidden => EPERM,
            Self::BadKey | Self::BadType => EINVAL,
            Self::Exists => EEXIST,
            Self::NoSpace => ENOMEM,
        }
    }
}

/// Where a publish lands (`dsp` with its history, `store.c:310-326`).
///
/// Two named ways: a fresh seat (attributes fully written by the commit
/// half) or a held seat (overwrite already authorized here, heap release
/// left to the commit half under A-3). The name keeps the commit half
/// from re-deciding (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishTarget {
    /// A vacant seat taken for the new entry.
    Create {
        /// The taken seat.
        slot: EntrySlot,
    },
    /// The held seat, overwrite authorized.
    Overwrite {
        /// The held seat.
        slot: EntrySlot,
    },
}

/// Decide a publish (`do_publish` verdict, `store.c:297-326,329,366`).
///
/// Six steps, in C order: nameless source refuses; non-RS label refuses;
/// key bounds refuse; name-and-type lookup finds the seat (labels also
/// try the numeric door, `store.c:312-313`); a missing seat is taken
/// (full house refuses); a held seat needs overwrite asked (else
/// `Exists`) and authorized (else `Forbidden`); the type arm is
/// validated before any heap moves (`BadType`).
///
/// Inputs are tendered resolved: `source` is 05's verdict on the
/// caller, `is_rs` the caller's RS-ness, `key` the ferried name,
/// `label_ep` the label endpoint for the numeric door (LABEL letters
/// only). Flags carry the type arm plus `OVERWRITE`.
pub fn plan_publish(
    store: &DsStore,
    source: Option<&[u8]>,
    is_rs: bool,
    key: &[u8],
    flags: DsFlags,
    label_ep: u32,
) -> Result<PublishTarget, PublishReject> {
    if source.is_none() {
        return Err(PublishReject::UnknownSource);
    }
    if flags.intersects(DsFlags::TYPE_LABEL) && !is_rs {
        return Err(PublishReject::LabelNotRs);
    }
    check_key_len(key.len()).map_err(|_| PublishReject::BadKey)?;

    let ty = flags.intersection(DsFlags::from_bits_truncate(DSF_MASK_TYPE));
    let mut seat = lookup_entry(store, key, ty);
    if flags.intersects(DsFlags::TYPE_LABEL) && seat.is_none() {
        seat = lookup_label_entry(store, label_ep);
    }

    let slot = match seat {
        None => alloc_entry_slot(store)
            .map(|slot| (slot, true))
            .ok_or(PublishReject::NoSpace)?,
        Some(slot) => (slot, false),
    };

    let target = match slot {
        (slot, true) => PublishTarget::Create { slot },
        (slot, false) => {
            if !flags.intersects(DsFlags::OVERWRITE) {
                return Err(PublishReject::Exists);
            }
            let entry = slot
                .get(store)
                .expect("lookup hit always seats a body");
            // The overwrite gate is judged here, through 05's instrument:
            // the commit half must not re-decide (D3/D5).
            if !check_auth(entry, source, DsFlags::PRIV_OVERWRITE) {
                return Err(PublishReject::Forbidden);
            }
            PublishTarget::Overwrite { slot }
        }
    };

    // Validate the type arm before any heap moves (D6): waste types fail
    // while the taken seat is still clean.
    match ty {
        t if t == DsFlags::TYPE_U32
            || t == DsFlags::TYPE_STR
            || t == DsFlags::TYPE_MEM
            || t == DsFlags::TYPE_LABEL =>
        {
            Ok(target)
        }
        _ => Err(PublishReject::BadType),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DataBody, DataEntry};

    fn stored(key: &[u8], ty: DsFlags, owner: &[u8], gate: DsFlags) -> DataEntry {
        let mut entry = DataEntry {
            flags: DsFlags::IN_USE | ty | gate,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 1 },
        };
        entry.key[..key.len()].copy_from_slice(key);
        entry.owner[..owner.len()].copy_from_slice(owner);
        entry
    }

    fn keyed_store() -> DsStore {
        let mut store: DsStore = [None; crate::store::NR_DS_KEYS];
        store[0] = Some(stored(b"cfg", DsFlags::TYPE_U32, b"rs", DsFlags::empty()));
        store[1] = Some(stored(
            b"secret",
            DsFlags::TYPE_STR,
            b"vfs",
            DsFlags::PRIV_OVERWRITE,
        ));
        store
    }

    #[test]
    fn test_reject_unknown_source() {
        // Nameless callers plant nothing (C: EPERM, `store.c:298-299`).
        let store = keyed_store();
        let r = plan_publish(&store, None, false, b"cfg", DsFlags::TYPE_U32, 0);
        assert_eq!(r.unwrap_err(), PublishReject::UnknownSource);
        assert_eq!(PublishReject::UnknownSource.errno(), EPERM);
    }

    #[test]
    fn test_reject_foreign_label() {
        // Only RS publishes labels (`store.c:302-303`).
        let store = keyed_store();
        let r = plan_publish(&store, Some(b"pm"), false, b"rs", DsFlags::TYPE_LABEL, 2);
        assert_eq!(r.unwrap_err(), PublishReject::LabelNotRs);
        let ok = plan_publish(&store, Some(b"rs"), true, b"rs", DsFlags::TYPE_LABEL, 2);
        assert!(ok.is_ok());
    }

    #[test]
    fn test_reject_bad_key() {
        // Keys outside 2..=80 are bogus (`store.c:163`).
        assert_eq!(check_key_len(0), Err(EINVAL));
        assert_eq!(check_key_len(1), Err(EINVAL));
        assert_eq!(check_key_len(2), Ok(2));
        assert_eq!(check_key_len(80), Ok(80));
        assert_eq!(check_key_len(81), Err(EINVAL));
        let store = keyed_store();
        let r = plan_publish(&store, Some(b"rs"), true, b"x", DsFlags::TYPE_U32, 0);
        assert_eq!(r.unwrap_err(), PublishReject::BadKey);
    }

    #[test]
    fn test_create_fresh_seat() {
        // A new name takes the lowest vacant seat (`store.c:315-318`).
        let store = keyed_store();
        let r = plan_publish(&store, Some(b"rs"), true, b"new", DsFlags::TYPE_U32, 0);
        assert_eq!(
            r.unwrap(),
            PublishTarget::Create {
                slot: EntrySlot::from_index(2).unwrap()
            }
        );
    }

    #[test]
    fn test_reject_exists_without_overwrite() {
        // A held seat without overwrite asked reads EEXIST (`store.c:325`).
        let store = keyed_store();
        let r = plan_publish(&store, Some(b"rs"), true, b"cfg", DsFlags::TYPE_U32, 0);
        assert_eq!(r.unwrap_err(), PublishReject::Exists);
        assert_eq!(PublishReject::Exists.errno(), EEXIST);
    }

    #[test]
    fn test_overwrite_gate() {
        // Overwrite with the owner's name lands; a stranger is refused
        // (`store.c:319-322` through 05's gate).
        let store = keyed_store();
        let flags = DsFlags::TYPE_STR | DsFlags::OVERWRITE;
        let ok = plan_publish(&store, Some(b"vfs"), false, b"secret", flags, 0);
        assert!(matches!(ok.unwrap(), PublishTarget::Overwrite { .. }));
        let denied = plan_publish(&store, Some(b"pm"), false, b"secret", flags, 0);
        assert_eq!(denied.unwrap_err(), PublishReject::Forbidden);
        // ...while an unguarded seat overwrites for anyone holding the name.
        let open = DsFlags::TYPE_U32 | DsFlags::OVERWRITE;
        let ok = plan_publish(&store, Some(b"pm"), false, b"cfg", open, 0);
        assert!(matches!(ok.unwrap(), PublishTarget::Overwrite { .. }));
    }

    #[test]
    fn test_reject_full_house() {
        // No vacant seat reads ENOMEM (`store.c:317-318`).
        let mut store: DsStore = [None; crate::store::NR_DS_KEYS];
        for seat in store.iter_mut() {
            *seat = Some(stored(b"k", DsFlags::TYPE_U32, b"rs", DsFlags::empty()));
        }
        let r = plan_publish(&store, Some(b"rs"), true, b"other", DsFlags::TYPE_U32, 0);
        assert_eq!(r.unwrap_err(), PublishReject::NoSpace);
        assert_eq!(PublishReject::NoSpace.errno(), ENOMEM);
    }

    #[test]
    fn test_reject_waste_type() {
        // Arms outside the four fail even with a clean seat waiting
        // (`store.c:366`).
        let store = keyed_store();
        let r = plan_publish(&store, Some(b"rs"), true, b"new", DsFlags::empty(), 0);
        assert_eq!(r.unwrap_err(), PublishReject::BadType);
        assert_eq!(PublishReject::BadType.errno(), EINVAL);
    }

    #[test]
    fn test_label_numeric_door() {
        // A label publish finds its seat by endpoint number even under a
        // different key name (`store.c:312-313`): the numeric door opens
        // what the name door misses.
        let mut store: DsStore = [None; crate::store::NR_DS_KEYS];
        let mut label = stored(b"rs", DsFlags::TYPE_LABEL, b"rs", DsFlags::empty());
        label.body = DataBody { u32: 7 };
        store[4] = Some(label);
        let flags = DsFlags::TYPE_LABEL | DsFlags::OVERWRITE;
        let r = plan_publish(&store, Some(b"rs"), true, b"other-name", flags, 7);
        assert_eq!(
            r.unwrap(),
            PublishTarget::Overwrite {
                slot: EntrySlot::from_index(4).unwrap()
            }
        );
    }
}
