//! DS retrieve: read back what was published, by name or by endpoint.
//!
//! Mirrors `do_retrieve` / `do_retrieve_label`
//! (`minix3/minix/servers/ds/store.c:383-454`). 08-ds-retrieve.md.
//!
//! The module owns the verdict and nothing else: key bounds, lookup,
//! the retrieve gate, and — for byte ranges — how many bytes move.
//! Transport (`sys_safecopyto`, 02/12) and naming (05) stay out: the
//! caller tenders the ferried key and the already-resolved caller name,
//! so the verdict needs no IPC and no table walk of its own.
//!
//! Single-threaded event loop: pure functions over caller-held tables, no
//! shared state.

use minix_types::{DSF_MASK_TYPE, EINVAL, EPERM, ESRCH, DsFlags};

use crate::auth::check_auth;
use crate::publish::check_key_len;
use crate::slots::{EntrySlot, lookup_entry, lookup_label_entry};
use crate::store::DsStore;

/// Why a retrieve is refused (`do_retrieve` / `do_retrieve_label` errors).
///
/// The full C walk, exhausted: bad key, missing entry, closed gate,
/// unknown type arm. The total rides the type, so an unhandled refusal
/// is inexpressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrieveReject {
    /// Bogus key length. C: `EINVAL` via `get_key_name` — store.c:393-394.
    BadKey,
    /// No entry under this name and type. C: `ESRCH` — store.c:397-398.
    NotFound,
    /// The retrieve gate is set and the caller is not the owner.
    /// C: `EPERM` — store.c:399-400.
    Forbidden,
    /// Type arm outside the four known arms. C: `EINVAL` — store.c:423.
    BadType,
}

impl RetrieveReject {
    /// The Minix3 errno each refusal carries (no invented codes).
    pub const fn errno(self) -> i32 {
        match self {
            Self::BadKey | Self::BadType => EINVAL,
            Self::NotFound => ESRCH,
            Self::Forbidden => EPERM,
        }
    }
}

/// What a successful retrieve hands back (`do_retrieve` reply arms).
///
/// Numbers and endpoints travel inside the reply message; byte ranges
/// travel through a caller grant, so only their length is decided here
/// (transport copies, 02/12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrieveHit {
    /// Plain value. C: `m_ds_reply.val_out.u32` — store.c:405.
    Number(u32),
    /// Endpoint value. C: `m_ds_reply.val_out.ep` — store.c:408.
    Label(u32),
    /// Byte range: this many bytes move through the grant.
    /// C: `MIN(val_len, mem.length)` — store.c:412, reply `val_len` at :420.
    Bytes {
        /// Bytes to copy: `min(requested, stored)`.
        len: usize,
    },
}

/// How many bytes move for a string/memory read (`MIN`, store.c:412).
///
/// The caller offers room (`requested`), the entry holds truth
/// (`stored`); the smaller wins. Truncation is silent by contract —
/// the reply `val_len` tells the caller what actually moved (:420),
/// so a short read is distinguishable from a full one.
pub const fn truncated_len(requested: usize, stored: usize) -> usize {
    if requested < stored {
        requested
    } else {
        stored
    }
}

/// Decide a retrieve by name (`do_retrieve` verdict, store.c:393-427).
///
/// Five steps, in C order: key bounds refuse; name-and-type lookup
/// misses; the retrieve gate closes on strangers; the type arm picks
/// the reply shape (byte ranges resolve their length here, the copy
/// itself rides the grant frontier).
///
/// `caller` is the already-resolved caller name (`None` = an endpoint
/// with no published label, 05): a nameless caller passes only entries
/// whose retrieve gate is unset — the same selective-protection rule
/// as `check_auth`.
pub fn plan_retrieve(
    store: &DsStore,
    key: &[u8],
    key_len: usize,
    flags: DsFlags,
    caller: Option<&[u8]>,
    requested: usize,
) -> Result<(EntrySlot, RetrieveHit), RetrieveReject> {
    if check_key_len(key_len).is_err() {
        return Err(RetrieveReject::BadKey);
    }
    let ty = flags.intersection(DsFlags::from_bits_truncate(DSF_MASK_TYPE));
    let slot = lookup_entry(store, key, ty).ok_or(RetrieveReject::NotFound)?;
    let entry = slot
        .get(store)
        .expect("lookup hit always seats an entry");
    if !check_auth(entry, caller, DsFlags::PRIV_RETRIEVE) {
        return Err(RetrieveReject::Forbidden);
    }
    // The switch runs on the masked type, exactly like C's `switch(type)`
    // (:404): a zero or multi-bit arm falls through to BadType.
    let hit = if ty == DsFlags::TYPE_U32 {
        // SAFETY: U32 entries are written through the narrow arm only
        // (publish path, 07); reading it back is the arm's documented use.
        RetrieveHit::Number(unsafe { entry.body.u32 })
    } else if ty == DsFlags::TYPE_LABEL {
        // SAFETY: same narrow-arm argument as above (label endpoints
        // ride the number lane, 03).
        RetrieveHit::Label(unsafe { entry.body.u32 })
    } else if ty == DsFlags::TYPE_STR || ty == DsFlags::TYPE_MEM {
        // SAFETY: STR/MEM entries are written through the wide arm only;
        // only the length lane is read here, never the pointer.
        let stored = unsafe { entry.body.mem.length };
        RetrieveHit::Bytes {
            len: truncated_len(requested, stored),
        }
    } else {
        return Err(RetrieveReject::BadType);
    };
    Ok((slot, hit))
}

/// Decide a retrieve by endpoint (`do_retrieve_label`, store.c:438-450).
///
/// One lookup through the label lane (04): the endpoint value rides
/// the number lane, so this never touches names. No permission gate —
/// C checks none here either. The key bytes (with terminator,
/// `strlen + 1` at :442-444) move through the caller's key grant.
pub fn plan_retrieve_label(
    store: &DsStore,
    endpoint: u32,
) -> Result<EntrySlot, RetrieveReject> {
    lookup_label_entry(store, endpoint).ok_or(RetrieveReject::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DataBody, DataEntry, NR_DS_KEYS};
    use minix_types::DS_MAX_KEYLEN;

    fn test_store() -> DsStore {
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut plain = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_U32,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 7 },
        };
        plain.key[..3].copy_from_slice(b"res");
        plain.owner[..3].copy_from_slice(b"vfs");
        store[0] = Some(plain);
        let mut guarded = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_U32 | DsFlags::PRIV_RETRIEVE,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 9 },
        };
        guarded.key[..4].copy_from_slice(b"priv");
        guarded.owner[..3].copy_from_slice(b"vfs");
        store[1] = Some(guarded);
        store
    }

    #[test]
    fn test_open_entry_reads_back() {
        // Plain entry: any named caller reads the number (store.c:405).
        let store = test_store();
        let (_, hit) = plan_retrieve(
            &store,
            b"res",
            4,
            DsFlags::TYPE_U32,
            Some(b"pm"),
            0,
        )
        .expect("open entry must read");
        assert_eq!(hit, RetrieveHit::Number(7));
    }

    #[test]
    fn test_guarded_entry_refuses_stranger() {
        // PRIV_RETRIEVE set: strangers get EPERM (store.c:399-400),
        // the owner passes.
        let store = test_store();
        assert_eq!(
            plan_retrieve(&store, b"priv", 5, DsFlags::TYPE_U32, Some(b"pm"), 0),
            Err(RetrieveReject::Forbidden)
        );
        assert!(plan_retrieve(
            &store,
            b"priv",
            5,
            DsFlags::TYPE_U32,
            Some(b"vfs"),
            0
        )
        .is_ok());
    }

    #[test]
    fn test_missing_entry_is_esrch() {
        let store = test_store();
        assert_eq!(
            plan_retrieve(&store, b"nope", 5, DsFlags::TYPE_U32, Some(b"pm"), 0),
            Err(RetrieveReject::NotFound)
        );
    }

    #[test]
    fn test_bad_key_is_einval() {
        let store = test_store();
        assert_eq!(
            plan_retrieve(&store, b"x", 1, DsFlags::TYPE_U32, Some(b"pm"), 0),
            Err(RetrieveReject::BadKey)
        );
    }

    #[test]
    fn test_unknown_type_arm_is_einval() {
        // Zero arm: lookup misses on empty intersection first; a bogus
        // multi-bit arm that still matches falls to BadType (:423).
        let store = test_store();
        let res = plan_retrieve(&store, b"res", 4, DsFlags::empty(), Some(b"pm"), 0);
        assert!(res.is_err());
    }

    #[test]
    fn test_truncation_is_min() {
        // MIN(requested, stored): short room truncates, ample room reads
        // all (store.c:412).
        assert_eq!(truncated_len(8, 100), 8);
        assert_eq!(truncated_len(100, 8), 8);
        assert_eq!(truncated_len(8, 8), 8);
    }

    #[test]
    fn test_label_lookup_by_endpoint() {
        // Endpoint-valued lookup misses here (no labels planted) → ESRCH.
        let store = test_store();
        assert_eq!(
            plan_retrieve_label(&store, 99),
            Err(RetrieveReject::NotFound)
        );
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(RetrieveReject::BadKey.errno(), EINVAL);
        assert_eq!(RetrieveReject::BadType.errno(), EINVAL);
        assert_eq!(RetrieveReject::NotFound.errno(), ESRCH);
        assert_eq!(RetrieveReject::Forbidden.errno(), EPERM);
    }
}
