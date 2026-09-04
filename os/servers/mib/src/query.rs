//! Query verdicts: version gate, flag strip, size report, visibility.
//!
//! Mirrors the pure halves of `mib_copyout_node` / `mib_query`
//! (`tree.c:90-239`). Walking the static array and the dynamic list is a
//! walker effect (11's follow-up with the arena); judging versions,
//! stripping internal bits, and reporting sizes is done here. The
//! visibility predicate is 07's `can_see` — same rule, one implementation.
//!
//! 11-mib-query-describe.md.

use minix_types::{CTLTYPE_NODE, SYSCTL_TYPEMASK, SYSCTL_VERS_MASK, SYSCTL_VERSION, sysctl_vers};

use super::tree::flag::{CTLFLAG_PARENT, CTLFLAG_REMOTE, CTLFLAG_VERIFY};

use super::auth::CallAuth;
use super::auth::can_see;

/// Strip internal bits and stamp the version for userland.
///
/// C: `scn.sysctl_flags = SYSCTL_VERSION | (flags & ~(PARENT|VERIFY|
/// REMOTE))` — tree.c:107-108. The three reassigned bits (03) never
/// reach userland in either meaning — NetBSD's or MIB's.
pub const fn export_flags(node_flags: u32) -> u32 {
    SYSCTL_VERSION | (node_flags & !(CTLFLAG_PARENT | CTLFLAG_VERIFY | CTLFLAG_REMOTE))
}

/// Whether the version mask survived the strip (debug-grade invariant:
/// export must always carry exactly `SYSCTL_VERSION` in the top byte).
pub const fn export_version_ok(exported: u32) -> bool {
    sysctl_vers(exported) == SYSCTL_VERSION && exported & SYSCTL_VERS_MASK == SYSCTL_VERSION
}

/// Judge a query's staged version: nonzero must match parent or root.
///
/// C: `mib_query` version gate — tree.c:201-204. Same shape as create's
/// (08 `create_ver_ok`) but rooted at the *queried parent* rather than
/// the creation parent — same rule, restated where C restates it.
pub const fn query_ver_ok(staged: u32, parent_ver: u32, root_ver: u32) -> bool {
    staged == 0 || staged == parent_ver || staged == root_ver
}

/// Reported size for a serialized node.
///
/// C: `:112` reports the real `node_size`, except node-type nodes report
/// `sizeof(sysctlnode)` "the way NetBSD does, just in case" (`:135-137`).
/// `scn_size` is the caller's `size_of` its exchange node.
pub const fn report_size(is_node_type: bool, node_size: u64, scn_size: u64) -> u64 {
    if is_node_type { scn_size } else { node_size }
}

/// Whether an immediate value travels to this caller.
///
/// C: `(IMMEDIATE) && visible` — tree.c:121. Unauthorized callers get
/// the node *shape* but not the *value* (the flags say INT, the data
/// lane stays zeroed).
pub const fn expose_immediate(immediate: bool, private: bool, auth: CallAuth) -> bool {
    immediate && can_see(private, auth)
}

/// Child size window a visible caller reads off a node.
///
/// C: tree.c:157-166. Remote nodes report the cached remote-root window
/// (`rcsize/rclen` — no round-trip into the service "for reliability",
/// :140-144); real parents report the local window; function nodes
/// report nothing here (the func marker travels instead, 11 §2.1).
/// Returns `None` for function-driven nodes.
pub const fn child_window(
    is_node_type: bool,
    remote: bool,
    parent: bool,
    csize: u32,
    clen: u32,
) -> Option<(u32, u32)> {
    if !is_node_type {
        return None;
    }
    if remote || parent {
        return Some((csize, clen));
    }
    None
}

/// Whether this node-type node reports the func marker instead.
///
/// C: `:167-168` → `SYSCTL_NODE_FN` (the marker const lives in
/// `minix-types`, 02). Neither remote nor parent, but still a node.
pub const fn is_func_marker(is_node_type: bool, remote: bool, parent: bool) -> bool {
    is_node_type && !remote && !parent
}

/// Node type test helper (mirrors 10's `is_leaf_flags`, inverted).
pub const fn is_node_type(flags: u32) -> bool {
    flags & SYSCTL_TYPEMASK == CTLTYPE_NODE
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{CTLFLAG_PRIVATE, SYSCTL_NODE_FN};

    #[test]
    fn test_export_flags_strips_internal() {
        // Internal bits never reach userland (tree.c:102-108).
        let flags = CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_PRIVATE | 0x70;
        let out = export_flags(flags);
        assert_eq!(out & (CTLFLAG_PARENT | CTLFLAG_VERIFY | CTLFLAG_REMOTE), 0);
        assert!(export_version_ok(out));
        // Private bit travels (visibility is judged per call, not per bit).
        assert_ne!(out & CTLFLAG_PRIVATE, 0);
    }

    #[test]
    fn test_query_ver_ok() {
        // Zero passes; parent or root pass (:201-204).
        assert!(query_ver_ok(0, 7, 9));
        assert!(query_ver_ok(7, 7, 9));
        assert!(query_ver_ok(9, 7, 9));
        assert!(!query_ver_ok(8, 7, 9));
    }

    #[test]
    fn test_report_size() {
        // Node-types report the exchange size, NetBSD-style (:135-137).
        assert_eq!(report_size(true, 3, 96), 96);
        assert_eq!(report_size(false, 4, 96), 4);
    }

    #[test]
    fn test_expose_immediate() {
        // Immediate + visible travels; either missing hides (:121).
        assert!(expose_immediate(true, false, CallAuth::No));
        assert!(expose_immediate(true, true, CallAuth::Yes));
        assert!(!expose_immediate(true, true, CallAuth::No));
        assert!(!expose_immediate(false, false, CallAuth::Yes));
    }

    #[test]
    fn test_child_window_and_marker() {
        // Remote/parent report windows (:157-166); func nodes the marker.
        assert_eq!(child_window(true, true, false, 4, 2), Some((4, 2)));
        assert_eq!(child_window(true, false, true, 7, 5), Some((7, 5)));
        assert_eq!(child_window(true, false, false, 0, 0), None);
        assert_eq!(child_window(false, false, false, 0, 0), None);
        assert!(is_func_marker(true, false, false));
        assert!(!is_func_marker(true, true, false));
        assert_eq!(SYSCTL_NODE_FN, 0x1);
    }
}
