//! DS authorization: whose name may touch an entry, and when.
//!
//! Mirrors `check_auth` (`minix3/minix/servers/ds/store.c:143-153`).
//! 05-ds-identity-auth.md.
//!
//! The module owns the verdict and nothing else: given an entry, an
//! already-resolved caller name, and one permission gate, answer allow
//! or deny. Translation (endpoint ↔ name) lives next door
//! (`identity.rs`, D3): judging takes the name as tendered, so it needs
//! no table and no IPC — pure judgement, purely testable.
//!
//! Single-threaded event loop: pure function, no shared state.

use minix_types::DsFlags;

use crate::slots::key_eq;
use crate::store::DataEntry;

/// Judge access (`check_auth`, `store.c:143-153`).
///
/// Two rules, in C order. An unset gate allows (`store.c:148-149`):
/// protection is selective — a publisher opts into "owner only" per
/// gate (`DSF_PRIV_*`, 02), and entries without the gate stay open to
/// every named caller. A set gate compares (`store.c:151-152`): the
/// entry owner's name must equal the caller's, through the same NUL
/// stop rule as every other name verdict (`key_eq`, 04 D4).
///
/// A nameless caller (`None` — an endpoint with no published label)
/// closes a set gate: with no name there is nothing to compare, and
/// the safe default for a guarded entry is deny (D4). C reaches the
/// same verdict by short-circuit (`source && ...`, `store.c:152`).
pub fn check_auth(entry: &DataEntry, caller: Option<&[u8]>, perm: DsFlags) -> bool {
    if !entry.flags.intersects(perm) {
        return true;
    }
    caller.map(|name| key_eq(&entry.owner, name)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DataBody;
    use minix_types::DS_MAX_KEYLEN;

    fn guarded(owner: &[u8], perm: DsFlags) -> DataEntry {
        let mut entry = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_U32 | perm,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        };
        entry.owner[..owner.len()].copy_from_slice(owner);
        entry
    }

    #[test]
    fn test_open_gate_allows_stranger() {
        // No gate set: even a nameless stranger passes (`store.c:148-149`
        // — protection is selective, not default).
        let entry = guarded(b"rs", DsFlags::empty());
        let plain = DataEntry {
            flags: DsFlags::IN_USE | DsFlags::TYPE_U32,
            ..entry
        };
        assert!(check_auth(&plain, None, DsFlags::PRIV_RETRIEVE));
        assert!(check_auth(&plain, Some(b"intruder"), DsFlags::PRIV_OVERWRITE));
    }

    #[test]
    fn test_owner_passes_set_gate() {
        // The owner's own name opens its guarded gates.
        let entry = guarded(b"vfs", DsFlags::PRIV_RETRIEVE | DsFlags::PRIV_OVERWRITE);
        assert!(check_auth(&entry, Some(b"vfs"), DsFlags::PRIV_RETRIEVE));
        assert!(check_auth(&entry, Some(b"vfs"), DsFlags::PRIV_OVERWRITE));
    }

    #[test]
    fn test_stranger_refuses_set_gate() {
        // Any other name closes a set gate (`store.c:151-152`).
        let entry = guarded(b"vfs", DsFlags::PRIV_RETRIEVE);
        assert!(!check_auth(&entry, Some(b"pm"), DsFlags::PRIV_RETRIEVE));
        // ...while gates not set on the entry stay open beside it.
        assert!(check_auth(&entry, Some(b"pm"), DsFlags::PRIV_OVERWRITE));
    }

    #[test]
    fn test_nameless_caller_closed() {
        // No name, set gate: deny — C's `source && ...` short-circuit
        // (`store.c:152`) in type form.
        let entry = guarded(b"vfs", DsFlags::PRIV_SUBSCRIBE);
        assert!(!check_auth(&entry, None, DsFlags::PRIV_SUBSCRIBE));
    }
}
