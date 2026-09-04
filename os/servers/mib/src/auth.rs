//! Auth verdicts: one question per call, gates everywhere.
//!
//! Mirrors `mib_authed` (`main.c:259-272`) and the nine permission gates
//! scattered over `tree.c` (:115 query visibility, :500/:511 create,
//! :848/:852 destroy, :934 describe, :995/:1004 describe-settings,
//! :1228 write-alloc, :1389 dispatch descent, :1447-1456 dispatch write).
//! The question ("is this caller superuser?") is asked of PM once per
//! call and cached in the call flags; the gates only read the cache.
//! The PM round-trip itself is a transport effect (A-12); this module
//! judges answers, never asks.
//!
//! 07-mib-auth-model.md.

use minix_types::{CTLFLAG_ANYWRITE, CTLFLAG_PRIVATE, CTLFLAG_READWRITE, EPERM};

/// Superuser uid. C: `SUPER_USER` — minix/const.h:44.
pub const SUPER_USER: u32 = 0;

/// Refusal code for every denied gate in this module. C: all seven
/// `EPERM` returns across the nine gates — 07-mib-auth-model.md §2.3
/// (MIB never speaks `EACCES`).
pub const WRITE_DENIED: i32 = EPERM;

/// Cached answer to "is the caller superuser", one per call.
///
/// C: `MIB_FLAG_AUTH` / `MIB_FLAG_NOAUTH` in `call_flags` — mib.h:49-50.
/// `Unknown` means unasked; the first gate that cares resolves it via PM
/// (`getnuid(call_endpt) == SUPER_USER`, main.c:265-268) and the answer
/// sticks for the rest of the call — PM is asked at most once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallAuth {
    /// Not asked yet. C: neither flag set — main.c:263.
    #[default]
    Unknown,
    /// Asked: superuser. C: `MIB_FLAG_AUTH`.
    Yes,
    /// Asked: regular user. C: `MIB_FLAG_NOAUTH`.
    No,
}

impl CallAuth {
    /// Resolve an unasked cache from PM's answer; asked caches stick.
    ///
    /// `superuser` is "`getnuid` returned `SUPER_USER`". Re-resolving a
    /// decided cache returns it unchanged — the "ask once" is structural,
    /// not conventional.
    pub const fn resolve(self, superuser: bool) -> Self {
        match self {
            Self::Unknown => {
                if superuser {
                    Self::Yes
                } else {
                    Self::No
                }
            }
            decided => decided,
        }
    }

    /// Whether the caller may do privileged things.
    /// C: `return (call_flags & MIB_FLAG_AUTH)` — main.c:271.
    pub const fn is_authed(self) -> bool {
        matches!(self, Self::Yes)
    }
}

/// Whether the caller may see the node at all.
///
/// C: `visible = (!PRIVATE || authed)` — tree.c:115 (query), :934
/// (describe), :1389 (dispatch descent). Public nodes are visible to
/// everyone; private nodes only to superusers. Descriptions of private
/// nodes are private too (:934) — same predicate, no second rule.
pub const fn can_see(private: bool, auth: CallAuth) -> bool {
    !private || auth.is_authed()
}

/// Judge a write at dispatch (`tree.c:1447-1456`).
///
/// Only leaf/function landings with new data are judged (parents never
/// take writes). Two bars, in order: the node must be `READWRITE`
/// (`:1447` — read-only is read-only for everyone, root included), and
/// then either `ANYWRITE` or superuser (`:1455-1456` — `ANYWRITE` never
/// overrides a missing `READWRITE`, it only waives the uid check).
/// Every refusal is `EPERM`.
pub const fn check_write(node_flags: u32, has_new: bool, auth: CallAuth) -> Result<(), i32> {
    if !has_new {
        return Ok(());
    }
    if node_flags & CTLFLAG_READWRITE == 0 {
        return Err(EPERM);
    }
    if node_flags & CTLFLAG_ANYWRITE == 0 && !auth.is_authed() {
        return Err(EPERM);
    }
    Ok(())
}

/// Whether a node may accept the write this call carries.
///
/// Convenience over [`check_write`] reading the same two bars.
pub const fn node_writable_by(node_flags: u32, auth: CallAuth) -> bool {
    check_write(node_flags, true, auth).is_ok()
}

/// Judge structural mutation (create/destroy): caller must be superuser
/// and the parent must be writable.
///
/// C: `!authed → EPERM` (tree.c:500,848) then `!parent READWRITE →
/// EPERM` (:511,852). Order matters: identity before topology — a
/// regular user learns nothing about the parent's writability.
pub const fn check_mutate(auth: CallAuth, parent_flags: u32) -> Result<(), i32> {
    if !auth.is_authed() {
        return Err(EPERM);
    }
    if parent_flags & CTLFLAG_READWRITE == 0 {
        return Err(EPERM);
    }
    Ok(())
}

/// Whether the flag word marks the node private. C: `CTLFLAG_PRIVATE`.
pub const fn is_private(flags: u32) -> bool {
    flags & CTLFLAG_PRIVATE != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_asked_once() {
        // Unasked resolves from PM's answer (main.c:263-269)...
        assert_eq!(CallAuth::Unknown.resolve(true), CallAuth::Yes);
        assert_eq!(CallAuth::Unknown.resolve(false), CallAuth::No);
        // ...and decided caches stick (PM asked at most once).
        assert_eq!(CallAuth::Yes.resolve(false), CallAuth::Yes);
        assert_eq!(CallAuth::No.resolve(true), CallAuth::No);
        assert!(CallAuth::Yes.is_authed());
        assert!(!CallAuth::No.is_authed());
        assert!(!CallAuth::Unknown.is_authed());
        assert_eq!(SUPER_USER, 0);
    }

    #[test]
    fn test_visibility() {
        // Public: everyone (tree.c:115). Private: superusers only.
        assert!(can_see(false, CallAuth::No));
        assert!(can_see(false, CallAuth::Unknown));
        assert!(can_see(true, CallAuth::Yes));
        assert!(!can_see(true, CallAuth::No));
        assert!(!can_see(true, CallAuth::Unknown));
    }

    #[test]
    fn test_write_two_bars() {
        let rw = CTLFLAG_READWRITE;
        let rw_any = CTLFLAG_READWRITE | CTLFLAG_ANYWRITE;
        // No new data: no judgement (tree.c:1446).
        assert_eq!(check_write(0, false, CallAuth::No), Ok(()));
        // Bar 1: read-only refuses everyone, root included.
        assert_eq!(check_write(0, true, CallAuth::Yes), Err(EPERM));
        // Bar 2: rw without anywrite needs superuser...
        assert_eq!(check_write(rw, true, CallAuth::No), Err(EPERM));
        assert_eq!(check_write(rw, true, CallAuth::Yes), Ok(()));
        // ...anywrite waives the uid check, never the rw bar.
        assert_eq!(check_write(rw_any, true, CallAuth::No), Ok(()));
        assert_eq!(
            check_write(CTLFLAG_ANYWRITE, true, CallAuth::No),
            Err(EPERM)
        );
    }

    #[test]
    fn test_mutate_identity_first() {
        let rw = CTLFLAG_READWRITE;
        // Regular users refused before topology is consulted.
        assert_eq!(check_mutate(CallAuth::No, rw), Err(EPERM));
        assert_eq!(check_mutate(CallAuth::Unknown, rw), Err(EPERM));
        assert_eq!(check_mutate(CallAuth::Yes, 0), Err(EPERM));
        assert_eq!(check_mutate(CallAuth::Yes, rw), Ok(()));
    }
}
