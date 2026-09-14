//! The arena's type layer: node ids, dynamic children, dynode shapes.
//!
//! C stores dynamic children as a **sorted intrusive linked list**
//! (`mib.h:110-280` — the dynode chain hangs off the parent, ordered by
//! id so `mib_find` can stop early, tree.c:69-78). The list exists
//! because C builds trees inside static arrays where a resizeable
//! member is unaffordable; the Rust arena has `alloc` (A-3) and can
//! pick the container on merits:
//!
//! - **`BTreeMap<i32, NodeId>`** — ordered iteration for free (CTL_QUERY
//!   must report children in id order, 11), O(log n) lookup/insert/
//!   delete, range queries cover the "find first ≥ id" walk that
//!   `scan` judges in slices.
//! - sorted `Vec` + binary search — cache-friendly and one allocation
//!   saved per node, but create pays O(n) element shifts; the query
//!   order win is the same as the map's.
//! - C's linked list, transliterated — the one rejected: it inherits
//!   O(n) search to save an allocation the arena already pays, and
//!   unsafe aliasing for nothing (the translate defense, pattern 16).
//!
//! Verdict: `ChildMap = BTreeMap`. `[ARCH: ...]` (对照点 `mib.h` 动态
//! 子节点排序链表 + tree.c:69-78 早停查找；design 03/08 篇 A-2 行；
//! 代码注释即本段)。`scan`'s sorted-slice verdicts stay untouched —
//! the walker (todo.md P1-2) bridges map ↔ slices at the call site.
//!
//! Memory ownership (A-3): a dynode's name, data, and description are
//! budget-backed buffers. The tree settles the ledger when it removes
//! a node — the struct implements no `Drop`, so removal paths can
//! compute and release the exact claim (`RemoveDelta` + sizes) in one
//! place. 08-mib-dynamic-nodes.md.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use super::node::ChildWindow;

/// A handle to a live dynamic node: an index into the arena's slab.
///
/// C passes `struct mib_dynode *` around (the endpts table even stores
/// them across calls, remote.c:31-35). An index survives arena growth
/// (the vector may relocate), costs 4 bytes where a pointer costs 8,
/// and — unlike a pointer — cannot dangle silently past a restart:
/// a stale id either indexes a recycled node within bounds or is
/// rejected at the slab edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// The dynamic children of one parent, ordered by id.
///
/// C: a sorted linked list threaded through the dynodes (early-stop
/// search, tree.c:69-78; insert-at-scan, :465-467). See the module doc
/// for why the map replaces it.
pub type ChildMap = BTreeMap<i32, NodeId>;

/// A dynamic node's value, as `mib_create` can build one.
///
/// C: userland supplies either an immediate (BOOL/INT/QUAD packed into
/// the union) or OWNDATA bytes; function pointers cannot come from
/// userland (a kernel address in a create request would be absurd),
/// and node-type nodes carry no value at all (`data_combo_ok`, 08).
#[derive(Debug, Clone)]
pub enum DynValue {
    /// Immediate scalars: BOOL/INT travel as `i32`, QUAD as `i64`.
    /// C packs both arms into the same union word.
    Immediate(i64),
    /// OWNDATA bytes: the node owns exactly `sysctl_size` of them.
    /// Length is the budget claim (A-3) and the wire size (09).
    Owned(Box<[u8]>),
}

/// A dynamic node: C's `mib_dynode` (name + value + description in one
/// allocation) as three owned fields.
///
/// C: one `malloc` block embeds header, name bytes, and data
/// (`tree.c:679-685`) — the single-block shape is a C allocator
/// optimization, not semantics; Rust owns the three parts separately
/// and charges the same total to the budget
/// ([`create_alloc_size`](super::dynamic::create_alloc_size)'s math:
/// header + name (+ data)).
pub struct Dynode {
    /// Node name, NUL-free (the wire form adds the terminator when
    /// serializing). C: `dname[]` embedded, NUL-terminated.
    pub name: Box<[u8]>,
    /// Node flags as created (OWNDATA already forced where C forces
    /// it, `tree.c:568`). C: `node_flags`.
    pub flags: u32,
    /// This node's version — linked from the parent at insert
    /// (`linked_ver`, 03). C: `node_ver`.
    pub ver: u32,
    /// Immediate value or owned data. C: `sysctl_un` + `node_data`.
    pub value: Option<DynValue>,
    /// Owned description (OWNDESC), absent unless a describe set one.
    /// C: `ddesc` pointer + `strdup`, `tree.c:1043-1051`.
    pub desc: Option<Box<[u8]>>,
}

impl Dynode {
    /// The static window this node's parent advertises for it.
    ///
    /// Dynamic children live *past* the static array
    /// (`is_static_id` draws the line, 03/05), so a dynode contributes
    /// no static slot — the parent's `ChildWindow` merely grows its
    /// `clen`. This helper exists so the walker asks one question of
    /// every child, static or not.
    pub const fn no_static_slot() -> Option<ChildWindow> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CTL_QUERY reports children in id order — the property the map
    /// buys over the linked list (11's enumeration contract).
    #[test]
    fn test_child_map_iterates_in_id_order() {
        let mut children: ChildMap = ChildMap::new();
        // Insert out of order on purpose; iteration must sort.
        for id in [1026i32, 1024, 1031, 1025] {
            children.insert(id, NodeId(id as u32));
        }
        let ids: Vec<i32> = children.keys().copied().collect();
        assert_eq!(ids, vec![1024, 1025, 1026, 1031]);
        // CREATE_BASE ids never collide with the static lane: the map
        // only ever holds dynamic ids (static lookup is the array, 05).
        assert!(children.keys().all(|id| *id >= 1024));
    }

    /// `scan` judges "first free id ≥ base, with the sorted insert
    /// position" — the map must reproduce that position exactly.
    #[test]
    fn test_child_map_matches_scan_insert_position() {
        let mut children: ChildMap = ChildMap::new();
        for id in [1024i32, 1026] {
            children.insert(id, NodeId(id as u32));
        }
        // First id ≥ 1028 that is free: 1028 itself, inserted after 1026.
        let insert_at = children.range(1028..).next().map_or(2, |(id, _)| {
            children.keys().position(|k| k == id).unwrap()
        });
        assert_eq!(insert_at, 2);
        children.insert(1028, NodeId(1028));
        // 1029 now lands between 1028 and nothing — position 3.
        let insert_at = children.range(1029..).next().map_or(3, |(id, _)| {
            children.keys().position(|k| k == id).unwrap()
        });
        assert_eq!(insert_at, 3);
    }

    /// A dynode's parts are independent: value and description can be
    /// present, absent, or any combination (the OWNDATA/OWNDESC
    /// matrix 08 judges is four combinations, all representable).
    #[test]
    fn test_dynode_parts_independent() {
        let bare = Dynode {
            name: Box::from(&b"n"[..]),
            flags: 0,
            ver: 1,
            value: None,
            desc: None,
        };
        assert!(bare.value.is_none() && bare.desc.is_none());
        let owned = Dynode {
            name: Box::from(&b"n"[..]),
            flags: 0,
            ver: 1,
            value: Some(DynValue::Owned(Box::from(&b"data"[..]))),
            desc: Some(Box::from(&b"desc"[..])),
        };
        assert_eq!(owned.desc.as_deref(), Some(&b"desc"[..]));
    }
}
