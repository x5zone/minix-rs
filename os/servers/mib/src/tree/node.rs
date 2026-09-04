//! Node shape: counts, child windows, remote packing, scratch, versions.
//!
//! Owns the *invariants* of `struct mib_node` / `struct mib_dynode`
//! (mib.h:188-249) without copying their C unions: the tree keeps real
//! nodes in 04/05/08, this module judges the numbers that guard them.
//! The arena itself (static tables, dynamic lists, mount storage) lands
//! with its owners; verdicts land here so they are testable today.
//!
//! 03-mib-node-model.md.

use minix_types::{CTLTYPE_NODE, SYSCTL_TYPEMASK};

use super::flag::NodeType;

/// Child window of a real parent: slots vs occupants.
///
/// C: `node_csize` / `node_clen` — mib.h:195-196. `csize` counts the
/// static array length, `clen` the live entries (static valid + dynamic);
/// the gap is empty slots (`mib_find` skips zeroed flags, tree.c:48-88).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildWindow {
    /// Static slots. C: `nvuc_csize`.
    pub csize: u32,
    /// Live children. C: `nvuc_clen`.
    pub clen: u32,
}

impl ChildWindow {
    /// Build a window; a `clen` past `csize` is a corrupt tree, not a
    /// wide one — refused, never clamped (clamping would hide a writer
    /// bug while lookups silently miss).
    pub const fn new(csize: u32, clen: u32) -> Option<Self> {
        if clen > csize {
            return None;
        }
        Some(Self { csize, clen })
    }

    /// Free static slots in the window.
    pub const fn free_slots(self) -> u32 {
        self.csize - self.clen
    }
}

/// Whether a static id addresses the array (`mib_find` fast path).
///
/// C: `IS_STATIC_ID(parent, id)` — tree.c:12. Dynamic ids live past the
/// array; the comparison is unsigned, so negative user ids convert to
/// huge values and miss (never wrap into the array).
pub const fn is_static_id(csize: u32, id: i32) -> bool {
    (id as u32) < csize
}

/// Remote packing: endpoint index + remote-root window in one word.
///
/// C: `node_eid` / `node_rcsize` / `node_rclen` bitfields — mib.h:181-182.
/// While `REMOTE` is set, the child window lanes carry this instead
/// (mib.h:150-156): reading them as a window would invent children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemotePack {
    /// Endpoint table index (5 bits → 32 services). C: `MIB_EID_BITS`.
    pub eid: u32,
    /// Remote root slots (12 bits → 4096). C: `MIB_RC_BITS`.
    pub rcsize: u32,
    /// Remote root children (12 bits). C: `MIB_RC_BITS`.
    pub rclen: u32,
}

/// Endpoint-id width. C: `MIB_EID_BITS` — mib.h:181.
pub const EID_BITS: u32 = 5;
/// Remote-window width. C: `MIB_RC_BITS` — mib.h:182.
pub const RC_BITS: u32 = 12;
/// Max remote endpoints (`1 << 5`). C: implied by `MIB_EID_BITS`.
pub const MAX_ENDPOINTS: u32 = 1 << EID_BITS;
/// Max remote-root children (`1 << 12`). C: implied by `MIB_RC_BITS`.
pub const MAX_REMOTE_CHILDREN: u32 = 1 << RC_BITS;

impl RemotePack {
    /// Pack the three lanes; out-of-range lanes refuse (a pack that
    /// silently truncates `eid` would route to the wrong service).
    pub const fn pack(eid: u32, rcsize: u32, rclen: u32) -> Option<Self> {
        if eid >= MAX_ENDPOINTS || rcsize >= MAX_REMOTE_CHILDREN || rclen >= MAX_REMOTE_CHILDREN {
            return None;
        }
        Some(Self { eid, rcsize, rclen })
    }

    /// Pack into the single `u32` the C union stores.
    pub const fn to_word(self) -> u32 {
        self.eid | (self.rcsize << EID_BITS) | (self.rclen << (EID_BITS + RC_BITS))
    }

    /// Unpack a stored word. Total: every word unpacks (masks bound it).
    pub const fn from_word(word: u32) -> Self {
        Self {
            eid: word & (MAX_ENDPOINTS - 1),
            rcsize: (word >> EID_BITS) & (MAX_REMOTE_CHILDREN - 1),
            rclen: (word >> (EID_BITS + RC_BITS)) & (MAX_REMOTE_CHILDREN - 1),
        }
    }
}

/// Tree-wide counters behind the `minix.mib.*` statistics (15).
///
/// C: `mib_nodes` / `mib_objects` / `mib_remotes` — tree.c:25-27.
/// `nodes` counts live nodes (root = 1 after init, tree.c:1524);
/// `objects` counts allocated memory objects (dynodes, descriptions,
/// temp buffers); `remotes` counts mounted subtrees (tree.c:1776).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TreeCounts {
    /// Live nodes. C: `mib_nodes`.
    pub nodes: u32,
    /// Allocated objects. C: `mib_objects`.
    pub objects: u32,
    /// Mounted remote subtrees. C: `mib_remotes`.
    pub remotes: u32,
}

impl TreeCounts {
    /// Post-init baseline: the root alone, nothing allocated.
    /// C: `mib_nodes = 1; mib_objects = 0` — tree.c:1524-1525.
    pub const fn baseline() -> Self {
        Self {
            nodes: 1,
            objects: 0,
            remotes: 0,
        }
    }

    /// A node was created (`mib_add`, tree.c:474; `mib_tree_recurse`, :1501).
    pub const fn node_added(self) -> Self {
        Self {
            nodes: self.nodes + 1,
            ..self
        }
    }

    /// A node was destroyed (`mib_remove`, tree.c:829).
    pub const fn node_removed(self) -> Self {
        Self {
            nodes: self.nodes - 1,
            ..self
        }
    }

    /// An object was allocated (dynode/desc/temp buffer).
    pub const fn object_added(self) -> Self {
        Self {
            objects: self.objects + 1,
            ..self
        }
    }

    /// An object was freed.
    pub const fn object_removed(self) -> Self {
        Self {
            objects: self.objects - 1,
            ..self
        }
    }

    /// A remote subtree mounted (tree.c:1776).
    pub const fn remote_added(self) -> Self {
        Self {
            remotes: self.remotes + 1,
            ..self
        }
    }

    /// A remote subtree unmounted.
    pub const fn remote_removed(self) -> Self {
        Self {
            remotes: self.remotes - 1,
            ..self
        }
    }
}

/// Scratch buffer budget: one page, int32-aligned.
///
/// C: `SCRATCH_SIZE = MAX(PAGE_SIZE, sizeof(struct sysctldesc) + 1024)`
/// with `static char scratch[SCRATCH_SIZE] __aligned(sizeof(int32_t))` —
/// tree.c:21-23. `sizeof(sysctldesc)` is 16, so the max is `PAGE_SIZE`
/// (4096 on all three minix-rs archs): the scratch *is* a page. Uses:
/// string-length probes (tree.c:402), describe staging (:945), write
/// staging (:1242), mount name/desc fetch (:1702).
pub const SCRATCH_SIZE: usize = 4096;
/// Scratch alignment. C: `__aligned(sizeof(int32_t))` — tree.c:23.
pub const SCRATCH_ALIGN: usize = 4;
/// Longest description the scratch stages. C: `MAXDESCLEN 1024` — tree.c:21.
pub const MAX_DESC_LEN: usize = 1024;

/// Root version after init. C: `mib_root.node_ver = 1` — tree.c:1531.
pub const ROOT_VER: u32 = 1;

/// Child version at link time: a fresh child matches its parent.
/// C: `node->node_ver = parent->node_ver` — tree.c:1505.
pub const fn linked_ver(parent_ver: u32) -> u32 {
    parent_ver
}

/// Whether a staged version still matches the node (query/describe
/// consistency, tree.c:203,545,889,1030).
pub const fn version_matches(staged: u32, current: u32) -> bool {
    staged == 0 || staged == current
}

/// Whether a flag word may carry children at all (gates `ChildWindow` use).
pub const fn can_have_children(flags: u32) -> bool {
    flags & SYSCTL_TYPEMASK == CTLTYPE_NODE && NodeType::from_raw(flags).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_child_window_bounds() {
        // csize slots, clen live (mib.h:195-196).
        let w = ChildWindow::new(7, 5).unwrap();
        assert_eq!(w.free_slots(), 2);
        // Corrupt inputs refuse, never clamp.
        assert_eq!(ChildWindow::new(5, 7), None);
        assert_eq!(ChildWindow::new(0, 0).unwrap().free_slots(), 0);
    }

    #[test]
    fn test_static_id_unsigned() {
        // C: IS_STATIC_ID (tree.c:12) — unsigned compare.
        assert!(is_static_id(7, 0));
        assert!(is_static_id(7, 6));
        assert!(!is_static_id(7, 7));
        // Negative ids convert huge and miss (never wrap into the array).
        assert!(!is_static_id(7, -1));
        assert!(!is_static_id(7, -2));
    }

    #[test]
    fn test_remote_pack_roundtrip() {
        // 5 + 12 + 12 bits (mib.h:181-182).
        assert_eq!(MAX_ENDPOINTS, 32);
        assert_eq!(MAX_REMOTE_CHILDREN, 4096);
        let p = RemotePack::pack(31, 4095, 7).unwrap();
        assert_eq!(RemotePack::from_word(p.to_word()), p);
        // Out-of-range lanes refuse (no silent truncation to eid 0).
        assert_eq!(RemotePack::pack(32, 0, 0), None);
        assert_eq!(RemotePack::pack(0, 4096, 0), None);
        assert_eq!(RemotePack::pack(0, 0, 4096), None);
        // Bit positions: eid low, then rcsize, then rclen.
        let q = RemotePack::pack(1, 2, 3).unwrap();
        assert_eq!(q.to_word(), 1 | (2 << 5) | (3 << 17));
    }

    #[test]
    fn test_counts_lifecycle() {
        // Baseline: root alone (tree.c:1524-1525).
        let c = TreeCounts::baseline();
        assert_eq!((c.nodes, c.objects, c.remotes), (1, 0, 0));
        let c = c.node_added().object_added().remote_added();
        assert_eq!((c.nodes, c.objects, c.remotes), (2, 1, 1));
        let c = c.node_removed().object_removed().remote_removed();
        assert_eq!(c, TreeCounts::baseline());
    }

    #[test]
    fn test_scratch_budget() {
        // max(4096, 16 + 1024) = 4096: the scratch is a page.
        assert_eq!(SCRATCH_SIZE, 4096);
        assert_eq!(SCRATCH_ALIGN, 4);
        assert_eq!(MAX_DESC_LEN, 1024);
        assert!(SCRATCH_SIZE >= MAX_DESC_LEN + 16);
    }

    #[test]
    fn test_versions() {
        // Root starts at 1 (tree.c:1531); children link parent's.
        assert_eq!(ROOT_VER, 1);
        assert_eq!(linked_ver(7), 7);
        // Staged 0 means "no check" (tree.c:889 pattern).
        assert!(version_matches(0, 99));
        assert!(version_matches(5, 5));
        assert!(!version_matches(5, 6));
    }
}
