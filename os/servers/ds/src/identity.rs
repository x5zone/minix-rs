//! DS identity: which name an endpoint has, which endpoint a name has.
//!
//! Mirrors `ds_getprocname`/`ds_getprocep` (`minix3/minix/servers/ds/
//! store.c:110-138`). 05-ds-identity-auth.md.
//!
//! The module owns translation and nothing else: endpoint-to-name and
//! name-to-endpoint over the label lane. Comparison lives next door
//! (`auth.rs`, D3): translating and judging stay separate, so judging
//! needs no table. Grant transport (02/12) and the boot owner (06)
//! stay out.
//!
//! Single-threaded event loop: pure functions over caller-held tables, no
//! shared state.

use minix_types::{DS_MAX_KEYLEN, DsFlags, Endpoint};

use crate::slots::lookup_entry;
use crate::slots::lookup_label_entry;
use crate::store::DsStore;

/// The store's own name (`store.c:115`: `first_proc_name = "ds"`).
///
/// The self lane is answered before any table lookup: the store's own
/// endpoint never rides the label lane (nothing ever publishes it), so
/// asking the table would always miss (D1).
pub const DS_SELF_NAME: &[u8; DS_MAX_KEYLEN] = &{
    let mut lane = [0u8; DS_MAX_KEYLEN];
    lane[0] = b'd';
    lane[1] = b's';
    lane
};

/// Name the caller (`ds_getprocname`, `store.c:110-125`).
///
/// Three comings, in C order: the store itself answers its own lane
/// (`store.c:118-119`); any other endpoint is read back through its
/// published label (`store.c:121-122`, 04's numeric lookup); an
/// unpublished endpoint has no name (`store.c:124`, C: `NULL` → `None`).
///
/// The name is lent, not copied: the lane lives in the table seat, and
/// the only readers (attribute writes in 07/09, comparisons in `auth.rs`)
/// consume it through NUL-aware instruments (`key_eq`, strcpy-equivalent
/// copies) — a copy per translation would price every handler call (D1).
pub fn resolve_name(store: &DsStore, ep: Endpoint) -> Option<&[u8; DS_MAX_KEYLEN]> {
    if ep == Endpoint::DS {
        return Some(DS_SELF_NAME);
    }
    // C passes `endpoint_t` (int) into an `unsigned` parameter: negative
    // endpoints wrap to huge values and miss, same as the `as u32` wrap
    // below — the conversion mirrors C, it does not extend it.
    lookup_label_entry(store, ep.0 as u32).and_then(|slot| slot.get(store).map(|e| &e.key))
}

/// Name the endpoint (`ds_getprocep`, `store.c:130-138`).
///
/// The forward read through the label lane (`store.c:135-136`). A name
/// with no label seat reads `None`.
///
/// C panics here (`store.c:137`); Rust surfaces the absence instead. The
/// verdict (skip, refuse, or abort) belongs to the calling chain — the
/// subscriber sweep in 10 walks owner names that must exist, while future
/// callers may tolerate strangers — so this pure translator does not
/// presume it (D2: deferred verdict, owned by 10).
pub fn resolve_endpoint(store: &DsStore, name: &[u8]) -> Option<Endpoint> {
    lookup_entry(store, name, DsFlags::TYPE_LABEL).map(|slot| {
        let entry = slot.get(store).expect("lookup hit always seats a body");
        // SAFETY: labels are written through the narrow arm only
        // (`store.c:241`, map path in 06); reading it back is the arm's
        // documented use (03 D2). `as i32` mirrors C's unsigned-to-int
        // return conversion bit-for-bit.
        Endpoint(unsafe { entry.body.u32 } as i32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DataBody;
    use crate::store::DataEntry;

    fn label_seat(key: &[u8], ep: u32) -> DataEntry {
        let mut entry = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_LABEL,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: ep },
        };
        entry.key[..key.len()].copy_from_slice(key);
        entry
    }

    fn labelled_store() -> DsStore {
        let mut store: DsStore = [None; crate::store::NR_DS_KEYS];
        store[0] = Some(label_seat(b"rs", 2));
        store[1] = Some(label_seat(b"vfs", 9));
        store
    }

    #[test]
    fn test_self_name_first() {
        // The store names itself without consulting the table
        // (`store.c:118-119`).
        let store: DsStore = [None; crate::store::NR_DS_KEYS];
        assert_eq!(resolve_name(&store, Endpoint::DS), Some(DS_SELF_NAME));
    }

    #[test]
    fn test_name_through_label() {
        // Other endpoints read back through their published label
        // (`store.c:121-122`).
        let store = labelled_store();
        assert_eq!(&resolve_name(&store, Endpoint(9)).unwrap()[..3], b"vfs");
    }

    #[test]
    fn test_unknown_endpoint_nameless() {
        // Unpublished endpoints have no name (C: NULL, `store.c:124`).
        let store = labelled_store();
        assert_eq!(resolve_name(&store, Endpoint(42)), None);
    }

    #[test]
    fn test_endpoint_roundtrip() {
        // Names read forward to their endpoints (`store.c:135-136`).
        let store = labelled_store();
        assert_eq!(resolve_endpoint(&store, b"rs"), Some(Endpoint(2)));
        assert_eq!(resolve_endpoint(&store, b"vfs"), Some(Endpoint(9)));
    }

    #[test]
    fn test_unknown_name_endpointless() {
        // A name with no label seat reads None (C would panic,
        // `store.c:137`; the verdict is deferred to 10, D2).
        let store = labelled_store();
        assert_eq!(resolve_endpoint(&store, b"pm"), None);
    }
}
