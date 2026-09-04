//! Root version arithmetic: bump, skip zero, inherit.
//!
//! Mirrors `mib_upgrade` (`tree.c:426-449`) and the two staged-version
//! rules (create `:544-546`, destroy `:889-890`). The path walk itself
//! (parent-pointer climbing) lives with the arena; this module owns the
//! numbers: what the next version is, and which staged versions pass.
//!
//! 08-mib-dynamic-nodes.md.

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
}
