//! Root version arithmetic: bump, skip zero, inherit.
//!
//! Mirrors `mib_upgrade` (`tree.c:426-449`), the two staged-version
//! rules (create `:544-546`, destroy `:889-890`), and the API-version
//! gate on staged requests (query `:194-195`, create `:537-538`). The
//! path walk itself (parent-pointer climbing) lives with the arena;
//! this module owns the numbers: what the next version is, and which
//! staged versions pass.
//!
//! 08-mib-dynamic-nodes.md (create half); 11-mib-query-describe.md
//! (query half — same rule, one implementation).

use minix_types::{SYSCTL_VERSION, sysctl_vers};

/// Next root version: +1, skipping 0.
///
/// C: `ver = root + 1; if (ver == 0) ver = 1` — tree.c:439-441. Zero
/// means "no interest in versions" everywhere (:889 pattern, 03), so it
/// must never become a real version — not even across the u32 wrap.
pub const fn next_root_ver(root_ver: u32) -> u32 {
    let ver = root_ver.wrapping_add(1);
    if ver == 0 { 1 } else { ver }
}

/// Whether a create request's staged version passes.
///
/// C: nonzero staged versions must match the parent *or* the root —
/// tree.c:544-546. The version is *not* inherited by the new node (fresh
/// children link the parent's via `linked_ver`, 03); the check only
/// proves the caller saw a recent tree.
pub const fn create_ver_ok(staged: u32, parent_ver: u32, root_ver: u32) -> bool {
    staged == 0 || staged == parent_ver || staged == root_ver
}

/// Whether a staged request speaks the tree's API version.
///
/// C: `SYSCTL_VERS(scn.sysctl_flags) != SYSCTL_VERSION → EINVAL` —
/// checked on *every* staged `sysctlnode`, in query (`:194-195`) and
/// create (`:537-538`) alike: a caller still speaking an older protocol
/// misparses every later field, so its request is refused before any of
/// them is read. One check, two C sites — one verdict here.
pub const fn staged_vers_ok(staged_flags: u32) -> bool {
    sysctl_vers(staged_flags) == SYSCTL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_root_ver_skips_zero() {
        // Normal bump (tree.c:439).
        assert_eq!(next_root_ver(1), 2);
        assert_eq!(next_root_ver(41), 42);
        // Wrap skips zero: MAX + 1 would be 0 → 1 (tree.c:440-441).
        assert_eq!(next_root_ver(u32::MAX), 1);
        assert_eq!(next_root_ver(0), 1);
    }

    #[test]
    fn test_create_ver_ok() {
        // Zero passes; parent or root pass; anything else fails (:544-546).
        assert!(create_ver_ok(0, 7, 9));
        assert!(create_ver_ok(7, 7, 9));
        assert!(create_ver_ok(9, 7, 9));
        assert!(!create_ver_ok(8, 7, 9));
    }

    #[test]
    fn test_staged_vers_ok() {
        use minix_types::CTLTYPE_INT;
        // VERS_1 in the flags word passes — the only version the tree
        // speaks (:194-195/:537-538).
        assert!(staged_vers_ok(SYSCTL_VERSION | CTLTYPE_INT));
        // Any other API version is refused before its fields are read —
        // a caller speaking a different protocol would misparse all of
        // them. VERS_2 = 0x0200_0000 (sysctl.h:130-134 scheme).
        assert!(!staged_vers_ok(0x0200_0000 | CTLTYPE_INT));
        // No version at all (VERS_0, the NetBSD legacy) is not "any".
        assert!(!staged_vers_ok(CTLTYPE_INT));
    }
}
