//! Init walk: wire four subtrees, count the static tree, reset remote.
//!
//! Mirrors `mib_init` (`main.c:384-410`) + `mib_tree_recurse` /
//! `mib_tree_init` (`tree.c:1476-1536`). The subtree *contents* land in
//! 13/14/15 and the endpoint table in 12; this module owns the order,
//! the per-child verdict, and the counting fold.
//!
//! 04-mib-static-tree-init.md.

use minix_types::{CTLTYPE_NODE, SYSCTL_TYPEMASK};

use super::flag::{CTLFLAG_PARENT, NodeType};

/// One wiring step of `mib_init`: which subtree plugs into which slot.
///
/// C: `mib_init` body — main.c:395-398. Order is kern → vm → hw → minix
/// (net/user/vendor need no wiring: net fills by remote mount, user lives
/// in libc, vendor starts empty — main.c:38-44).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireStep {
    /// `mib_kern_init(&mib_table[CTL_KERN])` — main.c:395.
    Kern,
    /// `mib_vm_init(&mib_table[CTL_VM])` — main.c:396.
    Vm,
    /// `mib_hw_init(&mib_table[CTL_HW])` — main.c:397.
    Hw,
    /// `mib_minix_init(&mib_table[CTL_MINIX])` — main.c:398.
    Minix,
}

/// Wiring order. C: main.c:395-398, exactly this sequence.
pub const WIRE_ORDER: [WireStep; 4] = [WireStep::Kern, WireStep::Vm, WireStep::Hw, WireStep::Minix];

/// Phases of `mib_init`, in order.
///
/// C: main.c:384-410. Wiring (four `MIB_INIT_ENODE`), then the recursive
/// count (`mib_tree_init`), then the endpoint reset (`mib_remote_init`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitPhase {
    /// Plug the four static tables in. C: main.c:395-398.
    WireSubtrees,
    /// Walk the whole tree counting and linking. C: `:404` → `mib_tree_init`.
    InitTree,
    /// Reset the remote endpoint table. C: `:407` → `mib_remote_init` (12).
    InitRemote,
}

/// Phase order. C: main.c:384-410.
pub const PHASE_ORDER: [InitPhase; 3] = [
    InitPhase::WireSubtrees,
    InitPhase::InitTree,
    InitPhase::InitRemote,
];

/// Verdict for one static child during the walk.
///
/// C: `mib_tree_recurse` loop body — tree.c:1497-1512.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildVerdict {
    /// Vacant slot (`flags == 0`): skip, count nothing — tree.c:1498-1499.
    SkipVacant,
    /// Live child: count it, inherit the version, link the parent —
    /// tree.c:1501-1506.
    CountLink,
    /// Live node-type parent: all of the above, then recurse into it —
    /// tree.c:1509-1511.
    CountLinkRecurse,
}

/// Judge one static child.
///
/// `child_flags == 0` is vacant (zeroed rows are not in use — mib.h:131);
/// otherwise the child counts; node-type parents recurse. Mirrors the
/// loop body exactly, so the counting fold below cannot drift from it.
pub const fn judge_child(child_flags: u32) -> ChildVerdict {
    if child_flags == 0 {
        return ChildVerdict::SkipVacant;
    }
    if child_flags & SYSCTL_TYPEMASK == CTLTYPE_NODE && child_flags & CTLFLAG_PARENT != 0 {
        return ChildVerdict::CountLinkRecurse;
    }
    ChildVerdict::CountLink
}

/// Fold one static array: how many live children, and does any need
/// recursion.
///
/// Pure form of the `:1497-1512` loop minus the pointer writes (linking
/// lives with the arena in 04's follow-up; versions inherit the parent's
/// — `linked_ver`, 03). Returns `(live, needs_recurse)`.
pub const fn fold_static(children: &[u32]) -> (u32, bool) {
    let mut live = 0;
    let mut recurse = false;
    let mut i = 0;
    while i < children.len() {
        match judge_child(children[i]) {
            ChildVerdict::SkipVacant => {}
            ChildVerdict::CountLink => {
                live += 1;
            }
            ChildVerdict::CountLinkRecurse => {
                live += 1;
                recurse = true;
            }
        }
        i += 1;
    }
    (live, recurse)
}

/// Preconditions the walk asserts on entry.
///
/// C: `mib_tree_recurse` head — tree.c:1485-1486. Only node-type parents
/// are walked; anything else is a caller bug, refused here instead of
/// recursing into garbage.
pub const fn check_parent(flags: u32) -> bool {
    match NodeType::from_raw(flags) {
        Some(t) => t.is_node() && flags & CTLFLAG_PARENT != 0,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{CTLFLAG_READONLY, CTLTYPE_INT};

    #[test]
    fn test_wire_and_phase_order() {
        // C: main.c:384-410 — four wirings, three phases.
        assert_eq!(
            WIRE_ORDER,
            [WireStep::Kern, WireStep::Vm, WireStep::Hw, WireStep::Minix]
        );
        assert_eq!(
            PHASE_ORDER,
            [
                InitPhase::WireSubtrees,
                InitPhase::InitTree,
                InitPhase::InitRemote
            ]
        );
    }

    #[test]
    fn test_judge_child() {
        // Vacant rows skipped (tree.c:1498-1499).
        assert_eq!(judge_child(0), ChildVerdict::SkipVacant);
        // Leaves count, no recursion.
        assert_eq!(
            judge_child(CTLTYPE_INT | CTLFLAG_READONLY),
            ChildVerdict::CountLink
        );
        // Node-type parents count and recurse (tree.c:1509-1511).
        assert_eq!(
            judge_child(CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_READONLY),
            ChildVerdict::CountLinkRecurse
        );
        // Func-tree nodes (no PARENT) count without recursion.
        assert_eq!(
            judge_child(CTLTYPE_NODE | CTLFLAG_READONLY),
            ChildVerdict::CountLink
        );
    }

    #[test]
    fn test_fold_static() {
        // Mixed array: vacant, leaf, sub-parent, vacant.
        let children = [
            0,
            CTLTYPE_INT | CTLFLAG_READONLY,
            CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_READONLY,
            0,
        ];
        assert_eq!(fold_static(&children), (2, true));
        assert_eq!(fold_static(&[0, 0]), (0, false));
        assert_eq!(fold_static(&[CTLTYPE_INT | CTLFLAG_READONLY]), (1, false));
    }

    #[test]
    fn test_check_parent() {
        // Only node-type parents enter the walk (tree.c:1485-1486).
        assert!(check_parent(CTLTYPE_NODE | CTLFLAG_PARENT));
        assert!(!check_parent(CTLTYPE_NODE));
        assert!(!check_parent(CTLTYPE_INT | CTLFLAG_PARENT));
    }
}
